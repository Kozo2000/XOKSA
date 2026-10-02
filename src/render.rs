use colored::*;

const BIPOLAR_GAUGE_WIDTH: usize = 51;
const UNIPOLAR_GAUGE_WIDTH: usize = 25;

use crate::config::{Config, ExtensionIndicator, Stance};
use crate::render_indicators::{
    render_adx, render_bollinger, render_ema, render_fibonacci, render_ichimoku, render_roc,
    render_sma, render_stochastics, render_vwap,
};
use crate::technical::calculate_final_score_snapshot;
use crate::technical::types::{
    AnalysisResult, FinalScoreSnapshot, SignalStrength, TechnicalDataGuard,
};
use crate::utils;

/// The user's configured display weight for one extension indicator — the number
/// that expresses how much they value it. RSI/MACD share the single `weight_basic`.
fn extension_weight(config: &Config, indicator: &ExtensionIndicator) -> f64 {
    match indicator {
        ExtensionIndicator::Ema => config.weight_ema,
        ExtensionIndicator::Sma => config.weight_sma,
        ExtensionIndicator::Bollinger => config.weight_bollinger,
        ExtensionIndicator::Roc => config.weight_roc,
        ExtensionIndicator::Adx => config.weight_adx,
        ExtensionIndicator::Stochastics => config.weight_stochastics,
        ExtensionIndicator::Fibonacci => config.weight_fibonacci,
        ExtensionIndicator::Vwap => config.weight_vwap,
        ExtensionIndicator::Ichimoku => config.weight_ichimoku,
    }
}

fn render_one(
    config: &Config,
    guard: &TechnicalDataGuard,
    indicator: &ExtensionIndicator,
) -> AnalysisResult {
    match indicator {
        ExtensionIndicator::Ema => render_ema(config, guard),
        ExtensionIndicator::Sma => render_sma(config, guard),
        ExtensionIndicator::Bollinger => render_bollinger(config, guard),
        ExtensionIndicator::Roc => render_roc(config, guard),
        ExtensionIndicator::Adx => render_adx(config, guard),
        ExtensionIndicator::Stochastics => render_stochastics(config, guard),
        ExtensionIndicator::Fibonacci => render_fibonacci(config, guard),
        ExtensionIndicator::Vwap => render_vwap(config, guard),
        ExtensionIndicator::Ichimoku => render_ichimoku(config, guard),
    }
}

/// Every indicator to show — the basic (RSI/MACD) block plus each enabled
/// extension — ranked by the user's configured weight, descending. `sort_by` is a
/// stable sort, so indicators of EQUAL weight keep their canonical insertion order
/// (basic first, then the `enabled_extensions` order) instead of reshuffling on
/// every render. This single ordering feeds the terminal display, `/show
/// technical`, and the LLM prompt, so the leading indicator is always the user's
/// most-weighted one — never the LLM's own pick (design-philosophy §4.2 / §7.1).
pub fn render_ranked(config: &Config, guard: &TechnicalDataGuard) -> Vec<AnalysisResult> {
    let mut units: Vec<(f64, AnalysisResult)> =
        vec![(config.weight_basic, render_basic(config, guard))];
    for indicator in config.analysis_extensions() {
        units.push((
            extension_weight(config, indicator),
            render_one(config, guard, indicator),
        ));
    }
    // Weight descending; the stable sort preserves insertion order within a tie.
    units.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    units.into_iter().map(|(_, result)| result).collect()
}

pub trait Renderer {
    fn render_to_terminal(&self, config: &Config, guard: &TechnicalDataGuard);
    fn compose_final_score_lines(
        &self,
        snap: &FinalScoreSnapshot,
        stance: &Stance,
        include_gauge: bool,
        lang: &str,
    ) -> Vec<String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TerminalRenderer;

impl Renderer for TerminalRenderer {
    fn render_to_terminal(&self, config: &Config, guard: &TechnicalDataGuard) {
        technical_render_to_terminal(config, guard);
    }

