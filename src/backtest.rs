//! Minimal backtest skeleton.
//!
//! Bar source = the **provider** (one contiguous history fetch). Bars are always
//! fetched fresh; the backtest keeps no local store of its own.
//!
//! Strategy = the live SOT score: long-only, enter when the composite score ≥
//! `entry_score`, exit when it ≤ `exit_score`. Indicators at each bar are recomputed
//! by the same engine over the bar prefix (identical to the live analysis). Fills
//! are at the signal bar's close — the bar is closed, so the decision is
//! look-ahead-free. Fees are not modelled yet (skeleton).

use crate::config::{AnalysisMode, Config};
use crate::market::MarketData;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct BacktestSpec {
    pub symbol: String,
    #[serde(default)]
    pub timeframe: Option<String>,
    #[serde(default)]
    pub bars: Option<usize>,
    /// Backtest period as a Yahoo `range` token (e.g. "60d", "5y") chosen in the
    /// GUI for the selected timeframe. Must be one of that timeframe's allowed
    /// ranges; otherwise ignored (falls back to the timeframe default).
    #[serde(default)]
    pub period: Option<String>,
    #[serde(default)]
    pub initial_cash: Option<f64>,
    /// Fraction of the starting cash to buy up front (0.0–1.0), before any signal.
    /// Default 0.5 (half). Buying only part at the start leaves room to both add on
    /// buy signals and trim on sell signals — more realistic than all-in/all-out.
    #[serde(default)]
    pub start_fraction: Option<f64>,
    /// Cash amount to buy on each buy signal and to sell on each sell signal (in the
    /// instrument's currency). Whole lots only; leftover stays as cash. Default =
    /// a quarter of the starting cash.
    #[serde(default)]
    pub step_cash: Option<f64>,
    #[serde(default)]
    pub entry_score: Option<f64>,
    #[serde(default)]
    pub exit_score: Option<f64>,
    /// User-defined rule (from the rule editor). When present it replaces the
    /// preset score-threshold strategy. When absent, the preset is used.
    #[serde(default)]
    pub rules: Option<StrategyRules>,
    #[serde(default)]
    pub lang: Option<String>,
    /// Human description of the rule (built by the UI from the user's editor input).
    /// Echoed verbatim into the engine-rendered report — it is an echo of the user's
    /// own input, not analysis, so the UI (which holds the editor state) supplies it.
    #[serde(default)]
    pub rule_desc: Option<String>,
}

/// One condition: `<left> <op> <right>`, where right is a constant or another
/// indicator. Indicator keys: close, rsi, macd, macd_signal, ema_s, ema_l,
/// sma_s, sma_l, bb_u, bb_l, pct_b, adx, stoch_k, vwap, volume, score.
/// Operators: gt, lt, ge, le, cross_up, cross_down.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Condition {
    pub left: String,
    pub op: String,
    /// "value" → use `value`; "indicator" → use `right` as an indicator key.
    #[serde(default)]
    pub right_kind: String,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub right: Option<String>,
}

/// A user-defined long-only strategy: enter when the entry conditions hold, exit
/// when the exit conditions hold. Conditions are combined by AND or OR.
#[derive(Deserialize, Serialize, Clone, Default)]
pub struct StrategyRules {
    /// "and" (default) or "or".
    #[serde(default)]
    pub entry_combine: String,
    #[serde(default)]
    pub entry: Vec<Condition>,
    #[serde(default)]
    pub exit_combine: String,
    #[serde(default)]
    pub exit: Vec<Condition>,
}

#[derive(Serialize, Clone)]
pub struct BacktestTrade {
    pub entry_at: String,
    pub exit_at: String,
    pub entry_price: f64,
    pub exit_price: f64,
    pub quantity: f64,
    pub pnl: f64,
    pub pnl_pct: f64,
}

#[derive(Serialize, Default)]
pub struct BacktestResult {
    pub symbol: String,
    pub timeframe: String,
    /// Currency the cash figures are in (from the provider, e.g. "JPY", "USD").
    /// Empty when the provider didn't report one — the UI then omits a unit rather
    /// than guessing.
    pub currency: String,
    pub bars: usize,
    pub initial_cash: f64,
    pub final_equity: f64,
    pub total_return_pct: f64,
    /// Buy & Hold over the same window (buy at the first usable close, hold to the
    /// last) — the honest benchmark shown next to the strategy so a weak rule can't
    /// be mistaken for the stock's own return.
    pub buy_hold_return_pct: f64,
    pub num_trades: usize,
    pub win_rate_pct: f64,
    pub max_drawdown_pct: f64,
    pub entry_score: f64,
    pub exit_score: f64,
    /// Shares per tradable unit (lot). Assumed 100 for Tokyo (`.T`) issues, 1
    /// otherwise — the provider feed does not carry the exact unit size, so this is
    /// a documented assumption, not a fetched fact.
    pub lot_size: f64,
    /// Fraction of the starting cash bought up front (0.0–1.0). Echoed for display.
    pub start_fraction: f64,
    /// Cash traded per buy/sell signal (instrument currency). Echoed for display.
    pub step_cash: f64,
    pub started_at: String,
    pub ended_at: String,
    pub trades: Vec<BacktestTrade>,
    /// Non-fatal notices (e.g. the starting cash couldn't afford one lot). Unlike
    /// `note`, these do NOT replace the result — they are shown alongside it.
    pub warnings: Vec<String>,
    pub note: String,
    /// The fully-rendered, localized backtest report — the SINGLE source of the
    /// report text (the engine owns the narrative, derived figures, verdict, and
    /// trade ledger; see `render_report`). The Web UI displays these lines verbatim.
    /// Empty on the error path (`note` is shown instead).
    pub report: Vec<String>,
}

