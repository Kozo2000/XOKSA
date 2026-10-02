//! Composite score calculation and gauge conversion

use super::indicators::{get_extension_evaluator as get_indicator_evaluator, ExtensionEvaluator};
use super::types::{FinalScoreSnapshot, TechnicalDataGuard, MAX_ABS_INDICATOR_SCORE};
use crate::config::{Config, ExtensionIndicator};

fn extension_score_and_weight(
    config: &Config,
    guard: &TechnicalDataGuard,
    ext: &ExtensionIndicator,
) -> Option<(f64, f64)> {
    match ext {
        ExtensionIndicator::Ema => guard
            .get_ema_score()
            .map(|score| (score, config.weight_ema)),
        ExtensionIndicator::Sma => guard
            .get_sma_score()
            .map(|score| (score, config.weight_sma)),
        ExtensionIndicator::Roc => guard
            .get_roc_score()
            .map(|score| (score, config.weight_roc)),
        ExtensionIndicator::Adx => guard
            .get_adx_score()
            .map(|score| (score, config.weight_adx)),
        ExtensionIndicator::Stochastics => guard
            .get_stochastics_score()
            .map(|score| (score, config.weight_stochastics)),
        ExtensionIndicator::Bollinger => guard
            .get_bollinger_score()
            .map(|score| (score, config.weight_bollinger)),
        ExtensionIndicator::Fibonacci => guard
            .get_fibonacci_score()
            .map(|score| (score, config.weight_fibonacci)),
        ExtensionIndicator::Vwap => guard
            .get_vwap_score()
            .map(|score| (score, config.weight_vwap)),
        ExtensionIndicator::Ichimoku => guard
            .get_ichimoku_score()
            .map(|score| (score, config.weight_ichimoku)),
    }
}

pub fn calculate_final_score_snapshot(
    config: &Config,
    guard: &TechnicalDataGuard,
) -> FinalScoreSnapshot {
    let total_score = calculate_final_score(config, guard);

    let mut sum_weights = config.weight_basic;
    for ext in config.analysis_extensions() {
        if let Some((_, weight)) = extension_score_and_weight(config, guard, ext) {
            sum_weights += weight;
        }
    }

    let total_weight = MAX_ABS_INDICATOR_SCORE * sum_weights;
    let score_ratio = if total_weight != 0.0 {
        total_score / total_weight
    } else {
        0.0
    };

    FinalScoreSnapshot {
        total_score,
        total_weight,
        score_ratio,
    }
}

fn calculate_final_score(config: &Config, guard: &TechnicalDataGuard) -> f64 {
    let mut total_score = guard.get_signal_score() * config.weight_basic;

    for ext in config.analysis_extensions() {
        if let Some((score, weight)) = extension_score_and_weight(config, guard, ext) {
            total_score += score * weight;
        }
    }

    total_score
}

pub fn calculate_score_gauge(config: &Config, guard: &TechnicalDataGuard) -> String {
    let snap = calculate_final_score_snapshot(config, guard);
    let ratio = if snap.total_weight > 0.0 {
        (snap.total_score / snap.total_weight).clamp(-1.0, 1.0)
    } else {
        0.0
    };

    let width = 25usize;
    let center = width;
    let marker = ((ratio + 1.0) * width as f64).round() as usize;
    let marker = marker.min(width * 2);

    let mut bar = String::with_capacity(width * 2 + 1);
    for i in 0..=(width * 2) {
        let ch = match (i == center, i == marker) {
            (true, _) => '|',
            (false, true) => '█',
            (false, false) => '.',
        };
        bar.push(ch);
    }

    format!("{} ({:+.0}%)", bar, ratio * 100.0)
}

