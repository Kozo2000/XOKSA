//! HTTP API handlers and their JSON request/response models.
//!
//! Boundary rule (see the crate-level design docs): analysis, indicator math,
//! fundamental/market fetching, the LLM call, and backtests all stay on the
//! native side. These handlers *invoke* that native engine and *shape* the
//! result into JSON; the browser/WASM layer only displays what it receives.

use super::SessionId;
use crate::config::{AnalysisMode, Args, Config};
use crate::technical::types::TechnicalDataGuard;
use axum::{
    extract::{ConnectInfo, Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// ── /api/health ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "xoksa-web",
        version: env!("CARGO_PKG_VERSION"),
    })
}

// ── /api/config ──────────────────────────────────────────────────────────────
//
// Client bootstrap config so the Web UI hardcodes nothing: the default ticker
// (first valid CHAT_DEFAULT_TICKER, same as the CLI chat), the default bar mode,
// and the full list of selectable timeframes (AnalysisMode::ALL — single source).

#[derive(Serialize)]
pub struct ConfigResponse {
    pub default_ticker: String,
    pub default_timeframe: String,
    pub timeframes: Vec<String>,
    /// Product version (native binary `CARGO_PKG_VERSION`) — the single source of
    /// truth, shown next to the brand in the header.
    pub version: String,
    /// Max chat message length (chars) the server accepts. Exposed so the Web UI
    /// checks a brushed-interval message before sending — single source, no drift.
    pub max_chat_msg_chars: usize,
    /// Selectable backtest period tokens per timeframe (e.g. "weekly" → ["1y","5y",
    /// …]), so the GUI shows the achievable period the moment a timeframe is picked.
    /// Single source (AnalysisMode::backtest_period_ranges); no manual/help drift.
    pub backtest_periods: HashMap<String, Vec<String>>,
    /// Rule-editor indicator vocabulary (key + localized labels). Single source
    /// (`backtest::rule_indicator_catalog`) so the GUI never hardcodes the keys.
    pub rule_indicators: Vec<LabeledKey>,
    /// Rule-editor operator vocabulary (key + localized labels).
    pub rule_operators: Vec<LabeledKey>,
    /// Built-in starter strategies (name + rules JSON), served so the GUI shows the
    /// same template set the engine defines (`backtest::rule_templates_catalog`).
    pub rule_templates: Vec<RuleTemplate>,
    /// Localized label for each backtest period token (`key` = token), so the GUI
    /// never re-invents the wording (`backtest::backtest_period_label`).
    pub period_labels: Vec<LabeledKey>,
    /// The configured UI language (`LANG` in `xoksa.env`) — the single source the
    /// dashboard, the connection screen, the settings app, and the CLI all read,
    /// so no surface can disagree about which language the user chose.
    pub lang: String,
}

/// A key with its localized display labels (indicator / operator / period token).
#[derive(Serialize)]
pub struct LabeledKey {
    pub key: String,
    pub label_ja: String,
    pub label_en: String,
}

/// A starter strategy served to the rule editor.
#[derive(Serialize)]
pub struct RuleTemplate {
    /// Localized display names, the same shape as [`LabeledKey`] — a template's
    /// name is a label only (nothing is stored under it), so the GUI picks the
    /// one matching the configured language.
    pub name_ja: String,
    pub name_en: String,
    pub spec_json: String,
}

/// First valid entry of CHAT_DEFAULT_TICKER (normalized/sanitized like the CLI),
/// or empty if unset/invalid.
fn default_ticker_from_env(env_map: &HashMap<String, String>) -> String {
    let Some(raw) = env_map.get("CHAT_DEFAULT_TICKER") else {
        return String::new();
    };
    for part in raw.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(sym) = sanitize_symbol(part) {
            return sym;
        }
    }
    String::new()
}

pub async fn config() -> Json<ConfigResponse> {
    let env_map = crate::bootstrap::load_env_map();
    let default_ticker = default_ticker_from_env(&env_map);
    // Resolve the default bar mode through the shared config builder (respects env).
    let probe = if default_ticker.is_empty() {
        "0000.T"
    } else {
        &default_ticker
    };
    let default_timeframe = build_server_config(probe, None)
        .analysis_mode
        .as_str()
        .to_string();
    let timeframes = AnalysisMode::ALL
        .iter()
        .map(|m| m.as_str().to_string())
        .collect();
    let backtest_periods = AnalysisMode::ALL
        .iter()
        .map(|m| {
            (
                m.as_str().to_string(),
                m.backtest_period_ranges()
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            )
        })
        .collect();
    // Rule-editor vocabulary, templates, and period labels — all single-sourced from
    // the engine (`crate::backtest`) so the Web UI renders them, never re-invents them.
    let to_labeled = |cat: &[(&'static str, &'static str, &'static str)]| -> Vec<LabeledKey> {
        cat.iter()
            .map(|(k, ja, en)| LabeledKey {
                key: k.to_string(),
                label_ja: ja.to_string(),
                label_en: en.to_string(),
            })
            .collect()
    };
    let rule_indicators = to_labeled(crate::backtest::rule_indicator_catalog());
    let rule_operators = to_labeled(crate::backtest::rule_operator_catalog());
    let rule_templates = crate::backtest::rule_templates_catalog()
        .iter()
        .map(|(ja, en, spec)| RuleTemplate {
            name_ja: ja.to_string(),
            name_en: en.to_string(),
            spec_json: spec.to_string(),
        })
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    let mut period_labels: Vec<LabeledKey> = Vec::new();
    for m in AnalysisMode::ALL.iter() {
        for tok in m.backtest_period_ranges().iter() {
            let tok = tok.to_string();
            if seen.insert(tok.clone()) {
                period_labels.push(LabeledKey {
                    label_ja: crate::backtest::backtest_period_label(&tok, "ja"),
                    label_en: crate::backtest::backtest_period_label(&tok, "en"),
                    key: tok,
                });
            }
        }
    }
    // The configured language, resolved through the same env map every other
    // surface reads — `ja` only when set to `ja`, English otherwise.
    let lang = match env_map.get("LANG").map(|v| v.trim().to_ascii_lowercase()) {
        Some(v) if v == "ja" => "ja".to_string(),
        _ => "en".to_string(),
    };
    Json(ConfigResponse {
        default_ticker,
        default_timeframe,
        timeframes,
        version: env!("CARGO_PKG_VERSION").to_string(),
        max_chat_msg_chars: MAX_CHAT_MSG_CHARS,
        backtest_periods,
        rule_indicators,
        rule_operators,
        rule_templates,
        period_labels,
        lang,
    })
}

/// Company names observed while building a summary, so the news panel can name a
/// symbol without re-running the analysis. Bounded; a miss simply falls back to
/// the alias file or the ticker itself.
static SYMBOL_NAMES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
const SYMBOL_NAME_CACHE_MAX: usize = 128;

fn remember_symbol_name(ticker: &str, name: &str) {
    if name.is_empty() || name == ticker {
        return;
    }
    let store = SYMBOL_NAMES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut m) = store.lock() {
        if m.len() >= SYMBOL_NAME_CACHE_MAX && !m.contains_key(ticker) {
            m.clear();
        }
        m.insert(ticker.to_string(), name.to_string());
    }
}