    fn compose_final_score_lines(
        &self,
        snap: &FinalScoreSnapshot,
        stance: &Stance,
        include_gauge: bool,
        lang: &str,
    ) -> Vec<String> {
        compose_final_score_lines_stance(snap, stance, include_gauge, lang)
    }
}

pub fn technical_render_to_terminal(config: &Config, guard: &TechnicalDataGuard) {
    display_main_info(config, guard);

    for result in render_ranked(config, guard) {
        display_analysis_result(&result);
    }

    render_final_score(config, guard);
}

fn render_final_score(config: &Config, guard: &TechnicalDataGuard) {
    let snap = calculate_final_score_snapshot(config, guard);
    let lines = compose_final_score_lines_stance(&snap, &config.stance, true, &config.lang);
    for line in lines {
        println!("{}", line);
    }
}

/// The localized "adjusted score = score × weight" line, shared by the basic
/// renderer and every extension-indicator renderer (SOT §4.2 — one implementation,
/// not one copy per indicator). Generic over the score type so it prints i32
/// (extension scores) and f64 (the basic signal score) exactly as before.
pub(crate) fn adjusted_score_line<T: std::fmt::Display>(
    lang: &str,
    adjusted: f64,
    score: T,
    weight: f64,
) -> String {
    match lang {
        "ja" => format!(
            "📝 スコア調整値({:.1}) = スコア({}) × Weight({:.1})",
            adjusted, score, weight
        ),
        _ => format!(
            "📝 Adjusted score ({:.1}) = score ({}) × weight ({:.1})",
            adjusted, score, weight
        ),
    }
}

/// The signed price-diff string `"{:+.2} ({:+.2}%)"`, colored green when up, red
/// when down, and left plain (`normal`) at zero — shared by the terminal report and
/// the chat reload line (SOT §4.2). Returns a `ColoredString`; call `.to_string()`
/// where a plain `String` is needed (a `normal` `ColoredString` renders without any
/// ANSI codes, matching a plain `format!`).
pub(crate) fn colored_price_diff(diff: f64, percent: f64) -> colored::ColoredString {
    let s = format!("{:+.2} ({:+.2}%)", diff, percent);
    if diff > 0.0 {
        s.green()
    } else if diff < 0.0 {
        s.red()
    } else {
        s.normal()
    }
}

/// The localized "{indicator} deviation rate: X% (vs {reference})" line, shared by
/// the EMA / SMA / MACD renderers (vs close) and VWAP (vs VWAP) — SOT §4.2, one
/// template instead of a copy per indicator. Display-only (the value is computed by
/// the caller).
pub(crate) fn deviation_line(
    lang: &str,
    name_ja: &str,
    name_en: &str,
    reference_ja: &str,
    reference_en: &str,
    diff_pct: f64,
) -> String {
    match lang {
        "ja" => format!("📊 {name_ja}乖離率: {diff_pct:+.4}%（{reference_ja}）"),
        _ => format!("📊 {name_en} deviation rate: {diff_pct:+.4}% ({reference_en})"),
    }
}

/// Labels for [`cross_status`]: the short/long line names in both languages, plus
/// the full "equal/flat" text per language (which differs per indicator — EMA's eps
/// band, SMA's "equal", Ichimoku's "crossing/flat").
pub(crate) struct CrossLabels<'a> {
    pub short_ja: &'a str,
    pub long_ja: &'a str,
    pub short_en: &'a str,
    pub long_en: &'a str,
    pub equal_ja: &'a str,
    pub equal_en: &'a str,
}

/// The localized golden/dead-cross status line, shared by the EMA, SMA and Ichimoku
/// renderers (SOT §4.2). Golden/dead wording follows one template parameterized by
/// the short/long line names; the `Equal` text is supplied in full.
pub(crate) fn cross_status(lang: &str, ordering: std::cmp::Ordering, l: &CrossLabels) -> String {
    use std::cmp::Ordering::{Greater, Less};
    match (lang == "ja", ordering) {
        (true, Greater) => format!(
            "📈 ゴールデンクロス進行中（{}が{}を上回る）",
            l.short_ja, l.long_ja
        ),
        (true, Less) => format!(
            "📉 デッドクロス進行中（{}が{}を下回る）",
            l.short_ja, l.long_ja
        ),
        (true, _) => l.equal_ja.to_string(),
        (false, Greater) => {
            format!(
                "📈 Golden cross in progress ({} above {})",
                l.short_en, l.long_en
            )
        }
        (false, Less) => format!(
            "📉 Dead cross in progress ({} below {})",
            l.short_en, l.long_en
        ),
        (false, _) => l.equal_en.to_string(),
    }
}

