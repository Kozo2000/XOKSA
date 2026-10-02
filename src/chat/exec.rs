//! Unified slash-command dispatch, shared by the CLI REPL and the Web UI server.
//!
//! `ChatSession::execute` runs one input line against the session and writes its
//! output to a [`ChatOut`] sink (the CLI prints immediately; the Web UI buffers
//! lines to return as JSON). One implementation → identical results on both
//! front-ends (SOT / no duplication).
//!
//! Migration note: commands are being moved here one batch at a time. Anything
//! not yet migrated returns [`Dispatch::Passthrough`] so the CLI REPL keeps
//! handling it inline. When every command is migrated the REPL's inline arms are
//! gone and this is the single dispatch.

use super::report::write_chat_status;
use super::*;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

/// Non-session, non-REPL state a command may read or modify. The autoreload
/// timer fields are `&mut` so state commands (/mode, /autoreload, /reload) write
/// back to the CLI REPL's live timer; the Web passes throwaway locals (it has no
/// timer), so the same command runs identically — only the timer side-effect is
/// inert on the Web.
pub(crate) struct ExecCtx<'a> {
    pub config: &'a mut Config,
    pub ticker_name_map: &'a HashMap<String, String>,
    pub cumulative_tokens: &'a mut ChatTokenUsage,
    /// Autoreload enable flag (the CLI REPL timer reads/writes this).
    pub autoreload_enabled: &'a mut bool,
    /// Autoreload per-ticker price-change notice toggle.
    pub autoreload_notice: &'a mut bool,
    /// Next scheduled reload (CLI REPL timer state; used for the `/status` countdown).
    pub next_reload_at: &'a mut Option<tokio::time::Instant>,
    /// Prices from the last completed reload (CLI REPL timer diff state).
    pub autoreload_last_prices: &'a mut Vec<Option<f64>>,
    pub lang: &'a str,
    /// True for the interactive CLI REPL (LLM calls race Ctrl-C for cancellation
    /// and stream to stdout); false for the stateless Web request (buffered, no
    /// signal handling). Only the transport differs — the prompt/logic is shared.
    pub interactive: bool,
}

/// Outcome of dispatching one input line.
pub(crate) enum Dispatch {
    /// Command handled; output already written to the sink.
    Handled,
    /// `/bye` — the caller should end the session.
    Exit,
    /// Not a migrated command; the caller continues its own handling.
    Passthrough,
}

/// Build a `ChatSession` from a seed and apply the config-derived defaults
/// (memory, debate mode, response styles). Shared by the CLI loop and the Web
/// server so session setup lives in one place.
pub(super) fn new_configured(
    seed: ChatSessionSeed,
    articles: &[Article],
    config: &Config,
) -> ChatSession {
    let params = parse_memory(&config.chat_memory);
    let mut s = ChatSession::new(seed, articles, params, &config.chat_guard, &config.lang);
    s.set_debate_mode(&config.debate);
    s.read_depth = config.read_depth.clone();
    s.knowledge_scope = config.knowledge_scope.clone();
    s.response_shape = config.response_shape.clone();
    s.forecast_mode = config.forecast_mode.clone();
    s
}

/// Build a fully-loaded chat session for `symbol` by reusing the native analysis
/// pipeline (same data the CLI chat loads). Best-effort fundamentals/news.
async fn build_web_session(
    symbol: &str,
    config: &Config,
    ticker_name_map: &HashMap<String, String>,
) -> Result<ChatSession> {
    let mut cfg = config.clone();
    cfg.analysis_mode = config.analysis_mode;
    cfg.no_news = false; // the chat session loads news (like the CLI chat)
    cfg.show_news = false; // data only; no terminal output from the news flow

    let guard = crate::app::build_analyzed_guard(&cfg, symbol, ticker_name_map).await?;
    let technical = crate::app::collect_display_lines(&cfg, &guard).join("\n");

    let latest = guard
        .get_latest_observed_price()
        .unwrap_or_else(|| guard.get_close());
    // Keep the fundamental data itself, not only its rendered text: the confirmed
    // facts the output guard verifies against are built from the structure.
    let fundamental_data = crate::fundamental::fetch_fundamental_data(symbol, &cfg, Some(latest))
        .await
        .ok();
    let fundamental = fundamental_data
        .as_ref()
        .map(|d| crate::fundamental::render_fundamental_display(d, &cfg.lang).join("\n"))
        .unwrap_or_default();

    let articles: Vec<Article> = if cfg.no_news {
        Vec::new()
    } else {
        let fetcher = crate::news::BraveArticleFetcher {
            proxy_url: cfg.https_proxy.clone(),
            no_proxy: cfg.no_proxy.clone(),
        };
        crate::news::news_flow_controller(
            &crate::news::NewsSubject::from_guard(&guard),
            &cfg,
            &fetcher,
        )
        .await
        .unwrap_or_default()
    };

    // base_context carries NO news: the model gets news solely from the live news
    // buffer (news_items(), refreshed per turn from the panel store), so `/basic` and
    // `/prompt` reflect the currently-displayed news instead of a copy frozen at
    // session-build (and no longer show it twice). Build it on a no-news config clone.
    let mut base_cfg = cfg.clone();
    base_cfg.no_news = true;
    let base_context = crate::prompt::build_analysis_prompt(&base_cfg, &guard, None, None);
    let ticker_label = format!("{} ({})", guard.get_ticker(), guard.get_name());
    let data_as_of = guard
        .get_market_data_latest_time()
        .or_else(|| guard.get_analyzed_at())
        .unwrap_or("")
        .to_string();

    Ok(new_configured(
        ChatSessionSeed {
            base_context,
            technical_text: technical,
            fundamental_text: fundamental,
            facts: Some(crate::integrity::SymbolFacts::from_sources(
                &guard,
                &cfg,
                None,
                fundamental_data.as_ref(),
            )),
            ticker_label,
            data_as_of,
        },
        &articles,
        config,
    ))
}

/// One persistent Web chat session's state. The CLI keeps a single session +
/// config + token counter for the whole REPL; the Web must persist the same
/// across `/api/chat` requests, otherwise per-request rebuilds discard them.
/// `config` carries chat-command mutations that live on `Config` (e.g. `/llm`
/// provider/model); `tokens` accumulates usage across turns. State that lives on
/// `ChatSession` (tune/memory/forum/debate/ticker set/news-extra) persists via
/// `session`.
struct WebSession {
    session: ChatSession,
    config: Config,
    tokens: ChatTokenUsage,
    /// UI language the cached rendered contexts (base_context task template,
    /// technical/fundamental/news text) were built in. When a request arrives in a
    /// different language, those contexts are rebuilt so `/basic` and the analysis
    /// template follow the current language.
    built_lang: String,
    /// When the grounding (market data / indicators) was last (re)loaded. The Web has
    /// no autoreload timer, so a long-open session is refreshed lazily: the first turn
    /// after the timeframe's confirmed-bar cadence elapses reloads the data (keeping
    /// the conversation), so chat never answers from a snapshot older than the
    /// dashboard's live panels.
    last_data_refresh: std::time::Instant,
}

/// A cached session behind its own mutex, plus its LRU timestamp. `last_used`
/// lives here on the outer map — not inside `WebSession` — so eviction can compare
/// timestamps under the brief outer-map lock without locking each per-session
/// mutex. A chat turn locks only its own `session` mutex (see `run_web_command`),
/// so a slow/stuck LLM turn on one session no longer stalls every other request on
/// a single global lock (OWASP availability / self-DoS).
struct SessionSlot {
    session: Arc<tokio::sync::Mutex<WebSession>>,
    /// Last access time, for LRU eviction so the map cannot grow without bound
    /// (OWASP API4 — unrestricted resource consumption).
    last_used: std::time::Instant,
}

/// Cap on retained Web chat sessions (per symbol|timeframe). A single user only
/// keeps a handful active; the cap bounds memory if many combinations are opened.
const MAX_WEB_SESSIONS: usize = 64;

/// Grounding-refresh cadence for non-intraday timeframes (daily/weekly/monthly),
/// which have no `autoreload_secs`. Bounds how stale a long-open chat session's data
/// can be: the first turn after this interval reloads it. Small enough that a session
/// resumed the next trading day (the reported bug) refreshes promptly; large enough
/// that rapid back-and-forth within it does not re-fetch on every message.
const WEB_DATA_REFRESH_FALLBACK_SECS: u64 = 300;

/// Persistent Web chat sessions, keyed by `symbol|timeframe`, so stateful
/// commands and the conversation carry across `/api/chat` requests — the CLI
/// keeps one session for the whole REPL; the Web used to rebuild one per call
/// (state was lost). Local-first single-user, so a process-wide map is enough.
/// The outer mutex guards only the map (get/insert/evict — never held across an
/// await); each session has its own inner mutex for the turn itself.
static WEB_SESSIONS: std::sync::OnceLock<tokio::sync::Mutex<HashMap<String, SessionSlot>>> =
    std::sync::OnceLock::new();

/// The client's chosen LLM — exactly the fields a `/llm` switch sets — kept per
/// client (`sid`) and INDEPENDENT of symbol/timeframe. This is the single source of
/// truth for "which model XOKSA uses for commentary": every Web chat session overlays
/// it, and the header picker, the summary badge, and the multi-timeframe endpoint all
/// read it, so the model shown is always the model actually called (and billed). Unset
/// ⇒ the env default. Replaces the old per-timeframe divergence + non-deterministic
/// sibling-session scan that made the model change at random when the timeframe
/// pulldown switched to a not-yet-opened timeframe.
#[derive(Clone)]
struct LlmSelection {
    provider: String,
    model: String,
    ollama_alias: String,
    ollama_host: String,
    ollama_port: u16,
}

struct LlmSelectionSlot {
    selection: LlmSelection,
    last_used: std::time::Instant,
}

/// Cap on retained per-client LLM selections — bounds memory if many `sid`s are
/// minted (OWASP API4, same rationale as `MAX_WEB_SESSIONS`).
const MAX_LLM_SELECTIONS: usize = MAX_WEB_SESSIONS;

/// Per-client (`sid`) LLM selection store. A std mutex: every critical section is a
/// short, synchronous map op with NO `.await` held, so a read never blocks on an
/// in-flight chat turn (the instability the old picker fallback hit, which is why the
/// `busy` path existed — no longer needed).
static LLM_SELECTIONS: std::sync::OnceLock<std::sync::Mutex<HashMap<String, LlmSelectionSlot>>> =
    std::sync::OnceLock::new();

/// Overlay the client's stored LLM selection onto `config` (no-op if none stored, so
/// the env default stands). Called at the start of every Web chat turn and by the
/// picker/badge/multi-timeframe readers, so all of them agree on one model.
pub(crate) fn apply_client_llm_selection(sid: &str, config: &mut Config) {
    let store = LLM_SELECTIONS.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut map = store.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(slot) = map.get_mut(sid) {
        slot.last_used = std::time::Instant::now();
        let s = &slot.selection;
        config.llm_provider = s.provider.clone();
        config.llm_model = s.model.clone();
        config.ollama_alias = s.ollama_alias.clone();
        config.ollama_host = s.ollama_host.clone();
        config.ollama_port = s.ollama_port;
    }
}

/// Save `config`'s current LLM selection as this client's choice (called after a
/// `/llm` switch, whether typed in chat or made via the header dropdown). Evicts the
/// least-recently-used entry when at capacity.
pub(crate) fn store_client_llm_selection(sid: &str, config: &Config) {
    let store = LLM_SELECTIONS.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut map = store.lock().unwrap_or_else(|e| e.into_inner());
    if !map.contains_key(sid) && map.len() >= MAX_LLM_SELECTIONS {
        if let Some(oldest) = map
            .iter()
            .min_by_key(|(_, slot)| slot.last_used)
            .map(|(k, _)| k.clone())
        {
            map.remove(&oldest);
        }
    }
    map.insert(
        sid.to_string(),
        LlmSelectionSlot {
            selection: LlmSelection {
                provider: config.llm_provider.clone(),
                model: config.llm_model.clone(),
                ollama_alias: config.ollama_alias.clone(),
                ollama_host: config.ollama_host.clone(),
                ollama_port: config.ollama_port,
            },
            last_used: std::time::Instant::now(),
        },
    );
}

