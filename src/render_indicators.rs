//! Rendering logic for extended technical indicators (render_ema through render_ichimoku)

use crate::config::{Config, ExtensionIndicator};
use crate::technical::types::{AnalysisResult, TechnicalDataGuard};
use crate::utils;

// The Ichimoku score used to need its wording softened on intraday bars whose gap
// was under 1%: the score compared a price difference against a fixed 2.0, so a
// ±2 could come from a separation of a fraction of a percent, and calling that
// "well above" would have overstated it. The score now reads the divergence rate,
// where ±2 requires 4% or more, so that state cannot arise and the workaround is
// gone — one wording path, from the same table every other indicator uses.

pub(crate) fn render_ema(config: &Config, guard: &TechnicalDataGuard) -> AnalysisResult {
    let weight = config.weight_ema;
    let lang = config.lang.as_str();
    let mut description_lines: Vec<String> = Vec::new();
    let short = guard.get_ema_short();
    let long = guard.get_ema_long();
    match lang {
        "ja" => {
            description_lines.push("📊 【EMA（指数平滑移動平均）】".to_string());
            description_lines.push(format!("短期EMA: {:.2} / 長期EMA: {:.2}", short, long));
        }
        _ => {
            description_lines.push("📊 [EMA (Exponential Moving Average)]".to_string());
            description_lines.push(format!("Short EMA: {:.2} / Long EMA: {:.2}", short, long));
        }
    }
    let close = guard.get_close();
    if close.abs() > f64::EPSILON {
        let diff_pct = (short - long) / close * 100.0;
        description_lines.push(crate::render::deviation_line(
            lang,
            "短期/長期EMA",
            "Short/Long EMA",
            "対終値比",
            "vs close",
            diff_pct,
        ));
    }

    // The two legs are compared by sign, as SMA and Ichimoku are — this line
    // reports where the lines sit, and the score line below reports the score.
    //
    // It used to call a price difference under ±0.01 "near equal → no score change",
    // which is a claim about the score made from a different measure: on a low-priced
    // instrument a 0.0075 gap is a 1.6% deviation, so the score was +1 while this
    // line said there was none. The same series scaled by 1,000 changed only the
    // wording. A renderer must not re-derive a classification the score already made.
    description_lines.push(crate::render::cross_status(
        lang,
        short
            .partial_cmp(&long)
            .unwrap_or(std::cmp::Ordering::Equal),
        &crate::render::CrossLabels {
            short_ja: "短期EMA",
            long_ja: "長期EMA",
            short_en: "short EMA",
            long_en: "long EMA",
            equal_ja: "➖ EMAが一致：クロス傾向なし",
            equal_en: "➖ EMAs equal: no cross tendency",
        },
    ));

    // An absent score stays absent. `unwrap_or(0.0)` rendered "not computed" as a
    // neutral 0 — the one distinction security-design §1 exists to keep.
    match guard.get_ema_score().map(|v| v.round() as i32) {
        Some(base_score) => {
            let adjusted = base_score as f64 * weight;
            // No `get_score_description` here: EMA is the one extension indicator
            // with no score-description table, so that call would land in the
            // catch-all and print "Score unavailable" for a score that exists. The
            // cross line above and the adjusted-score line below say it.
            description_lines.push(crate::render::adjusted_score_line(
                lang, adjusted, base_score, weight,
            ));
            AnalysisResult {
                indicator_name: "EMA".to_string(),
                description: description_lines,
                score: Some(base_score as f64),
            }
        }
        None => {
            description_lines.push(
                utils::get_score_description(&ExtensionIndicator::Ema, None, lang).to_string(),
            );
            AnalysisResult {
                indicator_name: "EMA".to_string(),
                description: description_lines,
                score: None,
            }
        }
    }
}

