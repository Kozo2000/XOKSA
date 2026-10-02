//! Fundamental data acquisition and display (--fundamental option)
//!
//! Japan: J-Quants API  /  US: SEC EDGAR
//! All numeric values are computed in Rust; LLM receives pre-formatted text only.

use crate::config::Config;
use anyhow::{anyhow, bail, Result};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use zeroize::Zeroizing;

// ─── Business rule constants ─────────────────────────────────────────────────
const US_TICKER_MAX_LEN: usize = 5;
const APPROX_DAYS_PER_YEAR: i32 = 365;
const APPROX_DAYS_PER_MONTH: i32 = 30;
const SINGLE_YEAR_MAX_DAYS: i32 = 400;

// ─── Data structures ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum FundamentalMarket {
    Japan,
    Us,
    Unsupported(String),
}

/// Values as they come off a provider, before validation. This is the *in-flight*
/// form, deliberately a different type from `FundamentalData`, so data that has
/// not been through the checks cannot reach display, logs or an LLM prompt.
/// Derived ratios are absent here on purpose: they are computed once, inside
/// `FundamentalData`, from validated inputs.
#[derive(Debug, Clone, Default)]
pub struct FundamentalInputs {
    pub market: String,
    pub data_source: String,
    pub currency: String,
    pub amount_unit: String,
    pub fiscal_period: Option<String>,
    pub reported_date: Option<String>,
    pub revenue: Option<f64>,
    pub operating_income: Option<f64>,
    pub net_income: Option<f64>,
    pub eps: Option<f64>,
    pub bps: Option<f64>,
    pub equity: Option<f64>,
    pub shares_outstanding: Option<f64>,
    pub dividend: Option<f64>,
    pub dividend_is_forecast: bool,
    pub trading_unit: Option<u64>,
    pub next_fiscal_year_end: Option<String>,
}

/// Confirmed fundamental data.
///
/// Fields are private: the only way to obtain one is `FundamentalData::build`,
/// which validates every input and computes the derived ratios itself. Display,
/// logging and the LLM prompt read the same values through the getters below, so
/// no path can show a number the checks never saw.
///
/// A legitimate zero and a meaningful negative (a loss, a negative EPS) are kept.
/// What is refused is a value that is not a number at all - NaN, infinity - and a
/// ratio whose denominator leaves it undefined.
#[derive(Debug, Clone, Default)]
pub struct FundamentalData {
    market: String,
    data_source: String,
    currency: String,
    amount_unit: String,
    fiscal_period: Option<String>,
    reported_date: Option<String>,
    revenue: Option<f64>,
    operating_income: Option<f64>,
    net_income: Option<f64>,
    eps: Option<f64>,
    bps: Option<f64>,
    equity: Option<f64>,
    shares_outstanding: Option<f64>,
    /// Price the ratios were derived from, kept with them so a ratio can never be
    /// shown beside a price it did not come from.
    latest_price: Option<f64>,
    per: Option<f64>,
    pbr: Option<f64>,
    roe: Option<f64>,
    dividend: Option<f64>,
    dividend_is_forecast: bool,
    trading_unit: Option<u64>,
    next_fiscal_year_end: Option<String>,
}

/// A reported figure is usable only when it is a real number. Missing stays
/// missing; NaN and infinity are treated as missing rather than displayed.
fn finite(value: Option<f64>) -> Option<f64> {
    value.filter(|v| v.is_finite())
}

/// `numerator / denominator`, defined only when the result is a real number.
///
/// Refuses a denominator that is zero, and one so small it is indistinguishable
/// from zero at `f64` precision. Rejecting only an exact zero is not enough: a
/// denominator of `1e-300` yields a finite but astronomically large ratio, which
/// would then be stored as confirmed data, displayed, and sent to an LLM as a
/// fact. Nothing a provider legitimately reports — an EPS, a BPS, equity —
/// lands below `f64::EPSILON`.
fn safe_ratio(numerator: Option<f64>, denominator: Option<f64>) -> Option<f64> {
    match (finite(numerator), finite(denominator)) {
        (Some(n), Some(d)) if d.abs() > f64::EPSILON => {
            let r = n / d;
            r.is_finite().then_some(r)
        }
        _ => None,
    }
}

impl FundamentalData {
    /// The one validated construction path. `latest_price` is the price the
    /// ratios are derived from; `None` leaves PER/PBR undefined rather than
    /// guessing a price.
    pub fn build(inputs: FundamentalInputs, latest_price: Option<f64>) -> Self {
        let mut data = FundamentalData {
            market: inputs.market,
            data_source: inputs.data_source,
            currency: inputs.currency,
            amount_unit: inputs.amount_unit,
            fiscal_period: inputs.fiscal_period,
            reported_date: inputs.reported_date,
            revenue: finite(inputs.revenue),
            operating_income: finite(inputs.operating_income),
            net_income: finite(inputs.net_income),
            eps: finite(inputs.eps),
            bps: finite(inputs.bps),
            equity: finite(inputs.equity),
            shares_outstanding: finite(inputs.shares_outstanding),
            latest_price: None,
            per: None,
            pbr: None,
            roe: None,
            dividend: finite(inputs.dividend),
            dividend_is_forecast: inputs.dividend_is_forecast,
            trading_unit: inputs.trading_unit,
            next_fiscal_year_end: inputs.next_fiscal_year_end,
        };
        data.recompute_derived(latest_price);
        data
    }

    /// Recompute every derived ratio from the current inputs. Run on build and on
    /// any input change, so PER/PBR/ROE cannot survive as stale numbers beside
    /// inputs that have moved on.
    pub fn recompute_derived(&mut self, latest_price: Option<f64>) {
        self.latest_price = finite(latest_price);
        self.per = safe_ratio(self.latest_price, self.eps);
        self.pbr = safe_ratio(self.latest_price, self.bps);
        self.roe = safe_ratio(self.net_income, self.equity);
    }

    /// Operating margin as a percentage. Derived here, once, so no display path
    /// computes it separately - and so an overflow (a huge income over a tiny
    /// revenue) yields no value instead of `inf%` on screen.
    pub fn operating_margin_pct(&self) -> Option<f64> {
        safe_ratio(self.operating_income, self.revenue).and_then(|r| {
            let pct = r * 100.0;
            pct.is_finite().then_some(pct)
        })
    }

    /// ROE as a percentage. The x100 conversion is validated too: a ratio that is
    /// finite can still overflow when scaled.
    pub fn roe_pct(&self) -> Option<f64> {
        self.roe.and_then(|r| {
            let pct = r * 100.0;
            pct.is_finite().then_some(pct)
        })
    }

    /// Dividend for one trading unit. Multiplying by the lot size can overflow,
    /// so the product is checked like any other derived value.
    pub fn lot_dividend(&self) -> Option<f64> {
        let unit = self.trading_unit.filter(|&u| u > 1)? as f64;
        let v = self.dividend? * unit;
        v.is_finite().then_some(v)
    }

    /// Replace a reported input and refresh the ratios that depend on it.
    pub fn set_bps(&mut self, bps: Option<f64>) {
        self.bps = finite(bps);
        let price = self.latest_price;
        self.recompute_derived(price);
    }

