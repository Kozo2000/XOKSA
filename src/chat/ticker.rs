//! Ticker loading, switching, reloading, and the autoreload notice.
//! Operates on `ChatSession`/`Config` via descendant-module access.

use super::*;

pub(super) struct TickerSwitchData {
    pub(super) ticker: String,
    pub(super) ticker_label: String,
    pub(super) base_context: String,
    pub(super) technical_text: String,
    pub(super) fundamental_text: String,
    pub(super) news: Vec<NewsItem>,
    pub(super) data_as_of: String,
    /// Confirmed values built from the guard and fundamental data at load time.
    pub(super) facts: Option<crate::integrity::SymbolFacts>,
}

pub(super) struct LoadedTickerContext {
    pub(super) technical_text: String,
    /// Compact reading of the enabled indicators (for the autoreload notice).
    pub(super) indicator_summary: String,
    pub(super) fundamental_text: String,
    /// None = not requested or fetch failed (caller keeps existing news); Some = fetched (possibly empty)
    pub(super) news_items: Option<Vec<NewsItem>>,
    pub(super) context: String,
    pub(super) extensions_ok: bool,
    pub(super) label: String,
    pub(super) latest_price: f64,
    pub(super) data_as_of: String,
    /// Whether this load actually went for the fundamentals. `false` means the
    /// caller asked for a technical-only refresh, so an empty `fundamental_text`
    /// means "not fetched" rather than "none"; `true` means the block below is
    /// the result, an empty one included (the fetch failed or reported nothing).
    pub(super) fundamental_fetched: bool,
    /// Confirmed values of this instrument, for the output-integrity guard.
    pub(super) facts: crate::integrity::SymbolFacts,
}

pub(super) async fn load_ticker_analysis(
    base_config: &Config,
    symbol: &str,
    ticker_name_map: &HashMap<String, String>,
    fetch_news: bool,
    fetch_fundamental: bool,
) -> Result<LoadedTickerContext> {
    let mut ticker_config = base_config.clone();
    ticker_config.ticker = symbol.to_string();

    let price_fetcher = crate::market::build_price_fetcher(base_config);
    let snapshot = crate::traits::PriceFetcher::fetch_snapshot(
        &price_fetcher,
        symbol,
        base_config.analysis_mode,
    )
    .await?;

    anyhow::ensure!(
        !snapshot.bars.is_empty(),
        "No price data for '{}'. The symbol may not exist or markets may be closed.",
        symbol
    );

    let mut sorted_data = snapshot.bars.clone();
    sorted_data.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.datetime.cmp(&b.datetime))
            .then_with(|| a.date.cmp(&b.date))
    });

    let mut guard = crate::technical::build_basic_technical_entry(
        &ticker_config,
        &sorted_data,
        ticker_name_map,
    )?;

    guard.set_timezone(&snapshot.timezone);
    if let Some(t) = snapshot.market_data_latest_time.as_deref() {
        guard.set_market_data_latest_time(t);
    }
    if let Some(ts) = snapshot.market_data_latest_timestamp {
        guard.set_market_data_latest_timestamp(ts);
    }
    if let Some(obs) = &snapshot.latest_observation {
        guard.set_latest_observed_price(obs.price);
    }
    guard.set_analyzed_at(&snapshot.analyzed_at);
    guard.set_analyzed_at_timestamp(snapshot.analyzed_at_timestamp);

    let extensions_ok = match crate::technical::evaluate_all_selected_extensions(
        &ticker_config,
        &sorted_data,
        &mut guard,
    ) {
        Ok(()) => true,
        Err(e) => {
            crate::logging::warn(
                "XK-IND-EVAL",
                &format!("extended indicator evaluation failed: {e}"),
            );
            false
        }
    };

    let technical_text = crate::app::collect_display_lines(&ticker_config, &guard).join("\n");

    let fundamental_fetched = base_config.fundamental && fetch_fundamental;
    let fundamental_data = if fundamental_fetched {
        let lp = Some(
            guard
                .get_latest_observed_price()
                .unwrap_or_else(|| guard.get_close()),
        );
        match crate::fundamental::fetch_fundamental_data(symbol, &ticker_config, lp).await {
            Ok(d) => Some(d),
            Err(e) => {
                // Fundamentals are supplementary; absence is expected (e.g. ETFs have
                // no SEC filings). Record to the log file at INFO (no console noise),
                // not as a user-facing error.
                crate::logging::info(
                    "XK-FUND-MISSING",
                    &format!("fundamental data unavailable for {symbol}: {e}"),
                );
                None
            }
        }
    } else {
        None
    };

    let fundamental_text = fundamental_data
        .as_ref()
        .map(|d| crate::fundamental::render_fundamental_display(d, &ticker_config.lang).join("\n"))
        .unwrap_or_default();

    // None = not requested or fetch failed (caller keeps existing news)
    // Some(vec) = fetch succeeded (empty vec means no articles found)
    let articles: Option<Vec<crate::news::Article>> = if fetch_news && !base_config.no_news {
        let nf = crate::news::BraveArticleFetcher {
            proxy_url: base_config.https_proxy.clone(),
            no_proxy: base_config.no_proxy.clone(),
        };
        match crate::news::news_flow_controller(
            &crate::news::NewsSubject::from_guard(&guard),
            &ticker_config,
            &nf,
        )
        .await
        {
            Ok(a) => Some(a),
            Err(e) => {
                // News is supplementary; a fetch failure is non-fatal. File-only record.
                crate::logging::info("XK-NEWS-FETCH", &format!("news fetch failed: {e}"));
                None
            }
        }
    } else {
        None
    };

    let news_items = articles.as_ref().map(|arts| {
        arts.iter()
            .enumerate()
            .map(|(i, a)| NewsItem {
                id: format!("N{:02}", i + 1),
                title: a.title.clone(),
                url: a.url.clone(),
            })
            .collect()
    });

    // base_context carries NO news — news reaches the model only via the live news
    // buffer (news_items), refreshed per turn from the panel, so the LLM sees the
    // displayed news (no frozen copy, no `/basic` duplication). Keep fundamentals.
    let mut ctx_cfg = ticker_config.clone();
    ctx_cfg.no_news = true;
    let context =
        crate::prompt::build_analysis_prompt(&ctx_cfg, &guard, None, fundamental_data.as_ref());

    let label = format!("{} ({})", guard.get_ticker(), guard.get_name());
    let latest_price = crate::utils::display_price_for_diff(&guard);
    let indicator_summary = notice_indicator_summary(&guard, &ticker_config, &ticker_config.lang);
    let data_as_of = guard
        .get_market_data_latest_time()
        .or_else(|| guard.get_analyzed_at())
        .unwrap_or("")
        .to_string();

    Ok(LoadedTickerContext {
        technical_text,
        indicator_summary,
        fundamental_text,
        news_items,
        context,
        extensions_ok,
        label,
        latest_price,
        data_as_of,
        fundamental_fetched,
        // Confirmed values are taken from the structures that own them - the
        // guard and the fundamental data - not from any rendered text. This is
        // what the output-integrity guard verifies the model's numbers against.
        facts: crate::integrity::SymbolFacts::from_sources(
            &guard,
            &ticker_config,
            snapshot.currency.as_deref(),
            fundamental_data.as_ref(),
        ),
    })
}