/// The display name for a symbol: the alias file first (the user's own mapping),
/// then whatever the last analysis of this symbol reported, else the ticker.
fn display_name(ticker: &str, name_map: &HashMap<String, String>) -> String {
    if let Some(n) = name_map.get(ticker) {
        return n.clone();
    }
    if let Some(code) = crate::bootstrap::jp_code_from_ticker(ticker) {
        if let Some(n) = name_map.get(&code) {
            return n.clone();
        }
    }
    SYMBOL_NAMES
        .get()
        .and_then(|m| m.lock().ok().and_then(|m| m.get(ticker).cloned()))
        .unwrap_or_else(|| ticker.to_string())
}

/// Sanitize a symbol through the canonical chain (`normalize_ticker_input` →
/// `sanitize_ticker` → `normalize_ticker`), returning the reason on failure. Single
/// implementation of the chain (SOT §4.2) — used directly where the error is shown to
/// the user, and via `sanitize_symbol` where it is discarded.
fn sanitize_symbol_result(symbol: &str) -> Result<String, &'static str> {
    let normalized = crate::bootstrap::normalize_ticker_input(symbol);
    crate::bootstrap::sanitize_ticker(&normalized).map(|s| crate::bootstrap::normalize_ticker(&s))
}

/// Sanitize the path symbol, returning the canonical ticker or `None` (error dropped).
fn sanitize_symbol(symbol: &str) -> Option<String> {
    sanitize_symbol_result(symbol).ok()
}

// ── /api/analysis/multi-timeframe ────────────────────────────────────────────
//
// Build the multi-timeframe context pack (Phase 4a) and have the LLM interpret it.
// The pack IS the confirmed data (SOT); the same chat guard constraint is prefixed
// so the model must not invent numbers/news and must label forecasts.

#[derive(Serialize)]
pub struct MultiTimeframeResponse {
    pub symbol: String,
    pub context_pack_text: String,
    pub response: String,
    pub llm_ok: bool,
    /// The ready-to-render "🧠 <provider>/<model> が解説:" attribution line for
    /// `response` (empty when the LLM did not run). Built server-side from the
    /// shared `llm_commentary_badge` so the badge format is identical to the CLI /
    /// Web chat and never drifts — the UI just places it above the commentary.
    pub badge: String,
}

pub async fn analysis_multi_timeframe(
    Extension(SessionId(sid)): Extension<SessionId>,
    Json(spec): Json<crate::context::ContextSpec>,
) -> Json<MultiTimeframeResponse> {
    let fail = |sym: String, msg: String| {
        Json(MultiTimeframeResponse {
            symbol: sym,
            context_pack_text: String::new(),
            response: msg,
            llm_ok: false,
            badge: String::new(),
        })
    };
    let Some(ticker) = sanitize_symbol(&spec.symbol) else {
        return fail(spec.symbol, "invalid symbol".to_string());
    };
    let mut spec = spec;
    spec.symbol = ticker.clone();

    // Run the LLM with the client's single chosen provider/model (SOT, independent of
    // symbol/timeframe), so the model that runs — and bills — is always the one shown
    // in the header; never a random sibling-session's provider.
    let mut config = build_server_config(&ticker, None);
    crate::chat::exec::apply_client_llm_selection(&sid, &mut config);
    apply_request_lang(&mut config, spec.lang.as_deref());
    config.no_llm = false; // this endpoint runs the LLM
    let name_map = match &config.alias_csv {
        Some(path) => crate::bootstrap::load_alias_csv(path).unwrap_or_default(),
        None => HashMap::new(),
    };

    let pack = match crate::context::build_context_pack(&config, &spec, &name_map).await {
        Ok(p) => p,
        Err(e) => return fail(ticker, format!("context build failed: {e}")),
    };

    // The pack IS the confirmed data (SOT). The interpretation prompt (guard +
    // pack + neutral task) is framed by the engine so CLI and Web never fork this
    // assembly (design-philosophy §4.2 / §6); this endpoint only calls it.
    let lang = config.lang.as_str();
    let prompt = pack.interpretation_prompt(&config.chat_guard, lang);

    // The pack's confirmed data goes to the output-integrity guard, per
    // timeframe, so this endpoint verifies attribution like every other LLM path.
    let (response, llm_ok) =
        match crate::llm::send_chat_turn_with_usage(&config, &prompt, Some(&pack.facts)).await {
            Ok((resp, _usage)) => (resp, true),
            Err(e) => (
                loc(
                    lang,
                    &format!("（LLM未実行: {e}）"),
                    &format!("(LLM not run: {e})"),
                ),
                false,
            ),
        };

    // Attribution badge (empty when the LLM did not run) — same shared source as
    // the CLI / Web chat, so the "🧠 …" line never drifts between surfaces.
    let badge = if llm_ok {
        crate::chat::llm::llm_commentary_badge(&config, lang)
    } else {
        String::new()
    };
    Json(MultiTimeframeResponse {
        symbol: ticker,
        context_pack_text: pack.context_pack_text,
        response,
        llm_ok,
        badge,
    })
}

// ── /api/symbol/{symbol}/chart ───────────────────────────────────────────────
//
// Per-bar chart series for the active symbol/timeframe: the actual market-data
// bars (close + volume) plus the engine's indicators computed **at each bar** by
// re-running the same engine on the bar prefix (SOT — identical to the analysis).
// This is the chart's source of truth, replacing client-side polling samples, so
// the chart matches the market data and its point count equals the bar count.

#[derive(Deserialize)]
pub struct ChartQuery {
    pub timeframe: Option<String>,
    pub bars: Option<usize>,
    pub lang: Option<String>,
}

#[derive(Serialize)]
pub struct ChartBar {
    pub t: String,
    pub price: Option<f64>,
    pub vwap: Option<f64>,
    pub ema_s: Option<f64>,
    pub ema_l: Option<f64>,
    pub sma_s: Option<f64>,
    pub sma_l: Option<f64>,
    pub bb_u: Option<f64>,
    pub bb_l: Option<f64>,
    pub rsi: Option<f64>,
    pub macd: Option<f64>,
    pub volume: Option<f64>,
}

#[derive(Serialize)]
pub struct ChartResponse {
    pub symbol: String,
    pub timeframe: String,
    pub timeframe_label: String,
    pub bars: Vec<ChartBar>,
}

