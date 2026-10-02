//! XOKSA Web UI — Rust/WASM frontend (Leptos, client-side rendered).
//!
//! Presentation only. It calls the native server's JSON API (`/api/*`) and
//! renders the result. No analysis / indicators / LLM / DB / backtest logic
//! lives here — those stay native (same boundary as the previous JS UI).
//!
//! Features over the JS version: dark/light theme toggle, S/M/L font size, and
//! reactive auto-refresh (a Leptos Resource refetches when symbol/timeframe
//! change, plus a periodic tick — so no manual "Analyze" press is required).

// Presentation layer holds no `unsafe` of its own — enforce at compile time.
#![forbid(unsafe_code)]

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::{ev, html};
use serde::Deserialize;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

// ── DTOs (mirror the server JSON contract) ───────────────────────────────────

#[derive(Clone, Default, Deserialize)]
struct SummaryResponse {
    ok: bool,
    error: Option<String>,
    meta: SummaryMeta,
    technical_display: String,
    /// Fundamental display lines, rendered server-side (same text as the CLI).
    fundamental: Option<Vec<String>>,
    fundamental_note: Option<String>,
    market_line: String,
    /// True when the server answered from its data-change-detection cache (no
    /// rebuild this poll). The data's own timestamp is
    /// `meta.market_data_latest_time`.
    #[serde(default)]
    from_cache: bool,
}

#[derive(Clone, Default, Deserialize)]
struct SummaryMeta {
    /// Yahoo's latest observation time (regularMarketTime) — the "data time" shown
    /// to the user. Advances only when Yahoo publishes newer data.
    #[serde(default)]
    market_data_latest_time: String,
    model: String,
}

#[derive(Clone, Default, Deserialize)]
struct NewsResponse {
    ok: bool,
    items: Vec<NewsItem>,
    note: Option<String>,
}

#[derive(Clone, Deserialize)]
struct NewsItem {
    title: String,
    url: String,
    published_at: Option<String>,
}

#[derive(Clone, Deserialize)]
struct HelpCommand {
    command: String,
    description: String,
}

// Per-bar chart series (GET /api/symbol/{symbol}/chart). The market-data bars are
// the source of truth — the chart plots these, not client-side polling samples.
#[derive(Clone, Default, Deserialize)]
struct ChartResp {
    #[serde(default)]
    timeframe_label: String,
    #[serde(default)]
    bars: Vec<ChartBarResp>,
}

#[derive(Clone, Default, Deserialize)]
struct ChartBarResp {
    #[serde(default)]
    t: String,
    #[serde(default)]
    price: Option<f64>,
    #[serde(default)]
    vwap: Option<f64>,
    #[serde(default)]
    ema_s: Option<f64>,
    #[serde(default)]
    ema_l: Option<f64>,
    #[serde(default)]
    sma_s: Option<f64>,
    #[serde(default)]
    sma_l: Option<f64>,
    #[serde(default)]
    bb_u: Option<f64>,
    #[serde(default)]
    bb_l: Option<f64>,
    #[serde(default)]
    rsi: Option<f64>,
    #[serde(default)]
    macd: Option<f64>,
    #[serde(default)]
    volume: Option<f64>,
}

// Backtest (POST /api/backtest) summary response.
/// Backtest response. The engine renders the entire report (SOT) into `report`;
/// the UI displays those lines verbatim. `note` carries the error-path message
/// (shown instead of the report). No other fields are needed here — the numbers,
/// verdict, and trade ledger all live inside `report`.
#[derive(Clone, Default, Deserialize)]
struct BacktestResp {
    #[serde(default)]
    note: String,
    #[serde(default)]
    report: Vec<String>,
}

// Alerts (GET /api/alerts). Rules + channels for the dashboard alert panel.
#[derive(Clone, Default, Deserialize)]
struct AlertRuleView {
    n: u8,
    ticker: String,
    mode: String,
    cond: String,
    notify: u8,
    explain: bool,
    active: bool,
    /// The last notification attempt for this rule in this run (absent until it
    /// fires once). Without this the panel could not distinguish a rule that
    /// fired and failed to send from one that never fired.
    #[serde(default)]
    last_send_at: Option<String>,
    #[serde(default)]
    last_send_ok: Option<bool>,
    #[serde(default)]
    last_send_error: Option<String>,
}
#[derive(Clone, Default, Deserialize)]
struct AlertChannelView {
    n: u8,
    kind: String,
    name: String,
    secret_set: bool,
}
#[derive(Clone, Default, Deserialize)]
struct AlertsResp {
    #[serde(default)]
    rules: Vec<AlertRuleView>,
    #[serde(default)]
    channels: Vec<AlertChannelView>,
}
// Result of an alert mutation (POST /api/alerts…).
#[derive(Clone, Default, Deserialize)]
struct AlertOpResp {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    /// The change took effect on the running monitor but could not be written to
    /// the config — the rule is live (or stopped) now, and will not be after a
    /// restart. Shown as a warning, not an error: the operation did happen.
    #[serde(default)]
    warning: Option<String>,
}

// LLM picker options (GET /api/llm/options) — the known selectable set.
#[derive(Clone, Default, Deserialize)]
struct LlmOpt {
    #[serde(default)]
    value: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    active: bool,
}

#[derive(Clone, Default, Deserialize)]
struct LlmOptionsResp {
    #[serde(default)]
    options: Vec<LlmOpt>,
}

// Multi-timeframe analysis (POST /api/analysis/multi-timeframe) response.
#[derive(Clone, Default, Deserialize)]
struct MtfResp {
    #[serde(default)]
    context_pack_text: String,
    #[serde(default)]
    response: String,
    #[serde(default)]
    llm_ok: bool,
    // Ready-to-render "🧠 …が解説:" line built server-side (shared badge source).
    #[serde(default)]
    badge: String,
}

#[derive(Clone, Default, Deserialize)]
struct ConfigResponse {
    default_ticker: String,
    default_timeframe: String,
    timeframes: Vec<String>,
    #[serde(default)]
    version: String,
    #[serde(default)]
    max_chat_msg_chars: usize,
    #[serde(default)]
    backtest_periods: std::collections::HashMap<String, Vec<String>>,
    /// Rule-editor vocabulary, templates, and period labels — all single-sourced from
    /// the engine (served here so the UI never hardcodes them).
    #[serde(default)]
    rule_indicators: Vec<LabeledKey>,
    #[serde(default)]
    rule_operators: Vec<LabeledKey>,
    #[serde(default)]
    rule_templates: Vec<RuleTemplateDto>,
    #[serde(default)]
    period_labels: Vec<LabeledKey>,
}

/// A key with localized labels (indicator / operator / period token), served by the engine.
#[derive(Clone, Default, Deserialize)]
struct LabeledKey {
    #[serde(default)]
    key: String,
    #[serde(default)]
    label_ja: String,
    #[serde(default)]
    label_en: String,
}

/// A starter strategy served by the engine (localized name + rules JSON).
#[derive(Clone, Default, Deserialize)]
struct RuleTemplateDto {
    #[serde(default)]
    name_ja: String,
    #[serde(default)]
    name_en: String,
    #[serde(default)]
    spec_json: String,
}

// Chat goes through SSE (/api/chat/stream); see `send_chat`. No request/response
// DTOs are needed here — output streams in line by line.

// ── HTTP (the only place that talks to the backend) ──────────────────────────

async fn fetch_summary(
    symbol: String,
    timeframe: String,
    lang: &str,
    force: bool,
) -> Result<SummaryResponse, String> {
    let mut url = format!("/api/symbol/{symbol}/summary?timeframe={timeframe}&lang={lang}");
    if force {
        // Manual refresh: bypass the server's data-change cache and re-fetch fresh
        // (incl. the fundamental) + mark the chat to refresh its fundamental.
        url.push_str("&force=true");
    }
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    resp.json::<SummaryResponse>()
        .await
        .map_err(|e| e.to_string())
}

async fn fetch_news(
    symbol: String,
    lang: &str,
    q: Option<String>,
    filter_off: bool,
) -> Result<NewsResponse, String> {
    let mut url = format!("/api/symbol/{symbol}/news?lang={lang}");
    if let Some(terms) = q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        url.push_str(&format!(
            "&q={}",
            js_sys::encode_uri_component(terms)
                .as_string()
                .unwrap_or_default()
        ));
    }
    // filter=true (finance filter on, the default) / filter=false (raw). Always
    // sent explicitly; the server also treats an absent param as on.
    url.push_str(if filter_off {
        "&filter=false"
    } else {
        "&filter=true"
    });
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    resp.json::<NewsResponse>().await.map_err(|e| e.to_string())
}

async fn fetch_commands(lang: &str) -> Result<Vec<HelpCommand>, String> {
    let resp = gloo_net::http::Request::get(&format!("/api/chat/commands?lang={lang}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    resp.json::<Vec<HelpCommand>>()
        .await
        .map_err(|e| e.to_string())
}

async fn fetch_config() -> Result<ConfigResponse, String> {
    let resp = gloo_net::http::Request::get("/api/config")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    resp.json::<ConfigResponse>()
        .await
        .map_err(|e| e.to_string())
}

// ── Small browser helpers ────────────────────────────────────────────────────

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}
fn ls_get(key: &str) -> Option<String> {
    storage()?.get_item(key).ok()?
}
fn ls_set(key: &str, val: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(key, val);
    }
}
fn set_doc_attr(name: &str, val: &str) {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
    {
        let _ = el.set_attribute(name, val);
    }
}

// ── Splitter drag (resizable panels) ─────────────────────────────────────────

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
}

/// In-flight drag: which panel size is being adjusted and from where.
/// Two window listeners (registered once in `App`) read this and update `size`.
#[derive(Clone, Copy)]
struct DragState {
    axis: Axis,
    sign: f64,
    size: RwSignal<f64>,
    start_pos: f64,
    start_size: f64,
    key: &'static str,
}

fn px_from_ls(key: &str, default: f64) -> f64 {
    ls_get(key).and_then(|s| s.parse().ok()).unwrap_or(default)
}

// ── Language (saved preference, else the browser locale) ─────────────────────

/// GUI language — the engine's `LANG` setting, stamped into the page by
/// `serve_index` as `<meta name="xoksa-lang">`. One source: the same value the
/// CLI, the connection screen, and the settings app read, so no surface can
/// disagree with the others. (It used to be a per-browser `localStorage` copy,
/// which is exactly how they drifted apart.) English on anything unexpected.
fn ui_lang() -> &'static str {
    let meta = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.query_selector("meta[name=\"xoksa-lang\"]").ok().flatten())
        .and_then(|el| el.get_attribute("content"))
        .unwrap_or_default();
    if meta.trim().eq_ignore_ascii_case("ja") {
        "ja"
    } else {
        "en"
    }
}

/// Whether this browser may change the setting: a page served from loopback
/// (the operator's own machine) in a session that persists settings at all. A
/// LAN client is shown the configured language but must not rewrite settings on
/// someone else's box, and a `--private` session writes nothing by definition —
/// the server enforces both (`POST /api/config/lang` → 403 / 409).
fn lang_is_settable() -> bool {
    let loopback = web_sys::window()
        .and_then(|w| w.location().hostname().ok())
        .map(|h| h == "127.0.0.1" || h == "localhost" || h == "[::1]" || h == "::1")
        .unwrap_or(false);
    loopback && !private_session()
}

/// Whether the engine runs as a no-trace session (`--private`), stamped into the
/// page alongside the language.
fn private_session() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| {
            d.query_selector("meta[name=\"xoksa-private\"]")
                .ok()
                .flatten()
        })
        .and_then(|el| el.get_attribute("content"))
        .map(|v| v.trim() == "1")
        .unwrap_or(false)
}

/// Save the UI language to the engine's config, then reload so every string —
/// including the server-generated text — comes back in it.
fn set_ui_lang(lang: &'static str) {
    spawn_local(async move {
        if let Ok(req) = gloo_net::http::Request::post("/api/config/lang")
            .header("Content-Type", "application/json")
            .body(format!("{{\"lang\":\"{lang}\"}}"))
        {
            if let Ok(resp) = req.send().await {
                if resp.ok() {
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().reload();
                    }
                }
            }
        }
    });
}

/// Pick the Japanese or English literal for the current GUI language.
fn tr(lang: &str, ja: &'static str, en: &'static str) -> &'static str {
    if lang == "ja" {
        ja
    } else {
        en
    }
}

// ── App ──────────────────────────────────────────────────────────────────────