pub(crate) fn render_sma(config: &Config, guard: &TechnicalDataGuard) -> AnalysisResult {
    let short = guard.get_sma_short();
    let long = guard.get_sma_long();
    let weight = config.weight_sma;
    let lang = config.lang.as_str();
    let mut description_lines: Vec<String> = Vec::new();
    match lang {
        "ja" => {
            description_lines.push("📊 【SMA（単純移動平均）】".to_string());
            description_lines.push(format!("短期SMA: {:.2} / 長期SMA: {:.2}", short, long));
        }
        _ => {
            description_lines.push("📊 [SMA (Simple Moving Average)]".to_string());
            description_lines.push(format!("Short SMA: {:.2} / Long SMA: {:.2}", short, long));
        }
    }
    let close = guard.get_close();
    if close.abs() > f64::EPSILON {
        let diff_pct = (short - long) / close * 100.0;
        description_lines.push(crate::render::deviation_line(
            lang,
            "短期/長期SMA",
            "Short/Long SMA",
            "対終値比",
            "vs close",
            diff_pct,
        ));
    }

    description_lines.push(crate::render::cross_status(
        lang,
        short
            .partial_cmp(&long)
            .unwrap_or(std::cmp::Ordering::Equal),
        &crate::render::CrossLabels {
            short_ja: "短期SMA",
            long_ja: "長期SMA",
            short_en: "short SMA",
            long_en: "long SMA",
            equal_ja: "➖ SMAが一致：クロス傾向なし",
            equal_en: "➖ SMAs equal: no cross tendency",
        },
    ));

    match guard.get_sma_score().map(|v| v as i32) {
        Some(base_score) => {
            let adjusted_score = base_score as f64 * weight;
            description_lines.push(
                utils::get_score_description(&ExtensionIndicator::Sma, Some(base_score), lang)
                    .to_string(),
            );
            description_lines.push(crate::render::adjusted_score_line(
                lang,
                adjusted_score,
                base_score,
                weight,
            ));
            AnalysisResult {
                indicator_name: "SMA".to_string(),
                description: description_lines,
                score: Some(base_score as f64),
            }
        }
        None => {
            description_lines.push(
                utils::get_score_description(&ExtensionIndicator::Sma, None, lang).to_string(),
            );
            AnalysisResult {
                indicator_name: "SMA".to_string(),
                description: description_lines,
                score: None,
            }
        }
    }
}

pub(crate) fn render_adx(config: &Config, guard: &TechnicalDataGuard) -> AnalysisResult {
    let lang = config.lang.as_str();
    let mut description_lines: Vec<String> = Vec::new();
    match lang {
        "ja" => description_lines.push("📊 【ADX（平均方向性指数）】".to_string()),
        _ => description_lines.push("📊 [ADX (Average Directional Index)]".to_string()),
    }

    match guard.get_adx() {
        Some(adx) => {
            match lang {
                "ja" => description_lines.push(format!("現在のADX: {:.2}", adx)),
                _ => description_lines.push(format!("Current ADX: {:.2}", adx)),
            }
            match adx {
                a if a >= 50.0 => match lang {
                    "ja" => description_lines.push(format!(
                        "⚠️ ADXが50以上（{:.2}）→ 非常に強いトレンド → 反転リスクに警戒",
                        adx
                    )),
                    _ => description_lines.push(format!(
                        "⚠️ ADX ≥50 ({:.2}) → Very strong trend → watch for reversal risk",
                        adx
                    )),
                },
                a if a <= 10.0 => match lang {
                    "ja" => description_lines.push(format!(
                        "⚠️ ADXが10以下（{:.2}）→ トレンド不在（レンジ相場） → 仕掛け注意",
                        adx
                    )),
                    _ => description_lines.push(format!(
                        "⚠️ ADX ≤10 ({:.2}) → No trend (range-bound) → caution on entries",
                        adx
                    )),
                },
                _ => {}
            }
            match guard.get_adx_score().map(|v| v as i32) {
                Some(base_score) => {
                    let adjusted_score = base_score as f64 * config.weight_adx;
                    description_lines.push(
                        utils::get_score_description(
                            &ExtensionIndicator::Adx,
                            Some(base_score),
                            lang,
                        )
                        .to_string(),
                    );
                    description_lines.push(crate::render::adjusted_score_line(
                        lang,
                        adjusted_score,
                        base_score,
                        config.weight_adx,
                    ));
                    AnalysisResult {
                        indicator_name: "ADX".to_string(),
                        description: description_lines,
                        score: Some(base_score as f64),
                    }
                }
                None => {
                    description_lines.push(
                        utils::get_score_description(&ExtensionIndicator::Adx, None, lang)
                            .to_string(),
                    );
                    AnalysisResult {
                        indicator_name: "ADX".to_string(),
                        description: description_lines,
                        score: None,
                    }
                }
            }
        }
        None => {
            match lang {
                "ja" => description_lines.push("⚠️ ADXデータなし".to_string()),
                _ => description_lines.push("⚠️ ADX data unavailable".to_string()),
            }
            AnalysisResult {
                indicator_name: "ADX".to_string(),
                description: description_lines,
                score: None,
            }
        }
    }
}