pub(super) fn apply_ticker_switch(
    session: &mut ChatSession,
    config: &mut Config,
    data: TickerSwitchData,
) {
    config.ticker = data.ticker;
    session.tickers = vec![super::TickerEntry {
        label: data.ticker_label,
        base_context: data.base_context,
        technical_text: data.technical_text,
        indicator_summary: String::new(),
        fundamental_text: data.fundamental_text,
        news: data.news,
        data_as_of: data.data_as_of,
        facts: data.facts,
    }];
    session.recent_turns.clear();
    session.conversation_summary = None;
    session.debate_entries.clear();
    session.board_entries.clear();
    session.board_entry_counter = 0;
}

pub(super) struct TickerReloadData {
    pub(super) base_context: String,
    pub(super) technical_text: String,
    pub(super) indicator_summary: String,
    pub(super) news: Option<Vec<NewsItem>>,
    pub(super) data_as_of: String,
    pub(super) facts: Option<crate::integrity::SymbolFacts>,
    /// The fundamental block this load produced, or `None` when it did not go
    /// for the fundamentals at all. This is the one switch that keeps the
    /// display, the prompt context and the confirmed data from drifting apart:
    /// `Some` replaces all three (an empty block included, which is what a failed
    /// fetch produces), `None` keeps all three.
    pub(super) fundamental: Option<String>,
}