#[component]
fn App() -> impl IntoView {
    // GUI language: a saved preference (the language selector), else the browser
    // locale (ja → Japanese, else English). Constant for the session — the selector
    // persists the choice and reloads; also sent to every API call so server text
    // matches the chrome.
    let lang: &'static str = ui_lang();

    // Display preferences (persisted).
    let theme = RwSignal::new(ls_get("xoksa.theme").unwrap_or_else(|| "dark".into()));
    let font = RwSignal::new(ls_get("xoksa.font").unwrap_or_else(|| "m".into()));
    Effect::new(move |_| {
        let t = theme.get();
        set_doc_attr("data-theme", &t);
        ls_set("xoksa.theme", &t);
    });
    Effect::new(move |_| {
        let f = font.get();
        set_doc_attr("data-font", &f);
        ls_set("xoksa.font", &f);
    });

    // Panel sizes (persisted) + drag handling. Two window listeners registered
    // once read `drag` and update the targeted size while a gutter is held.
    // Right-column width. The default is sized to a fraction of the ACTUAL
    // window width, measured after mount (see size_right_default) — inner_width
    // at signal-init is premature (the window is not yet at its final size),
    // which left the divider right-shifted. A saved drag value always wins.
    let right_w = RwSignal::new(px_from_ls("xoksa.right_w", 480.0));
    let fund_h = RwSignal::new(px_from_ls("xoksa.fund_h", 240.0));
    let chat_h = RwSignal::new(px_from_ls("xoksa.chat_h", 230.0));
    let drag = RwSignal::<Option<DragState>>::new(None);
    StoredValue::new_local(window_event_listener(
        ev::pointermove,
        move |e: web_sys::PointerEvent| {
            if let Some(st) = drag.get_untracked() {
                let cur = match st.axis {
                    Axis::X => e.client_x() as f64,
                    Axis::Y => e.client_y() as f64,
                };
                st.size
                    .set((st.start_size + st.sign * (cur - st.start_pos)).max(80.0));
            }
        },
    ));
    StoredValue::new_local(window_event_listener(
        ev::pointerup,
        move |_e: web_sys::PointerEvent| {
            if let Some(st) = drag.get_untracked() {
                ls_set(st.key, &st.size.get_untracked().to_string());
                drag.set(None);
            }
        },
    ));
    let start_drag = move |axis: Axis, sign: f64, size: RwSignal<f64>, key: &'static str| {
        move |e: web_sys::PointerEvent| {
            e.prevent_default();
            let start_pos = match axis {
                Axis::X => e.client_x() as f64,
                Axis::Y => e.client_y() as f64,
            };
            drag.set(Some(DragState {
                axis,
                sign,
                size,
                start_pos,
                start_size: size.get_untracked(),
                key,
            }));
        }
    };

    // Query state. `tickers` = loaded set (1–5, the header chips); `symbol` = the
    // active one driving the dashboard. Persisted to localStorage; defaults come
    // from the server (/api/config) — nothing hardcoded.
    let tickers = RwSignal::<Vec<String>>::new({
        let mut v: Vec<String> = ls_get("xoksa.tickers")
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        // Stale-duplicate cleanup: drop a raw chip ("9432") when its normalized
        // ".T" form ("9432.T") is also present (older builds could persist both).
        let dot_t: Vec<String> = v.iter().filter(|x| x.ends_with(".T")).cloned().collect();
        v.retain(|c| {
            c.ends_with(".T")
                || !dot_t
                    .iter()
                    .any(|n| n.strip_suffix(".T") == Some(c.as_str()))
        });
        v
    });
    let symbol = RwSignal::new(ls_get("xoksa.symbol").unwrap_or_default());
    // `targets` = the checked subset of `tickers` chosen as the analysis / chat
    // target (1..=all). The engine analyses exactly the ticker set it is sent, so we
    // send `targets` (not the full loaded set) as its ticker set; unchecked chips
    // stay in `tickers` (front-end only) and per-symbol views (summary/chart) work
    // regardless. Defaults to the full loaded set (see the fill Effect below).
    let targets = RwSignal::<Vec<String>>::new(
        ls_get("xoksa.targets")
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
    );
    // The (symbol, tickers-csv) to send for analysis/chat: the checked `targets`,
    // with the primary = the active symbol when it is itself a target, else the
    // first target. Falls back to the full loaded set if nothing is checked.
    let analysis_scope = move || -> (String, String) {
        let tgt = targets.get_untracked();
        let active = symbol.get_untracked();
        if tgt.is_empty() {
            return (active, tickers.get_untracked().join(","));
        }
        let sym = if tgt.iter().any(|x| x.eq_ignore_ascii_case(&active)) {
            active
        } else {
            tgt[0].clone()
        };
        (sym, tgt.join(","))
    };
    // Chips the engine did not return in the most recent turn that asked for them:
    // a typo like "APPL", or a symbol neither provider could answer for. They are
    // MARKED, never removed — the chip is the user's, and a provider outage (the
    // Stooq fallback is blocked on some networks) must not delete what they typed.
    // The engine already drops such a symbol from the chat's target set, so it is
    // not re-sent; what was missing was any sign of WHICH chip to clean up.
    // Not persisted: a failure is a fact about this session, and a stale mark read
    // back from storage would accuse a symbol that may be fine now.
    let rejected = RwSignal::<Vec<String>>::new(Vec::new());
    // The target set the last turn actually sent, so the engine's echo can be read
    // as "these were asked for, those came back".
    let sent_targets = StoredValue::new_local(Vec::<String>::new());
    let add_input = RwSignal::new(String::new());
    let timeframe = RwSignal::new(String::new());
    let tick = RwSignal::new(0u32);
    // One-shot "manual refresh" flag (non-reactive): set by the ⚙ "今すぐ更新" button,
    // read + cleared by the summary fetch, so ONLY a manual refresh forces a fresh
    // fetch (bypasses the server cache + re-fetches the fundamental). The 60s auto
    // refresh never sets it.
    let force_flag = StoredValue::new_local(false);
    // Selectable LLM set for the header dropdown (mouse-only; no free-text model).
    let llm_options = RwSignal::new(Vec::<LlmOpt>::new());
    // Bumped whenever a chat turn finishes, so the dropdown re-reads the session's
    // active LLM. A `/llm` switch typed in the chat box changes the session but
    // emits no ticker/timeframe control line, so without this the header keeps its
    // stale (env-default) marker while /status and the actual model are correct.
    let llm_sync = RwSignal::new(0u32);
    // Ref to the LLM <select>, so an Effect can set its value AFTER the options
    // re-render (setting the value property while the matching <option> isn't yet
    // in the DOM makes the browser fall back to the first option — the flip).
    let llm_select_ref = NodeRef::<html::Select>::new();
    // Header "actions" dropdown selection (resets to "" after each pick).
    let action_menu = RwSignal::new(String::new());
    let auto_refresh = RwSignal::new(true);
    // ⚙ settings popover visibility — the set-once controls (auto-refresh, refresh,
    // theme, language, font) live there, off the single-row header.
    let settings_open = RwSignal::new(false);
    // Server's chat message cap (chars); filled from /api/config (single source).
    // 0 = unknown/not loaded yet → the send-time check is skipped.
    let max_msg_chars = RwSignal::new(0usize);
    // Backtest panel state + per-timeframe period tokens (from /api/config, single
    // source). The panel shows the achievable period the moment a timeframe is
    // picked — no fixed default imposed.
    let bt_periods = RwSignal::new(std::collections::HashMap::<String, Vec<String>>::new());
    // Rule-editor vocabulary served by the engine (single source; never hardcoded).
    let bt_indicators = RwSignal::new(Vec::<(String, String, String)>::new()); // (key, ja, en)
    let bt_operators = RwSignal::new(Vec::<(String, String, String)>::new()); // (key, ja, en)
    let bt_templates = RwSignal::new(Vec::<SavedStrategy>::new()); // built-in starter rules
    let bt_period_labels =
        RwSignal::new(std::collections::HashMap::<String, (String, String)>::new()); // token -> (ja,en)
    let bt_open = RwSignal::new(false);
    let bt_tf = RwSignal::new(String::new());
    let bt_period = RwSignal::new(String::new());

    // ── Alerts panel state (the alert-rule product feature; /api/alerts) ──
    let al_open = RwSignal::new(false);
    let al_rules = RwSignal::new(Vec::<AlertRuleView>::new());
    let al_channels = RwSignal::new(Vec::<AlertChannelView>::new());
    let al_msg = RwSignal::new(String::new()); // status / error line
                                               // New-rule form inputs.
    let al_ticker = RwSignal::new(String::new());
    let al_ind = RwSignal::new("rsi".to_string());
    let al_op = RwSignal::new("le".to_string());
    let al_val = RwSignal::new("30".to_string());
    let al_mode = RwSignal::new("5m".to_string());
    let al_chan = RwSignal::new(String::new());
    let al_explain = RwSignal::new(false);
    // Starting cash for the backtest, entered by the user (the unit is the stock's
    // own currency — no home-currency assumption). Default 1,000,000 as a placeholder.
    let bt_cash = RwSignal::new(String::from("1000000"));
    // Sizing: how much of the cash to buy up front (percent, default 50 = half),
    // and how much to trade on each buy/sell signal (cash amount, default 250,000).
    let bt_start_pct = RwSignal::new(String::from("50"));
    let bt_step = RwSignal::new(String::from("250000"));
    let bt_running = RwSignal::new(false);
    // Rule editor state: entry/exit condition rows + AND/OR per side.
    let bt_entry = RwSignal::new(Vec::<CondRow>::new());
    let bt_exit = RwSignal::new(Vec::<CondRow>::new());
    let bt_entry_and = RwSignal::new(true); // true=AND, false=OR
    let bt_exit_and = RwSignal::new(true);
    let bt_row_seq = StoredValue::new(0usize);
    // Factory for a new condition row (fresh signal set + unique id).
    let mk_row = move |left: &str, op: &str, rkind: &str, rval: &str| -> CondRow {
        let id = bt_row_seq.get_value();
        bt_row_seq.set_value(id + 1);
        CondRow {
            id,
            left: RwSignal::new(left.to_string()),
            op: RwSignal::new(op.to_string()),
            rkind: RwSignal::new(rkind.to_string()),
            rval: RwSignal::new(rval.to_string()),
        }
    };
    // Saved strategies (name-keyed, from /api/backtest/rules).
    let bt_saved = RwSignal::new(Vec::<SavedStrategy>::new());
    let bt_save_name = RwSignal::new(String::new());
    let bt_load_sel = RwSignal::new(String::new());
    // Serialize the current editor rows to a StrategyRules JSON object (reused by
    // Run and Save). Keys/operators are fixed dropdown values; numbers are parsed.
    let build_rules_json = move || -> String {
        let conds = |rows: Vec<CondRow>| -> String {
            rows.iter()
                .filter_map(|r| {
                    let (l, op) = (r.left.get_untracked(), r.op.get_untracked());
                    if l.is_empty() || op.is_empty() {
                        return None;
                    }
                    if r.rkind.get_untracked() == "indicator" {
                        Some(format!(
                            "{{\"left\":\"{l}\",\"op\":\"{op}\",\"right_kind\":\"indicator\",\"right\":\"{}\"}}",
                            r.rval.get_untracked()
                        ))
                    } else {
                        let v: f64 = r.rval.get_untracked().trim().parse().unwrap_or(0.0);
                        Some(format!(
                            "{{\"left\":\"{l}\",\"op\":\"{op}\",\"right_kind\":\"value\",\"value\":{v}}}"
                        ))
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        let ec = if bt_entry_and.get_untracked() {
            "and"
        } else {
            "or"
        };
        let xc = if bt_exit_and.get_untracked() {
            "and"
        } else {
            "or"
        };
        format!(
            "{{\"entry_combine\":\"{ec}\",\"entry\":[{}],\"exit_combine\":\"{xc}\",\"exit\":[{}]}}",
            conds(bt_entry.get_untracked()),
            conds(bt_exit.get_untracked())
        )
    };
    // Plain-language description of the CURRENT editor rule (name + buy/sell
    // conditions), shown in the result so it reflects the actual rule that ran.
    let describe_rules = move || -> String {
        let op_lbl = |op: &str| -> String {
            bt_operators
                .get_untracked()
                .into_iter()
                .find(|(k, _, _)| k.as_str() == op)
                .map(|(_, ja, en)| if lang == "ja" { ja } else { en })
                .unwrap_or_else(|| op.to_string())
        };
        let ind_lbl = |k: &str| -> String {
            bt_indicators
                .get_untracked()
                .into_iter()
                .find(|(kk, _, _)| kk.as_str() == k)
                .map(|(_, ja, en)| if lang == "ja" { ja } else { en })
                .unwrap_or_else(|| k.to_string())
        };
        let cond = |r: &CondRow| -> String {
            let rhs = if r.rkind.get_untracked() == "indicator" {
                ind_lbl(&r.rval.get_untracked())
            } else {
                r.rval.get_untracked()
            };
            format!(
                "{} {} {}",
                ind_lbl(&r.left.get_untracked()),
                op_lbl(&r.op.get_untracked()),
                rhs
            )
        };
        let side = |rows: Vec<CondRow>, and: bool| -> String {
            let sep = if and {
                tr(lang, " かつ ", " and ")
            } else {
                tr(lang, " または ", " or ")
            };
            let parts: Vec<String> = rows.iter().map(cond).collect();
            if parts.is_empty() {
                tr(lang, "（条件なし）", "(no condition)").to_string()
            } else {
                parts.join(sep)
            }
        };
        let name = {
            let l = bt_load_sel.get_untracked();
            if !l.trim().is_empty() {
                l
            } else {
                let s = bt_save_name.get_untracked();
                if !s.trim().is_empty() {
                    s
                } else {
                    tr(lang, "（無題）", "(untitled)").to_string()
                }
            }
        };
        let buy = side(bt_entry.get_untracked(), bt_entry_and.get_untracked());
        let sell = side(bt_exit.get_untracked(), bt_exit_and.get_untracked());
        if lang == "ja" {
            format!("ルール名: {name}\n　買い: {buy}\n　売り: {sell}")
        } else {
            format!("Rule: {name}\n　Buy: {buy}\n　Sell: {sell}")
        }
    };
    // Fetch saved rules and show them in the SAME list as the built-in templates
    // (templates first, suffixed `_template`), so both load via one operation.
    let refresh_saved = move || {
        spawn_local(async move {
            let mut list = bt_templates.get_untracked();
            if let Ok(resp) = gloo_net::http::Request::get("/api/backtest/rules")
                .send()
                .await
            {
                if let Ok(saved) = resp.json::<Vec<SavedStrategy>>().await {
                    list.extend(saved);
                }
            }
            bt_saved.set(list);
        });
    };
    // Save the current editor rules under the entered name, then refresh the list.
    let save_rule = move || {
        let name = bt_save_name.get_untracked().trim().to_string();
        if name.is_empty() {
            return;
        }
        let spec = build_rules_json();
        let body = format!(
            "{{\"name\":{},\"spec_json\":{}}}",
            serde_json::to_string(&name).unwrap_or_else(|_| "\"\"".into()),
            serde_json::to_string(&spec).unwrap_or_else(|_| "\"\"".into()),
        );
        spawn_local(async move {
            if let Ok(req) = gloo_net::http::Request::post("/api/backtest/rules")
                .header("Content-Type", "application/json")
                .body(body)
            {
                let _ = req.send().await;
            }
            // Refresh the saved list so the new name appears immediately.
            if let Ok(resp) = gloo_net::http::Request::get("/api/backtest/rules")
                .send()
                .await
            {
                if let Ok(list) = resp.json::<Vec<SavedStrategy>>().await {
                    bt_saved.set(list);
                }
            }
        });
    };
    // Load the selected saved strategy into the editor rows.
    let load_rule = move || {
        let name = bt_load_sel.get_untracked();
        let Some(saved) = bt_saved
            .get_untracked()
            .into_iter()
            .find(|s| s.name == name)
        else {
            return;
        };
        let Ok(rj) = serde_json::from_str::<RulesJson>(&saved.spec_json) else {
            return;
        };
        let to_rows = |conds: Vec<CondJson>| -> Vec<CondRow> {
            conds
                .into_iter()
                .map(|c| {
                    let rval = if c.right_kind == "indicator" {
                        c.right.unwrap_or_default()
                    } else {
                        c.value.map(|v| v.to_string()).unwrap_or_default()
                    };
                    let rkind = if c.right_kind.is_empty() {
                        "value".to_string()
                    } else {
                        c.right_kind
                    };
                    mk_row(&c.left, &c.op, &rkind, &rval)
                })
                .collect()
        };
        bt_entry_and.set(rj.entry_combine != "or");
        bt_exit_and.set(rj.exit_combine != "or");
        bt_entry.set(to_rows(rj.entry));
        bt_exit.set(to_rows(rj.exit));
    };

    // Client bootstrap config (default ticker / timeframe / selectable timeframes).
    let app_config = LocalResource::new(|| async move { fetch_config().await });
    Effect::new(move |_| {
        if let Some(Ok(cfg)) = app_config.get() {
            max_msg_chars.set(cfg.max_chat_msg_chars);
            if !cfg.backtest_periods.is_empty() {
                bt_periods.set(cfg.backtest_periods.clone());
            }
            // Rule-editor vocabulary / templates / period labels — from the engine.
            if !cfg.rule_indicators.is_empty() {
                bt_indicators.set(
                    cfg.rule_indicators
                        .iter()
                        .map(|l| (l.key.clone(), l.label_ja.clone(), l.label_en.clone()))
                        .collect(),
                );
            }
            if !cfg.rule_operators.is_empty() {
                bt_operators.set(
                    cfg.rule_operators
                        .iter()
                        .map(|l| (l.key.clone(), l.label_ja.clone(), l.label_en.clone()))
                        .collect(),
                );
            }
            if !cfg.rule_templates.is_empty() {
                bt_templates.set(
                    cfg.rule_templates
                        .iter()
                        .map(|t| SavedStrategy {
                            // The load list shows templates beside saved rules, so the
                            // name is display text; take the configured language's.
                            name: if lang == "ja" {
                                t.name_ja.clone()
                            } else {
                                t.name_en.clone()
                            },
                            spec_json: t.spec_json.clone(),
                        })
                        .collect(),
                );
            }
            if !cfg.period_labels.is_empty() {
                bt_period_labels.set(
                    cfg.period_labels
                        .iter()
                        .map(|l| (l.key.clone(), (l.label_ja.clone(), l.label_en.clone())))
                        .collect(),
                );
            }
            if tickers.get_untracked().is_empty() && !cfg.default_ticker.is_empty() {
                tickers.set(vec![cfg.default_ticker.clone()]);
            }
            if symbol.get_untracked().is_empty() {
                symbol.set(
                    tickers
                        .get_untracked()
                        .first()
                        .cloned()
                        .unwrap_or_else(|| cfg.default_ticker.clone()),
                );
            }
            if timeframe.get_untracked().is_empty() && !cfg.default_timeframe.is_empty() {
                timeframe.set(cfg.default_timeframe.clone());
            }
        }
    });
    // Persist the loaded set + active selection + checked targets.
    Effect::new(move |_| ls_set("xoksa.tickers", &tickers.get().join(",")));
    Effect::new(move |_| ls_set("xoksa.symbol", &symbol.get()));
    Effect::new(move |_| ls_set("xoksa.targets", &targets.get().join(",")));
    // Default the checked set to the full loaded set (first run / migration): only
    // acts while nothing is checked, which — with the ≥1 rule below — is init-only.
    Effect::new(move |_| {
        if targets.get().is_empty() {
            let all = tickers.get();
            if !all.is_empty() {
                targets.set(all);
            }
        }
    });

    // Header-chip ticker management (shared set with the chat — see send_chat).
    let add_ticker = move || {
        let v = add_input.get_untracked().trim().to_uppercase();
        add_input.set(String::new());
        if v.is_empty() {
            return;
        }
        tickers.update(|t| {
            if t.len() < 5 && !t.iter().any(|x| x.eq_ignore_ascii_case(&v)) {
                t.push(v.clone());
            }
        });
        // A newly added ticker is checked (a target) by default.
        targets.update(|t| {
            if !t.iter().any(|x| x.eq_ignore_ascii_case(&v)) {
                t.push(v.clone());
            }
        });
        symbol.set(v); // activate the newly added ticker
    };
    let remove_ticker = move |sym: String| {
        tickers.update(|t| t.retain(|x| !x.eq_ignore_ascii_case(&sym)));
        rejected.update(|r| r.retain(|x| !x.eq_ignore_ascii_case(&sym)));
        targets.update(|t| t.retain(|x| !x.eq_ignore_ascii_case(&sym)));
        if symbol.get_untracked().eq_ignore_ascii_case(&sym) {
            symbol.set(tickers.get_untracked().first().cloned().unwrap_or_default());
        }
        // Keep at least one target while any ticker remains.
        if targets.get_untracked().is_empty() {
            if let Some(first) = tickers.get_untracked().first().cloned() {
                targets.set(vec![first]);
            }
        }
    };
    // Toggle a chip's "analysis target" membership; never leave zero targets.
    let toggle_target = move |sym: String| {
        targets.update(|t| {
            if let Some(pos) = t.iter().position(|x| x.eq_ignore_ascii_case(&sym)) {
                if t.len() > 1 {
                    t.remove(pos);
                }
            } else {
                t.push(sym.clone());
            }
        });
    };

    // Reactive data: refetches whenever symbol / timeframe / tick changes. While
    // the symbol is still empty (config not loaded yet) we skip the network call.
    let summary = LocalResource::new(move || {
        // Read the source signals synchronously so the resource re-runs when
        // symbol / timeframe / tick change (tracked at call time, not inside the
        // async body).
        let sym = symbol.get();
        let tf = timeframe.get();
        let _ = tick.get();
        // A manual "今すぐ更新" set this; consume it so only that one fetch forces.
        let force = force_flag.get_value();
        force_flag.set_value(false);
        async move {
            if sym.trim().is_empty() {
                return Err(String::from("__init__"));
            }
            fetch_summary(sym, tf, lang, force).await
        }
    });

    // News refetches when the symbol changes (not on the periodic tick) to
    // avoid hammering the Brave API, or when a free-text search is submitted.
    // `news_search` is the search box text; `news_include_ticker` mirrors the
    // "銘柄情報を含む" checkbox; `news_query` is the submitted search (None =
    // the default ticker-derived news).
    let news_search = RwSignal::new(String::new());
    let news_include_ticker = RwSignal::new(false);
    let news_query = RwSignal::new(Option::<String>::None);
    // "フィルタを外す" checkbox: false = finance-relevance filter on (the default),
    // true = raw / all results.
    let news_filter_off = RwSignal::new(false);
    let news = LocalResource::new(move || {
        let sym = symbol.get();
        let q = news_query.get();
        let filter_off = news_filter_off.get();
        async move {
            if sym.trim().is_empty() {
                return Err(String::from("__init__"));
            }
            fetch_news(sym, lang, q, filter_off).await
        }
    });
    let news_h = RwSignal::new(px_from_ls("xoksa.news_h", 260.0));

    // Keep the market panel visible at startup: fund+news must never fill the
    // whole right column (a saved/default height taller than the window would
    // otherwise clip market away). A CSS `max-height:%` cannot guard this — it
    // does not resolve against a stretch-sized flex parent in WKWebView — so we
    // measure the right column and shrink fund/news to reserve room for market.
    // Shrink-only and runtime-only (does not overwrite the saved preferences).
    let right_ref = NodeRef::<html::Div>::new();
    let clamp_right = move || {
        let Some(el) = right_ref.get_untracked() else {
            return;
        };
        let avail = el.client_height() as f64;
        if avail < 120.0 {
            return;
        }
        const GUTTERS: f64 = 16.0;
        const MARKET_MIN: f64 = 120.0;
        const FUND_MIN: f64 = 100.0;
        const NEWS_MIN: f64 = 150.0;
        let budget = (avail - GUTTERS - MARKET_MIN).max(FUND_MIN + NEWS_MIN);
        let (f, n) = (fund_h.get_untracked(), news_h.get_untracked());
        if f + n > budget {
            // Shrink fundamentals first (a scrollable reference list); protect the
            // news panel so a couple of headlines stay visible in a short window.
            let fund_new = (budget - n).max(FUND_MIN);
            let news_new = (budget - fund_new).max(NEWS_MIN);
            fund_h.set(fund_new);
            news_h.set(news_new);
        }
    };
    // Default right-column width = a fraction of the real window width, applied
    // after mount (and on resize) unless the user has dragged (a saved value in
    // localStorage). Fresh ls check distinguishes an auto-size from a real drag.
    let size_right_default = move || {
        if ls_get("xoksa.right_w").is_some() {
            return;
        }
        if let Some(w) = web_sys::window()
            .and_then(|w| w.inner_width().ok())
            .and_then(|v| v.as_f64())
        {
            right_w.set((w * 0.52).clamp(360.0, 900.0));
        }
    };
    Effect::new(move |_| {
        right_ref.get(); // reactive: re-measure once the column mounts
        request_animation_frame(move || {
            size_right_default();
            clamp_right();
        });
    });
    StoredValue::new_local(window_event_listener(ev::resize, move |_| {
        size_right_default();
        clamp_right();
    }));

    // Chat-command help (loaded once; shown in a separate popup window).
    let commands = LocalResource::new(move || async move { fetch_commands(lang).await });

    // Periodic auto-refresh: bump `tick` every 60s while enabled. 60s matches
    // Yahoo's intraday data granularity (~1 minute) and the server's
    // SUMMARY_CACHE_TTL — polling faster changes nothing, and a poll inside the
    // TTL is answered from the server cache (from_cache). What the user tracks is
    // the shown data time (meta.market_data_latest_time), which advances only when
    // Yahoo does; this internal cadence is not surfaced.
    let interval = set_interval_with_handle(
        move || {
            if auto_refresh.get_untracked() {
                tick.update(|t| *t += 1);
            }
        },
        std::time::Duration::from_secs(60),
    )
    .ok();
    StoredValue::new_local(interval); // keep alive for the app's lifetime

    // 市況データは基本データと同じ `summary`（単一SOTスナップショット）から描画する。
    // 独自にバーを数えて蓄積する市況ログは廃止（別タイミング取得＝SOT違反の原因）。復活させない。

    // Chart series for the popup: the actual market-data bars (one point per bar,
    // not per poll), fetched from `/api/symbol/{symbol}/chart` so the chart matches
    // the market data and its point count equals the bar count. Refetched on each
    // auto-reload while the popup is open.
    let chart_points = RwSignal::new(Vec::<(String, ChartSample)>::new());
    // Bar label (e.g. "5分足") of the active series, for the chart header.
    let chart_tf = RwSignal::new(String::new());
    // Which chart series are visible (price is always shown). State lives here; the
    // popup's checkboxes postMessage toggles back.
    let show_vwap = RwSignal::new(true);
    let show_ema = RwSignal::new(true);
    let show_sma = RwSignal::new(false);
    let show_bb = RwSignal::new(false);
    let show_rsi = RwSignal::new(true);
    let show_macd = RwSignal::new(true);
    let show_vol = RwSignal::new(true);
    let chart_toggles = move || ChartToggles {
        vwap: show_vwap.get(),
        ema: show_ema.get(),
        sma: show_sma.get(),
        bb: show_bb.get(),
        rsi: show_rsi.get(),
        macd: show_macd.get(),
        vol: show_vol.get(),
    };
    // Chat transcript (defined early so the chart brush can reference it).
    let messages = RwSignal::new(Vec::<(String, String)>::new());
    // A chart interval the user brushed, held as REFERENCE context (timeframe
    // label, from, to, confirmed-value summary) to attach to their next chat
    // message — not an auto-explanation. Cleared after send (or via the chip's ×).
    let attached_range = RwSignal::new(None::<(String, String, String, String)>);
    // Desktop: the chart is a separate native window with no shared JS realm, so its
    // brush-select arrives as a `chart-brush` Tauri event carrying the interval —
    // adopt it as the attached reference, same as an in-realm brush would.
    if is_tauri() {
        tauri_listen("chart-brush", move |ev| {
            use wasm_bindgen::JsValue;
            let get = |o: &JsValue, k: &str| js_sys::Reflect::get(o, &JsValue::from_str(k)).ok();
            let s =
                |o: &JsValue, k: &str| get(o, k).and_then(|v| v.as_string()).unwrap_or_default();
            if let Some(p) = get(&ev, "payload") {
                let summary = s(&p, "summary");
                if !summary.is_empty() {
                    attached_range.set(Some((s(&p, "tf"), s(&p, "from"), s(&p, "to"), summary)));
                }
            }
        });
    }
    let chart_win = StoredValue::new_local(None::<web_sys::Window>);
    // Fetch the per-bar series from the backend (market data + per-bar indicators,
    // SOT) and replace the chart points. Called on open and on each auto-reload
    // while the popup is open.
    let refresh_chart = move || {
        let sym = symbol.get_untracked();
        let tf = timeframe.get_untracked();
        if sym.is_empty() {
            return;
        }
        spawn_local(async move {
            let enc = js_sys::encode_uri_component(&sym)
                .as_string()
                .unwrap_or_default();
            let url = format!("/api/symbol/{enc}/chart?timeframe={tf}&bars=120&lang={lang}");
            if let Ok(resp) = gloo_net::http::Request::get(&url).send().await {
                if let Ok(cr) = resp.json::<ChartResp>().await {
                    chart_tf.set(cr.timeframe_label);
                    chart_points.set(
                        cr.bars
                            .into_iter()
                            .map(|b| {
                                (
                                    b.t,
                                    ChartSample {
                                        price: b.price,
                                        vwap: b.vwap,
                                        ema_s: b.ema_s,
                                        ema_l: b.ema_l,
                                        sma_s: b.sma_s,
                                        sma_l: b.sma_l,
                                        bb_u: b.bb_u,
                                        bb_l: b.bb_l,
                                        rsi: b.rsi,
                                        macd: b.macd,
                                        volume: b.volume,
                                    },
                                )
                            })
                            .collect(),
                    );
                }
            }
        });
    };
    // Keep the chart fresh: when a poll completes and the popup is open, refetch.
    Effect::new(move |_| {
        summary.get(); // tie the refresh to the auto-reload cadence
        let open = chart_win.with_value(|w| {
            w.as_ref()
                .map(|win| !win.closed().unwrap_or(true))
                .unwrap_or(false)
        });
        if open {
            refresh_chart();
        }
    });

    // Market chart popup (separate window, like Help). Fetches the bar series on
    // open, then live-updates on each auto-reload.
    // Brush-select on the chart popup → explain that interval. The chart is a
    // separate window with static injected SVG, so we attach mouse listeners from
    // here onto its <svg> (same-origin). Kept alive in `brush_keep`, re-attached
    // after each render (the SVG is recreated on every refresh).
    let brush_keep = StoredValue::new_local(Vec::<Closure<dyn FnMut(web_sys::MouseEvent)>>::new());
    let attach_brush = move |win: &web_sys::Window| {
        const VIEW_W: f64 = 860.0;
        const PAD_L: f64 = 64.0;
        const X_RIGHT: f64 = 816.0;
        let Some(doc) = win.document() else { return };
        let Some(svg) = doc.query_selector("svg").ok().flatten() else {
            return;
        };
        let start = std::rc::Rc::new(std::cell::Cell::new(Option::<f64>::None));

        // svg-x (in viewBox units) from a mouse event. The chart scales to fit the
        // window with `preserveAspectRatio="xMidYMid meet"`, i.e. it is uniformly
        // scaled and CENTERED (letterboxed) inside the element. Compute that scale
        // and centering offset directly from the element rect and the viewBox, so
        // the cursor maps exactly to the drawing (no browser-CTM dependency).
        let svg_x = {
            let svg = svg.clone();
            move |ev: &web_sys::MouseEvent| -> f64 {
                let r = svg.get_bounding_client_rect();
                // viewBox = "0 0 W H" → the drawing's user-space dimensions.
                let (vw, vh) = svg
                    .get_attribute("viewBox")
                    .and_then(|vb| {
                        let mut it = vb.split_whitespace().skip(2);
                        Some((
                            it.next()?.parse::<f64>().ok()?,
                            it.next()?.parse::<f64>().ok()?,
                        ))
                    })
                    .unwrap_or((VIEW_W, VIEW_W));
                let (ew, eh) = (r.width().max(1.0), r.height().max(1.0));
                let scale = (ew / vw).min(eh / vh); // uniform "meet" scale
                let off_x = (ew - vw * scale) / 2.0; // horizontal letterbox margin
                (((ev.client_x() as f64) - r.left() - off_x) / scale).clamp(0.0, vw)
            }
        };
        // Draw/update the selection rectangle spanning all panels.
        let set_rect = {
            let (svg, doc) = (svg.clone(), doc.clone());
            move |x0: f64, x1: f64| {
                let (lo, hi) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
                let el = match svg.query_selector("#xk-brush").ok().flatten() {
                    Some(e) => e,
                    None => {
                        let Ok(e) =
                            doc.create_element_ns(Some("http://www.w3.org/2000/svg"), "rect")
                        else {
                            return;
                        };
                        let _ = e.set_attribute("id", "xk-brush");
                        let _ = e.set_attribute("fill", "rgba(120,150,255,0.16)");
                        let _ = e.set_attribute("stroke", "#6f8fff");
                        let _ = e.set_attribute("stroke-dasharray", "3 3");
                        let _ = e.set_attribute("pointer-events", "none");
                        let _ = e.set_attribute("y", "0");
                        let _ = e.set_attribute("height", "5000");
                        let _ = svg.append_child(&e);
                        e
                    }
                };
                let _ = el.set_attribute("x", &format!("{lo:.1}"));
                let _ = el.set_attribute("width", &format!("{:.1}", (hi - lo).max(0.0)));
            }
        };
        let clear_rect = {
            let svg = svg.clone();
            move || {
                if let Some(e) = svg.query_selector("#xk-brush").ok().flatten() {
                    e.remove();
                }
            }
        };

        let down = {
            let (start, svg_x) = (start.clone(), svg_x.clone());
            Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |ev: web_sys::MouseEvent| {
                start.set(Some(svg_x(&ev)));
            })
        };
        let mv = {
            let (start, svg_x, set_rect) = (start.clone(), svg_x.clone(), set_rect.clone());
            Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |ev: web_sys::MouseEvent| {
                if let Some(x0) = start.get() {
                    set_rect(x0, svg_x(&ev));
                }
            })
        };
        let up = {
            let (start, svg_x, clear_rect) = (start.clone(), svg_x.clone(), clear_rect.clone());
            Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |ev: web_sys::MouseEvent| {
                let Some(x0) = start.take() else { return };
                let x1 = svg_x(&ev);
                clear_rect();
                let pts = chart_points.get_untracked();
                let n = pts.len();
                if n < 2 {
                    return;
                }
                let to_i = |x: f64| {
                    (((x - PAD_L) / (X_RIGHT - PAD_L)) * ((n - 1) as f64))
                        .round()
                        .clamp(0.0, (n - 1) as f64) as usize
                };
                let (mut a, mut b) = (to_i(x0), to_i(x1));
                if a > b {
                    std::mem::swap(&mut a, &mut b);
                }
                if b - a < 1 {
                    return; // need at least two bars
                }
                let (from, to) = (pts[a].0.clone(), pts[b].0.clone());
                // Attach the interval's confirmed values as REFERENCE for the user's
                // next message — do NOT auto-explain. The user writes their comment;
                // on send, this summary is prepended as grounding.
                let summary = range_summary(&pts[a..=b], chart_toggles());
                let tf = {
                    let l = chart_tf.get_untracked();
                    if l.is_empty() {
                        timeframe.get_untracked()
                    } else {
                        l
                    }
                };
                attached_range.set(Some((tf, from, to, summary)));
            })
        };

        let _ = svg.add_event_listener_with_callback("mousedown", down.as_ref().unchecked_ref());
        let _ = svg.add_event_listener_with_callback("mousemove", mv.as_ref().unchecked_ref());
        let _ = svg.add_event_listener_with_callback("mouseup", up.as_ref().unchecked_ref());
        brush_keep.set_value(vec![down, mv, up]);
    };

    let open_chart = move |_| {
        // Desktop (Tauri): open a real OS window. `window.open("")` returns null in
        // the desktop WebView, so the browser popup path below cannot be used there.
        if is_tauri() {
            // Desktop: open a native window pointed at the loopback dashboard in popup
            // mode (?popup=chart). It self-renders and live-updates on its own — exactly
            // like the browser popup — so nothing is pushed into it from here.
            tauri_invoke(
                "open_popup_window",
                popup_args(
                    "chart",
                    &symbol.get_untracked(),
                    &timeframe.get_untracked(),
                    lang,
                ),
            );
            return;
        }
        refresh_chart();
        let html = chart_doc_html(
            &chart_points.get_untracked(),
            &symbol.get_untracked(),
            &chart_tf.get_untracked(),
            chart_toggles(),
            lang,
        );
        if let Some(w) = web_sys::window() {
            if let Ok(Some(popup)) = w.open_with_url_and_target_and_features(
                "",
                "xoksa-chart",
                "popup=yes,width=860,height=560,scrollbars=yes,resizable=yes",
            ) {
                render_chart_popup(&popup, &html);
                // Wire the toggle listener ONCE per popup document (marker survives
                // innerHTML re-renders). The delegate flips the series signals
                // DIRECTLY here in the opener realm — no postMessage.
                if let Some(doc) = popup.document() {
                    if let Some(root) = doc.document_element() {
                        let token = page_token();
                        if root.get_attribute("data-xoksa-wired").as_deref() != Some(&token) {
                            let _ = root.set_attribute("data-xoksa-wired", &token);
                            attach_popup_click_delegate(&doc, "data-toggle", move |key| match key
                                .as_str()
                            {
                                "vwap" => show_vwap.update(|b| *b = !*b),
                                "ema" => show_ema.update(|b| *b = !*b),
                                "sma" => show_sma.update(|b| *b = !*b),
                                "bb" => show_bb.update(|b| *b = !*b),
                                "rsi" => show_rsi.update(|b| *b = !*b),
                                "macd" => show_macd.update(|b| *b = !*b),
                                "vol" => show_vol.update(|b| *b = !*b),
                                _ => {}
                            });
                        }
                    }
                }
                attach_brush(&popup);
                let _ = popup.focus();
                chart_win.set_value(Some(popup));
            }
        }
    };
    Effect::new(move |_| {
        chart_points.get(); // re-render when new points arrive…
        let toggles = chart_toggles(); // …or when a toggle changes
        chart_win.with_value(|w| {
            if let Some(win) = w {
                if !win.closed().unwrap_or(true) {
                    let html = chart_doc_html(
                        &chart_points.get_untracked(),
                        &symbol.get_untracked(),
                        &chart_tf.get_untracked(),
                        toggles,
                        lang,
                    );
                    render_chart_popup(win, &html);
                    attach_brush(win);
                }
            }
        });
    });

    // Chat.
    let chat_input = RwSignal::new(String::new());
    // Ref to the chat input (a multi-line textarea) so the Help popup can pre-fill
    // + focus it, and so we can auto-grow / read the caret position.
    let chat_input_ref = NodeRef::<html::Textarea>::new();
    // Desktop: the help window is a separate native window, so a clicked command
    // arrives as a `help-pick` Tauri event — insert it into the chat input + focus,
    // exactly as the in-realm browser popup's `on_pick` would.
    if is_tauri() {
        route_external_links_to_os();
        tauri_listen("help-pick", move |ev| {
            use wasm_bindgen::JsValue;
            let get = |o: &JsValue, k: &str| js_sys::Reflect::get(o, &JsValue::from_str(k)).ok();
            let cmd = get(&ev, "payload")
                .and_then(|p| get(&p, "cmd"))
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            if !cmd.is_empty() {
                chat_input.set(cmd);
                if let Some(el) = chat_input_ref.get() {
                    let _ = el.focus();
                }
            }
        });
    }
    // Command history (terminal-style ↑/↓ recall). `hist_idx` = None while editing
    // a fresh line; Some(i) points at the recalled entry.
    let history = RwSignal::new(Vec::<String>::new());
    let hist_idx = RwSignal::<Option<usize>>::new(None);
    // True while a request is in flight (disables send, blocks overlap).
    let pending = RwSignal::new(false);
    // Streaming/typewriter state. SSE appends lines to `stream_buf`; a timer
    // reveals it character-by-character up to `stream_pos` (chars). `stream_done`
    // = server finished sending; once revealed it is committed to `messages`.
    let stream_buf = RwSignal::new(String::new());
    let stream_pos = RwSignal::new(0usize);
    let stream_on = RwSignal::new(false);
    let stream_done = RwSignal::new(false);
    // Generation guard for the in-flight stream: bumped on each new send and on
    // cancel, so a superseded/cancelled SSE task stops writing and never commits.
    let stream_gen = RwSignal::new(0u32);
    // Refs to the scroll containers so new content auto-scrolls to the bottom.
    let chat_log_ref = NodeRef::<html::Div>::new();

    // Typewriter timer: reveal characters of the in-flight stream. Adaptive speed
    // — true char-by-char for short replies, faster catch-up for long transcripts
    // so the reader is never kept waiting after the data has arrived.
    let reveal = set_interval_with_handle(
        move || {
            if !stream_on.get_untracked() {
                return;
            }
            let total = stream_buf.with_untracked(|s| s.chars().count());
            let pos = stream_pos.get_untracked();
            if pos < total {
                let remaining = total - pos;
                let step = (remaining / 18).max(1);
                stream_pos.set((pos + step).min(total));
            } else if stream_done.get_untracked() {
                // Fully revealed and the server is done → commit to history.
                let text = stream_buf.get_untracked();
                if !text.is_empty() {
                    messages.update(|m| m.push(("assistant".into(), text)));
                }
                stream_on.set(false);
                stream_done.set(false);
                stream_buf.set(String::new());
                stream_pos.set(0);
                pending.set(false);
            }
        },
        std::time::Duration::from_millis(28),
    )
    .ok();
    StoredValue::new_local(reveal);

    // Auto-scroll the chat log to the newest message (after the DOM updates).
    Effect::new(move |_| {
        messages.get();
        stream_pos.get();
        stream_on.get();
        request_animation_frame(move || {
            if let Some(el) = chat_log_ref.get() {
                el.set_scroll_top(el.scroll_height());
            }
        });
    });

    // Open the command reference in a SEPARATE popup window. Clicking a command
    // row runs `on_pick` here in the opener realm (no postMessage), pre-filling the
    // chat input and focusing it.
    let open_help = move |_| {
        // Desktop (Tauri): open a real OS window (the WebView blocks `window.open`).
        if is_tauri() {
            tauri_invoke("open_popup_window", popup_args("help", "", "", lang));
            return;
        }
        let list = match commands.get() {
            Some(Ok(l)) => l,
            _ => Vec::new(),
        };
        open_help_window(&list, lang, move |cmd| {
            chat_input.set(cmd);
            if let Some(el) = chat_input_ref.get() {
                let _ = el.focus();
            }
        });
    };
    // Multi-timeframe analysis preset: POST a fixed spec (monthly MACD + daily
    // Bollinger + 5m price) for the active symbol; the server builds the context
    // pack and has the LLM interpret it. The pack + answer are appended to the chat.
    let open_mtf = move || {
        let sym = symbol.get_untracked();
        if sym.is_empty() {
            return;
        }
        let label = tr(
            lang,
            "📐 マルチタイムフレーム分析（月足MACD ＋ 日足ボリンジャー ＋ 5分足価格）",
            "📐 Multi-timeframe (monthly MACD + daily Bollinger + 5m price)",
        );
        messages.update(|m| m.push(("user".into(), label.to_string())));
        // Send the header timeframe so the server runs the LLM with the same
        // provider/model a `/llm` switch chose in chat (session is keyed by it).
        let tf = timeframe.get_untracked();
        let body = format!(
            "{{\"symbol\":\"{sym}\",\"lang\":\"{lang}\",\"timeframe\":\"{tf}\",\"contexts\":[\
               {{\"type\":\"indicator\",\"timeframe\":\"monthly\",\"indicator_name\":\"MACD\"}},\
               {{\"type\":\"indicator\",\"timeframe\":\"daily\",\"indicator_name\":\"BBANDS\"}},\
               {{\"type\":\"price\",\"timeframe\":\"5m\"}}]}}"
        );
        spawn_local(async move {
            let result = async {
                let req = gloo_net::http::Request::post("/api/analysis/multi-timeframe")
                    .header("Content-Type", "application/json")
                    .body(body)
                    .map_err(|e| e.to_string())?;
                req.send()
                    .await
                    .map_err(|e| e.to_string())?
                    .json::<MtfResp>()
                    .await
                    .map_err(|e| e.to_string())
            }
            .await;
            let text = match result {
                // Pack = deterministic data; the prose is the LLM's commentary. The
                // "🧠 …が解説:" badge is built server-side (shared source), so the UI
                // just places it above the commentary — no client-side formatting.
                Ok(r) if !r.context_pack_text.is_empty() => {
                    if r.llm_ok && !r.badge.is_empty() {
                        format!("{}\n\n{}\n{}", r.context_pack_text, r.badge, r.response)
                    } else {
                        format!("{}\n\n{}", r.context_pack_text, r.response)
                    }
                }
                Ok(r) => r.response,
                Err(e) => format!("MTF error: {e}"),
            };
            messages.update(|m| m.push(("assistant".into(), text)));
        });
    };
    // Open the backtest panel. The user picks timeframe + period there; the panel
    // shows the achievable period for the selected timeframe (from /api/config) —
    // no fixed default is imposed. Defaults the panel's timeframe to the header's
    // and the period to that timeframe's longest available.
    let open_backtest = move || {
        // Start from the preset rule so the user has something to edit, not blank.
        if bt_entry.get_untracked().is_empty() {
            bt_entry.set(vec![mk_row("score", "ge", "value", "2")]);
        }
        if bt_exit.get_untracked().is_empty() {
            bt_exit.set(vec![mk_row("score", "le", "value", "-1")]);
        }
        refresh_saved();
        // Open unconditionally; the timeframe/period are initialized by the Effect
        // below. Previously this read `bt_periods` at click time and bailed out (or
        // opened with an empty period) if `/api/config` had not resolved yet — so a
        // click right after page load did nothing until a reload. run_backtest_now
        // still validates the symbol before running.
        bt_open.set(true);
    };
    // Initialize (or repair) the backtest timeframe/period whenever the panel is
    // open and the period map is known. This fires both when the panel opens and
    // when `/api/config` resolves afterwards, so opening before config has loaded
    // self-heals instead of leaving the panel unusable. It only sets defaults when
    // the current timeframe is unset/invalid, so it never clobbers a user's pick.
    Effect::new(move |_| {
        if !bt_open.get() {
            return;
        }
        let periods = bt_periods.get();
        if periods.is_empty() {
            return;
        }
        let cur_tf = bt_tf.get_untracked();
        if cur_tf.is_empty() || !periods.contains_key(&cur_tf) {
            let header_tf = timeframe.get_untracked();
            let init_tf = if periods.contains_key(&header_tf) {
                header_tf
            } else {
                periods
                    .keys()
                    .next()
                    .cloned()
                    .unwrap_or_else(|| "weekly".into())
            };
            let init_period = periods
                .get(&init_tf)
                .and_then(|v| v.last().cloned())
                .unwrap_or_default();
            bt_tf.set(init_tf);
            bt_period.set(init_period);
        }
    });
    // Run the backtest with the panel's timeframe + period; append the result to
    // the chat and close the panel.
    let run_backtest_now = move || {
        let sym = symbol.get_untracked();
        if sym.is_empty() {
            return;
        }
        let (tf, period) = (bt_tf.get_untracked(), bt_period.get_untracked());
        // Parse the entered cash; fall back to 1,000,000 if blank/invalid, floored at 1.
        let parse_num = |s: String, min: f64, fallback: f64| -> f64 {
            s.replace([',', ' ', '_'], "")
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v >= min)
                .unwrap_or(fallback)
        };
        let cash = parse_num(bt_cash.get_untracked(), 1.0, 1_000_000.0);
        // Percent (0–100) → fraction (0–1); step is a cash amount.
        let start_frac =
            (parse_num(bt_start_pct.get_untracked(), 0.0, 50.0) / 100.0).clamp(0.0, 1.0);
        let step = parse_num(bt_step.get_untracked(), 0.0, cash * 0.25);
        bt_running.set(true);
        // The rule echo is UI-built (it describes the user's own editor input) and
        // sent to the engine, which renders the whole report (SOT). JSON-escape it.
        let rule_desc = describe_rules()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");
        let body = format!(
            "{{\"symbol\":\"{sym}\",\"timeframe\":\"{tf}\",\"period\":\"{period}\",\"initial_cash\":{cash},\"start_fraction\":{start_frac},\"step_cash\":{step},\"rules\":{},\"lang\":\"{lang}\",\"rule_desc\":\"{}\"}}",
            build_rules_json(),
            rule_desc
        );
        spawn_local(async move {
            let result = async {
                let req = gloo_net::http::Request::post("/api/backtest")
                    .header("Content-Type", "application/json")
                    .body(body)
                    .map_err(|e| e.to_string())?;
                req.send()
                    .await
                    .map_err(|e| e.to_string())?
                    .json::<BacktestResp>()
                    .await
                    .map_err(|e| e.to_string())
            }
            .await;
            let text = match result {
                Ok(r) if !r.note.is_empty() => format!("⚠️ {}", r.note),
                Ok(r) => r.report.join("\n"), // engine-rendered report (SOT), shown verbatim
                Err(e) => format!("Backtest error: {e}"),
            };
            messages.update(|m| m.push(("assistant".into(), text)));
            bt_running.set(false);
            bt_open.set(false);
        });
    };
    // Render one side (buy or sell) of the rule editor: an AND/OR combiner plus a
    // list of editable condition rows (indicator → operator → value|indicator),
    // with add/remove. Called for both the entry and exit sides.
    let render_side = move |title: &'static str,
                            rows: RwSignal<Vec<CondRow>>,
                            and_sig: RwSignal<bool>|
          -> AnyView {
        view! {
            <div class="bt-cond">
                <div class="bt-cond-head">
                    <b>{title}</b>
                    <select class="field"
                        on:change=move |ev| and_sig.set(event_target_value(&ev) == "and")>
                        <option value="and" selected=move || and_sig.get()>{tr(lang, "すべて満たす(AND)", "all (AND)")}</option>
                        <option value="or" selected=move || !and_sig.get()>{tr(lang, "いずれか満たす(OR)", "any (OR)")}</option>
                    </select>
                </div>
                {move || rows.get().into_iter().map(|row| view! {
                    <div class="bt-row2">
                        <select class="field"
                            on:change=move |ev| row.left.set(event_target_value(&ev))>
                            {bt_indicators.get().into_iter().map(|(k, ja, en)| {
                                let kk = k.clone();
                                let label = if lang == "ja" { ja } else { en };
                                view! { <option value=k selected=move || row.left.get() == kk>{label}</option> }
                            }).collect_view()}
                        </select>
                        <select class="field bt-op"
                            on:change=move |ev| row.op.set(event_target_value(&ev))>
                            {bt_operators.get().into_iter().map(|(k, ja, en)| {
                                let kk = k.clone();
                                let label = if lang == "ja" { ja } else { en };
                                view! { <option value=k selected=move || row.op.get() == kk>{label}</option> }
                            }).collect_view()}
                        </select>
                        <select class="field bt-op"
                            on:change=move |ev| row.rkind.set(event_target_value(&ev))>
                            <option value="value" selected=move || row.rkind.get() == "value">{tr(lang, "数値", "value")}</option>
                            <option value="indicator" selected=move || row.rkind.get() == "indicator">{tr(lang, "指標", "indicator")}</option>
                        </select>
                        {move || if row.rkind.get() == "indicator" {
                            view! { <select class="field"
                                on:change=move |ev| row.rval.set(event_target_value(&ev))>
                                {bt_indicators.get().into_iter().map(|(k, ja, en)| {
                                    let kk = k.clone();
                                    let label = if lang == "ja" { ja } else { en };
                                    view! { <option value=k selected=move || row.rval.get() == kk>{label}</option> }
                                }).collect_view()}
                            </select> }.into_any()
                        } else {
                            view! { <input class="field bt-num" type="number" step="any"
                                prop:value=move || row.rval.get()
                                on:input=move |ev| row.rval.set(event_target_value(&ev)) /> }.into_any()
                        }}
                        <button class="btn bt-x" title=tr(lang, "削除", "Remove")
                            on:click=move |_| rows.update(|v| v.retain(|r| r.id != row.id))>"×"</button>
                    </div>
                }).collect_view()}
                <button class="btn bt-add"
                    on:click=move |_| rows.update(|v| v.push(mk_row("close", "gt", "value", "0")))>
                    {tr(lang, "＋条件を追加", "+ add condition")}</button>
            </div>
        }
        .into_any()
    };
    // (The Help/Chart popups used to `postMessage` back to this window; they now run
    // their picks/toggles via a direct delegated listener — see
    // `attach_popup_click_delegate` — so no `message` receiver is needed. Removing it
    // also removes the cross-origin postMessage injection surface entirely.)
    // Cancel the in-flight chat request. Invalidates the running SSE task via the
    // generation guard (so it stops writing / never commits), keeps whatever was
    // already received as an assistant bubble (no data loss — mirrors the reveal
    // timer's commit), and returns the UI to idle so a new message can be sent.
    let cancel_chat = move || {
        if !pending.get_untracked() {
            return;
        }
        stream_gen.update(|g| *g = g.wrapping_add(1));
        let text = stream_buf.get_untracked();
        if !text.is_empty() {
            messages.update(|m| m.push(("assistant".into(), text)));
        }
        stream_on.set(false);
        stream_done.set(false);
        stream_buf.set(String::new());
        stream_pos.set(0);
        pending.set(false);
    };

    let send_chat = move || {
        // Block overlapping sends while a request is in flight.
        if pending.get_untracked() {
            return;
        }
        let msg = chat_input.get().trim().to_string();
        if msg.is_empty() {
            return;
        }
        // A brushed chart interval, if attached, becomes REFERENCE grounding for
        // this message: the LLM sees the interval's confirmed values + the user's
        // comment; the bubble shows the user's text with a small 📎 note.
        let attach = attached_range.get_untracked();
        // Name the symbol in the interval block so the confirmed values are
        // unambiguously tied to the brushed chart's active symbol. The analysis
        // itself covers the checked `targets` set (see analysis_scope).
        let cur_sym = symbol.get_untracked();
        let (display_msg, server_msg) = match &attach {
            Some((tf, from, to, summary)) => (
                format!(
                    "{msg}\n📎 {cur_sym} {tf} {} {from} 〜 {to}",
                    tr(lang, "区間", "interval")
                ),
                if lang == "ja" {
                    format!("【選択区間 {cur_sym} {tf} {from}〜{to} の確定値（全足）】\n{summary}\n\n{msg}")
                } else {
                    format!("[Selected interval {cur_sym} {from}–{to} ({tf}) — confirmed values (all bars)]\n{summary}\n\n{msg}")
                },
            ),
            None => (msg.clone(), msg.clone()),
        };
        // Enforce the server's message cap BEFORE sending: a GET-SSE with an
        // over-long interval block is rejected / never established, so the chat
        // would silently do nothing. Tell the user to narrow the range, and keep
        // the attachment + input so they can re-brush and resend.
        let cap = max_msg_chars.get_untracked();
        let n = server_msg.chars().count();
        if cap > 0 && n > cap {
            let warn = if attach.is_some() {
                if lang == "ja" {
                    format!("選択区間が大きすぎて送れません（{n}/{cap}文字）。チャートで範囲（足数）を狭めてから、もう一度送ってください。")
                } else {
                    format!("The selected interval is too large to send ({n}/{cap} chars). Narrow the range (fewer bars) on the chart, then send again.")
                }
            } else if lang == "ja" {
                format!("メッセージが長すぎます（{n}/{cap}文字）。短くしてください。")
            } else {
                format!("Message too long ({n}/{cap} chars). Please shorten it.")
            };
            messages.update(|m| m.push(("assistant".into(), warn)));
            return;
        }
        attached_range.set(None);
        // Record in history (skip consecutive duplicates), reset recall cursor.
        history.update(|h| {
            if h.last().map(|s| s.as_str()) != Some(msg.as_str()) {
                h.push(msg.clone());
            }
        });
        hist_idx.set(None);
        messages.update(|m| m.push(("user".into(), display_msg)));
        chat_input.set(String::new());
        // Reset the auto-grown textarea back to one row (set() doesn't fire on:input).
        if let Some(el) = chat_input_ref.get() {
            let ta: &web_sys::HtmlTextAreaElement = &el;
            let _ = web_sys::HtmlElement::style(ta).set_property("height", "auto");
        }
        // Begin a fresh stream; the reveal timer typewrites stream_buf into a
        // bubble and commits it to `messages` when done. Bump the generation and
        // capture it so this task can detect being superseded/cancelled.
        stream_gen.update(|g| *g = g.wrapping_add(1));
        let my_gen = stream_gen.get_untracked();
        pending.set(true);
        stream_buf.set(String::new());
        stream_pos.set(0);
        stream_done.set(false);
        stream_on.set(true);
        // Stream from /api/chat/stream (SSE): each output line arrives the moment
        // the server produces it (e.g. each forum round). Same unified dispatch as
        // the CLI server-side.
        let (sym, tks) = analysis_scope();
        sent_targets.set_value(
            tks.split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect(),
        );
        let tf = timeframe.get();
        let url = format!(
            "/api/chat/stream?message={}&symbol={}&tickers={}&timeframe={}&lang={}",
            js_sys::encode_uri_component(&server_msg)
                .as_string()
                .unwrap_or_default(),
            js_sys::encode_uri_component(&sym)
                .as_string()
                .unwrap_or_default(),
            js_sys::encode_uri_component(&tks)
                .as_string()
                .unwrap_or_default(),
            js_sys::encode_uri_component(&tf)
                .as_string()
                .unwrap_or_default(),
            lang,
        );
        spawn_local(async move {
            use futures::StreamExt;
            let mut error: Option<String> = None;
            match gloo_net::eventsource::futures::EventSource::new(&url) {
                Ok(mut es) => match es.subscribe("message") {
                    Ok(mut sub) => {
                        while let Some(ev) = sub.next().await {
                            // Superseded by a newer send or cancelled → stop and
                            // close the connection; the current turn's state has
                            // already been reset/committed by the newer path.
                            if stream_gen.get_untracked() != my_gen {
                                break;
                            }
                            match ev {
                                Ok((_, m)) => {
                                    let data = m.data().as_string().unwrap_or_default();
                                    if data == "[[XOKSA_DONE]]" {
                                        break;
                                    }
                                    // Control line: reflect the chat's loaded set
                                    // back to the header chips (e.g. after /sym add).
                                    if let Some(csv) = data.strip_prefix("[[XOKSA_TICKERS]]") {
                                        let set: Vec<String> = csv
                                            .split(',')
                                            .map(|x| x.trim().to_string())
                                            .filter(|x| !x.is_empty())
                                            .collect();
                                        if !set.is_empty() {
                                            // The server's loaded set == the target set we
                                            // sent, updated by any `/sym add|del` typed in
                                            // chat. Sync `targets` to it and union new
                                            // tickers into the chips; keep the active view.
                                            tickers.update(|t| {
                                                // The server set is the authoritative,
                                                // normalized (`.T`) form. Drop any raw chip
                                                // the server returned normalized so "9432"
                                                // and "9432.T" never coexist as duplicates.
                                                t.retain(|c| {
                                                    !set.iter().any(|s| {
                                                        !s.eq_ignore_ascii_case(c)
                                                            && s.strip_suffix(".T").is_some_and(
                                                                |base| base.eq_ignore_ascii_case(c),
                                                            )
                                                    })
                                                });
                                                for s in &set {
                                                    if !t.iter().any(|x| x.eq_ignore_ascii_case(s))
                                                    {
                                                        t.push(s.clone());
                                                    }
                                                }
                                            });
                                            // Asked for but not returned = the engine
                                            // could not use it this turn. Mark it;
                                            // returning clears the mark.
                                            let asked = sent_targets.get_value();
                                            rejected.update(|r| {
                                                r.retain(|x| {
                                                    !set.iter().any(|s| s.eq_ignore_ascii_case(x))
                                                });
                                                for a in &asked {
                                                    let back = set
                                                        .iter()
                                                        .any(|s| s.eq_ignore_ascii_case(a));
                                                    let known =
                                                        r.iter().any(|x| x.eq_ignore_ascii_case(a));
                                                    if !back && !known {
                                                        r.push(a.clone());
                                                    }
                                                }
                                            });
                                            targets.set(set);
                                            let chips = tickers.get_untracked();
                                            if !chips.iter().any(|x| {
                                                x.eq_ignore_ascii_case(&symbol.get_untracked())
                                            }) {
                                                symbol.set(
                                                    chips.first().cloned().unwrap_or_default(),
                                                );
                                            }
                                        }
                                        continue;
                                    }
                                    if let Some(tf) = data.strip_prefix("[[XOKSA_TIMEFRAME]]") {
                                        let tf = tf.trim().to_string();
                                        if !tf.is_empty() && tf != timeframe.get_untracked() {
                                            timeframe.set(tf);
                                        }
                                        continue;
                                    }
                                    // Server asked us to wipe the visible chat log
                                    // (the `/clear` command / header 🗑 button).
                                    if data.strip_prefix("[[XOKSA_CLEAR]]").is_some() {
                                        messages.set(Vec::new());
                                        continue;
                                    }
                                    stream_buf.update(|s| {
                                        if !s.is_empty() {
                                            s.push('\n');
                                        }
                                        s.push_str(&data);
                                    });
                                }
                                Err(_) => break, // connection closed / error → stop (no reconnect)
                            }
                        }
                        drop(es); // closes the EventSource
                    }
                    Err(e) => error = Some(e.to_string()),
                },
                Err(e) => error = Some(e.to_string()),
            }
            // If superseded or cancelled, the current turn was already reset/committed
            // by the newer path — do not touch shared stream state here.
            if stream_gen.get_untracked() != my_gen {
                return;
            }
            if let Some(e) = error {
                stream_buf.update(|s| {
                    if !s.is_empty() {
                        s.push('\n');
                    }
                    s.push_str(&format!("{}: {e}", tr(lang, "送信失敗", "Send failed")));
                });
            }
            // Signal the reveal timer to finish + commit once caught up.
            stream_done.set(true);
            // Re-sync the header LLM dropdown: the turn (and any `/llm` switch it
            // carried) is complete and the session lock is released, so the options
            // refetch now reads the true active provider/model.
            llm_sync.update(|n| *n += 1);
        });
    };

    // Derived view helpers.
    let model_label = move || {
        summary_ok(summary)
            .map(|d| d.meta.model)
            .unwrap_or_else(|| "-".into())
    };
    // LLM picker: fetch the selectable set, mark the active one, and switch on
    // change via the unified /llm dispatch (mouse-only — no free-text model entry).
    let refresh_llm_options = move || {
        let sym = symbol.get_untracked();
        if sym.is_empty() {
            return;
        }
        let tf = timeframe.get_untracked();
        spawn_local(async move {
            let enc = js_sys::encode_uri_component(&sym)
                .as_string()
                .unwrap_or_default();
            let url = format!("/api/llm/options?symbol={enc}&timeframe={tf}");
            if let Ok(r) = gloo_net::http::Request::get(&url).send().await {
                if let Ok(o) = r.json::<LlmOptionsResp>().await {
                    // Deterministic single-source selection: always adopt (the server
                    // reads it under a short lock, never blocked by an in-flight turn).
                    llm_options.set(o.options);
                }
            }
        });
    };
    // Refresh the picker (and its active marker) on symbol/timeframe change and
    // after each chat turn (llm_sync — catches a `/llm` typed in the chat box) or a
    // dropdown switch (which calls refresh directly). Deliberately NOT on the 30s
    // `tick`: that poll can fire while a chat turn holds the session lock, and the
    // server can't read the true LLM then — which used to flip the picker to the
    // env default. These triggers all fire when the session is lock-free.
    Effect::new(move |_| {
        symbol.get();
        timeframe.get();
        llm_sync.get();
        refresh_llm_options();
    });
    // Reflect the active option onto the <select> AFTER its options have rendered.
    // `active` is read synchronously (so the Effect tracks llm_options), but the
    // value is applied in a `request_animation_frame` — i.e. after this frame's DOM
    // update, so the matching <option> already exists. A `prop:value` binding (or a
    // bare effect) can apply mid-rebuild and leave the browser showing the first
    // option (the openai flip after /basic).
    Effect::new(move |_| {
        let active = llm_options
            .get()
            .iter()
            .find(|o| o.active)
            .map(|o| o.value.clone())
            .unwrap_or_default();
        request_animation_frame(move || {
            if let Some(sel) = llm_select_ref.get_untracked() {
                if sel.value() != active {
                    sel.set_value(&active);
                }
            }
        });
    });
    let on_llm_change = move |ev: web_sys::Event| {
        let value = event_target_value(&ev);
        let (sym, tks) = analysis_scope();
        if sym.is_empty() || value.is_empty() {
            return;
        }
        let tf = timeframe.get_untracked();
        let body = format!(
            "{{\"value\":\"{value}\",\"symbol\":\"{sym}\",\"tickers\":\"{tks}\",\"timeframe\":\"{tf}\"}}"
        );
        spawn_local(async move {
            if let Ok(req) = gloo_net::http::Request::post("/api/llm/select")
                .header("Content-Type", "application/json")
                .body(body)
            {
                let _ = req.send().await;
            }
            // Reflect immediately (picker active marker) and propagate to the
            // summary/chat/multi-timeframe (subsequent LLM calls) via a refetch.
            refresh_llm_options();
            tick.update(|t| *t += 1);
        });
    };

    // ── Alerts panel operations (all hit /api/alerts) ──
    // Operator key → the symbol the engine's condition parser expects.
    fn al_op_symbol(op: &str) -> &'static str {
        match op {
            "ge" => ">=",
            "lt" => "<",
            "gt" => ">",
            _ => "<=",
        }
    }
    let load_alerts = move || {
        spawn_local(async move {
            if let Ok(resp) = gloo_net::http::Request::get("/api/alerts").send().await {
                if let Ok(data) = resp.json::<AlertsResp>().await {
                    if al_chan.get_untracked().is_empty() {
                        if let Some(first) = data.channels.first() {
                            al_chan.set(first.name.clone());
                        }
                    }
                    al_rules.set(data.rules);
                    al_channels.set(data.channels);
                }
            }
        });
    };
    let open_alerts = move || {
        al_msg.set(String::new());
        al_open.set(true);
        load_alerts();
    };
    let add_alert = move || {
        let ticker = al_ticker.get_untracked().trim().to_string();
        if ticker.is_empty() {
            al_msg.set(tr(lang, "銘柄を入力してください。", "Enter a ticker.").into());
            return;
        }
        let notify = al_chan.get_untracked();
        if notify.is_empty() {
            al_msg.set(
                tr(
                    lang,
                    "通知先チャンネルがありません。設定でチャンネルを追加してください。",
                    "No channel yet — add one in the settings form.",
                )
                .into(),
            );
            return;
        }
        let when = format!(
            "{}{}{}",
            al_ind.get_untracked(),
            al_op_symbol(&al_op.get_untracked()),
            al_val.get_untracked().trim()
        );
        let body = serde_json::json!({
            "ticker": ticker, "when": when, "mode": al_mode.get_untracked(),
            "notify": notify, "explain": al_explain.get_untracked(),
        })
        .to_string();
        al_msg.set(tr(lang, "追加中…", "Adding…").into());
        spawn_local(async move {
            let outcome = async {
                let req = gloo_net::http::Request::post("/api/alerts")
                    .header("Content-Type", "application/json")
                    .body(body)
                    .map_err(|e| e.to_string())?;
                req.send()
                    .await
                    .map_err(|e| e.to_string())?
                    .json::<AlertOpResp>()
                    .await
                    .map_err(|e| e.to_string())
            }
            .await;
            match outcome {
                Ok(r) if r.ok => {
                    al_ticker.set(String::new());
                    al_msg.set(match r.warning {
                        Some(w) => format!("⚠️ {w}"),
                        None => tr(lang, "✅ 追加しました。", "✅ Added.").into(),
                    });
                    load_alerts();
                }
                Ok(r) => al_msg.set(format!("❌ {}", r.error.unwrap_or_default())),
                Err(e) => al_msg.set(format!("❌ {e}")),
            }
        });
    };
    let del_alert = move |n: u8| {
        let body = serde_json::json!({ "n": n }).to_string();
        spawn_local(async move {
            // Read the reply. Monitoring stops either way, but a config that could
            // not be written means the rule returns on the next start — discarding
            // the response left that as a silent surprise.
            let outcome = async {
                let req = gloo_net::http::Request::post("/api/alerts/delete")
                    .header("Content-Type", "application/json")
                    .body(body)
                    .map_err(|e| e.to_string())?;
                req.send()
                    .await
                    .map_err(|e| e.to_string())?
                    .json::<AlertOpResp>()
                    .await
                    .map_err(|e| e.to_string())
            }
            .await;
            match outcome {
                Ok(r) if r.ok => al_msg.set(match r.warning {
                    Some(w) => format!("⚠️ {w}"),
                    None => tr(lang, "✅ 削除しました。", "✅ Removed.").into(),
                }),
                Ok(r) => al_msg.set(format!("❌ {}", r.error.unwrap_or_default())),
                Err(e) => al_msg.set(format!("❌ {e}")),
            }
            load_alerts();
        });
    };
    let toggle_alert = move |n: u8, active: bool| {
        let body = serde_json::json!({ "n": n, "active": active }).to_string();
        spawn_local(async move {
            if let Ok(req) = gloo_net::http::Request::post("/api/alerts/toggle")
                .header("Content-Type", "application/json")
                .body(body)
            {
                let _ = req.send().await;
            }
            load_alerts();
        });
    };
    let test_chan = move |name: String| {
        let body = serde_json::json!({ "channel": name }).to_string();
        al_msg.set(tr(lang, "テスト送信中…", "Testing…").into());
        spawn_local(async move {
            let outcome = async {
                let req = gloo_net::http::Request::post("/api/alerts/test")
                    .header("Content-Type", "application/json")
                    .body(body)
                    .map_err(|e| e.to_string())?;
                req.send()
                    .await
                    .map_err(|e| e.to_string())?
                    .json::<AlertOpResp>()
                    .await
                    .map_err(|e| e.to_string())
            }
            .await;
            match outcome {
                Ok(r) if r.ok => {
                    al_msg.set(tr(lang, "✅ テスト送信しました。", "✅ Test sent.").into())
                }
                Ok(r) => al_msg.set(format!("❌ {}", r.error.unwrap_or_default())),
                Err(e) => al_msg.set(format!("❌ {e}")),
            }
        });
    };

    // Analysis actions grouped into one dropdown (Run / MTF / BT / History) to keep
    // the header uncluttered. Each pick fires the action and resets to the label.
    let on_action = move |ev: web_sys::Event| {
        match event_target_value(&ev).as_str() {
            "run" => {
                chat_input.set("/basic".to_string());
                send_chat();
            }
            "mtf" => open_mtf(),
            "bt" => open_backtest(),
            "alerts" => open_alerts(),
            _ => {}
        }
        // Reset back to the placeholder so the menu always reads "アクション ▾".
        action_menu.set(String::new());
    };

    view! {
        <header class="header">
            <div class="header__brand">
                "XOKSA"
                {move || {
                    app_config.get()
                        .and_then(|r| r.ok())
                        .map(|c| c.version)
                        .filter(|v| !v.is_empty())
                        .map(|v| view! { <span class="header__ver">{format!("v{v}")}</span> })
                }}
            </div>
            <div class="header__controls">
                // Ticker chips: the loaded set (1–5). The checkbox includes the chip
                // in the analysis / chat target set (≥1); the label makes it the
                // active (dashboard) ticker; × removes it. The target set drives the
                // chat comparison and stays in sync with /sym add|del.
                <span class="vdiv"></span>
                <div class="chips">
                    {move || {
                        let active = symbol.get();
                        tickers.get().into_iter().map(|t| {
                            let is_active = t.eq_ignore_ascii_case(&active);
                            let t_click = t.clone();
                            let t_del = t.clone();
                            let t_tgl = t.clone();
                            let t_chk = t.clone();
                            let t_bad = t.clone();
                            // A chip the last turn asked for and the engine could not
                            // use. Marked rather than removed, with the reason in the
                            // tooltip, so a typo is identifiable instead of being a
                            // chip that silently errors whenever it is selected.
                            let bad = Memo::new(move |_| {
                                rejected.get().iter().any(|x| x.eq_ignore_ascii_case(&t_bad))
                            });
                            let bad_title = tr(lang,
                                "直近の分析でデータを取得できませんでした（コードの誤り、または提供元が応答しない）。不要なら × で削除してください。",
                                "The last analysis could not fetch this symbol (a wrong code, or the providers did not answer). Remove it with × if it is not wanted.");
                            view! {
                                <span class=move || if is_active { "chip chip--active" } else { "chip" }>
                                    <input type="checkbox" class="chip__chk"
                                        title=tr(lang, "分析・チャットの対象", "Analysis / chat target")
                                        prop:checked=move || targets.get().iter().any(|x| x.eq_ignore_ascii_case(&t_chk))
                                        on:change=move |_| toggle_target(t_tgl.clone()) />
                                    <span class="chip__label"
                                        title=move || if bad.get() { bad_title } else { "" }
                                        on:click=move |_| symbol.set(t_click.clone())>
                                        {move || if bad.get() { "⚠️ " } else { "" }}{t.clone()}
                                    </span>
                                    <span class="chip__x" title=tr(lang,"削除","Remove") on:click=move |_| remove_ticker(t_del.clone())>"×"</span>
                                </span>
                            }
                        }).collect_view()
                    }}
                </div>
                <span class="push"></span>
                // Separator between the shown symbols and the +ticker entry, so the
                // "shown" set and the "add" box read as distinct.
                <span class="vdiv"></span>
                {move || (tickers.get().len() < 5).then(|| view! {
                    <span class="entry" title=tr(lang, "銘柄を追加するには、ここにコードを入力して Enter（最大5・例: NVDA、7203.T）", "To add a symbol, type its code here and press Enter (up to 5, e.g. NVDA, 7203.T)")>
                        <svg class="entry__icon" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <circle cx="11" cy="11" r="7"></circle>
                            <line x1="20" y1="20" x2="16.65" y2="16.65"></line>
                        </svg>
                        <input
                            class="entry__in" type="text"
                            aria-label=tr(lang, "銘柄コードを入力", "Enter a ticker code")
                            placeholder=tr(lang, "ここに銘柄コードを入力（例: NVDA）", "Type a ticker code here (e.g. NVDA)")
                            prop:value=move || add_input.get()
                            on:input=move |ev| add_input.set(event_target_value(&ev))
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if ev.key() == "Enter" { ev.prevent_default(); add_ticker(); }
                            }
                        />
                    </span>
                })}
                <select class="field" aria-label=tr(lang, "時間足", "Timeframe")
                    prop:value=move || timeframe.get()
                    on:change=move |ev| timeframe.set(event_target_value(&ev))
                >
                    // Options come from the server (/api/config → AnalysisMode::ALL),
                    // not a hardcoded list.
                    {move || {
                        let tfs = app_config.get()
                            .and_then(|r| r.ok())
                            .map(|c| c.timeframes)
                            .unwrap_or_default();
                        tfs.into_iter()
                            .map(|tf| { let label = tf.clone(); view! { <option value=tf>{label}</option> } })
                            .collect_view()
                    }}
                </select>
                // LLM picker (mouse-only). The set is the known available models
                // (key-present cloud providers + live ollama instances); no free-text
                // model entry. Selecting switches via the unified /llm dispatch.
                <select class="field" node_ref=llm_select_ref aria-label=tr(lang, "AI の選択", "AI selection")
                    title=tr(lang, "解説・チャットに使う AI（マウスで選択。キーボードでのモデル入力は不可）", "AI for commentary / chat (mouse-select only; no free-text model entry)")
                    // The displayed selection is driven by the value-sync Effect above
                    // (via `llm_select_ref`), which runs AFTER the options render — the
                    // `selected` attribute alone does not re-sync a <select> when its
                    // option list is re-rendered, and a `prop:value` binding can apply
                    // before the matching <option> exists (the openai flip after /basic).
                    on:change=on_llm_change
                >
                    {move || {
                        let opts = llm_options.get();
                        if opts.is_empty() {
                            view! { <option value="" selected=true>{model_label()}</option> }.into_any()
                        } else {
                            opts.into_iter()
                                .map(|o| view! {
                                    <option value=o.value.clone() selected=o.active>{o.label}</option>
                                })
                                .collect_view()
                                .into_any()
                        }
                    }}
                </select>
                <span class="vdiv"></span>
                // Analysis actions grouped into one dropdown. Picking runs the
                // action and resets to the placeholder.
                <select class="field" title=tr(lang, "分析メニュー", "Analyze")
                    prop:value=move || action_menu.get()
                    on:change=on_action
                >
                    <option value="" selected=true>{tr(lang, "ツール", "Tools")}</option>
                    <option value="run">{tr(lang, "🧠 基本分析", "🧠 Basic")}</option>
                    <option value="mtf">{tr(lang, "📐 マルチタイムフレーム", "📐 Multi-timeframe")}</option>
                    <option value="bt">{tr(lang, "🧪 バックテスト", "🧪 Backtest")}</option>
                    <option value="alerts">{tr(lang, "🔔 アラート", "🔔 Alerts")}</option>
                </select>
                <button class="hicon" title=tr(lang, "チャットコマンド一覧（別ウィンドウ）", "Chat commands (separate window)") on:click=open_help>"?"</button>
                <button class="hicon hicon--accent"
                    title=tr(lang, "設定（更新・表示）", "Settings (update / display)")
                    on:click=move |_| settings_open.update(|o| *o = !*o)>"⚙"</button>
            </div>
        </header>
        // ⚙ settings popover — set-once controls moved off the single-row header:
        // update cadence (auto-refresh + refresh-now) and display (theme / language
        // / font). A transparent backdrop closes it on outside click.
        {move || settings_open.get().then(|| view! {
            <div class="settings-backdrop" on:click=move |_| settings_open.set(false)></div>
            <div class="settings-pop">
                <h3>{tr(lang, "更新", "Update")}</h3>
                <div class="prow">
                    <span class="prow__k">{tr(lang, "自動更新（60秒ごと）", "Auto-refresh (every 60s)")}</span>
                    <label class="toggle">
                        <input type="checkbox"
                            prop:checked=move || auto_refresh.get()
                            on:change=move |ev| auto_refresh.set(event_target_checked(&ev)) />
                    </label>
                </div>
                <div class="prow">
                    <span class="prow__k">{tr(lang, "今すぐ更新", "Refresh now")}</span>
                    <button class="btn" on:click=move |_| { force_flag.set_value(true); tick.update(|t| *t += 1); }>{tr(lang, "↻ 更新", "↻ Refresh")}</button>
                </div>
                <h3>{tr(lang, "表示", "Display")}</h3>
                <div class="prow">
                    <span class="prow__k">{tr(lang, "テーマ", "Theme")}</span>
                    <span class="seg">
                        <button class=move || if theme.get() != "dark" { "on" } else { "" }
                            on:click=move |_| theme.set("light".into())>{tr(lang, "☀ ライト", "☀ Light")}</button>
                        <button class=move || if theme.get() == "dark" { "on" } else { "" }
                            on:click=move |_| theme.set("dark".into())>{tr(lang, "🌙 ダーク", "🌙 Dark")}</button>
                    </span>
                </div>
                <div class="prow">
                    <span class="prow__k">{tr(lang, "言語", "Language")}</span>
                    <span class="seg">
                        <button class={if lang == "ja" { "on" } else { "" }}
                            disabled=!lang_is_settable()
                            on:click=move |_| set_ui_lang("ja")>"日本語"</button>
                        <button class={if lang == "ja" { "" } else { "on" }}
                            disabled=!lang_is_settable()
                            on:click=move |_| set_ui_lang("en")>"English"</button>
                    </span>
                </div>
                {(!lang_is_settable()).then(|| view! {
                    <div class="prow"><span class="prow__k"></span>
                        <span class="hint">{if private_session() {
                            tr(lang,
                               "無痕跡セッション（--private）では設定を書きません。",
                               "A no-trace session (--private) does not write settings.")
                        } else {
                            tr(lang,
                               "言語はエンジン側の設定です。エンジンを動かしている端末で変更してください。",
                               "The language is the engine's setting — change it on the machine running the engine.")
                        }}</span>
                    </div>
                })}
                <div class="prow">
                    <span class="prow__k">{tr(lang, "文字サイズ", "Font size")}</span>
                    <span class="seg">
                        <button class=move || if font.get() == "s" { "on" } else { "" } on:click=move |_| font.set("s".into())>{tr(lang, "小", "S")}</button>
                        <button class=move || if font.get() == "m" { "on" } else { "" } on:click=move |_| font.set("m".into())>{tr(lang, "中", "M")}</button>
                        <button class=move || if font.get() == "l" { "on" } else { "" } on:click=move |_| font.set("l".into())>{tr(lang, "大", "L")}</button>
                    </span>
                </div>
            </div>
        })}

        // Backtest setup panel. Opened by the 🧪 action; the period options + the
        // "max period" hint update the moment a timeframe is picked (from
        // /api/config — not the manual/help). No fixed period is imposed.
        {move || bt_open.get().then(|| view! {
            <div class="bt-overlay" on:click=move |_| bt_open.set(false)>
                <div class="bt-card" on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()>
                    <h3 class="bt-title">{tr(lang, "🧪 バックテスト設定", "🧪 Backtest setup")}
                        " " <span class="bt-sym">{move || symbol.get()}</span></h3>
                    <label class="bt-row">{tr(lang, "足種", "Timeframe")}
                        <select class="field"
                            on:change=move |ev| {
                                let tf = event_target_value(&ev);
                                let p = bt_periods.get_untracked().get(&tf)
                                    .and_then(|v| v.last().cloned()).unwrap_or_default();
                                bt_tf.set(tf);
                                bt_period.set(p);
                            }>
                            {move || app_config.get().and_then(|r| r.ok()).map(|c| c.timeframes)
                                .unwrap_or_default().into_iter()
                                .map(|tf| { let sel = tf.clone();
                                    view! { <option value=sel.clone() selected=move || bt_tf.get() == sel>{tf}</option> } })
                                .collect_view()}
                        </select>
                    </label>
                    <p class="bt-hint">{move || {
                        let tf = bt_tf.get();
                        match bt_periods.get().get(&tf).and_then(|v| v.last()) {
                            Some(mx) => format!("{} {}",
                                tr(lang, "この足種で可能な期間: 最大", "Max period for this timeframe:"),
                                period_label(&bt_period_labels.get(), mx, lang)),
                            None => String::new(),
                        }
                    }}</p>
                    <label class="bt-row">{tr(lang, "期間", "Period")}
                        <select class="field"
                            on:change=move |ev| bt_period.set(event_target_value(&ev))>
                            {move || {
                                let tf = bt_tf.get();
                                let labels = bt_period_labels.get();
                                bt_periods.get().get(&tf).cloned().unwrap_or_default().into_iter()
                                    .map(|tok| {
                                        let t = tok.clone();
                                        let label = period_label(&labels, &tok, lang);
                                        view! { <option value=tok selected=move || bt_period.get() == t>{label}</option> }
                                    }).collect_view()
                            }}
                        </select>
                    </label>
                    <label class="bt-row">{tr(lang, "元手（現金）", "Starting cash")}
                        <input class="field" type="number" min="1" step="any"
                            prop:value=move || bt_cash.get()
                            on:input=move |ev| bt_cash.set(event_target_value(&ev)) />
                    </label>
                    <label class="bt-row">{tr(lang, "初期購入（元手の%）", "Buy up front (% of cash)")}
                        <input class="field" type="number" min="0" max="100" step="any"
                            prop:value=move || bt_start_pct.get()
                            on:input=move |ev| bt_start_pct.set(event_target_value(&ev)) />
                    </label>
                    <label class="bt-row">{tr(lang, "1シグナルの増減額", "Trade per signal")}
                        <input class="field" type="number" min="0" step="any"
                            prop:value=move || bt_step.get()
                            on:input=move |ev| bt_step.set(event_target_value(&ev)) />
                    </label>
                    <p class="bt-hint">{tr(lang,
                        "元手はこの銘柄の通貨で入力（例 日本株=円、米国株=ドル）。開始時に「元手の%」だけ買い、買い/売りシグナルごとに「増減額」相当を売買（最小単元単位・余りは現金）。",
                        "Enter cash in this stock's currency (JPY for Tokyo, USD for US). At the start, buy the given % of cash; on each buy/sell signal, trade the per-signal amount (whole lots; leftover stays cash).")}</p>
                    <p class="bt-hint">{tr(lang,
                        "ルール（買い・売りの条件）を自由に編集。テンプレ（_template）や保存済みは下の一覧から読込。",
                        "Edit the buy/sell rules freely. Load a template (_template) or a saved rule from the list below.")}</p>
                    {render_side(tr(lang, "買い条件", "Buy when"), bt_entry, bt_entry_and)}
                    {render_side(tr(lang, "売り条件", "Sell when"), bt_exit, bt_exit_and)}
                    <div class="bt-saverow">
                        <input class="field" type="text"
                            placeholder=tr(lang, "ルール名", "rule name")
                            prop:value=move || bt_save_name.get()
                            on:input=move |ev| bt_save_name.set(event_target_value(&ev)) />
                        <button class="btn" on:click=move |_| save_rule()>
                            {tr(lang, "保存", "Save")}</button>
                        <select class="field"
                            on:change=move |ev| bt_load_sel.set(event_target_value(&ev))>
                            <option value="" selected=move || bt_load_sel.get().is_empty()>{tr(lang, "-- テンプレ・保存済みから読込 --", "-- load a template / saved rule --")}</option>
                            {move || bt_saved.get().into_iter()
                                .map(|s| { let n = s.name.clone(); let val = s.name.clone();
                                    view! { <option value=val selected=move || bt_load_sel.get() == n>{s.name}</option> } })
                                .collect_view()}
                        </select>
                        <button class="btn" on:click=move |_| load_rule()>
                            {tr(lang, "読込", "Load")}</button>
                    </div>
                    <div class="bt-actions">
                        <button class="btn" on:click=move |_| bt_open.set(false)>
                            {tr(lang, "キャンセル", "Cancel")}</button>
                        <button class="btn btn--primary" prop:disabled=move || bt_running.get()
                            on:click=move |_| run_backtest_now()>
                            {move || if bt_running.get() { tr(lang, "実行中…", "Running…") }
                                     else { tr(lang, "実行", "Run") }}</button>
                    </div>
                </div>
            </div>
        })}

        {move || al_open.get().then(|| view! {
            <div class="bt-overlay" on:click=move |_| al_open.set(false)>
                <div class="bt-card" on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()>
                    <h3 class="bt-title">{tr(lang, "🔔 アラート", "🔔 Alerts")}</h3>
                    <p class="bt-hint">{tr(lang,
                        "アプリ起動中、取得した最新の足で条件が成立すると通知チャンネルへ1行プッシュします（日本株の分足では形成中の足を含み、足の確定は待ちません）。ここで作ったルールは xoksa.env に保存され、再起動後も有効です。通知先チャンネルは設定フォームで追加します。直近の送信結果は各ルールの下に出ます（失敗しても再送はしないため、ここが気付く場所です）。",
                        "While the app runs, a rule pushes a one-line message to your channel when its condition first holds on the latest fetched bar — on Japanese intraday timeframes that is the still-forming bar, so it does not wait for the bar to close. Rules made here are saved to xoksa.env and survive a restart. Add notification channels in the settings form. The last send shows under each rule — a failed one is never retried, so this is where you notice it.")}</p>

                    <div style="display:flex;flex-direction:column;gap:4px;margin:6px 0">
                        {move || {
                            let rules = al_rules.get();
                            if rules.is_empty() {
                                view! { <p class="muted">{tr(lang, "ルールはまだありません。", "No rules yet.")}</p> }.into_any()
                            } else {
                                rules.into_iter().map(|r| {
                                    let (n, active) = (r.n, r.active);
                                    // The last send, shown under the rule. A failure here is
                                    // the only place it surfaces: the monitor swallows it so
                                    // one bad channel cannot stop it, and it fires once per
                                    // crossing with no retry.
                                    let status = r.last_send_at.clone().map(|at| {
                                        if r.last_send_ok.unwrap_or(false) {
                                            format!("✅ {} {}", at, tr(lang, "送信", "sent"))
                                        } else {
                                            match r.last_send_error.clone() {
                                                Some(why) if !why.is_empty() => format!("⚠️ {} {}: {}",
                                                    at, tr(lang, "送信失敗", "send failed"), why),
                                                _ => format!("⚠️ {} {}", at, tr(lang, "送信失敗", "send failed")),
                                            }
                                        }
                                    });
                                    view! {
                                        <div style="display:flex;flex-direction:column;gap:2px">
                                            <div style="display:flex;gap:8px;align-items:center">
                                                <span style="flex:1">
                                                    {format!("#{} [{}] {} {}  → #{}{}{}",
                                                        r.n, r.mode, r.ticker, r.cond, r.notify,
                                                        if r.explain { " +explain" } else { "" },
                                                        if r.active { "" } else { tr(lang, "  (無効)", "  (off)") })}
                                                </span>
                                                <button class="btn" on:click=move |_| toggle_alert(n, !active)>
                                                    {if active { tr(lang, "無効化", "Disable") } else { tr(lang, "有効化", "Enable") }}</button>
                                                <button class="btn" on:click=move |_| del_alert(n)>{tr(lang, "削除", "Delete")}</button>
                                            </div>
                                            {status.map(|s| view! {
                                                <span class="muted" style="font-size:0.85em;word-break:break-word">{s}</span>
                                            })}
                                        </div>
                                    }
                                }).collect_view().into_any()
                            }
                        }}
                    </div>

                    <h4 style="margin:10px 0 4px">{tr(lang, "ルールを追加", "Add a rule")}</h4>
                    <div style="display:flex;flex-wrap:wrap;gap:8px;align-items:center">
                        <input class="field" type="text" style="flex:1;min-width:130px"
                            placeholder=tr(lang, "銘柄 例 7203.T / NVDA", "ticker e.g. 7203.T / NVDA")
                            prop:value=move || al_ticker.get()
                            on:input=move |ev| al_ticker.set(event_target_value(&ev)) />
                        <select class="field" on:change=move |ev| al_ind.set(event_target_value(&ev))>
                            {move || bt_indicators.get().into_iter().map(|(k, ja, en)| {
                                let kk = k.clone(); let label = if lang == "ja" { ja } else { en };
                                view! { <option value=k selected=move || al_ind.get() == kk>{label}</option> }
                            }).collect_view()}
                        </select>
                        <select class="field" on:change=move |ev| al_op.set(event_target_value(&ev))>
                            {move || bt_operators.get().into_iter()
                                .filter(|(k, _, _)| matches!(k.as_str(), "le" | "ge" | "lt" | "gt"))
                                .map(|(k, ja, en)| {
                                    let kk = k.clone(); let label = if lang == "ja" { ja } else { en };
                                    view! { <option value=k selected=move || al_op.get() == kk>{label}</option> }
                                }).collect_view()}
                        </select>
                        <input class="field" type="number" step="any" style="width:90px"
                            prop:value=move || al_val.get()
                            on:input=move |ev| al_val.set(event_target_value(&ev)) />
                        <select class="field" on:change=move |ev| al_mode.set(event_target_value(&ev))>
                            {["1m", "5m", "15m", "30m", "60m"].into_iter()
                                .map(|m| view! { <option value=m selected=move || al_mode.get() == m>{m}</option> })
                                .collect_view()}
                        </select>
                        <select class="field" on:change=move |ev| al_chan.set(event_target_value(&ev))>
                            {move || al_channels.get().into_iter().map(|c| {
                                let name = c.name.clone(); let sel = c.name.clone();
                                view! { <option value=name selected=move || al_chan.get() == sel>{c.name}</option> }
                            }).collect_view()}
                        </select>
                        <label class="check">
                            <input type="checkbox" prop:checked=move || al_explain.get()
                                on:change=move |ev| al_explain.set(event_target_checked(&ev)) />
                            " " {tr(lang, "解説", "Explain")}
                        </label>
                        <button class="btn btn--primary" on:click=move |_| add_alert()>{tr(lang, "追加", "Add")}</button>
                    </div>

                    <h4 style="margin:12px 0 4px">{tr(lang, "通知チャンネル", "Notification channels")}</h4>
                    <div style="display:flex;flex-direction:column;gap:4px">
                        {move || {
                            let chans = al_channels.get();
                            if chans.is_empty() {
                                view! { <p class="muted">{tr(lang, "チャンネルなし（設定フォームで追加）。", "None — add one in the settings form.")}</p> }.into_any()
                            } else {
                                chans.into_iter().map(|c| {
                                    let name = c.name.clone();
                                    view! {
                                        <div style="display:flex;gap:8px;align-items:center">
                                            <span style="flex:1">{format!("#{} [{}] {}  {}", c.n, c.kind, c.name,
                                                if c.secret_set { "🔑" } else { "⚠️" })}</span>
                                            <button class="btn" on:click=move |_| test_chan(name.clone())>{tr(lang, "テスト送信", "Test")}</button>
                                        </div>
                                    }
                                }).collect_view().into_any()
                            }
                        }}
                    </div>

                    <p class="bt-hint" style="min-height:1.2em">{move || al_msg.get()}</p>
                    <div class="bt-actions">
                        <button class="btn" on:click=move |_| al_open.set(false)>{tr(lang, "閉じる", "Close")}</button>
                    </div>
                </div>
            </div>
        })}

        <div class="app">
            <div class="dashboard">
                <section class="panel left">
                    <h2 class="panel__title panel__title--row">
                        <span>{tr(lang, "基本データ・指標分析", "Technical / Indicators")}</span>
                        // Yahoo's own data time (SOT) — the freshness signal the user
                        // tracks. Advances only when Yahoo publishes newer data; a
                        // cached (unchanged) answer is marked so it is identifiable.
                        <span class="data-time"
                            title=tr(lang, "データ提供元（Yahoo Finance）の最新データ時刻", "Latest data time from the source (Yahoo Finance)")>
                            {move || match summary.get() {
                                Some(Ok(d)) if d.ok && !d.meta.market_data_latest_time.is_empty() => {
                                    let mut s = format!(
                                        "{} {}",
                                        tr(lang, "データ時刻", "Data time"),
                                        d.meta.market_data_latest_time
                                    );
                                    if d.from_cache {
                                        s.push_str(tr(lang, "（キャッシュ）", " (cached)"));
                                    }
                                    s
                                }
                                _ => String::new(),
                            }}
                        </span>
                    </h2>
                    <div class="panel__body">
                        {move || match summary.get() {
                            None => view! { <p class="muted">{tr(lang, "読み込み中…", "Loading…")}</p> }.into_any(),
                            Some(Err(e)) => view! { <p class="muted">{if e == "__init__" { tr(lang, "銘柄を入力してください", "Enter a ticker").to_string() } else { format!("{}: {e}", tr(lang, "取得失敗", "Fetch failed")) }}</p> }.into_any(),
                            Some(Ok(d)) => {
                                if d.ok {
                                    view! { <pre class="analysis" inner_html=analysis_html(&d.technical_display)></pre> }.into_any()
                                } else {
                                    view! { <p class="muted">{format!("{}: {}", tr(lang, "取得失敗", "Fetch failed"), d.error.unwrap_or_default())}</p> }.into_any()
                                }
                            }
                        }}
                    </div>
                </section>

                <div class="gutter gutter--col" on:pointerdown=start_drag(Axis::X, -1.0, right_w, "xoksa.right_w")></div>

                <div class="right" node_ref=right_ref style:width=move || format!("{}px", right_w.get())>
                    <section class="panel panel--fundamental" style:height=move || format!("{}px", fund_h.get())>
                        <h2 class="panel__title">{tr(lang, "ファンダメンタルデータ", "Fundamentals")}</h2>
                        <div class="panel__body">
                            {move || fundamental_view(summary)}
                        </div>
                    </section>

                    <div class="gutter gutter--row" on:pointerdown=start_drag(Axis::Y, 1.0, fund_h, "xoksa.fund_h")></div>

                    <section class="panel panel--news" style:height=move || format!("{}px", news_h.get())>
                        <h2 class="panel__title">"News"</h2>
                        // Free-text news search: the box text is sent to Brave as-is;
                        // "銘柄情報を含む" appends the active ticker to the box.
                        <div class="news-search">
                            <input
                                class="field news-search__input" type="text" autocomplete="off"
                                placeholder=tr(lang, "ニュース検索", "Search news")
                                prop:value=move || news_search.get()
                                on:input=move |ev| news_search.set(event_target_value(&ev))
                                on:keydown=move |ev: web_sys::KeyboardEvent| {
                                    // Skip an Enter that confirms an IME conversion.
                                    if ev.key() == "Enter"
                                        && !ev.is_composing()
                                        && ev.key_code() != 229
                                    {
                                        ev.prevent_default();
                                        let t = news_search.get_untracked();
                                        news_query.set(if t.trim().is_empty() { None } else { Some(t) });
                                    }
                                }
                            />
                            <label class="news-search__chk">
                                <input
                                    type="checkbox"
                                    prop:checked=move || news_include_ticker.get()
                                    on:change=move |ev| {
                                        let on = event_target_checked(&ev);
                                        news_include_ticker.set(on);
                                        let sym = symbol.get_untracked();
                                        if sym.trim().is_empty() {
                                            return;
                                        }
                                        news_search.update(|s| {
                                            let present = s.split_whitespace().any(|w| w == sym);
                                            if on && !present {
                                                if !s.trim().is_empty() {
                                                    s.push(' ');
                                                }
                                                s.push_str(&sym);
                                            } else if !on && present {
                                                *s = s
                                                    .split_whitespace()
                                                    .filter(|w| *w != sym)
                                                    .collect::<Vec<_>>()
                                                    .join(" ");
                                            }
                                        });
                                    }
                                />
                                {tr(lang, "銘柄情報を含む", "Include ticker")}
                            </label>
                            <label class="news-search__chk"
                                title=tr(lang, "投資と無関係な話題（スポーツ等）の絞り込みを解除して全件表示", "Show all results, without the investor-relevance filter")>
                                <input
                                    type="checkbox"
                                    prop:checked=move || news_filter_off.get()
                                    on:change=move |ev| news_filter_off.set(event_target_checked(&ev))
                                />
                                {tr(lang, "フィルタを外す", "Unfilter")}
                            </label>
                            <button
                                class="btn news-search__btn" type="button"
                                on:click=move |_| {
                                    let t = news_search.get_untracked();
                                    news_query.set(if t.trim().is_empty() { None } else { Some(t) });
                                }
                            >
                                {tr(lang, "検索", "Search")}
                            </button>
                            // Reset to the initial state: clear the box + checkbox and
                            // fall back to the default ticker-derived news.
                            <button
                                class="btn news-search__btn" type="button"
                                on:click=move |_| {
                                    news_search.set(String::new());
                                    news_include_ticker.set(false);
                                    news_filter_off.set(false);
                                    news_query.set(None);
                                }
                            >
                                {tr(lang, "リセット", "Reset")}
                            </button>
                        </div>
                        <div class="panel__body news-list">
                            {move || match news.get() {
                                None => view! { <p class="muted">{tr(lang, "読み込み中…", "Loading…")}</p> }.into_any(),
                                Some(Err(e)) => view! { <p class="muted">{if e == "__init__" { tr(lang, "銘柄を入力してください", "Enter a ticker").to_string() } else { format!("{}: {e}", tr(lang, "取得失敗", "Fetch failed")) }}</p> }.into_any(),
                                Some(Ok(n)) => {
                                    if !n.ok {
                                        view! { <p class="muted">{n.note.unwrap_or_default()}</p> }.into_any()
                                    } else if n.items.is_empty() {
                                        view! { <p class="muted">{tr(lang, "ニュースなし", "No news")}</p> }.into_any()
                                    } else {
                                        n.items.into_iter().map(|it| {
                                            // Only turn http(s) URLs into clickable links. An upstream
                                            // result with e.g. a javascript: URL must never become an href
                                            // (OWASP API10 — unsafe consumption of upstream APIs → XSS).
                                            let safe = safe_http_url(&it.url);
                                            view! {
                                                <div class="news-item">
                                                    {if safe {
                                                        view! { <a class="news-title" href=it.url.clone() target="_blank" rel="noopener noreferrer">{it.title.clone()}</a> }.into_any()
                                                    } else {
                                                        view! { <span class="news-title">{it.title.clone()}</span> }.into_any()
                                                    }}
                                                    {if safe {
                                                        view! { <a class="news-url" href=it.url.clone() target="_blank" rel="noopener noreferrer">{it.url.clone()}</a> }.into_any()
                                                    } else {
                                                        view! { <span class="news-url">{it.url.clone()}</span> }.into_any()
                                                    }}
                                                    <div class="news-date">{it.published_at.unwrap_or_default()}</div>
                                                </div>
                                            }
                                        }).collect_view().into_any()
                                    }
                                }
                            }}
                        </div>
                    </section>

                    <div class="gutter gutter--row" on:pointerdown=start_drag(Axis::Y, 1.0, news_h, "xoksa.news_h")></div>

                    <section class="panel panel--market">
                        <h2 class="panel__title panel__title--row">
                            <span>{tr(lang, "市況データ", "Market")}</span>
                            <button class="btn btn--mini" title=tr(lang, "チャート（別ウィンドウ）", "Chart (separate window)") on:click=open_chart>"📈"</button>
                        </h2>
                        <div class="panel__body market-log">
                            {move || match summary.get() {
                                Some(Ok(d)) if d.ok && !d.market_line.is_empty() => {
                                    view! { <div class="row">{d.market_line.clone()}</div> }.into_any()
                                }
                                _ => view! { <p class="muted">"—"</p> }.into_any(),
                            }}
                        </div>
                    </section>
                </div>
            </div>

            <div class="gutter gutter--row" on:pointerdown=start_drag(Axis::Y, -1.0, chat_h, "xoksa.chat_h")></div>

            <section class="panel panel--chat" style:height=move || format!("{}px", chat_h.get())>
                <h2 class="panel__title panel__title--row">
                    <span>{tr(lang, "チャット / コマンド", "Chat / Commands")}</span>
                    <button
                        class="btn btn--mini"
                        title=tr(lang, "チャットを消去（履歴・検討バッファをリセット）", "Clear chat (reset history + debate buffer)")
                        on:click=move |_| {
                            // SOT: 消去 is exactly the `/clear` command — identical to the CLI
                            // (immediate clear, no confirm). No `window.confirm`, which the Tauri
                            // WebView suppresses (it split browser-works vs desktop-broken).
                            chat_input.set("/clear".to_string());
                            send_chat();
                        }
                    >{format!("🗑 {}", tr(lang, "消去", "Clear"))}</button>
                </h2>
                <div class="panel__body chat-log" node_ref=chat_log_ref>
                    {move || messages.get().into_iter()
                        .map(|(role, text)| {
                            let body = if role == "assistant" { render_rich(&text) } else { text.into_any() };
                            view! { <div class=format!("msg msg--{role}")>{body}</div> }
                        })
                        .collect_view()}
                    {move || stream_on.get().then(|| {
                        let shown: String = stream_buf.get().chars().take(stream_pos.get()).collect();
                        if shown.is_empty() {
                            view! { <div class="msg msg--assistant streaming">{tr(lang, "応答を生成中…", "Generating…")}</div> }.into_any()
                        } else {
                            view! { <div class="msg msg--assistant streaming">{render_rich(&shown)}</div> }.into_any()
                        }
                    })}
                </div>
                // Attached chart interval (from the chart brush): a removable chip
                // shown above the input. Its confirmed values ground the next message.
                {move || attached_range.get().map(|(tf, from, to, _)| view! {
                    <div class="attach-chip">
                        <span>{format!("📎 {tf} {} {from} 〜 {to}", tr(lang, "区間", "interval"))}</span>
                        <button type="button" class="chip__x" title=tr(lang, "添付を外す", "Remove attachment")
                            on:click=move |_| attached_range.set(None)>"×"</button>
                    </div>
                })}
                <form class="chat-input" on:submit=move |ev| { ev.prevent_default(); send_chat(); }>
                    <textarea
                        node_ref=chat_input_ref
                        class="field field--grow chat-ta" rows="2" autocomplete="off"
                        placeholder=tr(lang, "例: 本日の状況を教えて / この銘柄の弱点を教えて（Enterで送信・Shift+Enterで改行）", "e.g. How is it today? / What are this stock's weaknesses? (Enter to send, Shift+Enter for a newline)")
                        prop:value=move || chat_input.get()
                        on:input=move |ev| {
                            chat_input.set(event_target_value(&ev));
                            // Auto-grow to fit the content (capped by CSS max-height).
                            if let Some(el) = chat_input_ref.get() {
                                let ta: &web_sys::HtmlTextAreaElement = &el;
                                let _ = web_sys::HtmlElement::style(ta)
                                    .set_property("height", "auto");
                                let _ = web_sys::HtmlElement::style(ta)
                                    .set_property("height", &format!("{}px", ta.scroll_height()));
                            }
                        }
                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                            // Enter sends; Shift+Enter inserts a newline (LLM-app style).
                            // But an Enter that CONFIRMS an IME (Japanese FEP) conversion
                            // must not send: while composing, isComposing is true and
                            // legacy engines report keyCode 229 — skip both.
                            if ev.key() == "Enter"
                                && !ev.shift_key()
                                && !ev.is_composing()
                                && ev.key_code() != 229
                            {
                                ev.prevent_default();
                                send_chat();
                                return;
                            }
                            // ↑/↓ recall previous inputs — only at the caret boundaries so
                            // multi-line editing (moving between lines) still works.
                            let hist = history.get();
                            if hist.is_empty() {
                                return;
                            }
                            let Some(el) = chat_input_ref.get() else { return };
                            let ta: &web_sys::HtmlTextAreaElement = &el;
                            let caret = ta.selection_start().ok().flatten().unwrap_or(0);
                            let len = ta.value().encode_utf16().count() as u32;
                            match ev.key().as_str() {
                                "ArrowUp" if caret == 0 => {
                                    ev.prevent_default();
                                    let i = match hist_idx.get() {
                                        None => hist.len() - 1,
                                        Some(0) => 0,
                                        Some(i) => i - 1,
                                    };
                                    hist_idx.set(Some(i));
                                    chat_input.set(hist[i].clone());
                                }
                                "ArrowDown" if caret >= len => {
                                    if let Some(i) = hist_idx.get() {
                                        ev.prevent_default();
                                        if i + 1 < hist.len() {
                                            hist_idx.set(Some(i + 1));
                                            chat_input.set(hist[i + 1].clone());
                                        } else {
                                            hist_idx.set(None);
                                            chat_input.set(String::new());
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    ></textarea>
                    <button type="submit" class="btn btn--primary" disabled=move || pending.get()>
                        {move || if pending.get() { tr(lang, "送信中…", "Sending…") } else { tr(lang, "送信", "Send") }}
                    </button>
                    // Cancel button: shown only while a request is in flight, aborts
                    // it and returns the input to an idle, ready-to-send state.
                    {move || pending.get().then(|| view! {
                        <button type="button" class="btn chat-cancel"
                            title=tr(lang, "取り消し", "Cancel")
                            on:click=move |_| cancel_chat()>"✕"</button>
                    })}
                </form>
            </section>
        </div>
    }
}

/// True for characters allowed in a URL (RFC 3986 unreserved + reserved + `%`).
/// Critically excludes whitespace AND Japanese / full-width punctuation, so a URL
/// ends correctly even when CJK text follows it with no space.
fn is_url_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '-' | '.'
                | '_'
                | '~'
                | ':'
                | '/'
                | '?'
                | '#'
                | '['
                | ']'
                | '@'
                | '!'
                | '$'
                | '&'
                | '\''
                | '('
                | ')'
                | '*'
                | '+'
                | ','
                | ';'
                | '='
                | '%'
        )
}