/// The panel's last-fetched news for a client's symbol — the EXACT articles the
/// dashboard news list is showing. Written by the `/news` handler on every fetch
/// (symbol change, free-text search, or the "フィルタを外す" toggle) and read by the
/// chat before each turn, so the LLM reasons over the same confirmed news the user
/// sees (SOT), updated the moment the news-search request changes. Keyed `sid|symbol`
/// — news is timeframe-independent, so every timeframe session for the symbol shares
/// one entry. A std mutex; every critical section is a short map op with no `.await`.
struct PanelNewsSlot {
    arts: Vec<crate::news::Article>,
    last_used: std::time::Instant,
}

/// Cap retained per-(client,symbol) news sets — bounds memory (OWASP API4).
const MAX_PANEL_NEWS: usize = MAX_WEB_SESSIONS;

static PANEL_NEWS: std::sync::OnceLock<std::sync::Mutex<HashMap<String, PanelNewsSlot>>> =
    std::sync::OnceLock::new();

fn panel_news_key(sid: &str, symbol: &str) -> String {
    format!("{sid}|{symbol}")
}

/// Store the panel's freshly-fetched articles as this client's current news for
/// `symbol`. Overwrites unconditionally — an empty set clears it — so the store
/// always equals the panel's last display. Evicts the LRU entry at capacity.
pub(crate) fn store_panel_news(sid: &str, symbol: &str, arts: &[crate::news::Article]) {
    let store = PANEL_NEWS.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut map = store.lock().unwrap_or_else(|e| e.into_inner());
    let key = panel_news_key(sid, symbol);
    if !map.contains_key(&key) && map.len() >= MAX_PANEL_NEWS {
        if let Some(oldest) = map
            .iter()
            .min_by_key(|(_, slot)| slot.last_used)
            .map(|(k, _)| k.clone())
        {
            map.remove(&oldest);
        }
    }
    map.insert(
        key,
        PanelNewsSlot {
            arts: arts.to_vec(),
            last_used: std::time::Instant::now(),
        },
    );
}

/// The panel's current news for `(sid, symbol)` as chat `NewsItem`s (same ids/order
/// as the session buffer), or `None` if the panel never fetched for this client+symbol
/// (the CLI has no panel → always `None` → the chat keeps its own fetched news).
pub(super) fn panel_news_items(sid: &str, symbol: &str) -> Option<Vec<super::NewsItem>> {
    let store = PANEL_NEWS.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut map = store.lock().unwrap_or_else(|e| e.into_inner());
    map.get_mut(&panel_news_key(sid, symbol)).map(|slot| {
        slot.last_used = std::time::Instant::now();
        slot.arts
            .iter()
            .enumerate()
            .map(|(i, a)| super::NewsItem {
                id: format!("N{:02}", i + 1),
                title: a.title.clone(),
                url: a.url.clone(),
            })
            .collect()
    })
}

/// One-shot "refresh the fundamental" flags, keyed `sid|symbol`, set by a manual
/// "今すぐ更新" (the `/summary` `force` path) and consumed by the chat's next grounding
/// refresh. Fundamentals change quarterly, so the periodic refresh skips them; this
/// lets a long-open session pull a fresh fundamental on demand without an always-on
/// per-turn fetch. A std mutex; short synchronous critical sections, no `.await` held.
static FORCE_FUNDAMENTAL: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::OnceLock::new();

/// Mark this client+symbol for a one-shot fundamental refresh on the next chat turn.
pub(crate) fn store_force_fundamental(sid: &str, symbol: &str) {
    let store =
        FORCE_FUNDAMENTAL.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    let mut set = store.lock().unwrap_or_else(|e| e.into_inner());
    // Bound memory (API4): flags are normally cleared on the next turn, but if a client
    // forces without ever chatting, drop the accumulation.
    if set.len() >= MAX_WEB_SESSIONS {
        set.clear();
    }
    set.insert(panel_news_key(sid, symbol));
}

/// Return and clear the one-shot fundamental-refresh flag for this client+symbol.
pub(super) fn take_force_fundamental(sid: &str, symbol: &str) -> bool {
    let store =
        FORCE_FUNDAMENTAL.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    let mut set = store.lock().unwrap_or_else(|e| e.into_inner());
    set.remove(&panel_news_key(sid, symbol))
}

/// The `provider/model` badge for this client's effective LLM — its selection overlaid
/// on `base`, else `base`'s env default. One source for the header/summary badge and
/// the `/llm/select` reply, so the displayed model never drifts from the used one.
pub(crate) fn client_llm_label(sid: &str, base: &Config) -> String {
    let mut c = base.clone();
    apply_client_llm_selection(sid, &mut c);
    super::llm::env_llm_label(&c)
}

/// Core Web runner: dispatch one slash command (or free-text) for `symbol` (the
/// active/primary ticker) against the persistent session, keeping the session's
/// comparison set in sync with `extras` (the other header chips), writing output
/// to `out` (a `ChatOut::Channel` sink streams each line live over SSE). Emits a
/// final `[[XOKSA_TICKERS]]…` control line so the Web UI chips reflect the loaded
/// set (e.g. after `/sym add` in chat).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_web_command(
    sid: &str,
    symbol: &str,
    raw_input: &str,
    config: &Config,
    ticker_name_map: &HashMap<String, String>,
    extras: &[String],
    out: &mut ChatOut,
) {
    let key = format!("{}|{}|{}", sid, symbol, config.analysis_mode.as_str());
    let store = WEB_SESSIONS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()));

    // Resolve the per-session handle. The outer map is locked only briefly and
    // never across an await, so a long (or stuck) chat turn on one session can no
    // longer stall requests for other sessions — nor the summary model badge — on
    // a single process-wide lock.
    let existing: Option<Arc<tokio::sync::Mutex<WebSession>>> = {
        let mut sessions = store.lock().await;
        sessions.get_mut(&key).map(|slot| {
            slot.last_used = std::time::Instant::now();
            slot.session.clone()
        })
    };
    // Build + cache the session on first use for this symbol+timeframe. The build
    // (network I/O) runs WITHOUT holding any lock. A failed build is NOT cached, so
    // it retries next time; a transient empty session still lets no-data commands
    // like /help work. The persisted `config`/`tokens` start from this request's
    // env-built config.
    let handle: Arc<tokio::sync::Mutex<WebSession>> = match existing {
        Some(h) => h,
        None => {
            let built = build_web_session(symbol, config, ticker_name_map).await;
            let mut sessions = store.lock().await;
            if let Some(slot) = sessions.get_mut(&key) {
                // A concurrent request built it first — reuse that one.
                slot.last_used = std::time::Instant::now();
                slot.session.clone()
            } else if let Ok(s) = built {
                // Evict the least-recently-used session when at capacity (API4).
                if sessions.len() >= MAX_WEB_SESSIONS {
                    if let Some(oldest) = sessions
                        .iter()
                        .min_by_key(|(_, slot)| slot.last_used)
                        .map(|(k, _)| k.clone())
                    {
                        sessions.remove(&oldest);
                    }
                }
                let handle = Arc::new(tokio::sync::Mutex::new(WebSession {
                    session: s,
                    config: config.clone(),
                    tokens: ChatTokenUsage::default(),
                    built_lang: config.lang.clone(),
                    last_data_refresh: std::time::Instant::now(),
                }));
                sessions.insert(
                    key.clone(),
                    SessionSlot {
                        session: handle.clone(),
                        last_used: std::time::Instant::now(),
                    },
                );
                handle
            } else {
                // Build failed: a transient, un-cached session for this call only.
                Arc::new(tokio::sync::Mutex::new(WebSession {
                    session: new_configured(
                        ChatSessionSeed {
                            base_context: String::new(),
                            technical_text: String::new(),
                            fundamental_text: String::new(),
                            ticker_label: String::new(),
                            data_as_of: String::new(),
                            // Nothing loaded: no confirmed data to verify against.
                            facts: None,
                        },
                        &[],
                        config,
                    ),
                    config: config.clone(),
                    tokens: ChatTokenUsage::default(),
                    built_lang: config.lang.clone(),
                    last_data_refresh: std::time::Instant::now(),
                }))
            }
        }
    };

    // Lock ONLY this session for the turn — including the LLM call. Requests for
    // other symbols/timeframes never touch this mutex, so they run concurrently;
    // the same session serializes (correct — it is shared mutable state). Mutations
    // to `config`/`tokens` (e.g. `/llm`, token usage) survive to the next request.
    let mut ws = handle.lock().await;
    // The cached rendered contexts are language-baked at build time. If the UI
    // language changed since, rebuild them (keeping the session's `/llm` selection
    // and `/set` params, only overriding the language) so `/basic` and the analysis
    // template follow the current language instead of the first-built one.
    if ws.built_lang != config.lang {
        let mut rebuild_cfg = ws.config.clone();
        rebuild_cfg.lang = config.lang.clone();
        if let Ok(fresh) = build_web_session(symbol, &rebuild_cfg, ticker_name_map).await {
            ws.session = fresh;
            ws.built_lang = config.lang.clone();
        }
    }
    let WebSession {
        session,
        config: cfg,
        tokens,
        last_data_refresh,
        ..
    } = &mut *ws;

    // Per-request overrides onto the persistent config: the browser language, the
    // header timeframe (also the session key), and news-as-data. Everything else
    // (e.g. the LLM provider/model set via `/llm`) is preserved across requests.
    cfg.lang = config.lang.clone();
    cfg.analysis_mode = config.analysis_mode;
    cfg.no_news = false;
    cfg.show_news = false; // data only — no terminal output from the news flow

    // Single-source the LLM: overlay the client's chosen model (SOT, kept per-client
    // independent of symbol/timeframe) onto this session's config, so every timeframe
    // uses — and the header shows — the SAME model. A `/llm` switch below re-stores it.
    apply_client_llm_selection(sid, cfg);

    // Refresh the grounding when the confirmed bar could have advanced, so chat never
    // answers from a snapshot older than the dashboard's live panels. The Web has no
    // autoreload timer, so this is lazy: the first turn after the timeframe's bar
    // cadence (intraday) — or a modest fallback for daily/weekly/monthly — reloads the
    // data via the shared `/reload` path, which keeps the conversation (history /
    // memory / debate) and only swaps the data. Silent (no notice lines) and data-only
    // (no news re-fetch). A fresh/empty session is a no-op (nothing loaded yet).
    let force_fund = take_force_fundamental(sid, symbol);
    let refresh_gate_secs =
        super::ticker::autoreload_secs(cfg.analysis_mode).unwrap_or(WEB_DATA_REFRESH_FALLBACK_SECS);
    if force_fund || last_data_refresh.elapsed().as_secs() >= refresh_gate_secs {
        // `force_fund` (a manual 今すぐ更新) also re-fetches the fundamental and runs
        // regardless of the gate; the periodic refresh (fetch_fundamental=false) skips it.
        let _ = super::ticker::reload_chat_tickers(
            session,
            cfg,
            ticker_name_map,
            false,
            false,
            config.lang.as_str(),
            out,
            force_fund,
        )
        .await;
        *last_data_refresh = std::time::Instant::now();
    }

    // Keep the comparison set (header chips beyond the active one) in sync.
    super::ticker::sync_extra_tickers(session, cfg, ticker_name_map, extras, out).await;
    // Point the LLM's news buffer at exactly what the panel is displaying now (the
    // `/news` store, updated on symbol change / free-text search / "フィルタを外す"
    // toggle). Runs every turn so the request→buffer sync is immediate; a store miss
    // (CLI, or the panel not yet loaded) keeps the session's own fetched news.
    super::ticker::refresh_news_from_panel(session, sid);
    // The Web has no autoreload timer; these locals absorb any timer-state writes.
    let mut autoreload_enabled = false;
    let mut autoreload_notice = false;
    let mut next_reload_at = None;
    let mut autoreload_last_prices = Vec::new();
    let mut ctx = ExecCtx {
        config: &mut *cfg,
        ticker_name_map,
        cumulative_tokens: &mut *tokens,
        autoreload_enabled: &mut autoreload_enabled,
        autoreload_notice: &mut autoreload_notice,
        next_reload_at: &mut next_reload_at,
        autoreload_last_prices: &mut autoreload_last_prices,
        lang: config.lang.as_str(),
        interactive: false,
    };
    let dispatch = session.execute(raw_input, &mut ctx, out).await;
    if matches!(dispatch, Dispatch::Passthrough) {
        let trimmed = raw_input.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('/') {
            // Free-text turn through the EXACT same function the CLI uses
            // (send_chat_input_to_llm → session.build_prompt). interactive=false:
            // no Ctrl-C handling. Identical grounding/prompt as the terminal (SOT).
            // Every command targets the full loaded ticker table (no single-symbol
            // focus): the algorithm never changes with ticker count.
            super::llm::send_chat_input_to_llm(
                session,
                cfg,
                tokens,
                trimmed.to_string(),
                config.lang.as_str(),
                false,
                out,
            )
            .await;
        }
    }
    // Persist a `/llm` switch (typed in chat or made via the header dropdown) as this
    // client's single LLM selection, so every timeframe/symbol — and the header
    // picker/badge — uses it. Only a Switch writes; List/Net/free-text do not.
    if matches!(
        super::llm::parse_llm_switch_command(raw_input),
        Some(super::llm::LlmSwitchCmd::Switch { .. })
    ) {
        store_client_llm_selection(sid, cfg);
    }
    // Reflect the session's loaded ticker set back to the Web UI (so the header
    // chips stay in sync after e.g. `/sym add` typed in chat).
    out.line(format!(
        "[[XOKSA_TICKERS]]{}",
        super::ticker::loaded_symbols(session).join(",")
    ));
    // Reflect the active timeframe (so the header dropdown stays in sync after a
    // `/mode` change in chat).
    out.line(format!("[[XOKSA_TIMEFRAME]]{}", cfg.analysis_mode.as_str()));
}

