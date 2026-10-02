//! Market data fetching

use crate::config::AnalysisMode;
use anyhow::{anyhow, bail, Result};
use chrono::{NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;
use tokio::time::sleep;

/// OHLCV for one bar, plus the date, timestamp, timezone and instrument name it
/// belongs to.
///
/// A confirmed bar is never rewritten. The one exception is a Japanese
/// **intraday** timeframe, where the bar the live quote falls in has its close
/// updated — and its high/low widened only if the trade sits outside them — by
/// `apply_realtime_intraday_observation`.
///
/// 1 本足の OHLCV と、それが属する日付・タイムスタンプ・タイムゾーン・銘柄名。
///
/// 確定した足は書き換えない。例外は日本株の**分足**で、リアルタイム気配が属する足だけ、
/// 終値を更新し、約定が範囲外のときに限り高値・安値を広げる
/// （`apply_realtime_intraday_observation`）。
#[derive(Clone, Debug, Deserialize)]
pub struct MarketData {
    pub date: String,
    #[serde(default)]
    pub datetime: Option<String>,
    #[serde(default)]
    pub timestamp: Option<i64>,
    #[serde(default)]
    pub timezone: Option<String>,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: Option<f64>,
    #[serde(default)]
    pub name: Option<String>,
}

/// The latest price the provider reported, and nothing else.
///
/// The time and timezone that price was observed at live in
/// [`MarketDataSnapshot`], which holds them whether or not a price came back —
/// so a missing price never takes the timestamp down with it.
///
/// プロバイダが報告した最新価格だけを持つ。
///
/// その価格を観測した時刻とタイムゾーンは [`MarketDataSnapshot`] 側にあり、価格が
/// 返らなかった場合でも保持される。価格の欠落が時刻まで道連れにしないため。
#[derive(Clone, Debug)]
pub struct MarketLatestObservation {
    pub price: f64,
}

/// One retrieval, kept together: the bars, the latest observed price, and the
/// time and timezone that retrieval belongs to.
///
/// Time and timezone are held regardless of whether a price came back, so the
/// display can always say *when* the data is from. A snapshot is never spliced
/// across providers — see [`PriceFetcherKind`] — because mixing two vendors'
/// numbers inside one result would break the single source of truth.
///
/// 1 回の取得をひとまとめにしたもの。足・最新取得価格・その取得が属する時刻と
/// タイムゾーン。
///
/// 時刻とタイムゾーンは価格の有無に関わらず保持する。表示が「いつのデータか」を常に
/// 言えるようにするため。スナップショットをプロバイダ間で継ぎ接ぎすることはない
/// （[`PriceFetcherKind`] 参照）。1 つの結果に 2 社の数値が混ざると、単一の情報源で
/// あることが崩れるため。
#[derive(Clone, Debug)]
pub struct MarketDataSnapshot {
    pub bars: Vec<MarketData>,
    pub latest_observation: Option<MarketLatestObservation>,
    pub market_data_latest_time: Option<String>,
    pub market_data_latest_timestamp: Option<i64>,
    pub timezone: String,
    /// ISO 4217 currency the prices are quoted in (Yahoo `meta.currency`, e.g.
    /// "JPY", "USD"). `None` when the provider didn't report one — never guessed.
    pub currency: Option<String>,
    pub analyzed_at: String,
    pub analyzed_at_timestamp: i64,
    /// Honest provenance note. `None` for a primary (Yahoo) fetch. `Some(..)`
    /// when the snapshot was adopted whole from a fallback vendor (Stooq) after
    /// the primary failed — the numbers come from a different source and may
    /// differ. Surfaced to the user as one clearly-marked warning line. The
    /// snapshot is NEVER a splice of two providers; this only labels which
    /// single source it came from.
    pub source_note: Option<String>,
}

fn normalize_bar_timestamp(ts: i64, analysis_mode: AnalysisMode, tz: Tz) -> i64 {
    if let Some(minutes) = analysis_mode.intraday_minutes() {
        tz.timestamp_opt(ts, 0).single().map_or(ts, |dt| {
            let minute_remainder = i64::from(dt.minute() % minutes);
            ts - (minute_remainder * 60) - i64::from(dt.second())
        })
    } else {
        ts
    }
}

/// Cap and clean untrusted provider text before it reaches a message or a log.
///
/// A provider can answer with anything — an HTML challenge page, a multi-kilobyte
/// blob, terminal escape sequences — and none of that belongs in the line the user
/// reads. Control characters become spaces (a response must not be able to rewrite
/// the line it is printed on), runs of whitespace collapse, and the result is
/// capped. Quoting a body whole is how a `<!DOCTYPE html>…` page ended up in the
/// error for a mistyped ticker.
///
/// プロバイダ由来の非信頼テキストを、メッセージやログに載る前に短く整える。
///
/// プロバイダは何でも返し得る——HTML のチャレンジページ、数 KB の塊、端末エスケープ——
/// そのいずれも利用者が読む 1 行に載るべきものではない。制御文字は空白に置き換え
/// （応答が、自分が印字される行を書き換えられてはならない）、連続する空白を畳み、
/// 長さを切る。本文をそのまま引用していたために、打ち間違いのティッカーに対して
/// `<!DOCTYPE html>…` がエラーに出ていた。
fn bounded_provider_text(raw: &str) -> String {
    const MAX_CHARS: usize = 160;
    let cleaned = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    match cleaned.char_indices().nth(MAX_CHARS) {
        Some((cut, _)) => format!("{}…", &cleaned[..cut]),
        None => cleaned,
    }
}

/// The reason Yahoo itself gives, out of the JSON body it sends *with* a non-2xx
/// status: `{"chart":{"error":{"code":"Not Found","description":"No data found,
/// symbol may be delisted"}}}`. `None` when the body is not that shape.
///
/// Reading only the status threw this away, so a mistyped ticker was reported as
/// "Market data API request failed" — true, but not the thing the user needed.
///
/// Yahoo が非 2xx ステータスと**一緒に**返す JSON 本文から、Yahoo 自身が述べる理由を
/// 取り出す。その形でない本文なら `None`。
///
/// ステータスだけを読んでいたためにこれが失われ、打ち間違いのティッカーが
/// 「Market data API request failed」と報告されていた——嘘ではないが、利用者が
/// 必要としている情報ではない。
fn yahoo_chart_error_reason(body: &str) -> Option<String> {
    let json: Value = serde_json::from_str(body).ok()?;
    let err = json.get("chart")?.get("error")?;
    if err.is_null() {
        return None;
    }
    let code = err.get("code").and_then(Value::as_str).unwrap_or_default();
    let description = err
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let joined = match (code.is_empty(), description.is_empty()) {
        (false, false) => format!("{code} — {description}"),
        (false, true) => code.to_string(),
        (true, false) => description.to_string(),
        (true, true) => return None,
    };
    Some(bounded_provider_text(&joined))
}

fn should_retry_market_data_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn should_retry_market_data_error(err: &reqwest::Error) -> bool {
    err.is_timeout()
        || err.is_connect()
        || err.status().is_some_and(should_retry_market_data_status)
}

/// The primary provider. Fetches from `query2.finance.yahoo.com`, and when that
/// host's retries are exhausted retries the alternate `query1.finance.yahoo.com`.
///
/// The two hosts serve the same data, so the alternate is resilience, not a
/// second opinion — nothing about the result changes depending on which answered.
///
/// 主プロバイダ。`query2.finance.yahoo.com` から取得し、そのホストの再試行を使い切ると
/// 代替の `query1.finance.yahoo.com` を試す。
///
/// 2 つのホストは同じデータを返すので、代替は堅牢化であって第 2 の意見ではない。
/// どちらが応答したかで結果が変わることはない。
pub struct YahooFinancePriceFetcher {
    pub proxy_url: Option<String>,
    pub no_proxy: Option<String>,
}

impl crate::traits::PriceFetcher for YahooFinancePriceFetcher {
    async fn fetch_snapshot(&self, ticker: &str, mode: AnalysisMode) -> Result<MarketDataSnapshot> {
        fetch_market_data_snapshot_for_mode(
            ticker,
            mode,
            self.proxy_url.as_deref(),
            self.no_proxy.as_deref(),
            None,
        )
        .await
    }
}

/// Fallback vendor: Stooq. A genuine second data source (NOT Yahoo mirrored),
/// so its numbers may differ from Yahoo's — any snapshot it produces is marked
/// `source_note` so the user is told. Free feed is daily-oriented: it can serve
/// daily/weekly/monthly, and returns a clear `Err` for intraday timeframes
/// rather than fabricating or substituting daily bars.
pub struct StooqPriceFetcher {
    pub proxy_url: Option<String>,
    pub no_proxy: Option<String>,
}

/// The honest label attached to any snapshot adopted from the fallback vendor.
const STOOQ_FALLBACK_NOTE: &str = "Yahoo unavailable — figures from Stooq (fallback; may differ)";

/// Map a ticker to Stooq's symbol form.
/// - JP code (`7203.T` or bare `7203`) → `7203.jp`
/// - everything else (US etc.) → lowercased `<ticker>.us` (e.g. `AAPL` → `aapl.us`)
fn stooq_symbol(ticker: &str) -> String {
    if let Some(code) = crate::bootstrap::jp_code_from_ticker(ticker) {
        format!("{}.jp", code)
    } else {
        format!("{}.us", ticker.trim().to_ascii_lowercase())
    }
}

/// Interval Stooq's free daily CSV endpoint can genuinely serve. Only the
/// timeframes below are honoured; anything else must Err (no daily-for-intraday
/// substitution). Weekly/monthly are intentionally excluded here to avoid
/// client-side aggregation we cannot guarantee matches — daily-only is the
/// correct, smaller scope.
fn stooq_supports(mode: AnalysisMode) -> bool {
    matches!(mode, AnalysisMode::Daily)
}

/// Parse Stooq daily CSV (`Date,Open,High,Low,Close,Volume`) into bars matching
/// the shape Yahoo produces. Stooq dates are plain `YYYY-MM-DD` in exchange
/// local time with no intraday component, so `datetime`/`timestamp`/`timezone`
/// are left `None` (daily bars carry no wall-clock time) — same as a daily
/// Yahoo bar would where a time is absent.
fn parse_stooq_csv(csv: &str) -> Result<Vec<MarketData>> {
    let mut out: Vec<MarketData> = Vec::new();
    let mut lines = csv.lines();
    let header = lines
        .next()
        .ok_or_else(|| anyhow!("❌ Stooq CSV empty (no header)."))?;
    // A provider can answer with a page instead of data: Stooq's anti-bot check
    // serves an HTML/JS challenge on some networks. Name what happened instead of
    // quoting the page, which used to put a whole `<!DOCTYPE html>…` line in the
    // user's terminal under the heading "unexpected header".
    if header.trim_start().starts_with('<') {
        bail!("❌ Stooq answered with a web page instead of CSV data (its anti-bot check, most likely); the fallback provider is unavailable on this network.");
    }
    if !header.to_ascii_lowercase().starts_with("date,") {
        bail!(
            "❌ Stooq CSV header not recognised: {}",
            bounded_provider_text(header)
        );
    }
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split(',').collect();
        // Date,Open,High,Low,Close,Volume — need at least through Close.
        if cols.len() < 5 {
            continue;
        }
        let date = cols[0].trim().to_string();
        let parse = |s: &str| s.trim().parse::<f64>().ok().filter(|v| v.is_finite());
        let (high, low, close) = match (parse(cols[2]), parse(cols[3]), parse(cols[4])) {
            (Some(h), Some(l), Some(c)) => (h, l, c),
            // Stooq uses "N/D" for missing values on non-trading rows — skip.
            _ => continue,
        };
        let volume = cols.get(5).and_then(|v| parse(v)).filter(|v| *v >= 0.0);
        out.push(MarketData {
            date,
            datetime: None,
            timestamp: None,
            timezone: None,
            high,
            low,
            close,
            volume,
            name: None,
        });
    }
    Ok(out)
}