/// Per-bar indicator values for rule/score evaluation. Recomputed by the engine
/// over the bar prefix (identical to the live analysis; look-ahead-free).
fn bar_indicators(
    config: &Config,
    slice: &[MarketData],
    name_map: &HashMap<String, String>,
    close: f64,
    volume: Option<f64>,
) -> Option<HashMap<&'static str, f64>> {
    let mut g = crate::technical::build_basic_technical_entry(config, slice, name_map).ok()?;
    // Extension failures are not discarded: an indicator that failed here simply
    // never reaches the map (its setter never ran), and the reason is logged so a
    // silent all-zero evaluation cannot happen unnoticed.
    match crate::technical::evaluate_all_selected_extensions_with_report(config, slice, &mut g) {
        Ok(report) if report.has_failures() => crate::logging::warn(
            "XK-BT-IND",
            &format!(
                "Indicator(s) not computed for this bar; conditions using them are false: {}",
                report.failure_message()
            ),
        ),
        Ok(_) => {}
        Err(e) => crate::logging::warn(
            "XK-BT-IND",
            &format!("All extended indicators failed for this bar: {e}"),
        ),
    }
    Some(guard_indicator_map(config, &g, close, volume))
}

/// Build the rule/score indicator map from an analyzed guard — the SINGLE place
/// that maps indicator keys → values, so the live alert monitor evaluates the
/// exact same snapshot the backtest does (SOT). `close`/`volume` come from the
/// bar in a backtest, or from the guard's confirmed close (with no volume) live.
pub(crate) fn guard_indicator_map(
    config: &Config,
    g: &crate::technical::types::TechnicalDataGuard,
    close: f64,
    volume: Option<f64>,
) -> HashMap<&'static str, f64> {
    let score = crate::technical::calculate_final_score_snapshot(config, g).total_score;
    let mut m: HashMap<&'static str, f64> = HashMap::new();
    m.insert("close", close);
    // Only indicators that were actually computed enter the map. An uncomputed one
    // (too few bars, or a failed evaluator) must not arrive as 0.0 and satisfy
    // `close > ema_l`; a missing key makes its condition false (see `eval_condition`),
    // which leaves the existing AND/OR semantics untouched.
    for key in [
        "rsi",
        "macd",
        "macd_signal",
        "ema_s",
        "ema_l",
        "sma_s",
        "sma_l",
        "bb_u",
        "bb_l",
        "pct_b",
    ] {
        if let Some(v) = g.computed_indicator(key) {
            m.insert(key, v);
        }
    }
    if let Some(v) = g.get_adx() {
        m.insert("adx", v);
    }
    if let Some(v) = g.get_stochastics_k() {
        m.insert("stoch_k", v);
    }
    if let Some(v) = g.get_vwap() {
        m.insert("vwap", v);
    }
    if let Some(v) = volume {
        m.insert("volume", v);
    }
    m.insert("score", score);
    m
}

/// Right-hand side value of a condition: a constant, or another indicator.
fn condition_rhs(cur: &HashMap<&'static str, f64>, c: &Condition) -> Option<f64> {
    if c.right_kind == "indicator" {
        c.right.as_deref().and_then(|k| cur.get(k).copied())
    } else {
        c.value
    }
}

/// Evaluate one condition on the current bar (crossovers also use the previous).
/// A missing indicator (not computed) makes the condition false.
pub(crate) fn eval_condition(
    cur: &HashMap<&'static str, f64>,
    prev: Option<&HashMap<&'static str, f64>>,
    c: &Condition,
) -> bool {
    let (Some(l), Some(r)) = (cur.get(c.left.as_str()).copied(), condition_rhs(cur, c)) else {
        return false;
    };
    match c.op.as_str() {
        "gt" => l > r,
        "lt" => l < r,
        "ge" => l >= r,
        "le" => l <= r,
        "cross_up" | "cross_down" => {
            let Some(p) = prev else { return false };
            let Some(pl) = p.get(c.left.as_str()).copied() else {
                return false;
            };
            let pr = if c.right_kind == "indicator" {
                match c.right.as_deref().and_then(|k| p.get(k).copied()) {
                    Some(v) => v,
                    None => return false,
                }
            } else {
                r
            };
            if c.op == "cross_up" {
                pl <= pr && l > r
            } else {
                pl >= pr && l < r
            }
        }
        _ => false,
    }
}

/// Combine a condition set with AND/OR. Empty set = no signal (false).
pub(crate) fn eval_rules(
    cur: &HashMap<&'static str, f64>,
    prev: Option<&HashMap<&'static str, f64>>,
    conds: &[Condition],
    combine: &str,
) -> bool {
    if conds.is_empty() {
        return false;
    }
    if combine == "or" {
        conds.iter().any(|c| eval_condition(cur, prev, c))
    } else {
        conds.iter().all(|c| eval_condition(cur, prev, c))
    }
}

/// Time label for a bar (datetime if present, else date).
fn bar_label(b: &MarketData) -> String {
    b.datetime
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| b.date.clone())
}