pub(crate) fn render_roc(config: &Config, guard: &TechnicalDataGuard) -> AnalysisResult {
    let lang = config.lang.as_str();
    let mut description_lines: Vec<String> = Vec::new();
    match lang {
        "ja" => description_lines.push("📊  【ROC（変化率）】".to_string()),
        _ => description_lines.push("📊  [ROC (Rate of Change)]".to_string()),
    }

    match guard.get_roc() {
        Some(roc) => {
            match lang {
                "ja" => description_lines.push(format!(
                    "{}のROC: {:.2}%",
                    config.analysis_mode.period_context(lang, config.roc_period),
                    roc
                )),
                _ => description_lines.push(format!(
                    "ROC over {}: {:.2}%",
                    config.analysis_mode.period_context(lang, config.roc_period),
                    roc
                )),
            }
            match roc {
                r if r >= 15.0 => match lang {
                    "ja" => description_lines.push(format!(
                        "⚠️ ROCが+15%以上（{:.2}%）→ 短期的な過熱上昇、反落に警戒",
                        roc
                    )),
                    _ => description_lines.push(format!(
                        "⚠️ ROC ≥+15% ({:.2}%) → Short-term overheating, watch for pullback",
                        roc
                    )),
                },
                r if r <= -15.0 => match lang {
                    "ja" => description_lines.push(format!(
                        "⚠️ ROCが-15%以下（{:.2}%）→ パニック売りの可能性、反発に備えた注視を",
                        roc
                    )),
                    _ => description_lines.push(format!(
                        "⚠️ ROC ≤-15% ({:.2}%) → Possible panic selling, watch for rebound",
                        roc
                    )),
                },
                _ => {}
            }
            match guard.get_roc_score().map(|v| v as i32) {
                Some(base_score) => {
                    let adjusted_score = base_score as f64 * config.weight_roc;
                    description_lines.push(
                        utils::get_score_description(
                            &ExtensionIndicator::Roc,
                            Some(base_score),
                            lang,
                        )
                        .to_string(),
                    );
                    description_lines.push(crate::render::adjusted_score_line(
                        lang,
                        adjusted_score,
                        base_score,
                        config.weight_roc,
                    ));
                    AnalysisResult {
                        indicator_name: "ROC".to_string(),
                        description: description_lines,
                        score: Some(base_score as f64),
                    }
                }
                None => {
                    description_lines.push(
                        utils::get_score_description(&ExtensionIndicator::Roc, None, lang)
                            .to_string(),
                    );
                    AnalysisResult {
                        indicator_name: "ROC".to_string(),
                        description: description_lines,
                        score: None,
                    }
                }
            }
        }
        None => {
            match lang {
                "ja" => description_lines.push("⚠️ ROCデータが不足しています".to_string()),
                _ => description_lines.push("⚠️ ROC data insufficient".to_string()),
            }
            AnalysisResult {
                indicator_name: "ROC".to_string(),
                description: description_lines,
                score: None,
            }
        }
    }
}

pub(crate) fn render_stochastics(config: &Config, guard: &TechnicalDataGuard) -> AnalysisResult {
    let lang = config.lang.as_str();
    let indicator_name = match lang {
        "ja" => "ストキャスティクス",
        _ => "Stochastics",
    };
    let mut description_lines: Vec<String> = Vec::new();
    match lang {
        "ja" => description_lines.push("📊 【ストキャスティクス】".to_string()),
        _ => description_lines.push("📊 [Stochastics]".to_string()),
    }

    let k_opt = guard.get_stochastics_k();
    let d_opt = guard.get_stochastics_d();

    match (k_opt, d_opt) {
        (Some(k), Some(d)) => {
            match lang {
                "ja" => {
                    description_lines.push(format!("現在の%K: {:.2}% / 現在の%D: {:.2}%", k, d))
                }
                _ => {
                    description_lines.push(format!("Current %K: {:.2}% / Current %D: {:.2}%", k, d))
                }
            }
            if k == 0.0 && d == 0.0 {
                match lang {
                    "ja" => description_lines.push("⚠️ %Kおよび%Dが0.00%に張り付き → 極端な売られすぎ水準 → リバウンドの可能性あり（注目シグナル）".to_string()),
                    _ => description_lines.push("⚠️ %K and %D pinned at 0.00% → Extremely oversold → rebound possible (notable signal)".to_string()),
                }
            }
        }
        (Some(_), None) => match lang {
            "ja" => description_lines.push("⚠️ %Dデータが不足しています".to_string()),
            _ => description_lines.push("⚠️ %D data insufficient".to_string()),
        },
        (None, _) => match lang {
            "ja" => description_lines.push("⚠️ %Kデータが不足しています".to_string()),
            _ => description_lines.push("⚠️ %K data insufficient".to_string()),
        },
    }

    match guard.get_stochastics_score().map(|v| v as i32) {
        Some(base_score) => {
            let adjusted_score = base_score as f64 * config.weight_stochastics;
            description_lines.push(
                utils::get_score_description(
                    &ExtensionIndicator::Stochastics,
                    Some(base_score),
                    lang,
                )
                .to_string(),
            );
            description_lines.push(crate::render::adjusted_score_line(
                lang,
                adjusted_score,
                base_score,
                config.weight_stochastics,
            ));
            AnalysisResult {
                indicator_name: indicator_name.to_string(),
                description: description_lines,
                score: Some(base_score as f64),
            }
        }
        None => {
            description_lines.push(
                utils::get_score_description(&ExtensionIndicator::Stochastics, None, lang)
                    .to_string(),
            );
            AnalysisResult {
                indicator_name: indicator_name.to_string(),
                description: description_lines,
                score: None,
            }
        }
    }
}