pub async fn symbol_chart(
    Path(symbol): Path<String>,
    Query(q): Query<ChartQuery>,
) -> Json<ChartResponse> {
    let empty = |sym: String| {
        Json(ChartResponse {
            symbol: sym,
            timeframe: String::new(),
            timeframe_label: String::new(),
            bars: Vec::new(),
        })
    };
    let Some(ticker) = sanitize_symbol(&symbol) else {
        return empty(symbol);
    };

    let mut config = build_server_config(&ticker, None);
    apply_request_lang(&mut config, q.lang.as_deref());
    if let Some(mode) = q
        .timeframe
        .as_deref()
        .and_then(crate::config::AnalysisMode::from_value)
    {
        config.analysis_mode = mode;
    }
    // The chart offers price/VWAP/EMA/SMA/Bollinger overlays, so force those on
    // regardless of the server's configured extensions; the browser toggles
    // visibility.
    config.enabled_extensions = vec![
        crate::config::ExtensionIndicator::Ema,
        crate::config::ExtensionIndicator::Sma,
        crate::config::ExtensionIndicator::Vwap,
        crate::config::ExtensionIndicator::Bollinger,
    ];
    config.silent = true;
    config.no_news = true;
    config.no_llm = true;

    let name_map = match &config.alias_csv {
        Some(path) => crate::bootstrap::load_alias_csv(path).unwrap_or_default(),
        None => HashMap::new(),
    };

    // One network fetch for the whole series; indicators are recomputed locally.
    let fetcher = crate::market::build_price_fetcher(&config);
    let snapshot =
        match crate::traits::PriceFetcher::fetch_snapshot(&fetcher, &ticker, config.analysis_mode)
            .await
        {
            Ok(s) => s,
            Err(_) => return empty(ticker),
        };
    let mut bars = snapshot.bars;
    bars.sort_by_key(|b| b.timestamp.unwrap_or(0));

    let total = bars.len();
    let n = q.bars.unwrap_or(120).clamp(2, 300);
    let start = total.saturating_sub(n);

    let mut out = Vec::with_capacity(total - start);
    for k in start..total {
        let slice = &bars[..=k];
        let bar = &bars[k];
        let len = slice.len();
        let (mut rsi, mut macd) = (None, None);
        let (mut ema_s, mut ema_l, mut sma_s, mut sma_l, mut vwap) = (None, None, None, None, None);
        let (mut bb_u, mut bb_l) = (None, None);
        // Re-run the engine on the prefix: Ok implies enough bars for the basics;
        // EMA/SMA are gated by their longest period so partial windows stay blank.
        if let Ok(mut guard) =
            crate::technical::build_basic_technical_entry(&config, slice, &name_map)
        {
            rsi = Some(guard.get_rsi());
            macd = Some(guard.get_macd());
            // Silent variant: runs per bar, so the printing one would spam stderr.
            let _ = crate::technical::evaluate_all_selected_extensions_with_report(
                &config, slice, &mut guard,
            );
            if len >= config.ema_long_period {
                ema_s = Some(guard.get_ema_short());
                ema_l = Some(guard.get_ema_long());
            }
            if len >= config.sma_long_period {
                sma_s = Some(guard.get_sma_short());
                sma_l = Some(guard.get_sma_long());
            }
            if len >= config.bollinger_period {
                bb_u = Some(guard.get_bb_upper());
                bb_l = Some(guard.get_bb_lower());
            }
            vwap = guard.get_vwap();
        }
        let t = bar
            .datetime
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| bar.date.clone());
        out.push(ChartBar {
            t,
            price: Some(bar.close),
            vwap,
            ema_s,
            ema_l,
            sma_s,
            sma_l,
            bb_u,
            bb_l,
            rsi,
            macd,
            volume: bar.volume,
        });
    }

    Json(ChartResponse {
        symbol: ticker,
        timeframe: config.analysis_mode.as_str().to_string(),
        timeframe_label: config
            .analysis_mode
            .bar_label(config.lang.as_str())
            .to_string(),
        bars: out,
    })
}

// ── /api/llm/options, /api/llm/select ────────────────────────────────────────
//
// Mouse selection of the LLM (no free-text model entry): the dropdown is built
// from the known set — cloud providers that have an API key (+ their default
// model) and live ollama instances — the same source as the chat `/llm` list.
// Selecting one routes through the unified `/llm` dispatch (SOT), so it persists
// per session and applies to chat/summary/multi-timeframe alike.

// The picker is symbol-scoped only: the LLM selection is a single per-client value,
// independent of timeframe, so the client's `timeframe` query param is intentionally
// not read (serde ignores it). Do not re-add it — that reintroduces the per-timeframe
// coupling that made the model change at random when the pulldown switched.
#[derive(Deserialize)]
pub struct LlmQuery {
    pub symbol: Option<String>,
}

#[derive(Serialize)]
pub struct LlmOption {
    pub value: String,
    pub label: String,
    pub active: bool,
}

#[derive(Serialize)]
pub struct LlmOptionsResponse {
    pub options: Vec<LlmOption>,
}

pub async fn llm_options(
    Extension(SessionId(sid)): Extension<SessionId>,
    Query(q): Query<LlmQuery>,
) -> Json<LlmOptionsResponse> {
    // The picker's active marker is the client's single LLM selection (SOT), applied
    // on an env-built config. It is independent of symbol/timeframe, so switching the
    // timeframe pulldown never re-selects a model — never a random sibling-session's
    // provider (the old non-deterministic scan). Reading the selection takes a short
    // sync lock, so it never blocks on an in-flight chat turn (the old `busy` reason).
    let ticker = q.symbol.as_deref().and_then(sanitize_symbol);
    let mut config = build_server_config(ticker.as_deref().unwrap_or("0000.T"), None);
    crate::chat::exec::apply_client_llm_selection(&sid, &mut config);

    let mut options = Vec::new();
    for (p, key, model) in config.cloud_provider_rows() {
        if crate::utils::resolve_api_key(key).ok().flatten().is_some() {
            options.push(LlmOption {
                value: p.to_string(),
                label: format!("{p} · {model}"),
                active: config.llm_provider == p && config.ollama_alias.is_empty(),
            });
        }
    }
    for inst in crate::chat::llm::live_ollama_instances() {
        options.push(LlmOption {
            active: config.llm_provider == "ollama" && config.ollama_alias == inst.alias,
            // The `/llm` dispatch expects the qualified `ollama:<alias>` form for
            // ollama (a bare alias is not recognized); the label stays friendly.
            value: format!("ollama:{}", inst.alias),
            label: format!("{} · {} (ollama)", inst.alias, inst.model),
        });
    }
    Json(LlmOptionsResponse { options })
}

#[derive(Deserialize)]
pub struct LlmSelectInput {
    pub value: String,
    pub symbol: String,
    pub tickers: Option<String>,
    pub timeframe: Option<String>,
}

#[derive(Serialize)]
pub struct LlmSelectResponse {
    pub ok: bool,
    pub label: String,
}

// ── /api/config/lang ─────────────────────────────────────────────────────────
//
// The UI language is a setting, and settings live in `xoksa.env` — so the
// dashboard's language selector writes `LANG` there rather than keeping a copy
// of its own (it used to sit in the browser's localStorage, which is why the
// dashboard could disagree with the CLI, the native menu, and the settings app).
//
// Loopback callers only: this is the operator's own machine writing the
// operator's own config. A LAN client is served the configured language but
// cannot change it — its browser must not rewrite settings on someone else's
// machine. `--private` writes nothing (`utils::set_env_value` is a no-op there).

#[derive(Deserialize)]
pub struct LangInput {
    pub lang: String,
}

#[derive(Serialize)]
pub struct LangResponse {
    pub ok: bool,
    pub lang: String,
}

pub async fn set_lang(
    ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>,
    Json(req): Json<LangInput>,
) -> Response {
    if !peer.ip().is_loopback() {
        return (
            StatusCode::FORBIDDEN,
            "the language is a server setting; change it on the machine running the engine",
        )
            .into_response();
    }
    // A no-trace session writes no settings — say so instead of reporting a save
    // that did not happen.
    if crate::private::is_private() {
        return (
            StatusCode::CONFLICT,
            "this is a --private (no-trace) session: settings are not written",
        )
            .into_response();
    }
    let lang = match req.lang.as_str() {
        "ja" => "ja",
        "en" => "en",
        _ => return (StatusCode::BAD_REQUEST, "lang must be \"ja\" or \"en\"").into_response(),
    };
    match crate::utils::set_env_value("LANG", lang) {
        Ok(()) => Json(LangResponse {
            ok: true,
            lang: lang.to_string(),
        })
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not save the language: {e}"),
        )
            .into_response(),
    }
}