/// Locate the first `http://` / `https://` URL in `s`, returning its byte range.
/// The URL ends at the first non-URL character (whitespace, CJK, 「」（）…); a few
/// common trailing punctuation marks are then trimmed (e.g. `url.` / `(url)`).
/// True only for http(s) URLs — used to gate which strings become clickable
/// links, so a non-http scheme (e.g. `javascript:`) from upstream data can never
/// be turned into an href (OWASP API10 → XSS).
fn safe_http_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn find_url(s: &str) -> Option<(usize, usize)> {
    let start = match (s.find("http://"), s.find("https://")) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => return None,
    };
    let mut end = start;
    for (i, c) in s[start..].char_indices() {
        if is_url_char(c) {
            end = start + i + c.len_utf8();
        } else {
            break;
        }
    }
    while end > start {
        let last = s[start..end].chars().next_back().unwrap();
        if matches!(last, '.' | ',' | ')' | ']' | '>' | ';' | '!') {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    Some((start, end))
}

/// Split one line into inline pieces, turning each URL into an inline clickable
/// link that wraps if long. Inline (not block) keeps citation parentheses like
/// `（https://…）` attached to their text; a standalone URL line stays on its own
/// line because it is already a separate line in the source.
fn line_children(t: &str) -> Vec<AnyView> {
    let mut out = Vec::new();
    let mut rest = t;
    while let Some((start, end)) = find_url(rest) {
        let before = &rest[..start];
        let url = &rest[start..end];
        if !before.is_empty() {
            out.push(view! { <span>{before.to_string()}</span> }.into_any());
        }
        out.push(
            view! {
                <a class="ln-link" href=url.to_string() target="_blank" rel="noopener noreferrer">{url.to_string()}</a>
            }
            .into_any(),
        );
        rest = &rest[end..];
    }
    if !rest.is_empty() {
        out.push(view! { <span>{rest.to_string()}</span> }.into_any());
    }
    out
}

/// Render the analysis / fundamental preformatted text, coloring signed change
/// deltas green/red via CSS. Lines like `📊 前足比: +0.06 (+0.03%)` (i.e. ending
/// in `%)` after a `: `) get their value wrapped in a `.pos`/`.neg` span. This is
/// the Web equivalent of the CLI's ANSI colors, which are disabled server-side so
/// raw escape codes never reach the browser. Text is HTML-escaped first.
fn analysis_html(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            let esc = html_escape(line);
            if line.trim_end().ends_with("%)") {
                if let Some(pos) = esc.rfind(": ") {
                    let (head, val) = esc.split_at(pos + 2);
                    let cls = if val.trim_start().starts_with('-') {
                        "neg"
                    } else {
                        "pos"
                    };
                    return format!("{head}<span class=\"{cls}\">{val}</span>");
                }
            }
            esc
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render an assistant transcript with light visual hierarchy (no backend
/// markdown): heading-like lines — `Round …`, bracketed labels like
/// `[R1 | 参加者: …]`, and short `見出し:` / `label:` lines — are bold + accent;
/// URLs become clickable links on their own line; blank lines become spacing.
fn render_rich(text: &str) -> AnyView {
    text.split('\n')
        .map(|line| {
            let t = line.trim_end();
            let trimmed = t.trim_start();
            if trimmed.is_empty() {
                return view! { <div class="ln-sp"></div> }.into_any();
            }
            let is_head = trimmed.starts_with("Round ")
                || trimmed.starts_with('⏳')
                || (trimmed.starts_with('[') && trimmed.ends_with(']'))
                || ((trimmed.ends_with('：') || trimmed.ends_with(':'))
                    && trimmed.chars().count() <= 24);
            let cls = if is_head { "ln-h" } else { "ln" };
            view! { <div class=cls>{line_children(t)}</div> }.into_any()
        })
        .collect_view()
        .into_any()
}

/// Reduce a help-catalog command to a clickable stub: cut at the first
/// placeholder (`<...>`, `[...]`, or `a|b` options) so a click pre-fills the
/// command and leaves the cursor where the user types arguments. Commands with no
/// placeholder are returned verbatim (e.g. `/status`, `/help`).
fn command_stub(cmd: &str) -> String {
    // `<`/`[` mark an argument directly; a `|` marks a value-choice list
    // (`a|b|c`) whose FIRST option starts one word back, so cut at the space
    // before it (not at the `|`, which would keep the first option).
    let bracket = cmd.find(['<', '[']);
    let choice = cmd
        .find('|')
        .map(|p| cmd[..p].rfind(' ').map_or(0, |s| s + 1));
    let cut = match (bracket, choice) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    match cut {
        Some(i) => format!("{} ", cmd[..i].trim_end()),
        None => cmd.to_string(),
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Localized label for a backtest period token, from the engine-served map (falls
/// back to the raw token if the map hasn't loaded / lacks it). Money and the whole
/// backtest report are rendered by the engine now (SOT) — not here.
fn period_label(
    labels: &std::collections::HashMap<String, (String, String)>,
    tok: &str,
    lang: &str,
) -> String {
    labels
        .get(tok)
        .map(|(ja, en)| if lang == "ja" { ja.clone() } else { en.clone() })
        .unwrap_or_else(|| tok.to_string())
}

/// A saved strategy from /api/backtest/rules (name + its rules JSON string).
#[derive(Clone, Default, Deserialize)]
struct SavedStrategy {
    name: String,
    spec_json: String,
}

/// Parsed rules (mirrors the backend StrategyRules) for loading a saved strategy.
#[derive(Default, Deserialize)]
struct RulesJson {
    #[serde(default)]
    entry_combine: String,
    #[serde(default)]
    entry: Vec<CondJson>,
    #[serde(default)]
    exit_combine: String,
    #[serde(default)]
    exit: Vec<CondJson>,
}

#[derive(Deserialize)]
struct CondJson {
    left: String,
    op: String,
    #[serde(default)]
    right_kind: String,
    #[serde(default)]
    value: Option<f64>,
    #[serde(default)]
    right: Option<String>,
}

/// One editable condition row in the rule editor. All fields are Copy signals so
/// rows can be re-rendered and edited without threading state by hand.
#[derive(Clone, Copy)]
struct CondRow {
    id: usize,
    left: RwSignal<String>,
    op: RwSignal<String>,
    rkind: RwSignal<String>, // "value" | "indicator"
    rval: RwSignal<String>,  // number text OR indicator key
}

/// Adopt a constructable stylesheet into a popup document — CSP-safe styling with
/// no inline `<style>` (which a strict CSP without `'unsafe-inline'` blocks).
/// Applied once when the popup opens; it survives later `innerHTML` re-renders
/// because the sheet lives on the document, not on its replaced content.
/// A `<link rel=stylesheet>` to a same-origin popup stylesheet (served by XOKSA as
/// a static asset). Popups are `about:blank` documents; an ABSOLUTE URL avoids the
/// undefined base-URL of `about:blank`, and `style-src 'self'` permits it. This
/// replaces CSSOM `adoptedStyleSheets`, which Firefox does not apply to
/// `about:blank` popups (Chrome does) — the reason the popups rendered unstyled there.
fn popup_css_link(file: &str) -> String {
    let origin = web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_default();
    format!("<link rel=\"stylesheet\" href=\"{origin}/{file}\">")
}

thread_local! {
    /// A random token minted once per page load. Popup click listeners mark their
    /// document with it, so a window REUSED across a dashboard reload — whose old
    /// listener now points at the unloaded previous WASM instance — is detected and
    /// re-wired, while a same-page reopen is still skipped. A fixed marker value
    /// could not tell "wired by this page" from "wired by a dead one".
    static PAGE_TOKEN: String = js_sys::Math::random().to_string();
}

/// This page load's popup-wiring token (see [`PAGE_TOKEN`]).
fn page_token() -> String {
    PAGE_TOKEN.with(String::clone)
}

/// One delegated `click` listener on a popup document: when an element carrying
/// `data-<attr>` (or a descendant of one) is clicked, call `handler(value)`.
/// Replaces the per-element inline `onclick` handlers (blocked under a strict CSP).
/// Attached once; survives `innerHTML` re-renders (the listener is on the document).
fn attach_popup_click_delegate<F>(doc: &web_sys::Document, attr: &'static str, handler: F)
where
    F: Fn(String) + 'static,
{
    // The closure is opener-realm code (created here, in the opener). When a click
    // in the POPUP document bubbles to it, it runs in the opener realm and calls
    // `handler` DIRECTLY — flipping the opener's signals. No `postMessage`, so
    // nothing crosses a window/realm boundary (the earlier postMessage route did
    // not deliver reliably across the about:blank popup).
    let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |ev: web_sys::Event| {
        // The click target belongs to the POPUP's realm; `dyn_into` (an `instanceof`
        // check against the OPENER's `Element` constructor) fails across realms and
        // silently drops the event. `unchecked_into` skips that check — `get_attribute`
        // and `parent_element` are plain DOM methods that work regardless of realm.
        let mut node = ev.target().map(|t| t.unchecked_into::<web_sys::Element>());
        while let Some(el) = node {
            if let Some(val) = el.get_attribute(attr) {
                handler(val);
                return;
            }
            node = el.parent_element();
        }
    });
    let _ = doc.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref());
    cb.forget();
}

/// Split a `<head>…</head><body>…</body>` document string into (head_inner, body_inner).
fn split_head_body(doc_html: &str) -> (&str, &str) {
    let head = doc_html
        .split_once("<head>")
        .and_then(|(_, r)| r.split_once("</head>"))
        .map(|(h, _)| h)
        .unwrap_or("");
    let body = doc_html
        .split_once("<body>")
        .and_then(|(_, r)| r.rsplit_once("</body>"))
        .map(|(b, _)| b)
        .unwrap_or(doc_html);
    (head, body)
}

/// Replace a popup document's content cross-engine-safely. Assigning a
/// `<head>…</head><body>…</body>` string to `documentElement.innerHTML` works in
/// WebKit (macOS WKWebView) but leaves the window BLANK in Chromium/WebView2
/// (Windows): Blink does not reconstruct `<head>`/`<body>` from that innerHTML
/// fragment. Injecting the head and body content into the existing elements
/// separately renders identically on both engines.
fn set_popup_document(doc: &web_sys::Document, doc_html: &str) {
    let (head_inner, body_inner) = split_head_body(doc_html);
    if let Some(head) = doc.query_selector("head").ok().flatten() {
        head.set_inner_html(head_inner);
    }
    if let Some(body) = doc.query_selector("body").ok().flatten() {
        body.set_inner_html(body_inner);
    }
}

/// Render (or re-render) chart HTML into a popup window. The toggle click listener
/// is wired ONCE by `open_chart` (where the series signals are in scope); it lives
/// on the document and survives these re-renders. CSS is a `<link>`.
fn render_chart_popup(popup: &web_sys::Window, html: &str) {
    if let Some(doc) = popup.document() {
        set_popup_document(&doc, html);
    }
}

/// A URL query parameter (manual parse; avoids the `UrlSearchParams` web-sys feature).
fn query_param(key: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    let search = search.trim_start_matches('?');
    for pair in search.split('&') {
        let mut it = pair.splitn(2, '=');
        if it.next() == Some(key) {
            let raw = it.next().unwrap_or("");
            return Some(
                js_sys::decode_uri_component(raw)
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_else(|| raw.to_string()),
            );
        }
    }
    None
}

/// True when running inside the Tauri desktop shell (`window.__TAURI__` present).
fn is_tauri() -> bool {
    web_sys::window()
        .and_then(|w| js_sys::Reflect::get(&w, &wasm_bindgen::JsValue::from_str("__TAURI__")).ok())
        .map(|v| !v.is_undefined() && !v.is_null())
        .unwrap_or(false)
}

/// Fire a Tauri command (fire-and-forget) via `window.__TAURI__.core.invoke`.
fn tauri_invoke(cmd: &str, args: wasm_bindgen::JsValue) {
    use wasm_bindgen::JsValue;
    let get = |obj: &JsValue, k: &str| {
        js_sys::Reflect::get(obj, &JsValue::from_str(k))
            .ok()
            .filter(|v| !v.is_undefined() && !v.is_null())
    };
    let Some(w) = web_sys::window() else { return };
    let Some(f) = get(&JsValue::from(w), "__TAURI__")
        .and_then(|t| get(&t, "core"))
        .and_then(|c| get(&c, "invoke"))
    else {
        return;
    };
    if let Ok(func) = f.dyn_into::<js_sys::Function>() {
        let _ = func.call2(&JsValue::UNDEFINED, &JsValue::from_str(cmd), &args);
    }
}

/// Resolve `window.__TAURI__.<ns>.<name>` to a JS function, if present.
fn tauri_fn(ns: &str, name: &str) -> Option<js_sys::Function> {
    use wasm_bindgen::JsValue;
    let get = |obj: &JsValue, k: &str| {
        js_sys::Reflect::get(obj, &JsValue::from_str(k))
            .ok()
            .filter(|v| !v.is_undefined() && !v.is_null())
    };
    let w = web_sys::window()?;
    get(&JsValue::from(w), "__TAURI__")
        .and_then(|t| get(&t, ns))
        .and_then(|e| get(&e, name))
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
}

/// Emit a Tauri event (fire-and-forget) via `window.__TAURI__.event.emit`. Used by
/// the chart popup (a separate native window) to hand its brushed interval back to
/// the main window, which has no shared JS realm with it.
fn tauri_emit(event: &str, payload: wasm_bindgen::JsValue) {
    use wasm_bindgen::JsValue;
    if let Some(func) = tauri_fn("event", "emit") {
        let _ = func.call2(&JsValue::UNDEFINED, &JsValue::from_str(event), &payload);
    }
}

/// Listen for a Tauri event via `window.__TAURI__.event.listen`. `handler` receives
/// the event object (`{event, id, payload}`); the listener stays registered for the
/// page's lifetime (the closure is leaked, matching the popup/dashboard lifecycle).
fn tauri_listen(event: &str, handler: impl Fn(wasm_bindgen::JsValue) + 'static) {
    use wasm_bindgen::JsValue;
    let Some(func) = tauri_fn("event", "listen") else {
        return;
    };
    let cb = Closure::<dyn FnMut(JsValue)>::new(move |ev: JsValue| handler(ev));
    let _ = func.call2(
        &JsValue::UNDEFINED,
        &JsValue::from_str(event),
        cb.as_ref().unchecked_ref(),
    );
    cb.forget();
}

/// Desktop: the WebView cannot follow `target="_blank"` / `window.open`, so a
/// delegated click listener intercepts clicks on external (`target="_blank"`)
/// http(s) links and hands the URL to the OS default browser via `open_external`.
/// Internal navigation (no `_blank`) is left untouched.
fn route_external_links_to_os() {
    use wasm_bindgen::JsValue;
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |ev: web_sys::Event| {
        let mut node = ev.target().map(|t| t.unchecked_into::<web_sys::Element>());
        while let Some(el) = node {
            if el.tag_name().eq_ignore_ascii_case("a") {
                let blank = el.get_attribute("target").as_deref() == Some("_blank");
                if let Some(href) = el.get_attribute("href") {
                    if blank && (href.starts_with("http://") || href.starts_with("https://")) {
                        ev.prevent_default();
                        let o = js_sys::Object::new();
                        let _ = js_sys::Reflect::set(
                            &o,
                            &JsValue::from_str("url"),
                            &JsValue::from_str(&href),
                        );
                        tauri_invoke("open_external", o.into());
                    }
                }
                return;
            }
            node = el.parent_element();
        }
    });
    let _ = doc.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref());
    cb.forget();
}

/// Build the args object for the `open_popup_window` Tauri command.
fn popup_args(kind: &str, symbol: &str, tf: &str, lang: &str) -> wasm_bindgen::JsValue {
    use wasm_bindgen::JsValue;
    let o = js_sys::Object::new();
    for (k, v) in [
        ("kind", kind),
        ("symbol", symbol),
        ("tf", tf),
        ("lang", lang),
    ] {
        let _ = js_sys::Reflect::set(&o, &JsValue::from_str(k), &JsValue::from_str(v));
    }
    o.into()
}

/// Popup-mode entry (desktop): a native window loads `/?popup=chart&…` and this
/// renders the chart in that window's own realm, reusing `chart_doc_html`, the
/// toggle delegate, and the brush-select (which emits a `chart-brush` Tauri event
/// picked up by the main window).
fn run_chart_popup() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let symbol = query_param("symbol").unwrap_or_default();
    let tf = query_param("tf").unwrap_or_else(|| "daily".to_string());
    let lang: &'static str = if query_param("lang").as_deref() == Some("ja") {
        "ja"
    } else {
        "en"
    };

    let points: Rc<RefCell<Vec<(String, ChartSample)>>> = Rc::new(RefCell::new(Vec::new()));
    let tf_label = Rc::new(RefCell::new(tf.clone()));
    let toggles = Rc::new(RefCell::new(ChartToggles {
        vwap: true,
        ema: true,
        sma: false,
        bb: false,
        rsi: true,
        macd: true,
        vol: true,
    }));

    // Brush-select on the chart's <svg>: drag to pick a bar interval, then emit a
    // `chart-brush` Tauri event carrying the interval's confirmed-value summary so
    // the main window can attach it as chat reference. The SVG is recreated on every
    // render, so this is re-run after each render; the listeners are kept alive in
    // `brush_keep` (replaced each time, dropping the ones on the now-gone SVG).
    type BrushListeners = Rc<RefCell<Vec<Closure<dyn FnMut(web_sys::MouseEvent)>>>>;
    let brush_keep: BrushListeners = Rc::new(RefCell::new(Vec::new()));
    let attach_brush: Rc<dyn Fn()> = {
        let (points, toggles, tf_label, tf_arg, brush_keep) = (
            points.clone(),
            toggles.clone(),
            tf_label.clone(),
            tf.clone(),
            brush_keep.clone(),
        );
        Rc::new(move || {
            use wasm_bindgen::JsValue;
            const VIEW_W: f64 = 860.0;
            const PAD_L: f64 = 64.0;
            const X_RIGHT: f64 = 816.0;
            let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
                return;
            };
            let Some(svg) = doc.query_selector("svg").ok().flatten() else {
                return;
            };
            let start = Rc::new(std::cell::Cell::new(Option::<f64>::None));
            // svg-x (viewBox units) from a mouse event, accounting for the uniform
            // "meet" scale + letterbox centering (see the main-window brush).
            let svg_x = {
                let svg = svg.clone();
                move |ev: &web_sys::MouseEvent| -> f64 {
                    let r = svg.get_bounding_client_rect();
                    let (vw, vh) = svg
                        .get_attribute("viewBox")
                        .and_then(|vb| {
                            let mut it = vb.split_whitespace().skip(2);
                            Some((
                                it.next()?.parse::<f64>().ok()?,
                                it.next()?.parse::<f64>().ok()?,
                            ))
                        })
                        .unwrap_or((VIEW_W, VIEW_W));
                    let (ew, eh) = (r.width().max(1.0), r.height().max(1.0));
                    let scale = (ew / vw).min(eh / vh);
                    let off_x = (ew - vw * scale) / 2.0;
                    (((ev.client_x() as f64) - r.left() - off_x) / scale).clamp(0.0, vw)
                }
            };
            let set_rect = {
                let (svg, doc) = (svg.clone(), doc.clone());
                move |x0: f64, x1: f64| {
                    let (lo, hi) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
                    let el = match svg.query_selector("#xk-brush").ok().flatten() {
                        Some(e) => e,
                        None => {
                            let Ok(e) =
                                doc.create_element_ns(Some("http://www.w3.org/2000/svg"), "rect")
                            else {
                                return;
                            };
                            let _ = e.set_attribute("id", "xk-brush");
                            let _ = e.set_attribute("fill", "rgba(120,150,255,0.16)");
                            let _ = e.set_attribute("stroke", "#6f8fff");
                            let _ = e.set_attribute("stroke-dasharray", "3 3");
                            let _ = e.set_attribute("pointer-events", "none");
                            let _ = e.set_attribute("y", "0");
                            let _ = e.set_attribute("height", "5000");
                            let _ = svg.append_child(&e);
                            e
                        }
                    };
                    let _ = el.set_attribute("x", &format!("{lo:.1}"));
                    let _ = el.set_attribute("width", &format!("{:.1}", (hi - lo).max(0.0)));
                }
            };
            let clear_rect = {
                let svg = svg.clone();
                move || {
                    if let Some(e) = svg.query_selector("#xk-brush").ok().flatten() {
                        e.remove();
                    }
                }
            };
            let down = {
                let (start, svg_x) = (start.clone(), svg_x.clone());
                Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |ev: web_sys::MouseEvent| {
                    start.set(Some(svg_x(&ev)));
                })
            };
            let mv = {
                let (start, svg_x, set_rect) = (start.clone(), svg_x.clone(), set_rect.clone());
                Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |ev: web_sys::MouseEvent| {
                    if let Some(x0) = start.get() {
                        set_rect(x0, svg_x(&ev));
                    }
                })
            };
            let up = {
                let (start, svg_x, clear_rect, points, toggles, tf_label, tf_arg) = (
                    start.clone(),
                    svg_x.clone(),
                    clear_rect.clone(),
                    points.clone(),
                    toggles.clone(),
                    tf_label.clone(),
                    tf_arg.clone(),
                );
                Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |ev: web_sys::MouseEvent| {
                    let Some(x0) = start.take() else { return };
                    let x1 = svg_x(&ev);
                    clear_rect();
                    let pts = points.borrow();
                    let n = pts.len();
                    if n < 2 {
                        return;
                    }
                    let to_i = |x: f64| {
                        (((x - PAD_L) / (X_RIGHT - PAD_L)) * ((n - 1) as f64))
                            .round()
                            .clamp(0.0, (n - 1) as f64) as usize
                    };
                    let (mut a, mut b) = (to_i(x0), to_i(x1));
                    if a > b {
                        std::mem::swap(&mut a, &mut b);
                    }
                    if b - a < 1 {
                        return;
                    }
                    let (from, to) = (pts[a].0.clone(), pts[b].0.clone());
                    let summary = range_summary(&pts[a..=b], *toggles.borrow());
                    let tfl = {
                        let l = tf_label.borrow().clone();
                        if l.is_empty() {
                            tf_arg.clone()
                        } else {
                            l
                        }
                    };
                    let o = js_sys::Object::new();
                    for (k, v) in [
                        ("tf", &tfl),
                        ("from", &from),
                        ("to", &to),
                        ("summary", &summary),
                    ] {
                        let _ =
                            js_sys::Reflect::set(&o, &JsValue::from_str(k), &JsValue::from_str(v));
                    }
                    tauri_emit("chart-brush", o.into());
                })
            };
            // DOM tooltip (WKWebView ignores SVG `<title>`): show the hovered
            // target's `data-tip` in a floating div that follows the cursor.
            let tip_move = {
                let doc = doc.clone();
                Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |ev: web_sys::MouseEvent| {
                    let Some(tip) = doc.get_element_by_id("chart-tip") else {
                        return;
                    };
                    let Ok(tip) = tip.dyn_into::<web_sys::HtmlElement>() else {
                        return;
                    };
                    let data = ev
                        .target()
                        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                        .and_then(|e| e.get_attribute("data-tip"));
                    match data {
                        Some(text) => {
                            tip.set_text_content(Some(&text));
                            let s = tip.style();
                            let _ = s.set_property("left", &format!("{}px", ev.client_x() + 14));
                            let _ = s.set_property("top", &format!("{}px", ev.client_y() + 14));
                            let _ = s.set_property("display", "block");
                        }
                        None => {
                            let _ = tip.style().set_property("display", "none");
                        }
                    }
                })
            };
            let tip_leave = {
                let doc = doc.clone();
                Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |_ev: web_sys::MouseEvent| {
                    if let Some(tip) = doc.get_element_by_id("chart-tip") {
                        if let Ok(tip) = tip.dyn_into::<web_sys::HtmlElement>() {
                            let _ = tip.style().set_property("display", "none");
                        }
                    }
                })
            };
            let _ =
                svg.add_event_listener_with_callback("mousedown", down.as_ref().unchecked_ref());
            let _ = svg.add_event_listener_with_callback("mousemove", mv.as_ref().unchecked_ref());
            let _ = svg.add_event_listener_with_callback("mouseup", up.as_ref().unchecked_ref());
            let _ = svg
                .add_event_listener_with_callback("mousemove", tip_move.as_ref().unchecked_ref());
            let _ = svg
                .add_event_listener_with_callback("mouseleave", tip_leave.as_ref().unchecked_ref());
            *brush_keep.borrow_mut() = vec![down, mv, up, tip_move, tip_leave];
        })
    };

    let render: Rc<dyn Fn()> = {
        let (points, tf_label, toggles, symbol, attach_brush) = (
            points.clone(),
            tf_label.clone(),
            toggles.clone(),
            symbol.clone(),
            attach_brush.clone(),
        );
        Rc::new(move || {
            let html = chart_doc_html(
                &points.borrow(),
                &symbol,
                &tf_label.borrow(),
                *toggles.borrow(),
                lang,
            );
            if let Some(w) = web_sys::window() {
                render_chart_popup(&w, &html);
            }
            attach_brush(); // re-wire brush onto the freshly rendered SVG
        })
    };

    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        let (toggles, render) = (toggles.clone(), render.clone());
        attach_popup_click_delegate(&doc, "data-toggle", move |key| {
            {
                let mut t = toggles.borrow_mut();
                match key.as_str() {
                    "vwap" => t.vwap = !t.vwap,
                    "ema" => t.ema = !t.ema,
                    "sma" => t.sma = !t.sma,
                    "bb" => t.bb = !t.bb,
                    "rsi" => t.rsi = !t.rsi,
                    "macd" => t.macd = !t.macd,
                    "vol" => t.vol = !t.vol,
                    _ => {}
                }
            }
            render();
        });
    }

    let (sym, tf2) = (symbol.clone(), tf.clone());
    let (points_f, tf_label_f, render_f) = (points.clone(), tf_label.clone(), render.clone());
    // wasm_bindgen_futures (not leptos::task): popup mode mounts no Leptos executor.
    wasm_bindgen_futures::spawn_local(async move {
        // Defer the first DOM swap to a task: replacing `documentElement.innerHTML`
        // synchronously while the WASM bootstrap script is still on the stack tears
        // down the running context. By the time this task runs the DOM is stable.
        render_f(); // initial "waiting for data" frame
        let enc = js_sys::encode_uri_component(&sym)
            .as_string()
            .unwrap_or_default();
        let url = format!("/api/symbol/{enc}/chart?timeframe={tf2}&bars=120&lang={lang}");
        if let Ok(resp) = gloo_net::http::Request::get(&url).send().await {
            if let Ok(cr) = resp.json::<ChartResp>().await {
                *tf_label_f.borrow_mut() = cr.timeframe_label;
                *points_f.borrow_mut() = cr
                    .bars
                    .into_iter()
                    .map(|b| {
                        (
                            b.t,
                            ChartSample {
                                price: b.price,
                                vwap: b.vwap,
                                ema_s: b.ema_s,
                                ema_l: b.ema_l,
                                sma_s: b.sma_s,
                                sma_l: b.sma_l,
                                bb_u: b.bb_u,
                                bb_l: b.bb_l,
                                rsi: b.rsi,
                                macd: b.macd,
                                volume: b.volume,
                            },
                        )
                    })
                    .collect();
            }
        }
        render_f();
    });
}