/// Fetch a whole snapshot from Stooq for the daily timeframe. Errs (never
/// substitutes) for intraday/weekly/monthly.
async fn fetch_stooq_snapshot(
    ticker: &str,
    mode: AnalysisMode,
    proxy_url: Option<&str>,
    no_proxy: Option<&str>,
) -> Result<MarketDataSnapshot> {
    if !stooq_supports(mode) {
        bail!(
            "❌ Stooq fallback cannot serve the {} timeframe (daily-only feed).",
            mode.data_interval()
        );
    }

    let symbol = stooq_symbol(ticker);
    let url = format!(
        "https://stooq.com/q/d/l/?s={}&i=d",
        urlencoding::encode(&symbol)
    );

    let mut builder = Client::builder()
        .user_agent("Mozilla/5.0 (xoksa)")
        .gzip(true)
        .brotli(true)
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(3));
    if let Some(url) = proxy_url {
        builder = builder.proxy(crate::utils::build_proxy(url, no_proxy)?);
    }
    let client = builder.build()?;

    let response = client.get(&url).send().await?.error_for_status()?;
    let text = response.text().await?;

    let mut bars = parse_stooq_csv(&text)?;
    // Stooq appends no post-close empty bucket, but keep the same hygiene as
    // Yahoo so both providers yield equivalently-clean series.
    trim_trailing_empty_bars(&mut bars);
    if bars.len() < 2 {
        bail!("❌ Stooq returned fewer than 2 bars; cannot build technical indicators.");
    }

    let latest = bars.last().expect("bars non-empty (len checked >= 2)");
    let market_data_latest_time = Some(latest.date.clone());
    let latest_observation = Some(MarketLatestObservation {
        price: latest.close,
    });

    let analyzed_at = Utc::now();
    Ok(MarketDataSnapshot {
        bars,
        latest_observation,
        market_data_latest_time,
        market_data_latest_timestamp: None,
        // Stooq CSV carries no timezone; daily bars are date-only. Use UTC as the
        // neutral label (mirrors Yahoo's UTC fallback when tz is absent).
        timezone: "UTC".to_string(),
        // Stooq does not report a currency in the CSV — never guess from the
        // ticker suffix; leave None so downstream shows a neutral label.
        currency: None,
        analyzed_at: analyzed_at.format("%Y-%m-%d %H:%M").to_string(),
        analyzed_at_timestamp: analyzed_at.timestamp(),
        source_note: Some(STOOQ_FALLBACK_NOTE.to_string()),
    })
}