pub async fn llm_select(
    Extension(SessionId(sid)): Extension<SessionId>,
    Json(req): Json<LlmSelectInput>,
) -> Json<LlmSelectResponse> {
    let fail = || {
        Json(LlmSelectResponse {
            ok: false,
            label: String::new(),
        })
    };
    let Some(ticker) = sanitize_symbol(&req.symbol) else {
        return fail();
    };
    // Defense-in-depth: the value is one of the known set, but bound it to a safe
    // token charset before it reaches the dispatch.
    if req.value.is_empty()
        || req.value.len() > 40
        || !req
            .value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '.' | '-' | '_'))
    {
        return fail();
    }
    let mode = req
        .timeframe
        .as_deref()
        .and_then(crate::config::AnalysisMode::from_value)
        .unwrap_or(crate::config::AnalysisMode::Daily);

    // Comparison extras = the other chips (so the switch doesn't clear them).
    let mut extras: Vec<String> = Vec::new();
    for part in req.tickers.as_deref().unwrap_or("").split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(sym) = sanitize_symbol(part) {
            if !sym.eq_ignore_ascii_case(&ticker)
                && !extras.iter().any(|e| e.eq_ignore_ascii_case(&sym))
            {
                extras.push(sym);
            }
        }
    }

    let mut config = build_server_config(&ticker, Some(mode.as_str()));
    config.no_llm = false;
    let name_map = match &config.alias_csv {
        Some(p) => crate::bootstrap::load_alias_csv(p).unwrap_or_default(),
        None => HashMap::new(),
    };
    // Route through the unified dispatch (persists in the session) with a discard
    // sink so nothing is streamed to the chat panel.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let mut out = crate::chat::ChatOut::Channel(tx);
    let cmd = format!("/llm {}", req.value);
    crate::chat::exec::run_web_command(&sid, &ticker, &cmd, &config, &name_map, &extras, &mut out)
        .await;
    drop(out);
    let mut emitted = String::new();
    while let Ok(line) = rx.try_recv() {
        emitted.push_str(&line);
    }
    let label = crate::chat::exec::client_llm_label(&sid, &config);
    Json(LlmSelectResponse {
        ok: !emitted.contains('❌') && !label.is_empty(),
        label,
    })
}

// ── /api/backtest ────────────────────────────────────────────────────────────
//
// Skeleton backtest: fetch contiguous history from the provider, apply the live
// SOT score with a long-only threshold strategy, and return a summary + trades.
// POST + JSON body → preflight-protected.

pub async fn backtest(
    Json(spec): Json<crate::backtest::BacktestSpec>,
) -> Json<crate::backtest::BacktestResult> {
    let Some(ticker) = sanitize_symbol(&spec.symbol) else {
        return Json(crate::backtest::BacktestResult {
            note: "invalid symbol".to_string(),
            ..Default::default()
        });
    };
    let mut spec = spec;
    spec.symbol = ticker.clone();

    let mut config = build_server_config(&ticker, None);
    apply_request_lang(&mut config, spec.lang.as_deref());
    let name_map = match &config.alias_csv {
        Some(path) => crate::bootstrap::load_alias_csv(path).unwrap_or_default(),
        None => HashMap::new(),
    };

    let result = match crate::backtest::run_backtest(&config, &spec, &name_map).await {
        Ok(r) => r,
        Err(e) => crate::backtest::BacktestResult {
            symbol: ticker,
            note: format!("backtest failed: {e}"),
            ..Default::default()
        },
    };
    Json(result)
}

// ── /api/backtest/rules ──────────────────────────────────────────────────────
//
// Save / list user-defined backtest strategies (the rule editor). Persisted in a
// local JSON file (`~/.xoksa.strategies.json`); saving is skipped in private mode.

#[derive(Deserialize)]
pub struct SaveRuleReq {
    pub name: String,
    pub spec_json: String,
}

#[derive(Serialize)]
pub struct SaveRuleResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub async fn list_rules() -> Json<Vec<crate::strategies::SavedStrategy>> {
    Json(crate::strategies::load())
}

pub async fn save_rule(Json(req): Json<SaveRuleReq>) -> Json<SaveRuleResponse> {
    let err = |e: &str| {
        Json(SaveRuleResponse {
            ok: false,
            error: Some(e.to_string()),
        })
    };
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return err("name required (<=80 chars)");
    }
    // Bound the rule spec size explicitly (the global body limit already caps the
    // whole request; this is a per-field guard).
    if req.spec_json.len() > 32 * 1024 {
        return err("rule spec too large (<=32 KiB)");
    }
    // Reject anything that is not a valid StrategyRules JSON (no garbage stored).
    let Ok(parsed) = serde_json::from_str::<crate::backtest::StrategyRules>(&req.spec_json) else {
        return err("invalid rule spec");
    };
    // `{}` parses fine — every field has a default — so the check above accepted a
    // rule with no conditions at all. `eval_rules` treats an empty set as "no
    // signal" (not a vacuous AND), so such a rule can never open a position and its
    // backtest reports nothing, with the saved name as the only hint that anything
    // was stored. An empty EXIT is left alone: entering and holding to the end of
    // the period is a real baseline to measure against.
    if parsed.entry.is_empty() {
        return err("a rule needs at least one buy condition");
    }
    match crate::strategies::save(name, &req.spec_json) {
        Ok(()) => Json(SaveRuleResponse {
            ok: true,
            error: None,
        }),
        Err(e) => err(&e.to_string()),
    }
}

// ── /api/alerts ──────────────────────────────────────────────────────────────
//
// The dashboard's alert panel. Operates on the same in-session monitor store as
// the chat `/alert` command (Phase 5), but dashboard add/delete ALSO persist the
// rule to `xoksa.env` (`ALERT_<n>_*`) so it survives a restart. Channels come from
// the settings form / `--update-key` (their secrets are Class A) and are only
// referenced here by name.

#[derive(Serialize)]
pub struct AlertsResponse {
    rules: Vec<crate::server::monitor::RuleView>,
    channels: Vec<crate::server::monitor::ChannelView>,
}

pub async fn alerts_list() -> Json<AlertsResponse> {
    let (rules, channels) = crate::server::monitor::list();
    Json(AlertsResponse { rules, channels })
}

#[derive(Serialize)]
pub struct AlertOpResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// The operation took effect on the running monitor, but could not be written
    /// to `xoksa.env`. Distinct from `error`, which means nothing happened at all:
    /// here the rule really is added or removed for this run, and the caller has to
    /// be told it will not survive a restart. Silently returning `ok` would leave a
    /// deleted rule to reappear with no explanation.
    #[serde(skip_serializing_if = "Option::is_none")]
    warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    n: Option<u8>,
}

fn alert_ok(n: Option<u8>) -> Json<AlertOpResponse> {
    Json(AlertOpResponse {
        ok: true,
        error: None,
        warning: None,
        n,
    })
}
fn alert_warn(n: Option<u8>, w: String) -> Json<AlertOpResponse> {
    Json(AlertOpResponse {
        ok: true,
        error: None,
        warning: Some(w),
        n,
    })
}
fn alert_err(e: String) -> Json<AlertOpResponse> {
    Json(AlertOpResponse {
        ok: false,
        error: Some(e),
        warning: None,
        n: None,
    })
}

#[derive(Deserialize)]
pub struct AddAlertReq {
    ticker: String,
    when: String,
    mode: String,
    /// The target channel's `NOTIFY_<n>_NAME`.
    notify: String,
    #[serde(default)]
    explain: bool,
}