pub(crate) fn render_bollinger(config: &Config, guard: &TechnicalDataGuard) -> AnalysisResult {
    let upper: f64 = guard.get_bb_upper();
    let lower: f64 = guard.get_bb_lower();
    let percent_b: f64 = guard.get_bb_percent_b();
    let bandwidth_pct: f64 = guard.get_bb_bandwidth();
    let weight: f64 = config.weight_bollinger;
    let lang = config.lang.as_str();
    let indicator_name = match lang {
        "ja" => "ボリンジャーバンド",
        _ => "Bollinger Bands",
    };

    let mut description_lines: Vec<String> = Vec::new();
    match lang {
        "ja" => {
            description_lines.push("📊 【ボリンジャーバンド】".to_string());
            description_lines.push(format!("上限 {:.2} / 下限 {:.2}", upper, lower));
        }
        _ => {
            description_lines.push("📊 [Bollinger Bands]".to_string());
            description_lines.push(format!("Upper {:.2} / Lower {:.2}", upper, lower));
        }
    }

    if (upper - lower).abs() < f64::EPSILON {
        match lang {
            "ja" => description_lines
                .push("⚠️ バンド幅が0に近いため、%b/帯幅の解釈に注意（計算不安定）".to_string()),
            _ => description_lines.push(
                "⚠️ Band width near 0 — interpret %b/bandwidth with caution (unstable)".to_string(),
            ),
        }
    }

    match lang {
        "ja" => description_lines.push(format!(
            "%b indicator: {:.2} / 帯幅(Bandwidth): {:.1}%",
            percent_b, bandwidth_pct
        )),
        _ => description_lines.push(format!(
            "%b indicator: {:.2} / Bandwidth: {:.1}%",
            percent_b, bandwidth_pct
        )),
    }
    let bw = guard.get_bb_bandwidth();
    let th = config.bb_bandwidth_squeeze_pct;

    match (lang, bw <= th) {
        ("ja", true) => description_lines.push(format!(
            "⚠️ スクイーズ進行中（帯幅が設定閾値 {:.1}% 以下）",
            th
        )),
        ("ja", false) => description_lines.push(format!(
            "ℹ️ 帯幅は設定閾値 {:.1}% を上回り、スクイーズ未発生",
            th
        )),
        (_, true) => description_lines.push(format!(
            "⚠️ Squeeze in progress (bandwidth ≤ threshold {:.1}%)",
            th
        )),
        _ => description_lines.push(format!(
            "ℹ️ Bandwidth above threshold {:.1}% — no squeeze",
            th
        )),
    }

    // The position line is read from the score, not re-derived from %B. Deciding it
    // by `%B > 1.0` while the score compares the price to the band inclusively made
    // the two disagree at the band itself: a close equal to the upper band scores -1
    // and the line still said "within bands → neutral". %B is still printed, because
    // it is the useful number; only the verdict comes from the score.
    //
    // This also retires a hint line that existed to paper over the same gap ("within
    // bands, upper-biased → score -1"). Under the old strict comparison it could
    // never fire — `%B ≤ 1` means the close is at or below the upper band, while a
    // -1 required it strictly above — so it was dead code that the boundary change
    // brought to life as a contradiction.
    let base_opt = guard.get_bollinger_score().map(|v| v as i32);
    let state_line = match (lang, base_opt) {
        ("ja", Some(b)) if b <= -1 => format!(
            "⚠️ 上限ブレイク（%b {:.2}）→ 伸び一巡後の反動に注意",
            percent_b
        ),
        ("ja", Some(b)) if b >= 1 => format!(
            "⚠️ 下限ブレイク（%b {:.2}）→ リバウンド/続落の分岐に注意",
            percent_b
        ),
        ("ja", Some(_)) => format!(
            "➡️ 指標計算最終足終値がバンド内（%b {:.2}）→ 中立",
            percent_b
        ),
        ("ja", None) => format!("⚠️ バンド位置の判定なし（%b {:.2}）", percent_b),
        (_, Some(b)) if b <= -1 => format!(
            "⚠️ Upper band break (%b {:.2}) → watch for reaction after extension",
            percent_b
        ),
        (_, Some(b)) if b >= 1 => format!(
            "⚠️ Lower band break (%b {:.2}) → watch for rebound vs. continued drop",
            percent_b
        ),
        (_, Some(_)) => format!("➡️ Close within bands (%b {:.2}) → neutral", percent_b),
        (_, None) => format!("⚠️ Band position unavailable (%b {:.2})", percent_b),
    };
    description_lines.push(state_line);

    match base_opt {
        Some(base) => {
            let adjusted = base as f64 * weight;

            description_lines.push(crate::render::adjusted_score_line(
                lang, adjusted, base, weight,
            ));
            AnalysisResult {
                indicator_name: indicator_name.to_string(),
                description: description_lines,
                score: Some(base as f64),
            }
        }
        None => {
            match lang {
                "ja" => description_lines.push("⚠️ ボリンジャーバンドスコア情報なし".to_string()),
                _ => description_lines.push("⚠️ Bollinger Bands score unavailable".to_string()),
            }
            AnalysisResult {
                indicator_name: indicator_name.to_string(),
                description: description_lines,
                score: None,
            }
        }
    }
}