    // Read-only access. Display, logs and the LLM prompt all use these.
    pub fn market(&self) -> &str {
        &self.market
    }
    pub fn data_source(&self) -> &str {
        &self.data_source
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub fn amount_unit(&self) -> &str {
        &self.amount_unit
    }
    pub fn fiscal_period(&self) -> Option<&str> {
        self.fiscal_period.as_deref()
    }
    pub fn reported_date(&self) -> Option<&str> {
        self.reported_date.as_deref()
    }
    pub fn revenue(&self) -> Option<f64> {
        self.revenue
    }
    pub fn operating_income(&self) -> Option<f64> {
        self.operating_income
    }
    pub fn net_income(&self) -> Option<f64> {
        self.net_income
    }
    pub fn eps(&self) -> Option<f64> {
        self.eps
    }
    pub fn bps(&self) -> Option<f64> {
        self.bps
    }
    pub fn equity(&self) -> Option<f64> {
        self.equity
    }
    pub fn shares_outstanding(&self) -> Option<f64> {
        self.shares_outstanding
    }
    pub fn latest_price(&self) -> Option<f64> {
        self.latest_price
    }
    pub fn per(&self) -> Option<f64> {
        self.per
    }
    pub fn pbr(&self) -> Option<f64> {
        self.pbr
    }
    pub fn roe(&self) -> Option<f64> {
        self.roe
    }
    pub fn dividend(&self) -> Option<f64> {
        self.dividend
    }
    pub fn dividend_is_forecast(&self) -> bool {
        self.dividend_is_forecast
    }
    pub fn trading_unit(&self) -> Option<u64> {
        self.trading_unit
    }
    pub fn next_fiscal_year_end(&self) -> Option<&str> {
        self.next_fiscal_year_end.as_deref()
    }
}

// ─── Market detection ─────────────────────────────────────────────────────────

pub fn detect_market(ticker: &str) -> FundamentalMarket {
    if ticker.ends_with(".T") {
        return FundamentalMarket::Japan;
    }
    if crate::bootstrap::jp_code_from_ticker(ticker).is_some() {
        return FundamentalMarket::Japan;
    }
    // US ticker: alphabetic-only or common US formats (e.g. BRK.B, BF.B, BRK-B)
    let core = ticker.split('.').next().unwrap_or(ticker);
    if core.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && core.len() <= US_TICKER_MAX_LEN
        && !core.is_empty()
    {
        return FundamentalMarket::Us;
    }
    FundamentalMarket::Unsupported(ticker.to_string())
}

// ─── Entry point ─────────────────────────────────────────────────────────────

pub async fn fetch_fundamental_data(
    ticker: &str,
    config: &Config,
    latest_price: Option<f64>,
) -> Result<FundamentalData> {
    match detect_market(ticker) {
        FundamentalMarket::Japan => {
            let jp_code = crate::bootstrap::jp_code_from_ticker(ticker)
                .unwrap_or_else(|| ticker.trim_end_matches(".T").to_string());
            fetch_jquants(&jp_code, config, latest_price).await
        }
        FundamentalMarket::Us => fetch_sec_edgar(ticker, config, latest_price).await,
        FundamentalMarket::Unsupported(t) => bail!(
            "❌ Fundamental data is currently supported for Japan (.T) and US stocks only. (ticker: {})",
            t
        ),
    }
}

// ─── Formatting helpers ───────────────────────────────────────────────────────

fn fmt_opt(v: Option<f64>, decimal: usize) -> String {
    v.map(|x| format!("{:.prec$}", x, prec = decimal))
        .unwrap_or_else(|| "N/A".to_string())
}

fn fmt_with_commas(x: f64) -> String {
    // Round to the nearest integer, then group (SOT §4.2 — shared with utils).
    crate::utils::group_thousands(x.round() as i64)
}

fn currency_symbol(data: &FundamentalData) -> &'static str {
    match data.currency.as_str() {
        s if s.starts_with("USD") => "$",
        s if s.starts_with("JPY") => "¥",
        _ => "",
    }
}

fn large_unit(data: &FundamentalData) -> &str {
    &data.amount_unit
}

fn display_market<'a>(market: &'a str, lang: &str) -> &'a str {
    match (lang, market) {
        ("ja", "Japan") => "日本株",
        ("ja", "US") => "米国株",
        _ => market,
    }
}

fn display_currency<'a>(currency: &'a str, lang: &str) -> &'a str {
    match (lang, currency) {
        ("ja", "JPY") => "JPY (円)",
        ("ja", "JPY (millions)") => "JPY (百万円)",
        _ => currency,
    }
}

fn display_amount_unit<'a>(unit: &'a str, lang: &str) -> &'a str {
    match (lang, unit) {
        ("ja", "JPY") => "円",
        ("ja", "JPY millions") => "百万円",
        _ => unit,
    }
}

fn per_share_unit(data: &FundamentalData, lang: &str) -> &'static str {
    match (lang, data.currency.starts_with("JPY")) {
        ("ja", true) => "円/株",
        ("ja", false) => "$/株",
        (_, true) => "¥/share",
        _ => "$/share",
    }
}

fn fmt_currency_auto(v: Option<f64>, sym: &str) -> String {
    match v {
        None => "N/A".to_string(),
        Some(x) if x.abs() >= 100.0 => format!("{}{}", sym, fmt_with_commas(x)),
        Some(x) => format!("{}{:.2}", sym, x),
    }
}

fn fmt_currency_large(v: Option<f64>, sym: &str) -> String {
    v.map(|x| format!("{}{}", sym, fmt_with_commas(x)))
        .unwrap_or_else(|| "N/A".to_string())
}

fn fmt_currency(v: Option<f64>, sym: &str, decimal: usize) -> String {
    v.map(|x| format!("{}{:.prec$}", sym, x, prec = decimal))
        .unwrap_or_else(|| "N/A".to_string())
}

// ─── Display ─────────────────────────────────────────────────────────────────

struct FundamentalLabels {
    header_display: &'static str,
    header_llm: &'static str,
    market: &'static str,
    data_source: &'static str,
    source_short: &'static str,
    currency: &'static str,
    fiscal_period: &'static str,
    reported_date: &'static str,
    revenue: &'static str,
    operating_income: &'static str,
    operating_margin: &'static str,
    net_income: &'static str,
    dividend_per_share: &'static str,
    min_trading_unit: &'static str,
    shares_suffix: &'static str,
    dividend_per_lot: &'static str,
    next_fy_end: &'static str,
}

fn fundamental_labels(lang: &str) -> FundamentalLabels {
    match lang {
        "ja" => FundamentalLabels {
            header_display: "---ファンダメンタル補助情報---",
            header_llm: "【ファンダメンタル補助情報】",
            market: "対象市場",
            data_source: "データ取得元",
            source_short: "データ取得元",
            currency: "通貨単位",
            fiscal_period: "決算期間",
            reported_date: "報告日",
            revenue: "売上高",
            operating_income: "営業利益",
            operating_margin: "営業利益率",
            net_income: "純利益",
            dividend_per_share: "配当（1株あたり）",
            min_trading_unit: "最少単位株",
            shares_suffix: "株",
            dividend_per_lot: "配当（1単元あたり）",
            next_fy_end: "次期会計年度末",
        },
        _ => FundamentalLabels {
            header_display: "--- Fundamental Data ---",
            header_llm: "[Fundamental Data]",
            market: "Market",
            data_source: "Data Source",
            source_short: "Source",
            currency: "Currency",
            fiscal_period: "Fiscal Period",
            reported_date: "Reported Date",
            revenue: "Revenue",
            operating_income: "Operating Income",
            operating_margin: "Operating Margin",
            net_income: "Net Income",
            dividend_per_share: "Dividend (per share)",
            min_trading_unit: "Min. Trading Unit",
            shares_suffix: " shares",
            dividend_per_lot: "Dividend (per lot)",
            next_fy_end: "Next FY End",
        },
    }
}