/// Run the skeleton backtest. `base` is the per-symbol config (its enabled
/// extensions/weights define the score, matching the live analysis).
pub async fn run_backtest(
    base: &Config,
    spec: &BacktestSpec,
    name_map: &HashMap<String, String>,
) -> anyhow::Result<BacktestResult> {
    let mut config = base.clone();
    if let Some(tf) = spec.timeframe.as_deref().and_then(AnalysisMode::from_value) {
        config.analysis_mode = tf;
    }
    config.silent = true;
    config.no_news = true;
    config.no_llm = true;

    // Contiguous history from the provider (a single fetch). The period token
    // is validated against this timeframe's allowed ranges so a bad value can't
    // reach the provider (avoids a 422); an unknown value falls back to the
    // timeframe default.
    let allowed = config.analysis_mode.backtest_period_ranges();
    let range_override = spec
        .period
        .as_deref()
        .filter(|p| allowed.contains(p))
        .map(str::to_string);
    let snapshot = crate::market::fetch_market_data_snapshot_for_mode(
        &spec.symbol,
        config.analysis_mode,
        config.https_proxy.as_deref(),
        config.no_proxy.as_deref(),
        range_override.as_deref(),
    )
    .await?;
    let currency = snapshot.currency.clone().unwrap_or_default();
    let mut bars = snapshot.bars;
    bars.sort_by_key(|b| b.timestamp.unwrap_or(0));
    let total = bars.len();

    let tf_label = config.analysis_mode.as_str().to_string();
    let initial_cash = spec.initial_cash.unwrap_or(1_000_000.0).max(1.0);
    let entry_score = spec.entry_score.unwrap_or(2.0);
    let exit_score = spec.exit_score.unwrap_or(-1.0);

    // The fetched range already IS the chosen period, so use all of it by default;
    // `bars` only caps the count (safety bound on per-bar recompute).
    let n = spec.bars.unwrap_or(total).clamp(10, 2000);
    let start = total.saturating_sub(n);
    if total - start < 10 {
        return Ok(BacktestResult {
            symbol: spec.symbol.clone(),
            timeframe: tf_label,
            currency,
            initial_cash,
            entry_score,
            exit_score,
            note: "insufficient bars for a backtest".to_string(),
            ..Default::default()
        });
    }

    // A user-defined rule can reference any indicator, so enable the optional
    // extensions for this run (the preset path keeps the symbol's own config).
    let rules = spec.rules.as_ref().filter(|r| !r.entry.is_empty());
    if rules.is_some() {
        use crate::config::ExtensionIndicator::{Adx, Bollinger, Ema, Sma, Stochastics, Vwap};
        config.enabled_extensions = vec![Ema, Sma, Bollinger, Adx, Stochastics, Vwap];
    }

    // Shares per tradable unit (lot). Tokyo (`.T`) trades in units of 100; most
    // other venues allow single shares. The provider feed does not report the exact
    // unit, so this is a documented assumption (see BacktestResult::lot_size).
    let lot_size = if spec.symbol.to_ascii_uppercase().ends_with(".T") {
        100.0
    } else {
        1.0
    };

    // Position sizing (owner-specified, more realistic than all-in/all-out):
    //   * buy `start_fraction` of the cash up front (default half),
    //   * add/trim `step_cash` worth on each buy/sell signal,
    //   * unused cash just accumulates.
    let start_fraction = spec.start_fraction.unwrap_or(0.5).clamp(0.0, 1.0);
    let step_cash = spec.step_cash.unwrap_or(initial_cash * 0.25).max(0.0);

    // Holdings are kept as FIFO tranches (price, qty, opened_at); a sell consumes
    // them front-to-back, so each realized round-trip is still one BacktestTrade
    // (win-rate and ledger keep the same shape — just more, smaller fills).
    let mut cash = initial_cash;
    let mut lots_open: std::collections::VecDeque<(f64, f64, String)> =
        std::collections::VecDeque::new();
    let held = |q: &std::collections::VecDeque<(f64, f64, String)>| -> f64 {
        q.iter().map(|(_, qty, _)| qty).sum()
    };
    let mut trades: Vec<BacktestTrade> = Vec::new();
    let mut peak = initial_cash;
    let mut max_dd = 0.0f64;
    let mut prev_map: Option<HashMap<&'static str, f64>> = None;
    let mut seeded = false;
    let mut buy_fills = 0usize;
    let mut sell_signals_starved = 0usize; // sell signal but nothing held to sell

    // Buy up to `budget` cash worth of whole lots at `close`; returns units bought.
    let buy_units = |cash: &mut f64, budget: f64, close: f64| -> f64 {
        let affordable = (cash.min(budget) / (close * lot_size)).floor();
        if affordable >= 1.0 {
            let units = affordable * lot_size;
            *cash -= units * close;
            units
        } else {
            0.0
        }
    };

    for i in start..total {
        let close = bars[i].close;
        if !close.is_finite() || close <= 0.0 {
            continue;
        }
        let t = bar_label(&bars[i]);

        // All indicator values (incl. the SOT score) at this closed bar.
        let Some(cur) = bar_indicators(&config, &bars[..=i], name_map, close, bars[i].volume)
        else {
            continue; // not enough data for the basics yet
        };

        // Seed the initial position at the first usable bar (before any signal).
        if !seeded {
            seeded = true;
            let units = buy_units(&mut cash, initial_cash * start_fraction, close);
            if units > 0.0 {
                lots_open.push_back((close, units, t.clone()));
            }
        }

        // Entry/exit: the user's rule if given, else the preset score threshold.
        let want_entry = match rules {
            Some(r) => eval_rules(&cur, prev_map.as_ref(), &r.entry, &r.entry_combine),
            None => cur.get("score").copied().unwrap_or(0.0) >= entry_score,
        };
        let want_exit = match rules {
            Some(r) => eval_rules(&cur, prev_map.as_ref(), &r.exit, &r.exit_combine),
            None => cur.get("score").copied().unwrap_or(0.0) <= exit_score,
        };

        if want_entry {
            // Add `step_cash` worth (whole lots, capped by cash), but at least one
            // lot per signal so a small step still trades. Leftover stays cash.
            let budget = step_cash.max(close * lot_size);
            let units = buy_units(&mut cash, budget, close);
            if units > 0.0 {
                lots_open.push_back((close, units, t.clone()));
                buy_fills += 1;
            }
        } else if want_exit {
            // Trim `step_cash` worth by consuming FIFO tranches into realized trades.
            let mut want_units = (step_cash / (close * lot_size)).floor() * lot_size;
            if want_units < lot_size {
                // step too small for a lot: sell one lot if we hold at least one.
                want_units = lot_size;
            }
            let mut sold_any = false;
            while want_units >= lot_size {
                let Some((buy_px, lot_qty, opened_at)) = lots_open.front().cloned() else {
                    break; // nothing left to sell
                };
                let take = lot_qty.min(want_units);
                trades.push(BacktestTrade {
                    entry_at: opened_at,
                    exit_at: t.clone(),
                    entry_price: buy_px,
                    exit_price: close,
                    quantity: take,
                    pnl: (close - buy_px) * take,
                    pnl_pct: (close / buy_px - 1.0) * 100.0,
                });
                cash += take * close;
                want_units -= take;
                sold_any = true;
                if take >= lot_qty {
                    lots_open.pop_front();
                } else {
                    lots_open.front_mut().unwrap().1 -= take;
                    break; // partial lot consumed; step satisfied
                }
            }
            if !sold_any {
                sell_signals_starved += 1;
            }
        }

        let equity = cash + held(&lots_open) * close;
        peak = peak.max(equity);
        if peak > 0.0 {
            max_dd = max_dd.max((peak - equity) / peak * 100.0);
        }
        prev_map = Some(cur);
    }

    // Close any remaining tranches at the last close (so metrics are realized).
    {
        let last = &bars[total - 1];
        let close = last.close;
        let last_at = bar_label(last);
        while let Some((buy_px, lot_qty, opened_at)) = lots_open.pop_front() {
            trades.push(BacktestTrade {
                entry_at: opened_at,
                exit_at: last_at.clone(),
                entry_price: buy_px,
                exit_price: close,
                quantity: lot_qty,
                pnl: (close - buy_px) * lot_qty,
                pnl_pct: (close / buy_px - 1.0) * 100.0,
            });
            cash += lot_qty * close;
        }
    }

    // Buy & Hold over the same window: first usable close → last usable close.
    // Apples-to-apples with the strategy: same starting cash, same lot rule (buy
    // whole lots at the first close, any remainder stays as cash).
    let bh_entry = bars[start..total]
        .iter()
        .map(|b| b.close)
        .find(|c| c.is_finite() && *c > 0.0);
    let bh_exit = bars[start..total]
        .iter()
        .rev()
        .map(|b| b.close)
        .find(|c| c.is_finite() && *c > 0.0);
    let mut bh_affordable = true;
    let buy_hold_return_pct = match (bh_entry, bh_exit) {
        (Some(e), Some(x)) if e > 0.0 => {
            let lots = (initial_cash / (e * lot_size)).floor();
            if lots >= 1.0 {
                let units = lots * lot_size;
                let bh_final = (initial_cash - units * e) + units * x;
                (bh_final / initial_cash - 1.0) * 100.0
            } else {
                bh_affordable = false;
                0.0
            }
        }
        _ => 0.0,
    };

    // Non-fatal notices shown alongside the result (never hide it via `note`).
    let mut warnings: Vec<String> = Vec::new();
    let ja = spec.lang.as_deref() == Some("ja");
    let unit_price = bh_entry.map(|e| e * lot_size);
    // Never bought anything all run → the cash can't afford even one lot.
    if trades.is_empty() && buy_fills == 0 {
        let up = unit_price.map(|p| format!("{p:.0}")).unwrap_or_default();
        warnings.push(if ja {
            format!(
                "元手 {initial_cash:.0} では最小単元（{lot_size:.0}株／1単元 ≈ {up}）を1単元も買えず、一度も売買できませんでした。元手を増やすか、単元価格の低い銘柄で試してください。"
            )
        } else {
            format!(
                "The starting cash {initial_cash:.0} can't afford even one lot ({lot_size:.0} shares/lot ≈ {up}); nothing traded. Raise the cash, or try a stock with a cheaper lot."
            )
        });
    }
    if sell_signals_starved > 0 {
        warnings.push(if ja {
            format!("売りシグナルが出たが保有がゼロで見送った回数: {sell_signals_starved} 回（初期購入比率を上げると保有を持てます）。")
        } else {
            format!("Sell signals with nothing held (skipped): {sell_signals_starved}. Raise the up-front fraction to hold more.")
        });
    }
    if !bh_affordable {
        let up = unit_price.map(|p| format!("{p:.0}")).unwrap_or_default();
        warnings.push(if ja {
            format!("「持ち続け」も元手では1単元（{lot_size:.0}株 ≈ {up}）を買えないため 0% として扱いました。")
        } else {
            format!("Buy & hold also can't afford one lot ({lot_size:.0} shares ≈ {up}) with this cash, shown as 0%.")
        });
    }

    let final_equity = cash;
    let num_trades = trades.len();
    let wins = trades.iter().filter(|t| t.pnl > 0.0).count();
    let win_rate_pct = if num_trades == 0 {
        0.0
    } else {
        wins as f64 / num_trades as f64 * 100.0
    };

    let mut result = BacktestResult {
        symbol: spec.symbol.clone(),
        timeframe: tf_label,
        currency,
        bars: total - start,
        initial_cash,
        final_equity,
        total_return_pct: (final_equity / initial_cash - 1.0) * 100.0,
        buy_hold_return_pct,
        num_trades,
        win_rate_pct,
        max_drawdown_pct: max_dd,
        entry_score,
        exit_score,
        lot_size,
        start_fraction,
        step_cash,
        started_at: bar_label(&bars[start]),
        ended_at: bar_label(&bars[total - 1]),
        trades,
        warnings,
        note: String::new(),
        report: Vec::new(),
    };
    // The engine owns the report text (SOT). Rendered here so CLI/API/Web all show
    // one narrative; the UI supplies only the rule echo it built from its own state.
    let lang = spec.lang.as_deref().unwrap_or("en");
    result.report = render_report(&result, spec.rule_desc.as_deref().unwrap_or(""), lang);
    Ok(result)
}

