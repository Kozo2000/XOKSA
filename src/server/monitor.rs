//! Alert monitor: a background supervisor task that watches the intraday alert
//! rules from `xoksa.env` (`ALERT_<n>_*`) while `xoksa serve` runs and pushes a
//! one-line SOT message to a chat channel (`NOTIFY_<n>_*`) when a rule's
//! condition first holds on the latest fetched bar. That bar is not necessarily
//! closed: on Japanese intraday timeframes it is the still-forming one carrying
//! the real-time quote, so a crossing is caught within the bar rather than only
//! after it closes (security-design §4). "Confirmed" below means a value the
//! PROGRAM computed rather than one a model produced — not a closed bar.
//!
//! Design:
//! - A SINGLE supervisor task ticks on a base interval and evaluates each active
//!   rule when its own intraday bar interval has elapsed — the same cadence as the
//!   terminal `/autoreload`. Rules and channels live in a process-global store so
//!   the Web-chat `/alert` command and the dashboard can add/remove/toggle them
//!   live. The store is seeded from `xoksa.env` once, at startup; `add`/`del`
//!   write back so a rule survives a restart whichever surface created it.
//!   Enable/disable is deliberately NOT persisted — it is a pause for this run,
//!   and a rule left silently disabled across restarts is the worse failure.
//! - Each evaluation recomputes the symbol through the SHARED analysis path
//!   ([`crate::app::build_analyzed_guard`]) — the identical computed reading the
//!   dashboard and CLI show — then evaluates the single condition with the
//!   backtest evaluator. There is no parallel indicator math here (SOT).
//! - Rising-edge + per-bar dedupe: fire only when the condition goes
//!   false→true, and never twice for the same bar timestamp.
//! - The message carries only computed (SOT) values. The channel secret is read
//!   from the OS keychain at dispatch and kept in `Zeroizing`; it never touches
//!   the env map, argv, or the log. The secret is NEVER accepted via `/alert`.

use crate::config::{AlertRule, AnalysisMode, NotifyChannel};
use crate::notify::Notifier;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// How often the supervisor wakes. Each rule still self-limits to its own bar
/// interval, so a short base tick only bounds how soon a newly-added rule or a
/// re-enabled one starts — it does not increase provider fetches for slow bars.
const SUPERVISOR_TICK_SECS: u64 = 30;

/// The process-global alert store, shared by the supervisor task and the Web-chat
/// `/alert` command. Seeded once at serve startup.
static MONITOR: OnceLock<Mutex<Monitor>> = OnceLock::new();

fn store() -> &'static Mutex<Monitor> {
    MONITOR.get_or_init(|| Mutex::new(Monitor::default()))
}

#[derive(Default)]
struct Monitor {
    rules: Vec<RuleState>,
    /// Channels are read-only here (defined in `xoksa.env`); `/alert` references
    /// them by number but never creates one (the secret is Class A).
    channels: Vec<NotifyChannel>,
    https_proxy: Option<String>,
    no_proxy: Option<String>,
}

/// The outcome of the last notification attempt for a rule.
///
/// Live state, not persisted — like `active`, it describes this run. A failure
/// that mattered is one the operator can act on now; a stale one read from a file
/// after a restart would say nothing about whether the channel works today.
struct SendOutcome {
    /// Local time, in the same `%Y-%m-%d %H:%M` form the analysis stamps use.
    at: String,
    ok: bool,
    /// Why it failed. Already free of the channel secret — the notifier redacts
    /// before it returns (see `crate::notify`), and the messages built here name
    /// only a rule, a channel number and a reason.
    detail: Option<String>,
}

/// A rule plus its live monitoring state.
struct RuleState {
    rule: AlertRule,
    active: bool,
    /// The condition's state on the last observed bar (rising-edge gate).
    prev_true: bool,
    /// The bar we last fired on, by timestamp (dedupe against re-polls of it).
    last_fired_bar: Option<i64>,
    /// When this rule was last evaluated (for per-rule interval gating).
    last_eval: Option<tokio::time::Instant>,
    /// The last notification attempt, for the dashboard to show.
    last_send: Option<SendOutcome>,
}

impl RuleState {
    fn new(rule: AlertRule) -> Self {
        RuleState {
            rule,
            active: true,
            prev_true: false,
            last_fired_bar: None,
            last_eval: None,
            last_send: None,
        }
    }
}

/// Record the outcome of an attempt so the dashboard can show it.
///
/// Called at **every** terminus that follows a fire, including the ones that
/// never reach the network (no channel, no secret, keychain unreadable, secret
/// invalid). Those are precisely the failures that were invisible: a rule that
/// fired and failed to send looked exactly like a rule that never fired, and the
/// only trace was a line in the diagnostic log. The lock is taken and released
/// here, never held across an await.
fn record_send(rule_n: u8, ok: bool, detail: Option<String>) {
    let at = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut mon = store().lock().expect("alert store poisoned");
    if let Some(rs) = mon.rules.iter_mut().find(|r| r.rule.n == rule_n) {
        rs.last_send = Some(SendOutcome { at, ok, detail });
    }
}

/// Failure of an `/alert` mutation, mapped to a localized message by the caller.
pub(crate) enum OpError {
    RuleNotFound(u8),
    ChannelNotFound(u8),
    NotIntraday,
    Full,
}

/// Failure of an `/alert test`, mapped to a localized message by the caller.
pub(crate) enum TestError {
    ChannelNotFound(u8),
    SecretMissing(u8),
    Keychain(String),
    Invalid(String),
    Send(String),
}

/// A rule row for `/alert list` and the dashboard alerts panel.
#[derive(serde::Serialize)]
pub(crate) struct RuleView {
    pub n: u8,
    pub ticker: String,
    pub mode: String,
    pub cond: String,
    pub notify: u8,
    pub explain: bool,
    pub active: bool,
    /// The last notification attempt in this run — absent until the rule fires
    /// once. A failure here is the only place the operator can see that a rule
    /// fired but the message did not arrive.
    pub last_send_at: Option<String>,
    pub last_send_ok: Option<bool>,
    pub last_send_error: Option<String>,
}