pub fn collect_main_info_lines(config: &Config, guard: &TechnicalDataGuard) -> Vec<String> {
    let lang = config.lang.as_str();
    let mut lines = Vec::new();

    if (config.buy_rsi - crate::config::Config::default().buy_rsi).abs() > f64::EPSILON {
        let msg = match lang {
            "ja" => format!(
                "🔧 --buy-rsi={:.2} を指定 → RSIが{:.2}以下で買い圏とみなされます",
                config.buy_rsi, config.buy_rsi
            ),
            _ => format!(
                "🔧 --buy-rsi={:.2} set → RSI at or below {:.2} is treated as a buy zone",
                config.buy_rsi, config.buy_rsi
            ),
        };
        lines.push(format!("{}", msg.red()));
    }
    if (config.sell_rsi - crate::config::Config::default().sell_rsi).abs() > f64::EPSILON {
        let msg = match lang {
            "ja" => format!(
                "🔧 --sell-rsi={:.2} を指定 → RSIが{:.2}以上で売り圏とみなされます",
                config.sell_rsi, config.sell_rsi
            ),
            _ => format!(
                "🔧 --sell-rsi={:.2} set → RSI at or above {:.2} is treated as a sell zone",
                config.sell_rsi, config.sell_rsi
            ),
        };
        lines.push(format!("{}", msg.red()));
    }
    if (config.macd_diff_low - crate::config::Config::default().macd_diff_low).abs() > f64::EPSILON
    {
        let msg = match lang {
            "ja" => format!(
                "🔧 --macd-diff-low={:.2} を指定 → MACD差が{:.2}未満ならスコアを中立に補正します",
                config.macd_diff_low, config.macd_diff_low
            ),
            _ => format!(
                "🔧 --macd-diff-low={:.2} set → MACD diff below {:.2} corrects score to neutral",
                config.macd_diff_low, config.macd_diff_low
            ),
        };
        lines.push(format!("{}", msg.red()));
    }
    if (config.macd_diff_mid - crate::config::Config::default().macd_diff_mid).abs() > f64::EPSILON
    {
        let msg = match lang {
            "ja" => format!(
                "🔧 --macd-diff-mid={:.2} を指定 → MACD差が{:.2}以上でスコアを強化します",
                config.macd_diff_mid, config.macd_diff_mid
            ),
            _ => format!(
                "🔧 --macd-diff-mid={:.2} set → MACD diff at or above {:.2} boosts the score",
                config.macd_diff_mid, config.macd_diff_mid
            ),
        };
        lines.push(format!("{}", msg.red()));
    }

    lines.push(String::new());

    // Honest degraded-source warning. When the snapshot was adopted whole from a
    // fallback vendor (primary provider failed), tell the user plainly — one
    // clearly-marked line. Shared by CLI and Web (both render these lines).
    if let Some(note) = guard.get_source_note() {
        let msg = match lang {
            "ja" => format!("⚠️ 代替データ元を使用: {}", note),
            _ => format!("⚠️ Fallback data source in use: {}", note),
        };
        lines.push(format!("{}", msg.yellow()));
    }

    match lang {
        "ja" => lines.push(format!(
            "📊 銘柄: {}（{}）",
            guard.get_name(),
            guard.get_ticker()
        )),
        _ => lines.push(format!(
            "📊 Ticker: {} ({})",
            guard.get_name(),
            guard.get_ticker()
        )),
    }
    if config.analysis_mode != crate::config::AnalysisMode::Daily {
        match lang {
            "ja" => lines.push(format!(
                "🕒 分析モード: {}（{}）",
                config.analysis_mode.label("ja"),
                config.analysis_mode.context("ja")
            )),
            _ => lines.push(format!(
                "🕒 Analysis mode: {} ({})",
                config.analysis_mode.label("en"),
                config.analysis_mode.context("en")
            )),
        }
    }

    let lbl = utils::freshness_labels(lang);
    lines.push(format!(
        "{}: {}",
        lbl[0],
        utils::format_analysis_time(guard, lang)
    ));
    lines.push(format!(
        "{}: {}",
        lbl[1],
        utils::format_market_data_latest_time(guard, lang)
    ));
    lines.push(format!(
        "{}: {}",
        lbl[2],
        utils::format_latest_observed_price_bar(config, guard)
    ));
    lines.push(format!(
        "{}: {}",
        lbl[3],
        utils::format_indicator_latest_bar(config, guard)
    ));

    if let Some(latest) = guard.get_latest_observed_price() {
        match lang {
            "ja" => lines.push(format!("💰 最新取得価格: {:.2}", latest)),
            _ => lines.push(format!("💰 Latest price: {:.2}", latest)),
        }
    } else {
        let (lbl_no, lbl_close) = match lang {
            "ja" => ("💰 最新取得価格: 取得なし", "💰 指標計算最終足終値"),
            _ => ("💰 Latest price: not available", "💰 Indicator bar close"),
        };
        lines.push(lbl_no.to_string());
        lines.push(format!("{}: {:.2}", lbl_close, guard.get_close()));
    }
    lines.push(format!(
        "💰 {}: {:.2}",
        config.analysis_mode.previous_close_label_l(&config.lang),
        guard.get_previous_close()
    ));

    let (diff, percent) = utils::displayed_price_diff(guard);
    let diff_str = colored_price_diff(diff, percent);
    lines.push(format!(
        "📊 {}: {}",
        config.analysis_mode.price_diff_label_l(&config.lang),
        diff_str
    ));

    // Volume section
    match guard.get_latest_volume() {
        None => {
            let lbl = match lang {
                "ja" => "📈 出来高: データなし",
                _ => "📈 Volume: unavailable",
            };
            lines.push(lbl.to_string());
        }
        Some(vol) => {
            let vol_str = utils::format_volume_display(vol);
            match lang {
                "ja" => lines.push(format!("📈 出来高: {}株", vol_str)),
                _ => lines.push(format!("📈 Volume: {} shares", vol_str)),
            }
            if let (Some(avg), Some(ratio)) = (guard.get_avg_volume(), guard.get_volume_ratio()) {
                let avg_str = utils::format_volume_display(avg);
                match lang {
                    "ja" => lines.push(format!(
                        "📈 平均出来高 ({}本): {}株 / 倍率: {:.2}x",
                        config.sma_long_period, avg_str, ratio
                    )),
                    _ => lines.push(format!(
                        "📈 Avg volume ({} bars): {} shares / ratio: {:.2}x",
                        config.sma_long_period, avg_str, ratio
                    )),
                }
                let comment = utils::volume_comment_str(ratio, guard.get_price_diff(), lang);
                lines.push(format!("   {comment}"));
            }
        }
    }

    let applied = guard.get_macd() < 0.0 && guard.get_macd() > guard.get_signal();
    lines.push(
        utils::macd_minus_policy_label(config.macd_minus_ok, applied, &config.lang).to_string(),
    );
    lines.push(String::new());

    lines
}