pub(crate) fn render_fibonacci(config: &Config, guard: &TechnicalDataGuard) -> AnalysisResult {
    let weight = config.weight_fibonacci;
    let lang = config.lang.as_str();
    let mut description_lines: Vec<String> = Vec::new();
    match lang {
        "ja" => {
            description_lines.push("📊 【フィボナッチリトレースメント】".to_string());
            description_lines
                .push("💡 トレンド内の押し目や戻り目を判断するための価格帯".to_string());
        }
        _ => {
            description_lines.push("📊 [Fibonacci Retracement]".to_string());
            description_lines.push(
                "💡 Price zones for identifying pullbacks and recoveries within a trend"
                    .to_string(),
            );
        }
    }

    if let (Some(level_38_2), Some(level_50), Some(level_61_8)) = (
        guard.get_fibo_38_2(),
        guard.get_fibo_50_0(),
        guard.get_fibo_61_8(),
    ) {
        description_lines.push(format!(
            "38.2%: {:.2} / 50.0%: {:.2} / 61.8%: {:.2}",
            level_38_2, level_50, level_61_8
        ));

        let base_score = guard.get_fibonacci_score().map(|v| v.round() as i32);
        // The wording states the rule the score actually applied: ±2 is decided by
        // the 38.2% / 61.8% level, and ±1 / 0 by the neutral band around the 50%
        // level. It previously named a ±2.00 / ±0.50 distance from the 50% level,
        // which is not the rule — a close 1.64 below the 50% level was reported as
        // "more than 2.00 below" — and hardcoded the band at 0.50 whatever it was
        // configured to be. The band comes from the same function the score uses.
        let eps = crate::technical::indicators::fibonacci_neutral_band(
            config.fibonacci_neutral_ratio,
            level_38_2,
            level_50,
        );
        let band_line = match base_score {
            Some(2) => match lang {
                "ja" => format!(
                    "🟢 終値が38.2%水準（{:.2}）以上 → 非常に強い上昇 → スコア+2",
                    level_38_2
                ),
                _ => format!(
                    "🟢 Close at or above the 38.2% level ({:.2}) → Very strong upward move → score +2",
                    level_38_2
                ),
            },
            Some(1) => match lang {
                "ja" => format!(
                    "🟢 終値が50%（{:.2}）より+{:.2}以上上、かつ38.2%水準（{:.2}）未満 → 上昇傾向 → スコア+1",
                    level_50, eps, level_38_2
                ),
                _ => format!(
                    "🟢 Close at least {:.2} above 50% ({:.2}) and below the 38.2% level ({:.2}) → Upward trend → score +1",
                    eps, level_50, level_38_2
                ),
            },
            Some(0) => match lang {
                "ja" => format!("➡️ 終値が50%（{:.2}）±{:.2}内 → 中立（0）", level_50, eps),
                _ => format!(
                    "➡️ Close within ±{:.2} of 50% ({:.2}) → neutral (0)",
                    eps, level_50
                ),
            },
            Some(-1) => match lang {
                "ja" => format!(
                    "🔴 終値が50%（{:.2}）より-{:.2}以上下、かつ61.8%水準（{:.2}）超 → 下降傾向 → スコア-1",
                    level_50, eps, level_61_8
                ),
                _ => format!(
                    "🔴 Close at least {:.2} below 50% ({:.2}) and above the 61.8% level ({:.2}) → Downward trend → score -1",
                    eps, level_50, level_61_8
                ),
            },
            Some(-2) => match lang {
                "ja" => format!(
                    "🔴 終値が61.8%水準（{:.2}）以下 → 非常に強い下落 → スコア-2",
                    level_61_8
                ),
                _ => format!(
                    "🔴 Close at or below the 61.8% level ({:.2}) → Very strong downward move → score -2",
                    level_61_8
                ),
            },
            Some(other) => match lang {
                "ja" => format!("⚠️ 想定外スコア({}) → 中立扱い（0）", other),
                _ => format!("⚠️ Unexpected score ({}) → treated as neutral (0)", other),
            },
            None => match lang {
                "ja" => "⚠️ フィボナッチスコア情報なし".to_string(),
                _ => "⚠️ Fibonacci score unavailable".to_string(),
            },
        };
        description_lines.push(band_line);
    } else {
        match lang {
            "ja" => description_lines.push("⚠️ フィボナッチデータが不足しています".to_string()),
            _ => description_lines.push("⚠️ Fibonacci data insufficient".to_string()),
        }
    }

    let indicator_name_fibo = match lang {
        "ja" => "フィボナッチ",
        _ => "Fibonacci",
    };
    match guard.get_fibonacci_score().map(|v| v as i32) {
        Some(base_score) => {
            let adjusted_score = base_score as f64 * weight;
            description_lines.push(crate::render::adjusted_score_line(
                lang,
                adjusted_score,
                base_score,
                weight,
            ));
            AnalysisResult {
                indicator_name: indicator_name_fibo.to_string(),
                description: description_lines,
                score: Some(base_score as f64),
            }
        }
        None => {
            match lang {
                "ja" => description_lines.push("⚠️ フィボナッチスコア情報なし".to_string()),
                _ => description_lines.push("⚠️ Fibonacci score unavailable".to_string()),
            }
            AnalysisResult {
                indicator_name: indicator_name_fibo.to_string(),
                description: description_lines,
                score: None,
            }
        }
    }
}