struct FundamentalComputedValues {
    market: String,
    data_source: String,
    currency: String,
    fiscal_period: String,
    reported_date: String,
    revenue: String,
    revenue_unit: String,
    operating_income: String,
    operating_income_unit: String,
    operating_margin: Option<String>,
    net_income: String,
    net_income_unit: String,
    eps: String,
    eps_unit: String,
    bps: String,
    bps_unit: String,
    per: String,
    pbr: String,
    roe: String,
    dividend: String,
    dividend_suffix: String,
    trading_unit: Option<u32>,
    lot_dividend: Option<String>,
    next_fy_end: String,
}

fn fundamental_computed_values(data: &FundamentalData, lang: &str) -> FundamentalComputedValues {
    let sym = currency_symbol(data);
    let lu = display_amount_unit(large_unit(data), lang);
    let psu = per_share_unit(data, lang);
    let (unit_open, unit_close) = match lang {
        "ja" => ("（", "）"),
        _ => (" (", ")"),
    };
    let forecast_sfx = match lang {
        "ja" => "（予想）",
        _ => " (forecast)",
    };
    let unit_sfx = |v: Option<f64>| {
        v.map(|_| format!("{}{}{}", unit_open, lu, unit_close))
            .unwrap_or_default()
    };
    let ps_sfx = |v: Option<f64>| {
        v.map(|_| format!("{}{}{}", unit_open, psu, unit_close))
            .unwrap_or_default()
    };
    // Derived values are read from the confirmed data, never recomputed here:
    // a second formula in a display path is a second place for them to disagree,
    // and it bypasses the finiteness checks.
    let operating_margin = data
        .operating_margin_pct()
        .map(|pct| format!("{:.1}%", pct));
    let lot_dividend = data.lot_dividend().map(|v| fmt_currency_auto(Some(v), sym));
    FundamentalComputedValues {
        market: display_market(&data.market, lang).to_string(),
        data_source: data.data_source.clone(),
        currency: display_currency(&data.currency, lang).to_string(),
        fiscal_period: data.fiscal_period.as_deref().unwrap_or("N/A").to_string(),
        reported_date: data.reported_date.as_deref().unwrap_or("N/A").to_string(),
        revenue: fmt_currency_large(data.revenue, sym),
        revenue_unit: unit_sfx(data.revenue),
        operating_income: fmt_currency_large(data.operating_income, sym),
        operating_income_unit: unit_sfx(data.operating_income),
        operating_margin,
        net_income: fmt_currency_large(data.net_income, sym),
        net_income_unit: unit_sfx(data.net_income),
        eps: fmt_currency(data.eps, sym, 2),
        eps_unit: ps_sfx(data.eps),
        bps: fmt_currency(data.bps, sym, 2),
        bps_unit: ps_sfx(data.bps),
        per: fmt_opt(data.per, 2),
        pbr: fmt_opt(data.pbr, 2),
        roe: data
            .roe_pct()
            .map(|pct| format!("{:.2}%", pct))
            .unwrap_or_else(|| "N/A".to_string()),
        dividend: fmt_currency_auto(data.dividend, sym),
        dividend_suffix: if data.dividend_is_forecast && data.dividend.is_some() {
            forecast_sfx.to_string()
        } else {
            String::new()
        },
        trading_unit: data.trading_unit.map(|u| u as u32),
        lot_dividend,
        next_fy_end: data
            .next_fiscal_year_end
            .as_deref()
            .unwrap_or("N/A")
            .to_string(),
    }
}

/// The revenue→next-fiscal-year-end value lines, identical in order and content for
/// the terminal display and the LLM prompt — they differ only by an emoji prefix
/// (SOT §4.2, one body instead of two parallel copies). `with_emoji` selects the
/// terminal form; the header block (grouped vs per-line) stays in each caller.
fn fundamental_value_lines(
    cv: &FundamentalComputedValues,
    l: &FundamentalLabels,
    with_emoji: bool,
) -> Vec<String> {
    let e = |emoji: &'static str| if with_emoji { emoji } else { "" };
    let mut lines = Vec::new();
    lines.push(format!(
        "{}{}: {}{}",
        e("💰 "),
        l.revenue,
        cv.revenue,
        cv.revenue_unit
    ));
    lines.push(format!(
        "{}{}: {}{}",
        e("📊 "),
        l.operating_income,
        cv.operating_income,
        cv.operating_income_unit
    ));
    if let Some(m) = &cv.operating_margin {
        lines.push(format!("{}{}: {}", e("📉 "), l.operating_margin, m));
    }
    lines.push(format!(
        "{}{}: {}{}",
        e("💹 "),
        l.net_income,
        cv.net_income,
        cv.net_income_unit
    ));
    lines.push(format!("{}EPS: {}{}", e("📈 "), cv.eps, cv.eps_unit));
    lines.push(format!("{}BPS: {}{}", e("📚 "), cv.bps, cv.bps_unit));
    lines.push(format!("{}PER: {}", e("🔍 "), cv.per));
    lines.push(format!("{}PBR: {}", e("📐 "), cv.pbr));
    lines.push(format!("{}ROE: {}", e("♻️  "), cv.roe));
    lines.push(format!(
        "{}{}: {}{}",
        e("💸 "),
        l.dividend_per_share,
        cv.dividend,
        cv.dividend_suffix
    ));
    if let Some(unit) = cv.trading_unit {
        lines.push(format!(
            "{}{}: {}{}",
            e("📦 "),
            l.min_trading_unit,
            unit,
            l.shares_suffix
        ));
        if let Some(lot) = &cv.lot_dividend {
            lines.push(format!("{}{}: {}", e("💸 "), l.dividend_per_lot, lot));
        }
    }
    lines.push(format!(
        "{}{}: {}",
        e("🗓️  "),
        l.next_fy_end,
        cv.next_fy_end
    ));
    lines
}

pub fn render_fundamental_display(data: &FundamentalData, lang: &str) -> Vec<String> {
    let cv = fundamental_computed_values(data, lang);
    let l = fundamental_labels(lang);
    let mut lines = Vec::new();
    lines.push(l.header_display.to_string());
    lines.push(format!("🌏 {}: {}", l.market, cv.market));
    lines.push(format!("🔗 {}: {}", l.data_source, cv.data_source));
    lines.push(format!("💱 {}: {}", l.currency, cv.currency));
    lines.push(format!("📋 {}: {}", l.fiscal_period, cv.fiscal_period));
    lines.push(format!("📅 {}: {}", l.reported_date, cv.reported_date));
    lines.extend(fundamental_value_lines(&cv, &l, true));
    lines
}

pub fn format_fundamental_for_llm(data: &FundamentalData, lang: &str) -> Vec<String> {
    let cv = fundamental_computed_values(data, lang);
    let l = fundamental_labels(lang);
    let mut lines = Vec::new();
    lines.push(l.header_llm.to_string());
    lines.push(format!(
        "{}: {} / {}: {} / {}: {}",
        l.market, cv.market, l.source_short, cv.data_source, l.currency, cv.currency
    ));
    lines.push(format!(
        "{}: {} / {}: {}",
        l.fiscal_period, cv.fiscal_period, l.reported_date, cv.reported_date
    ));
    lines.extend(fundamental_value_lines(&cv, &l, false));
    lines
}

// ─── J-Quants ────────────────────────────────────────────────────────────────

/// The J-Quants v2 bearer key (`JQUANTS_API_KEY`). The V1 token flow (id/refresh
/// token, email+password → `auth_user`/`auth_refresh`) was removed when J-Quants
/// discontinued the V1 API on 2026-06-01; every account now uses a v2 API key.
async fn jquants_bearer_token() -> Result<Zeroizing<String>> {
    crate::utils::resolve_api_key("JQUANTS_API_KEY")?
        .ok_or_else(|| anyhow!("❌ J-Quants config missing: JQUANTS_API_KEY not set"))
}