pub async fn alerts_add(Json(req): Json<AddAlertReq>) -> Json<AlertOpResponse> {
    let ticker = match sanitize_symbol_result(&req.ticker) {
        Ok(s) => s,
        Err(e) => return alert_err(e.to_string()),
    };
    let Some(when) = crate::config::parse_alert_condition(&req.when) else {
        return alert_err("Cannot parse the condition (e.g. rsi<=30).".to_string());
    };
    let Some(mode) =
        crate::config::AnalysisMode::from_value(req.mode.trim()).filter(|m| m.is_intraday())
    else {
        return alert_err("mode must be one of 1m|5m|15m|30m|60m.".to_string());
    };
    let Some(notify) = crate::server::monitor::channel_number_by_name(req.notify.trim()) else {
        return alert_err(format!(
            "No notification channel named \"{}\".",
            req.notify.trim()
        ));
    };
    // Allocation and the write happen under one lock, so the number cannot be
    // handed out against a file the write has not landed in.
    match crate::server::monitor::add_persisted(
        ticker.clone(),
        when,
        req.when.trim(),
        mode,
        notify,
        req.explain,
    ) {
        // The rule IS live. A failed write is a warning, not an error: reporting it
        // as a failure would have the dashboard hide a rule that is about to start
        // notifying.
        Ok((n, Ok(()))) => alert_ok(Some(n)),
        Ok((n, Err(e))) => alert_warn(
            Some(n),
            format!("Rule #{n} is being monitored but could not be saved, so it will be lost on restart: {e}"),
        ),
        Err(e) => alert_err(crate::server::monitor::op_error_en(&e)),
    }
}

#[derive(Deserialize)]
pub struct AlertNReq {
    n: u8,
}

pub async fn alerts_delete(Json(req): Json<AlertNReq>) -> Json<AlertOpResponse> {
    // Monitoring stops either way; the write decides whether it stays stopped. A
    // read-only or unwritable config would otherwise let the rule come back on the
    // next start with nothing said — the delete would look like it had not worked.
    match crate::server::monitor::remove_persisted(req.n) {
        Ok(Ok(())) => alert_ok(None),
        Ok(Err(e)) => alert_warn(
            None,
            format!(
                "Rule #{} is no longer monitored but could not be removed from the config, so it will come back on restart: {e}",
                req.n
            ),
        ),
        Err(e) => alert_err(crate::server::monitor::op_error_en(&e)),
    }
}

#[derive(Deserialize)]
pub struct ToggleAlertReq {
    n: u8,
    active: bool,
}

pub async fn alerts_toggle(Json(req): Json<ToggleAlertReq>) -> Json<AlertOpResponse> {
    match crate::server::monitor::set_active(req.n, req.active) {
        Ok(()) => alert_ok(None),
        Err(e) => alert_err(crate::server::monitor::op_error_en(&e)),
    }
}

#[derive(Deserialize)]
pub struct TestAlertReq {
    /// The channel's `NOTIFY_<n>_NAME`.
    channel: String,
}

pub async fn alerts_test(Json(req): Json<TestAlertReq>) -> Json<AlertOpResponse> {
    let Some(n) = crate::server::monitor::channel_number_by_name(req.channel.trim()) else {
        return alert_err(format!(
            "No notification channel named \"{}\".",
            req.channel.trim()
        ));
    };
    // Send the test in the CONFIGURED output language (`xoksa.env` LANG), which is
    // what real alert messages use — a test must read exactly like the real thing.
    let lang = build_server_config("", None).lang;
    match crate::server::monitor::send_test(n, &lang).await {
        Ok(()) => alert_ok(None),
        Err(e) => alert_err(crate::server::monitor::test_error_en(&e)),
    }
}

// ── /api/symbol/{symbol}/summary ────────────────────────────────────────────
//
// Runs the real native pipeline (market snapshot -> technical analysis ->
// optional fundamentals) and returns one payload feeding every dashboard panel.
// `technical_display` is the exact CLI render (collect_display_lines), so the
// browser shows the same source-of-truth text the terminal does.

#[derive(Deserialize)]
pub struct SummaryQuery {
    pub timeframe: Option<String>,
    /// GUI language ("ja"/"en", derived from the browser). Overrides config.lang
    /// for this request so the dashboard text matches the browser; absent on CLI.
    pub lang: Option<String>,
    /// A manual "今すぐ更新" (refresh now): bypass the data-change cache and build fresh
    /// — re-fetching the fundamental — and mark the chat to refresh its fundamental on
    /// its next turn. The 60s auto-refresh never sets this (it uses the cache).
    pub force: Option<bool>,
}

/// Override config language for a single GUI request. The GUI sends "ja"/"en"
/// (browser-derived: only "ja" → Japanese, everything else → English). CLI
/// requests omit it and keep the configured language.
fn apply_request_lang(config: &mut Config, lang: Option<&str>) {
    if let Some(l) = lang {
        config.lang = if l.eq_ignore_ascii_case("ja") {
            "ja".to_string()
        } else {
            "en".to_string()
        };
    }
}

/// Localized message by the (already-resolved) language.
fn loc(lang: &str, ja: &str, en: &str) -> String {
    if lang == "ja" {
        ja.to_string()
    } else {
        en.to_string()
    }
}

/// Max characters accepted for a single chat message — guards against
/// unbounded resource consumption (OWASP API4) via a huge query string / body.
const MAX_CHAT_MSG_CHARS: usize = 8000;

/// CSRF guard for browser-triggered endpoints (OWASP API6). Same-origin GETs
/// frequently omit `Origin`, so reject only when an `Origin` (or `Referer`) IS
/// present and its authority does not match the request `Host`. A cross-origin
/// page — which always sends `Origin` on fetch/EventSource — is thereby blocked
/// from triggering LLM work while the local dashboard is running. Returns true
/// when the request must be rejected.
pub(super) fn cross_origin_rejected(headers: &axum::http::HeaderMap) -> bool {
    use axum::http::header::{HOST, ORIGIN, REFERER};
    let Some(host) = headers.get(HOST).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let authority_matches = |url: &str| {
        let after = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
        after
            .split('/')
            .next()
            .unwrap_or("")
            .eq_ignore_ascii_case(host)
    };
    if let Some(origin) = headers.get(ORIGIN).and_then(|v| v.to_str().ok()) {
        return !authority_matches(origin);
    }
    if let Some(referer) = headers.get(REFERER).and_then(|v| v.to_str().ok()) {
        return !authority_matches(referer);
    }
    false
}

#[derive(Serialize, Clone)]
pub struct SummaryResponse {
    pub symbol: String,
    pub ok: bool,
    pub error: Option<String>,
    pub meta: SummaryMeta,
    /// Machine-readable analysis record — the SAME JSON object CLI `--log-format json`
    /// emits (single source of truth: `crate::output::technical_json_value`). `null`
    /// when analysis failed. Integrations should read numbers from here.
    pub data: serde_json::Value,
    /// Full technical analysis text (same lines the CLI prints). Rendered as-is.
    pub technical_display: String,
    /// Fundamental display lines, rendered by the native `render_fundamental_display`
    /// (same output as the CLI). `None` when not fetched (see `fundamental_note`).
    pub fundamental: Option<Vec<String>>,
    pub fundamental_note: Option<String>,
    pub market_line: String,
    /// True when this response was served from the server-side cache (no rebuild
    /// this request) — see `SUMMARY_CACHE_TTL`. Freshly-computed answers are
    /// `false`. The data's own freshness is shown separately via
    /// `meta.market_data_latest_time` (Yahoo's latest observation time).
    pub from_cache: bool,
}