/// Await `fut`, additionally racing Ctrl-C in interactive (CLI) mode so a long
/// LLM call can be cancelled. In non-interactive (Web) mode there is no signal to
/// handle, so it simply awaits. Returns `None` when cancelled. Shared by every
/// LLM turn (free-text, /prompt, /criticize, /council) so cancellation behaves
/// identically everywhere.
pub(super) async fn await_or_cancel<F, T>(
    fut: F,
    interactive: bool,
    lang: &str,
    out: &mut ChatOut,
) -> Option<T>
where
    F: std::future::Future<Output = T>,
{
    if !interactive {
        return Some(fut.await);
    }
    tokio::select! {
        r = fut => Some(r),
        _ = tokio::signal::ctrl_c() => {
            out.err(match lang {
                "ja" => "\n⚠️ キャンセルしました。終了する場合は /bye または Ctrl+C",
                _ => "\n⚠️ Cancelled. Type /bye or Ctrl+C to exit.",
            });
            None
        }
    }
}

/// Map a now-removed old command (first token, with slash) to its replacement,
/// so a one-time migration hint can be shown instead of "unknown command".
fn renamed_command(head: &str) -> Option<&'static str> {
    Some(match head {
        "/memory" => "/mem",
        "/ticker" => "/sym",
        "/autoreload" => "/auto",
        "/news-extra" => "/news",
        "/nx" => "/news",
        "/show" => "/tech · /funda · /news",
        "/prompt" => "/basic",
        "/council" => "/forum",
        "/criticize" => "/crit",
        "/response-shape" => "/shape",
        "/read-depth" => "/depth",
        "/forecast-mode" => "/cast",
        _ => return None,
    })
}

/// Shared handler for the six response-style setters (/answer-tone,
/// /hypothesis-mode, …): show current value when `arg` is empty, set it when
/// valid, else report the allowed values. One implementation → identical
/// behavior on CLI and Web. `valid_display` is the human list shown on error.
fn set_response_style(
    out: &mut ChatOut,
    lang: &str,
    label: &str,
    field: &mut String,
    valid: &[&str],
    valid_display: &str,
    arg: &str,
) {
    if arg.is_empty() {
        out.line(match lang {
            "ja" => format!("現在の {}: {}", label, field),
            _ => format!("Current {}: {}", label, field),
        });
    } else if valid.contains(&arg) {
        *field = arg.to_string();
        out.line(match lang {
            "ja" => format!("✅ {} を {} に設定しました。", label, arg),
            _ => format!("✅ {} set to {}.", label, arg),
        });
    } else {
        out.err(match lang {
            "ja" => format!("❌ 不明な値: {}。有効: {}", arg, valid_display),
            _ => format!("❌ Unknown value: {}. Valid: {}", arg, valid_display),
        });
    }
}

/// Current runtime-tunable analysis parameters (the `/set` group): thresholds/periods
/// AND the indicator weights (`weight-*`). Weights scale each indicator's score
/// contribution; changing one makes a later-stored final_score reflect it (comparability
/// caveat). The stored log/CSV column set is NOT affected by anything here.
fn analysis_param_lines(config: &Config) -> Vec<String> {
    vec![
        format!("  stance           : {}", config.stance),
        format!("  macd-minus-ok    : {}", config.macd_minus_ok),
        format!("  buy-rsi          : {}", config.buy_rsi),
        format!("  sell-rsi         : {}", config.sell_rsi),
        format!("  macd-diff-low    : {}", config.macd_diff_low),
        format!("  macd-diff-mid    : {}", config.macd_diff_mid),
        format!("  macd-diff-extreme: {}", config.macd_diff_extreme),
        format!("  bb-period        : {}", config.bollinger_period),
        format!("  bb-sigma         : {}", config.bollinger_stddev_multiplier),
        format!("  bb-squeeze       : {}", config.bb_bandwidth_squeeze_pct),
        format!("  adx-period       : {}", config.adx_period),
        format!("  roc-period       : {}", config.roc_period),
        format!("  stoch-period     : {}", config.stochastics_period),
        format!("  vwap-period      : {}", config.vwap_period),
        format!("  fib-ratio        : {}", config.fibonacci_neutral_ratio),
        format!(
            "  weights          : basic={} ema={} sma={} bollinger={} roc={} adx={} stochastics={} fibonacci={} vwap={} ichimoku={}",
            config.weight_basic,
            config.weight_ema,
            config.weight_sma,
            config.weight_bollinger,
            config.weight_roc,
            config.weight_adx,
            config.weight_stochastics,
            config.weight_fibonacci,
            config.weight_vwap,
            config.weight_ichimoku,
        ),
        format!(
            "  active indicators: {} (of enabled: {})",
            active_extension_names(config),
            enabled_extension_names(config),
        ),
    ]
}

/// Comma-joined lower-case names of the active analysis indicators (score/display/LLM).
fn active_extension_names(config: &Config) -> String {
    join_extension_names(config.analysis_extensions())
}

/// Comma-joined names of the env-enabled (computed/stored) indicators.
fn enabled_extension_names(config: &Config) -> String {
    join_extension_names(&config.enabled_extensions)
}

fn join_extension_names(exts: &[crate::config::ExtensionIndicator]) -> String {
    if exts.is_empty() {
        return "(none)".to_string();
    }
    exts.iter()
        .map(|e| format!("{:?}", e).to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Apply one `/set <key> <value>` to the session config. Returns a "key = value"
/// confirmation, or an error string. Thresholds/periods and indicator weights
/// (`weight-*`) are settable here; the stored log/CSV column set is unaffected.
pub(super) fn set_analysis_param(
    config: &mut Config,
    key: &str,
    val: &str,
) -> Result<String, String> {
    let f = |v: &str| -> Result<f64, String> {
        v.parse::<f64>()
            .ok()
            .filter(|x| x.is_finite())
            .ok_or_else(|| format!("not a number: {v}"))
    };
    let u = |v: &str| -> Result<usize, String> {
        match v.parse::<usize>() {
            Ok(0) | Err(_) => Err(format!("must be an integer >= 1: {v}")),
            Ok(n) => Ok(n),
        }
    };
    let b = |v: &str| -> Result<bool, String> {
        match v.to_ascii_lowercase().as_str() {
            "on" | "true" | "1" | "yes" | "y" => Ok(true),
            "off" | "false" | "0" | "no" | "n" => Ok(false),
            _ => Err(format!("use on/off: {v}")),
        }
    };
    // `/set indicator <name> <on|off>` — narrow the ACTIVE analysis set (score +
    // display + LLM). Computation and the log/CSV columns are unaffected (still the env
    // baseline), so a deactivated indicator is still computed and stored.
    if key == "indicator" {
        return set_active_indicator(config, val);
    }
    // Indicator weights (`/set weight-<indicator> <n>`). Runtime-settable; they scale
    // each indicator's contribution to the composite score. Changing a weight makes a
    // subsequently-stored final_score reflect the new weighting (a comparability
    // caveat, accepted by design). Negative weights are rejected.
    if let Some(ind) = key.strip_prefix("weight-") {
        let w = f(val)?;
        if w < 0.0 {
            return Err(format!("weight must be >= 0: {val}"));
        }
        let slot = match ind {
            "basic" => &mut config.weight_basic,
            "ema" => &mut config.weight_ema,
            "sma" => &mut config.weight_sma,
            "bollinger" => &mut config.weight_bollinger,
            "roc" => &mut config.weight_roc,
            "adx" => &mut config.weight_adx,
            "stochastics" => &mut config.weight_stochastics,
            "fibonacci" => &mut config.weight_fibonacci,
            "vwap" => &mut config.weight_vwap,
            "ichimoku" => &mut config.weight_ichimoku,
            _ => return Err(format!("unknown weight: {ind}")),
        };
        *slot = w;
        return Ok(format!("weight-{ind} = {w}"));
    }
    match key {
        // Interpretation stance (buyer/seller/holder). Affects only the score-gauge
        // presentation and the LLM's reading lean — NOT the composite score itself, so
        // no stored-column comparability caveat. Config-level (like the others), hence in `/set`.
        "stance" => {
            config.stance = match val.trim().to_ascii_lowercase().as_str() {
                "buyer" | "buy" => crate::config::Stance::Buyer,
                "seller" | "sell" => crate::config::Stance::Seller,
                "holder" | "hold" | "neutral" => crate::config::Stance::Holder,
                _ => return Err(format!("use buyer/seller/holder: {val}")),
            };
            Ok(format!("stance = {}", config.stance))
        }
        "macd-minus-ok" => {
            config.macd_minus_ok = b(val)?;
            Ok(format!("macd-minus-ok = {}", config.macd_minus_ok))
        }
        "buy-rsi" => {
            config.buy_rsi = f(val)?;
            Ok(format!("buy-rsi = {}", config.buy_rsi))
        }
        "sell-rsi" => {
            config.sell_rsi = f(val)?;
            Ok(format!("sell-rsi = {}", config.sell_rsi))
        }
        "macd-diff-low" => {
            config.macd_diff_low = f(val)?;
            Ok(format!("macd-diff-low = {}", config.macd_diff_low))
        }
        "macd-diff-mid" => {
            config.macd_diff_mid = f(val)?;
            Ok(format!("macd-diff-mid = {}", config.macd_diff_mid))
        }
        "macd-diff-extreme" => {
            config.macd_diff_extreme = f(val)?;
            Ok(format!("macd-diff-extreme = {}", config.macd_diff_extreme))
        }
        "bb-period" => {
            config.bollinger_period = u(val)?;
            Ok(format!("bb-period = {}", config.bollinger_period))
        }
        "bb-sigma" => {
            config.bollinger_stddev_multiplier = f(val)?;
            Ok(format!("bb-sigma = {}", config.bollinger_stddev_multiplier))
        }
        "bb-squeeze" => {
            config.bb_bandwidth_squeeze_pct = f(val)?;
            Ok(format!("bb-squeeze = {}", config.bb_bandwidth_squeeze_pct))
        }
        "adx-period" => {
            config.adx_period = u(val)?;
            Ok(format!("adx-period = {}", config.adx_period))
        }
        "roc-period" => {
            config.roc_period = u(val)?;
            Ok(format!("roc-period = {}", config.roc_period))
        }
        "stoch-period" => {
            config.stochastics_period = u(val)?;
            Ok(format!("stoch-period = {}", config.stochastics_period))
        }
        "vwap-period" => {
            config.vwap_period = u(val)?;
            Ok(format!("vwap-period = {}", config.vwap_period))
        }
        "fib-ratio" => {
            // Bounded here too, so a session override cannot do what the CLI and env
            // paths are prevented from doing (leave the ±1 bands unreachable).
            config.fibonacci_neutral_ratio = crate::config::clamp_fibonacci_neutral_ratio(f(val)?);
            Ok(format!("fib-ratio = {}", config.fibonacci_neutral_ratio))
        }
        _ => Err(format!("unknown key: {key}")),
    }
}

/// Parse a user-facing indicator name (`ema`, `bollinger`, `stochastics`, …) to its
/// `ExtensionIndicator`. Accepts a few common aliases.
fn parse_extension_name(name: &str) -> Option<crate::config::ExtensionIndicator> {
    use crate::config::ExtensionIndicator as E;
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "ema" => E::Ema,
        "sma" => E::Sma,
        "bollinger" | "bb" | "bollingerbands" => E::Bollinger,
        "roc" => E::Roc,
        "adx" => E::Adx,
        "stochastics" | "stoch" | "stochastic" => E::Stochastics,
        "fibonacci" | "fibo" | "fib" => E::Fibonacci,
        "vwap" => E::Vwap,
        "ichimoku" => E::Ichimoku,
        _ => return None,
    })
}