async fn fetch_jquants(
    jp_code: &str,
    config: &Config,
    latest_price: Option<f64>,
) -> Result<FundamentalData> {
    let bearer_token = jquants_bearer_token().await?;
    let client = build_client(config.https_proxy.as_deref(), config.no_proxy.as_deref())?;

    let url = format!(
        "https://api.jquants.com/v2/fins/summary?code={}",
        urlencoding::encode(jp_code)
    );
    let resp = client
        .get(&url)
        .header("x-api-key", &**bearer_token)
        .send()
        .await
        .map_err(|e| anyhow!("❌ J-Quants financial data retrieval failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!(
            "❌ J-Quants financial data retrieval failed: HTTP {} / {}",
            status,
            body
        );
    }
    let json: Value = resp.json().await?;
    let records = json["data"]
        .as_array()
        .ok_or_else(|| anyhow!("❌ J-Quants: 'data' field not found in response"))?;
    if records.is_empty() {
        bail!("❌ J-Quants: no financial data found (code: {})", jp_code);
    }
    let s = &records[records.len() - 1];

    let parse_f64 = |key: &str| -> Option<f64> {
        s[key]
            .as_f64()
            .or_else(|| s[key].as_str().and_then(|v| v.parse::<f64>().ok()))
            .filter(|v| v.is_finite())
    };

    let eps = parse_f64("EPS");
    let bps = parse_f64("BPS");
    let net_income = parse_f64("NP");
    let equity = parse_f64("Eq");
    let shares_outstanding = parse_f64("ShOutFY");

    // PER/PBR/ROE are not computed here. They belong to the confirmed data and
    // are derived once, in `FundamentalData::build`, from validated inputs - so
    // the two providers cannot drift apart and a ratio cannot go stale.
    let fiscal_period = {
        let period_type = s["CurPerType"].as_str().unwrap_or("");
        let fy_end = s["CurFYEn"].as_str().unwrap_or("");
        if !period_type.is_empty() && !fy_end.is_empty() {
            Some(format!("{} (FY ends {})", period_type, fy_end))
        } else {
            None
        }
    };

    let trading_unit = (async {
        let resp = client
            .get(format!(
                "https://api.jquants.com/v2/equities/master?code={}",
                urlencoding::encode(jp_code)
            ))
            .header("x-api-key", &**bearer_token)
            .send()
            .await?;
        anyhow::ensure!(resp.status().is_success(), "HTTP {}", resp.status());
        let json: Value = resp.json().await?;
        Ok::<_, anyhow::Error>(
            json["data"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|info| info["TradingUnit"].as_u64()),
        )
    })
    .await
    .ok()
    .flatten();

    let div_actual = parse_f64("DivAnn");
    Ok(FundamentalData::build(
        FundamentalInputs {
            market: "Japan".to_string(),
            data_source: "J-Quants (v2)".to_string(),
            currency: "JPY".to_string(),
            amount_unit: "JPY".to_string(),
            fiscal_period,
            reported_date: s["DiscDate"].as_str().map(|s| s.to_string()),
            revenue: parse_f64("Sales"),
            operating_income: parse_f64("OP"),
            net_income,
            eps,
            bps,
            equity,
            shares_outstanding,
            dividend: div_actual
                .or_else(|| parse_f64("FDivAnn"))
                .or_else(|| parse_f64("NxFDivAnn")),
            dividend_is_forecast: div_actual.is_none(),
            trading_unit,
            next_fiscal_year_end: s["NxtFYEn"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
        },
        latest_price,
    ))
}

// ─── SEC EDGAR ────────────────────────────────────────────────────────────────

async fn fetch_sec_edgar(
    ticker: &str,
    config: &Config,
    latest_price: Option<f64>,
) -> Result<FundamentalData> {
    let user_agent = config
        .sec_user_agent
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            anyhow!("❌ SEC config missing: SEC_USER_AGENT not set (add to xoksa.env)")
        })?;

    let mut builder = Client::builder()
        .user_agent(user_agent)
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(url) = config.https_proxy.as_deref() {
        builder = builder.proxy(crate::utils::build_proxy(url, config.no_proxy.as_deref())?);
    }
    let client = builder.build()?;

    // Step 1: ticker → CIK
    let ticker_upper = ticker.to_ascii_uppercase();
    // Class shares (e.g. BRK.B) may be registered in SEC EDGAR as BRK-B or BRKB; try all variants
    let ticker_variants: Vec<String> = {
        let mut v = vec![ticker_upper.clone()];
        if ticker_upper.contains('.') {
            v.push(ticker_upper.replace('.', "-"));
            v.push(ticker_upper.replace('.', ""));
        }
        v
    };
    let tickers_json: Value = client
        .get("https://www.sec.gov/files/company_tickers.json")
        .send()
        .await
        .map_err(|e| anyhow!("❌ SEC EDGAR: failed to fetch company_tickers.json: {e}"))?
        .error_for_status()
        .map_err(|e| anyhow!("❌ SEC EDGAR: HTTP error fetching company_tickers.json: {e}"))?
        .json()
        .await
        .map_err(|e| anyhow!("❌ SEC EDGAR: failed to parse company_tickers.json: {e}"))?;

    let cik_num = tickers_json
        .as_object()
        .and_then(|map| {
            map.values().find(|entry| {
                entry["ticker"]
                    .as_str()
                    .map(|t| ticker_variants.iter().any(|v| v.eq_ignore_ascii_case(t)))
                    .unwrap_or(false)
            })
        })
        .and_then(|entry| entry["cik_str"].as_u64())
        .ok_or_else(|| anyhow!("❌ SEC EDGAR: CIK not found for ticker '{}'", ticker))?;

    let cik_padded = format!("{:010}", cik_num);

    // Step 2: fetch companyfacts
    let facts_url = format!(
        "https://data.sec.gov/api/xbrl/companyfacts/CIK{}.json",
        cik_padded
    );
    let facts: Value = client
        .get(&facts_url)
        .send()
        .await
        .map_err(|e| anyhow!("❌ SEC EDGAR: failed to fetch companyfacts: {e}"))?
        .error_for_status()
        .map_err(|e| anyhow!("❌ SEC EDGAR: HTTP error fetching companyfacts: {e}"))?
        .json()
        .await
        .map_err(|e| anyhow!("❌ SEC EDGAR: failed to parse companyfacts response: {e}"))?;

    let us_gaap = &facts["facts"]["us-gaap"];
    if us_gaap.is_null() {
        bail!("❌ SEC EDGAR: us-gaap data not found (CIK: {})", cik_padded);
    }

    // Step 3: determine the anchor reporting-period end date (shared by all flow indicators).
    // Adopt the most recent disclosed period (quarter or full year), not FY only, so the panel
    // reflects the latest 10-Q/10-K. Try NetIncomeLoss then Revenues in order; capture the
    // period type (fp) too, to label the period and to select the matching flow duration.
    let (target_period_end, target_period_fp): (Option<String>, Option<String>) = {
        let anchors: &[(&str, &str)] = &[
            ("NetIncomeLoss", "USD"),
            ("Revenues", "USD"),
            ("RevenueFromContractWithCustomerExcludingAssessedTax", "USD"),
        ];
        let mut result = (None, None);
        for (name, unit) in anchors {
            if let Some(arr) = us_gaap[name]["units"][*unit].as_array() {
                if let Some(e) = pick_best_xbrl(arr, false) {
                    result = (
                        e["end"].as_str().map(|s| s.to_string()),
                        e["fp"].as_str().map(|s| s.to_string()),
                    );
                    break;
                }
            }
        }
        result
    };

    // Flow indicators (income statement): restricted to the anchor reporting period only.
    // All items share the same target_end + period basis (quarter vs full year), which keeps
    // them consistent and prevents structural artifacts like "OI > Revenue" from period mixing.
    let get_flow = |metric_names: &[&str], unit: &str| -> Option<f64> {
        let end = target_period_end.as_deref()?;
        let fp = target_period_fp.as_deref();
        for name in metric_names {
            if let Some(arr) = us_gaap[*name]["units"][unit].as_array() {
                if let Some(e) = pick_xbrl_for_period(arr, end, fp) {
                    return e["val"].as_f64().filter(|v| v.is_finite());
                }
            }
        }
        None
    };

    let get_flow_filed = |metric_names: &[&str], unit: &str| -> Option<String> {
        let end = target_period_end.as_deref()?;
        let fp = target_period_fp.as_deref();
        for name in metric_names {
            if let Some(arr) = us_gaap[*name]["units"][unit].as_array() {
                if let Some(e) = pick_xbrl_for_period(arr, end, fp) {
                    return e["filed"].as_str().map(|s| s.to_string());
                }
            }
        }
        None
    };

    // Balance sheet indicators: fetched from the same period end date (aligns equity and shares periods)
    let get_balance_sheet = |metric_names: &[&str], unit: &str| -> Option<f64> {
        let end = target_period_end.as_deref()?;
        for name in metric_names {
            if let Some(arr) = us_gaap[*name]["units"][unit].as_array() {
                if let Some(e) = pick_balance_sheet_for_period(arr, end) {
                    return e["val"].as_f64().filter(|v| v.is_finite());
                }
            }
        }
        None
    };

    // Per-share dividend: only FY entries covering a single year (start-to-end ≤400 days)
    // Excludes multi-year cumulative entries that carry an FY label but span several years
    let get_latest_dividend_per_share = || -> Option<f64> {
        let candidates = [
            "CommonStockDividendsPerShareCashPaid",
            "CommonStockDividendsPerShareDeclared",
        ];
        for name in candidates {
            if let Some(arr) = us_gaap[name]["units"]["USD/shares"].as_array() {
                let mut filtered: Vec<&Value> = arr
                    .iter()
                    .filter(|e| {
                        e["form"]
                            .as_str()
                            .map(|f| f == "10-K" || f == "10-Q")
                            .unwrap_or(false)
                            && e["fp"].as_str().map(|f| f == "FY").unwrap_or(false)
                            && is_single_year_period(
                                e["start"].as_str().unwrap_or(""),
                                e["end"].as_str().unwrap_or(""),
                            )
                    })
                    .collect();
                filtered.sort_by(|a, b| {
                    a["end"]
                        .as_str()
                        .unwrap_or("")
                        .cmp(b["end"].as_str().unwrap_or(""))
                        .then_with(|| {
                            a["filed"]
                                .as_str()
                                .unwrap_or("")
                                .cmp(b["filed"].as_str().unwrap_or(""))
                        })
                });
                if let Some(e) = filtered.last() {
                    if let Some(v) = e["val"].as_f64().filter(|v| v.is_finite()) {
                        return Some(v);
                    }
                }
            }
        }
        None
    };

    // Flow items (income statement): all fetched from the same FY end date
    let revenue = get_flow(
        &[
            "Revenues",
            "RevenueFromContractWithCustomerExcludingAssessedTax",
            "SalesRevenueNet",
            "RevenuesNetOfInterestExpense",
        ],
        "USD",
    );
    let operating_income = get_flow(&["OperatingIncomeLoss"], "USD");
    let net_income = get_flow(&["NetIncomeLoss"], "USD");
    let eps = get_flow(
        &["EarningsPerShareBasic", "EarningsPerShareDiluted"],
        "USD/shares",
    );
    // Balance sheet (B/S): equity and shares fetched from the same FY end date
    let equity = get_balance_sheet(
        &[
            "StockholdersEquity",
            "StockholdersEquityIncludingPortionAttributableToNoncontrollingInterest",
        ],
        "USD",
    );
    // Shares outstanding: prefer the period-end balance-sheet value (us-gaap). Many filers,
    // however, report us-gaap:CommonStockSharesOutstanding only at fiscal-year ends, so a
    // quarterly period finds none there; fall back to the cover-page count
    // (dei:EntityCommonStockSharesOutstanding), which is disclosed on every 10-Q/10-K, so a
    // quarterly panel still yields BPS/PBR. Both are actual reported values (no fabrication).
    let shares_outstanding = get_balance_sheet(&["CommonStockSharesOutstanding"], "shares")
        .or_else(|| {
            facts["facts"]["dei"]["EntityCommonStockSharesOutstanding"]["units"]["shares"]
                .as_array()
                .and_then(|arr| pick_latest_instant(arr))
                .and_then(|e| e["val"].as_f64().filter(|v| v.is_finite()))
        });
    let dividend = get_latest_dividend_per_share();

    let bps = match (equity, shares_outstanding) {
        (Some(eq), Some(sh)) if sh > 0.0 => Some(eq / sh),
        _ => None,
    };
    // Derived ratios are computed once inside `FundamentalData::build`.
    let fiscal_period =
        fiscal_period_label(target_period_end.as_deref(), target_period_fp.as_deref());
    let reported_date = get_flow_filed(&["NetIncomeLoss", "Revenues"], "USD");

    Ok(FundamentalData::build(
        FundamentalInputs {
            market: "US".to_string(),
            data_source: "SEC EDGAR".to_string(),
            currency: "USD".to_string(),
            amount_unit: "USD".to_string(),
            fiscal_period,
            reported_date,
            revenue,
            operating_income,
            net_income,
            eps,
            bps,
            equity,
            shares_outstanding,
            dividend,
            dividend_is_forecast: false,
            trading_unit: Some(1),
            next_fiscal_year_end: None,
        },
        latest_price,
    ))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