fn display_main_info(config: &Config, guard: &TechnicalDataGuard) {
    for line in collect_main_info_lines(config, guard) {
        println!("{}", line);
    }
}

pub fn render_basic(config: &Config, guard: &TechnicalDataGuard) -> AnalysisResult {
    let rsi = guard.get_rsi();
    let macd = guard.get_macd();
    let signal = guard.get_signal();
    let prev_macd = guard.get_prev_macd();
    let prev_signal = guard.get_prev_signal();
    let weight = config.weight_basic;
    let lang = config.lang.as_str();

    let score = guard.get_signal_score();
    let signal_strength = guard.get_signal_strength();
    let adjusted_score = score * weight;

    let mut description_lines: Vec<String> = Vec::new();

    match lang {
        "ja" => description_lines.push("基本テクニカル分析（MACDとRSIによる評価）".to_string()),
        _ => {
            description_lines.push("Basic technical analysis (MACD and RSI evaluation)".to_string())
        }
    }
    description_lines.push(format!("📈 MACD: {:.4} / Signal: {:.4}", macd, signal));
    description_lines.push(format!("📊 RSI: {:.2}", rsi));

    match (
        prev_macd < prev_signal && macd > signal,
        macd > signal,
        prev_macd > prev_signal && macd < signal,
        macd < signal,
    ) {
        (true, _, _, _) => match lang {
            "ja" => description_lines
                .push("⚠️ MACDがゴールデンクロス → 上昇トレンド転換の可能性".to_string()),
            _ => description_lines
                .push("⚠️ MACD golden cross → possible bullish trend reversal".to_string()),
        },
        (_, true, _, _) => match lang {
            "ja" => description_lines.push(
                "⚠️ MACDがSignalを上回る状態が継続 → 上昇トレンドが維持されている可能性"
                    .to_string(),
            ),
            _ => description_lines
                .push("⚠️ MACD remains above signal → uptrend may be sustained".to_string()),
        },
        (_, _, true, _) => match lang {
            "ja" => description_lines
                .push("⚠️ MACDがデッドクロス → 下落トレンド転換の可能性".to_string()),
            _ => description_lines
                .push("⚠️ MACD dead cross → possible bearish trend reversal".to_string()),
        },
        (_, _, _, true) => match lang {
            "ja" => description_lines
                .push("⚠️ MACDがSignalを下回る状態が継続 → 弱含みトレンドが継続中".to_string()),
            _ => description_lines
                .push("⚠️ MACD remains below signal → weak trend continuing".to_string()),
        },
        _ => {}
    }

    let macd_diff = macd - signal;
    match macd_diff {
        d if d >= 5.0 => match lang {
            "ja" => description_lines.push(format!(
                "⚠️ MACDがSignalより大幅に上回っています（+{:.2}）→ 過熱感がある可能性があります",
                macd_diff
            )),
            _ => description_lines.push(format!(
                "⚠️ MACD significantly above signal (+{:.2}) → possible overheating",
                macd_diff
            )),
        },
        d if d <= -5.0 => match lang {
            "ja" => description_lines.push(format!(
                "⚠️ MACDがSignalより大幅に下回っています（{:.2}）→ 割安感がある可能性があります",
                macd_diff
            )),
            _ => description_lines.push(format!(
                "⚠️ MACD significantly below signal ({:.2}) → possible undervaluation",
                macd_diff
            )),
        },
        _ => {}
    }
    let close = guard.get_close();
    if close.abs() > f64::EPSILON {
        let diff_pct = macd_diff / close * 100.0;
        description_lines.push(deviation_line(
            lang,
            "MACD-Signal",
            "MACD-Signal",
            "対終値比",
            "vs close",
            diff_pct,
        ));
    }

    match rsi {
        r if r <= 5.0 => match lang {
            "ja" => description_lines
                .push("⚠️ RSIが 0% に近い極端な売られすぎ → 反発に警戒".to_string()),
            _ => description_lines
                .push("⚠️ RSI near 0% — extreme oversold → watch for rebound".to_string()),
        },
        r if r >= 95.0 => match lang {
            "ja" => description_lines
                .push("⚠️ RSIが 100% に近い極端な買われすぎ → 反転下落に注意".to_string()),
            _ => description_lines
                .push("⚠️ RSI near 100% — extreme overbought → watch for reversal".to_string()),
        },
        _ => {}
    }

    match signal_strength {
        SignalStrength::StrongBuy => description_lines.push(
            match (lang, rsi < 30.0) {
                ("ja", true) => {
                    "🟢 [基本スコア:+2] RSIが極端に割安 → 強い買いシグナル → スコア+2加点"
                }
                ("ja", false) => "🟢 [基本スコア:+2] MACDが強い上昇トレンド → スコア+2加点",
                (_, true) => {
                    "🟢 [basic score:+2] RSI extremely oversold → strong buy signal → score +2"
                }
                _ => "🟢 [basic score:+2] MACD strong uptrend → score +2",
            }
            .to_string(),
        ),
        SignalStrength::Buy => description_lines.push(
            match (lang, rsi < 40.0) {
                ("ja", true) => "🟢 [基本スコア:+1] RSIが割安圏 → 買いシグナル → スコア+1加点",
                ("ja", false) => "🟢 [基本スコア:+1] MACDが上昇傾向 → スコア+1加点",
                (_, true) => "🟢 [basic score:+1] RSI in oversold zone → buy signal → score +1",
                _ => "🟢 [basic score:+1] MACD rising → score +1",
            }
            .to_string(),
        ),
        SignalStrength::Neutral => description_lines.push(
            match lang {
                "ja" => "⚪️ [基本スコア:0] RSI・MACDともに中立 → スコアなし",
                _ => "⚪️ [basic score:0] RSI and MACD both neutral → no score",
            }
            .to_string(),
        ),
        SignalStrength::Sell => description_lines.push(
            match (lang, rsi > 60.0) {
                ("ja", true) => "🔴 [基本スコア:-1] RSIが割高圏 → 売りシグナル → スコア-1減点",
                ("ja", false) => "🔴 [基本スコア:-1] MACDが下降傾向 → スコア-1減点",
                (_, true) => "🔴 [basic score:-1] RSI in overbought zone → sell signal → score -1",
                _ => "🔴 [basic score:-1] MACD declining → score -1",
            }
            .to_string(),
        ),
        SignalStrength::StrongSell => description_lines.push(
            match (lang, rsi > 70.0) {
                ("ja", true) => {
                    "🔴 [基本スコア:-2] RSIが極端に割高 → 強い売りシグナル → スコア-2減点"
                }
                ("ja", false) => "🔴 [基本スコア:-2] MACDが強い下降トレンド → スコア-2減点",
                (_, true) => {
                    "🔴 [basic score:-2] RSI extremely overbought → strong sell signal → score -2"
                }
                _ => "🔴 [basic score:-2] MACD strong downtrend → score -2",
            }
            .to_string(),
        ),
    }

    description_lines.push(adjusted_score_line(lang, adjusted_score, score, weight));

    AnalysisResult {
        indicator_name: match lang {
            "ja" => "基本テクニカル分析",
            _ => "Basic Technical Analysis",
        }
        .to_string(),
        description: description_lines,
        score: Some(score),
    }
}