/// Open the chat-command reference in a SEPARATE browser popup window. Each row
/// carries `data-cmd=<command>`; a single delegated listener (see
/// `attach_popup_click_delegate`) runs `on_pick(<command>)` so clicking a row
/// pre-fills the main window's chat input. The popup is a standalone document
/// (CSS via a same-origin `<link>`) — independent, never overlapping the dashboard.
fn open_help_window(list: &[HelpCommand], lang: &str, on_pick: impl Fn(String) + 'static) {
    let doc_html = help_doc_html(list, lang);
    let Some(win) = web_sys::window() else { return };
    if let Ok(Some(popup)) = win.open_with_url_and_target_and_features(
        "",
        "xoksa-help",
        "popup=yes,width=520,height=720,scrollbars=yes,resizable=yes",
    ) {
        if let Some(doc) = popup.document() {
            if let Some(root) = doc.document_element() {
                set_popup_document(&doc, &doc_html);
                // Wire the delegated click listener ONCE per popup document (marker
                // survives innerHTML re-renders; avoids duplicate listeners when the
                // window is reused). CSS is a `<link>` in the HTML above.
                let token = page_token();
                if root.get_attribute("data-xoksa-wired").as_deref() != Some(&token) {
                    let _ = root.set_attribute("data-xoksa-wired", &token);
                    attach_popup_click_delegate(&doc, "data-cmd", on_pick);
                }
            }
        }
        let _ = popup.focus();
    }
}