// Selects the best entry from an XBRL array: sorts by end date then filed date, takes the last
// When annual_only=true, prefers fp=="FY" entries (for annual flow indicators)
fn pick_best_xbrl(arr: &[Value], annual_only: bool) -> Option<&Value> {
    let mut candidates: Vec<&Value> = arr
        .iter()
        .filter(|e| {
            e["form"]
                .as_str()
                .map(|f| f == "10-K" || f == "10-Q")
                .unwrap_or(false)
        })
        .collect();
    if annual_only {
        let fy: Vec<&Value> = candidates
            .iter()
            .copied()
            .filter(|e| e["fp"].as_str().map(|f| f == "FY").unwrap_or(false))
            .collect();
        if !fy.is_empty() {
            candidates = fy;
        }
    }
    candidates.sort_by(|a, b| {
        a["end"]
            .as_str()
            .unwrap_or("")
            .cmp(b["end"].as_str().unwrap_or(""))
            .then_with(|| {
                a["filed"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(b["filed"].as_str().unwrap_or(""))
            })
    });
    candidates.last().copied()
}

// Fetches the flow entry for the latest disclosed period ending on target_end.
// target_fp selects how the period is matched so income-statement items stay on one basis:
//   - Some("FY") → the full-year (12-month) figure (fp=="FY"), most recently filed
//   - otherwise  → the standalone quarter: the shortest reported duration at that end
//                  (3-month over YTD), most recently filed
fn pick_xbrl_for_period<'a>(
    arr: &'a [Value],
    target_end: &str,
    target_fp: Option<&str>,
) -> Option<&'a Value> {
    let candidates: Vec<&Value> = arr
        .iter()
        .filter(|e| {
            e["end"].as_str().unwrap_or("") == target_end
                && e["form"]
                    .as_str()
                    .map(|f| f == "10-K" || f == "10-Q")
                    .unwrap_or(false)
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let by_filed = |a: &&Value, b: &&Value| {
        a["filed"]
            .as_str()
            .unwrap_or("")
            .cmp(b["filed"].as_str().unwrap_or(""))
    };
    if target_fp == Some("FY") {
        // Full year: restrict to fp=="FY" (12-month) entries, take the most recently filed.
        candidates
            .iter()
            .copied()
            .filter(|e| e["fp"].as_str().map(|f| f == "FY").unwrap_or(false))
            .max_by(by_filed)
    } else {
        // Standalone quarter: shortest reported duration, then most recently filed.
        let min_days = candidates.iter().copied().map(period_days).min()?;
        candidates
            .iter()
            .copied()
            .filter(|e| period_days(e) == min_days)
            .max_by(by_filed)
    }
}

// For balance sheet indicators: fetches the entry matching the given end date, preferring 10-K
fn pick_balance_sheet_for_period<'a>(arr: &'a [Value], target_end: &str) -> Option<&'a Value> {
    for form in &["10-K", "10-Q"] {
        let mut candidates: Vec<&Value> = arr
            .iter()
            .filter(|e| {
                e["end"].as_str().unwrap_or("") == target_end
                    && e["form"].as_str().map(|f| f == *form).unwrap_or(false)
            })
            .collect();
        if !candidates.is_empty() {
            candidates.sort_by(|a, b| {
                a["filed"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(b["filed"].as_str().unwrap_or(""))
            });
            return candidates.last().copied();
        }
    }
    None
}