// ── Rule vocabulary, templates, period labels, and report rendering ─────────
//
// SINGLE SOURCE OF TRUTH for the backtest UI. The rule editor's indicator/operator
// vocabulary, the starter templates, the period labels, and the entire result
// report are defined/rendered HERE and served to the Web UI, so the browser never
// re-invents them. The evaluator keys (`indicator_value`/`eval_condition`) must
// stay in sync with `rule_indicator_catalog`; `catalog_keys_are_evaluated` guards it.

/// Indicator keys the rule editor offers: `(key, ja label, en label)`. The keys are
/// exactly the ones the evaluator understands.
pub fn rule_indicator_catalog() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        ("close", "終値", "Close"),
        ("rsi", "RSI", "RSI"),
        ("macd", "MACD", "MACD"),
        ("macd_signal", "MACDシグナル", "MACD signal"),
        ("ema_s", "EMA短", "EMA short"),
        ("ema_l", "EMA長", "EMA long"),
        ("sma_s", "SMA短", "SMA short"),
        ("sma_l", "SMA長", "SMA long"),
        ("bb_u", "BB上", "BB upper"),
        ("bb_l", "BB下", "BB lower"),
        ("pct_b", "%b", "%b"),
        ("adx", "ADX", "ADX"),
        ("stoch_k", "ストキャス%K", "Stoch %K"),
        ("vwap", "VWAP", "VWAP"),
        ("volume", "出来高", "Volume"),
        ("score", "総合スコア", "Total score"),
    ]
}