fn display_analysis_result(result: &AnalysisResult) {
    for line in &result.description {
        println!("{}", line);
    }
    println!();
}

/// Returns (mark_emoji, action_text) for a given percentage and stance direction.
/// Thresholds: 90+/61-89/40-60/20-39/0-19 → strong/moderate/neutral/weak/avoid.
fn verdict_mark_and_text(
    percent: u8,
    is_buy_direction: bool,
    lang: &str,
) -> (&'static str, &'static str) {
    match (percent, is_buy_direction, lang) {
        (90..=u8::MAX, true, "ja") => ("🟢", "積極的に買う"),
        (61..=89, true, "ja") => ("🟡", "買う"),
        (40..=60, _, "ja") => ("⚪️", "中立"),
        (20..=39, true, "ja") => ("🟠", "買いを推奨しない"),
        (_, true, "ja") => ("🔴", "買わない"),
        (90..=u8::MAX, false, "ja") => ("🟢", "積極的に売る"),
        (61..=89, false, "ja") => ("🟡", "売る"),
        (20..=39, false, "ja") => ("🟠", "売りを推奨しない"),
        (_, false, "ja") => ("🔴", "売らない"),
        (90..=u8::MAX, true, _) => ("🟢", "Strong buy"),
        (61..=89, true, _) => ("🟡", "Buy"),
        (40..=60, _, _) => ("⚪️", "Neutral"),
        (20..=39, true, _) => ("🟠", "Avoid buying"),
        (_, true, _) => ("🔴", "Do not buy"),
        (90..=u8::MAX, false, _) => ("🟢", "Strong sell"),
        (61..=89, false, _) => ("🟡", "Sell"),
        (20..=39, false, _) => ("🟠", "Avoid selling"),
        (_, false, _) => ("🔴", "Do not sell"),
    }
}