#[derive(Serialize, Default, Clone)]
pub struct SummaryMeta {
    pub company_name: String,
    pub analyzed_at: String,
    pub market_data_latest_time: String,
    /// The confirmed analysis bar time (e.g. the 15-min bar boundary), which only
    /// advances when a new bar forms — unlike `market_data_latest_time`, which is
    /// the latest observation and ticks every minute within a forming bar. The Web
    /// UI keys its market-data log on this so a row is appended once per bar.
    pub bar_time: String,
    pub model: String,
    pub timeframe: String,
    /// Human-readable bar label for `timeframe` (e.g. "5分足" / "5min bar"), from
    /// `AnalysisMode::bar_label` (SOT) so the Web UI shows no hardcoded labels.
    pub timeframe_label: String,
}

/// Server-side summary cache — the data-change-detection cadence.
///
/// Yahoo publishes intraday data at ~1-minute granularity, so a rebuild faster
/// than that yields identical numbers. Each `(lang|timeframe|symbol)` build is
/// kept for `SUMMARY_CACHE_TTL`; a request inside that window is answered from the
/// cache (flagged `from_cache`, skipping the Yahoo fetch, analysis, and
/// fundamental call) rather than redoing work that cannot have changed yet. The
/// dashboard polls at the same cadence, so it tracks Yahoo's update rate without
/// busy-polling.
const SUMMARY_CACHE_TTL: Duration = Duration::from_secs(60);

struct CachedSummary {
    built_at: Instant,
    response: SummaryResponse,
}

static SUMMARY_CACHE: OnceLock<Mutex<HashMap<String, CachedSummary>>> = OnceLock::new();

fn summary_cache() -> &'static Mutex<HashMap<String, CachedSummary>> {
    SUMMARY_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub async fn symbol_summary(
    Extension(SessionId(sid)): Extension<SessionId>,
    Path(symbol): Path<String>,
    Query(q): Query<SummaryQuery>,
) -> Json<SummaryResponse> {
    // Sanitize the path-supplied symbol with the same rules as the CLI.
    let ticker = match sanitize_symbol_result(&symbol) {
        Ok(s) => s,
        Err(e) => return Json(error_summary(symbol, e.to_string())),
    };

    let mut config = build_server_config(&ticker, q.timeframe.as_deref());
    apply_request_lang(&mut config, q.lang.as_deref());

    // Data-change-detection cache (see SUMMARY_CACHE_TTL): within the TTL the
    // numbers cannot have changed, so answer from the cache and skip the Yahoo
    // fetch, analysis, and fundamental call. Only the per-client model badge is
    // refreshed so a shared entry still shows this caller's own LLM selection
    // (SOT); every other field is identical to the cached build.
    // A manual refresh (今すぐ更新, `force`) bypasses the cache — build fresh, re-fetching
    // the fundamental — and marks the chat to refresh its fundamental on its next turn.
    // The 60s auto-refresh uses the cache and leaves the (rarely-changing) fundamental.
    let force = q.force.unwrap_or(false);
    if force {
        crate::chat::exec::store_force_fundamental(&sid, &ticker);
    }
    let cache_key = format!(
        "{}|{}|{}",
        config.lang,
        config.analysis_mode.as_str(),
        ticker
    );
    if !force {
        if let Some(mut cached) = summary_cache().lock().ok().and_then(|cache| {
            cache
                .get(&cache_key)
                .filter(|entry| entry.built_at.elapsed() < SUMMARY_CACHE_TTL)
                .map(|entry| entry.response.clone())
        }) {
            cached.from_cache = true;
            cached.meta.model = crate::chat::exec::client_llm_label(&sid, &config);
            return Json(cached);
        }
    }

    let name_map = match &config.alias_csv {
        Some(path) => crate::bootstrap::load_alias_csv(path).unwrap_or_default(),
        None => HashMap::new(),
    };

    let guard = match crate::app::build_analyzed_guard(&config, &ticker, &name_map).await {
        Ok(g) => g,
        Err(e) => return Json(error_summary(ticker, format!("analysis failed: {e}"))),
    };

    remember_symbol_name(&ticker, guard.get_name());
    let technical_display = crate::app::collect_display_lines(&config, &guard).join("\n");
    let market_line = build_market_line(&ticker, &guard, &config);
    // Machine-readable analysis JSON — identical to CLI `--log-format json` (single
    // source of truth). See `summary_analysis_json`.
    let data = summary_analysis_json(&config, &guard);

    // Fundamentals are best-effort: missing API keys / unsupported symbols are a
    // note, not an error (the technical panel still renders).
    let latest_price = guard
        .get_latest_observed_price()
        .unwrap_or_else(|| guard.get_close());
    let (fundamental, fundamental_note) = match crate::fundamental::fetch_fundamental_data(
        &ticker,
        &config,
        Some(latest_price),
    )
    .await
    {
        // Reuse the CLI's fundamental renderer (single source) — same text as the terminal.
        Ok(d) => (
            Some(crate::fundamental::render_fundamental_display(
                &d,
                &config.lang,
            )),
            None,
        ),
        Err(e) => (
            None,
            Some(loc(
                &config.lang,
                &format!("ファンダメンタルデータ未取得（APIキー未設定/対象外の可能性）: {e}"),
                &format!("Fundamental data unavailable (API key missing / unsupported): {e}"),
            )),
        ),
    };

    // Model badge: the client's single LLM selection (SOT) overlaid on this request's
    // env-built config, else the env default — the exact model that will run and bill,
    // so the header never shows one model while another is used. Same value `/status`
    // and the picker report.
    let model_label = crate::chat::exec::client_llm_label(&sid, &config);

    let response = SummaryResponse {
        symbol: ticker.clone(),
        ok: true,
        error: None,
        meta: SummaryMeta {
            company_name: guard.get_name().to_string(),
            analyzed_at: guard.get_analyzed_at().unwrap_or("").to_string(),
            market_data_latest_time: guard
                .get_market_data_latest_time()
                .unwrap_or("")
                .to_string(),
            bar_time: guard
                .get_bar_time()
                .unwrap_or_else(|| guard.get_date())
                .to_string(),
            model: model_label,
            timeframe: config.analysis_mode.as_str().to_string(),
            timeframe_label: config.analysis_mode.bar_label(&config.lang).to_string(),
        },
        data,
        technical_display,
        fundamental,
        fundamental_note,
        market_line,
        from_cache: false,
    };
    // Store the freshly-built snapshot so requests within the TTL are served from
    // cache (data-change-detection cadence).
    if let Ok(mut cache) = summary_cache().lock() {
        cache.insert(
            cache_key,
            CachedSummary {
                built_at: Instant::now(),
                response: response.clone(),
            },
        );
    }
    Json(response)
}

fn error_summary(symbol: String, error: String) -> SummaryResponse {
    SummaryResponse {
        symbol,
        ok: false,
        error: Some(error),
        meta: SummaryMeta::default(),
        data: serde_json::Value::Null,
        technical_display: String::new(),
        fundamental: None,
        fundamental_note: None,
        market_line: String::new(),
        from_cache: false,
    }
}