/// Operators the rule editor offers: `(key, ja label, en label)`.
pub fn rule_operator_catalog() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        ("gt", "＞", ">"),
        ("lt", "＜", "<"),
        ("ge", "≧", ">="),
        ("le", "≦", "<="),
        ("cross_up", "上抜け", "cross up"),
        ("cross_down", "下抜け", "cross down"),
    ]
}

/// Built-in starter strategies: `(ja name, en name, rules-JSON)`. Shown in the
/// editor's load list beside the user's saved rules.
///
/// The name is a label, not an identifier: a template is never written to disk
/// (saving uses the name the user typed), so localizing it changes nothing about
/// what is stored. It is the one entry in this module's vocabulary that had no
/// English member, which left the English UI showing Japanese rule names. The
/// `_template` suffix is kept in both languages — the panel's own hint tells the
/// reader that suffix is what marks a built-in.
pub fn rule_templates_catalog() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        (
            "総合スコア(買い≥2/売り≤-1)_template",
            "Total score (buy ≥2 / sell ≤-1)_template",
            r#"{"entry_combine":"and","entry":[{"left":"score","op":"ge","right_kind":"value","value":2}],"exit_combine":"and","exit":[{"left":"score","op":"le","right_kind":"value","value":-1}]}"#,
        ),
        (
            "トレンド追随(EMAクロス)_template",
            "Trend following (EMA cross)_template",
            r#"{"entry_combine":"and","entry":[{"left":"ema_s","op":"cross_up","right_kind":"indicator","right":"ema_l"}],"exit_combine":"and","exit":[{"left":"ema_s","op":"cross_down","right_kind":"indicator","right":"ema_l"}]}"#,
        ),
        (
            "RSI逆張り(30/70)_template",
            "RSI mean reversion (30/70)_template",
            r#"{"entry_combine":"and","entry":[{"left":"rsi","op":"lt","right_kind":"value","value":30}],"exit_combine":"and","exit":[{"left":"rsi","op":"gt","right_kind":"value","value":70}]}"#,
        ),
        (
            "MACDクロス_template",
            "MACD cross_template",
            r#"{"entry_combine":"and","entry":[{"left":"macd","op":"cross_up","right_kind":"indicator","right":"macd_signal"}],"exit_combine":"and","exit":[{"left":"macd","op":"cross_down","right_kind":"indicator","right":"macd_signal"}]}"#,
        ),
    ]
}

/// Localized label for a backtest period token (Yahoo `range`), e.g. `1mo` → `1か月`.
pub fn backtest_period_label(tok: &str, lang: &str) -> String {
    let ja = lang == "ja";
    match tok {
        "7d" => {
            if ja {
                "7日"
            } else {
                "7d"
            }
        }
        "1mo" => {
            if ja {
                "1か月"
            } else {
                "1mo"
            }
        }
        "3mo" => {
            if ja {
                "3か月"
            } else {
                "3mo"
            }
        }
        "6mo" => {
            if ja {
                "6か月"
            } else {
                "6mo"
            }
        }
        "60d" => {
            if ja {
                "60日"
            } else {
                "60d"
            }
        }
        "1y" => {
            if ja {
                "1年"
            } else {
                "1y"
            }
        }
        "2y" => {
            if ja {
                "2年"
            } else {
                "2y"
            }
        }
        "5y" => {
            if ja {
                "5年"
            } else {
                "5y"
            }
        }
        "10y" => {
            if ja {
                "10年"
            } else {
                "10y"
            }
        }
        "max" => {
            if ja {
                "最大"
            } else {
                "max"
            }
        }
        other => other,
    }
    .to_string()
}

/// Group an amount's integer part with thousands separators (decimals dropped).
/// Amounts here are non-negative (equity/cash; no shorting), so rounding then
/// grouping matches the previous output (SOT §4.2 — shared with utils).
fn grp0(v: f64) -> String {
    crate::utils::group_thousands(v.round() as i64)
}

/// Format a money amount with its currency code, e.g. `1,000,000 JPY`. The code comes
/// from the provider (never guessed); when absent, the number is shown with no unit.
fn money(v: f64, currency: &str) -> String {
    if currency.is_empty() {
        grp0(v)
    } else {
        format!("{} {}", grp0(v), currency)
    }
}

/// Render the per-trade cash ledger: for each round-trip, the buy amount and the
/// sell amount (so compounding is visible). Capped so an active rule doesn't flood.
fn render_ledger(r: &BacktestResult, lang: &str) -> Vec<String> {
    const MAX_ROWS: usize = 12;
    let ja = lang == "ja";
    let mut out: Vec<String> = Vec::new();
    for (i, t) in r.trades.iter().take(MAX_ROWS).enumerate() {
        let buy_amt = t.entry_price * t.quantity;
        let sell_amt = t.exit_price * t.quantity;
        let d_in = t.entry_at.get(..10).unwrap_or(&t.entry_at);
        let d_out = t.exit_at.get(..10).unwrap_or(&t.exit_at);
        if ja {
            out.push(format!(
                "　#{} {} 買い {} → {} 売り {}（{:+.1}%）",
                i + 1,
                d_in,
                money(buy_amt, &r.currency),
                d_out,
                money(sell_amt, &r.currency),
                t.pnl_pct
            ));
        } else {
            out.push(format!(
                "  #{} {} buy {} -> {} sell {} ({:+.1}%)",
                i + 1,
                d_in,
                money(buy_amt, &r.currency),
                d_out,
                money(sell_amt, &r.currency),
                t.pnl_pct
            ));
        }
    }
    if r.trades.len() > MAX_ROWS {
        let more = r.trades.len() - MAX_ROWS;
        out.push(if ja {
            format!("　… ほか {more} 件（省略）")
        } else {
            format!("  … and {more} more (omitted)")
        });
    }
    out
}