/// A channel row for `/alert list` and the dashboard alerts panel.
#[derive(serde::Serialize)]
pub(crate) struct ChannelView {
    pub n: u8,
    pub kind: String,
    pub name: String,
    /// Whether `NOTIFY_<n>_SECRET` is present in the keychain (channel is usable).
    pub secret_set: bool,
}

/// Seed the store from the current `xoksa.env` and spawn the supervisor. Called
/// once at serve startup. The supervisor runs even with zero rules so `/alert add`
/// works on a fresh session; it simply idles until a rule is active.
pub fn spawn_alert_monitor() {
    let env_map = crate::bootstrap::load_env_map();
    let rules = crate::config::scan_alert_rules(&env_map);
    let channels = crate::config::scan_notify_channels(&env_map);
    // A representative env-resolved config, only to carry the proxy settings the
    // notifier needs (the ticker is irrelevant here).
    let base = super::api::build_server_config("SPY", None);

    {
        let mut mon = store().lock().expect("alert store poisoned");
        mon.channels = channels;
        mon.https_proxy = base.https_proxy.clone();
        mon.no_proxy = base.no_proxy.clone();
        mon.rules = rules
            .into_iter()
            .filter_map(|rule| match rule.mode.filter(|m| m.is_intraday()) {
                Some(_) => Some(RuleState::new(rule)),
                None => {
                    crate::logging::warn(
                        "XK-ALERT",
                        &format!(
                            "alert #{} ({}) skipped: MODE must be an intraday bar (1m|5m|15m|30m|60m)",
                            rule.n, rule.ticker
                        ),
                    );
                    None
                }
            })
            .collect();
        crate::logging::info(
            "XK-ALERT",
            &format!(
                "monitor started: {} rule(s), {} channel(s)",
                mon.rules.len(),
                mon.channels.len()
            ),
        );
    }

    tokio::spawn(supervise());
}

/// The supervisor loop: on each base tick, evaluate every active rule whose bar
/// interval has elapsed. The lock is held only to pick due rules and to update
/// per-rule state — never across the network I/O of evaluation or dispatch.
async fn supervise() {
    let mut tick = tokio::time::interval(Duration::from_secs(SUPERVISOR_TICK_SECS));
    loop {
        tick.tick().await;
        let now = tokio::time::Instant::now();
        let due: Vec<AlertRule> = {
            let mut mon = store().lock().expect("alert store poisoned");
            let mut due = Vec::new();
            for rs in mon.rules.iter_mut() {
                if !rs.active {
                    continue;
                }
                let mode = rs.rule.mode.unwrap_or(AnalysisMode::Intraday5m);
                let iv = rule_interval(mode);
                if rs.last_eval.is_none_or(|t| now.duration_since(t) >= iv) {
                    rs.last_eval = Some(now);
                    due.push(rs.rule.clone());
                }
            }
            due
        };
        for rule in due {
            evaluate_and_maybe_fire(rule).await;
        }
    }
}

fn rule_interval(mode: AnalysisMode) -> Duration {
    Duration::from_secs(
        mode.intraday_minutes()
            .map(|m| u64::from(m) * 60)
            .unwrap_or(300),
    )
}

/// Recompute one rule's symbol, evaluate the condition, and — on a fresh rising
/// edge — dispatch the notification. Errors are logged and swallowed so one bad
/// symbol or channel can never stop the supervisor.
async fn evaluate_and_maybe_fire(rule: AlertRule) {
    let Some(mode) = rule.mode else { return };
    let config = super::api::build_server_config(&rule.ticker, Some(mode.as_str()));
    let name_map = match &config.alias_csv {
        Some(path) => crate::bootstrap::load_alias_csv(path).unwrap_or_default(),
        None => HashMap::new(),
    };
    let guard = match crate::app::build_analyzed_guard(&config, &rule.ticker, &name_map).await {
        Ok(g) => g,
        Err(e) => {
            crate::logging::warn(
                "XK-ALERT",
                &format!(
                    "alert #{} ({}) recompute failed: {}",
                    rule.n, rule.ticker, e
                ),
            );
            return;
        }
    };

    // Evaluate on the computed reading via the shared backtest evaluator (value
    // comparisons only, so no previous bar needed). The guard already holds the
    // latest bar's volume, so a `volume` condition is evaluated too; when the
    // provider omits it the value is simply absent (the condition reads false).
    let map = crate::backtest::guard_indicator_map(
        &config,
        &guard,
        guard.get_close(),
        guard.get_latest_volume(),
    );
    let met = crate::backtest::eval_condition(&map, None, &rule.when);
    let bar_id = guard.get_bar_timestamp().unwrap_or_default();

    // Decide under the lock (updating per-rule state), then release before sending.
    let (fire, channel) = {
        let mut mon = store().lock().expect("alert store poisoned");
        let fire = {
            let Some(rs) = mon.rules.iter_mut().find(|r| r.rule.n == rule.n) else {
                return;
            };
            if !rs.active {
                return;
            }
            let f = should_fire(met, rs.prev_true, bar_id, rs.last_fired_bar);
            rs.prev_true = met;
            if f {
                rs.last_fired_bar = Some(bar_id);
            }
            f
        };
        let channel = mon.channels.iter().find(|c| c.n == rule.notify).cloned();
        (fire, channel)
    };
    if !fire {
        return;
    }
    let Some(channel) = channel else {
        crate::logging::warn(
            "XK-ALERT",
            &format!(
                "alert #{} ({}): NOTIFY={} has no matching channel — not sent",
                rule.n, rule.ticker, rule.notify
            ),
        );
        record_send(
            rule.n,
            false,
            Some(format!("NOTIFY={} has no matching channel", rule.notify)),
        );
        return;
    };

    let base = format_message(&rule, mode, &guard, &map, &config.lang);
    let message = if rule.explain {
        // The confirmed values of this instrument, straight from the guard the
        // alert was evaluated on — the guard verifies the note's numbers against
        // these structures, not against the prompt text.
        let mut facts =
            crate::integrity::facts_from_parts([crate::integrity::SymbolFacts::from_sources(
                &guard, &config, None, None,
            )]);
        // The message states the rule that fired, so its threshold is part of
        // what the engine told the model. Restating it ("score <= 5 のアラート
        // 条件が成立") must not be read as a claim that the score is 5.
        if let (Some(op), Some(threshold)) = (
            crate::integrity::Comparison::from_op(&rule.when.op),
            rule.when.value,
        ) {
            facts.push_bound(&rule.when.left, op, threshold);
        }
        match explain_note(&config, &base, Some(&facts)).await {
            Some(note) => {
                let sep = if config.lang == "ja" {
                    "  ／ "
                } else {
                    "  — "
                };
                format!("{base}{sep}{note}")
            }
            None => base,
        }
    } else {
        base
    };
    let (https_proxy, no_proxy) = proxy_snapshot();
    dispatch(
        &rule,
        &channel,
        &message,
        https_proxy.as_deref(),
        no_proxy.as_deref(),
    )
    .await;
}