impl crate::traits::PriceFetcher for StooqPriceFetcher {
    async fn fetch_snapshot(&self, ticker: &str, mode: AnalysisMode) -> Result<MarketDataSnapshot> {
        fetch_stooq_snapshot(
            ticker,
            mode,
            self.proxy_url.as_deref(),
            self.no_proxy.as_deref(),
        )
        .await
    }
}

/// Runtime-selectable market data provider.
///
/// To add a provider (e.g. a brokerage API), add a variant here, implement
/// `PriceFetcher` for the new fetcher, and add one arm to the `match` below and
/// to `build_price_fetcher`. Call sites do not change. See design-philosophy.md
/// §8.1.
pub enum PriceFetcherKind {
    Yahoo(YahooFinancePriceFetcher),
    Stooq(StooqPriceFetcher),
    /// Try `primary`; only if it fully fails, adopt `secondary`'s WHOLE snapshot.
    /// Never a splice — a snapshot always comes from exactly one provider.
    Failover {
        primary: Box<PriceFetcherKind>,
        secondary: Box<PriceFetcherKind>,
    },
}

impl crate::traits::PriceFetcher for PriceFetcherKind {
    async fn fetch_snapshot(&self, ticker: &str, mode: AnalysisMode) -> Result<MarketDataSnapshot> {
        match self {
            Self::Yahoo(fetcher) => fetcher.fetch_snapshot(ticker, mode).await,
            Self::Stooq(fetcher) => fetcher.fetch_snapshot(ticker, mode).await,
            Self::Failover { primary, secondary } => {
                // Recursive async over the enum requires boxed indirection.
                match Box::pin(primary.fetch_snapshot(ticker, mode)).await {
                    // A successful primary always wins — return it untouched.
                    Ok(snapshot) => Ok(snapshot),
                    // Primary genuinely failed: adopt the secondary's whole
                    // snapshot. It already carries its own `source_note`.
                    Err(primary_err) => {
                        match Box::pin(secondary.fetch_snapshot(ticker, mode)).await {
                            Ok(snapshot) => Ok(snapshot),
                            // Both failed, so the primary's reason leads: an unknown
                            // ticker is the primary's verdict, not the fallback's.
                            // Discarding it reported the fallback's CSV complaint for
                            // a mistyped ticker, which named neither the ticker nor
                            // the real cause.
                            Err(secondary_err) => Err(anyhow!(
                                "{primary_err} (the fallback provider did not answer either: {secondary_err})"
                            )),
                        }
                    }
                }
            }
        }
    }
}

/// Build the configured market data provider. Single extension point for
/// provider selection. Default is Yahoo with a Stooq fallback: a successful
/// Yahoo fetch always wins; Stooq is used only when Yahoo genuinely fails, and
/// any Stooq snapshot is marked degraded (`source_note`).
pub fn build_price_fetcher(config: &crate::config::Config) -> PriceFetcherKind {
    let yahoo = PriceFetcherKind::Yahoo(YahooFinancePriceFetcher {
        proxy_url: config.https_proxy.clone(),
        no_proxy: config.no_proxy.clone(),
    });
    let stooq = PriceFetcherKind::Stooq(StooqPriceFetcher {
        proxy_url: config.https_proxy.clone(),
        no_proxy: config.no_proxy.clone(),
    });
    PriceFetcherKind::Failover {
        primary: Box::new(yahoo),
        secondary: Box::new(stooq),
    }
}

/// Merge adjacent observations which normalize to the same intraday bucket.
///
/// Yahoo may append its latest trade as a zero-volume point timestamped at the
/// observation time (for example 09:41), after the accumulated 09:30 15-minute
/// bar. Timestamp normalization maps both to 09:30. Keep the accumulated volume
/// and OHLC range, but advance the bucket's close to the genuinely latest price.
fn merge_duplicate_trailing_bucket(bars: &mut Vec<MarketData>) {
    while bars.len() >= 2 {
        let last = bars.len() - 1;
        if bars[last - 1].timestamp != bars[last].timestamp {
            break;
        }

        let latest = bars.pop().expect("length checked above");
        let bucket = bars.last_mut().expect("length checked above");
        bucket.high = bucket.high.max(latest.high);
        bucket.low = bucket.low.min(latest.low);
        bucket.close = latest.close;
        if latest.volume.is_some_and(|v| v > 0.0) {
            bucket.volume = latest.volume;
        }
    }
}

/// Apply a real-time trade to the current intraday bucket. Yahoo! JAPAN exposes
/// the live price but not a matching public OHLCV payload, so this updates only
/// what the observation proves: close, and the high/low bounds if the trade lies
/// outside them. A new forming bucket starts with H=L=C=price and no invented
/// volume; volume rendering already falls back to the latest positive observation.
fn apply_realtime_intraday_observation(
    bars: &mut Vec<MarketData>,
    price: f64,
    observation_ts: i64,
    analysis_mode: AnalysisMode,
    tz: Tz,
    tz_name: &str,
) {
    if analysis_mode.intraday_minutes().is_none() {
        return;
    }
    let bucket_ts = normalize_bar_timestamp(observation_ts, analysis_mode, tz);
    if let Some(last) = bars.last_mut() {
        if last.timestamp == Some(bucket_ts) {
            last.high = last.high.max(price);
            last.low = last.low.min(price);
            last.close = price;
            return;
        }
        if last.timestamp.is_some_and(|ts| ts > bucket_ts) {
            return;
        }
    }
    let Some(dt) = tz.timestamp_opt(bucket_ts, 0).single() else {
        return;
    };
    bars.push(MarketData {
        date: dt.format("%Y-%m-%d").to_string(),
        datetime: Some(dt.format("%Y-%m-%d %H:%M").to_string()),
        timestamp: Some(bucket_ts),
        timezone: Some(tz_name.to_string()),
        high: price,
        low: price,
        close: price,
        volume: None,
        name: None,
    });
}

/// Remove trailing bars that carry no traded volume (0 or unreported). After
/// same-bucket live observations have been merged above, the remaining empty
/// trailing bar is a distinct post-close snapshot bucket (for example 15:30
/// after the real 15:15 closing bar).
fn trim_trailing_empty_bars(bars: &mut Vec<MarketData>) {
    while bars
        .last()
        .is_some_and(|b| b.volume.is_none_or(|v| v <= 0.0))
    {
        bars.pop();
    }
}