/// Render the full localized backtest report — the ONE source of the report text.
/// `rule_desc` is the UI-built echo of the user's rule (passed through, not analysis).
pub fn render_report(r: &BacktestResult, rule_desc: &str, lang: &str) -> Vec<String> {
    let d0 = r.started_at.get(..10).unwrap_or(&r.started_at);
    let d1 = r.ended_at.get(..10).unwrap_or(&r.ended_at);
    let bh_eq = r.initial_cash * (1.0 + r.buy_hold_return_pct / 100.0);
    let wins = (r.num_trades as f64 * r.win_rate_pct / 100.0).round() as i64;
    let vs = r.total_return_pct - r.buy_hold_return_pct;
    let cash = money(r.initial_cash, &r.currency);
    let equity = money(r.final_equity, &r.currency);
    let bh_eq_s = money(bh_eq, &r.currency);
    let start_pct = r.start_fraction * 100.0;
    let step_s = money(r.step_cash, &r.currency);
    let ledger = render_ledger(r, lang);
    let mut lines: Vec<String> = if lang == "ja" {
        let verdict = if vs < 0.0 {
            format!(
                "持ち続けた方が {:.1}pt 良かった（このルールは上昇を取り逃した）",
                -vs
            )
        } else {
            format!("このルールが持ち続けを {:.1}pt 上回った", vs)
        };
        let mut v = vec![
            format!("🧪 バックテスト：{}（{}・{}〜{}／{}本）", r.symbol, r.timeframe, d0, d1, r.bars),
            String::new(),
            "■ 試したルール".to_string(),
            rule_desc.to_string(),
            String::new(),
            "■ お金の動かし方（このテストの前提）".to_string(),
            format!("　・元手＝現金 {} でスタート（株数ではなくお金。単位はこの銘柄の通貨）。", cash),
            format!("　・開始時に元手の {:.0}% 相当を買っておく（残りは現金として保持）。", start_pct),
            format!("　・「買い」シグナルで {} 相当を買い増し、「売り」シグナルで {} 相当を売る（保有している範囲まで）。使い切れない分は現金のまま。", step_s, step_s),
            "　・空売りなし／その足の終値で約定／手数料・税は未計上。売却は古い買いから順に充当（先入先出）。".to_string(),
            format!("　・売買は最小単元（1単元＝{:.0}株）単位。端数は買わず、余りは現金として残す（単元株数は仮定値）。", r.lot_size),
            "　・最後まで保有が残れば、最終足の終値で手仕舞いして計算。".to_string(),
            String::new(),
            format!("■ 結果（元手＝現金 {}）", cash),
            format!("　このルールで売買 ： {}（{:+.2}%）", equity, r.total_return_pct),
            format!("　ただ買って持ち続け： {}（{:+.2}%）", bh_eq_s, r.buy_hold_return_pct),
            format!("　→ {}", verdict),
            String::new(),
            "■ 売買の中身".to_string(),
            format!(
                "　売買 {}回／勝ち {}回（勝率 {:.0}%）／保有中の最大の下落（高値からの落ち込み） -{:.1}%",
                r.num_trades, wins, r.win_rate_pct, r.max_drawdown_pct
            ),
        ];
        if !ledger.is_empty() {
            v.push(String::new());
            v.push("■ 売買の記録（いくらで買って・いくらで売ったか）".to_string());
            v.extend(ledger);
        }
        v
    } else {
        let verdict = if vs < 0.0 {
            format!(
                "holding beat it by {:.1}pt — this rule missed the rally",
                -vs
            )
        } else {
            format!("this rule beat holding by {:.1}pt", vs)
        };
        let mut v = vec![
            format!("🧪 Backtest: {} ({} · {}-{} / {} bars)", r.symbol, r.timeframe, d0, d1, r.bars),
            String::new(),
            "The rule tested:".to_string(),
            rule_desc.to_string(),
            String::new(),
            "How the money moves (the test's assumptions):".to_string(),
            format!("  - Start with {} of CASH (a money amount, not a share count; unit is this stock's currency).", cash),
            format!("  - At the start, buy {:.0}% of the cash up front (the rest stays as cash).", start_pct),
            format!("  - On a BUY signal add {} more; on a SELL signal sell {} (up to what's held). Anything unusable stays as cash.", step_s, step_s),
            "  - No shorting, fill at bar close, no fees or taxes. Sells consume the oldest buys first (FIFO).".to_string(),
            format!("  - Trades in whole lots ({:.0} shares/lot); no fractional shares — the leftover stays as cash (lot size is an assumption).", r.lot_size),
            "  - Any position still held at the end is closed at the last bar's close for the final figure.".to_string(),
            String::new(),
            format!("Result (starting cash {})", cash),
            format!("  This rule: {} ({:+.2}%)", equity, r.total_return_pct),
            format!("  Just buy & hold: {} ({:+.2}%)", bh_eq_s, r.buy_hold_return_pct),
            format!("  -> {}", verdict),
            String::new(),
            "Trades:".to_string(),
            format!(
                "  {} trades / {} winners ({:.0}% win) / worst drop while holding (from peak): -{:.1}%",
                r.num_trades, wins, r.win_rate_pct, r.max_drawdown_pct
            ),
        ];
        if !ledger.is_empty() {
            v.push(String::new());
            v.push("Trade log (bought for → sold for):".to_string());
            v.extend(ledger);
        }
        v
    };
    if !r.warnings.is_empty() {
        lines.push(String::new());
        lines.push(if lang == "ja" {
            "■ 注意".to_string()
        } else {
            "Notes:".to_string()
        });
        for w in &r.warnings {
            lines.push(format!("　⚠️ {w}"));
        }
    }
    lines
}