/// The proxy settings captured at seed time (the notifier needs them per send).
fn proxy_snapshot() -> (Option<String>, Option<String>) {
    let mon = store().lock().expect("alert store poisoned");
    (mon.https_proxy.clone(), mon.no_proxy.clone())
}

/// Resolve the channel secret from the keychain and send. All failures are logged
/// and swallowed — a misconfigured channel must not tear the monitor down.
async fn dispatch(
    rule: &AlertRule,
    channel: &NotifyChannel,
    message: &str,
    https_proxy: Option<&str>,
    no_proxy: Option<&str>,
) {
    let secret = match crate::keystore::get_key(&format!("NOTIFY_{}_SECRET", channel.n)) {
        Ok(Some(s)) => s,
        Ok(None) => {
            crate::logging::warn(
                "XK-ALERT",
                &format!(
                    "alert #{} ({}): NOTIFY_{}_SECRET is not set in the keychain — cannot send",
                    rule.n, rule.ticker, channel.n
                ),
            );
            record_send(
                rule.n,
                false,
                Some(format!(
                    "NOTIFY_{}_SECRET is not set in the keychain",
                    channel.n
                )),
            );
            return;
        }
        Err(e) => {
            crate::logging::warn(
                "XK-ALERT",
                &format!(
                    "alert #{} ({}): keychain read failed for NOTIFY_{}_SECRET: {}",
                    rule.n, rule.ticker, channel.n, e
                ),
            );
            record_send(rule.n, false, Some(format!("keychain read failed: {e}")));
            return;
        }
    };

    let notifier = match crate::notify::build_notifier(channel.kind, secret, channel.to.clone()) {
        Ok(n) => n,
        Err(e) => {
            crate::logging::warn(
                "XK-ALERT",
                &format!(
                    "alert #{} ({}): channel secret invalid: {}",
                    rule.n, rule.ticker, e
                ),
            );
            record_send(rule.n, false, Some(format!("channel secret invalid: {e}")));
            return;
        }
    };

    match notifier.send(message, https_proxy, no_proxy).await {
        Ok(()) => {
            crate::logging::info(
                "XK-ALERT",
                &format!(
                    "alert #{} ({}) fired → notify #{} [{}]",
                    rule.n,
                    rule.ticker,
                    channel.n,
                    channel.kind.as_str()
                ),
            );
            record_send(rule.n, true, None);
        }
        Err(e) => {
            crate::logging::warn(
                "XK-ALERT",
                &format!("alert #{} ({}) send failed: {}", rule.n, rule.ticker, e),
            );
            record_send(rule.n, false, Some(e.to_string()));
        }
    }
}

// ── /alert command API (Web chat) ────────────────────────────────────────────

/// Snapshot the current rules and channels for `/alert list`.
pub(crate) fn list() -> (Vec<RuleView>, Vec<ChannelView>) {
    let mon = store().lock().expect("alert store poisoned");
    let rules = mon
        .rules
        .iter()
        .map(|rs| RuleView {
            n: rs.rule.n,
            ticker: rs.rule.ticker.clone(),
            mode: rs
                .rule
                .mode
                .map(|m| m.as_str().to_string())
                .unwrap_or_default(),
            cond: condition_text(&rs.rule.when),
            notify: rs.rule.notify,
            explain: rs.rule.explain,
            active: rs.active,
            last_send_at: rs.last_send.as_ref().map(|o| o.at.clone()),
            last_send_ok: rs.last_send.as_ref().map(|o| o.ok),
            last_send_error: rs.last_send.as_ref().and_then(|o| o.detail.clone()),
        })
        .collect();
    let channels = mon
        .channels
        .iter()
        .map(|c| ChannelView {
            n: c.n,
            kind: c.kind.as_str().to_string(),
            name: c.name.clone(),
            secret_set: matches!(
                crate::keystore::resolve_key_presence(&format!("NOTIFY_{}_SECRET", c.n)),
                crate::keystore::KeyPresence::Found(_)
            ),
        })
        .collect();
    (rules, channels)
}

/// Resolve a channel's `NOTIFY_<n>_NAME` to its number (first match). `/alert add`
/// references a channel by name, matching the documented grammar.
pub(crate) fn channel_number_by_name(name: &str) -> Option<u8> {
    let mon = store().lock().expect("alert store poisoned");
    mon.channels.iter().find(|c| c.name == name).map(|c| c.n)
}

/// Enable or disable rule `n` for this run. Not written to `xoksa.env`: the
/// toggle is a pause, and a rule that stayed disabled across a restart without
/// saying so would silently stop notifying. A restart re-enables it.
pub(crate) fn set_active(n: u8, active: bool) -> Result<(), OpError> {
    let mut mon = store().lock().expect("alert store poisoned");
    let rs = mon
        .rules
        .iter_mut()
        .find(|r| r.rule.n == n)
        .ok_or(OpError::RuleNotFound(n))?;
    rs.active = active;
    // Re-arm so a re-enabled rule can fire again on the next rising edge.
    if active {
        rs.prev_true = false;
        rs.last_eval = None;
    }
    Ok(())
}