/// Build a Config for a server-side analysis: respects `xoksa.env` (provider,
/// weights, periods, language, enabled indicators) via the shared CLI loader,
/// then forces only the dashboard transport flags (silent, no terminal news/log).
///
/// The enabled indicators come straight from the user's config — identical to the
/// CLI. The server does NOT inject its own indicator set; if the user enabled
/// none, the dashboard shows none, exactly like the terminal. (SOT: the Web UI
/// must never diverge from the CLI for the same configuration.)
pub(crate) fn build_server_config(ticker: &str, timeframe: Option<&str>) -> Config {
    let env_map = crate::bootstrap::load_env_map();
    // Reuse the full CLI config builder by synthesizing a minimal arg vector.
    let args = Args::parse_from(["xoksa", "-t", ticker]);
    let mut config = crate::config::build_config(&args, &env_map);

    if let Some(tf) = timeframe {
        if let Some(mode) = AnalysisMode::from_value(tf) {
            config.analysis_mode = mode;
        }
    }
    config.ticker = ticker.to_string();
    config.no_news = true;
    config.no_llm = true;
    config.silent = true;
    config.save_technical_log = false;
    config.chat = false;
    config.fundamental = true;
    config
}

/// Compact market snapshot line. The indicator selection is NOT chosen here — it
/// is the CLI's canonical reading (`notice_indicator_summary`): the always-on
/// RSI/MACD basics plus exactly the extensions enabled in `config` (SOT). This is
/// the same set the terminal autoreload notice and `/status` show, so the Web UI
/// never invents its own indicator subset.
fn build_market_line(ticker: &str, guard: &TechnicalDataGuard, config: &Config) -> String {
    let price = guard
        .get_latest_observed_price()
        .unwrap_or_else(|| guard.get_close());
    let when = guard
        .get_market_data_latest_time()
        .or_else(|| guard.get_analyzed_at())
        .unwrap_or("");
    // Identity + price rendered by the SHARED segment (SOT) so the dashboard's market
    // line matches the CLI: resolved company-name label, localized labels, one price
    // format. Indicators come from the shared `notice_indicator_summary`. A one-shot
    // line, so no "vs prev reload" delta (that is the autoreload notice's addition).
    let label = format!("{} ({})", ticker, guard.get_name());
    let identity = crate::chat::ticker::market_identity_price(&label, price, &config.lang);
    let indicators = crate::chat::ticker::notice_indicator_summary(guard, config, &config.lang);
    format!("{when}  {identity}  {indicators}")
}

/// The API's machine-readable analysis JSON (the `data` field of `/summary`).
/// **Single source of truth**: this returns the EXACT object CLI `--log-format json`
/// writes — `crate::output::technical_json_value` over the same guard + snapshot the
/// CLI uses. Named (not inlined) so the SOT gate test `api_data_equals_cli_json_row`
/// can assert API `data` == CLI JSON by construction.
pub(crate) fn summary_analysis_json(
    config: &Config,
    guard: &TechnicalDataGuard,
) -> serde_json::Value {
    let snap = crate::technical::calculate_final_score_snapshot(config, guard);
    crate::output::technical_json_value(config, guard, &snap)
}

// ── /api/symbol/{symbol}/news ───────────────────────────────────────────────
//
// Separate from /summary so the dashboard's periodic auto-refresh does NOT keep
// hitting the Brave API: the frontend fetches news only when the symbol changes.
// Best-effort: a missing BRAVE_API_KEY is a note, not an error.

#[derive(Serialize)]
pub struct NewsItem {
    pub title: String,
    pub url: String,
    pub published_at: Option<String>,
}

#[derive(Serialize)]
pub struct NewsResponse {
    pub ok: bool,
    pub items: Vec<NewsItem>,
    pub note: Option<String>,
}

fn news_err(note: String) -> Json<NewsResponse> {
    Json(NewsResponse {
        ok: false,
        items: Vec::new(),
        note: Some(note),
    })
}

#[derive(Deserialize)]
pub struct NewsQuery {
    pub lang: Option<String>,
    /// Free-text search terms. When present, they replace the ticker-derived
    /// query so Brave searches exactly what the user typed (the terms are
    /// URL-encoded at the Brave request boundary — see `news.rs`).
    pub q: Option<String>,
    /// Finance-relevance filter, controlled by the news panel's "フィルタを外す"
    /// checkbox. Absent → on (the dashboard default): results are narrowed to
    /// investor-relevant news. `false` → raw (unfiltered) results.
    pub filter: Option<bool>,
}

pub async fn symbol_news(
    Extension(SessionId(sid)): Extension<SessionId>,
    Path(symbol): Path<String>,
    Query(q): Query<NewsQuery>,
) -> Json<NewsResponse> {
    let ticker = match sanitize_symbol_result(&symbol) {
        Ok(s) => s,
        Err(e) => return news_err(e.to_string()),
    };

    let mut config = build_server_config(&ticker, None);
    apply_request_lang(&mut config, q.lang.as_deref());
    config.show_news = false; // return data only; no terminal output
                              // Free-text search: use the typed terms as the Brave query instead of the
                              // ticker-derived one. Route through the SAME sanitizer the CLI uses
                              // (`sanitize_news_query`: rejects shell/URL metacharacters and over-long
                              // input) — single source, no Web-UI-specific bypass. Blank input falls back
                              // to the default; rejected input returns a note rather than a raw query.
    if let Some(raw) = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        match crate::bootstrap::sanitize_news_query(raw) {
            Ok(clean) => config.custom_news_query = Some(clean),
            Err(e) => {
                return news_err(loc(
                    &config.lang,
                    &format!("検索語が不正です: {e}"),
                    &format!("Invalid search terms: {e}"),
                ))
            }
        }
    }
    // Finance-relevance filter is ON by default for the dashboard (the panel's
    // "フィルタを外す" checkbox turns it off). The UI control is the source of truth
    // here, so it overrides whatever NEWS_FILTER resolved to from the env.
    config.news_filter = q.filter.unwrap_or(true);

    let name_map = match &config.alias_csv {
        Some(path) => crate::bootstrap::load_alias_csv(path).unwrap_or_default(),
        None => HashMap::new(),
    };
    // News needs the ticker and the company name — not an analysis. Building a
    // guard here meant a second market fetch and a second full indicator pass for
    // a symbol the summary had just analyzed.
    let subject = crate::news::NewsSubject {
        ticker: ticker.clone(),
        name: display_name(&ticker, &name_map),
    };

    let fetcher = crate::news::BraveArticleFetcher {
        proxy_url: config.https_proxy.clone(),
        no_proxy: config.no_proxy.clone(),
    };
    match crate::news::news_flow_controller(&subject, &config, &fetcher).await {
        Ok(arts) => {
            // The chat/LLM must reason over exactly the news the panel shows (SOT):
            // store this fetch (keyed sid|symbol) so the next chat turn's news buffer
            // matches. Runs for a symbol change, a free-text search, and the
            // "フィルタを外す" toggle alike — whatever the panel is currently displaying.
            crate::chat::exec::store_panel_news(&sid, &ticker, &arts);
            Json(NewsResponse {
                ok: true,
                items: arts
                    .into_iter()
                    .map(|a| NewsItem {
                        title: a.title,
                        url: a.url,
                        published_at: a.published_at,
                    })
                    .collect(),
                note: None,
            })
        }
        Err(e) => news_err(loc(
            &config.lang,
            &format!("ニュース取得失敗（BRAVE_API_KEY 未設定/対象外の可能性）: {e}"),
            &format!("News fetch failed (BRAVE_API_KEY missing / unsupported): {e}"),
        )),
    }
}