#[cfg(test)]
mod rule_tests {
    use super::{eval_condition, eval_rules, Condition};
    use std::collections::HashMap;

    fn cond(left: &str, op: &str, v: f64) -> Condition {
        Condition {
            left: left.to_string(),
            op: op.to_string(),
            right_kind: "value".to_string(),
            value: Some(v),
            right: None,
        }
    }
    fn cond_ind(left: &str, op: &str, right: &str) -> Condition {
        Condition {
            left: left.to_string(),
            op: op.to_string(),
            right_kind: "indicator".to_string(),
            value: None,
            right: Some(right.to_string()),
        }
    }

    #[test]
    fn value_comparisons() {
        let mut m: HashMap<&'static str, f64> = HashMap::new();
        m.insert("rsi", 25.0);
        assert!(eval_condition(&m, None, &cond("rsi", "lt", 30.0)));
        assert!(!eval_condition(&m, None, &cond("rsi", "gt", 30.0)));
        assert!(eval_condition(&m, None, &cond("rsi", "le", 25.0)));
        // missing indicator → false
        assert!(!eval_condition(&m, None, &cond("adx", "gt", 20.0)));
    }

    #[test]
    fn crossover_needs_prev() {
        let mut prev: HashMap<&'static str, f64> = HashMap::new();
        prev.insert("macd", -0.2);
        prev.insert("macd_signal", -0.1);
        let mut cur: HashMap<&'static str, f64> = HashMap::new();
        cur.insert("macd", 0.1);
        cur.insert("macd_signal", -0.05);
        // macd was below signal, now above → cross_up true
        let c = cond_ind("macd", "cross_up", "macd_signal");
        assert!(eval_condition(&cur, Some(&prev), &c));
        // no prev → false
        assert!(!eval_condition(&cur, None, &c));
        // cross_down false here
        assert!(!eval_condition(
            &cur,
            Some(&prev),
            &cond_ind("macd", "cross_down", "macd_signal")
        ));
    }

    #[test]
    fn combine_and_or() {
        let mut m: HashMap<&'static str, f64> = HashMap::new();
        m.insert("rsi", 25.0);
        m.insert("adx", 30.0);
        let conds = vec![cond("rsi", "lt", 30.0), cond("adx", "gt", 40.0)];
        assert!(!eval_rules(&m, None, &conds, "and")); // adx fails
        assert!(eval_rules(&m, None, &conds, "or")); // rsi passes
        assert!(!eval_rules(&m, None, &[], "and")); // empty = no signal
    }
}

#[cfg(test)]
mod sot_gate_tests {
    use super::rule_indicator_catalog;
    use std::collections::BTreeSet;

    /// **SOT gate.** The indicator keys the rule editor is offered (`rule_indicator_catalog`,
    /// served to the Web UI) MUST equal the keys the evaluator's per-bar map
    /// (`bar_indicators`) inserts — otherwise the editor could offer a key that the
    /// engine silently evaluates to `false`. Kept in lockstep; if `bar_indicators`
    /// gains/loses a key, update the catalog AND this list together.
    #[test]
    fn indicator_catalog_matches_evaluator_keys() {
        let catalog: BTreeSet<&str> = rule_indicator_catalog()
            .iter()
            .map(|(k, _, _)| *k)
            .collect();
        let evaluator: BTreeSet<&str> = [
            "close",
            "rsi",
            "macd",
            "macd_signal",
            "ema_s",
            "ema_l",
            "sma_s",
            "sma_l",
            "bb_u",
            "bb_l",
            "pct_b",
            "adx",
            "stoch_k",
            "vwap",
            "volume",
            "score",
        ]
        .into_iter()
        .collect();
        assert_eq!(
            catalog, evaluator,
            "SOT: rule indicator catalog and the evaluator's bar_indicators keys diverged."
        );
    }
}

#[cfg(test)]
mod uncomputed_indicator_tests {
    //! R1: an indicator that could not be computed must not enter rule evaluation
    //! as a valid zero. These tests drive the real technical pipeline, not a
    //! hand-built map, so a regression anywhere between calculation and the
    //! rule/alert map is caught.
    use super::{eval_condition, guard_indicator_map, Condition};
    use crate::market::MarketData;

    fn bars(n: usize) -> Vec<MarketData> {
        (0..n)
            .map(|i| MarketData {
                date: format!("2026-01-{:02}", i + 1),
                datetime: None,
                timestamp: None,
                timezone: None,
                high: 110.0 + i as f64,
                low: 90.0 + i as f64,
                close: 100.0 + i as f64,
                volume: Some(1_000.0),
                name: None,
            })
            .collect()
    }

    fn cond_ind(left: &str, op: &str, right: &str) -> Condition {
        Condition {
            left: left.to_string(),
            op: op.to_string(),
            right_kind: "indicator".to_string(),
            value: None,
            right: Some(right.to_string()),
        }
    }