/// The chat-command reference document (head + body). Each row carries `data-cmd`
/// so a delegated click listener can act on it. Shared by the browser popup
/// (`open_help_window`) and the desktop native window (`run_help_popup`).
fn help_doc_html(list: &[HelpCommand], lang: &str) -> String {
    let rows: String = list
        .iter()
        .map(|c| {
            let stub = html_escape(&command_stub(&c.command));
            let row_title = tr(
                lang,
                "クリックでメイン画面のチャット欄に挿入",
                "Click to insert into the main chat input",
            );
            format!(
                "<div class=\"cmd\" title=\"{row_title}\" data-cmd=\"{stub}\"><code>{}</code><span>{}</span></div>",
                html_escape(&c.command),
                html_escape(&c.description),
            )
        })
        .collect();
    let title = tr(lang, "XOKSA コマンド一覧", "XOKSA Commands");
    let heading = tr(lang, "チャット コマンド一覧", "Chat Commands");
    let note = tr(
        lang,
        "行をクリックするとメイン画面のチャット欄に挿入されます（CLIと同一の結果）。",
        "Click a row to insert it into the main chat input (same result as the CLI).",
    );
    let css = popup_css_link("xoksa-help.css");
    format!(
        "<head><meta charset=\"utf-8\"><title>{title}</title>{css}</head>\
         <body><h1>{heading}</h1>\
         <p class=\"note\">{note}</p>{rows}</body>"
    )
}