// ── /api/chat/stream (SSE) ───────────────────────────────────────────────────
//
// Streaming variant of /api/chat: each output line is emitted as an SSE event the
// moment it is produced, so long multi-step commands (e.g. `/forum ask`) show
// progress live instead of appearing only when fully finished. The final
// sentinel line tells the client to close (so the browser EventSource does not
// auto-reconnect and re-run the command).

/// Sentinel marking the end of a stream — the client closes on receiving it.
pub const STREAM_DONE: &str = "[[XOKSA_DONE]]";

#[derive(Deserialize)]
pub struct ChatStreamQuery {
    pub message: String,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
    /// All loaded tickers (comma-separated, active first) — the header chips. The
    /// active one (`symbol`) is the primary; the rest are comparison extras.
    #[serde(default)]
    pub tickers: Option<String>,
    /// Header timeframe (daily/5m/…), so the chat analyzes the same bar mode the
    /// dashboard shows.
    #[serde(default)]
    pub timeframe: Option<String>,
}

pub async fn chat_stream(
    Extension(SessionId(sid)): Extension<SessionId>,
    headers: axum::http::HeaderMap,
    Query(q): Query<ChatStreamQuery>,
) -> axum::response::Response {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use axum::response::IntoResponse;

    // CSRF (API6): block cross-origin browser requests from triggering LLM work.
    if cross_origin_rejected(&headers) {
        return (
            axum::http::StatusCode::FORBIDDEN,
            "cross-origin request rejected",
        )
            .into_response();
    }
    // Resource cap (API4): bound a single message length.
    if q.message.chars().count() > MAX_CHAT_MSG_CHARS {
        return (
            axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            "message too long",
        )
            .into_response();
    }

    let lang: &'static str = match q.lang.as_deref() {
        Some(l) if l.eq_ignore_ascii_case("ja") => "ja",
        Some(_) => "en",
        None => "ja",
    };
    let message = q.message.trim().to_string();
    let raw_symbol = q.symbol.clone().unwrap_or_default();
    let lang_param = q.lang.clone();
    let tickers_csv = q.tickers.clone().unwrap_or_default();
    let timeframe = q.timeframe.clone();

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        let done = tx.clone();
        run_chat_stream(
            sid,
            message,
            raw_symbol,
            tickers_csv,
            timeframe,
            lang_param,
            lang,
            tx,
        )
        .await;
        let _ = done.send(STREAM_DONE.to_string());
    });

    let stream = futures::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|line| {
            (
                Ok::<_, std::convert::Infallible>(Event::default().data(line)),
                rx,
            )
        })
    });
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

#[allow(clippy::too_many_arguments)]
async fn run_chat_stream(
    sid: String,
    message: String,
    raw_symbol: String,
    tickers_csv: String,
    timeframe: Option<String>,
    lang_param: Option<String>,
    lang: &str,
    tx: tokio::sync::mpsc::UnboundedSender<String>,
) {
    if message.is_empty() {
        let _ = tx.send(loc(lang, "メッセージが空です。", "Message is empty."));
        return;
    }
    let ticker = match sanitize_symbol_result(&raw_symbol) {
        Ok(s) => s,
        Err(_) => {
            let _ = tx.send(loc(
                lang,
                "銘柄が未指定/不正です。ヘッダーで銘柄を指定してください。",
                "No valid symbol. Set one in the header.",
            ));
            return;
        }
    };
    // Comparison extras = all chips minus the active one (sanitized, deduped).
    let mut extras: Vec<String> = Vec::new();
    for part in tickers_csv.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(sym) = sanitize_symbol(part) {
            if !sym.eq_ignore_ascii_case(&ticker)
                && !extras.iter().any(|e| e.eq_ignore_ascii_case(&sym))
            {
                extras.push(sym);
            }
        }
    }
    let mut config = build_server_config(&ticker, timeframe.as_deref());
    apply_request_lang(&mut config, lang_param.as_deref());
    config.no_llm = false;
    let name_map = match &config.alias_csv {
        Some(path) => crate::bootstrap::load_alias_csv(path).unwrap_or_default(),
        None => HashMap::new(),
    };
    // Every command targets the full loaded ticker table — the algorithm never
    // changes with ticker count (no single-symbol focus). Owner decision.
    let mut out = crate::chat::ChatOut::Channel(tx);
    crate::chat::exec::run_web_command(
        &sid, &ticker, &message, &config, &name_map, &extras, &mut out,
    )
    .await;
}

// ── /api/chat/commands ──────────────────────────────────────────────────────
//
// Help list for the chat-mode slash commands, shown in the Web UI Help screen.
// Derived from the single command catalog in `chat::help` (same source the CLI
// `/help` renders), so the two never drift. Continuation notes (entries with an
// empty command) are skipped here.

#[derive(Serialize)]
pub struct HelpCommand {
    pub command: String,
    pub description: String,
}

pub async fn chat_commands(Query(q): Query<NewsQuery>) -> Json<Vec<HelpCommand>> {
    // GUI passes its (browser-derived) language; otherwise fall back to config.
    let mut config = build_server_config("0000.T", None);
    apply_request_lang(&mut config, q.lang.as_deref());
    let commands = crate::chat::help::command_catalog(&config.lang)
        .into_iter()
        // Skip section headers ("#") and continuation notes ("").
        .filter(|entry| !entry.command.is_empty() && entry.command != "#")
        .map(|entry| HelpCommand {
            command: format!("/{}", entry.command),
            description: entry.desc,
        })
        .collect();
    Json(commands)
}

#[cfg(test)]
mod sot_gate_tests {
    use super::summary_analysis_json;
    use crate::config::{Config, ExtensionIndicator};
    use crate::output::{DefaultTechnicalLogFormatter, TechnicalLogFormatter};
    use crate::technical::calculate_final_score_snapshot;
    use crate::technical::types::TechnicalDataGuard;

    /// **SOT gate (deterministic, non-negotiable).** The API `/summary` `data` field
    /// and the CLI `--log-format json` output MUST be byte-identical: both are the one
    /// `output::technical_json_value` serializer. This is not a preference — it is the
    /// SOT invariant. If any future edit re-introduces a server-local analysis
    /// serializer, or makes the CLI `json_row` stop delegating, THIS TEST FAILS the
    /// build. That is the enforcement — not memory, not a promise.
    #[test]
    fn api_data_equals_cli_json_row() {
        let config = Config {
            enabled_extensions: vec![
                ExtensionIndicator::Ema,
                ExtensionIndicator::Bollinger,
                ExtensionIndicator::Vwap,
            ],
            ..Config::default()
        };
        let mut guard = TechnicalDataGuard::new("7203.T".to_string(), "2026-07-03".to_string());
        guard.set_signal_score(2.0);
        guard.set_ema_score(1.0);

        let snap = calculate_final_score_snapshot(&config, &guard);
        // CLI path — exactly what `xoksa --log-format json` writes.
        let cli_json = DefaultTechnicalLogFormatter
            .json_row(&config, &guard, &snap)
            .expect("json_row");
        // API path — exactly what `/api/symbol/{}/summary` puts in `data`.
        let api_json =
            serde_json::to_string(&summary_analysis_json(&config, &guard)).expect("serialize data");

        assert_eq!(
            cli_json, api_json,
            "SOT VIOLATION: CLI `--log-format json` and API `data` diverged. Both MUST \
             come from output::technical_json_value — do not add a second serializer."
        );
    }
}