// Approximate day count for a YYYY-MM-DD date (ordering / duration comparison only).
fn approx_days(s: &str) -> Option<i32> {
    let mut it = s.splitn(3, '-');
    let y: i32 = it.next()?.parse().ok()?;
    let m: i32 = it.next()?.parse().ok()?;
    let d: i32 = it.next()?.parse().ok()?;
    Some(y * APPROX_DAYS_PER_YEAR + m * APPROX_DAYS_PER_MONTH + d)
}

// Reported duration (end - start) of an XBRL flow entry, in approximate days.
// A missing start/end sorts last so it is never mistaken for the shortest period.
fn period_days(e: &Value) -> i32 {
    match (
        e["start"].as_str().and_then(approx_days),
        e["end"].as_str().and_then(approx_days),
    ) {
        (Some(s), Some(en)) => en - s,
        _ => i32::MAX,
    }
}

// Picks the most recently dated instant entry (max end, then max filed) from 10-K/10-Q filings.
// Used for cover-page shares (dei:EntityCommonStockSharesOutstanding), which have no period
// to match; the latest reported count is the current shares outstanding.
fn pick_latest_instant(arr: &[Value]) -> Option<&Value> {
    arr.iter()
        .filter(|e| {
            e["form"]
                .as_str()
                .map(|f| f == "10-K" || f == "10-Q")
                .unwrap_or(false)
        })
        .max_by(|a, b| {
            a["end"]
                .as_str()
                .unwrap_or("")
                .cmp(b["end"].as_str().unwrap_or(""))
                .then_with(|| {
                    a["filed"]
                        .as_str()
                        .unwrap_or("")
                        .cmp(b["filed"].as_str().unwrap_or(""))
                })
        })
}

// Returns true if the YYYY-MM-DD range spans approximately one year or less (≤400 days)
// Simple heuristic to exclude multi-year cumulative entries where start is several years back
fn is_single_year_period(start: &str, end: &str) -> bool {
    match (approx_days(start), approx_days(end)) {
        (Some(s), Some(e)) => (e - s) <= SINGLE_YEAR_MAX_DAYS,
        _ => false,
    }
}

// Formats the fiscal-period display value as "<end> (<FP>)" — e.g. "2026-07-27 (Q2)" or
// "2026-01-25 (FY)" — so quarterly vs. annual disclosures are unambiguous in the panel.
fn fiscal_period_label(end: Option<&str>, fp: Option<&str>) -> Option<String> {
    let end = end?;
    match fp {
        Some(fp) if !fp.is_empty() => Some(format!("{end} ({fp})")),
        _ => Some(end.to_string()),
    }
}