/// Remove rule `n` from the running store. Callers pair this with
/// [`persist_remove_from_env`] so the rule does not return on the next start.
pub(crate) fn remove(n: u8) -> Result<(), OpError> {
    let mut mon = store().lock().expect("alert store poisoned");
    let before = mon.rules.len();
    mon.rules.retain(|r| r.rule.n != n);
    if mon.rules.len() == before {
        return Err(OpError::RuleNotFound(n));
    }
    Ok(())
}

/// Add a rule to the running store. Validates the mode is intraday and the
/// target channel exists, assigns the lowest free rule number (1..=ALERT_MAX),
/// and returns it. Callers pair this with [`persist_add_to_env`].
pub(crate) fn add(
    ticker: String,
    when: crate::backtest::Condition,
    mode: AnalysisMode,
    notify: u8,
    explain: bool,
) -> Result<u8, OpError> {
    if !mode.is_intraday() {
        return Err(OpError::NotIntraday);
    }
    let mut mon = store().lock().expect("alert store poisoned");
    if !mon.channels.iter().any(|c| c.n == notify) {
        return Err(OpError::ChannelNotFound(notify));
    }
    let n = (1u8..=crate::config::ALERT_MAX)
        .find(|n| !mon.rules.iter().any(|r| r.rule.n == *n))
        .ok_or(OpError::Full)?;
    mon.rules.push(RuleState::new(AlertRule {
        n,
        ticker,
        mode: Some(mode),
        when,
        notify,
        explain,
    }));
    Ok(n)
}

/// Send a test message to channel `n` right now (validates the channel end-to-end).
pub(crate) async fn send_test(n: u8, lang: &str) -> Result<(), TestError> {
    let (channel, https_proxy, no_proxy) = {
        let mon = store().lock().expect("alert store poisoned");
        let channel = mon
            .channels
            .iter()
            .find(|c| c.n == n)
            .cloned()
            .ok_or(TestError::ChannelNotFound(n))?;
        (channel, mon.https_proxy.clone(), mon.no_proxy.clone())
    };

    let secret = match crate::keystore::get_key(&format!("NOTIFY_{}_SECRET", n)) {
        Ok(Some(s)) => s,
        Ok(None) => return Err(TestError::SecretMissing(n)),
        Err(e) => return Err(TestError::Keychain(e.to_string())),
    };
    let notifier = crate::notify::build_notifier(channel.kind, secret, channel.to.clone())
        .map_err(|e| TestError::Invalid(e.to_string()))?;
    let message = match lang {
        "ja" => "🔔 XOKSA テスト通知：このチャンネル設定は正常です。",
        _ => "🔔 XOKSA test notification: this channel is configured correctly.",
    };
    notifier
        .send(message, https_proxy.as_deref(), no_proxy.as_deref())
        .await
        .map_err(|e| TestError::Send(e.to_string()))
}

/// English text for an `/api/alerts` mutation error (the chat layer localizes its
/// own; the dashboard shows this).
pub(crate) fn op_error_en(e: &OpError) -> String {
    match e {
        OpError::RuleNotFound(n) => format!("No such rule #{n}."),
        OpError::ChannelNotFound(n) => {
            format!("Notification channel #{n} is not defined (set NOTIFY_{n}_KIND in xoksa.env).")
        }
        OpError::NotIntraday => "mode must be an intraday bar (1m|5m|15m|30m|60m).".to_string(),
        OpError::Full => format!(
            "The rule limit ({}) has been reached. Remove a rule to free a slot.",
            crate::config::ALERT_MAX
        ),
    }
}

/// English text for an `/api/alerts` test error.
pub(crate) fn test_error_en(e: &TestError) -> String {
    match e {
        TestError::ChannelNotFound(n) => format!("Notification channel #{n} is not defined."),
        TestError::SecretMissing(n) => {
            format!(
                "NOTIFY_{n}_SECRET is not set (register it in the settings form or --update-key)."
            )
        }
        TestError::Keychain(s) => format!("Keychain read failed: {s}"),
        TestError::Invalid(s) => format!("The channel secret is invalid: {s}"),
        TestError::Send(s) => format!("Send failed: {s}"),
    }
}

/// Whether the loader would read this line as a key of rule `n`.
///
/// Decided with the loader's own parser rather than a prefix test on the raw
/// text, because the two disagree in ways that matter: `parse_env_line` accepts
/// an `export ` prefix, and the sanitizer drops a BOM on the first line. A line
/// the loader reads but the remover does not would survive a delete and come back
/// on the next start — and, on a replace, would keep a stale
/// `ALERT_<n>_EXPLAIN=true` alive under a new rule. A comment parses to nothing
/// and is kept. The trailing `_` is what keeps `ALERT_1_` off `ALERT_10_TICKER`.
///
/// The key is compared case-sensitively because the loader is: `get_env_val`
/// looks up `ALERT_<n>_TICKER` exactly, so a lowercase spelling never defines a
/// rule and must not be removed as if it did.
fn is_rule_line(raw: &str, first_line: bool, prefix: &str) -> bool {
    let line = if first_line {
        raw.strip_prefix('\u{FEFF}').unwrap_or(raw)
    } else {
        raw
    };
    crate::utils::parse_env_line(line).is_some_and(|(k, _)| k.starts_with(prefix))
}

/// The text with every line that defines rule `n` removed.
fn without_rule(text: &str, n: u8) -> Vec<&str> {
    let prefix = format!("ALERT_{}_", n);
    text.lines()
        .enumerate()
        .filter(|(i, line)| !is_rule_line(line, *i == 0, &prefix))
        .map(|(_, line)| line)
        .collect()
}

/// The `ALERT_<n>_*` block for a rule. Control characters are stripped; inputs are
/// already validated by the caller.
fn rule_block(n: u8, ticker: &str, when: &str, mode: &str, notify: u8, explain: bool) -> String {
    let clean = |s: &str| -> String { s.chars().filter(|c| !c.is_control()).take(256).collect() };
    let mut b = String::new();
    b.push_str(&format!("ALERT_{}_TICKER={}\n", n, clean(ticker)));
    b.push_str(&format!("ALERT_{}_WHEN={}\n", n, clean(when)));
    b.push_str(&format!("ALERT_{}_MODE={}\n", n, clean(mode)));
    b.push_str(&format!("ALERT_{}_NOTIFY={}\n", n, notify));
    if explain {
        b.push_str(&format!("ALERT_{}_EXPLAIN=true\n", n));
    }
    b
}