/// Popup-mode entry (desktop): a native window loads `/?popup=help`. Fetch the
/// command list, render it in this window, and emit a `help-pick` Tauri event on
/// row click so the main window can insert the command into its chat input.
fn run_help_popup() {
    use wasm_bindgen::JsValue;
    let lang: &'static str = if query_param("lang").as_deref() == Some("ja") {
        "ja"
    } else {
        "en"
    };
    wasm_bindgen_futures::spawn_local(async move {
        let list = fetch_commands(lang).await.unwrap_or_default();
        let html = help_doc_html(&list, lang);
        let Some(w) = web_sys::window() else { return };
        if let Some(doc) = w.document() {
            set_popup_document(&doc, &html);
        }
        if let Some(doc) = w.document() {
            attach_popup_click_delegate(&doc, "data-cmd", move |cmd| {
                let o = js_sys::Object::new();
                let _ =
                    js_sys::Reflect::set(&o, &JsValue::from_str("cmd"), &JsValue::from_str(&cmd));
                tauri_emit("help-pick", o.into());
            });
        }
    });
}

/// One polled observation: every numeric value parseable from the market line.
#[derive(Clone, Copy, Default)]
struct ChartSample {
    price: Option<f64>,
    vwap: Option<f64>,
    ema_s: Option<f64>,
    ema_l: Option<f64>,
    sma_s: Option<f64>,
    sma_l: Option<f64>,
    bb_u: Option<f64>,
    bb_l: Option<f64>,
    rsi: Option<f64>,
    macd: Option<f64>,
    volume: Option<f64>,
}