pub fn get_extension_evaluator(_config: &Config, indicator: &str) -> Option<ExtensionEvaluator> {
    let normalized = indicator.trim().to_ascii_lowercase();
    let parsed = match normalized.as_str() {
        "ema" => ExtensionIndicator::Ema,
        "sma" => ExtensionIndicator::Sma,
        "bollinger" | "bollingerbands" => ExtensionIndicator::Bollinger,
        "roc" => ExtensionIndicator::Roc,
        "adx" => ExtensionIndicator::Adx,
        "stochastics" | "stochastic" => ExtensionIndicator::Stochastics,
        "fibonacci" => ExtensionIndicator::Fibonacci,
        "vwap" => ExtensionIndicator::Vwap,
        "ichimoku" => ExtensionIndicator::Ichimoku,
        _ => return None,
    };

    let f: ExtensionEvaluator = get_indicator_evaluator(&parsed);
    Some(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ExtensionIndicator};
    use crate::technical::types::TechnicalDataGuard;

    fn guard_with_signal(score: f64) -> TechnicalDataGuard {
        let mut g = TechnicalDataGuard::new("TEST".to_string(), "2026-01-01".to_string());
        g.set_signal_score(score);
        g
    }

    // ── Layer 3: Score aggregation and summation ──────────────────────────────

    #[test]
    fn snapshot_basic_only_neutral_score() {
        // signal_score=0, weight_basic=1 → total=0, total_weight=2, ratio=0
        // (weight_basic pinned to 1.0 so the arithmetic is independent of the
        // shipped default, which is 2.0)
        let config = Config {
            weight_basic: 1.0,
            ..Config::default()
        };
        let guard = guard_with_signal(0.0);
        let snap = calculate_final_score_snapshot(&config, &guard);
        assert!((snap.total_score).abs() < f64::EPSILON);
        assert!((snap.total_weight - 2.0).abs() < f64::EPSILON);
        assert!((snap.score_ratio).abs() < f64::EPSILON);
    }

    #[test]
    fn snapshot_basic_only_max_positive() {
        // signal_score=2, weight_basic=1 → ratio = 2/2 = 1.0
        let config = Config {
            weight_basic: 1.0,
            ..Config::default()
        };
        let guard = guard_with_signal(2.0);
        let snap = calculate_final_score_snapshot(&config, &guard);
        assert!((snap.score_ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn snapshot_with_ema_extension() {
        // signal_score=1, ema_score=2, weights all 1.0
        // total_score = 1*1 + 2*1 = 3
        // sum_weights = 1+1 = 2, total_weight = 4
        // score_ratio = 3/4 = 0.75
        let config = Config {
            weight_basic: 1.0,
            enabled_extensions: vec![ExtensionIndicator::Ema],
            ..Config::default()
        };
        let mut guard = guard_with_signal(1.0);
        guard.set_ema_score(2.0);
        let snap = calculate_final_score_snapshot(&config, &guard);
        assert!((snap.total_score - 3.0).abs() < f64::EPSILON);
        assert!((snap.total_weight - 4.0).abs() < f64::EPSILON);
        assert!((snap.score_ratio - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn snapshot_ignores_extensions_without_scores() {
        let config = Config {
            weight_basic: 1.0,
            enabled_extensions: vec![ExtensionIndicator::Ema, ExtensionIndicator::Sma],
            ..Config::default()
        };
        let guard = guard_with_signal(2.0);

        let snap = calculate_final_score_snapshot(&config, &guard);

        assert!((snap.total_score - 2.0).abs() < f64::EPSILON);
        assert!((snap.total_weight - 2.0).abs() < f64::EPSILON);
        assert!((snap.score_ratio - 1.0).abs() < f64::EPSILON);
    }

    /// Feature-B invariant: deactivating an indicator (`active_extensions`) drops it
    /// from the SCORE but NOT from the computed/stored columns (which follow
    /// `enabled_extensions`). This is the whole point of the two sets.
    #[test]
    fn active_extensions_narrow_score_but_not_columns() {
        let mut config = Config {
            weight_basic: 1.0,
            enabled_extensions: vec![ExtensionIndicator::Ema],
            ..Config::default()
        };
        let mut guard = guard_with_signal(1.0);
        guard.set_ema_score(2.0);

        // Default (active = None = all enabled): basic + ema = 1 + 2 = 3.
        let full = calculate_final_score_snapshot(&config, &guard).total_score;
        assert!((full - 3.0).abs() < f64::EPSILON);

        // Deactivate Ema → score is basic only (1); Ema no longer contributes.
        config.active_extensions = Some(vec![]);
        let narrowed = calculate_final_score_snapshot(&config, &guard).total_score;
        assert!((narrowed - 1.0).abs() < f64::EPSILON);

        // ...but the machine record STILL carries the ema columns (from enabled).
        let snap = calculate_final_score_snapshot(&config, &guard);
        let json = crate::output::technical_json_value(&config, &guard, &snap);
        let keys = json.as_object().unwrap();
        assert!(
            keys.contains_key("ema_score"),
            "deactivating for analysis must NOT drop the stored column"
        );
    }

    #[test]
    fn gauge_contains_positive_100_percent() {
        let config = Config::default();
        let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-01-01".to_string());
        guard.set_signal_score(2.0);
        let gauge = calculate_score_gauge(&config, &guard);
        assert!(gauge.contains("+100%"));
    }

    #[test]
    fn gauge_contains_negative_100_percent() {
        let config = Config::default();
        let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-01-01".to_string());
        guard.set_signal_score(-2.0);
        let gauge = calculate_score_gauge(&config, &guard);
        assert!(gauge.contains("-100%"));
    }
}