/// Extract Yahoo! JAPAN's server-rendered real-time TSE quote. The international
/// chart API marks Japanese quotes as delayed during the session, while the
/// public Japanese quote page exposes the current exchange price and HH:MM.
/// Class suffixes are build hashes, so match only the stable component prefixes.
fn parse_yahoo_japan_realtime_quote(html: &str) -> Option<(f64, NaiveTime)> {
    let price_block = html.find("_CommonPriceBoard__price_")?;
    let value_rel = html[price_block..].find("_StyledNumber__value_")?;
    let value_start = price_block + value_rel;
    let text_start = value_start + html[value_start..].find('>')? + 1;
    let text_end = text_start + html[text_start..].find('<')?;
    let price = html[text_start..text_end]
        .replace(',', "")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())?;

    let realtime = html.find("リアルタイム株価</li>")?;
    let time_rel = html[realtime..].find("<time>")?;
    let time_start = realtime + time_rel + "<time>".len();
    let time_end = time_start + html[time_start..].find("</time>")?;
    let time = NaiveTime::parse_from_str(&html[time_start..time_end], "%H:%M").ok()?;
    Some((price, time))
}

/// Fetches Yahoo! JAPAN's quote page and hands it to the parser.
///
/// Any failure — the page, the network, the parse — returns `None` rather than
/// an error, and the caller keeps the chart API's value. The real-time quote is
/// an improvement on that value, never a precondition for having one.
///
/// Yahoo!ファイナンス日本版の気配ページを取得し、パーサに渡す。
///
/// ページ・ネットワーク・パースのいずれが失敗しても、エラーではなく `None` を返し、
/// 呼び出し側はチャート API の値を保持する。リアルタイム気配はその値を良くするもので
/// あって、値を持つための前提条件ではない。
async fn fetch_yahoo_japan_realtime_quote(
    client: &Client,
    jp_code: &str,
) -> Option<(f64, NaiveTime)> {
    let url = format!("https://finance.yahoo.co.jp/quote/{jp_code}.T");
    let response = client
        .get(url)
        .header("accept", "text/html")
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    parse_yahoo_japan_realtime_quote(&response.text().await.ok()?)
}

/// Bars for the timeframe the [`AnalysisMode`] names — daily, intraday, weekly
/// or monthly. The interval and range that go with each mode are fixed here, so
/// two callers asking for the same mode receive the same window.
///
/// [`AnalysisMode`] が指す足——日足・分足・週足・月足——を取得する。各モードに対応する
/// interval と range はここで固定しているので、同じモードを求めた 2 つの呼び出しは
/// 同じ窓を受け取る。
pub async fn fetch_market_data_for_mode(
    ticker: &str,
    analysis_mode: AnalysisMode,
) -> Result<Vec<MarketData>> {
    Ok(
        fetch_market_data_snapshot_for_mode(ticker, analysis_mode, None, None, None)
            .await?
            .bars,
    )
}