    fn map_for(bar_count: usize) -> std::collections::HashMap<&'static str, f64> {
        let mut config = crate::config::Config::default();
        config.ticker = "TEST".into();
        config.ema_long_period = 20;
        config.sma_long_period = 20;
        config.enabled_extensions = vec![
            crate::config::ExtensionIndicator::Ema,
            crate::config::ExtensionIndicator::Sma,
            crate::config::ExtensionIndicator::Bollinger,
        ];
        let data = bars(bar_count);
        let names = std::collections::HashMap::new();
        let mut g = crate::technical::build_basic_technical_entry(&config, &data, &names).unwrap();
        let _ =
            crate::technical::evaluate_all_selected_extensions_with_report(&config, &data, &mut g);
        let close = data[data.len() - 1].close;
        guard_indicator_map(&config, &g, close, Some(1_000.0))
    }

    #[test]
    fn too_few_bars_leaves_long_averages_uncomputed() {
        // 10 bars cannot produce a 20-period EMA/SMA or Bollinger band.
        let m = map_for(10);
        assert!(!m.contains_key("ema_l"), "ema_l must not be present");
        assert!(!m.contains_key("sma_l"), "sma_l must not be present");
        assert!(!m.contains_key("bb_u"), "bb_u must not be present");
        // The reported defect: `close > ema_l` was true because ema_l was 0.0.
        assert!(
            !eval_condition(&m, None, &cond_ind("close", "gt", "ema_l")),
            "close > ema_l must not hold when EMA was never computed"
        );
        assert!(
            !eval_condition(&m, None, &cond_ind("close", "lt", "ema_l")),
            "the reverse must not hold either"
        );
        // RSI/MACD are computed from the basic entry and stay usable.
        assert!(m.contains_key("rsi") && m.contains_key("macd"));
    }

    #[test]
    fn enough_bars_computes_and_evaluates() {
        let m = map_for(60);
        let ema_l = *m.get("ema_l").expect("ema_l computed with 60 bars");
        assert!(ema_l.is_finite() && ema_l > 0.0);
        assert!(eval_condition(&m, None, &cond_ind("close", "gt", "ema_l")));
    }

    #[test]
    fn a_genuine_zero_is_kept() {
        // A setter called with 0.0 marks the indicator computed: a real zero is
        // not treated as missing.
        let mut g = crate::technical::types::TechnicalDataGuard::new("T".into(), "d".into());
        g.set_ema_long(0.0);
        assert_eq!(g.computed_indicator("ema_l"), Some(0.0));
        // A non-finite result is refused: it is a failure, not a value.
        g.set_sma_long(f64::NAN);
        assert_eq!(g.computed_indicator("sma_l"), None);
        g.set_bb_upper(f64::INFINITY);
        assert_eq!(g.computed_indicator("bb_u"), None);
    }

    #[test]
    fn crossover_with_uncomputed_side_is_false() {
        let cur = map_for(10);
        let prev = map_for(9);
        assert!(!eval_condition(
            &cur,
            Some(&prev),
            &cond_ind("close", "cross_up", "ema_l")
        ));
        assert!(!eval_condition(
            &cur,
            Some(&prev),
            &cond_ind("ema_s", "cross_up", "ema_l")
        ));
    }
}

#[cfg(test)]
mod uncomputed_to_trade_tests {
    //! R1: the failure must stay false all the way to the trade/notification
    //! decision, not just in a single comparison. This walks bars exactly as the
    //! simulation loop does (same `bar_indicators`, same `eval_rules`), offline.
    use super::{bar_indicators, eval_rules, Condition, StrategyRules};
    use crate::market::MarketData;

    fn bars(n: usize) -> Vec<MarketData> {
        (0..n)
            .map(|i| MarketData {
                date: format!("2026-02-{:02}", i + 1),
                datetime: None,
                timestamp: Some(i as i64),
                timezone: None,
                high: 110.0 + i as f64,
                low: 90.0 + i as f64,
                close: 100.0 + i as f64,
                volume: Some(1_000.0),
                name: None,
            })
            .collect()
    }

    fn rules_on_ema_long() -> StrategyRules {
        let c = |op: &str| Condition {
            left: "close".into(),
            op: op.into(),
            right_kind: "indicator".into(),
            value: None,
            right: Some("ema_l".into()),
        };
        StrategyRules {
            entry_combine: "and".into(),
            entry: vec![c("gt")],
            exit_combine: "and".into(),
            exit: vec![c("lt")],
        }
    }

    #[test]
    fn uncomputed_indicator_never_fires_a_signal() {
        let mut config = crate::config::Config::default();
        config.ticker = "TEST".into();
        config.ema_long_period = 50; // never satisfied by 30 bars
        config.enabled_extensions = vec![crate::config::ExtensionIndicator::Ema];
        let data = bars(30);
        let names = std::collections::HashMap::new();
        let rules = rules_on_ema_long();

        let mut prev: Option<std::collections::HashMap<&'static str, f64>> = None;
        let (mut entries, mut exits) = (0usize, 0usize);
        for i in 2..data.len() {
            let close = data[i].close;
            let Some(cur) = bar_indicators(&config, &data[..=i], &names, close, data[i].volume)
            else {
                continue;
            };
            if eval_rules(&cur, prev.as_ref(), &rules.entry, &rules.entry_combine) {
                entries += 1;
            }
            if eval_rules(&cur, prev.as_ref(), &rules.exit, &rules.exit_combine) {
                exits += 1;
            }
            prev = Some(cur);
        }
        // The seed purchase at the first usable bar is by design and is not a
        // signal; what must never happen is a *signal-driven* buy or sell built
        // on an indicator that was never computed.
        assert_eq!(
            entries, 0,
            "no entry signal may come from an uncomputed EMA"
        );
        assert_eq!(exits, 0, "no exit signal may come from an uncomputed EMA");
    }

    #[test]
    fn computed_indicator_still_fires() {
        let mut config = crate::config::Config::default();
        config.ticker = "TEST".into();
        config.ema_long_period = 10;
        config.enabled_extensions = vec![crate::config::ExtensionIndicator::Ema];
        let data = bars(40);
        let names = std::collections::HashMap::new();
        let rules = rules_on_ema_long();
        let mut fired = 0usize;
        for i in 2..data.len() {
            let close = data[i].close;
            if let Some(cur) = bar_indicators(&config, &data[..=i], &names, close, data[i].volume) {
                if eval_rules(&cur, None, &rules.entry, &rules.entry_combine) {
                    fired += 1;
                }
            }
        }
        assert!(fired > 0, "a computed EMA must still drive signals");
    }
}