/// `/set indicator <name> <on|off>` — toggle whether an indicator is ACTIVE for the
/// session's analysis (score/display/LLM). Only indicators in the env baseline
/// (`enabled_extensions`, i.e. what is computed) can be toggled — you cannot activate
/// an indicator that is not computed. Stores `None` when the active set equals the
/// full baseline, so "everything on" is the natural default state.
fn set_active_indicator(config: &mut Config, val: &str) -> Result<String, String> {
    let mut it = val.split_whitespace();
    let name = it
        .next()
        .ok_or_else(|| "usage: /set indicator <name> <on|off>".to_string())?;
    let state = it
        .next()
        .ok_or_else(|| "usage: /set indicator <name> <on|off>".to_string())?;
    let target = parse_extension_name(name).ok_or_else(|| format!("unknown indicator: {name}"))?;
    if !config.enabled_extensions.contains(&target) {
        return Err(format!(
            "{name} is not enabled in env — only computed (env-enabled) indicators can be toggled"
        ));
    }
    let on = match state.to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" | "y" => true,
        "off" | "false" | "0" | "no" | "n" => false,
        _ => return Err(format!("use on/off: {state}")),
    };
    let mut desired: Vec<crate::config::ExtensionIndicator> = config.analysis_extensions().to_vec();
    if on {
        if !desired.contains(&target) {
            desired.push(target.clone());
        }
    } else {
        desired.retain(|e| e != &target);
    }
    // Rebuild in the env baseline's order (stable display); None == full baseline.
    let ordered: Vec<crate::config::ExtensionIndicator> = config
        .enabled_extensions
        .iter()
        .filter(|e| desired.contains(e))
        .cloned()
        .collect();
    config.active_extensions = if ordered.len() == config.enabled_extensions.len() {
        None
    } else {
        Some(ordered)
    };
    Ok(format!(
        "indicator {name} = {}",
        if on { "on" } else { "off" }
    ))
}

/// Reset the `/set` group back to the env/default baseline (rebuild config from
/// env and copy just those fields — leaves mode/llm/extensions intact).
fn reset_analysis_params(config: &mut Config) {
    use clap::Parser as _;
    let env_map = crate::bootstrap::load_env_map();
    let args = crate::config::Args::parse_from(["xoksa", "-t", config.ticker.as_str()]);
    let fresh = crate::config::build_config(&args, &env_map);
    config.macd_minus_ok = fresh.macd_minus_ok;
    config.buy_rsi = fresh.buy_rsi;
    config.sell_rsi = fresh.sell_rsi;
    config.macd_diff_low = fresh.macd_diff_low;
    config.macd_diff_mid = fresh.macd_diff_mid;
    config.macd_diff_extreme = fresh.macd_diff_extreme;
    config.bollinger_period = fresh.bollinger_period;
    config.bollinger_stddev_multiplier = fresh.bollinger_stddev_multiplier;
    config.bb_bandwidth_squeeze_pct = fresh.bb_bandwidth_squeeze_pct;
    config.adx_period = fresh.adx_period;
    config.roc_period = fresh.roc_period;
    config.stochastics_period = fresh.stochastics_period;
    config.vwap_period = fresh.vwap_period;
    config.fibonacci_neutral_ratio = fresh.fibonacci_neutral_ratio;
    config.weight_basic = fresh.weight_basic;
    config.weight_ema = fresh.weight_ema;
    config.weight_sma = fresh.weight_sma;
    config.weight_bollinger = fresh.weight_bollinger;
    config.weight_roc = fresh.weight_roc;
    config.weight_adx = fresh.weight_adx;
    config.weight_stochastics = fresh.weight_stochastics;
    config.weight_fibonacci = fresh.weight_fibonacci;
    config.weight_vwap = fresh.weight_vwap;
    config.weight_ichimoku = fresh.weight_ichimoku;
    config.stance = fresh.stance;
    // Active indicator set back to "all enabled".
    config.active_extensions = None;
}