pub(crate) fn render_vwap(config: &Config, guard: &TechnicalDataGuard) -> AnalysisResult {
    let weight = config.weight_vwap;
    let lang = config.lang.as_str();
    let mut description_lines: Vec<String> = Vec::new();
    match lang {
        "ja" => description_lines.push("📊 【VWAP（出来高加重平均価格）】".to_string()),
        _ => description_lines.push("📊 [VWAP (Volume Weighted Average Price)]".to_string()),
    }
    let vwap_basis = if config.analysis_mode.is_intraday() {
        match lang {
            "ja" => format!(
                "指標計算最終足と同日セッション全足（{}）",
                config.analysis_mode.bar_label_ja()
            ),
            _ => format!(
                "all {} bars in same-day session as indicator bar",
                config.analysis_mode.bar_label(lang)
            ),
        }
    } else {
        config
            .analysis_mode
            .period_context(lang, config.vwap_period)
    };
    match lang {
        "ja" => description_lines.push(format!(
            "VWAPはTypical Priceを出来高で加重して算出しています（参照期間: {}）。",
            vwap_basis
        )),
        _ => description_lines.push(format!(
            "VWAP is calculated as volume-weighted typical price (reference period: {}).",
            vwap_basis
        )),
    }

    if let Some(vwap_value) = guard.get_vwap() {
        match lang {
            "ja" => description_lines.push(format!("VWAP値: {:.2}", vwap_value)),
            _ => description_lines.push(format!("VWAP: {:.2}", vwap_value)),
        }
        if vwap_value.abs() > f64::EPSILON {
            let close = guard.get_close();
            let diff_pct = (close - vwap_value) / vwap_value * 100.0;
            description_lines.push(crate::render::deviation_line(
                lang,
                "VWAP",
                "VWAP",
                "対VWAP比",
                "vs VWAP",
                diff_pct,
            ));
        }
    } else {
        match lang {
            "ja" => description_lines.push("⚠️ VWAPデータが不足しています".to_string()),
            _ => description_lines.push("⚠️ VWAP data insufficient".to_string()),
        }
    }

    match guard.get_vwap_score().map(|v| v as i32) {
        Some(base_score) => {
            let adjusted_score = base_score as f64 * weight;
            description_lines.push(
                utils::get_score_description(&ExtensionIndicator::Vwap, Some(base_score), lang)
                    .to_string(),
            );
            description_lines.push(crate::render::adjusted_score_line(
                lang,
                adjusted_score,
                base_score,
                weight,
            ));
            AnalysisResult {
                indicator_name: "VWAP".to_string(),
                description: description_lines,
                score: Some(base_score as f64),
            }
        }
        None => {
            description_lines.push(
                utils::get_score_description(&ExtensionIndicator::Vwap, None, lang).to_string(),
            );
            AnalysisResult {
                indicator_name: "VWAP".to_string(),
                description: description_lines,
                score: None,
            }
        }
    }
}

