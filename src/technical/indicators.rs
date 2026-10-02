//! Technical indicator calculations and evaluation

use super::types::TechnicalDataGuard;
use crate::config::Config;
use crate::market::MarketData;
use anyhow::{anyhow, bail, Result};
use std::collections::HashMap;
use ta::indicators::BollingerBands;
use ta::indicators::{MovingAverageConvergenceDivergence, RelativeStrengthIndex};
use ta::Next;

pub type ExtensionEvaluator = fn(&Config, &[MarketData], &mut TechnicalDataGuard) -> Result<()>;

// ── Score classification thresholds ──────────────────────────────────────────
//
// The four indicators whose score reads a *separation* — the two legs of a moving
// average, the Ichimoku pair, the close against VWAP — are scored on a
// **percentage deviation**, never on an amount of money. An absolute threshold
// cannot mean the same thing on a JPY 8,500 stock and a USD 13 one: measured on
// daily bars 2026-09-24 across 20 instruments, the shared 2.0 / 0.5 price
// difference gave every Japanese listing +-2 on Ichimoku whatever its actual
// separation, while a US listing sat at 0 — 1605.T at 0.16% scored -2 and F at
// 1.89% scored 0. The score was not monotonic in the distance it claimed to read.
//
// Each indicator carries its own pair, because they measure structurally
// different separations. EMA and SMA both run 5/20 periods, but the simple
// average lags further behind in a trend, and Ichimoku's 9/26 is wider still:
// the measured median |deviation| was EMA 1.03%, SMA 1.60%, Ichimoku 2.47%. The
// pairs below put a comparable share of that sample in each of the neutral /
// slight / strong bands instead of collapsing into one of them — EMA 6/8/6,
// SMA 6/8/6, Ichimoku 7/8/5 of 20, and VWAP 4/9/3 of 16. A one-day snapshot
// fixes the order of magnitude, which is what the old constants had wrong; it is
// not a claim about the long-run distribution.
//
// Each rate is the same figure the display already prints for that indicator, so
// a reading and its score cannot disagree.
const EMA_STRONG_DEV_PCT: f64 = 2.0;
const EMA_WEAK_DEV_PCT: f64 = 0.5;
const SMA_STRONG_DEV_PCT: f64 = 3.0;
const SMA_WEAK_DEV_PCT: f64 = 1.0;
const ICHIMOKU_STRONG_DEV_PCT: f64 = 4.0;
const ICHIMOKU_WEAK_DEV_PCT: f64 = 1.0;
const VWAP_STRONG_DEV_PCT: f64 = 3.0;
const VWAP_WEAK_DEV_PCT: f64 = 1.0;
const ROC_STRONG_THRESHOLD: f64 = 10.0;
const ROC_WEAK_THRESHOLD: f64 = 3.0;
const ADX_VERY_STRONG: f64 = 50.0;
const ADX_STRONG: f64 = 30.0;
const ADX_NEUTRAL: f64 = 20.0;
const ADX_WEAK: f64 = 10.0;
const STOCH_OVERBOUGHT_STRONG: f64 = 90.0;
const STOCH_OVERBOUGHT: f64 = 80.0;
const STOCH_OVERSOLD: f64 = 20.0;
const STOCH_OVERSOLD_STRONG: f64 = 10.0;
const BOLL_OUTSIDE_UPPER_FACTOR: f64 = 1.02;
const BOLL_OUTSIDE_LOWER_FACTOR: f64 = 0.98;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionEvaluationFailure {
    pub indicator: crate::config::ExtensionIndicator,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtensionEvaluationReport {
    pub attempted: usize,
    pub succeeded: usize,
    pub failures: Vec<ExtensionEvaluationFailure>,
}

impl ExtensionEvaluationReport {
    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }

    pub fn failure_message(&self) -> String {
        self.failures
            .iter()
            .map(|failure| format!("{:?}: {}", failure.indicator, failure.message))
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

/// Formal names for tickers the provider reports no name for. Used only after the
/// alias CSV and the provider's own name, so it never overrides what a source said.
///
/// Each entry is the fund's **own** name. The index it tracks used to be appended
/// in brackets — `Invesco QQQ Trust (NASDAQ100)` — which reads as though the fund
/// were the index; those brackets are gone. A `FANG+` entry was removed too: it
/// claimed "NYSE FANG+ Index" for a ticker that can never reach here, because
/// `normalize_ticker_input` rewrote `FANG+` before this point and `sanitize_ticker`
/// does not admit `+` in a ticker at all.
///
/// プロバイダが名前を返さないティッカーの正式名。エイリアス CSV とプロバイダ報告名の
/// 後にしか使わないので、情報源が述べた名前を上書きすることはない。
///
/// 各項目はファンド**自身**の名前である。以前は連動先の指数を括弧で添えていた
/// （`Invesco QQQ Trust (NASDAQ100)`）が、これはファンドが指数そのものだと読めるので
/// 削除した。`FANG+` の項目も削除した——ここへ到達し得ないティッカーに「NYSE FANG+ Index」
/// と名乗らせていた（`normalize_ticker_input` が手前で書き換えており、そもそも
/// `sanitize_ticker` はティッカーに `+` を認めない）。
struct HardcodedInfo {
    formal_name: &'static str,
}

fn resolve_hardcoded_info(ticker: &str) -> Option<HardcodedInfo> {
    match ticker {
        "QQQ" => Some(HardcodedInfo {
            formal_name: "Invesco QQQ Trust",
        }),
        "SPY" => Some(HardcodedInfo {
            formal_name: "SPDR S&P 500 ETF Trust",
        }),
        "ACWI" => Some(HardcodedInfo {
            formal_name: "iShares MSCI ACWI ETF",
        }),
        _ => None,
    }
}

/// Calculates RSI/MACD, assembles diff and score from the previous bar, and returns a Guard
pub fn build_basic_technical_entry(
    config: &Config,
    data: &[MarketData],
    ticker_name_map: &HashMap<String, String>,
) -> Result<TechnicalDataGuard> {
    if data.len() < 2 {
        bail!("❌ Time series has fewer than 2 bars; cannot build technical indicators.");
    }

    let hardcoded = resolve_hardcoded_info(&config.ticker);

    let latest = &data[data.len() - 1];
    let previous = &data[data.len() - 2];

    let alias_name_opt = if config.no_alias {
        None
    } else {
        crate::bootstrap::jp_code_from_ticker(&config.ticker)
            .and_then(|code| ticker_name_map.get(&code).cloned())
    };

    let name = alias_name_opt
        .or(latest.name.clone())
        .or_else(|| hardcoded.map(|h| h.formal_name.to_string()))
        .unwrap_or_else(|| config.ticker.clone());

    let diff = latest.close - previous.close;
    let diff_percent = if previous.close != 0.0 {
        diff / previous.close * 100.0
    } else {
        0.0
    };

    let closes: Vec<f64> = data.iter().map(|d| d.close).collect();

    let mut rsi_calc =
        RelativeStrengthIndex::new(14).map_err(|e| anyhow!("❌ RSI initialization failed: {e}"))?;
    let rsi = closes
        .iter()
        .cloned()
        .map(|close_value| rsi_calc.next(close_value))
        .last()
        .ok_or_else(|| anyhow!("❌ RSI calculation failed"))?;

    let mut macd_calc = MovingAverageConvergenceDivergence::new(12, 26, 9)
        .map_err(|e| anyhow!("❌ MACD initialization failed: {e}"))?;

    let mut prev_macd: f64 = 0.0;
    let mut prev_signal: f64 = 0.0;
    let mut macd: f64 = 0.0;
    let mut signal: f64 = 0.0;

    for (index, close_value) in closes.iter().cloned().enumerate() {
        let out = macd_calc.next(close_value);
        if index == closes.len() - 2 {
            prev_macd = out.macd;
            prev_signal = out.signal;
        }
        if index == closes.len() - 1 {
            macd = out.macd;
            signal = out.signal;
        }
    }

    let macd_diff = (macd - signal).abs();
    let macd_up = macd > signal && (macd > 0.0 || config.macd_minus_ok);
    let macd_down = macd < signal;
    let rsi_low = rsi <= config.buy_rsi;
    let rsi_high = rsi >= config.sell_rsi;

    // MACD rising with extreme divergence while RSI overbought → strong reversal risk
    let macd_up_rsi_high_extreme = macd_up && rsi_high && macd_diff > config.macd_diff_extreme;
    // MACD rising while RSI overbought → overbought despite uptrend
    let macd_up_rsi_high = macd_up && rsi_high;
    // MACD falling while RSI overbought → sell signal from both
    let macd_down_rsi_high = macd_down && rsi_high;
    // MACD rising while RSI oversold → strong buy: uptrend starting from oversold
    let macd_up_rsi_low = macd_up && rsi_low;
    // MACD falling while RSI oversold → weak buy: falling but oversold may bounce
    let macd_down_rsi_low = macd_down && rsi_low;
    // MACD rising, RSI neutral: score by MACD momentum magnitude
    let macd_up_neutral = macd_up && !rsi_high && !rsi_low;
    // MACD falling, RSI neutral: score by MACD momentum magnitude
    let macd_down_neutral = macd_down && !rsi_high && !rsi_low;

    let mut signal_score: f64 = if macd_up_rsi_high_extreme {
        -2.0 // MACD up but RSI extremely overbought with large divergence
    } else if macd_up_rsi_high || macd_down_rsi_high {
        -1.0 // RSI overbought: uptrend exhaustion or aligned sell pressure
    } else if macd_up_rsi_low {
        2.0 // Strong buy: uptrend + oversold
    } else if macd_down_rsi_low || (macd_up_neutral && macd_diff < config.macd_diff_mid) {
        1.0 // Weak buy: oversold recovery or positive MACD below mid threshold
    } else if macd_up_neutral {
        2.0 // MACD up at or above mid threshold — momentum boosted
    } else if macd_down_neutral && macd_diff < config.macd_diff_low {
        0.0 // MACD falling but small momentum; near neutral
    } else if macd_down_neutral {
        -1.0 // MACD falling with meaningful momentum
    } else {
        0.0 // Neutral: no clear signal
    };

    if !config.macd_minus_ok && macd < 0.0 && signal_score > 0.0 {
        signal_score = 0.0;
    }

    let mut guard = TechnicalDataGuard::new(config.ticker.clone(), latest.date.clone());

    guard.set_name(&name);
    if let Some(tz) = latest.timezone.as_deref() {
        guard.set_timezone(tz);
    }
    if let Some(dt) = latest.datetime.as_deref() {
        guard.set_datetime(dt);
    }
    if let Some(ts) = latest.timestamp {
        guard.set_timestamp(ts);
    }

    guard.set_close(latest.close);
    guard.set_previous_close(previous.close);
    guard.set_price_diff(diff);
    guard.set_price_diff_percent(diff_percent);
    guard.set_rsi(rsi);
    guard.set_macd(macd);
    guard.set_prev_macd(prev_macd);
    guard.set_prev_signal(prev_signal);
    guard.set_signal(signal);
    guard.set_signal_score(signal_score);
    evaluate_and_store_volume(data, &mut guard, config.sma_long_period);

    Ok(guard)
}

/// Calculates volume stats (latest, average, ratio) and stores them in the guard.
/// Uses the last `period` bars for the average; silently skips if volume data is absent.
/// The latest volume is taken from the most recent bar with volume > 0, skipping trailing
/// zero-volume bars that Yahoo Finance may include as incomplete current-bar placeholders.
pub fn evaluate_and_store_volume(
    data: &[MarketData],
    guard: &mut TechnicalDataGuard,
    period: usize,
) {
    let Some(latest_vol) = data
        .iter()
        .rev()
        .filter_map(|bar| bar.volume)
        .find(|v| v.is_finite() && *v > 0.0)
    else {
        return;
    };
    guard.set_latest_volume(latest_vol);

    let n = period.min(data.len());
    let volumes: Vec<f64> = data[data.len() - n..]
        .iter()
        .filter_map(|bar| bar.volume)
        .filter(|v| v.is_finite() && *v > 0.0)
        .collect();

    if volumes.is_empty() {
        return;
    }
    let avg = volumes.iter().sum::<f64>() / volumes.len() as f64;
    guard.set_avg_volume(avg);
    guard.set_volume_ratio(latest_vol / avg);
}

/// Orchestrate evaluation of all selected extension indicators
pub fn evaluate_all_selected_extensions(
    config: &Config,
    data: &[MarketData],
    guard: &mut TechnicalDataGuard,
) -> Result<()> {
    let report = evaluate_all_selected_extensions_with_report(config, data, guard)?;
    if report.has_failures() {
        crate::logging::warn(
            "XK-IND-EVAL",
            &format!(
                "Some extended technical indicator evaluations failed: {}",
                report.failure_message()
            ),
        );
    }
    Ok(())
}

/// Evaluate selected extensions and return structured partial-failure details.
pub fn evaluate_all_selected_extensions_with_report(
    config: &Config,
    data: &[MarketData],
    guard: &mut TechnicalDataGuard,
) -> Result<ExtensionEvaluationReport> {
    let mut report = ExtensionEvaluationReport {
        attempted: config.enabled_extensions.len(),
        ..ExtensionEvaluationReport::default()
    };
    for indicator in &config.enabled_extensions {
        let evaluator = get_extension_evaluator(indicator);
        match evaluator(config, data, guard) {
            Ok(()) => report.succeeded += 1,
            Err(e) => report.failures.push(ExtensionEvaluationFailure {
                indicator: indicator.clone(),
                message: e.to_string(),
            }),
        }
    }

    if report.failures.is_empty() || report.succeeded > 0 || report.attempted == 0 {
        Ok(report)
    } else {
        bail!("{}", report.failure_message())
    }
}

/// Get the evaluator function for a specific indicator
pub fn get_extension_evaluator(
    indicator: &crate::config::ExtensionIndicator,
) -> ExtensionEvaluator {
    match indicator {
        crate::config::ExtensionIndicator::Ema => evaluate_and_store_ema,
        crate::config::ExtensionIndicator::Sma => evaluate_and_store_sma,
        crate::config::ExtensionIndicator::Bollinger => evaluate_and_store_bollinger,
        crate::config::ExtensionIndicator::Roc => evaluate_and_store_roc,
        crate::config::ExtensionIndicator::Adx => evaluate_and_store_adx,
        crate::config::ExtensionIndicator::Stochastics => evaluate_and_store_stochastics,
        crate::config::ExtensionIndicator::Fibonacci => evaluate_and_store_fibonacci,
        crate::config::ExtensionIndicator::Vwap => evaluate_and_store_vwap,
        crate::config::ExtensionIndicator::Ichimoku => evaluate_and_store_ichimoku,
    }
}

// ── Score classification helpers (pure functions / testable) ─────────────────

/// The five-band rule, shared by every indicator scored on a signed quantity:
/// `weak` and `strong` are the positive boundaries and the negative side mirrors
/// them, so a larger reading never scores lower than a smaller one.
///
/// **A threshold is met at the threshold.** This is the project's one boundary
/// rule, and every score ladder follows it: at exactly a boundary the band it
/// bounds applies, and a neutral band is therefore open at both ends. Before 2.9.9
/// the codebase held four different conventions — the deviation indicators, ADX and
/// Stochastics put a boundary in the stronger band, while ROC and Bollinger put it
/// in the weaker one and Fibonacci put it in the inner band — so a reader had to
/// carry a different rule per indicator, and the manuals disagreed with the code
/// about which.
///
/// 5 段階の規則。符号付きの量で採点するすべての指標が共有する。`weak` と `strong` が
/// 正側の境界で、負側はそれを鏡写しにする。したがって読み取り値が大きくなったときに
/// スコアが下がることはない。
///
/// **閾値ちょうどは、その閾値を満たしたものとする。** これがこのプロジェクト唯一の境界
/// 規約で、すべてのスコア段階がこれに従う。境界はそれが区切る帯に属し、中立帯は両端が
/// 開く。2.9.9 より前はコードベースに 4 通りの規約が混在していた——乖離率の各指標・ADX・
/// ストキャスは境界を強い側に、ROC とボリンジャーは弱い側に、フィボナッチは内側の帯に
/// 入れていた——読む側は指標ごとに別の規則を覚える必要があり、しかもマニュアルはどちらか
/// についてコードと食い違っていた。
fn five_band_score(dev_pct: f64, weak: f64, strong: f64) -> f64 {
    match dev_pct {
        d if d >= strong => 2.0,
        d if d >= weak => 1.0,
        d if d <= -strong => -2.0,
        d if d <= -weak => -1.0,
        _ => 0.0,
    }
}

/// `(short EMA − long EMA) / close × 100`, the rate the EMA block displays.
pub(crate) fn ema_score_from_dev_pct(dev_pct: f64) -> f64 {
    five_band_score(dev_pct, EMA_WEAK_DEV_PCT, EMA_STRONG_DEV_PCT)
}

/// `(short SMA − long SMA) / close × 100`, the rate the SMA block displays.
pub(crate) fn sma_score_from_dev_pct(dev_pct: f64) -> f64 {
    five_band_score(dev_pct, SMA_WEAK_DEV_PCT, SMA_STRONG_DEV_PCT)
}

/// `(tenkan − kijun) / kijun × 100`, the divergence rate the Ichimoku block
/// displays (which shows it unsigned; the score needs the sign).
pub(crate) fn ichimoku_score_from_dev_pct(dev_pct: f64) -> f64 {
    five_band_score(dev_pct, ICHIMOKU_WEAK_DEV_PCT, ICHIMOKU_STRONG_DEV_PCT)
}

/// `(close − VWAP) / VWAP × 100`, the rate the VWAP block displays.
pub(crate) fn vwap_score_from_dev_pct(dev_pct: f64) -> f64 {
    five_band_score(dev_pct, VWAP_WEAK_DEV_PCT, VWAP_STRONG_DEV_PCT)
}

/// The Fibonacci neutral band in price terms: `ratio` of the 50% → 38.2% distance,
/// which is the room a ±1 band has. One definition, so the score and the line that
/// explains it cannot state different widths.
pub(crate) fn fibonacci_neutral_band(ratio: f64, f38: f64, f50: f64) -> f64 {
    crate::config::clamp_fibonacci_neutral_ratio(ratio) * (f38 - f50)
}

/// ROC is already a percentage change, so it is the same five-band ladder as the
/// deviation indicators and shares their boundary rule. It used to carry its own
/// match with `>` on the positive side and `>= -weak` for neutral, which put a
/// boundary in the weaker band — the opposite of every other ladder.
pub(crate) fn roc_score_from_pct(roc: f64) -> f64 {
    five_band_score(roc, ROC_WEAK_THRESHOLD, ROC_STRONG_THRESHOLD)
}

pub(crate) fn adx_score_from_val(adx: f64) -> f64 {
    match adx {
        a if a >= ADX_VERY_STRONG => 2.0,
        a if a >= ADX_STRONG => 1.0,
        a if a >= ADX_NEUTRAL => 0.0,
        a if a >= ADX_WEAK => -1.0,
        _ => -2.0,
    }
}

pub(crate) fn stoch_score_from_k(k: f64) -> f64 {
    match k {
        k if k >= STOCH_OVERBOUGHT_STRONG => -2.0,
        k if k >= STOCH_OVERBOUGHT => -1.0,
        k if k <= STOCH_OVERSOLD_STRONG => 2.0,
        k if k <= STOCH_OVERSOLD => 1.0,
        _ => 0.0,
    }
}

/// Bollinger is not a signed deviation, so it keeps its own ladder — but it follows
/// the one boundary rule: a band is reached **at** the band. Touching the upper band
/// scores `-1`, and reaching 2% beyond it scores `-2`. The comparisons were strict
/// (`>` / `<`) before 2.9.9, which put a price sitting exactly on a band inside it.
pub(crate) fn bollinger_score_from_price(price: f64, upper: f64, lower: f64) -> f64 {
    // A band with no width carries no position: with zero volatility the two edges
    // coincide, so a price is simultaneously "at" both and the score would be decided
    // by arm order — a perfectly flat series was penalised -1. There is nothing to be
    // outside of, so the reading is neutral.
    if !(upper - lower).is_finite() || upper <= lower {
        return 0.0;
    }
    match price {
        p if p >= upper * BOLL_OUTSIDE_UPPER_FACTOR => -2.0,
        p if p >= upper => -1.0,
        p if p <= lower * BOLL_OUTSIDE_LOWER_FACTOR => 2.0,
        p if p <= lower => 1.0,
        _ => 0.0,
    }
}

/// Evaluates EMA (Exponential Moving Average) and stores results in the guard
fn evaluate_and_store_ema(
    config: &Config,
    data: &[MarketData],
    guard: &mut TechnicalDataGuard,
) -> Result<()> {
    use ta::indicators::ExponentialMovingAverage;

    if config.ema_short_period >= config.ema_long_period {
        bail!("❌ EMA period config invalid (short period must be less than long period)");
    }
    if data.len() < config.ema_long_period {
        bail!(
            "❌ EMA calculation requires at least {} bars",
            config.ema_long_period
        );
    }

    let closes: Vec<f64> = data.iter().map(|d| d.close).collect();

    let mut ema_short = ExponentialMovingAverage::new(config.ema_short_period)
        .map_err(|e| anyhow!("❌ EMA short-period initialization failed: {e}"))?;
    let mut ema_long = ExponentialMovingAverage::new(config.ema_long_period)
        .map_err(|e| anyhow!("❌ EMA long-period initialization failed: {e}"))?;

    let mut ema_short_val = 0.0;
    let mut ema_long_val = 0.0;

    for close in closes.iter().cloned() {
        ema_short_val = ema_short.next(close);
        ema_long_val = ema_long.next(close);
    }

    guard.set_ema_short(ema_short_val);
    guard.set_ema_long(ema_long_val);

    // A close of zero yields no rate, so the score stays absent rather than
    // becoming a stand-in 0 (security-design §1).
    let close = guard.get_close();
    if close.is_finite() && close.abs() > f64::EPSILON {
        let dev_pct = (ema_short_val - ema_long_val) / close * 100.0;
        if dev_pct.is_finite() {
            guard.set_ema_score(ema_score_from_dev_pct(dev_pct));
        }
    }

    Ok(())
}

/// Evaluates SMA (Simple Moving Average) and stores results in the guard
fn evaluate_and_store_sma(
    config: &Config,
    data: &[MarketData],
    guard: &mut TechnicalDataGuard,
) -> Result<()> {
    use ta::indicators::SimpleMovingAverage;

    if config.sma_short_period >= config.sma_long_period {
        bail!("❌ SMA period config invalid (short period must be less than long period)");
    }
    if data.len() < config.sma_long_period {
        bail!(
            "❌ SMA calculation requires at least {} bars",
            config.sma_long_period
        );
    }

    let closes: Vec<f64> = data.iter().map(|d| d.close).collect();

    let mut sma_short = SimpleMovingAverage::new(config.sma_short_period)?;
    let mut sma_long = SimpleMovingAverage::new(config.sma_long_period)?;

    let short = closes
        .iter()
        .cloned()
        .map(|c| sma_short.next(c))
        .last()
        .unwrap_or(0.0);
    let long = closes
        .iter()
        .cloned()
        .map(|c| sma_long.next(c))
        .last()
        .unwrap_or(0.0);

    guard.set_sma_short(short);
    guard.set_sma_long(long);

    // As for EMA: no trustworthy close, no rate, no score.
    let close = guard.get_close();
    if close.is_finite() && close.abs() > f64::EPSILON {
        let dev_pct = (short - long) / close * 100.0;
        if dev_pct.is_finite() {
            guard.set_sma_score(sma_score_from_dev_pct(dev_pct));
        }
    }
    Ok(())
}

/// Evaluates ADX and stores results in the guard (Wilder smoothing)
fn evaluate_and_store_adx(
    config: &Config,
    data: &[MarketData],
    guard: &mut TechnicalDataGuard,
) -> Result<()> {
    let period = config.adx_period;
    let min_len = 2 * period;
    if data.len() < min_len {
        bail!(
            "❌ ADX (Wilder smoothing) calculation requires at least {} bars",
            min_len
        );
    }

    let n = data.len() - 1;
    let mut trs = Vec::with_capacity(n);
    let mut plus_dms = Vec::with_capacity(n);
    let mut minus_dms = Vec::with_capacity(n);

    for i in 0..n {
        let curr = &data[i + 1];
        let prev = &data[i];
        if !curr.high.is_finite()
            || !curr.low.is_finite()
            || !curr.close.is_finite()
            || !prev.high.is_finite()
            || !prev.low.is_finite()
            || !prev.close.is_finite()
        {
            bail!("❌ ADX calculation: invalid price data");
        }

        let high_diff = curr.high - prev.high;
        let low_diff = prev.low - curr.low;
        let tr = (curr.high - curr.low)
            .max((curr.high - prev.close).abs())
            .max((curr.low - prev.close).abs());
        trs.push(tr);
        plus_dms.push(if high_diff > low_diff && high_diff > 0.0 {
            high_diff
        } else {
            0.0
        });
        minus_dms.push(if low_diff > high_diff && low_diff > 0.0 {
            low_diff
        } else {
            0.0
        });
    }

    let dx_from = |atr: f64, p_dm: f64, m_dm: f64| -> f64 {
        if atr > f64::EPSILON && atr.is_finite() {
            let p_di = 100.0 * (p_dm / atr);
            let m_di = 100.0 * (m_dm / atr);
            let di_sum = p_di + m_di;
            if p_di.is_finite() && m_di.is_finite() && di_sum > f64::EPSILON {
                100.0 * ((p_di - m_di).abs() / di_sum)
            } else {
                0.0
            }
        } else {
            0.0
        }
    };

    // Wilder seed: sum of the first `period` bars
    let mut atr: f64 = trs[..period].iter().sum();
    let mut plus_dm: f64 = plus_dms[..period].iter().sum();
    let mut minus_dm: f64 = minus_dms[..period].iter().sum();

    let mut dx_values = Vec::with_capacity(n - period + 1);
    dx_values.push(dx_from(atr, plus_dm, minus_dm));

    // RMA rolling update: atr_new = atr - atr/period + tr
    for i in period..n {
        atr = atr - atr / period as f64 + trs[i];
        plus_dm = plus_dm - plus_dm / period as f64 + plus_dms[i];
        minus_dm = minus_dm - minus_dm / period as f64 + minus_dms[i];
        dx_values.push(dx_from(atr, plus_dm, minus_dm));
    }

    // ADX seed: simple average of the first `period` DX values
    let mut adx: f64 = dx_values[..period].iter().sum::<f64>() / period as f64;
    // ADX rolling update: adx_new = (adx * (period-1) + dx) / period
    for &dx in &dx_values[period..] {
        adx = (adx * (period as f64 - 1.0) + dx) / period as f64;
    }

    let adx_score = adx_score_from_val(adx);
    guard.set_adx(adx);
    guard.set_adx_score(adx_score);

    Ok(())
}

/// Calculates ROC (Rate of Change) and stores it in the guard
fn evaluate_and_store_roc(
    config: &Config,
    data: &[MarketData],
    guard: &mut TechnicalDataGuard,
) -> Result<()> {
    if data.len() <= config.roc_period {
        bail!(
            "❌ ROC calculation requires at least {} bars",
            config.roc_period + 1
        );
    }

    let latest_close = data[data.len() - 1].close;
    let previous_close = data[data.len() - (config.roc_period + 1)].close;
    if !latest_close.is_finite() || !previous_close.is_finite() {
        bail!("❌ ROC calculation: invalid close price data");
    }
    if previous_close.abs() <= f64::EPSILON {
        bail!("❌ ROC calculation: past close price is 0");
    }

    let roc = ((latest_close - previous_close) / previous_close) * 100.0;

    let roc_score = roc_score_from_pct(roc);

    guard.set_roc(roc);
    guard.set_roc_score(roc_score);

    Ok(())
}

/// Calculates Stochastics and stores results in the guard
fn evaluate_and_store_stochastics(
    config: &Config,
    data: &[MarketData],
    guard: &mut TechnicalDataGuard,
) -> Result<()> {
    let period = config.stochastics_period;
    let min_len = period.max(3);
    if data.len() < min_len {
        bail!(
            "❌ Stochastics calculation requires at least {} bars",
            min_len
        );
    }

    let mut highest_highs = Vec::new();
    let mut lowest_lows = Vec::new();
    let mut closes = Vec::new();

    for i in 0..data.len() {
        closes.push(data[i].close);
        let start = (i + 1).saturating_sub(period);
        let high = data[start..=i]
            .iter()
            .map(|d| d.high)
            .fold(f64::MIN, f64::max);
        let low = data[start..=i]
            .iter()
            .map(|d| d.low)
            .fold(f64::MAX, f64::min);
        highest_highs.push(high);
        lowest_lows.push(low);
    }

    let last = data.len() - 1;
    let high = highest_highs[last];
    let low = lowest_lows[last];
    let close = closes[last];

    let percent_k = if high != low {
        ((close - low) / (high - low)) * 100.0
    } else {
        0.0
    };

    let mut percent_ds = Vec::new();
    for i in (last + 1 - 3)..=last {
        let high = highest_highs[i];
        let low = lowest_lows[i];
        let close = closes[i];
        let k = if high != low {
            ((close - low) / (high - low)) * 100.0
        } else {
            0.0
        };
        percent_ds.push(k);
    }

    let percent_d = percent_ds.iter().copied().sum::<f64>() / percent_ds.len() as f64;

    guard.set_stochastics_k(percent_k);
    guard.set_stochastics_d(percent_d);

    let stoch_score = stoch_score_from_k(percent_k);

    guard.set_stochastics_score(stoch_score);

    Ok(())
}

/// Calculates Bollinger Bands and stores results in the guard
fn evaluate_and_store_bollinger(
    config: &Config,
    data: &[MarketData],
    guard: &mut TechnicalDataGuard,
) -> Result<()> {
    let closes: Vec<f64> = data.iter().map(|d| d.close).collect();

    let period = config.bollinger_period;
    if closes.len() < period {
        bail!("❌ Bollinger Bands: fewer than {} data points.", period);
    }
    let stddev_multiplier = config.bollinger_stddev_multiplier;

    let mut bb = BollingerBands::new(period, stddev_multiplier)?;

    let mut upper: f64 = 0.0;
    let mut lower: f64 = 0.0;

    for &price in &closes {
        let bands = bb.next(price);
        upper = bands.upper;
        lower = bands.lower;
    }

    let current_price: f64 = *closes.last().unwrap_or(&0.0);

    guard.set_bb_upper(upper);
    guard.set_bb_lower(lower);

    let mid: f64 = (upper + lower) * 0.5;

    let denom = upper - lower;
    let percent_b: f64 = if denom != 0.0 {
        (current_price - lower) / denom
    } else {
        0.0
    };

    let bandwidth_pct: f64 = if mid != 0.0 {
        (upper - lower) / mid * 100.0
    } else {
        0.0
    };

    guard.set_bb_percent_b(percent_b);
    guard.set_bb_bandwidth(bandwidth_pct);

    let bollinger_score = bollinger_score_from_price(current_price, upper, lower);
    guard.set_bollinger_score(bollinger_score);

    Ok(())
}

/// Calculates Fibonacci retracement levels and stores results in the guard
fn evaluate_and_store_fibonacci(
    config: &Config,
    data: &[MarketData],
    guard: &mut TechnicalDataGuard,
) -> Result<()> {
    if data.len() < 2 {
        guard.set_fibonacci_score(0.0);
        bail!("❌ Fibonacci calculation requires at least 2 bars");
    }

    let highs: Vec<f64> = data.iter().map(|d| d.high).collect();
    let lows: Vec<f64> = data.iter().map(|d| d.low).collect();
    let high = highs.iter().cloned().fold(f64::MIN, f64::max);
    let low = lows.iter().cloned().fold(f64::MAX, f64::min);
    let span = high - low;
    if span <= 0.0 {
        guard.set_fibonacci_score(0.0);
        return Ok(());
    }

    let f38 = high - span * 0.382;
    let f50 = high - span * 0.500;
    let f62 = high - span * 0.618;

    guard.set_fibo_38_2(f38);
    guard.set_fibo_50_0(f50);
    guard.set_fibo_61_8(f62);

    let close = guard.get_close();
    // The neutral band is a share of the room the ±1 bands have (the 50% → 38.2%
    // distance, `0.118 × span`), capped below 1.0, so it scales with the instrument
    // and can never leave `±1` unreachable. As a price difference it did: at 0.5 on
    // F, `+1` required a close both above 15.080 and below 14.984.
    let eps = fibonacci_neutral_band(config.fibonacci_neutral_ratio, f38, f50);

    // The one boundary rule: a level is reached **at** the level, so the neutral
    // band is open at both ends and the 38.2% / 61.8% levels belong to `±2`.
    //
    // The arms used to be strict (`c > f38`) with `|c - f50| <= eps` for neutral,
    // which put a boundary in the inner band and needed a fall-through arm to stay
    // total — a close sitting exactly on the 38.2% level scored `+1`. The ratio cap
    // keeps `f50 + eps < f38`, so the five arms below cover every close with no gap
    // and no overlap, and no fall-through is needed.
    // The ±2 levels are tested first so no neutral band can swallow them, and each
    // ±1 arm requires the close to be on its own side of the midpoint. That second
    // condition is what makes `fibonacci_neutral_ratio = 0` well defined: with no
    // band the two ±1 conditions would otherwise meet at the 50% level and arm order
    // alone would decide it leaned up. The midpoint itself is neutral whatever the
    // band width, and for a positive width the boundary still belongs to `±1`,
    // keeping the one rule — a threshold is met at the threshold.
    let score = match close {
        c if c >= f38 => 2.0,
        c if c <= f62 => -2.0,
        c if c > f50 && c >= f50 + eps => 1.0,
        c if c < f50 && c <= f50 - eps => -1.0,
        _ => 0.0,
    };

    guard.set_fibonacci_score(score);
    Ok(())
}

/// Helper for VWAP calculation over a slice of bars
pub fn calculate_period_vwap(data: &[MarketData], period: usize) -> Result<f64> {
    if data.len() < period {
        bail!(
            "❌ VWAP calculation: insufficient data ({} bars required)",
            period
        );
    }

    let mut numerator = 0.0;
    let mut denominator = 0.0;

    for row in &data[data.len() - period..] {
        let volume = row
            .volume
            .ok_or_else(|| anyhow!("❌ VWAP calculation: insufficient volume data"))?;
        if !volume.is_finite() || volume < 0.0 {
            bail!("❌ VWAP calculation: invalid volume data");
        }
        if !row.high.is_finite() || !row.low.is_finite() || !row.close.is_finite() {
            bail!("❌ VWAP calculation: invalid price data");
        }
        // Skip bars with no traded volume (e.g. pre/post-market gaps, illiquid intervals)
        if volume == 0.0 {
            continue;
        }

        let typical_price = (row.high + row.low + row.close) / 3.0;
        numerator += typical_price * volume;
        denominator += volume;
    }

    if denominator <= 0.0 {
        bail!("❌ VWAP calculation: no bars with non-zero volume in the period");
    }

    Ok(numerator / denominator)
}

/// Calculates VWAP (Volume Weighted Average Price) and stores it in the guard
fn evaluate_and_store_vwap(
    config: &Config,
    data: &[MarketData],
    guard: &mut TechnicalDataGuard,
) -> Result<()> {
    let vwap = if config.analysis_mode.is_intraday() {
        // Intraday mode: restrict to bars from the same session date as the last bar (daily reset)
        let last_date = data
            .last()
            .ok_or_else(|| anyhow!("❌ VWAP calculation requires data"))?
            .date
            .as_str();
        let start = data.partition_point(|bar| bar.date.as_str() < last_date);
        let session = &data[start..];
        calculate_period_vwap(session, session.len())?
    } else {
        calculate_period_vwap(data, config.vwap_period)?
    };

    guard.set_vwap(vwap);

    // A VWAP of zero, or one the division cannot be trusted on, yields no rate —
    // so the score stays absent rather than becoming a stand-in 0.
    if vwap.is_finite() && vwap.abs() > f64::EPSILON {
        let dev_pct = (guard.get_close() - vwap) / vwap * 100.0;
        if dev_pct.is_finite() {
            guard.set_vwap_score(vwap_score_from_dev_pct(dev_pct));
        }
    }

    Ok(())
}

/// Calculates Ichimoku Tenkan-sen and Kijun-sen lines and stores them in the guard
fn evaluate_and_store_ichimoku(
    config: &Config,
    data: &[MarketData],
    guard: &mut TechnicalDataGuard,
) -> Result<()> {
    if config.ichimoku_tenkan_period >= config.ichimoku_kijun_period {
        bail!("❌ Ichimoku period config invalid (conversion line must be less than base line)");
    }
    if data.len() < config.ichimoku_kijun_period {
        bail!(
            "❌ Ichimoku evaluation requires at least {} periods of data",
            config.ichimoku_kijun_period
        );
    }

    let recent_tenkan = &data[data.len() - config.ichimoku_tenkan_period..];
    let high_tenkan = recent_tenkan
        .iter()
        .map(|d| d.high)
        .fold(f64::MIN, f64::max);
    let low_tenkan = recent_tenkan.iter().map(|d| d.low).fold(f64::MAX, f64::min);
    let tenkan = (high_tenkan + low_tenkan) / 2.0;

    let recent_kijun = &data[data.len() - config.ichimoku_kijun_period..];
    let high_kijun = recent_kijun.iter().map(|d| d.high).fold(f64::MIN, f64::max);
    let low_kijun = recent_kijun.iter().map(|d| d.low).fold(f64::MAX, f64::min);
    let kijun = (high_kijun + low_kijun) / 2.0;

    guard.set_tenkan_sen(tenkan);
    guard.set_kijun_sen(kijun);

    // The divergence rate, normalised by the base line — the same figure the
    // display prints, there unsigned. A kijun of zero yields no rate, so the score
    // stays absent rather than becoming a stand-in 0 (security-design §1).
    if kijun.is_finite() && kijun.abs() > f64::EPSILON {
        let dev_pct = (tenkan - kijun) / kijun * 100.0;
        if dev_pct.is_finite() {
            guard.set_ichimoku_score(ichimoku_score_from_dev_pct(dev_pct));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        adx_score_from_val, bollinger_score_from_price, build_basic_technical_entry,
        ema_score_from_dev_pct, fibonacci_neutral_band, ichimoku_score_from_dev_pct,
        roc_score_from_pct, sma_score_from_dev_pct, stoch_score_from_k, vwap_score_from_dev_pct,
    };
    use crate::config::Config;
    use crate::market::MarketData;
    use crate::technical::types::TechnicalDataGuard;
    use std::collections::HashMap;

    fn make_row(date: &str, price: f64) -> MarketData {
        MarketData {
            date: date.to_string(),
            datetime: None,
            timestamp: None,
            timezone: None,
            high: price + 1.0,
            low: price - 1.0,
            close: price,
            volume: Some(1_000_000.0),
            name: None,
        }
    }

    /// A row with an explicit high/low, so a test can fix the Fibonacci span and
    /// move only the close.
    fn make_row_hl(date: &str, low: f64, high: f64, close: f64) -> MarketData {
        MarketData {
            date: date.to_string(),
            datetime: None,
            timestamp: None,
            timezone: None,
            high,
            low,
            close,
            volume: Some(1_000_000.0),
            name: None,
        }
    }

    fn flat_data(n: usize, price: f64) -> Vec<MarketData> {
        (0..n)
            .map(|i| make_row(&format!("2026-01-{:02}", i + 1), price))
            .collect()
    }

    fn zero_range_data(n: usize, price: f64) -> Vec<MarketData> {
        (0..n)
            .map(|i| MarketData {
                date: format!("2026-02-{:02}", i + 1),
                datetime: None,
                timestamp: None,
                timezone: None,
                high: price,
                low: price,
                close: price,
                volume: Some(1_000_000.0),
                name: None,
            })
            .collect()
    }

    // ── Layer 1: EMA score boundary values (deviation rate, in percent) ──────
    #[test]
    fn ema_score_max_positive() {
        assert_eq!(ema_score_from_dev_pct(2.0), 2.0);
    }
    #[test]
    fn ema_score_mid_positive() {
        assert_eq!(ema_score_from_dev_pct(0.5), 1.0);
    }
    #[test]
    fn ema_score_neutral_below_boundary() {
        assert_eq!(ema_score_from_dev_pct(0.49), 0.0);
    }
    #[test]
    fn ema_score_neutral_zero() {
        assert_eq!(ema_score_from_dev_pct(0.0), 0.0);
    }
    #[test]
    fn ema_score_mid_negative() {
        assert_eq!(ema_score_from_dev_pct(-0.5), -1.0);
    }
    #[test]
    fn ema_score_max_negative() {
        assert_eq!(ema_score_from_dev_pct(-2.0), -2.0);
    }

    // ── Layer 1: SMA score boundary values (deviation rate, in percent) ──────
    #[test]
    fn sma_score_max_positive() {
        assert_eq!(sma_score_from_dev_pct(3.0), 2.0);
    }
    #[test]
    fn sma_score_mid_positive() {
        assert_eq!(sma_score_from_dev_pct(1.0), 1.0);
    }
    #[test]
    fn sma_score_neutral_below_boundary() {
        assert_eq!(sma_score_from_dev_pct(0.99), 0.0);
    }
    #[test]
    fn sma_score_mid_negative() {
        assert_eq!(sma_score_from_dev_pct(-1.0), -1.0);
    }
    #[test]
    fn sma_score_max_negative() {
        assert_eq!(sma_score_from_dev_pct(-3.0), -2.0);
    }

    // ── Layer 1: Ichimoku score boundary values (deviation rate, in percent) ──
    #[test]
    fn ichimoku_score_max_positive() {
        assert_eq!(ichimoku_score_from_dev_pct(4.0), 2.0);
    }
    #[test]
    fn ichimoku_score_mid_positive() {
        assert_eq!(ichimoku_score_from_dev_pct(1.0), 1.0);
    }
    #[test]
    fn ichimoku_score_neutral_below_boundary() {
        assert_eq!(ichimoku_score_from_dev_pct(0.99), 0.0);
    }
    #[test]
    fn ichimoku_score_mid_negative() {
        assert_eq!(ichimoku_score_from_dev_pct(-1.0), -1.0);
    }
    #[test]
    fn ichimoku_score_max_negative() {
        assert_eq!(ichimoku_score_from_dev_pct(-4.0), -2.0);
    }

    /// **A threshold is met at the threshold** — the project's one boundary rule,
    /// asserted at every ladder's exact boundaries. Nothing pinned this before:
    /// the existing per-indicator tests all used off-boundary values, which is how
    /// four different conventions coexisted and how the manuals came to disagree
    /// with the code about which one applied.
    #[test]
    fn every_score_ladder_meets_its_threshold_at_the_threshold() {
        // Signed five-band ladders: the boundary belongs to the band it bounds, and
        // the neutral band is open at both ends.
        for (name, f, weak, strong) in [
            ("EMA", ema_score_from_dev_pct as fn(f64) -> f64, 0.5, 2.0),
            ("SMA", sma_score_from_dev_pct, 1.0, 3.0),
            ("Ichimoku", ichimoku_score_from_dev_pct, 1.0, 4.0),
            ("VWAP", vwap_score_from_dev_pct, 1.0, 3.0),
            ("ROC", roc_score_from_pct, 3.0, 10.0),
        ] {
            assert_eq!(f(strong), 2.0, "{name}: +strong must score +2");
            assert_eq!(f(weak), 1.0, "{name}: +weak must score +1");
            assert_eq!(f(-weak), -1.0, "{name}: -weak must score -1");
            assert_eq!(f(-strong), -2.0, "{name}: -strong must score -2");
        }

        // ADX and Stochastics are one-sided ladders on a 0-100 reading.
        assert_eq!(adx_score_from_val(50.0), 2.0);
        assert_eq!(adx_score_from_val(30.0), 1.0);
        assert_eq!(adx_score_from_val(20.0), 0.0);
        assert_eq!(adx_score_from_val(10.0), -1.0);
        assert_eq!(stoch_score_from_k(90.0), -2.0);
        assert_eq!(stoch_score_from_k(80.0), -1.0);
        assert_eq!(stoch_score_from_k(20.0), 1.0);
        assert_eq!(stoch_score_from_k(10.0), 2.0);

        // Bollinger: a band is reached AT the band.
        let (upper, lower) = (100.0_f64, 90.0_f64);
        assert_eq!(bollinger_score_from_price(upper, upper, lower), -1.0);
        assert_eq!(bollinger_score_from_price(upper * 1.02, upper, lower), -2.0);
        assert_eq!(bollinger_score_from_price(lower, upper, lower), 1.0);
        assert_eq!(bollinger_score_from_price(lower * 0.98, upper, lower), 2.0);
    }

    /// A band with no width carries no position, so a flat series must score 0. With
    /// zero volatility the upper and lower bands coincide, and once the comparisons
    /// became inclusive the upper arm matched first and a perfectly flat price was
    /// penalised `-1` — a regression introduced with the boundary unification.
    #[test]
    fn a_band_with_no_width_scores_neutral() {
        assert_eq!(bollinger_score_from_price(100.0, 100.0, 100.0), 0.0);
        // Still neutral when the price sits away from the degenerate band.
        assert_eq!(bollinger_score_from_price(100.0, 100.0, 100.0), 0.0);
    }

    /// `fibonacci_neutral_ratio = 0` is an accepted setting (no neutral band). The
    /// midpoint itself must still be neutral: with no band, the `+1` and `-1`
    /// conditions meet at the 50% level and arm order alone decided it leaned up.
    #[test]
    fn the_fibonacci_midpoint_is_neutral_even_with_no_neutral_band() {
        let (high, low) = (110.0_f64, 90.0_f64);
        let f50 = high - (high - low) * 0.500;
        let config = Config {
            fibonacci_neutral_ratio: 0.0,
            ..Config::default()
        };
        let data = vec![
            make_row_hl("2026-09-24", low, high, f50),
            make_row_hl("2026-09-25", low, high, f50),
        ];
        let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-09-25".to_string());
        guard.set_close(f50);
        super::evaluate_and_store_fibonacci(&config, &data, &mut guard).expect("scores");
        assert_eq!(
            guard.get_fibonacci_score(),
            Some(0.0),
            "a close exactly on the 50% level must not lean up"
        );
    }

    // ── Layer 1: Fibonacci neutral band ──────────────────────────────────────
    /// The band scales with the instrument, so the same ratio means the same share
    /// of the ±1 room on a JPY 4,000 listing and on a USD 13 one.
    #[test]
    fn fibonacci_neutral_band_scales_with_the_instrument() {
        // room = f38 - f50: 120.36 on 1605.T, 0.404 on F (measured 2026-09-24).
        for (f38, f50, room) in [
            (4083.36_f64, 3963.0_f64, 120.36_f64),
            (14.984, 14.58, 0.404),
        ] {
            let band = fibonacci_neutral_band(0.05, f38, f50);
            assert!(
                (band / room - 0.05).abs() < 1e-9,
                "band {band} should be 5% of room {room}"
            );
        }
    }

    /// Fibonacci follows the same boundary rule: the 38.2% / 61.8% levels belong to
    /// `±2`, and the neutral band is open at both ends. The arms were strict before
    /// 2.9.9 and needed a fall-through to stay total, which put a close sitting
    /// exactly on the 38.2% level at `+1`.
    #[test]
    fn fibonacci_levels_belong_to_the_band_they_bound() {
        let (high, low) = (110.0_f64, 90.0_f64);
        let span = high - low;
        let (f38, f50, f62) = (
            high - span * 0.382,
            high - span * 0.500,
            high - span * 0.618,
        );
        let eps = fibonacci_neutral_band(0.05, f38, f50);
        let config = Config {
            fibonacci_neutral_ratio: 0.05,
            ..Config::default()
        };
        // One bar per close so the high/low span is fixed; only the close moves.
        for (close, expected, what) in [
            (f38, 2.0, "the 38.2% level"),
            (f62, -2.0, "the 61.8% level"),
            (f50 + eps, 1.0, "the upper edge of the neutral band"),
            (f50 - eps, -1.0, "the lower edge of the neutral band"),
            (f50, 0.0, "the 50% level"),
        ] {
            let data = vec![
                make_row_hl("2026-09-23", low, high, close),
                make_row_hl("2026-09-24", low, high, close),
            ];
            let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-09-24".to_string());
            guard.set_close(close);
            super::evaluate_and_store_fibonacci(&config, &data, &mut guard)
                .expect("a positive span scores");
            assert_eq!(
                guard.get_fibonacci_score(),
                Some(expected),
                "a close on {what} must score {expected}"
            );
        }
    }

    /// The ±1 bands must keep room whatever is configured. With a price difference
    /// they did not: at 0.5 on F, `+1` required a close above 15.080 and below
    /// 14.984 — an empty interval, so `+1` and `-1` could never be scored.
    #[test]
    fn fibonacci_neutral_band_can_never_swallow_the_plus_one_bands() {
        let (f38, f50) = (14.984_f64, 14.58_f64);
        let room = f38 - f50;
        for requested in [0.0, 0.05, 0.5, 1.0, 10.0, f64::INFINITY, f64::NAN, -3.0] {
            let band = fibonacci_neutral_band(requested, f38, f50);
            assert!(
                band >= 0.0 && band <= room * crate::config::FIBONACCI_NEUTRAL_RATIO_MAX,
                "ratio {requested} produced a band of {band}, outside 0..={}",
                room * crate::config::FIBONACCI_NEUTRAL_RATIO_MAX
            );
            assert!(
                f50 + band < f38,
                "ratio {requested} left no room for +1: f50+band={} vs f38={f38}",
                f50 + band
            );
        }
    }

    /// Every deviation-scored indicator must be monotonic in its own rate. The
    /// previous scoring compared a price difference against a fixed constant, so
    /// the order broke as soon as price levels differed: measured on 2026-09-24
    /// daily bars, 1605.T sat 0.16% from its base line and scored -2 on Ichimoku
    /// while F at 1.89% scored 0.
    #[test]
    fn every_deviation_score_is_monotonic_in_its_rate() {
        let ladder = [
            -14.9, -6.7, -4.0, -3.0, -1.89, -1.0, -0.16, 0.0, 0.16, 1.0, 1.89, 3.0, 4.0, 6.7, 14.9,
        ];
        for (name, f) in [
            ("EMA", ema_score_from_dev_pct as fn(f64) -> f64),
            ("SMA", sma_score_from_dev_pct),
            ("Ichimoku", ichimoku_score_from_dev_pct),
            ("VWAP", vwap_score_from_dev_pct),
        ] {
            let mut previous = f64::NEG_INFINITY;
            for dev in ladder {
                let score = f(dev);
                assert!(
                    score >= previous,
                    "{name}: score fell from {previous} to {score} as the rate rose to {dev}%"
                );
                previous = score;
            }
        }
    }

    /// The display notes a gap under 1% as "close after cross — trend not yet
    /// confirmed". That must be the same state the score calls 0, or the two lines
    /// would contradict each other on the same bar.
    #[test]
    fn the_close_gap_note_and_a_zero_ichimoku_score_describe_one_state() {
        for rate in [
            -6.7_f64, -4.0, -1.01, -1.0, -0.99, 0.0, 0.99, 1.0, 1.01, 4.0, 6.7,
        ] {
            let note_fires = rate.abs() < 1.0;
            let scored_zero = ichimoku_score_from_dev_pct(rate) == 0.0;
            assert_eq!(
                note_fires, scored_zero,
                "rate {rate}%: close-gap note {note_fires} but zero score {scored_zero}"
            );
        }
    }

    /// Two instruments separated by the same proportion must score the same
    /// whatever they cost — the property a price-difference threshold cannot hold.
    #[test]
    fn deviation_scores_do_not_depend_on_the_price_level() {
        // A 5% separation, on a JPY 6,000 listing and on a USD 13 one.
        for (short, long) in [(6300.0_f64, 6000.0_f64), (13.65, 13.0)] {
            let dev_pct = (short - long) / long * 100.0;
            assert_eq!(ema_score_from_dev_pct(dev_pct), 2.0);
            assert_eq!(sma_score_from_dev_pct(dev_pct), 2.0);
            assert_eq!(ichimoku_score_from_dev_pct(dev_pct), 2.0);
            assert_eq!(vwap_score_from_dev_pct(dev_pct), 2.0);
        }
    }

    // ── Layer 1: ROC score boundary values ───────────────────────────────────
    #[test]
    fn roc_score_strong_up() {
        assert_eq!(roc_score_from_pct(10.1), 2.0);
    }
    #[test]
    fn roc_score_mild_up() {
        assert_eq!(roc_score_from_pct(5.0), 1.0);
    }
    #[test]
    fn roc_score_neutral() {
        assert_eq!(roc_score_from_pct(0.0), 0.0);
    }
    #[test]
    fn roc_score_mild_down() {
        assert_eq!(roc_score_from_pct(-5.0), -1.0);
    }
    #[test]
    fn roc_score_strong_down() {
        assert_eq!(roc_score_from_pct(-10.1), -2.0);
    }

    // ── Layer 1: ADX score boundary values ───────────────────────────────────
    #[test]
    fn adx_score_very_strong() {
        assert_eq!(adx_score_from_val(50.0), 2.0);
    }
    #[test]
    fn adx_score_strong() {
        assert_eq!(adx_score_from_val(35.0), 1.0);
    }
    #[test]
    fn adx_score_moderate() {
        assert_eq!(adx_score_from_val(25.0), 0.0);
    }
    #[test]
    fn adx_score_weak() {
        assert_eq!(adx_score_from_val(15.0), -1.0);
    }
    #[test]
    fn adx_score_none() {
        assert_eq!(adx_score_from_val(5.0), -2.0);
    }

    // ── Layer 1: Stochastics score boundary values ────────────────────────────
    #[test]
    fn stoch_score_extreme_overbought() {
        assert_eq!(stoch_score_from_k(90.0), -2.0);
    }
    #[test]
    fn stoch_score_overbought() {
        assert_eq!(stoch_score_from_k(85.0), -1.0);
    }
    #[test]
    fn stoch_score_neutral() {
        assert_eq!(stoch_score_from_k(50.0), 0.0);
    }
    #[test]
    fn stoch_score_oversold() {
        assert_eq!(stoch_score_from_k(15.0), 1.0);
    }
    #[test]
    fn stoch_score_extreme_oversold() {
        assert_eq!(stoch_score_from_k(10.0), 2.0);
    }

    // ── Layer 1: Bollinger score boundary values ──────────────────────────────
    #[test]
    fn bollinger_score_far_above_upper() {
        assert_eq!(bollinger_score_from_price(103.0, 100.0, 90.0), -2.0);
    }
    #[test]
    fn bollinger_score_just_above_upper() {
        assert_eq!(bollinger_score_from_price(101.0, 100.0, 90.0), -1.0);
    }
    #[test]
    fn bollinger_score_inside_band() {
        assert_eq!(bollinger_score_from_price(95.0, 100.0, 90.0), 0.0);
    }
    #[test]
    fn bollinger_score_just_below_lower() {
        assert_eq!(bollinger_score_from_price(89.0, 100.0, 90.0), 1.0);
    }
    #[test]
    fn bollinger_score_far_below_lower() {
        assert_eq!(bollinger_score_from_price(87.0, 100.0, 90.0), 2.0);
    }

    // ── Layer 1: VWAP score boundary values (deviation rate, in percent) ──────
    #[test]
    fn vwap_score_strong_above() {
        assert_eq!(vwap_score_from_dev_pct(3.0), 2.0);
    }
    #[test]
    fn vwap_score_slightly_above() {
        assert_eq!(vwap_score_from_dev_pct(1.0), 1.0);
    }
    #[test]
    fn vwap_score_neutral() {
        assert_eq!(vwap_score_from_dev_pct(0.99), 0.0);
        assert_eq!(vwap_score_from_dev_pct(-0.99), 0.0);
    }
    #[test]
    fn vwap_score_slightly_below() {
        assert_eq!(vwap_score_from_dev_pct(-1.0), -1.0);
    }
    #[test]
    fn vwap_score_strong_below() {
        assert_eq!(vwap_score_from_dev_pct(-3.0), -2.0);
    }

    /// The score must not fall as the deviation grows. The previous scoring
    /// compared a price difference against a fixed 4.0 / 1.0, so the order broke
    /// whenever price levels differed: measured on 2026-09-24 daily bars, F was
    /// 5.03% below its VWAP and scored 0 while 7203.T at 2.19% below scored -2.
    #[test]
    fn vwap_score_is_monotonic_in_the_deviation() {
        let ladder = [
            -14.9, -5.03, -3.0, -2.19, -1.0, -0.15, 0.0, 0.47, 1.48, 2.89, 3.96, 14.9,
        ];
        let mut previous = f64::NEG_INFINITY;
        for dev in ladder {
            let score = vwap_score_from_dev_pct(dev);
            assert!(
                score >= previous,
                "score fell from {previous} to {score} as the deviation rose to {dev}%"
            );
            previous = score;
        }
    }

    /// Two instruments at the same deviation must score the same whatever they
    /// cost — the property the price-difference thresholds could not hold.
    #[test]
    fn vwap_score_does_not_depend_on_the_price_level() {
        for (close, vwap) in [(13.0_f64, 13.65_f64), (6000.0, 6300.0)] {
            let dev_pct = (close - vwap) / vwap * 100.0;
            assert_eq!(
                vwap_score_from_dev_pct(dev_pct),
                -2.0,
                "a {dev_pct}% deviation should score -2 at close {close}"
            );
        }
    }

    // ── Layer 2: Error boundary cases ────────────────────────────────────────
    #[test]
    fn build_basic_empty_returns_err() {
        let config = Config::default();
        let map = HashMap::new();
        assert!(build_basic_technical_entry(&config, &[], &map).is_err());
    }
    #[test]
    fn build_basic_one_row_returns_err() {
        let config = Config::default();
        let map = HashMap::new();
        let data = flat_data(1, 100.0);
        assert!(build_basic_technical_entry(&config, &data, &map).is_err());
    }
    #[test]
    fn ema_rejects_short_gte_long() {
        let config = Config {
            ema_short_period: 20,
            ema_long_period: 5,
            ..Config::default()
        };
        let data = flat_data(30, 100.0);
        let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-01-01".to_string());
        assert!(super::evaluate_and_store_ema(&config, &data, &mut guard).is_err());
    }
    #[test]
    fn ema_rejects_insufficient_data() {
        let config = Config::default(); // ema_long_period = 20
        let data = flat_data(10, 100.0);
        let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-01-01".to_string());
        assert!(super::evaluate_and_store_ema(&config, &data, &mut guard).is_err());
    }
    #[test]
    fn roc_rejects_insufficient_data() {
        let config = Config::default(); // roc_period = 10, needs > 10 rows
        let data = flat_data(9, 100.0);
        let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-01-01".to_string());
        assert!(super::evaluate_and_store_roc(&config, &data, &mut guard).is_err());
    }

    #[test]
    fn roc_rejects_zero_previous_close_without_storing_score() {
        let config = Config::default();
        let mut data = flat_data(11, 100.0);
        data[0].close = 0.0;
        let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-01-11".to_string());

        assert!(super::evaluate_and_store_roc(&config, &data, &mut guard).is_err());
        assert!(guard.get_roc().is_none());
        assert!(guard.get_roc_score().is_none());
    }

    #[test]
    fn adx_zero_range_data_stores_finite_safe_value() {
        let config = Config::default();
        let data = zero_range_data(30, 100.0);
        let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-02-28".to_string());

        super::evaluate_and_store_adx(&config, &data, &mut guard)
            .expect("zero-range ADX should use a finite safe value");

        assert_eq!(guard.get_adx(), Some(0.0));
        assert_eq!(guard.get_adx_score(), Some(-2.0));
    }

    #[test]
    fn stochastics_period_two_rejects_two_rows_without_panic() {
        let config = Config {
            stochastics_period: 2,
            ..Config::default()
        };
        let data = flat_data(2, 100.0);
        let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-01-01".to_string());

        assert!(super::evaluate_and_store_stochastics(&config, &data, &mut guard).is_err());
    }

    #[test]
    fn extension_evaluation_continues_after_one_indicator_fails() {
        let config = Config {
            enabled_extensions: vec![
                crate::config::ExtensionIndicator::Vwap,
                crate::config::ExtensionIndicator::Ema,
            ],
            ..Config::default()
        };
        let mut data = flat_data(30, 100.0);
        for row in &mut data {
            row.volume = None;
        }
        let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-01-30".to_string());
        // The deviation-rate scores divide by the close, which every real path sets
        // through `build_basic_technical_entry` before the extensions run.
        guard.set_close(100.0);

        let result = super::evaluate_all_selected_extensions(&config, &data, &mut guard);

        assert!(result.is_ok());
        assert!(guard.get_vwap_score().is_none());
        assert!(guard.get_ema_score().is_some());
    }

    #[test]
    fn extension_evaluation_report_exposes_partial_failures() {
        let config = Config {
            enabled_extensions: vec![
                crate::config::ExtensionIndicator::Vwap,
                crate::config::ExtensionIndicator::Ema,
            ],
            ..Config::default()
        };
        let mut data = flat_data(30, 100.0);
        for row in &mut data {
            row.volume = None;
        }
        let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-01-30".to_string());

        let report =
            super::evaluate_all_selected_extensions_with_report(&config, &data, &mut guard)
                .expect("partial failure should not abort successful indicators");

        assert_eq!(report.attempted, 2);
        assert_eq!(report.succeeded, 1);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(
            report.failures[0].indicator,
            crate::config::ExtensionIndicator::Vwap
        );
    }

    #[test]
    fn extension_evaluation_returns_err_when_all_indicators_fail() {
        let config = Config {
            enabled_extensions: vec![crate::config::ExtensionIndicator::Vwap],
            ..Config::default()
        };
        let mut data = flat_data(30, 100.0);
        for row in &mut data {
            row.volume = None;
        }
        let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-01-30".to_string());

        let result = super::evaluate_all_selected_extensions(&config, &data, &mut guard);

        assert!(result.is_err());
        assert!(guard.get_vwap_score().is_none());
    }

    #[test]
    fn vwap_intraday_excludes_previous_day_bars() {
        // 5 previous-day bars (price=200) + 5 today bars (price=100) with vwap_period=10.
        // In intraday mode only same-session bars are used, so VWAP ≈ 100.0.
        // Regression guard: if previous-day bars leak in, VWAP ≈ 150.0.
        let yesterday: Vec<MarketData> = (0..5).map(|_| make_row("2026-05-15", 200.0)).collect();
        let today: Vec<MarketData> = (0..5).map(|_| make_row("2026-05-16", 100.0)).collect();
        let data: Vec<MarketData> = yesterday.into_iter().chain(today).collect();

        let config = Config {
            analysis_mode: crate::config::AnalysisMode::Intraday5m,
            vwap_period: 10,
            ..Config::default()
        };
        let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-05-16".to_string());
        guard.set_close(100.0);

        super::evaluate_and_store_vwap(&config, &data, &mut guard).unwrap();

        let vwap = guard.get_vwap().expect("VWAP should be set");
        assert!(
            (vwap - 100.0).abs() < 0.01,
            "intraday VWAP should use only today's bars, expected ~100.0, got {vwap}"
        );
    }
}