/// Retrieves the analysis bars and the latest-price metadata as one snapshot,
/// keeping them separable: the bars are what indicators are computed on, the
/// latest observation is what the header shows.
///
/// For a Japanese ticker the real-time TSE quote is then overlaid on the latest
/// price and the latest-data time. That overlay is best-effort — if it fails,
/// the chart API's values stand, and the snapshot is still internally consistent.
///
/// 分析用の足と最新価格のメタ情報を 1 つのスナップショットとして取得し、両者を分離した
/// まま保つ。足は指標を計算する対象、最新取得価格はヘッダに出す値である。
///
/// 日本株ではそのあと、東証のリアルタイム気配を最新価格とデータ最新時刻に重ねる。この
/// 重ね合わせはベストエフォートで、失敗すればチャート API の値がそのまま残り、
/// スナップショットの内部整合は保たれる。
pub async fn fetch_market_data_snapshot_for_mode(
    ticker: &str,
    analysis_mode: AnalysisMode,
    proxy_url: Option<&str>,
    no_proxy: Option<&str>,
    range_override: Option<&str>,
) -> Result<MarketDataSnapshot> {
    let ysym = if let Some(code) = crate::bootstrap::jp_code_from_ticker(ticker) {
        format!("{}.T", code)
    } else {
        ticker.trim().to_string()
    };

    // Live analysis uses the timeframe's default range; the backtest passes a
    // user-chosen period (longer history) here.
    let range = range_override.unwrap_or_else(|| analysis_mode.data_range());
    // Yahoo serves the SAME data from two hosts (query2 primary, query1 alt).
    // Trying query1 after query2 is exhausted is pure resilience — identical
    // numbers, never a different data source. Order matters: primary first.
    let encoded_sym = urlencoding::encode(&ysym);
    let interval = analysis_mode.data_interval();
    let urls: [String; 2] = ["query2", "query1"].map(|host| {
        format!(
            "https://{host}.finance.yahoo.com/v8/finance/chart/{}?interval={}&range={}",
            encoded_sym, interval, range
        )
    });

    let mut builder = Client::builder()
        .user_agent("Mozilla/5.0 (xoksa)")
        .gzip(true)
        .brotli(true)
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(3));
    if let Some(url) = proxy_url {
        builder = builder.proxy(crate::utils::build_proxy(url, no_proxy)?);
    }
    let client = builder.build()?;

    // One slot for whichever failure happened last, transport or status, so the
    // final message can name the reason the provider gave rather than only the
    // shape of the failure.
    let mut last_failure: Option<String> = None;
    let mut text: Option<String> = None;
    const MARKET_DATA_FETCH_ATTEMPTS: usize = 2;
    // Try the primary host's retry budget first; if it is fully exhausted move
    // to the alternate host. Same data on both — this is resilience only.
    'hosts: for url in &urls {
        for attempt in 0..MARKET_DATA_FETCH_ATTEMPTS {
            match client
                .get(url)
                .header("accept", "application/json")
                .send()
                .await
            {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        text = Some(response.text().await?);
                        break 'hosts;
                    }
                    let should_retry = should_retry_market_data_status(status)
                        && attempt + 1 < MARKET_DATA_FETCH_ATTEMPTS;
                    // Read the body before discarding the response: Yahoo answers an
                    // unknown symbol with 404 *and* a JSON payload naming the reason.
                    // Keeping only the status threw that away.
                    let reason = response
                        .text()
                        .await
                        .ok()
                        .as_deref()
                        .and_then(yahoo_chart_error_reason);
                    last_failure = Some(match reason {
                        // The numeric status only: its canonical phrase ("404 Not
                        // Found") otherwise repeats the reason Yahoo just gave.
                        Some(reason) => format!("HTTP {} ({reason})", status.as_u16()),
                        None => status.to_string(),
                    });
                    if should_retry {
                        sleep(Duration::from_millis(200)).await;
                        continue;
                    }
                    break;
                }
                Err(err) => {
                    let should_retry = should_retry_market_data_error(&err)
                        && attempt + 1 < MARKET_DATA_FETCH_ATTEMPTS;
                    last_failure = Some(bounded_provider_text(&err.to_string()));
                    if should_retry {
                        sleep(Duration::from_millis(200)).await;
                        continue;
                    }
                    break;
                }
            }
        }
    }
    let text = text.ok_or_else(|| {
        let detail = last_failure.map(|d| format!(": {d}")).unwrap_or_default();
        anyhow!("❌ Market data API request failed{detail}")
    })?;

    let json: Value = serde_json::from_str(&text)?;
    if json.get("chart").is_none() || !json["chart"]["error"].is_null() {
        bail!("❌ Market data API request failed.");
    }

    let result = json["chart"]["result"]
        .as_array()
        .ok_or_else(|| anyhow!("❌ chart.result array missing"))?;
    if result.is_empty() {
        bail!("❌ chart.result is empty.");
    }

    let r0 = &result[0];

    let tz_name = r0["meta"]["exchangeTimezoneName"]
        .as_str()
        .unwrap_or("UTC")
        .to_string();
    let tz_parsed: Result<Tz, _> = tz_name.parse();
    let tz = match tz_parsed {
        Ok(v) => v,
        Err(_) => {
            crate::logging::info(
                "XK-MARKET-TZ",
                &format!("exchangeTimezoneName parse failed: {tz_name} -> fallback to UTC"),
            );
            chrono_tz::UTC
        }
    };

    let timestamps = r0["timestamp"]
        .as_array()
        .ok_or_else(|| anyhow!("❌ timestamp missing."))?;
    let q0 = &r0["indicators"]["quote"][0];
    let highs = q0["high"]
        .as_array()
        .ok_or_else(|| anyhow!("❌ high missing."))?;
    let lows = q0["low"]
        .as_array()
        .ok_or_else(|| anyhow!("❌ low missing."))?;
    let closes = q0["close"]
        .as_array()
        .ok_or_else(|| anyhow!("❌ close missing."))?;
    let volumes = q0["volume"].as_array();

    let n = timestamps
        .len()
        .min(highs.len())
        .min(lows.len())
        .min(closes.len());

    let mut out: Vec<MarketData> = Vec::with_capacity(n);

    for i in 0..n {
        let ts = match timestamps[i].as_i64() {
            Some(v) => v,
            None => continue,
        };

        let (h, l, c) = (highs[i].as_f64(), lows[i].as_f64(), closes[i].as_f64());
        if let (Some(h), Some(l), Some(c)) = (h, l, c) {
            let volume = volumes
                .and_then(|items| items.get(i))
                .and_then(|v| v.as_f64())
                .filter(|v| v.is_finite() && *v >= 0.0);

            let bar_ts = normalize_bar_timestamp(ts, analysis_mode, tz);
            let dt = tz
                .timestamp_opt(bar_ts, 0)
                .single()
                .ok_or_else(|| anyhow!("❌ timestamp conversion failed"))?;

            let date = dt.format("%Y-%m-%d").to_string();
            let datetime = dt.format("%Y-%m-%d %H:%M").to_string();

            out.push(MarketData {
                date,
                datetime: Some(datetime),
                timestamp: Some(bar_ts),
                timezone: Some(tz_name.clone()),
                high: h,
                low: l,
                close: c,
                volume,
                name: None,
            });
        }
    }

    // Yahoo also appends a current-price point with volume 0 during the live
    // session. Its exact observation timestamp normalizes into the same bucket
    // as the accumulated bar, so merge it first; otherwise the generic empty-bar
    // trim below would discard the freshest price (up to one interval stale).
    merge_duplicate_trailing_bucket(&mut out);

    // Drop trailing empty bars. After the session close Yahoo appends a
    // start-labelled bucket at the close time (e.g. a 15:30 bar on a 15:30 close)
    // carrying only the last price and volume 0 — an empty bar for a period that
    // never traded. It duplicates the real closing bar's indicators and shows a
    // false "0 volume", so remove it; a live forming bar keeps volume > 0 and is
    // retained.
    trim_trailing_empty_bars(&mut out);

    let mut market_data_latest_timestamp = r0["meta"]["regularMarketTime"].as_i64();
    let mut latest_observed_price = r0["meta"]["regularMarketPrice"]
        .as_f64()
        .or_else(|| r0["meta"]["regularMarketPrice"]["raw"].as_f64())
        .filter(|v| v.is_finite());

    // finance.yahoo.com deliberately reports TSE quotes about 15 minutes late.
    // Yahoo! JAPAN's public quote page is real-time, so use it for the latest
    // observation (the historical OHLCV series remains the chart response). This
    // is best-effort: any page/network/parse failure preserves the chart value.
    if let Some(jp_code) = crate::bootstrap::jp_code_from_ticker(ticker) {
        if let Some((price, quote_time)) = fetch_yahoo_japan_realtime_quote(&client, &jp_code).await
        {
            let quote_timestamp = market_data_latest_timestamp
                .and_then(|ts| tz.timestamp_opt(ts, 0).single())
                .and_then(|chart_dt| {
                    tz.from_local_datetime(&chart_dt.date_naive().and_time(quote_time))
                        .single()
                })
                .map(|dt| dt.timestamp());
            latest_observed_price = Some(price);
            if let Some(ts) = quote_timestamp {
                market_data_latest_timestamp = Some(ts);
                apply_realtime_intraday_observation(
                    &mut out,
                    price,
                    ts,
                    analysis_mode,
                    tz,
                    &tz_name,
                );
            }
        }
    }

    let market_data_latest_time = market_data_latest_timestamp.and_then(|ts| {
        tz.timestamp_opt(ts, 0)
            .single()
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
    });
    let latest_observation = latest_observed_price.map(|price| MarketLatestObservation { price });

    if out.len() < 2 {
        bail!("❌ Time series has fewer than 2 bars; cannot build technical indicators.");
    }

    // Currency the prices are quoted in, straight from the provider (not guessed
    // from the ticker suffix). Absent → None, so downstream shows a neutral label.
    let currency = r0["meta"]["currency"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let analyzed_at = Utc::now();
    let analyzed_at_local = analyzed_at.with_timezone(&tz).format("%Y-%m-%d %H:%M");
    Ok(MarketDataSnapshot {
        bars: out,
        latest_observation,
        market_data_latest_time,
        market_data_latest_timestamp,
        timezone: tz_name,
        currency,
        analyzed_at: analyzed_at_local.to_string(),
        analyzed_at_timestamp: analyzed_at.timestamp(),
        source_note: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn bar(tag: &str, volume: Option<f64>) -> MarketData {
        MarketData {
            date: tag.to_string(),
            datetime: Some(tag.to_string()),
            timestamp: None,
            timezone: None,
            high: 1.0,
            low: 1.0,
            close: 1.0,
            volume,
            name: None,
        }
    }

    #[test]
    fn trailing_empty_bar_is_dropped_but_live_forming_bar_kept() {
        // Post-close snapshot: last bar has volume 0 → dropped.
        let mut bars = vec![
            bar("15:00", Some(100.0)),
            bar("15:15", Some(6_352_700.0)),
            bar("15:30", Some(0.0)),
        ];
        trim_trailing_empty_bars(&mut bars);
        assert_eq!(bars.len(), 2);
        assert_eq!(bars.last().unwrap().date, "15:15");

        // Unreported (None) trailing volume is also dropped.
        let mut with_none = vec![bar("a", Some(5.0)), bar("b", None)];
        trim_trailing_empty_bars(&mut with_none);
        assert_eq!(with_none.len(), 1);

        // A live forming bar (volume > 0) is retained.
        let mut live = vec![bar("a", Some(100.0)), bar("b", Some(5.0))];
        trim_trailing_empty_bars(&mut live);
        assert_eq!(live.len(), 2);
    }

    #[test]
    fn zero_volume_live_observation_updates_same_bucket() {
        let mut bars = vec![
            MarketData {
                timestamp: Some(100),
                high: 3048.0,
                low: 3020.0,
                close: 3022.0,
                volume: Some(587_700.0),
                ..bar("09:30", Some(587_700.0))
            },
            MarketData {
                timestamp: Some(100),
                high: 3050.0,
                low: 3023.0,
                close: 3049.0,
                volume: Some(0.0),
                ..bar("09:41", Some(0.0))
            },
        ];

        merge_duplicate_trailing_bucket(&mut bars);
        trim_trailing_empty_bars(&mut bars);

        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].close, 3049.0);
        assert_eq!(bars[0].high, 3050.0);
        assert_eq!(bars[0].low, 3020.0);
        assert_eq!(bars[0].volume, Some(587_700.0));
    }

    #[test]
    fn distinct_post_close_empty_bucket_is_still_dropped() {
        let mut bars = vec![
            MarketData {
                timestamp: Some(100),
                ..bar("15:15", Some(6_352_700.0))
            },
            MarketData {
                timestamp: Some(200),
                ..bar("15:30", Some(0.0))
            },
        ];

        merge_duplicate_trailing_bucket(&mut bars);
        trim_trailing_empty_bars(&mut bars);

        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].date, "15:15");
    }

    #[test]
    fn yahoo_japan_realtime_quote_parser_reads_price_and_time() {
        let html = r#"
            <span class="_CommonPriceBoard__price_ab12">
              <span class="_StyledNumber__value_cd34">3,057.5</span>
            </span>
            <li>リアルタイム株価</li><li><time>10:26</time></li>
        "#;

        let (price, time) = parse_yahoo_japan_realtime_quote(html).expect("quote");
        assert_eq!(price, 3057.5);
        assert_eq!(time, NaiveTime::from_hms_opt(10, 26, 0).unwrap());
    }

    #[test]
    fn yahoo_japan_realtime_quote_parser_rejects_missing_marker() {
        let html = r#"<span class="_CommonPriceBoard__price_x"><span class="_StyledNumber__value_y">3,057</span></span>"#;
        assert!(parse_yahoo_japan_realtime_quote(html).is_none());
    }

    #[test]
    fn realtime_quote_appends_the_current_intraday_bucket() {
        let tz = chrono_tz::Asia::Tokyo;
        let old_ts = tz
            .with_ymd_and_hms(2026, 8, 20, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let live_ts = tz
            .with_ymd_and_hms(2026, 8, 20, 10, 28, 0)
            .single()
            .unwrap()
            .timestamp();
        let mut bars = vec![MarketData {
            timestamp: Some(old_ts),
            ..bar("10:00", Some(100.0))
        }];

        apply_realtime_intraday_observation(
            &mut bars,
            3054.0,
            live_ts,
            AnalysisMode::Intraday15m,
            tz,
            "Asia/Tokyo",
        );

        assert_eq!(bars.len(), 2);
        assert_eq!(bars[1].datetime.as_deref(), Some("2026-08-20 10:15"));
        assert_eq!(bars[1].close, 3054.0);
        assert_eq!(bars[1].volume, None);
    }

    #[test]
    fn realtime_quote_updates_an_existing_intraday_bucket() {
        let tz = chrono_tz::Asia::Tokyo;
        let bucket_ts = tz
            .with_ymd_and_hms(2026, 8, 20, 10, 15, 0)
            .single()
            .unwrap()
            .timestamp();
        let live_ts = bucket_ts + 13 * 60;
        let mut bars = vec![MarketData {
            timestamp: Some(bucket_ts),
            high: 3050.0,
            low: 3030.0,
            close: 3040.0,
            volume: Some(50.0),
            ..bar("10:15", Some(50.0))
        }];

        apply_realtime_intraday_observation(
            &mut bars,
            3054.0,
            live_ts,
            AnalysisMode::Intraday15m,
            tz,
            "Asia/Tokyo",
        );

        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].high, 3054.0);
        assert_eq!(bars[0].low, 3030.0);
        assert_eq!(bars[0].close, 3054.0);
        assert_eq!(bars[0].volume, Some(50.0));
    }

    #[test]
    fn daily_mode_uses_existing_data_params() {
        assert_eq!(AnalysisMode::Daily.data_interval(), "1d");
        assert_eq!(AnalysisMode::Daily.data_range(), "3mo");
    }

    #[test]
    fn intraday_30m_mode_uses_short_data_params() {
        assert_eq!(AnalysisMode::Intraday30m.data_interval(), "30m");
        assert_eq!(AnalysisMode::Intraday30m.data_range(), "1mo");
    }

    #[test]
    fn intraday_15m_mode_uses_short_data_params() {
        assert_eq!(AnalysisMode::Intraday15m.data_interval(), "15m");
        assert_eq!(AnalysisMode::Intraday15m.data_range(), "1mo");
    }

    #[test]
    fn intraday_5m_mode_uses_short_data_params() {
        assert_eq!(AnalysisMode::Intraday5m.data_interval(), "5m");
        assert_eq!(AnalysisMode::Intraday5m.data_range(), "5d");
    }

    #[test]
    fn weekly_mode_uses_higher_timeframe_data_params() {
        assert_eq!(AnalysisMode::Weekly.data_interval(), "1wk");
        assert_eq!(AnalysisMode::Weekly.data_range(), "2y");
    }

    #[test]
    fn monthly_mode_uses_higher_timeframe_data_params() {
        assert_eq!(AnalysisMode::Monthly.data_interval(), "1mo");
        assert_eq!(AnalysisMode::Monthly.data_range(), "10y");
    }

    #[test]
    fn retry_status_is_limited_to_rate_limit_and_server_errors() {
        assert!(should_retry_market_data_status(
            StatusCode::TOO_MANY_REQUESTS
        ));
        assert!(should_retry_market_data_status(
            StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(should_retry_market_data_status(StatusCode::BAD_GATEWAY));

        assert!(!should_retry_market_data_status(StatusCode::BAD_REQUEST));
        assert!(!should_retry_market_data_status(StatusCode::UNAUTHORIZED));
        assert!(!should_retry_market_data_status(StatusCode::FORBIDDEN));
        assert!(!should_retry_market_data_status(StatusCode::NOT_FOUND));
    }

    #[test]
    fn intraday_bar_timestamp_floors_to_bucket_start() {
        let tz: Tz = "Asia/Tokyo".parse().expect("valid timezone");
        let input = tz
            .with_ymd_and_hms(2026, 5, 12, 13, 23, 45)
            .single()
            .expect("valid datetime")
            .timestamp();
        let expected_30m = tz
            .with_ymd_and_hms(2026, 5, 12, 13, 0, 0)
            .single()
            .expect("valid datetime")
            .timestamp();
        let expected_15m = tz
            .with_ymd_and_hms(2026, 5, 12, 13, 15, 0)
            .single()
            .expect("valid datetime")
            .timestamp();
        let expected_5m = tz
            .with_ymd_and_hms(2026, 5, 12, 13, 20, 0)
            .single()
            .expect("valid datetime")
            .timestamp();

        assert_eq!(
            normalize_bar_timestamp(input, AnalysisMode::Intraday30m, tz),
            expected_30m
        );
        assert_eq!(
            normalize_bar_timestamp(input, AnalysisMode::Intraday15m, tz),
            expected_15m
        );
        assert_eq!(
            normalize_bar_timestamp(input, AnalysisMode::Intraday5m, tz),
            expected_5m
        );
    }

    #[test]
    fn daily_bar_timestamp_keeps_source_timestamp() {
        let tz: Tz = "Asia/Tokyo".parse().expect("valid timezone");
        let input = tz
            .with_ymd_and_hms(2026, 5, 12, 13, 23, 45)
            .single()
            .expect("valid datetime")
            .timestamp();

        assert_eq!(
            normalize_bar_timestamp(input, AnalysisMode::Daily, tz),
            input
        );
    }

    // ── Retry policy (regression) ──────────────────────────────────────────
    // Locks which HTTP statuses are considered transient and retried.

    #[test]
    fn retries_on_rate_limit_and_server_errors() {
        assert!(should_retry_market_data_status(
            StatusCode::TOO_MANY_REQUESTS
        )); // 429
        assert!(should_retry_market_data_status(
            StatusCode::SERVICE_UNAVAILABLE
        )); // 503
        assert!(should_retry_market_data_status(
            StatusCode::INTERNAL_SERVER_ERROR
        )); // 500
        assert!(should_retry_market_data_status(StatusCode::BAD_GATEWAY)); // 502
    }

    #[test]
    fn does_not_retry_on_client_errors() {
        assert!(!should_retry_market_data_status(StatusCode::NOT_FOUND)); // 404
        assert!(!should_retry_market_data_status(StatusCode::BAD_REQUEST)); // 400
        assert!(!should_retry_market_data_status(StatusCode::UNAUTHORIZED)); // 401
    }

    // ── Failure injection via the PriceFetcher seam ────────────────────────
    // A provider failure must surface as an Err to the caller — never a
    // fabricated snapshot. The trait is the seam where alternative or failing
    // providers can be injected.

    struct FailingPriceFetcher;
    impl crate::traits::PriceFetcher for FailingPriceFetcher {
        async fn fetch_snapshot(
            &self,
            _ticker: &str,
            _mode: AnalysisMode,
        ) -> Result<MarketDataSnapshot> {
            bail!("simulated upstream failure")
        }
    }

    #[tokio::test]
    async fn provider_failure_propagates_as_error() {
        use crate::traits::PriceFetcher;
        let fetcher = FailingPriceFetcher;
        let result = fetcher.fetch_snapshot("SPY", AnalysisMode::Daily).await;
        assert!(
            result.is_err(),
            "a provider failure must propagate as Err, not a fabricated snapshot"
        );
    }

    /// Shipping-inspection item 10 (end-to-end, real fallback vendor): when the
    /// primary provider genuinely fails, the real `Failover` arm adopts Stooq's
    /// WHOLE snapshot and that snapshot is labeled degraded (`source_note`).
    /// The primary is forced to fail offline via an unreachable proxy; the
    /// secondary hits the real Stooq feed.
    ///
    /// Ignored by default (hits the network). Run it in the shipping inspection:
    /// `cargo test -- --ignored failover_adopts_stooq`.
    #[tokio::test]
    #[ignore]
    async fn failover_adopts_stooq_and_labels_degraded_when_primary_fails() {
        use crate::traits::PriceFetcher;
        // Primary Yahoo pointed at an unreachable proxy → the fetch genuinely fails.
        let primary = PriceFetcherKind::Yahoo(YahooFinancePriceFetcher {
            proxy_url: Some("http://127.0.0.1:1".to_string()),
            no_proxy: None,
        });
        // Secondary is the real Stooq feed (no proxy).
        let secondary = PriceFetcherKind::Stooq(StooqPriceFetcher {
            proxy_url: None,
            no_proxy: None,
        });
        let fetcher = PriceFetcherKind::Failover {
            primary: Box::new(primary),
            secondary: Box::new(secondary),
        };
        let snap = fetcher
            .fetch_snapshot("AAPL", AnalysisMode::Daily)
            .await
            .expect("primary down → adopt the Stooq fallback snapshot");
        assert!(snap.bars.len() >= 2, "Stooq must return real bars");
        assert_eq!(
            snap.source_note.as_deref(),
            Some(STOOQ_FALLBACK_NOTE),
            "a fallback snapshot must be labeled degraded"
        );
    }

    /// Shipping-inspection item 10 (offline, deterministic): the real `Failover`
    /// arm delegates to the SECONDARY when the primary genuinely fails. The
    /// primary is Yahoo pointed at an unreachable proxy (fails on connect); the
    /// secondary is Stooq asked for an intraday timeframe it rejects *before any
    /// network*. The surfaced error is therefore the secondary's — which can only
    /// happen if the failover fell through to it. No network required.
    #[tokio::test]
    async fn failover_delegates_to_secondary_when_primary_fails() {
        use crate::traits::PriceFetcher;
        let primary = PriceFetcherKind::Yahoo(YahooFinancePriceFetcher {
            proxy_url: Some("http://127.0.0.1:1".to_string()),
            no_proxy: None,
        });
        let secondary = PriceFetcherKind::Stooq(StooqPriceFetcher {
            proxy_url: None,
            no_proxy: None,
        });
        let fetcher = PriceFetcherKind::Failover {
            primary: Box::new(primary),
            secondary: Box::new(secondary),
        };
        let err = fetcher
            .fetch_snapshot("AAPL", AnalysisMode::Intraday30m)
            .await
            .expect_err("both providers fail for an intraday request");
        let msg = err.to_string();
        assert!(
            msg.contains("Stooq") && msg.contains("daily-only"),
            "failover must fall through to the Stooq secondary; got: {msg}"
        );
        // When both fail the primary's reason must survive: it is the one that knows
        // an unknown ticker. Reporting only the fallback's complaint told the user
        // about a CSV feed when their ticker was wrong.
        assert!(
            msg.contains("Market data API request failed"),
            "the primary's reason must lead the message; got: {msg}"
        );
    }

    // ── Untrusted provider text in messages ────────────────────────────────

    /// A provider body must not be able to rewrite the line it is printed on, and
    /// must not run away with the message.
    #[test]
    fn bounded_provider_text_drops_control_characters_and_caps_length() {
        let hostile = "abc\u{1b}[2Jdef\nghi\tjkl";
        let out = bounded_provider_text(hostile);
        assert!(
            !out.chars().any(char::is_control),
            "control characters must not survive: {out:?}"
        );
        assert_eq!(out, "abc [2Jdef ghi jkl");

        let long = "x".repeat(500);
        let capped = bounded_provider_text(&long);
        assert!(
            capped.chars().count() <= 161,
            "capped text was {} chars",
            capped.chars().count()
        );
        assert!(capped.ends_with('…'));
    }

    /// Yahoo names the reason in the body it sends with a non-2xx status. That is
    /// what the user needs for a mistyped ticker.
    #[test]
    fn yahoo_chart_error_reason_reads_the_reason_yahoo_gives() {
        let body = r#"{"chart":{"result":null,"error":{"code":"Not Found","description":"No data found, symbol may be delisted"}}}"#;
        let reason = yahoo_chart_error_reason(body).expect("a chart error carries a reason");
        assert!(reason.contains("Not Found"));
        assert!(reason.contains("No data found"));
    }

    #[test]
    fn yahoo_chart_error_reason_is_absent_for_a_normal_body() {
        assert!(yahoo_chart_error_reason(r#"{"chart":{"result":[],"error":null}}"#).is_none());
        assert!(yahoo_chart_error_reason("not json at all").is_none());
    }

    // ── Stooq symbol mapping ───────────────────────────────────────────────

    #[test]
    fn stooq_symbol_maps_jp_ticker_to_dot_jp() {
        assert_eq!(stooq_symbol("7203.T"), "7203.jp");
        // Bare 4-digit JP code also maps to .jp.
        assert_eq!(stooq_symbol("7203"), "7203.jp");
    }

    #[test]
    fn stooq_symbol_maps_us_ticker_to_lowercased_dot_us() {
        assert_eq!(stooq_symbol("AAPL"), "aapl.us");
        assert_eq!(stooq_symbol("spy"), "spy.us");
    }

    // ── Stooq CSV parsing ──────────────────────────────────────────────────

    /// On some networks Stooq serves an anti-bot JS challenge page. That is a
    /// blocked fallback, not a malformed CSV, and the page must not be quoted into
    /// the error — a whole `<!DOCTYPE html>…` line used to reach the terminal.
    #[test]
    fn stooq_html_challenge_is_named_rather_than_quoted() {
        let page = "<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head>\
                    <body><noscript>This site requires JavaScript</noscript></body></html>";
        let err = parse_stooq_csv(page).expect_err("an HTML page is not CSV data");
        let msg = err.to_string();
        assert!(
            msg.contains("web page instead of CSV"),
            "the failure must be named as what it is; got: {msg}"
        );
        assert!(
            !msg.contains("<!DOCTYPE") && !msg.contains("<html"),
            "the page must not be quoted into the message; got: {msg}"
        );
    }

    #[test]
    fn stooq_csv_parses_into_expected_bars() {
        let csv = "Date,Open,High,Low,Close,Volume\n\
                   2026-06-01,10.0,12.5,9.5,11.0,1000\n\
                   2026-06-02,11.0,13.0,10.5,12.25,2000\n";
        let bars = parse_stooq_csv(csv).expect("valid CSV parses");
        assert_eq!(bars.len(), 2);

        assert_eq!(bars[0].date, "2026-06-01");
        assert_eq!(bars[0].high, 12.5);
        assert_eq!(bars[0].low, 9.5);
        assert_eq!(bars[0].close, 11.0);
        assert_eq!(bars[0].volume, Some(1000.0));
        // Daily Stooq bars carry no intraday wall-clock time.
        assert!(bars[0].datetime.is_none());
        assert!(bars[0].timestamp.is_none());
        assert!(bars[0].timezone.is_none());

        assert_eq!(bars[1].date, "2026-06-02");
        assert_eq!(bars[1].close, 12.25);
    }

    #[test]
    fn stooq_csv_skips_rows_with_missing_values() {
        // Stooq marks missing values as "N/D"; such rows must be skipped, not fabricated.
        let csv = "Date,Open,High,Low,Close,Volume\n\
                   2026-06-01,10.0,12.5,9.5,11.0,1000\n\
                   2026-06-02,N/D,N/D,N/D,N/D,N/D\n";
        let bars = parse_stooq_csv(csv).expect("valid CSV parses");
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].date, "2026-06-01");
    }

    #[test]
    fn stooq_rejects_intraday_timeframes() {
        // Daily-only scope: intraday must Err, never substitute daily bars.
        assert!(!stooq_supports(AnalysisMode::Intraday5m));
        assert!(!stooq_supports(AnalysisMode::Intraday60m));
        assert!(stooq_supports(AnalysisMode::Daily));
    }

    // ── Failover orchestration ─────────────────────────────────────────────

    struct StubStooqFetcher;
    impl crate::traits::PriceFetcher for StubStooqFetcher {
        async fn fetch_snapshot(
            &self,
            _ticker: &str,
            _mode: AnalysisMode,
        ) -> Result<MarketDataSnapshot> {
            Ok(MarketDataSnapshot {
                bars: vec![
                    MarketData {
                        date: "2026-06-01".into(),
                        datetime: None,
                        timestamp: None,
                        timezone: None,
                        high: 12.5,
                        low: 9.5,
                        close: 11.0,
                        volume: Some(1000.0),
                        name: None,
                    },
                    MarketData {
                        date: "2026-06-02".into(),
                        datetime: None,
                        timestamp: None,
                        timezone: None,
                        high: 13.0,
                        low: 10.5,
                        close: 12.25,
                        volume: Some(2000.0),
                        name: None,
                    },
                ],
                latest_observation: Some(MarketLatestObservation { price: 12.25 }),
                market_data_latest_time: Some("2026-06-02".into()),
                market_data_latest_timestamp: None,
                timezone: "UTC".into(),
                currency: None,
                analyzed_at: "2026-06-02 00:00".into(),
                analyzed_at_timestamp: 0,
                source_note: Some(STOOQ_FALLBACK_NOTE.to_string()),
            })
        }
    }

    // A generic failover exercising the SAME orchestration logic as
    // `PriceFetcherKind::Failover`, over injectable trait doubles: a failing
    // primary must yield the secondary's WHOLE snapshot, degraded-labelled.
    struct GenericFailover<P, S> {
        primary: P,
        secondary: S,
    }
    impl<P, S> crate::traits::PriceFetcher for GenericFailover<P, S>
    where
        P: crate::traits::PriceFetcher,
        S: crate::traits::PriceFetcher,
    {
        async fn fetch_snapshot(
            &self,
            ticker: &str,
            mode: AnalysisMode,
        ) -> Result<MarketDataSnapshot> {
            match self.primary.fetch_snapshot(ticker, mode).await {
                Ok(s) => Ok(s),
                Err(_) => self.secondary.fetch_snapshot(ticker, mode).await,
            }
        }
    }

    #[tokio::test]
    async fn failover_uses_secondary_and_marks_degraded_when_primary_errs() {
        use crate::traits::PriceFetcher;
        let failover = GenericFailover {
            primary: FailingPriceFetcher,
            secondary: StubStooqFetcher,
        };
        let snapshot = failover
            .fetch_snapshot("AAPL", AnalysisMode::Daily)
            .await
            .expect("secondary provides a snapshot when primary fails");
        assert_eq!(
            snapshot.source_note.as_deref(),
            Some(STOOQ_FALLBACK_NOTE),
            "a fallback snapshot must be marked degraded"
        );
        assert_eq!(snapshot.bars.len(), 2);
    }

    #[test]
    fn build_price_fetcher_yields_yahoo_primary_stooq_secondary_failover() {
        let config = crate::config::Config::default();
        match build_price_fetcher(&config) {
            PriceFetcherKind::Failover { primary, secondary } => {
                assert!(matches!(*primary, PriceFetcherKind::Yahoo(_)));
                assert!(matches!(*secondary, PriceFetcherKind::Stooq(_)));
            }
            _ => panic!("default provider must be a Yahoo→Stooq failover"),
        }
    }
}