fn compose_buyer_seller_lines(
    snap: &FinalScoreSnapshot,
    stance: &Stance,
    has_weight: bool,
    weight_abs: f64,
    include_gauge: bool,
    lang: &str,
) -> Vec<String> {
    let mut lines = Vec::new();
    let buyer_percent: u8 = if !has_weight {
        50
    } else {
        let p = ((weight_abs + snap.total_score).clamp(0.0, 2.0 * weight_abs) / (2.0 * weight_abs))
            * 100.0;
        p.round().clamp(0.0, 100.0) as u8
    };
    let seller_percent: u8 = 100u8.saturating_sub(buyer_percent);
    let (percent, is_buy_direction) = match stance {
        Stance::Buyer => (buyer_percent, true),
        _ => (seller_percent, false),
    };
    let (mark, action_text) = verdict_mark_and_text(percent, is_buy_direction, lang);
    match lang {
        "ja" => lines.push(format!(
            "→ 判定: {mark} {action} {pct}%",
            action = action_text,
            pct = percent
        )),
        _ => lines.push(format!(
            "→ Verdict: {mark} {action} {pct}%",
            action = action_text,
            pct = percent
        )),
    }
    if include_gauge {
        let gauge = match (stance, lang) {
            (Stance::Buyer, "ja") => render_unipolar_gauge_rtl(
                percent,
                "買い 100％",
                "0％ 買わない",
                UNIPOLAR_GAUGE_WIDTH,
            ),
            (Stance::Buyer, _) => {
                render_unipolar_gauge_rtl(percent, "Buy 100%", "0% No buy", UNIPOLAR_GAUGE_WIDTH)
            }
            (Stance::Seller, "ja") => render_unipolar_gauge_rtl(
                percent,
                "売り 100％",
                "0％ 売らない",
                UNIPOLAR_GAUGE_WIDTH,
            ),
            (Stance::Seller, _) => {
                render_unipolar_gauge_rtl(percent, "Sell 100%", "0% No sell", UNIPOLAR_GAUGE_WIDTH)
            }
            _ => String::new(),
        };
        if !gauge.is_empty() {
            let colored_g = match mark {
                "🟢" => gauge.replace("█", &"█".green().to_string()),
                "🟡" => gauge.replace("█", &"█".yellow().to_string()),
                "⚪️" => gauge.replace("█", &"█".white().to_string()),
                "🟠" => gauge.replace("█", &"█".truecolor(255, 165, 0).to_string()),
                "🔴" => gauge.replace("█", &"█".red().to_string()),
                _ => gauge,
            };
            lines.push(colored_g);
        }
    }
    lines
}