fn build_client(proxy_url: Option<&str>, no_proxy: Option<&str>) -> Result<Client> {
    let mut builder = Client::builder()
        .user_agent("Mozilla/5.0 (xoksa)")
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(url) = proxy_url {
        builder = builder.proxy(crate::utils::build_proxy(url, no_proxy)?);
    }
    builder
        .build()
        .map_err(|e| anyhow!("❌ HTTP client build failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ─── detect_market ─────────────────────────────────────────────────────────

    #[test]
    fn test_detect_market_japan_dot_t() {
        assert!(matches!(detect_market("1719.T"), FundamentalMarket::Japan));
        assert!(matches!(detect_market("9984.T"), FundamentalMarket::Japan));
    }

    #[test]
    fn test_detect_market_us() {
        assert!(matches!(detect_market("AAPL"), FundamentalMarket::Us));
        assert!(matches!(detect_market("CSCO"), FundamentalMarket::Us));
        assert!(matches!(detect_market("BRK.B"), FundamentalMarket::Us));
        assert!(matches!(detect_market("BRK-B"), FundamentalMarket::Us));
    }

    #[test]
    fn test_detect_market_unsupported() {
        assert!(matches!(
            detect_market("UNKNOWN.XX"),
            FundamentalMarket::Unsupported(_)
        ));
    }

    // ─── fmt_with_commas ───────────────────────────────────────────────────────

    #[test]
    fn test_fmt_with_commas_basic() {
        assert_eq!(fmt_with_commas(1000.0), "1,000");
        assert_eq!(fmt_with_commas(1000000.0), "1,000,000");
        assert_eq!(fmt_with_commas(123.0), "123");
        assert_eq!(fmt_with_commas(-1234567.0), "-1,234,567");
    }

    // ─── currency_symbol ───────────────────────────────────────────────────────

    #[test]
    fn test_currency_symbol() {
        let usd = FundamentalData {
            currency: "USD".to_string(),
            ..Default::default()
        };
        let jpy = FundamentalData {
            currency: "JPY".to_string(),
            ..Default::default()
        };
        let other = FundamentalData {
            currency: "EUR".to_string(),
            ..Default::default()
        };
        assert_eq!(currency_symbol(&usd), "$");
        assert_eq!(currency_symbol(&jpy), "¥");
        assert_eq!(currency_symbol(&other), "");
    }

    // ─── PER / PBR / ROE calculation ─────────────────────────────────────────────

    #[test]
    fn test_per_pbr_roe_calculation() {
        let price = 100.0_f64;
        let eps = 5.0_f64;
        let bps = 50.0_f64;
        let net_income = 10.0_f64;
        let equity = 100.0_f64;

        let per = price / eps;
        let pbr = price / bps;
        let roe = net_income / equity;

        assert!((per - 20.0).abs() < f64::EPSILON);
        assert!((pbr - 2.0).abs() < f64::EPSILON);
        assert!((roe - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_per_eps_zero_is_none() {
        let price = Some(100.0_f64);
        let eps = Some(0.0_f64);
        let per = match (price, eps) {
            (Some(p), Some(e)) if e.abs() > f64::EPSILON => Some(p / e),
            _ => None,
        };
        assert!(per.is_none());
    }

    // ─── pick_best_xbrl ────────────────────────────────────────────────────────

    #[test]
    fn test_pick_best_xbrl_sort_by_end_date() {
        let arr = vec![
            json!({"form": "10-K", "fp": "FY", "end": "2022-12-31", "filed": "2023-02-01", "val": 100.0}),
            json!({"form": "10-K", "fp": "FY", "end": "2023-12-31", "filed": "2024-02-01", "val": 200.0}),
            json!({"form": "10-K", "fp": "FY", "end": "2021-12-31", "filed": "2022-02-01", "val": 50.0}),
        ];
        let best = pick_best_xbrl(&arr, true).unwrap();
        assert_eq!(best["val"].as_f64().unwrap(), 200.0);
    }

    #[test]
    fn test_pick_best_xbrl_annual_only_prefers_fy() {
        let arr = vec![
            json!({"form": "10-Q", "fp": "Q3", "end": "2024-09-30", "filed": "2024-11-01", "val": 50.0}),
            json!({"form": "10-K", "fp": "FY", "end": "2023-12-31", "filed": "2024-02-01", "val": 200.0}),
        ];
        let best = pick_best_xbrl(&arr, true).unwrap();
        assert_eq!(best["val"].as_f64().unwrap(), 200.0);
    }

    #[test]
    fn test_pick_best_xbrl_annual_only_false_takes_most_recent() {
        let arr = vec![
            json!({"form": "10-Q", "fp": "Q3", "end": "2024-09-30", "filed": "2024-11-01", "val": 50.0}),
            json!({"form": "10-K", "fp": "FY", "end": "2023-12-31", "filed": "2024-02-01", "val": 200.0}),
        ];
        let best = pick_best_xbrl(&arr, false).unwrap();
        assert_eq!(best["val"].as_f64().unwrap(), 50.0);
    }

    #[test]
    fn test_pick_best_xbrl_excludes_non_10k_10q() {
        let arr = vec![
            json!({"form": "8-K", "fp": "FY", "end": "2024-12-31", "filed": "2025-01-01", "val": 999.0}),
            json!({"form": "10-K", "fp": "FY", "end": "2023-12-31", "filed": "2024-02-01", "val": 100.0}),
        ];
        let best = pick_best_xbrl(&arr, false).unwrap();
        assert_eq!(best["val"].as_f64().unwrap(), 100.0);
    }

    // ─── pick_xbrl_for_period (latest disclosed period) ──────────────────────────

    #[test]
    fn test_pick_xbrl_for_period_quarter_prefers_standalone_over_ytd() {
        // At a quarter end, the 3-month standalone quarter is chosen over the 6-month YTD.
        let arr = vec![
            json!({"form": "10-Q", "fp": "Q2", "start": "2026-01-26", "end": "2026-07-27", "filed": "2026-08-27", "val": 190.0}),
            json!({"form": "10-Q", "fp": "Q2", "start": "2026-04-28", "end": "2026-07-27", "filed": "2026-08-27", "val": 100.0}),
        ];
        let e = pick_xbrl_for_period(&arr, "2026-07-27", Some("Q2")).unwrap();
        assert_eq!(e["val"].as_f64().unwrap(), 100.0);
    }

    #[test]
    fn test_pick_xbrl_for_period_annual_picks_full_year() {
        // With target_fp=FY, the 12-month full-year figure is chosen, not a stray 3-month entry.
        let arr = vec![
            json!({"form": "10-K", "fp": "FY", "start": "2025-01-27", "end": "2026-01-25", "filed": "2026-02-25", "val": 800.0}),
            json!({"form": "10-K", "fp": "Q3", "start": "2025-10-27", "end": "2026-01-25", "filed": "2026-02-25", "val": 250.0}),
        ];
        let e = pick_xbrl_for_period(&arr, "2026-01-25", Some("FY")).unwrap();
        assert_eq!(e["val"].as_f64().unwrap(), 800.0);
    }

    #[test]
    fn test_pick_xbrl_for_period_quarter_tie_breaks_by_filed() {
        // Two 3-month entries at the same end: the most recently filed (restated) wins.
        let arr = vec![
            json!({"form": "10-Q", "fp": "Q2", "start": "2026-04-28", "end": "2026-07-27", "filed": "2026-08-27", "val": 100.0}),
            json!({"form": "10-Q", "fp": "Q2", "start": "2026-04-28", "end": "2026-07-27", "filed": "2026-11-05", "val": 105.0}),
        ];
        let e = pick_xbrl_for_period(&arr, "2026-07-27", Some("Q2")).unwrap();
        assert_eq!(e["val"].as_f64().unwrap(), 105.0);
    }

    #[test]
    fn test_pick_latest_instant_cover_shares() {
        // Cover-page shares (dei): the most recent instant (max end) wins; non-10K/10Q excluded.
        let arr = vec![
            json!({"form": "10-Q", "end": "2026-02-20", "filed": "2026-02-25", "val": 24000000000.0}),
            json!({"form": "10-Q", "end": "2026-05-15", "filed": "2026-05-20", "val": 24200000000.0}),
            json!({"form": "8-K", "end": "2026-06-01", "filed": "2026-06-02", "val": 99999999999.0}),
        ];
        let e = pick_latest_instant(&arr).unwrap();
        assert_eq!(e["val"].as_f64().unwrap(), 24200000000.0);
    }

    #[test]
    fn test_fiscal_period_label_quarter_and_annual() {
        assert_eq!(
            fiscal_period_label(Some("2026-07-27"), Some("Q2")).unwrap(),
            "2026-07-27 (Q2)"
        );
        assert_eq!(
            fiscal_period_label(Some("2026-01-25"), Some("FY")).unwrap(),
            "2026-01-25 (FY)"
        );
        assert_eq!(
            fiscal_period_label(Some("2026-01-25"), None).unwrap(),
            "2026-01-25"
        );
        assert!(fiscal_period_label(None, Some("Q2")).is_none());
    }

    // ─── dividend_is_forecast display ────────────────────────────────────────────

    #[test]
    fn test_render_dividend_forecast_label() {
        let data = FundamentalData {
            market: "Japan".to_string(),
            data_source: "J-Quants (v2)".to_string(),
            currency: "JPY".to_string(),
            dividend: Some(100.0),
            dividend_is_forecast: true,
            ..Default::default()
        };
        let lines = render_fundamental_display(&data, "en");
        let div_line = lines
            .iter()
            .find(|l| l.contains("Dividend (per share)"))
            .unwrap();
        assert!(
            div_line.contains("(forecast)"),
            "forecast label should be present: {div_line}"
        );
    }

    #[test]
    fn test_render_dividend_no_forecast_label_when_actual() {
        let data = FundamentalData {
            market: "US".to_string(),
            data_source: "SEC EDGAR".to_string(),
            currency: "USD".to_string(),
            dividend: Some(1.5),
            dividend_is_forecast: false,
            ..Default::default()
        };
        let lines = render_fundamental_display(&data, "en");
        let div_line = lines
            .iter()
            .find(|l| l.contains("Dividend (per share)"))
            .unwrap();
        assert!(
            !div_line.contains("(forecast)"),
            "forecast label should not be present for actual value: {div_line}"
        );
    }
}

#[cfg(test)]
mod confirmed_boundary_tests {
    //! R4: confirmed fundamental data can only be built through the validated
    //! path, and the derived ratios are computed there - never per display path,
    //! never left stale.
    use super::{FundamentalData, FundamentalInputs};

    fn inputs() -> FundamentalInputs {
        FundamentalInputs {
            market: "Japan".into(),
            data_source: "J-Quants (v2)".into(),
            currency: "JPY".into(),
            amount_unit: "JPY".into(),
            fiscal_period: Some("FY (FY ends 2027-03-31)".into()),
            reported_date: Some("2026-05-12".into()),
            revenue: Some(13_000_000_000.0),
            operating_income: Some(1_800_000_000.0),
            net_income: Some(1_200_000_000.0),
            eps: Some(15.0),
            bps: Some(120.0),
            equity: Some(9_600_000_000.0),
            shares_outstanding: Some(80_000_000.0),
            dividend: Some(5.3),
            dividend_is_forecast: false,
            trading_unit: Some(100),
            next_fiscal_year_end: Some("2027-03-31".into()),
        }
    }

    #[test]
    fn ratios_are_derived_once_from_validated_inputs() {
        let d = FundamentalData::build(inputs(), Some(180.0));
        assert_eq!(d.per(), Some(12.0)); // 180 / 15
        assert_eq!(d.pbr(), Some(1.5)); // 180 / 120
        assert_eq!(d.roe(), Some(0.125)); // 1.2e9 / 9.6e9
        assert_eq!(d.latest_price(), Some(180.0));
    }

    #[test]
    fn derived_values_cannot_go_stale() {
        let mut d = FundamentalData::build(inputs(), Some(180.0));
        assert_eq!(d.per(), Some(12.0));
        // The price moves: the ratio must move with it, not stay behind.
        d.recompute_derived(Some(300.0));
        assert_eq!(d.per(), Some(20.0));
        assert_eq!(d.pbr(), Some(2.5));
        // An input changes: the ratios that depend on it are refreshed.
        d.set_bps(Some(150.0));
        assert_eq!(d.pbr(), Some(2.0));
        assert_eq!(d.per(), Some(20.0), "unrelated ratios stay correct");
    }

    #[test]
    fn zero_denominator_leaves_the_ratio_undefined() {
        let mut i = inputs();
        i.eps = Some(0.0);
        i.equity = Some(0.0);
        let d = FundamentalData::build(i, Some(180.0));
        assert_eq!(d.per(), None, "division by zero must not produce a ratio");
        assert_eq!(d.roe(), None);
        // The zero itself is a real reported value and is kept.
        assert_eq!(d.eps(), Some(0.0));
        assert_eq!(d.equity(), Some(0.0));
    }

    #[test]
    fn meaningful_negatives_survive() {
        let mut i = inputs();
        i.net_income = Some(-4_800_000_000.0);
        i.eps = Some(-60.0);
        i.operating_income = Some(-1.0);
        let d = FundamentalData::build(i, Some(180.0));
        assert_eq!(d.eps(), Some(-60.0), "a negative EPS is a real figure");
        assert_eq!(d.operating_income(), Some(-1.0));
        assert_eq!(d.roe(), Some(-0.5), "a loss-making ROE is still a ratio");
        assert_eq!(d.per(), Some(-3.0));
    }

    #[test]
    fn non_finite_inputs_never_become_confirmed_values() {
        let mut i = inputs();
        i.eps = Some(f64::NAN);
        i.bps = Some(f64::INFINITY);
        i.revenue = Some(f64::NEG_INFINITY);
        let d = FundamentalData::build(i, Some(180.0));
        assert_eq!(d.eps(), None);
        assert_eq!(d.bps(), None);
        assert_eq!(d.revenue(), None);
        assert_eq!(d.per(), None, "a ratio from a rejected input is undefined");
        assert_eq!(d.pbr(), None);
    }

    #[test]
    fn a_non_finite_price_does_not_produce_ratios() {
        let d = FundamentalData::build(inputs(), Some(f64::INFINITY));
        assert_eq!(d.latest_price(), None);
        assert_eq!(d.per(), None);
        assert_eq!(d.pbr(), None);
        // ROE does not depend on price and is still available.
        assert_eq!(d.roe(), Some(0.125));
    }

    #[test]
    fn a_denominator_indistinguishable_from_zero_is_refused() {
        // Not an overflow — the quotient is finite. Rejecting only an exact zero
        // would let 1.8e302 through as a PER.
        let mut i = inputs();
        i.eps = Some(1e-300);
        let d = FundamentalData::build(i, Some(180.0));
        assert_eq!(
            d.per(),
            None,
            "a denominator below f64::EPSILON is not a divisor"
        );
        assert_eq!(d.eps(), Some(1e-300), "the reported figure itself is kept");
    }

    #[test]
    fn an_overflowing_ratio_is_refused() {
        let mut i = inputs();
        i.eps = Some(f64::MIN_POSITIVE);
        let d = FundamentalData::build(i, Some(f64::MAX));
        assert_eq!(d.per(), None, "an overflowing ratio is not a value");
    }

    #[test]
    fn unit_currency_period_and_source_travel_with_the_values() {
        let d = FundamentalData::build(inputs(), Some(180.0));
        assert_eq!(d.currency(), "JPY");
        assert_eq!(d.amount_unit(), "JPY");
        assert_eq!(d.data_source(), "J-Quants (v2)");
        assert_eq!(d.fiscal_period(), Some("FY (FY ends 2027-03-31)"));
        assert_eq!(d.reported_date(), Some("2026-05-12"));
    }

    #[test]
    fn unknown_information_is_not_invented() {
        let d = FundamentalData::build(FundamentalInputs::default(), None);
        assert_eq!(d.per(), None);
        assert_eq!(d.pbr(), None);
        assert_eq!(d.roe(), None);
        assert_eq!(d.fiscal_period(), None);
        assert_eq!(d.reported_date(), None);
        assert_eq!(d.latest_price(), None);
    }

    #[test]
    fn the_percentage_helpers_never_return_a_non_finite_value() {
        // `operating_margin_pct` and `roe_pct` scale a ratio by 100, so they can
        // overflow where the underlying ratio did not. A returned `inf` would be
        // rendered as "inf%" and, worse, would become a number the integrity
        // guard has to treat as confirmed. Overflow, zero and non-finite inputs
        // must all yield no percentage at all.
        /// (revenue, operating_income, equity, net_income)
        type Case = (Option<f64>, Option<f64>, Option<f64>, Option<f64>);
        let cases: [Case; 5] = [
            // (revenue, operating_income, equity, net_income)
            (Some(1e-308), Some(1e308), Some(1e-308), Some(1e308)),
            (Some(0.0), Some(1.0), Some(0.0), Some(1.0)),
            (Some(f64::NAN), Some(1.0), Some(f64::NAN), Some(1.0)),
            (
                Some(f64::INFINITY),
                Some(1.0),
                Some(f64::INFINITY),
                Some(1.0),
            ),
            (None, Some(1.0), None, Some(1.0)),
        ];
        for (revenue, operating_income, equity, net_income) in cases {
            let d = FundamentalData::build(
                FundamentalInputs {
                    revenue,
                    operating_income,
                    equity,
                    net_income,
                    ..inputs()
                },
                Some(180.0),
            );
            assert_eq!(
                d.operating_margin_pct(),
                None,
                "revenue={revenue:?} operating_income={operating_income:?}"
            );
            assert_eq!(
                d.roe_pct(),
                None,
                "equity={equity:?} net_income={net_income:?}"
            );
        }
    }

    #[test]
    fn a_percentage_that_is_representable_is_returned() {
        let d = FundamentalData::build(inputs(), Some(180.0));
        // 1.8e9 / 1.3e10 = 13.846...%, 1.2e9 / 9.6e9 = 12.5%
        let margin = d.operating_margin_pct().expect("margin is computable");
        assert!((margin - 13.846_153_846).abs() < 1e-6, "{margin}");
        assert_eq!(d.roe_pct(), Some(12.5));
    }
}