/// Which series the chart popup shows (price is always shown). State lives in the
/// main window; the popup's checkboxes postMessage toggles back.
#[derive(Clone, Copy)]
struct ChartToggles {
    vwap: bool,
    ema: bool,
    sma: bool,
    bb: bool,
    rsi: bool,
    macd: bool,
    vol: bool,
}

/// A labelled line for one chart panel: a colour and the per-bar values.
struct Series<'a> {
    color: &'a str,
    name: &'a str,
    vals: Vec<Option<f64>>,
}

/// A count with thousands separators and no decimals ("358691760" → "358,691,760").
/// Used for the volume axis, where the value is a number of shares: the fraction is
/// meaningless and the magnitude is what the reader needs.
fn grouped_count(v: f64) -> String {
    if !v.is_finite() {
        return String::new();
    }
    let neg = v < 0.0;
    let digits = format!("{:.0}", v.abs());
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if neg {
        format!("-{out}")
    } else {
        out
    }
}

/// Render one stacked panel (box + grid guides + min/max + legend). Series are
/// drawn as polylines, or as vertical bars when `bars` is set (for volume).
/// `range` fixes the y-axis (else auto from the data); `guides` draws dashed
/// reference lines (e.g. RSI 30/70, MACD 0).
#[allow(clippy::too_many_arguments)]
fn panel_svg(
    title: &str,
    y0: f64,
    height: f64,
    series: &[Series],
    range: Option<(f64, f64)>,
    guides: &[f64],
    n: usize,
    pad_l: f64,
    x_right: f64,
    bars: bool,
    times: &[String],
    // Indices where a new trading day begins, drawn as a vertical divider in every
    // panel so a big overnight gap is not read as an intraday move. Empty when
    // every bar IS a day (daily and coarser): there is no gap inside the series to
    // mark, and a divider at every bar is noise, not information.
    dividers: &[usize],
    // The x-axis labels to draw in this panel: `(bar index, text)`, already thinned
    // by the caller so they cannot overlap. Empty for every panel but the top one.
    x_labels: &[(usize, String)],
) -> String {
    let (pad_t, pad_b) = (18.0_f64, 16.0_f64);
    let (top, bot) = (y0 + pad_t, y0 + height - pad_b);
    let (mut lo, mut hi) = range.unwrap_or((f64::INFINITY, f64::NEG_INFINITY));
    if range.is_none() {
        for s in series {
            for v in s.vals.iter().flatten() {
                lo = lo.min(*v);
                hi = hi.max(*v);
            }
        }
        for g in guides {
            lo = lo.min(*g);
            hi = hi.max(*g);
        }
        if !lo.is_finite() || !hi.is_finite() {
            lo = 0.0;
            hi = 1.0;
        }
        let pad = ((hi - lo) * 0.08).max(1e-6);
        lo -= pad;
        hi += pad;
    }
    if (hi - lo).abs() < 1e-9 {
        lo -= 1.0;
        hi += 1.0;
    }
    let yof = |v: f64| top + (bot - top) * (1.0 - (v - lo) / (hi - lo));
    let poly = |vals: &[Option<f64>]| -> String {
        if n == 1 {
            return vals
                .first()
                .copied()
                .flatten()
                .map(|v| format!("{:.1},{:.1} {:.1},{:.1}", pad_l, yof(v), x_right, yof(v)))
                .unwrap_or_default();
        }
        vals.iter()
            .enumerate()
            .filter_map(|(i, ov)| {
                ov.map(|v| {
                    let xx = pad_l + (x_right - pad_l) * (i as f64) / ((n - 1) as f64);
                    format!("{:.1},{:.1}", xx, yof(v))
                })
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let _ = title; // panel is labeled by its legend (below), not a separate title
    let cx_at = |i: usize| {
        if n <= 1 {
            (pad_l + x_right) / 2.0
        } else {
            pad_l + (x_right - pad_l) * (i as f64) / ((n - 1) as f64)
        }
    };
    let fmt_v = |v: f64| {
        if v.abs() >= 1000.0 {
            format!("{v:.0}")
        } else {
            format!("{v:.2}")
        }
    };
    // Hover tooltip text: include the bar's time so a point's when/what is visible.
    // Escaped as one string so it is safe inside the `data-tip` attribute (a custom
    // DOM tooltip renders it — WKWebView does not show SVG `<title>` tooltips).
    let tooltip = |i: usize, name: &str, v: f64| -> String {
        let raw = match times.get(i) {
            Some(t) if !t.is_empty() => format!("{} — {}: {}", t, name, fmt_v(v)),
            _ => format!("{}: {}", name, fmt_v(v)),
        };
        html_escape(&raw)
    };

    let mut svg = String::new();
    // Per-panel clip so nothing (tall volume bars, line overshoot) draws outside
    // the frame.
    let cid = format!("clip{}", y0 as i64);
    svg.push_str(&format!(
        "<clipPath id=\"{cid}\"><rect x=\"{pad_l:.1}\" y=\"{top:.1}\" width=\"{wid:.1}\" height=\"{hgt:.1}\"/></clipPath>\
         <rect x=\"{pad_l:.1}\" y=\"{top:.1}\" width=\"{wid:.1}\" height=\"{hgt:.1}\" fill=\"none\" stroke=\"#2a2f3a\"/>",
        wid = x_right - pad_l,
        hgt = bot - top,
    ));

    // X of the boundary *between* bars i-1 and i (mid-gap), matching the
    // point layout so the divider aligns across stacked panels.
    let bound_x = |i: usize| -> f64 {
        if n <= 1 {
            (pad_l + x_right) / 2.0
        } else {
            pad_l + (x_right - pad_l) * (i as f64 - 0.5) / ((n - 1) as f64)
        }
    };

    svg.push_str(&format!("<g clip-path=\"url(#{cid})\">"));
    // Guide lines (dashed).
    for g in guides {
        let gy = yof(*g);
        svg.push_str(&format!(
            "<line x1=\"{pad_l:.1}\" y1=\"{gy:.1}\" x2=\"{x_right:.1}\" y2=\"{gy:.1}\" stroke=\"#3a4150\" stroke-dasharray=\"3 3\"/>"
        ));
    }
    // Session boundaries: a vertical divider at each new-day index (drawn behind
    // the series). Makes overnight gaps legible instead of looking intraday.
    for i in dividers {
        let bx = bound_x(*i);
        svg.push_str(&format!(
            "<line x1=\"{bx:.1}\" y1=\"{top:.1}\" x2=\"{bx:.1}\" y2=\"{bot:.1}\" stroke=\"#5b6472\" stroke-dasharray=\"2 3\"/>"
        ));
    }
    if bars {
        // Bars use a band layout: each point gets an equal-width slot and the bar
        // sits centered inside it, so every bar has the SAME width and none is
        // clipped at the frame edges (the width carries no meaning).
        let slot = (x_right - pad_l) / (n.max(1) as f64);
        let bw = (slot * 0.6).max(1.0);
        let baseline = yof(lo);
        for s in series {
            for (i, ov) in s.vals.iter().enumerate() {
                if let Some(v) = ov.filter(|v| v.is_finite()) {
                    let cx = pad_l + (i as f64 + 0.5) * slot;
                    let y = yof(v);
                    let h = (baseline - y).max(0.0);
                    svg.push_str(&format!(
                        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\" opacity=\"0.55\" data-tip=\"{}\"></rect>",
                        cx - bw / 2.0, y, bw, h, s.color, tooltip(i, s.name, v)
                    ));
                }
            }
        }
    } else {
        // Polylines + a visible marker at each point, with a larger transparent
        // hover target carrying a `data-tip` tooltip (the small dot is too tight to
        // hover). Skipped past 250 points to keep the DOM light.
        let show_dots = n <= 250;
        for s in series {
            let pts = poly(&s.vals);
            if !pts.is_empty() {
                svg.push_str(&format!(
                    "<polyline points=\"{pts}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.8\"/>",
                    s.color
                ));
            }
            if show_dots {
                for (i, ov) in s.vals.iter().enumerate() {
                    if let Some(v) = ov.filter(|v| v.is_finite()) {
                        let (cx, cy) = (cx_at(i), yof(v));
                        svg.push_str(&format!(
                            "<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"2.4\" fill=\"{}\"/>\
                             <circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"8\" fill=\"transparent\" pointer-events=\"all\" data-tip=\"{}\"></circle>",
                            s.color, tooltip(i, s.name, v)
                        ));
                    }
                }
            }
        }
    }
    svg.push_str("</g>");

    // Date label at each session boundary (top panel only), just under the top
    // edge, so the user can read which day the divider opens.
    {
        for (i, label) in x_labels {
            let bx = bound_x(*i);
            svg.push_str(&format!(
                "<text class=\"lbl\" x=\"{bx:.1}\" y=\"{ty:.1}\" text-anchor=\"middle\" fill=\"#9aa3b2\">{}</text>",
                html_escape(label),
                ty = top - 3.0,
            ));
        }
    }

    // y min/max labels (left) + a legend (series names, at the left edge). A share
    // count is a whole number in the hundreds of millions, so two decimals and no
    // separators ("358691760.00") cannot be read at a glance; it is grouped instead.
    let (hi_lbl, lo_lbl) = if bars {
        (grouped_count(hi), grouped_count(lo))
    } else {
        (format!("{hi:.2}"), format!("{lo:.2}"))
    };
    svg.push_str(&format!(
        "<text class=\"lbl\" x=\"6\" y=\"{tyy:.1}\">{hi_lbl}</text>\
         <text class=\"lbl\" x=\"6\" y=\"{byy:.1}\">{lo_lbl}</text>",
        tyy = top + 4.0,
        byy = bot,
    ));
    // Guide value labels on the right.
    for g in guides {
        svg.push_str(&format!(
            "<text class=\"lbl\" x=\"{lx:.1}\" y=\"{ty:.1}\">{g:.0}</text>",
            lx = x_right + 3.0,
            ty = yof(*g) + 4.0,
        ));
    }
    let mut lx = pad_l;
    for s in series {
        svg.push_str(&format!(
            "<text class=\"lbl\" fill=\"{}\" x=\"{lx:.1}\" y=\"{ty:.1}\">{}</text>",
            s.color,
            s.name,
            ty = y0 + 13.0,
        ));
        lx += 12.0 * (s.name.chars().count() as f64) + 14.0;
    }
    svg
}

/// Build the standalone multi-panel chart document (price + overlays, RSI, MACD)
/// as inline SVG, with toggle checkboxes. `points` are (time, sample) from polling.
/// Concise confirmed-value summary of a brushed chart interval, built from the
/// already-computed `chart_points` (so it's the same SOT numbers, no fabrication).
/// Attached as reference grounding to the user's chat message.
/// One raw-data column of a brushed interval: (chart legend name, per-bar
/// extractor). Mirrors a series drawn on the chart.
type RawCol = (&'static str, fn(&ChartSample) -> Option<f64>);

/// Display-round a grounding value to the display spec: at most 4 decimals,
/// trailing zeros trimmed (so prices read `333.74`, MACD `6.4642`, volume stays
/// integer). Keeps the attached chart-interval summary consistent with the
/// panels instead of leaking a raw f64 into the LLM prompt.
fn fmt_grounding(x: f64) -> String {
    let s = format!("{x:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn range_summary(seg: &[(String, ChartSample)], t: ChartToggles) -> String {
    if seg.is_empty() || seg.iter().all(|(_, s)| s.price.is_none()) {
        return String::new();
    }
    // Columns mirror EXACTLY the series drawn on the chart (price always; the rest
    // per the user's toggles), using the chart's own legend names. Every bar, no
    // downsampling / aggregation / relabeling — only display-rounded (≤4 dp, the
    // display spec) so the grounding matches the panels and the LLM does not echo
    // raw f64 like 6.464188372238425.
    let mut cols: Vec<RawCol> = Vec::new();
    cols.push(("Price", |s| s.price));
    if t.vwap {
        cols.push(("VWAP", |s| s.vwap));
    }
    if t.ema {
        cols.push(("EMA", |s| s.ema_s));
        cols.push(("EMA·L", |s| s.ema_l));
    }
    if t.sma {
        cols.push(("SMA", |s| s.sma_s));
        cols.push(("SMA·L", |s| s.sma_l));
    }
    if t.bb {
        cols.push(("BB↑", |s| s.bb_u));
        cols.push(("BB↓", |s| s.bb_l));
    }
    if t.rsi {
        cols.push(("RSI", |s| s.rsi));
    }
    if t.macd {
        cols.push(("MACD", |s| s.macd));
    }
    if t.vol {
        cols.push(("Vol", |s| s.volume));
    }

    let names: Vec<&str> = cols.iter().map(|c| c.0).collect();
    let mut out = format!("time / {}", names.join(" / "));
    for (tm, s) in seg {
        let vals: Vec<String> = cols
            .iter()
            .map(|c| (c.1)(s).map(fmt_grounding).unwrap_or_else(|| "-".into()))
            .collect();
        out.push_str(&format!("\n{tm}: {}", vals.join(" / ")));
    }
    out
}

fn chart_doc_html(
    points: &[(String, ChartSample)],
    symbol: &str,
    tf_label: &str,
    t: ChartToggles,
    lang: &str,
) -> String {
    // The ticker goes into this popup's innerHTML; escape it like every other
    // value here so a crafted symbol cannot inject markup/script.
    let symbol = html_escape(symbol);
    // Same-origin `<link>` to the served chart stylesheet (style-src 'self').
    let css = popup_css_link("xoksa-chart.css");
    let title = if lang == "ja" {
        "市況チャート"
    } else {
        "Market chart"
    };
    let tf = if tf_label.is_empty() {
        String::new()
    } else {
        format!(" <span class=\"tf\">{}</span>", html_escape(tf_label))
    };
    if points.is_empty() {
        let msg = if lang == "ja" {
            "価格データ取得待ち（自動更新で表示されます）"
        } else {
            "Waiting for price data (shown after the next refresh)"
        };
        return format!(
            "<head><meta charset=\"utf-8\"><title>{title}</title>{css}</head>\
             <body><h1>{symbol}{tf}</h1><p class=\"note\">{msg}</p></body>"
        );
    }
    let n = points.len();
    let col = |f: fn(&ChartSample) -> Option<f64>| {
        points
            .iter()
            .map(|(_, s)| f(s))
            .collect::<Vec<Option<f64>>>()
    };
    let last = points[n - 1].1.price.unwrap_or(0.0);

    let (w, pad_l) = (860.0_f64, 64.0_f64);
    let x_right = w - 44.0; // room on the right for guide-line labels
    let (h_price, h_osc, gap) = (250.0_f64, 130.0_f64, 8.0_f64);

    // Price panel: price + optional overlays (all price-scale).
    let mut price_series = vec![Series {
        color: "#e6e9ef",
        name: tr(lang, "価格", "Price"),
        vals: col(|s| s.price),
    }];
    if t.vwap {
        price_series.push(Series {
            color: "#d9b341",
            name: "VWAP",
            vals: col(|s| s.vwap),
        });
    }
    if t.ema {
        price_series.push(Series {
            color: "#4cd1ff",
            name: "EMA",
            vals: col(|s| s.ema_s),
        });
        price_series.push(Series {
            color: "#2a7fa0",
            name: "EMA·L",
            vals: col(|s| s.ema_l),
        });
    }
    if t.sma {
        price_series.push(Series {
            color: "#ff9d4c",
            name: "SMA",
            vals: col(|s| s.sma_s),
        });
        price_series.push(Series {
            color: "#a8662a",
            name: "SMA·L",
            vals: col(|s| s.sma_l),
        });
    }
    if t.bb {
        // Bollinger bands (upper/lower) — the range envelope, same colour.
        price_series.push(Series {
            color: "#9b7fd4",
            name: "BB↑",
            vals: col(|s| s.bb_u),
        });
        price_series.push(Series {
            color: "#9b7fd4",
            name: "BB↓",
            vals: col(|s| s.bb_l),
        });
    }

    // Per-bar time labels, so hover tooltips can show "<time> — <name>: <value>".
    let times: Vec<String> = points.iter().map(|(t, _)| t.clone()).collect();

    // Indices where the calendar date (first 10 chars of the "YYYY-MM-DD HH:MM"
    // stamp) changes.
    let day_of = |s: &str| s.get(0..10).unwrap_or("").to_string();
    let new_day: Vec<usize> = (1..n)
        .filter(|&i| day_of(&times[i]) != day_of(&times[i - 1]))
        .collect();

    // On an intraday chart those indices are session boundaries, and a divider
    // there is what keeps an overnight gap from reading as an intraday move. On a
    // daily or coarser chart EVERY bar is a new date, so the same rule drew a
    // divider and a date label at all 64 of them — a band of overlapping text
    // across the top of the price panel, and dividers that marked nothing. The
    // series itself says which it is: as many boundaries as bars means one bar per
    // day or longer.
    let one_bar_per_day = new_day.len() + 1 >= n;
    let dividers: Vec<usize> = if one_bar_per_day {
        Vec::new()
    } else {
        new_day.clone()
    };

    // The labels to actually draw, thinned so they cannot collide: at most this
    // many across the width, chosen at even spacing through the candidates.
    const MAX_X_LABELS: usize = 8;
    // A daily or coarser series can span years, so its labels carry the year
    // ("26/09/30"); an intraday one is a single day or a few, where "09/30" reads
    // better and the time of day is already in the first/last stamp under the axis.
    let label_at = |i: usize| -> String {
        let t = &times[i];
        if one_bar_per_day {
            t.get(2..10).unwrap_or("").replace('-', "/")
        } else {
            t.get(5..10).unwrap_or("").replace('-', "/")
        }
    };
    let candidates: Vec<usize> = if one_bar_per_day {
        (0..n).collect()
    } else {
        new_day.clone()
    };
    let step = candidates.len().div_ceil(MAX_X_LABELS).max(1);
    let x_labels: Vec<(usize, String)> = candidates
        .iter()
        .step_by(step)
        .map(|&i| (i, label_at(i)))
        .collect();

    let mut panels = String::new();
    let mut y = 0.0_f64;
    panels.push_str(&panel_svg(
        "Price",
        y,
        h_price,
        &price_series,
        None,
        &[],
        n,
        pad_l,
        x_right,
        false,
        &times,
        &dividers,
        &x_labels,
    ));
    y += h_price + gap;
    if t.rsi {
        let rsi = vec![Series {
            color: "#c08cff",
            name: "RSI",
            vals: col(|s| s.rsi),
        }];
        panels.push_str(&panel_svg(
            "RSI",
            y,
            h_osc,
            &rsi,
            Some((0.0, 100.0)),
            &[30.0, 70.0],
            n,
            pad_l,
            x_right,
            false,
            &times,
            &dividers,
            &[],
        ));
        y += h_osc + gap;
    }
    if t.macd {
        let macd = vec![Series {
            color: "#2ecc71",
            name: "MACD",
            vals: col(|s| s.macd),
        }];
        panels.push_str(&panel_svg(
            "MACD",
            y,
            h_osc,
            &macd,
            None,
            &[0.0],
            n,
            pad_l,
            x_right,
            false,
            &times,
            &dividers,
            &[],
        ));
        y += h_osc + gap;
    }
    if t.vol {
        // Volume as bars from a 0 baseline (own scale, padded headroom).
        let vols = col(|s| s.volume);
        let vmax = vols.iter().flatten().cloned().fold(0.0_f64, f64::max);
        let vol = vec![Series {
            color: "#5a7fb0",
            name: "Vol",
            vals: vols,
        }];
        let vrange = Some((0.0, if vmax > 0.0 { vmax * 1.2 } else { 1.0 }));
        panels.push_str(&panel_svg(
            "Volume",
            y,
            h_osc,
            &vol,
            vrange,
            &[],
            n,
            pad_l,
            x_right,
            true,
            &times,
            &dividers,
            &[],
        ));
        y += h_osc + gap;
    }
    let total_h = (y - gap).max(h_price);

    // X-axis time labels (shared) under everything.
    let t_first = html_escape(&points[0].0);
    let t_last = html_escape(&points[n - 1].0);
    let xlabels = format!(
        "<text class=\"lbl\" x=\"{pad_l}\" y=\"{ty:.1}\">{t_first}</text>\
         <text class=\"lbl\" x=\"{x_right}\" y=\"{ty:.1}\" text-anchor=\"end\">{t_last}</text>",
        ty = total_h - 2.0,
    );

    // Toggle checkboxes (state lives in the opener). `data-toggle` + the delegated
    // click listener flips the matching series signal directly (no inline onclick → CSP-safe).
    let cb = |key: &str, label: &str, on: bool| -> String {
        let checked = if on { " checked" } else { "" };
        format!("<label><input type=\"checkbox\"{checked} data-toggle=\"{key}\">{label}</label>")
    };
    let cbs = format!(
        "<div class=\"cbs\">{}{}{}{}{}{}{}</div>",
        cb("vwap", "VWAP", t.vwap),
        cb("ema", "EMA", t.ema),
        cb("sma", "SMA", t.sma),
        cb("bb", "BB", t.bb),
        cb("rsi", "RSI", t.rsi),
        cb("macd", "MACD", t.macd),
        cb("vol", "Vol", t.vol),
    );

    let foot = html_escape(&if lang == "ja" {
        format!(
            "足: {} ・ 市況データ（直近 {n} 足） ・ 🔎 ドラッグで区間選択→本体のコメントに参照として添付",
            if tf_label.is_empty() { "—" } else { tf_label }
        )
    } else {
        format!(
            "Bar: {} · market data (last {n} bars) · 🔎 drag to select an interval → attached as reference to your comment",
            if tf_label.is_empty() { "—" } else { tf_label }
        )
    });

    // Latest-bar time in the header, so a chart that stops at the last session's
    // close (e.g. a market holiday) reads as "as of <time>", not a stale bug.
    let asof_label = if lang == "ja" {
        "データ最新"
    } else {
        "as of"
    };
    format!(
        "<head><meta charset=\"utf-8\"><title>{title} — {symbol}</title>{css}</head>\
         <body><h1>{symbol}{tf} <span class=\"px\">{last:.2}</span> <span class=\"asof\">{asof_label}: {t_last}</span></h1>{cbs}\
         <svg viewBox=\"0 0 {w} {total_h:.0}\" preserveAspectRatio=\"xMidYMid meet\">{panels}{xlabels}</svg>\
         <p class=\"note\">{foot}</p><div id=\"chart-tip\"></div></body>"
    )
}

/// Latest successful summary (`ok == true`), if any.
fn summary_ok(summary: LocalResource<Result<SummaryResponse, String>>) -> Option<SummaryResponse> {
    match summary.get() {
        Some(Ok(d)) if d.ok => Some(d),
        _ => None,
    }
}

fn fundamental_view(summary: LocalResource<Result<SummaryResponse, String>>) -> AnyView {
    // Rendered server-side (native `render_fundamental_display`) and shown as-is,
    // exactly like the technical panel — no client-side number formatting.
    match summary.get() {
        Some(Ok(d)) if d.ok => match d.fundamental {
            Some(lines) if !lines.is_empty() => {
                view! { <pre class="analysis" inner_html=analysis_html(&lines.join("\n"))></pre> }
                    .into_any()
            }
            _ => view! { <p class="muted">{d.fundamental_note.unwrap_or_default()}</p> }.into_any(),
        },
        _ => view! { <p class="muted">"—"</p> }.into_any(),
    }
}

fn main() {
    console_error_panic_hook::set_once();
    // Desktop opens chart/help in native windows via `/?popup=…`; render that view
    // directly instead of mounting the full dashboard.
    match query_param("popup").as_deref() {
        Some("chart") => {
            run_chart_popup();
            return;
        }
        Some("help") => {
            run_help_popup();
            return;
        }
        _ => {}
    }
    leptos::mount::mount_to_body(App);
}