/// Write a rule's `ALERT_<n>_*` keys to `xoksa.env`, replacing whatever occupied
/// that number, so it survives a restart. Both surfaces that create a rule reach
/// this — the chat `/alert add` and the dashboard — so the store and the file stay
/// in step.
///
/// The slot is cleared in the same pass that writes it. `add` picks the lowest
/// number free in the STORE, and the store is not the file: a rule the seed
/// refused (a MODE that is not an intraday bar) leaves its keys behind while
/// freeing the number.
///
/// **`--private` writes nothing.** A no-trace session leaves no file behind, and
/// a rule carries the user's watched ticker and threshold — the rule still runs
/// for this session, it is simply not written to disk.
/// Test entry point for the write path with the lock in place. Production goes
/// through [`add_persisted`], which also holds the lock across the allocation.
#[cfg(test)]
pub(crate) fn persist_add_to_env(
    target: &std::path::Path,
    n: u8,
    ticker: &str,
    when: &str,
    mode: &str,
    notify: u8,
    explain: bool,
) -> std::io::Result<()> {
    let _g = crate::utils::env_write_lock();
    persist_add_locked(target, n, ticker, when, mode, notify, explain)
}

fn persist_add_locked(
    target: &std::path::Path,
    n: u8,
    ticker: &str,
    when: &str,
    mode: &str,
    notify: u8,
    explain: bool,
) -> std::io::Result<()> {
    if crate::private::is_private() {
        return Ok(());
    }
    let block = rule_block(n, ticker, when, mode, notify, explain);
    crate::utils::rewrite_env_file_locked(target, |text| {
        let mut kept = without_rule(text, n);
        while kept.last().is_some_and(|l| l.trim().is_empty()) {
            kept.pop();
        }
        let mut out = kept.join("\n");
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&block);
        out
    })
}

/// Test entry point for the delete path with the lock in place. Production goes
/// through [`remove_persisted`].
#[cfg(test)]
fn persist_remove_from_env_at(target: &std::path::Path, n: u8) -> std::io::Result<()> {
    let _g = crate::utils::env_write_lock();
    persist_remove_locked(target, n)
}

fn persist_remove_locked(target: &std::path::Path, n: u8) -> std::io::Result<()> {
    if crate::private::is_private() {
        return Ok(());
    }
    if !target.exists() {
        return Ok(());
    }
    crate::utils::rewrite_env_file_locked(target, |text| {
        let kept = without_rule(text, n);
        format!("{}\n", kept.join("\n"))
    })
}

/// Add a rule and write it, as one operation. Allocation and the file write are
/// under the same lock, so a number cannot be handed out against a file another
/// write has not landed in yet. The rule is live either way; the inner result says
/// whether it will still be there after a restart.
pub(crate) fn add_persisted(
    ticker: String,
    when: crate::backtest::Condition,
    when_text: &str,
    mode: AnalysisMode,
    notify: u8,
    explain: bool,
) -> Result<(u8, std::io::Result<()>), OpError> {
    let _g = crate::utils::env_write_lock();
    let n = add(ticker.clone(), when, mode, notify, explain)?;
    let saved = persist_add_locked(
        &crate::utils::env_path(),
        n,
        &ticker,
        when_text,
        mode.as_str(),
        notify,
        explain,
    );
    Ok((n, saved))
}

/// Remove a rule and drop it from the file, as one operation. The inner result
/// says whether it will stay gone after a restart.
pub(crate) fn remove_persisted(n: u8) -> Result<std::io::Result<()>, OpError> {
    let _g = crate::utils::env_write_lock();
    remove(n)?;
    Ok(persist_remove_locked(&crate::utils::env_path(), n))
}

// ── pure helpers ─────────────────────────────────────────────────────────────

/// Decide whether this observation should fire: a rising edge (met
/// now, not met on the previous observation) that has not already fired for this
/// bar. The rising edge keeps a condition that stays true from alerting
/// every bar; the per-bar guard blocks a re-poll of the same bar from re-firing.
fn should_fire(met: bool, prev_true: bool, bar_id: i64, last_fired_bar: Option<i64>) -> bool {
    met && !prev_true && last_fired_bar != Some(bar_id)
}