fn compose_holder_lines(
    snap: &FinalScoreSnapshot,
    has_weight: bool,
    include_gauge: bool,
    lang: &str,
) -> Vec<String> {
    let mut lines = Vec::new();
    let holder_pct: i32 = if has_weight {
        (snap.score_ratio * 100.0).round() as i32
    } else {
        0
    };
    let action_text = utils::classify_score(snap.score_ratio, lang);
    match lang {
        "ja" => lines.push(format!(
            "→ 判定: {action} スコア比率 {:+}%",
            holder_pct,
            action = action_text
        )),
        _ => lines.push(format!(
            "→ Verdict: {action} score ratio {:+}%",
            holder_pct,
            action = action_text
        )),
    }
    if include_gauge {
        lines.push(render_bipolar_gauge_lr(
            snap.score_ratio,
            BIPOLAR_GAUGE_WIDTH,
            lang,
        ));
    }
    lines
}

pub fn compose_final_score_lines_stance(
    snap: &FinalScoreSnapshot,
    stance: &Stance,
    include_gauge: bool,
    lang: &str,
) -> Vec<String> {
    let weight_abs = if snap.total_weight.is_finite() {
        snap.total_weight.abs()
    } else {
        0.0
    };
    let has_weight = weight_abs > f64::EPSILON;

    let mut lines = Vec::new();
    match lang {
        "ja" => {
            lines.push(format!(
                "🧮 総合スコア: {s:.1} ({w:.1}〜-{w:.1})の範囲",
                s = snap.total_score,
                w = weight_abs
            ));
            lines.push(format!(
                "トータルスコア（スタンス：{}）",
                stance_caption(stance)
            ));
        }
        _ => {
            lines.push(format!(
                "🧮 Total score: {s:.1} (range {w:.1} to -{w:.1})",
                s = snap.total_score,
                w = weight_abs
            ));
            lines.push(format!(
                "Overall score (stance: {})",
                stance_caption(stance)
            ));
        }
    }

    match stance {
        Stance::Buyer | Stance::Seller => {
            lines.extend(compose_buyer_seller_lines(
                snap,
                stance,
                has_weight,
                weight_abs,
                include_gauge,
                lang,
            ));
        }
        Stance::Holder => {
            lines.extend(compose_holder_lines(snap, has_weight, include_gauge, lang));
        }
    }

    lines.push(String::new());
    lines
}

fn render_unipolar_gauge_rtl(
    percent: u8,
    left_label: &str,
    right_label: &str,
    width: usize,
) -> String {
    let w = width.max(10);
    // Round-to-nearest: add half of 100 before integer division to avoid truncation bias.
    let filled = (((percent as usize) * w) + 50) / 100;
    let empty = w - filled;

    let bar = format!("[{}{}]", ".".repeat(empty), "█".repeat(filled));
    format!(
        "{left} {bar} {right}",
        left = left_label,
        right = right_label
    )
}

#[allow(clippy::needless_range_loop)]
fn render_bipolar_gauge_lr(score_ratio: f64, width: usize, lang: &str) -> String {
    let w = width.max(12);
    // mid is the center divider index (v[mid] = '|').
    // Left half: indices 0..mid (positive fill grows right-to-left toward mid).
    // Right half: indices (mid+1)..w (negative fill grows left-to-right from mid+1).
    // The +1 offset keeps the center '|' character intact in both fill paths.
    let mid = w / 2;

    // blocks: how many cells to fill, capped at mid so fill never crosses the center.
    let blocks = ((score_ratio.abs() * mid as f64).round() as usize).min(mid);

    let mut v = vec!['.'; w];
    if mid < w {
        v[mid] = '|';
    }

    match score_ratio {
        s if s > 0.0 => {
            for i in (mid.saturating_sub(blocks))..mid {
                v[i] = '█';
            }
        }
        s if s < 0.0 => {
            for i in (mid + 1)..(mid + 1 + blocks) {
                if i < w {
                    v[i] = '█';
                }
            }
        }
        _ => {}
    }

    let color = utils::get_color_for_score(score_ratio);
    let bar: String = v
        .into_iter()
        .map(|c| {
            if c == '█' {
                match color {
                    "green" => c.to_string().green().to_string(),
                    "yellow" => c.to_string().yellow().to_string(),
                    "orange" => c.to_string().truecolor(255, 165, 0).to_string(),
                    "red" => c.to_string().red().to_string(),
                    "white" => c.to_string().white().to_string(),
                    _ => c.to_string(),
                }
            } else {
                c.to_string()
            }
        })
        .collect();

    match lang {
        "ja" => format!("買い+100％[{bar}] -100% 売り"),
        _ => format!("Buy+100%[{bar}] -100% Sell"),
    }
}