pub(crate) fn render_ichimoku(config: &Config, guard: &TechnicalDataGuard) -> AnalysisResult {
    let weight = config.weight_ichimoku;
    let lang = config.lang.as_str();
    let mut description_lines: Vec<String> = Vec::new();
    match lang {
        "ja" => description_lines.push("📊 【一目均衡表】".to_string()),
        _ => description_lines.push("📊 [Ichimoku Cloud]".to_string()),
    }

    if let (Some(tenkan), Some(kijun)) = (guard.get_tenkan_sen(), guard.get_kijun_sen()) {
        match lang {
            "ja" => description_lines.push(format!("転換線: {:.2} / 基準線: {:.2}", tenkan, kijun)),
            _ => description_lines.push(format!(
                "Tenkan-sen: {:.2} / Kijun-sen: {:.2}",
                tenkan, kijun
            )),
        }
        description_lines.push(crate::render::cross_status(
            lang,
            tenkan
                .partial_cmp(&kijun)
                .unwrap_or(std::cmp::Ordering::Equal),
            &crate::render::CrossLabels {
                short_ja: "転換線",
                long_ja: "基準線",
                short_en: "tenkan",
                long_en: "kijun",
                equal_ja: "➡️ 転換線と基準線が交差中（横ばい）",
                equal_en: "➡️ Tenkan and kijun crossing (flat)",
            },
        ));
        if kijun != 0.0 {
            let gap_ratio = ((tenkan - kijun) / kijun).abs() * 100.0;
            match gap_ratio {
                g if g < 1.0 => match lang {
                    "ja" => description_lines.push(format!(
                        "💡 クロス直後の接近状態（乖離 {:.2}%）→ トレンド確定には弱い傾向",
                        gap_ratio
                    )),
                    _ => description_lines.push(format!(
                        "💡 Close after cross (gap {:.2}%) → trend not yet confirmed",
                        gap_ratio
                    )),
                },
                g if g > 5.0 => match lang {
                    "ja" => description_lines.push(format!(
                        "💡 クロス乖離が大きい（乖離 {:.2}%）→ 強いトレンドの可能性",
                        gap_ratio
                    )),
                    _ => description_lines.push(format!(
                        "💡 Large gap after cross ({:.2}%) → possible strong trend",
                        gap_ratio
                    )),
                },
                _ => {}
            }
        }
    } else {
        match lang {
            "ja" => description_lines.push("⚠️ 一目均衡表データが不足しています".to_string()),
            _ => description_lines.push("⚠️ Ichimoku data insufficient".to_string()),
        }
    }

    let indicator_name_ichi = match lang {
        "ja" => "一目均衡表",
        _ => "Ichimoku",
    };
    match guard.get_ichimoku_score().map(|v: f64| v as i32) {
        Some(base_score) => {
            let adjusted_score = base_score as f64 * weight;
            description_lines.push(
                utils::get_score_description(&ExtensionIndicator::Ichimoku, Some(base_score), lang)
                    .to_string(),
            );
            description_lines.push(crate::render::adjusted_score_line(
                lang,
                adjusted_score,
                base_score,
                weight,
            ));
            AnalysisResult {
                indicator_name: indicator_name_ichi.to_string(),
                description: description_lines,
                score: Some(base_score as f64),
            }
        }
        None => {
            description_lines.push(
                utils::get_score_description(&ExtensionIndicator::Ichimoku, None, lang).to_string(),
            );
            AnalysisResult {
                indicator_name: indicator_name_ichi.to_string(),
                description: description_lines,
                score: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AnalysisMode;
    use crate::technical::TechnicalDataGuard;

    fn guard_with(close: f64, ema_short: f64, ema_long: f64) -> TechnicalDataGuard {
        let mut g = TechnicalDataGuard::new("T".to_string(), "2026-09-24".to_string());
        g.set_close(close);
        g.set_ema_short(ema_short);
        g.set_ema_long(ema_long);
        g
    }

    /// The rendered lines must agree with the score, whatever the price level. The
    /// cross line used to call a price difference under ±0.01 "no score change": on
    /// a 0.459 close an EMA gap of 0.00747 is a 1.63% deviation, so the score was
    /// +1 while the same block said there was none — and scaling the series by
    /// 1,000 changed only the wording.
    #[test]
    fn the_ema_block_never_denies_the_score_it_prints() {
        let config = Config::default();
        for (close, short, long) in [
            (0.459_f64, 0.463735_f64, 0.456265_f64),
            (459.0, 463.735, 456.265),
        ] {
            let mut g = guard_with(close, short, long);
            let dev_pct = (short - long) / close * 100.0;
            g.set_ema_score(crate::technical::indicators::ema_score_from_dev_pct(
                dev_pct,
            ));

            let rendered = render_ema(&config, &g).description.join("\n");
            assert!(
                !rendered.contains("no score change"),
                "close {close}: a {dev_pct:.4}% deviation scores +1; the block must not deny it:\n{rendered}"
            );
            assert!(
                !rendered.contains("±0.01"),
                "close {close}: the block must not quote a price difference:\n{rendered}"
            );
        }
    }

    /// At the band itself the position line and the score must agree. Deciding the
    /// line by `%B > 1.0` while the score compares the price to the band inclusively
    /// made a close equal to the upper band print "within bands → neutral" beside a
    /// `-1`, and revived a "within bands, upper-biased" hint that had been dead code.
    #[test]
    fn the_bollinger_position_line_agrees_with_the_score_at_the_band() {
        let config = Config::default();
        for (score, expect, reject) in [
            (-1_i32, "Upper band break", "within bands"),
            (1, "Lower band break", "within bands"),
            (0, "within bands", "band break"),
        ] {
            let mut g = TechnicalDataGuard::new("T".to_string(), "2026-09-25".to_string());
            g.set_close(100.0);
            g.set_bb_upper(100.0);
            g.set_bb_lower(90.0);
            g.set_bb_percent_b(1.0);
            g.set_bb_bandwidth(10.0);
            g.set_bollinger_score(score as f64);

            let rendered = render_bollinger(&config, &g).description.join("\n");
            assert!(
                rendered.contains(expect),
                "score {score} must print {expect:?}:\n{rendered}"
            );
            assert!(
                !rendered.to_lowercase().contains(reject),
                "score {score} must not print {reject:?}:\n{rendered}"
            );
        }
    }

    /// A close landing exactly on a Fibonacci level scores `±2`, so the line that
    /// explains it must not say the close is merely "above"/"below" the level.
    #[test]
    fn the_fibonacci_line_matches_the_boundary_it_scored() {
        let config = Config::default();
        let (high, low) = (110.0_f64, 90.0_f64);
        let span = high - low;
        for (level, score, expect) in [
            (high - span * 0.382, 2_i32, "at or above the 38.2% level"),
            (high - span * 0.618, -2, "at or below the 61.8% level"),
        ] {
            let mut g = TechnicalDataGuard::new("T".to_string(), "2026-09-25".to_string());
            g.set_close(level);
            g.set_fibo_38_2(high - span * 0.382);
            g.set_fibo_50_0(high - span * 0.500);
            g.set_fibo_61_8(high - span * 0.618);
            g.set_fibonacci_score(score as f64);

            let rendered = render_fibonacci(&config, &g).description.join("\n");
            assert!(
                rendered.contains(expect),
                "a close on the level scored {score}; the line must say {expect:?}:\n{rendered}"
            );
        }
    }

    /// A score that could not be computed must stay absent all the way to the
    /// rendered result. `unwrap_or(0.0)` displayed "not computed" as a neutral 0 —
    /// the one distinction security-design §1 exists to keep.
    #[test]
    fn an_uncomputed_ema_score_is_not_rendered_as_zero() {
        let config = Config::default();
        // No `set_ema_score` call at all: the deviation rate could not be formed.
        let g = guard_with(0.0, 1.0, 1.0);
        let result = render_ema(&config, &g);
        assert!(
            result.score.is_none(),
            "an absent score must not become a number: {:?}",
            result.score
        );
    }

    /// The "strong" wording is warranted on every timeframe, because a ±2 now
    /// requires a divergence of at least `ICHIMOKU_STRONG_DEV_PCT`. Intraday used to
    /// need softer wording for a ±2 that came from a gap of a fraction of a percent;
    /// that state no longer exists, so both timeframes read from one table.
    #[test]
    fn ichimoku_score_wording_is_the_same_on_every_timeframe() {
        for mode in [AnalysisMode::Daily, AnalysisMode::Intraday30m] {
            let config = Config {
                analysis_mode: mode,
                ..Config::default()
            };
            let description = utils::get_score_description(
                &ExtensionIndicator::Ichimoku,
                Some(-2),
                config.lang.as_str(),
            );
            assert!(
                description.contains("well below") && description.contains("Strong"),
                "{mode:?}: expected the strong wording, got {description}"
            );
        }
    }
}