/// Fetch a one-line EXPLAIN note for a firing alert, or `None` when LLM is
/// unavailable or the §1 output-integrity guard drops the reply. The confirmed
/// SOT line is the ONLY factual input, and it carries every number, so the guard
/// has its reference; the note is a labeled, guarded explanation — never a source
/// of new figures (security-design §1/§4).
async fn explain_note(
    config: &crate::config::Config,
    sot_line: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> Option<String> {
    let mut llm = config.clone();
    llm.no_llm = false;
    let constraint = crate::chat::guard::constraint_text(&config.chat_guard, &config.lang);
    let (facts_label, instruction) = if config.lang == "ja" {
        (
            "確定値（数値は変更しないこと）:",
            "上記の確定値のみに基づき、この銘柄の現状を1文・プレーンテキストで簡潔に説明せよ。新しい数値は出さず、売買の指示や具体的な注文価格は述べないこと。",
        )
    } else {
        (
            "Confirmed reading (do not change any number):",
            "In one short plain-text sentence, explain what this reading suggests, based only on the confirmed values above. Introduce no new number, and give no buy/sell instruction or specific order price.",
        )
    };
    let prompt = format!("{facts_label} {sot_line}\n\n{constraint}\n\n{instruction}");
    crate::llm::alert_explain_note(&llm, &prompt, facts).await
}

/// Render a condition back to text for the message and the log (`rsi <= 30`).
/// The operator symbol comes from `rule_operator_catalog` (SOT §4.2) — the same
/// table the backtest/rule UIs use — so the code→symbol mapping lives in one place.
fn condition_text(c: &crate::backtest::Condition) -> String {
    let op = crate::backtest::rule_operator_catalog()
        .iter()
        .find(|(code, _, _)| *code == c.op)
        .map(|(_, _, en)| *en)
        .unwrap_or(c.op.as_str());
    format!("{} {} {}", c.left, op, c.value.unwrap_or_default())
}

/// One-line SOT message: the rule that fired plus the computed values behind it
/// (the triggering indicator's current value, the confirmed close, the score, the
/// bar time). No fabricated figures — every number comes from the guard/map.
fn format_message(
    rule: &AlertRule,
    mode: AnalysisMode,
    guard: &crate::technical::types::TechnicalDataGuard,
    map: &HashMap<&'static str, f64>,
    lang: &str,
) -> String {
    let name = guard.get_name();
    // Show "TICKER (Name)" only when a distinct company name is known; otherwise
    // just the ticker (avoids a redundant "AAPL (AAPL)" when no name is resolved).
    let label = if name.is_empty() || name == rule.ticker {
        rule.ticker.clone()
    } else {
        format!("{} ({})", rule.ticker, name)
    };
    let cond = condition_text(&rule.when);
    let close = guard.get_close();
    let score = map.get("score").copied().unwrap_or(0.0);
    let bar_time = guard
        .get_bar_time()
        .unwrap_or_else(|| guard.get_date())
        .to_string();
    let actual = map
        .get(rule.when.left.as_str())
        .map(|v| format!("{:.2}", v))
        .unwrap_or_else(|| "-".to_string());
    match lang {
        "ja" => {
            format!(
            "🔔 XOKSA アラート  {}  条件 {} 成立（現在 {}={}）  終値 {:.2}  スコア {:.1}  {} [{}]",
            label, cond, rule.when.left, actual, close, score, bar_time, mode.as_str()
        )
        }
        _ => format!(
            "🔔 XOKSA alert  {}  {} met (now {}={})  close {:.2}  score {:.1}  {} [{}]",
            label,
            cond,
            rule.when.left,
            actual,
            close,
            score,
            bar_time,
            mode.as_str()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{condition_text, persist_add_to_env, persist_remove_from_env_at, should_fire};
    use crate::backtest::Condition;

    /// A recorded send outcome has to reach the view the dashboard reads, because
    /// that view is the only place a failed send is visible: the monitor swallows
    /// the failure so one bad channel cannot stop it, and there is no retry. This
    /// pins the wiring from `record_send` through `list`.
    #[test]
    fn a_recorded_send_failure_reaches_the_rule_view() {
        // A number no other test or config uses, removed again at the end — the
        // store is process-global.
        const N: u8 = 99;
        {
            let mut mon = super::store().lock().expect("alert store poisoned");
            mon.rules
                .push(super::RuleState::new(crate::config::AlertRule {
                    n: N,
                    ticker: "TEST".to_string(),
                    mode: Some(crate::config::AnalysisMode::Intraday5m),
                    when: cond("close", "gt", 0.0),
                    notify: 1,
                    explain: false,
                }));
        }
        super::record_send(N, false, Some("channel secret invalid: nope".to_string()));
        let (rules, _) = super::list();
        let row = rules
            .iter()
            .find(|r| r.n == N)
            .expect("the seeded rule must be listed");
        assert_eq!(row.last_send_ok, Some(false));
        assert_eq!(
            row.last_send_error.as_deref(),
            Some("channel secret invalid: nope")
        );
        assert!(
            row.last_send_at.is_some(),
            "a recorded attempt must carry its time"
        );
        super::store()
            .lock()
            .expect("alert store poisoned")
            .rules
            .retain(|r| r.rule.n != N);
    }

    fn cond(left: &str, op: &str, v: f64) -> Condition {
        Condition {
            left: left.to_string(),
            op: op.to_string(),
            right_kind: "value".to_string(),
            value: Some(v),
            right: None,
        }
    }

    // `--private` promises a no-trace session. An alert rule carries the user's
    // watched ticker and threshold, so persisting one would leave exactly the
    // trace the flag rules out. Guards both directions: private writes no file,
    // and the non-private path still writes (so the guard cannot silently
    // disable persistence for everyone).
    #[test]
    fn private_mode_writes_no_alert_rule_to_disk() {
        let _gate = crate::private::test_gate();
        let dir = std::env::temp_dir().join(format!("xoksa-alert-private-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let env_file = dir.join("xoksa.env");
        let _ = std::fs::remove_file(&env_file);

        crate::private::set_private(true);
        persist_add_to_env(&env_file, 1, "AAPL", "rsi<=30", "5m", 1, false)
            .expect("private path is a no-op");
        assert!(!env_file.exists(), "--private must not create xoksa.env");

        crate::private::set_private(false);
        persist_add_to_env(&env_file, 1, "AAPL", "rsi<=30", "5m", 1, false)
            .expect("write succeeds");
        let written = std::fs::read_to_string(&env_file).expect("file written when not private");
        assert!(written.contains("ALERT_1_TICKER=AAPL"));

        let _ = std::fs::remove_file(&env_file);
        let _ = std::fs::remove_dir(&dir);
    }

    // A test's own directory, so the file-level cases cannot collide with each
    // other — they share one process-wide write lock and run in parallel threads.
    fn work_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("xoksa-alert-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("temp dir");
        d
    }

    // What the loader would make of the file, expressed the way the failures are
    // described: which rule numbers survive, and what they say.
    fn reload(path: &std::path::Path) -> Vec<(u8, String, bool)> {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let mut map = std::collections::HashMap::new();
        for (i, raw) in text.lines().enumerate() {
            let line = if i == 0 {
                raw.strip_prefix('\u{FEFF}').unwrap_or(raw)
            } else {
                raw
            };
            if let Some((k, v)) = crate::utils::parse_env_line(line) {
                map.insert(k.to_string(), v.to_string());
            }
        }
        let mut out = Vec::new();
        for n in 1u8..=crate::config::ALERT_MAX {
            if let Some(t) = map.get(&format!("ALERT_{n}_TICKER")) {
                let ex = map
                    .get(&format!("ALERT_{n}_EXPLAIN"))
                    .is_some_and(|v| v.eq_ignore_ascii_case("true"));
                out.push((n, t.clone(), ex));
            }
        }
        out
    }

    // The remover has to see every line the LOADER sees, or a rule survives its own
    // deletion. `parse_env_line` accepts an `export ` prefix and the sanitizer drops
    // a BOM on the first line, so a raw `starts_with` test misses both: the stale
    // EXPLAIN=true outlives a replacement that asked for false, and an export-form
    // rule comes back — with its old ticker — after a delete.
    #[test]
    fn the_remover_reads_a_line_the_way_the_loader_does() {
        let _gate = crate::private::test_gate();
        for (tag, seed) in [
            (
                "export",
                "export ALERT_3_TICKER=OLD\nexport ALERT_3_WHEN=rsi<=20\n\
                 export ALERT_3_MODE=5m\nexport ALERT_3_NOTIFY=1\nexport ALERT_3_EXPLAIN=true\n",
            ),
            (
                "bom",
                "\u{FEFF}ALERT_3_EXPLAIN=true\nALERT_3_TICKER=OLD\nALERT_3_WHEN=rsi<=20\n\
                 ALERT_3_MODE=5m\nALERT_3_NOTIFY=1\n",
            ),
        ] {
            let dir = work_dir(tag);
            let env_file = dir.join("xoksa.env");
            crate::private::set_private(false);
            std::fs::write(&env_file, seed).expect("seed");

            persist_add_to_env(&env_file, 3, "NEW", "rsi<=30", "5m", 1, false).expect("replace");
            assert_eq!(
                reload(&env_file),
                vec![(3u8, "NEW".to_string(), false)],
                "{tag}: the replacement must not inherit the old EXPLAIN"
            );

            persist_remove_from_env_at(&env_file, 3).expect("remove");
            assert!(
                reload(&env_file).is_empty(),
                "{tag}: nothing may survive the delete"
            );
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    // The alert rules and the dashboard's language selector write the same file.
    // A lock that lives inside one of them is not a lock: both writers rewrite the
    // whole file, so the one that finished second used to erase the other's change
    // while reporting success. Reproduced as: alert Ok, language Ok, LANG still on
    // its old value.
    #[test]
    fn an_alert_write_and_a_setting_write_do_not_erase_each_other() {
        let _gate = crate::private::test_gate();
        let dir = work_dir("shared-writer");
        let env_file = dir.join("xoksa.env");
        crate::private::set_private(false);
        std::fs::write(&env_file, "LANG=ja\n").expect("seed");

        // `set_env_value` targets the canonical path, so drive the same shared
        // writer directly — this is the contention the two features are in.
        std::thread::scope(|s| {
            for n in 1u8..=4 {
                let f = env_file.clone();
                s.spawn(move || {
                    persist_add_to_env(&f, n, &format!("T{n}"), "rsi<=30", "5m", 1, false)
                        .expect("alert write Ok")
                });
            }
            for _ in 0..4 {
                let f = env_file.clone();
                s.spawn(move || {
                    crate::utils::rewrite_env_file(&f, |text| {
                        let mut out: Vec<String> = text
                            .lines()
                            .map(|l| {
                                if l.trim_start().starts_with("LANG=") {
                                    "LANG=en".to_string()
                                } else {
                                    l.to_string()
                                }
                            })
                            .collect();
                        if !out.iter().any(|l| l.starts_with("LANG=")) {
                            out.push("LANG=en".to_string());
                        }
                        format!("{}\n", out.join("\n"))
                    })
                    .expect("setting write Ok")
                });
            }
        });

        let text = std::fs::read_to_string(&env_file).expect("read back");
        assert_eq!(reload(&env_file).len(), 4, "every alert write survives");
        assert!(
            text.lines().any(|l| l.trim() == "LANG=en"),
            "and so does the language change: {text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The replacement must not be more open than what it replaced. The temporary
    // file is a NEW file, so without care it takes the process umask (0644 under
    // the common 022) and a config the user kept at 0600 quietly widens on the
    // next rule write.
    #[cfg(unix)]
    #[test]
    fn replacing_the_file_keeps_its_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let _gate = crate::private::test_gate();
        let dir = work_dir("perms");
        let env_file = dir.join("xoksa.env");
        crate::private::set_private(false);
        std::fs::write(&env_file, "LANG=ja\n").expect("seed");
        std::fs::set_permissions(&env_file, std::fs::Permissions::from_mode(0o600)).expect("chmod");

        persist_add_to_env(&env_file, 1, "AAPL", "rsi<=30", "5m", 1, false).expect("add");
        assert_eq!(
            std::fs::metadata(&env_file).unwrap().permissions().mode() & 0o777,
            0o600,
            "an add must not widen the file"
        );

        persist_remove_from_env_at(&env_file, 1).expect("remove");
        assert_eq!(
            std::fs::metadata(&env_file).unwrap().permissions().mode() & 0o777,
            0o600,
            "nor must a delete"
        );

        // A file this process creates starts restricted rather than at the umask.
        let fresh = dir.join("fresh.env");
        persist_add_to_env(&fresh, 1, "AAPL", "rsi<=30", "5m", 1, false).expect("create");
        assert_eq!(
            std::fs::metadata(&fresh).unwrap().permissions().mode() & 0o777,
            0o600,
            "a new config is created restricted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Every write is a read-modify-write of the whole file. Without a lock held from
    // the read to the replace, concurrent writers each build on the same starting
    // text and the last one wins — every caller told Ok, one rule on disk.
    #[test]
    fn concurrent_writes_all_survive() {
        let _gate = crate::private::test_gate();
        let dir = work_dir("concurrent");
        let env_file = dir.join("xoksa.env");
        crate::private::set_private(false);
        std::fs::write(&env_file, "LANG=ja\n").expect("seed");

        std::thread::scope(|s| {
            for n in 1u8..=8 {
                let f = env_file.clone();
                s.spawn(move || {
                    persist_add_to_env(&f, n, &format!("T{n}"), "rsi<=30", "5m", 1, false)
                        .expect("write returned Ok");
                });
            }
        });

        let got = reload(&env_file);
        assert_eq!(got.len(), 8, "every write that returned Ok must be on disk");
        for (i, (n, t, _)) in got.iter().enumerate() {
            assert_eq!(*n, (i + 1) as u8);
            assert_eq!(t, &format!("T{n}"));
        }
        assert!(
            std::fs::read_to_string(&env_file)
                .unwrap()
                .contains("LANG=ja"),
            "unrelated settings survive"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Adds and deletes racing each other: whichever order they land in, the file has
    // to agree with itself — a deleted rule stays gone, a written one stays written.
    #[test]
    fn concurrent_add_and_delete_leave_a_consistent_file() {
        let _gate = crate::private::test_gate();
        let dir = work_dir("mixed");
        let env_file = dir.join("xoksa.env");
        crate::private::set_private(false);
        let seed: String = (1u8..=4)
            .map(|n| {
                format!(
                    "ALERT_{n}_TICKER=SEED{n}\nALERT_{n}_WHEN=rsi<=20\n\
                     ALERT_{n}_MODE=5m\nALERT_{n}_NOTIFY=1\n"
                )
            })
            .collect();
        std::fs::write(&env_file, seed).expect("seed");

        std::thread::scope(|s| {
            for n in 1u8..=4 {
                let f = env_file.clone();
                s.spawn(move || persist_remove_from_env_at(&f, n).expect("remove Ok"));
            }
            for n in 5u8..=8 {
                let f = env_file.clone();
                s.spawn(move || {
                    persist_add_to_env(&f, n, &format!("NEW{n}"), "rsi<=30", "5m", 1, false)
                        .expect("write Ok")
                });
            }
        });

        let got = reload(&env_file);
        assert_eq!(
            got.iter().map(|(n, ..)| *n).collect::<Vec<_>>(),
            vec![5, 6, 7, 8],
            "the four deletes stay deleted and the four adds stay written"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A commented example is not a definition, and ALERT_1_ is not a prefix of
    // ALERT_10_TICKER. Both are easy to break when the matcher is rewritten.
    #[test]
    fn a_comment_and_a_longer_number_are_left_alone() {
        let _gate = crate::private::test_gate();
        let dir = work_dir("neighbours");
        let env_file = dir.join("xoksa.env");
        crate::private::set_private(false);
        std::fs::write(
            &env_file,
            "# ALERT_1_TICKER=commented example\n\
             ALERT_1_TICKER=ONE\nALERT_1_WHEN=rsi<=30\nALERT_1_MODE=5m\nALERT_1_NOTIFY=1\n\
             ALERT_10_TICKER=TEN\nALERT_10_WHEN=rsi<=30\nALERT_10_MODE=5m\nALERT_10_NOTIFY=1\n\
             alert_1_ticker=lowercase is not a definition\n",
        )
        .expect("seed");

        persist_remove_from_env_at(&env_file, 1).expect("remove");
        let t = std::fs::read_to_string(&env_file).expect("read back");
        assert_eq!(
            reload(&env_file),
            vec![(10u8, "TEN".to_string(), false)],
            "only rule 1 goes"
        );
        assert!(t.contains("# ALERT_1_TICKER=commented example"));
        assert!(t.contains("alert_1_ticker=lowercase is not a definition"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // `add` picks the lowest number free in the STORE, which is not the same as
    // free in the FILE: a rule the seed refused (non-intraday MODE) frees the
    // number while leaving its keys on disk. Appending there would state one rule
    // number twice. The loader's last write wins for every key that is re-stated,
    // but EXPLAIN is written only when true — so the stale `true` would outlive
    // the rule that set it and attach an LLM note to its replacement.
    #[test]
    fn writing_a_rule_replaces_that_slot_rather_than_appending_beside_it() {
        let _gate = crate::private::test_gate();
        let dir = std::env::temp_dir().join(format!("xoksa-alert-slot-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let env_file = dir.join("xoksa.env");
        let _ = std::fs::remove_file(&env_file);
        crate::private::set_private(false);

        std::fs::write(
            &env_file,
            "LANG=ja
             ALERT_3_TICKER=OLD
             ALERT_3_WHEN=rsi<=20
             ALERT_3_MODE=1d
             ALERT_3_NOTIFY=1
             ALERT_3_EXPLAIN=true
             # ALERT_3_TICKER=commented example
",
        )
        .expect("seed");

        persist_add_to_env(&env_file, 3, "NEW", "rsi<=30", "5m", 2, false).expect("write");
        let t = std::fs::read_to_string(&env_file).expect("read back");

        assert_eq!(
            t.matches("ALERT_3_TICKER=").count(),
            2,
            "one live row + the comment"
        );
        assert!(t.contains("ALERT_3_TICKER=NEW"));
        assert!(
            !t.contains("ALERT_3_TICKER=OLD"),
            "the stale row must be gone"
        );
        assert!(
            !t.contains("ALERT_3_EXPLAIN"),
            "EXPLAIN=false leaves no key behind"
        );
        assert!(t.contains("ALERT_3_NOTIFY=2"));
        assert!(t.contains("LANG=ja"), "unrelated settings survive");
        assert!(
            t.contains("# ALERT_3_TICKER=commented example"),
            "commented examples are left alone"
        );

        let _ = std::fs::remove_file(&env_file);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn should_fire_only_on_a_fresh_rising_edge() {
        // Rising edge on a new bar → fire.
        assert!(should_fire(true, false, 100, None));
        assert!(should_fire(true, false, 200, Some(100)));
        // Condition stays true across bars → no repeat.
        assert!(!should_fire(true, true, 200, Some(100)));
        // Not met → never fires.
        assert!(!should_fire(false, false, 100, None));
        assert!(!should_fire(false, true, 100, None));
        // Same bar already fired (re-poll) → suppressed.
        assert!(!should_fire(true, false, 100, Some(100)));
    }

    #[test]
    fn condition_text_renders_operator_symbols() {
        assert_eq!(condition_text(&cond("rsi", "le", 30.0)), "rsi <= 30");
        assert_eq!(condition_text(&cond("score", "ge", 4.0)), "score >= 4");
        assert_eq!(condition_text(&cond("adx", "gt", 25.0)), "adx > 25");
        assert_eq!(condition_text(&cond("close", "lt", 100.0)), "close < 100");
    }
}