fn stance_caption(s: &Stance) -> &'static str {
    match s {
        Stance::Buyer => "Buyer",
        Stance::Holder => "Holder",
        Stance::Seller => "Seller",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AnalysisMode;
    use crate::utils;

    fn intraday_guard_with_market_data_latest_time() -> TechnicalDataGuard {
        let mut guard = TechnicalDataGuard::new("3774.T".to_string(), "2026-05-12".to_string());
        guard.set_datetime("2026-05-12 13:30");
        guard.set_latest_observed_price(2955.50);
        guard.set_market_data_latest_time("2026-05-12 14:11");
        guard
    }

    #[test]
    fn intraday_latest_observed_price_bar_uses_market_data_latest_bucket() {
        let config = Config {
            analysis_mode: AnalysisMode::Intraday30m,
            ..Config::default()
        };
        let guard = intraday_guard_with_market_data_latest_time();

        assert_eq!(
            utils::format_latest_observed_price_bar(&config, &guard),
            "2026-05-12 14:00 (30min bar)"
        );
        assert_eq!(
            utils::format_indicator_latest_bar(&config, &guard),
            "2026-05-12 13:30 (30min bar)"
        );
    }

    #[test]
    fn intraday_15m_latest_observed_price_bar_uses_market_data_latest_bucket() {
        let config = Config {
            analysis_mode: AnalysisMode::Intraday15m,
            ..Config::default()
        };
        let guard = intraday_guard_with_market_data_latest_time();

        assert_eq!(
            utils::format_latest_observed_price_bar(&config, &guard),
            "2026-05-12 14:00 (15min bar)"
        );
        assert_eq!(
            utils::format_indicator_latest_bar(&config, &guard),
            "2026-05-12 13:30 (15min bar)"
        );
    }

    #[test]
    fn intraday_5m_latest_observed_price_bar_uses_market_data_latest_bucket() {
        let config = Config {
            analysis_mode: AnalysisMode::Intraday5m,
            ..Config::default()
        };
        let guard = intraday_guard_with_market_data_latest_time();

        assert_eq!(
            utils::format_latest_observed_price_bar(&config, &guard),
            "2026-05-12 14:10 (5min bar)"
        );
        assert_eq!(
            utils::format_indicator_latest_bar(&config, &guard),
            "2026-05-12 13:30 (5min bar)"
        );
    }

    /// Shipping-inspection item 10 (user-facing half): a snapshot adopted from a
    /// degraded fallback source (`source_note` set) surfaces a clearly-marked ⚠️
    /// warning line in the rendered output — in both languages. Deterministic;
    /// no network.
    #[test]
    fn degraded_source_note_renders_visible_warning() {
        for (lang, needle) in [
            ("", "Fallback data source in use"),
            ("ja", "代替データ元を使用"),
        ] {
            let config = Config {
                lang: lang.to_string(),
                ..Config::default()
            };
            let mut guard = TechnicalDataGuard::new("AAPL".to_string(), "2026-07-02".to_string());
            guard.set_source_note("Yahoo unavailable — figures from Stooq (fallback; may differ)");
            let out = collect_main_info_lines(&config, &guard).join("\n");
            assert!(
                out.contains(needle),
                "lang={lang:?}: expected the degraded-source warning, got:\n{out}"
            );
            assert!(
                out.contains('⚠'),
                "lang={lang:?}: the warning must be clearly marked"
            );
        }
    }

    #[test]
    fn intraday_missing_latest_observed_price_bar_is_not_invented() {
        let config = Config {
            analysis_mode: AnalysisMode::Intraday30m,
            ..Config::default()
        };
        let mut guard = TechnicalDataGuard::new("3774.T".to_string(), "2026-05-12".to_string());
        guard.set_datetime("2026-05-12 13:30");
        guard.set_market_data_latest_time("2026-05-12 14:11");

        assert_eq!(
            utils::format_latest_observed_price_bar(&config, &guard),
            "not available"
        );
        assert_eq!(
            utils::format_indicator_latest_bar(&config, &guard),
            "2026-05-12 13:30 (30min bar)"
        );
    }
}