/// Swap in a fresh load for the ticker at `idx`.
///
/// Display text, prompt context and the confirmed data the output-integrity
/// guard verifies against all come from the same load, so an answer about the
/// new values is accepted and one repeating the old values is not.
///
/// Three fundamental outcomes are distinguished, because they are three
/// different states and only one of them may keep old data:
///
/// * **not fetched** — a technical-only refresh: the fundamental display, the
///   prompt context and the confirmed fundamental values all stay as they were;
/// * **fetched and failed** — the display goes empty, so the confirmed values go
///   with it: nothing is verified against a block the user can no longer see;
/// * **fetched successfully** — all three move to the new data.
pub(super) fn apply_ticker_reload(session: &mut ChatSession, idx: usize, data: TickerReloadData) {
    if let Some(entry) = session.tickers.get_mut(idx) {
        entry.base_context = data.base_context;
        entry.technical_text = data.technical_text;
        entry.indicator_summary = data.indicator_summary;
        if let Some(n) = data.news {
            entry.news = n;
        }
        entry.data_as_of = data.data_as_of;
        let carry_fundamentals = data.fundamental.is_none();
        if let Some(text) = data.fundamental {
            entry.fundamental_text = text;
        }
        if let Some(mut fresh) = data.facts {
            if carry_fundamentals {
                if let Some(previous) = entry.facts.as_ref() {
                    fresh.carry_over_fundamentals(previous);
                }
            }
            entry.facts = Some(fresh);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_ticker_add(
    session: &mut ChatSession,
    ticker_label: String,
    additional_context: String,
    technical_text: String,
    fundamental_text: String,
    news: Vec<NewsItem>,
    data_as_of: String,
    facts: Option<crate::integrity::SymbolFacts>,
) {
    session.tickers.push(super::TickerEntry {
        label: ticker_label,
        base_context: additional_context,
        technical_text,
        indicator_summary: String::new(),
        fundamental_text,
        news,
        data_as_of,
        facts,
    });
}

/// Symbol part of a ticker label (`"7203.T (トヨタ)"` → `"7203.T"`).
fn symbol_of(label: &str) -> String {
    label.split(" (").next().unwrap_or(label).to_string()
}

/// Point the session's news buffers at the panel's currently-displayed news (the
/// `/news` store keyed `sid|symbol`), so the LLM reasons over exactly what the user
/// sees. Overwrites a ticker's buffer only on a store HIT (the panel fetched news
/// for that symbol); a miss keeps the session's own fetched news — so the CLI (no
/// panel → always a miss) is unchanged. Called every turn, so a panel change
/// (symbol / search / filter toggle) is reflected on the next turn. The panel
/// fetches only the active symbol, so comparison extras keep their fetched news.
pub(super) fn refresh_news_from_panel(session: &mut ChatSession, sid: &str) {
    for t in session.tickers.iter_mut() {
        let sym = symbol_of(&t.label);
        if let Some(items) = super::exec::panel_news_items(sid, &sym) {
            t.news = items;
        }
    }
}

/// Make the session's comparison tickers (slots ≥1, i.e. everything after the
/// active/primary) exactly match `extras`: load+append the missing ones, drop the
/// ones no longer wanted. The primary (slot 0) and conversation history are left
/// untouched — switching the active ticker is handled by the caller (a new
/// session). Web UI: keeps the header chips and the chat's loaded set in sync.
pub(super) async fn sync_extra_tickers(
    session: &mut ChatSession,
    config: &Config,
    name_map: &HashMap<String, String>,
    extras: &[String],
    out: &mut ChatOut,
) {
    let is_empty = session.ticker_labels().len() == 1 && session.ticker_labels()[0].is_empty();
    if is_empty {
        return; // no primary loaded (e.g. build failed) — nothing to compare against
    }
    // Drop loaded extras that are no longer wanted (scan from the end).
    let mut i = session.tickers.len();
    while i > 1 {
        i -= 1;
        let sym = symbol_of(&session.tickers[i].label);
        let wanted = extras.iter().any(|e| e.eq_ignore_ascii_case(&sym));
        if !wanted {
            session.tickers.remove(i);
        }
    }
    // Add wanted extras not already loaded (max 5 total, never re-add the primary).
    let primary = symbol_of(&session.tickers[0].label);
    for sym in extras {
        if session.ticker_labels().len() >= 5 {
            break;
        }
        if sym.eq_ignore_ascii_case(&primary) {
            continue;
        }
        let loaded = session
            .ticker_labels()
            .iter()
            .any(|l| symbol_of(l).eq_ignore_ascii_case(sym));
        if loaded {
            continue;
        }
        match load_ticker_analysis(config, sym, name_map, !config.no_news, true).await {
            Ok(c) => apply_ticker_add(
                session,
                c.label,
                c.context,
                c.technical_text,
                c.fundamental_text,
                c.news_items.unwrap_or_default(),
                c.data_as_of,
                Some(c.facts),
            ),
            Err(e) => out.err(format!("⚠️ {}: {}", sym, e)),
        }
    }
}

/// The symbols currently loaded in the session (primary first), for reflecting
/// the chat's ticker set back to the Web UI chips.
pub(super) fn loaded_symbols(session: &ChatSession) -> Vec<String> {
    session
        .ticker_labels()
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| symbol_of(l))
        .collect()
}

pub(super) fn autoreload_secs(mode: crate::config::AnalysisMode) -> Option<u64> {
    mode.intraday_minutes().map(|m| u64::from(m) * 60)
}

pub(super) fn next_autoreload_deadline(
    mode: crate::config::AnalysisMode,
) -> Option<tokio::time::Instant> {
    autoreload_secs(mode).map(|s| tokio::time::Instant::now() + Duration::from_secs(s))
}

/// The localized "identity + latest price" segment of the market snapshot line.
/// **SINGLE SOURCE OF TRUTH**: both the CLI `/autoreload` notice and the Web/API
/// `market_line` render identity + price through this one function, so the field
/// labels and the price format never diverge between the terminal and the dashboard.
/// `label` is the resolved `TICKER (Name)` form; language-aware (ja/en).
pub(crate) fn market_identity_price(label: &str, price: f64, lang: &str) -> String {
    match lang {
        "ja" => {
            let label_ja = label.replacen(" (", "（", 1).replace(')', "）");
            format!("銘柄: {}  最新取得価格: {:.2}", label_ja, price)
        }
        _ => format!("Symbol: {}  Latest: {:.2}", label, price),
    }
}

/// Builds the `/autoreload notice` lines (timestamp, ticker, latest price, diff
/// vs the previous reload). Shared by the timer-driven reload so the lines can be
/// emitted via the external printer instead of `println!`.
pub(super) fn autoreload_notice_lines(
    session: &ChatSession,
    new_prices: &[Option<f64>],
    prev_prices: &[Option<f64>],
    lang: &str,
) -> Vec<String> {
    let now = Local::now().format("%Y/%m/%d %H:%M:%S").to_string();
    let mut out = Vec::new();
    for (i, new_price_opt) in new_prices.iter().enumerate() {
        if let Some(&new_price) = new_price_opt.as_ref() {
            let label = session.ticker_labels().get(i).cloned().unwrap_or_default();
            let prev_opt = prev_prices.get(i).copied().flatten();
            let (diff, percent) = match prev_opt {
                Some(prev) if prev != 0.0 => {
                    let d = new_price - prev;
                    (d, d / prev * 100.0)
                }
                _ => (0.0, 0.0),
            };
            let diff_str = crate::render::colored_price_diff(diff, percent).to_string();
            let identity = market_identity_price(&label, new_price, lang);
            match lang {
                "ja" => out.push(format!("{}  {}　前回比: {}", now, identity, diff_str)),
                _ => out.push(format!(
                    "{}  {}  vs prev reload: {}",
                    now, identity, diff_str
                )),
            }
            // Enabled-indicator readings on a second, indented line.
            if let Some(summary) = session.last_indicator_summaries().get(i) {
                if !summary.is_empty() {
                    match lang {
                        "ja" => out.push(format!("  指標: {}", summary)),
                        _ => out.push(format!("  Indicators: {}", summary)),
                    }
                }
            }
        }
    }
    out
}

/// Compact reading of the enabled indicators for the autoreload notice. Reads
/// computed values directly from the guard (SOT); the always-on RSI/MACD basics
/// plus each enabled extension. Option-valued indicators are skipped when absent.
pub(crate) fn notice_indicator_summary(
    guard: &crate::technical::TechnicalDataGuard,
    config: &Config,
    lang: &str,
) -> String {
    use crate::config::ExtensionIndicator;
    let mut parts: Vec<String> = Vec::new();
    // Volume leads the snapshot (Price | Vol | indicators), using the same
    // comma-formatted value the CLI shows. Defined here (shared) so the CLI and
    // Web market lines stay identical (SOT) — never added server-side only.
    if let Some(vol) = guard.get_latest_volume() {
        parts.push(format!("Vol={}", crate::utils::format_volume_display(vol)));
    }
    parts.push(format!("RSI={:.1}", guard.get_rsi()));
    parts.push(format!("MACD={:.2}", guard.get_macd()));
    for ext in config.analysis_extensions() {
        match ext {
            ExtensionIndicator::Ema => {
                let (s, l) = (guard.get_ema_short(), guard.get_ema_long());
                parts.push(format!(
                    "EMA={:.1}{}{:.1}",
                    s,
                    if s >= l { ">" } else { "<" },
                    l
                ));
            }
            ExtensionIndicator::Sma => {
                let (s, l) = (guard.get_sma_short(), guard.get_sma_long());
                parts.push(format!(
                    "SMA={:.1}{}{:.1}",
                    s,
                    if s >= l { ">" } else { "<" },
                    l
                ));
            }
            ExtensionIndicator::Bollinger => {
                parts.push(format!("%B={:.2}", guard.get_bb_percent_b()));
            }
            ExtensionIndicator::Roc => {
                if let Some(v) = guard.get_roc() {
                    parts.push(format!("ROC={:.1}%", v));
                }
            }
            ExtensionIndicator::Adx => {
                if let Some(v) = guard.get_adx() {
                    parts.push(format!("ADX={:.1}", v));
                }
            }
            ExtensionIndicator::Stochastics => {
                if let (Some(k), Some(d)) = (guard.get_stochastics_k(), guard.get_stochastics_d()) {
                    parts.push(format!("Stoch={:.0}/{:.0}", k, d));
                }
            }
            ExtensionIndicator::Fibonacci => {
                if let Some(v) = guard.get_fibo_50_0() {
                    parts.push(format!("Fib50={:.1}", v));
                }
            }
            ExtensionIndicator::Vwap => {
                if let Some(v) = guard.get_vwap() {
                    parts.push(format!("VWAP={:.1}", v));
                }
            }
            ExtensionIndicator::Ichimoku => {
                if let (Some(t), Some(k)) = (guard.get_tenkan_sen(), guard.get_kijun_sen()) {
                    let label = if lang == "ja" { "一目" } else { "Ichimoku" };
                    parts.push(format!("{}={:.1}/{:.1}", label, t, k));
                }
            }
        }
    }
    parts.join(" / ")
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn reload_chat_tickers(
    session: &mut ChatSession,
    chat_config: &Config,
    ticker_name_map: &HashMap<String, String>,
    reload_news: bool,
    notify: bool,
    lang: &str,
    out: &mut ChatOut,
    fetch_fundamental: bool,
) -> Vec<Option<f64>> {
    let is_empty = session.ticker_labels().len() == 1 && session.ticker_labels()[0].is_empty();
    if is_empty {
        return vec![];
    }

    let n = session.ticker_labels().len();
    if notify {
        out.line(match (lang, reload_news && !chat_config.no_news) {
            ("ja", true) => format!(
                "⏳ {} 銘柄のテクニカル指標・ニュースを再取得しています...",
                n
            ),
            ("ja", false) => format!("⏳ {} 銘柄のテクニカル指標を再取得しています...", n),
            (_, true) => format!(
                "⏳ Reloading technical data and news for {} ticker(s)...",
                n
            ),
            _ => format!("⏳ Reloading technical data for {} ticker(s)...", n),
        });
    }

    let mut new_prices: Vec<Option<f64>> = Vec::with_capacity(n);
    for i in 0..n {
        let label = session.ticker_labels()[i].clone();
        let symbol = symbol_of(&label);
        let display = short_display_label(&label).to_string();
        match load_ticker_analysis(
            chat_config,
            &symbol,
            ticker_name_map,
            reload_news,
            fetch_fundamental,
        )
        .await
        {
            Err(e) => {
                if notify {
                    out.err(match lang {
                        "ja" => format!("⚠️ {} の再取得に失敗しました: {}", display, e),
                        _ => format!("⚠️ Failed to reload {}: {}", display, e),
                    });
                }
                new_prices.push(None);
            }
            Ok(ctx) => {
                let news_fetched = ctx.news_items.is_some();
                let latest_price = ctx.latest_price;
                // Whether this load went for the fundamentals decides, in one
                // place, what happens to the display, the prompt context and the
                // confirmed data alike. A failed fetch is a fetch: the block is
                // empty and all three drop it together.
                let fundamental = ctx.fundamental_fetched.then_some(ctx.fundamental_text);
                apply_ticker_reload(
                    session,
                    i,
                    TickerReloadData {
                        base_context: ctx.context,
                        technical_text: ctx.technical_text,
                        indicator_summary: ctx.indicator_summary,
                        news: ctx.news_items,
                        data_as_of: ctx.data_as_of,
                        facts: Some(ctx.facts),
                        fundamental,
                    },
                );
                if notify {
                    out.line(match (lang, news_fetched) {
                        ("ja", true) => {
                            format!("✅ {} のテクニカル指標とニュースを更新しました。", display)
                        }
                        ("ja", false) => format!("✅ {} のテクニカル指標を更新しました。", display),
                        (_, true) => format!("✅ Technical data and news updated for {}.", display),
                        _ => format!("✅ Technical data updated for {}.", display),
                    });
                }
                new_prices.push(Some(latest_price));
            }
        }
    }
    new_prices
}