/// Handle `/llm` (show current provider/model, or switch). Mutates `config`
/// only; shared by CLI and Web so a switch behaves identically. The provider's
/// default model is resolved from the same `config` env (SOT).
fn handle_llm_switch(
    cmd: super::llm::LlmSwitchCmd<'_>,
    config: &mut Config,
    out: &mut ChatOut,
    lang: &str,
) {
    use super::llm::LlmSwitchCmd;
    use super::llm::{live_ollama_instances, render_llm_list, resolve_llm_default_model};
    match cmd {
        // `/llm` (list) and `/llm net` are handled in execute() — List renders
        // locally here; Net is async and dispatched there.
        LlmSwitchCmd::List => render_llm_list(config, out, lang),
        LlmSwitchCmd::Net => {}
        LlmSwitchCmd::UnknownProvider(p) => out.err(match lang {
            "ja" => format!(
                "❌ 不明なプロバイダー: {}。有効な値: openai, gemini, claude, ollama",
                p
            ),
            _ => format!(
                "❌ Unknown provider: {}. Valid: openai, gemini, claude, ollama",
                p
            ),
        }),
        LlmSwitchCmd::Switch {
            provider,
            explicit_model,
        } => {
            let ollama_instances = if provider == "ollama" {
                live_ollama_instances()
            } else {
                Vec::new()
            };
            let ollama_alias = if provider == "ollama" && !explicit_model.is_empty() {
                ollama_instances
                    .iter()
                    .find(|i| i.alias == explicit_model)
                    .cloned()
            } else {
                None
            };
            if let Some(inst) = ollama_alias {
                config.llm_provider = "ollama".to_string();
                config.ollama_host = inst.host.clone();
                config.ollama_port = inst.port;
                config.llm_model = inst.model.clone();
                config.ollama_alias = explicit_model.to_string();
                out.line(match lang {
                    "ja" => format!(
                        "✅ Ollamaインスタンス「{}」に切り替えました（モデル: {} / {}:{}）。",
                        explicit_model, inst.model, inst.host, inst.port
                    ),
                    _ => format!(
                        "✅ Switched to Ollama instance \"{}\" (model: {} / {}:{}).",
                        explicit_model, inst.model, inst.host, inst.port
                    ),
                });
            } else if provider == "ollama" && !explicit_model.is_empty() {
                if ollama_instances.is_empty() {
                    out.err(match lang {
                        "ja" => "❌ Ollamaインスタンスが設定されていません。xoksa.env で OLLAMA_N_ALIAS / OLLAMA_N_HOST / OLLAMA_N_PORT / OLLAMA_N_MODEL を設定してください。",
                        _ => "❌ No Ollama instances configured. Set OLLAMA_N_ALIAS / OLLAMA_N_HOST / OLLAMA_N_PORT / OLLAMA_N_MODEL in xoksa.env.",
                    });
                } else {
                    let available: String = ollama_instances
                        .iter()
                        .map(|i| i.alias.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.err(match lang {
                        "ja" => format!(
                            "❌ ollama エイリアス「{}」が見つかりません。利用可能: {}",
                            explicit_model, available
                        ),
                        _ => format!(
                            "❌ ollama alias \"{}\" not found. Available: {}",
                            explicit_model, available
                        ),
                    });
                }
            } else if provider == "ollama" {
                out.err(match lang {
                    "ja" => "❌ ollama はエイリアスの指定が必須です。例: /llm ollama:gpu1",
                    _ => "❌ ollama requires an alias. Example: /llm ollama:gpu1",
                });
            } else {
                let resolved_model = if !explicit_model.is_empty() {
                    explicit_model.to_string()
                } else {
                    resolve_llm_default_model(provider, config)
                };
                config.llm_provider = provider.to_string();
                config.llm_model = resolved_model.clone();
                config.ollama_alias = String::new();
                out.line(match lang {
                    "ja" => format!(
                        "✅ LLMを{}（モデル: {}）に切り替えました。",
                        provider, resolved_model
                    ),
                    _ => format!(
                        "✅ Switched to LLM: {} / model: {}",
                        provider, resolved_model
                    ),
                });
            }
        }
    }
}

impl ChatSession {
    pub(crate) async fn execute(
        &mut self,
        raw_input: &str,
        ctx: &mut ExecCtx<'_>,
        out: &mut ChatOut,
    ) -> Dispatch {
        let lang = ctx.lang;

        if raw_input == "/help" {
            super::help::write_help(out, lang);
            return Dispatch::Handled;
        }

        if raw_input == "/date" || raw_input.starts_with("/date ") {
            use chrono::Datelike;
            let now_local = chrono::Local::now();
            let now_utc = chrono::Utc::now();
            let local_str = now_local.format("%Y-%m-%d %H:%M:%S %:z").to_string();
            let utc_str = now_utc.format("%Y-%m-%d %H:%M:%S").to_string();
            let wd = now_local.weekday().num_days_from_monday() as usize;
            if lang == "ja" {
                let wd_ja = ["月", "火", "水", "木", "金", "土", "日"][wd];
                out.line(format!("📅 現在日時: {} ({})", local_str, wd_ja));
                out.line(format!("🌐 UTC: {} UTC", utc_str));
            } else {
                let wd_en = now_local.format("%a").to_string();
                out.line(format!("📅 Now: {} ({})", local_str, wd_en));
                out.line(format!("🌐 UTC: {} UTC", utc_str));
            }
            return Dispatch::Handled;
        }

        if raw_input == "/bye" {
            out.line(if lang == "ja" {
                "チャット終了。"
            } else {
                "Chat ended."
            });
            return Dispatch::Exit;
        }

        if raw_input == "/status" || raw_input.starts_with("/status ") {
            let arg = raw_input.strip_prefix("/status ").unwrap_or("").trim();
            if arg.is_empty() {
                write_chat_status(
                    out,
                    self,
                    ctx.config,
                    ctx.cumulative_tokens,
                    *ctx.autoreload_enabled,
                    ctx.next_reload_at.as_ref(),
                    lang,
                );
            } else {
                out.err(if lang == "ja" {
                    "❌ /status に引数はありません。"
                } else {
                    "❌ /status accepts no arguments."
                });
            }
            return Dispatch::Handled;
        }

        if raw_input == "/alert" || raw_input.starts_with("/alert ") {
            let arg = raw_input.strip_prefix("/alert").unwrap_or("").trim();
            handle_alert(arg, lang, ctx.interactive, out).await;
            return Dispatch::Handled;
        }

        if raw_input == "/tech" || raw_input.starts_with("/tech ") {
            if self.ticker_labels().len() == 1 && self.ticker_labels()[0].is_empty() {
                out.err(if lang == "ja" {
                    "❌ 銘柄がロードされていません。先に /sym add <銘柄> を実行してください。"
                } else {
                    "❌ No ticker loaded. Use /sym add <symbol> first."
                });
                return Dispatch::Handled;
            }
            let arg = raw_input.strip_prefix("/tech ").unwrap_or("").trim();
            let parts: Vec<&str> = arg.split_whitespace().collect();
            let n = self.technical_texts().len();
            // /tech [timeframe] [index]
            let p1 = parts.first().copied().unwrap_or("");
            let (target_mode, idx_str) =
                if let Some(mode) = crate::config::AnalysisMode::from_value(p1) {
                    (mode, parts.get(1).copied().unwrap_or(""))
                } else {
                    (ctx.config.analysis_mode, p1)
                };
            let indices: Vec<usize> = if idx_str.is_empty() {
                if n == 1 {
                    vec![0]
                } else {
                    out.err(if lang == "ja" {
                        format!(
                            "❌ 銘柄番号を指定してください: /tech {} [1-{}]",
                            target_mode, n
                        )
                    } else {
                        format!("❌ Specify ticker number: /tech {} [1-{}]", target_mode, n)
                    });
                    return Dispatch::Handled;
                }
            } else {
                match parse_show_indices(idx_str, n) {
                    Ok(v) => v,
                    Err(_) => {
                        out.err(if lang == "ja" {
                            format!("❌ 番号指定が無効です（有効: 1〜{}、例: 1,3 / 1-{}）", n, n)
                        } else {
                            format!("❌ Invalid index (valid: 1–{}, e.g. 1,3 / 1-{})", n, n)
                        });
                        return Dispatch::Handled;
                    }
                }
            };
            for i in indices {
                if target_mode == ctx.config.analysis_mode {
                    self.show_technical(i, out);
                } else {
                    let label = &self.ticker_labels()[i];
                    let symbol = label
                        .split(" (")
                        .next()
                        .unwrap_or(label.as_str())
                        .to_string();
                    let display = short_display_label(label).to_string();
                    let mut tmp_config = ctx.config.clone();
                    tmp_config.analysis_mode = target_mode;
                    out.line(if lang == "ja" {
                        format!("⏳ {} の {} データを取得中...", display, target_mode)
                    } else {
                        format!("⏳ Fetching {} data for {}...", target_mode, display)
                    });
                    match super::ticker::load_ticker_analysis(
                        &tmp_config,
                        &symbol,
                        ctx.ticker_name_map,
                        false,
                        false,
                    )
                    .await
                    {
                        Ok(c) => {
                            for line in c.technical_text.lines() {
                                out.line(line);
                            }
                        }
                        Err(e) => out.err(format!("❌ {}", e)),
                    }
                }
            }
            return Dispatch::Handled;
        }

        if raw_input == "/funda" || raw_input.starts_with("/funda ") {
            if self.ticker_labels().len() == 1 && self.ticker_labels()[0].is_empty() {
                out.err(if lang == "ja" {
                    "❌ 銘柄がロードされていません。先に /sym add <銘柄> を実行してください。"
                } else {
                    "❌ No ticker loaded. Use /sym add <symbol> first."
                });
                return Dispatch::Handled;
            }
            let arg = raw_input.strip_prefix("/funda ").unwrap_or("").trim();
            let parts: Vec<&str> = arg.split_whitespace().collect();
            let n = self.technical_texts().len();
            let num_str = parts.first().copied().unwrap_or("");
            let indices: Vec<usize> = if num_str.is_empty() {
                if n == 1 {
                    vec![0]
                } else {
                    out.err(if lang == "ja" {
                        format!("❌ 銘柄番号を指定してください: /funda [1-{}]", n)
                    } else {
                        format!("❌ Specify ticker number: /funda [1-{}]", n)
                    });
                    return Dispatch::Handled;
                }
            } else {
                match parse_show_indices(num_str, n) {
                    Ok(v) => v,
                    Err(_) => {
                        out.err(if lang == "ja" {
                            format!("❌ 番号指定が無効です（有効: 1〜{}、例: 1,3 / 1-{}）", n, n)
                        } else {
                            format!("❌ Invalid index (valid: 1–{}, e.g. 1,3 / 1-{})", n, n)
                        });
                        return Dispatch::Handled;
                    }
                }
            };
            for i in indices {
                self.show_fundamental(i, out, lang);
            }
            return Dispatch::Handled;
        }

        if raw_input == "/news" || raw_input.starts_with("/news ") {
            let arg = raw_input.strip_prefix("/news ").unwrap_or("").trim();
            let parts: Vec<&str> = arg.split_whitespace().collect();
            let first = parts.first().copied().unwrap_or("");
            // Buffer ops (find/list/use/del/clear) — delegate to the extra-news handler.
            if matches!(first, "find" | "list" | "use" | "del" | "clear") {
                super::news_extra::handle_news_extra_command(
                    raw_input, self, ctx.config, lang, out,
                )
                .await;
                return Dispatch::Handled;
            }
            // News display (default) and news-filtered triage (nf) need a loaded ticker.
            if self.ticker_labels().len() == 1 && self.ticker_labels()[0].is_empty() {
                out.err(if lang == "ja" {
                    "❌ 銘柄がロードされていません。先に /sym add <銘柄> を実行してください。"
                } else {
                    "❌ No ticker loaded. Use /sym add <symbol> first."
                });
                return Dispatch::Handled;
            }
            let n = self.technical_texts().len();
            let is_nf = first == "nf";
            let num_str = if is_nf {
                parts.get(1).copied().unwrap_or("")
            } else {
                first
            };
            let indices: Vec<usize> = if num_str.is_empty() {
                if n == 1 {
                    vec![0]
                } else {
                    let hint = if is_nf {
                        format!("/news nf [1-{}]", n)
                    } else {
                        format!("/news [1-{}]", n)
                    };
                    out.err(if lang == "ja" {
                        format!("❌ 銘柄番号を指定してください: {}", hint)
                    } else {
                        format!("❌ Specify ticker number: {}", hint)
                    });
                    return Dispatch::Handled;
                }
            } else {
                match parse_show_indices(num_str, n) {
                    Ok(v) => v,
                    Err(_) => {
                        out.err(if lang == "ja" {
                            format!("❌ 番号指定が無効です（有効: 1〜{}、例: 1,3 / 1-{}）", n, n)
                        } else {
                            format!("❌ Invalid index (valid: 1–{}, e.g. 1,3 / 1-{})", n, n)
                        });
                        return Dispatch::Handled;
                    }
                }
            };
            for i in indices {
                if is_nf {
                    super::news_extra::show_news_filtered_llm(
                        &self.news_items()[i],
                        &self.ticker_labels()[i],
                        ctx.config,
                        lang,
                        out,
                    )
                    .await
                } else {
                    self.show_news(i, out, lang);
                }
            }
            return Dispatch::Handled;
        }

        if raw_input == "/mem" || raw_input.starts_with("/mem ") {
            let arg = raw_input.strip_prefix("/mem ").unwrap_or("").trim();
            if !["low", "mid", "high"].contains(&arg) {
                out.err(if lang == "ja" {
                    "❌ /mem には low / mid / high を指定してください。"
                } else {
                    "❌ /mem requires low, mid, or high."
                });
            } else {
                self.params = parse_memory(arg);
                out.line(if lang == "ja" {
                    format!("✅ チャットメモリを {} に変更しました。", arg)
                } else {
                    format!("✅ Chat memory set to {}.", arg)
                });
            }
            return Dispatch::Handled;
        }

        if raw_input == "/forum" || raw_input.starts_with("/forum ") {
            super::council::handle_forum_command(
                raw_input,
                self,
                ctx.config,
                ctx.cumulative_tokens,
                lang,
                ctx.interactive,
                out,
            )
            .await;
            return Dispatch::Handled;
        }

        // Cross-LLM verification (split out of /forum).
        if raw_input == "/crit" {
            super::council::handle_crit(
                self,
                ctx.config,
                ctx.cumulative_tokens,
                lang,
                ctx.interactive,
                out,
            )
            .await;
            return Dispatch::Handled;
        }
        if raw_input == "/keep" || raw_input.starts_with("/keep ") {
            let arg = raw_input.strip_prefix("/keep ").unwrap_or("").trim();
            super::council::handle_keep(self, ctx.config, arg, lang, out);
            return Dispatch::Handled;
        }

        // Response-style knobs — flat top-level commands (was `/tune <sub>`).
        // `/status` shows the current values. Two interpretation axes — `depth`
        // (how far it reads INTO the given data) and `scope` (how far it reaches
        // OUTSIDE the input for general knowledge) — plus format (`shape`) and
        // forecast (`cast`). Each command with no value prints the current one.
        if raw_input == "/depth" || raw_input.starts_with("/depth ") {
            let val = raw_input.strip_prefix("/depth ").unwrap_or("").trim();
            set_response_style(
                out,
                lang,
                "depth",
                &mut self.read_depth,
                &["shallow", "mid", "deep"],
                "shallow, mid, deep",
                val,
            );
            return Dispatch::Handled;
        }
        if raw_input == "/scope" || raw_input.starts_with("/scope ") {
            let val = raw_input.strip_prefix("/scope ").unwrap_or("").trim();
            set_response_style(
                out,
                lang,
                "scope",
                &mut self.knowledge_scope,
                &["narrow", "mid", "wide"],
                "narrow, mid, wide",
                val,
            );
            return Dispatch::Handled;
        }
        if raw_input == "/shape" || raw_input.starts_with("/shape ") {
            let val = raw_input.strip_prefix("/shape ").unwrap_or("").trim();
            set_response_style(
                out,
                lang,
                "shape",
                &mut self.response_shape,
                &["talk", "points", "scenario"],
                "talk, points, scenario",
                val,
            );
            return Dispatch::Handled;
        }
        if raw_input == "/cast" || raw_input.starts_with("/cast ") {
            let val = raw_input.strip_prefix("/cast ").unwrap_or("").trim();
            set_response_style(
                out,
                lang,
                "cast",
                &mut self.forecast_mode,
                &["off", "soft", "bold"],
                "off, soft, bold",
                val,
            );
            return Dispatch::Handled;
        }

        // `/set` — runtime analysis parameters (session only; env is NOT rewritten).
        // Thresholds/periods and indicator weights (`weight-*`) are settable; the stored
        // log/CSV column set is never affected. Shared CLI/Web/serve.
        if raw_input == "/set" || raw_input.starts_with("/set ") {
            let arg = raw_input.strip_prefix("/set ").unwrap_or("").trim();
            let is_empty_session =
                self.ticker_labels().len() == 1 && self.ticker_labels()[0].is_empty();
            if arg.is_empty() {
                out.line(if lang == "ja" {
                    "現在の解析パラメータ（session限定・envは変更しません）:"
                } else {
                    "Current analysis params (session only; env unchanged):"
                });
                for l in analysis_param_lines(ctx.config) {
                    out.line(l);
                }
                out.line(if lang == "ja" {
                    "  例: /set stance buyer  /  /set weight-ema 1.5  /  /set indicator vwap off  /  /set buy-rsi 25  /  /set reset"
                } else {
                    "  e.g. /set stance buyer  /  /set weight-ema 1.5  /  /set indicator vwap off  /  /set buy-rsi 25  /  /set reset"
                });
                out.line(if lang == "ja" {
                    "  ※indicator off は解析(スコア/表示/LLM)から外すだけ。計算とログ列は不変。重み変更後の final_score は新しい重みで算出（比較の際は注意）"
                } else {
                    "  note: `indicator off` only drops it from analysis (score/display/LLM); computation & log columns are unchanged. After a weight change, final_score reflects the new weights."
                });
                return Dispatch::Handled;
            }
            if arg == "reset" {
                reset_analysis_params(ctx.config);
                out.line(if lang == "ja" {
                    "✅ 解析パラメータを env 既定に戻しました。"
                } else {
                    "✅ Analysis params reset to env defaults."
                });
                if !is_empty_session {
                    *ctx.autoreload_last_prices = super::ticker::reload_chat_tickers(
                        self,
                        ctx.config,
                        ctx.ticker_name_map,
                        false,
                        true,
                        lang,
                        out,
                        false,
                    )
                    .await;
                }
                return Dispatch::Handled;
            }
            let (key, val) = arg
                .split_once(' ')
                .map(|(k, v)| (k, v.trim()))
                .unwrap_or((arg, ""));
            if val.is_empty() {
                out.err(if lang == "ja" {
                    format!("❌ 値がありません: /set {key} <値>（項目一覧は /set）")
                } else {
                    format!("❌ Missing value: /set {key} <value> (list with /set)")
                });
                return Dispatch::Handled;
            }
            match set_analysis_param(ctx.config, key, val) {
                Ok(msg) => {
                    out.line(format!("✅ {msg}"));
                    if !is_empty_session {
                        *ctx.autoreload_last_prices = super::ticker::reload_chat_tickers(
                            self,
                            ctx.config,
                            ctx.ticker_name_map,
                            false,
                            true,
                            lang,
                            out,
                            false,
                        )
                        .await;
                    }
                }
                Err(e) => out.err(if lang == "ja" {
                    format!("❌ {e}（項目一覧は /set）")
                } else {
                    format!("❌ {e} (list keys with /set)")
                }),
            }
            return Dispatch::Handled;
        }

        if let Some(cmd) = super::llm::parse_llm_switch_command(raw_input) {
            // `/llm net` is async (queries provider APIs); the rest is synchronous.
            if matches!(cmd, super::llm::LlmSwitchCmd::Net) {
                super::llm::query_models_net(ctx.config, out, lang).await;
            } else {
                handle_llm_switch(cmd, ctx.config, out, lang);
            }
            return Dispatch::Handled;
        }

        if raw_input == "/token" || raw_input.starts_with("/token ") {
            let arg = raw_input.strip_prefix("/token ").unwrap_or("").trim();
            match arg {
                "" => super::report::write_token_summary(
                    out,
                    self,
                    ctx.cumulative_tokens,
                    lang,
                    false,
                ),
                "detail" | "d" => {
                    super::report::write_token_summary(out, self, ctx.cumulative_tokens, lang, true)
                }
                _ => out.err(if lang == "ja" {
                    "❌ /token には detail のみ指定できます。"
                } else {
                    "❌ /token accepts only detail."
                }),
            }
            return Dispatch::Handled;
        }

        // `/basic` (基本分析) is the canonical command; `/run` is a deprecated alias.
        if raw_input == "/basic" || raw_input == "/run" {
            if raw_input == "/run" {
                out.line(if lang == "ja" {
                    "ℹ️ `/run` は `/basic`（基本分析）に変わりました。当面は使えます。"
                } else {
                    "ℹ️ `/run` was renamed to `/basic` (basic analysis); it still works for now."
                });
            }
            if self.ticker_labels().len() == 1 && self.ticker_labels()[0].is_empty() {
                out.err(if lang == "ja" {
                    "❌ 銘柄がロードされていません。先に /sym add <銘柄> を実行してください。"
                } else {
                    "❌ No ticker loaded. Use /sym add <symbol> first."
                });
                return Dispatch::Handled;
            }
            let initial_prompt = self.build_initial_analysis_prompt();
            // The basic analysis is an LLM turn like any other: it reasons over
            // the loaded instruments confirmed values, so the output guard gets
            // them too (G2). Without this it fell back to the presence test.
            let facts = self.confirmed_facts();
            let Some(result) = await_or_cancel(
                crate::llm::send_chat_turn_with_usage(ctx.config, &initial_prompt, Some(&facts)),
                ctx.interactive,
                lang,
                out,
            )
            .await
            else {
                return Dispatch::Handled;
            };
            match result {
                Ok((response, usage)) => {
                    // Attribute the commentary to the model that produced it — the
                    // shared "🧠 <provider/model> が解説:" badge every LLM answer shows,
                    // so `/basic` is consistent (CLI + Web via one dispatch).
                    // Name every analysed ticker in the badge, not just the active
                    // one, so a multi-ticker comparison reads e.g.
                    // "… が 9432.T・9433.T を解説:" instead of a single symbol.
                    let badge = {
                        let labels = self.ticker_labels();
                        let syms: Vec<&str> = labels
                            .iter()
                            .map(|l| l.split_once(" (").map_or(l.as_str(), |(t, _)| t))
                            .filter(|t| !t.is_empty())
                            .collect();
                        if syms.len() > 1 {
                            let mut cfg = ctx.config.clone();
                            cfg.ticker = syms.join("・");
                            super::llm::llm_commentary_badge(&cfg, lang)
                        } else {
                            super::llm::llm_commentary_badge(ctx.config, lang)
                        }
                    };
                    out.line(badge);
                    out.line(format!("\n{}\n", response));
                    ctx.cumulative_tokens.accumulate(usage);
                    let model = super::llm::active_llm_model(ctx.config);
                    self.add_debate_entry(
                        &ctx.config.llm_provider,
                        &model,
                        &ctx.config.ollama_alias,
                        "/basic",
                        &response,
                    );
                    self.add_turn("/basic".to_string(), response);
                }
                Err(e) => out.err(format!("❌ {}", e)),
            }
            return Dispatch::Handled;
        }

        if raw_input == "/clear" {
            // Fresh start: wipe the chat history + debate buffer. Confirmed
            // indicator data is separate and kept. The `[[XOKSA_CLEAR]]` control
            // line tells the Web UI to wipe the visible log; the chat panel's 🗑
            // button sends this same command, so CLI and Web behave identically.
            self.recent_turns.clear();
            self.debate_entries.clear();
            out.line("[[XOKSA_CLEAR]]".to_string());
            out.line(
                match lang {
                    "ja" => "🧹 チャット履歴と検討バッファを消去しました（確定した指標データは保持）。",
                    _ => "🧹 Cleared the chat history and debate buffer (confirmed indicator data kept).",
                }
                .to_string(),
            );
            return Dispatch::Handled;
        }

        if raw_input == "/mode" || raw_input.starts_with("/mode ") {
            let arg = raw_input.strip_prefix("/mode ").unwrap_or("").trim();
            if arg.is_empty() {
                let mode_label = ctx.config.analysis_mode.label(&ctx.config.lang);
                let interval = ctx.config.analysis_mode.data_interval();
                out.line(match lang {
                    "ja" => format!("現在の足: {} ({})", mode_label, interval),
                    _ => format!("Current bar mode: {} ({})", mode_label, interval),
                });
                return Dispatch::Handled;
            }
            let Some(new_mode) = crate::config::AnalysisMode::from_value(arg) else {
                out.err(match lang {
                    "ja" => format!(
                        "❌ 不明な足です: {}。有効な値: daily, 60m, 30m, 15m, 5m, 1m, weekly, monthly",
                        arg
                    ),
                    _ => format!(
                        "❌ Unknown bar mode: {}. Valid: daily, 60m, 30m, 15m, 5m, 1m, weekly, monthly",
                        arg
                    ),
                });
                return Dispatch::Handled;
            };
            ctx.config.analysis_mode = new_mode;
            let bar_label = ctx.config.analysis_mode.bar_label(&ctx.config.lang);
            let disables_autoreload = *ctx.autoreload_enabled && !new_mode.is_intraday();
            let is_empty_session =
                self.ticker_labels().len() == 1 && self.ticker_labels()[0].is_empty();
            if disables_autoreload {
                *ctx.autoreload_enabled = false;
                *ctx.next_reload_at = None;
            }
            out.line(match (lang, disables_autoreload) {
                ("ja", true) => format!(
                    "✅ 分析足を {} に変更しました（自動リロードを無効化: 分足モードのみ対応）。",
                    bar_label
                ),
                ("ja", false) => format!("✅ 分析足を {} に変更しました。", bar_label),
                (_, true) => format!(
                    "✅ Bar mode set to {} (auto-reload disabled: intraday modes only).",
                    bar_label
                ),
                (_, false) => format!("✅ Bar mode set to {}.", bar_label),
            });
            if !disables_autoreload && *ctx.autoreload_enabled {
                *ctx.next_reload_at = super::ticker::next_autoreload_deadline(new_mode);
                let interval_label = new_mode.data_interval();
                out.line(match lang {
                    "ja" => format!(
                        "  自動リロード間隔を {} ごとに更新しました。",
                        interval_label
                    ),
                    _ => format!(
                        "  Auto-reload interval updated to every {}.",
                        interval_label
                    ),
                });
            }
            if !is_empty_session {
                *ctx.autoreload_last_prices = super::ticker::reload_chat_tickers(
                    self,
                    ctx.config,
                    ctx.ticker_name_map,
                    false,
                    true,
                    lang,
                    out,
                    false,
                )
                .await;
                if *ctx.autoreload_enabled {
                    *ctx.next_reload_at =
                        super::ticker::next_autoreload_deadline(ctx.config.analysis_mode);
                }
            }
            return Dispatch::Handled;
        }

        if raw_input == "/reload t"
            || raw_input == "/reload n"
            || raw_input == "/reload"
            || raw_input.starts_with("/reload ")
        {
            let reload_news = raw_input == "/reload n";
            let valid = raw_input == "/reload t" || raw_input == "/reload n";
            if !valid {
                out.err(match lang {
                    "ja" => "❌ /reload t または /reload n を指定してください。",
                    _ => "❌ Specify /reload t or /reload n.",
                });
                return Dispatch::Handled;
            }
            if self.ticker_labels().len() == 1 && self.ticker_labels()[0].is_empty() {
                out.err(match lang {
                    "ja" => {
                        "❌ 銘柄がロードされていません。先に /sym add <銘柄> を実行してください。"
                    }
                    _ => "❌ No tickers loaded. Use /sym add <symbol> first.",
                });
                return Dispatch::Handled;
            }
            *ctx.autoreload_last_prices = super::ticker::reload_chat_tickers(
                self,
                ctx.config,
                ctx.ticker_name_map,
                reload_news,
                true,
                lang,
                out,
                false,
            )
            .await;
            if *ctx.autoreload_enabled {
                *ctx.next_reload_at =
                    super::ticker::next_autoreload_deadline(ctx.config.analysis_mode);
            }
            return Dispatch::Handled;
        }

        if raw_input == "/auto" || raw_input.starts_with("/auto ") {
            let arg = raw_input.strip_prefix("/auto ").unwrap_or("").trim();
            let is_notice = arg == "notice";
            let turn_on = match arg {
                "on" | "notice" => Some(true),
                "off" => Some(false),
                "" => Some(!*ctx.autoreload_enabled),
                _ => None,
            };
            match turn_on {
                None => out.err(match lang {
                    "ja" => {
                        "❌ /auto には on / off / notice またはなし（トグル）を指定してください。"
                    }
                    _ => "❌ /auto accepts: on, off, notice, or no argument (toggle).",
                }),
                Some(true) => {
                    let is_empty_session =
                        self.ticker_labels().len() == 1 && self.ticker_labels()[0].is_empty();
                    if is_notice && is_empty_session {
                        out.err(match lang {
                            "ja" => "❌ /auto notice は銘柄をロードしてから使用してください。",
                            _ => "❌ /auto notice requires at least one ticker to be loaded first.",
                        });
                    } else if !ctx.config.analysis_mode.is_intraday() {
                        out.err(match lang {
                            "ja" => "❌ 自動リロードは分足モードでのみ使用できます。",
                            _ => "❌ Auto-reload is available only in intraday modes.",
                        });
                    } else {
                        *ctx.autoreload_enabled = true;
                        if is_notice {
                            *ctx.autoreload_notice = true;
                        }
                        *ctx.next_reload_at =
                            super::ticker::next_autoreload_deadline(ctx.config.analysis_mode);
                        let interval_label = ctx.config.analysis_mode.data_interval();
                        out.line(match (lang, is_notice) {
                            ("ja", true) => format!(
                                "✅ 自動リロードを有効にしました（{}ごと）。リロード時に価格変動を表示します。",
                                interval_label
                            ),
                            ("ja", false) => {
                                format!("✅ 自動リロードを有効にしました（{}ごと）。", interval_label)
                            }
                            (_, true) => format!(
                                "✅ Auto-reload enabled (every {}). Price change will be shown on each reload.",
                                interval_label
                            ),
                            (_, false) => format!("✅ Auto-reload enabled (every {}).", interval_label),
                        });
                        if is_notice {
                            // One-shot snapshot (no prev to diff against → "---").
                            let new_prices = super::ticker::reload_chat_tickers(
                                self,
                                ctx.config,
                                ctx.ticker_name_map,
                                false,
                                false,
                                lang,
                                out,
                                false,
                            )
                            .await;
                            let now = chrono::Local::now().format("%Y/%m/%d %H:%M:%S").to_string();
                            for (i, new_price_opt) in new_prices.iter().enumerate() {
                                if let Some(&new_price) = new_price_opt.as_ref() {
                                    let label =
                                        self.ticker_labels().get(i).cloned().unwrap_or_default();
                                    out.line(match lang {
                                        "ja" => {
                                            let label_ja =
                                                label.replacen(" (", "（", 1).replace(')', "）");
                                            format!(
                                                "{}  銘柄: {}  最新取得価格: {:.2}　前回比: ---",
                                                now, label_ja, new_price
                                            )
                                        }
                                        _ => format!(
                                            "{}  Symbol: {}  Latest: {:.2}  vs prev reload: ---",
                                            now, label, new_price
                                        ),
                                    });
                                    if let Some(summary) = self.last_indicator_summaries().get(i) {
                                        if !summary.is_empty() {
                                            out.line(match lang {
                                                "ja" => format!("  指標: {}", summary),
                                                _ => format!("  Indicators: {}", summary),
                                            });
                                        }
                                    }
                                }
                            }
                            *ctx.autoreload_last_prices = new_prices;
                        }
                    }
                }
                Some(false) => {
                    *ctx.autoreload_enabled = false;
                    *ctx.autoreload_notice = false;
                    *ctx.next_reload_at = None;
                    out.line(match lang {
                        "ja" => "✅ 自動リロードを無効にしました。",
                        _ => "✅ Auto-reload disabled.",
                    });
                }
            }
            return Dispatch::Handled;
        }

        if raw_input == "/sym add" || raw_input.starts_with("/sym add ") {
            let is_empty_session =
                self.ticker_labels().len() == 1 && self.ticker_labels()[0].is_empty();
            let raw_sym = raw_input.strip_prefix("/sym add ").unwrap_or("").trim();
            if raw_sym.is_empty() {
                out.err(match lang {
                    "ja" => "❌ /sym add には銘柄コードを指定してください。例: /sym add 9433.T",
                    _ => "❌ /sym add requires a symbol. Example: /sym add 9433.T",
                });
                return Dispatch::Handled;
            }
            if !is_empty_session && self.base_contexts().len() >= 5 {
                out.err(match lang {
                    "ja" => "❌ 比較モードは5銘柄までです。/sym del <番号> で削除してから /sym add <銘柄> を実行してください。",
                    _ => "❌ Comparison is limited to 5 tickers. Use /sym del <number> to remove one first, then /sym add <symbol>.",
                });
                return Dispatch::Handled;
            }
            let step1 = crate::bootstrap::normalize_ticker_input(raw_sym);
            let normalized = match crate::bootstrap::sanitize_ticker(&step1) {
                Err(err) => {
                    out.err(format!("❌ {}", err));
                    return Dispatch::Handled;
                }
                Ok(t) => crate::bootstrap::normalize_ticker(&t),
            };
            if !is_empty_session {
                let already_loaded = self.ticker_labels().iter().any(|lbl| {
                    let sym = match lbl.find(" (") {
                        Some(pos) => &lbl[..pos],
                        None => lbl.as_str(),
                    };
                    sym.eq_ignore_ascii_case(&normalized)
                });
                if already_loaded {
                    out.err(match lang {
                        "ja" => format!("❌ {} はすでにロードされています。", normalized),
                        _ => format!("❌ {} is already loaded.", normalized),
                    });
                    return Dispatch::Handled;
                }
            }
            out.line(match lang {
                "ja" => format!("⏳ {} のデータを取得中...", normalized),
                _ => format!("⏳ Fetching {}...", normalized),
            });
            match super::ticker::load_ticker_analysis(
                ctx.config,
                &normalized,
                ctx.ticker_name_map,
                !ctx.config.no_news,
                true,
            )
            .await
            {
                Err(e) => out.err(format!("❌ {}", e)),
                Ok(loaded) => {
                    let extensions_ok = loaded.extensions_ok;
                    let label = loaded.label.clone();
                    if is_empty_session {
                        super::ticker::apply_ticker_switch(
                            self,
                            ctx.config,
                            super::ticker::TickerSwitchData {
                                ticker: normalized.clone(),
                                ticker_label: loaded.label.clone(),
                                base_context: loaded.context,
                                technical_text: loaded.technical_text,
                                fundamental_text: loaded.fundamental_text,
                                news: loaded.news_items.unwrap_or_default(),
                                data_as_of: loaded.data_as_of,
                                facts: Some(loaded.facts),
                            },
                        );
                        out.line(match (lang, extensions_ok) {
                            ("ja", true) => format!("✅ {} を読み込みました。", label),
                            ("ja", false) => {
                                format!("⚠️ {} を読み込みました（基本指標のみ）。", label)
                            }
                            (_, true) => format!("✅ Loaded {}.", label),
                            (_, false) => {
                                format!("⚠️ Loaded {} with basic indicators only.", label)
                            }
                        });
                    } else {
                        super::ticker::apply_ticker_add(
                            self,
                            loaded.label.clone(),
                            loaded.context,
                            loaded.technical_text,
                            loaded.fundamental_text,
                            loaded.news_items.unwrap_or_default(),
                            loaded.data_as_of,
                            Some(loaded.facts),
                        );
                        out.line(match (lang, extensions_ok) {
                            ("ja", true) => format!(
                                "✅ {} を追加しました。比較分析が可能です。トークン消費が増えるため /mem low の使用を推奨します。会話履歴は維持されます。",
                                label
                            ),
                            ("ja", false) => format!(
                                "⚠️ {} を追加しました（基本指標のみ）。トークン消費が増えるため /mem low の使用を推奨します。会話履歴は維持されます。",
                                label
                            ),
                            (_, true) => format!(
                                "✅ Added {}. Comparison analysis is now available. Token usage increases — consider /mem low. Conversation history is preserved.",
                                label
                            ),
                            (_, false) => format!(
                                "⚠️ Added {} with basic indicators only. Token usage increases — consider /mem low. Conversation history is preserved.",
                                label
                            ),
                        });
                    }
                }
            }
            return Dispatch::Handled;
        }

        if raw_input == "/sym" || raw_input.starts_with("/sym ") {
            let raw_sym = raw_input.strip_prefix("/sym ").unwrap_or("").trim();
            if raw_sym.is_empty() {
                let empty_session =
                    self.ticker_labels().len() == 1 && self.ticker_labels()[0].is_empty();
                if empty_session {
                    out.line(match lang {
                        "ja" => "銘柄はロードされていません。/sym add <銘柄> で追加してください。",
                        _ => "No tickers loaded. Use /sym add <symbol>.",
                    });
                } else {
                    out.line(match lang {
                        "ja" => "ロード中の銘柄:",
                        _ => "Loaded tickers:",
                    });
                    for (i, label) in self.ticker_labels().iter().enumerate() {
                        out.line(format!("  {}: {}", i + 1, label));
                    }
                }
                return Dispatch::Handled;
            }
            if raw_input == "/sym del" || raw_input.starts_with("/sym del ") {
                let num_str = raw_input.strip_prefix("/sym del ").unwrap_or("").trim();
                if self.ticker_labels().len() == 1 && self.ticker_labels()[0].is_empty() {
                    out.err(match lang {
                        "ja" => "❌ 銘柄がロードされていません。",
                        _ => "❌ No tickers loaded.",
                    });
                    return Dispatch::Handled;
                }
                let n = self.ticker_labels().len();
                match num_str.parse::<usize>().ok().filter(|&x| x >= 1 && x <= n) {
                    None => out.err(match lang {
                        "ja" => {
                            format!("❌ 銘柄番号を指定してください（1〜{}）: /sym del <番号>", n)
                        }
                        _ => format!("❌ Specify ticker number (1–{}): /sym del <number>", n),
                    }),
                    Some(x) => {
                        let i = x - 1;
                        let removed_label = self.tickers[i].label.clone();
                        self.tickers.remove(i);
                        ctx.autoreload_last_prices.clear();
                        self.conversation_summary = None;
                        self.recent_turns.clear();
                        self.debate_entries.clear();
                        self.board_entries.clear();
                        self.board_entry_counter = 0;
                        if self.tickers.is_empty() {
                            self.tickers = vec![super::TickerEntry {
                                label: String::new(),
                                base_context: String::new(),
                                technical_text: String::new(),
                                indicator_summary: String::new(),
                                fundamental_text: String::new(),
                                news: Vec::new(),
                                data_as_of: String::new(),
                                facts: None,
                            }];
                            out.line(match lang {
                                "ja" => {
                                    format!(
                                        "✅ {} を削除しました。銘柄はロードされていません。",
                                        removed_label
                                    )
                                }
                                _ => format!("✅ Removed {}. No tickers loaded.", removed_label),
                            });
                        } else {
                            let first_sym = self.ticker_labels()[0]
                                .split(" (")
                                .next()
                                .unwrap_or("")
                                .to_string();
                            if !first_sym.is_empty() {
                                ctx.config.ticker = first_sym;
                            }
                            out.line(match lang {
                                "ja" => {
                                    format!(
                                        "✅ {} を削除しました。会話履歴をリセットしました。",
                                        removed_label
                                    )
                                }
                                _ => {
                                    format!(
                                        "✅ Removed {}. Conversation history cleared.",
                                        removed_label
                                    )
                                }
                            });
                            for (idx, label) in self.ticker_labels().iter().enumerate() {
                                out.line(format!("  {}: {}", idx + 1, label));
                            }
                        }
                    }
                }
            } else {
                out.err(match lang {
                    "ja" => "❌ 不明な /sym サブコマンドです。/sym del <番号> または /sym add <銘柄> を使用してください。",
                    _ => "❌ Unknown /sym subcommand. Use /sym del <number> or /sym add <symbol>.",
                });
            }
            return Dispatch::Handled;
        }

        // Migration hint: a renamed old command → point at the new one.
        if raw_input.starts_with('/') {
            let head = raw_input.split_whitespace().next().unwrap_or("");
            if let Some(new_cmd) = renamed_command(head) {
                out.line(match lang {
                    "ja" => format!(
                        "ℹ️ `{}` は廃止されました → `{}` を使用してください（/help 参照）。",
                        head, new_cmd
                    ),
                    _ => format!("ℹ️ `{}` was renamed → use `{}` (see /help).", head, new_cmd),
                });
                return Dispatch::Handled;
            }
        }

        Dispatch::Passthrough
    }
}

// ── /alert command (Web-only chat-notification control) ──────────────────────

/// Handle `/alert …`. The notification monitor lives in `xoksa serve`, so this is
/// informational in the terminal REPL and active only over the Web UI.
async fn handle_alert(arg: &str, lang: &str, interactive: bool, out: &mut ChatOut) {
    if interactive {
        out.line(if lang == "ja" {
            "🔔 /alert は Web UI（xoksa serve）専用です。通知モニタは serve 起動中のみ動作します。"
        } else {
            "🔔 /alert works only in the Web UI (xoksa serve); the notification monitor runs only while serve is up."
        });
        return;
    }

    let mut parts = arg.split_whitespace();
    let sub = parts.next().unwrap_or("list");
    match sub {
        "list" => alert_list(lang, out),
        "on" | "off" => {
            let Some(n) = parts.next().and_then(|s| s.parse::<u8>().ok()) else {
                out.err(if lang == "ja" {
                    "❌ 使い方: /alert on <ルール番号>"
                } else {
                    "❌ Usage: /alert on <rule#>"
                });
                return;
            };
            let active = sub == "on";
            match crate::server::monitor::set_active(n, active) {
                Ok(()) => out.line(match (lang, active) {
                    ("ja", true) => format!("✅ ルール #{n} を有効化しました。"),
                    ("ja", false) => format!("✅ ルール #{n} を無効化しました。"),
                    (_, true) => format!("✅ Rule #{n} enabled."),
                    (_, false) => format!("✅ Rule #{n} disabled."),
                }),
                Err(e) => out.err(alert_op_error(&e, lang)),
            }
        }
        "del" => {
            let Some(n) = parts.next().and_then(|s| s.parse::<u8>().ok()) else {
                out.err(if lang == "ja" {
                    "❌ 使い方: /alert del <ルール番号>"
                } else {
                    "❌ Usage: /alert del <rule#>"
                });
                return;
            };
            // Delete from the file too, under the same lock. Removing in memory
            // only would bring the rule back on the next start, which reads as the
            // delete not working.
            match crate::server::monitor::remove_persisted(n) {
                Ok(saved) => {
                    out.line(match (lang, &saved) {
                        ("ja", Ok(())) => format!("✅ ルール #{n} を削除しました。"),
                        ("ja", Err(e)) => format!(
                            "⚠️ ルール #{n} の監視は止めましたが、設定から消せませんでした（再起動で復活します）: {e}"
                        ),
                        (_, Ok(())) => format!("✅ Rule #{n} removed."),
                        (_, Err(e)) => format!(
                            "⚠️ Rule #{n} is no longer monitored but could not be removed from the config (it will come back on restart): {e}"
                        ),
                    })
                }
                Err(e) => out.err(alert_op_error(&e, lang)),
            }
        }
        "test" => {
            let Some(chan) = parts.next() else {
                out.err(if lang == "ja" {
                    "❌ 使い方: /alert test <チャンネル名>"
                } else {
                    "❌ Usage: /alert test <channel>"
                });
                return;
            };
            let Some(n) = crate::server::monitor::channel_number_by_name(chan) else {
                out.err(if lang == "ja" {
                    format!("❌ 「{chan}」という通知チャンネルがありません。")
                } else {
                    format!("❌ No notification channel named \"{chan}\".")
                });
                return;
            };
            match crate::server::monitor::send_test(n, lang).await {
                Ok(()) => out.line(if lang == "ja" {
                    format!("✅ チャンネル #{n} にテスト通知を送信しました。")
                } else {
                    format!("✅ Sent a test notification to channel #{n}.")
                }),
                Err(e) => out.err(alert_test_error(&e, lang)),
            }
        }
        "add" => {
            let rest: Vec<&str> = parts.collect();
            alert_add(&rest, lang, out);
        }
        _ => out.err(if lang == "ja" {
            "❌ 不明なサブコマンド。使い方: /alert [list | on <n> | off <n> | add <銘柄> <足> <条件> <チャンネル名> [explain] | del <n> | test <チャンネル名>]"
        } else {
            "❌ Unknown subcommand. Usage: /alert [list | on <n> | off <n> | add <ticker> <mode> <cond> <channel> [explain] | del <n> | test <channel>]"
        }),
    }
}

/// Render `/alert list`: the session's rules, then the env-defined channels.
fn alert_list(lang: &str, out: &mut ChatOut) {
    let (rules, channels) = crate::server::monitor::list();
    if lang == "ja" {
        out.line(format!(
            "🔔 アラートルール（{}/{} 件）",
            rules.len(),
            crate::config::ALERT_MAX
        ));
        if rules.is_empty() {
            out.line("  （ルールなし）");
        }
        for r in &rules {
            let state = if r.active { "有効" } else { "無効" };
            let ex = if r.explain { " +解説" } else { "" };
            out.line(format!(
                "  #{} [{}] {} {}  → 通知#{}{}  ({})",
                r.n, r.mode, r.ticker, r.cond, r.notify, ex, state
            ));
        }
        out.line("通知チャンネル");
        if channels.is_empty() {
            out.line("  （未定義。xoksa.env に NOTIFY_<n>_* を設定）");
        }
        for c in &channels {
            let sec = if c.secret_set {
                "🔑 設定済"
            } else {
                "⚠️ secret未設定"
            };
            out.line(format!("  #{} [{}] {}  {}", c.n, c.kind, c.name, sec));
        }
        out.line("※ 有効/無効はこの起動中だけです。再起動すると有効に戻ります。");
        out.line("※ xoksa.env を直接編集した場合は、再起動するまで反映されません。");
    } else {
        out.line(format!(
            "🔔 Alert rules ({}/{})",
            rules.len(),
            crate::config::ALERT_MAX
        ));
        if rules.is_empty() {
            out.line("  (none)");
        }
        for r in &rules {
            let state = if r.active { "on" } else { "off" };
            let ex = if r.explain { " +explain" } else { "" };
            out.line(format!(
                "  #{} [{}] {} {}  → notify#{}{}  ({})",
                r.n, r.mode, r.ticker, r.cond, r.notify, ex, state
            ));
        }
        out.line("Notification channels");
        if channels.is_empty() {
            out.line("  (none defined; set NOTIFY_<n>_* in xoksa.env)");
        }
        for c in &channels {
            let sec = if c.secret_set {
                "🔑 secret set"
            } else {
                "⚠️ secret missing"
            };
            out.line(format!("  #{} [{}] {}  {}", c.n, c.kind, c.name, sec));
        }
        out.line("Note: on/off lasts for this run only; a restart re-enables the rule.");
        out.line("Note: edits made directly in xoksa.env take effect on the next restart.");
    }
}

/// Parse and apply `/alert add <ticker> <mode> <cond> <channel> [explain]`. The
/// channel is referenced by its `NOTIFY_<n>_NAME`; `cond` is a single token
/// (e.g. `rsi<=30`); a trailing literal `explain` attaches the LLM note.
fn alert_add(args: &[&str], lang: &str, out: &mut ChatOut) {
    let usage = if lang == "ja" {
        "❌ 使い方: /alert add <銘柄> <足> <条件> <チャンネル名> [explain]（足=1m|5m|15m|30m|60m、条件は空白なし 例 rsi<=30）"
    } else {
        "❌ Usage: /alert add <ticker> <mode> <cond> <channel> [explain] (mode=1m|5m|15m|30m|60m, cond has no spaces, e.g. rsi<=30)"
    };
    let (Some(ticker), Some(mode_str), Some(cond_str), Some(chan)) =
        (args.first(), args.get(1), args.get(2), args.get(3))
    else {
        out.err(usage);
        return;
    };
    // A 5th arg is only the literal `explain`; anything else is a usage error.
    let explain = match args.get(4) {
        None => false,
        Some(&"explain") => true,
        Some(_) => {
            out.err(usage);
            return;
        }
    };
    if args.len() > 5 {
        out.err(usage);
        return;
    }

    let Some(mode) = crate::config::AnalysisMode::from_value(mode_str).filter(|m| m.is_intraday())
    else {
        out.err(if lang == "ja" {
            "❌ 足は 1m|5m|15m|30m|60m のいずれかです。"
        } else {
            "❌ mode must be one of 1m|5m|15m|30m|60m."
        });
        return;
    };
    let Some(when) = crate::config::parse_alert_condition(cond_str) else {
        out.err(if lang == "ja" {
            "❌ 条件を解釈できません。形式: <指標><op><数値>、op = <= >= < >（例 rsi<=30, score>=4）"
        } else {
            "❌ Cannot parse the condition. Form: <indicator><op><number>, op = <= >= < > (e.g. rsi<=30)"
        });
        return;
    };
    let Some(notify) = crate::server::monitor::channel_number_by_name(chan) else {
        out.err(if lang == "ja" {
            format!("❌ 「{chan}」という通知チャンネルがありません（xoksa.env の NOTIFY_<n>_NAME を確認）。")
        } else {
            format!("❌ No notification channel named \"{chan}\" (check NOTIFY_<n>_NAME in xoksa.env).")
        });
        return;
    };

    // A rule outlives the session it was created in, whichever surface created it —
    // the chat command persists exactly as the dashboard does, and under the same
    // lock that assigns the number. `--private` writes nothing (the helper returns
    // early), so a no-trace session keeps the rule in memory only.
    match crate::server::monitor::add_persisted(
        ticker.to_string(),
        when,
        cond_str,
        mode,
        notify,
        explain,
    ) {
        Ok((n, saved)) => match saved {
            Ok(()) => out.line(if lang == "ja" {
                format!("✅ ルール #{n} を追加しました（次の tick から監視）。")
            } else {
                format!("✅ Added rule #{n} (monitored from the next tick).")
            }),
            Err(e) => out.err(if lang == "ja" {
                format!("⚠️ ルール #{n} は監視を始めましたが、保存できませんでした（再起動で消えます）: {e}")
            } else {
                format!(
                    "⚠️ Rule #{n} is being monitored but could not be saved (it will be lost on restart): {e}"
                )
            }),
        },
        Err(e) => out.err(alert_op_error(&e, lang)),
    }
}

/// Localize an `/alert` mutation error.
fn alert_op_error(e: &crate::server::monitor::OpError, lang: &str) -> String {
    use crate::server::monitor::OpError::*;
    match e {
        RuleNotFound(n) => {
            if lang == "ja" {
                format!("❌ ルール #{n} は存在しません。")
            } else {
                format!("❌ No such rule #{n}.")
            }
        }
        ChannelNotFound(n) => {
            if lang == "ja" {
                format!(
                    "❌ 通知チャンネル #{n} が未定義です（xoksa.env に NOTIFY_{n}_KIND を設定）。"
                )
            } else {
                format!("❌ Notification channel #{n} is not defined (set NOTIFY_{n}_KIND in xoksa.env).")
            }
        }
        NotIntraday => {
            if lang == "ja" {
                "❌ mode は分足（1m|5m|15m|30m|60m）にしてください。".to_string()
            } else {
                "❌ mode must be an intraday bar (1m|5m|15m|30m|60m).".to_string()
            }
        }
        Full => {
            if lang == "ja" {
                format!(
                    "❌ ルールは最大{}件です。1件削除すると空きができます。",
                    crate::config::ALERT_MAX
                )
            } else {
                format!(
                    "❌ The rule limit ({}) has been reached. Remove a rule to free a slot.",
                    crate::config::ALERT_MAX
                )
            }
        }
    }
}

/// Localize an `/alert test` error.
fn alert_test_error(e: &crate::server::monitor::TestError, lang: &str) -> String {
    use crate::server::monitor::TestError::*;
    match e {
        ChannelNotFound(n) => {
            if lang == "ja" {
                format!("❌ 通知チャンネル #{n} が未定義です。")
            } else {
                format!("❌ Notification channel #{n} is not defined.")
            }
        }
        SecretMissing(n) => {
            if lang == "ja" {
                format!("❌ NOTIFY_{n}_SECRET が未設定です（--update-key で登録）。")
            } else {
                format!("❌ NOTIFY_{n}_SECRET is not set (register it with --update-key).")
            }
        }
        Keychain(s) => {
            if lang == "ja" {
                format!("❌ キーチェーン読み取りに失敗: {s}")
            } else {
                format!("❌ Keychain read failed: {s}")
            }
        }
        Invalid(s) => {
            if lang == "ja" {
                format!("❌ チャンネル secret が不正です: {s}")
            } else {
                format!("❌ The channel secret is invalid: {s}")
            }
        }
        Send(s) => {
            if lang == "ja" {
                format!("❌ 送信に失敗しました: {s}")
            } else {
                format!("❌ Send failed: {s}")
            }
        }
    }
}

#[cfg(test)]
mod panel_news_tests {
    use super::*;

    fn art(title: &str) -> crate::news::Article {
        crate::news::Article {
            title: title.to_string(),
            url: format!("https://example.test/{title}"),
            published_at: None,
        }
    }

    // The panel→LLM news bridge: what the /news handler stores is exactly what the
    // chat reads back (SOT). Covers a miss (CLI/no-panel fallback), the id/order
    // mapping, an overwrite (filter toggle / new search), and an empty result.
    #[test]
    fn panel_news_store_roundtrip_miss_and_overwrite() {
        let sid = "test-sid-panel-news-1";
        let sym = "TESTSYM.T";

        // Never fetched → None → the chat keeps its own fetched news (CLI path).
        assert!(panel_news_items(sid, "NEVERFETCHED.T").is_none());

        // "Filter on" (finance) set → buffer equals it, ids N01.. in panel order.
        store_panel_news(sid, sym, &[art("決算"), art("株価")]);
        let items = panel_news_items(sid, sym).expect("stored → Some");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "N01");
        assert_eq!(items[0].title, "決算");
        assert_eq!(items[1].id, "N02");
        assert_eq!(items[1].title, "株価");

        // "フィルタを外す" → panel refetches raw → overwrite → buffer == latest display.
        store_panel_news(sid, sym, &[art("野球")]);
        let raw = panel_news_items(sid, sym).expect("Some");
        assert_eq!(raw.len(), 1);
        assert_eq!(raw[0].title, "野球");

        // Searched, nothing found → stored empty, distinct from a never-fetched miss.
        store_panel_news(sid, sym, &[]);
        assert_eq!(panel_news_items(sid, sym).map(|v| v.len()), Some(0));
    }
}
