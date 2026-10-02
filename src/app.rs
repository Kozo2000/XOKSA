use crate::config::{Config, ExtensionIndicator};
use crate::render::{Renderer, TerminalRenderer};
use crate::technical::types::{AnalysisResult, TechnicalDataGuard};
use anyhow::{Context, Result};
use std::collections::HashMap;

/// Run the market-data → technical-analysis pipeline and return a populated,
/// read-only guard. Shared by the CLI (`main`) and the Web UI server so the
/// fetch/sort/build/evaluate sequence lives in exactly one place (SOT).
pub async fn build_analyzed_guard(
    config: &Config,
    ticker: &str,
    ticker_name_map: &HashMap<String, String>,
) -> Result<TechnicalDataGuard> {
    let price_fetcher = crate::market::build_price_fetcher(config);
    let market_snapshot =
        crate::traits::PriceFetcher::fetch_snapshot(&price_fetcher, ticker, config.analysis_mode)
            .await?;

    let mut sorted_data = market_snapshot.bars.clone();
    sorted_data.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.datetime.cmp(&b.datetime))
            .then_with(|| a.date.cmp(&b.date))
    });

    let mut guard =
        crate::technical::build_basic_technical_entry(config, &sorted_data, ticker_name_map)?;
    guard.set_timezone(&market_snapshot.timezone);
    // The quoted currency travels with the analysis, so the confirmed data
    // knows it even when no fundamental block was fetched (G7).
    if let Some(currency) = market_snapshot.currency.as_deref() {
        guard.set_currency(currency);
    }
    if let Some(market_data_latest_time) = market_snapshot.market_data_latest_time.as_deref() {
        guard.set_market_data_latest_time(market_data_latest_time);
    }
    if let Some(market_data_latest_timestamp) = market_snapshot.market_data_latest_timestamp {
        guard.set_market_data_latest_timestamp(market_data_latest_timestamp);
    }
    if let Some(latest_observation) = &market_snapshot.latest_observation {
        guard.set_latest_observed_price(latest_observation.price);
    }
    guard.set_analyzed_at(&market_snapshot.analyzed_at);
    guard.set_analyzed_at_timestamp(market_snapshot.analyzed_at_timestamp);
    if let Some(source_note) = market_snapshot.source_note.as_deref() {
        guard.set_source_note(source_note);
    }

    // Evaluate extensions, surfacing partial failures once per analysis — but only
    // when not running silently (background per-bar/context builds set `silent`, so
    // they don't repeat this; the printing path is for the user-facing analysis).
    let ext_report = crate::technical::evaluate_all_selected_extensions_with_report(
        config,
        &sorted_data,
        &mut guard,
    )
    .context("❌ Extended technical indicator evaluation failed")?;
    if !config.silent && ext_report.has_failures() {
        crate::logging::warn(
            "XK-IND-EVAL",
            &format!(
                "Some extended technical indicator evaluations failed: {}",
                ext_report.failure_message()
            ),
        );
    }

    Ok(guard)
}

pub fn run_output_pipeline(
    config: &Config,
    guard: &TechnicalDataGuard,
    fundamental_data: Option<&crate::fundamental::FundamentalData>,
) -> Result<()> {
    let warn_if_none = |score: Option<f64>, name: &str| -> Option<f64> {
        if score.is_none() {
            eprintln!(
                "⚠️ [{}] score calculation failed (CSV/log entry will be blank)",
                name
            );
        }
        score
    };

    let mut results: Vec<AnalysisResult> = Vec::new();

    for indicator in &config.enabled_extensions {
        match indicator {
            ExtensionIndicator::Ema => {
                results.push(AnalysisResult {
                    indicator_name: "EMA".to_string(),
                    description: Vec::new(),
                    score: warn_if_none(guard.get_ema_score(), "EMA"),
                });
            }
            ExtensionIndicator::Sma => {
                results.push(AnalysisResult {
                    indicator_name: "SMA".to_string(),
                    description: Vec::new(),
                    score: warn_if_none(guard.get_sma_score(), "SMA"),
                });
            }
            ExtensionIndicator::Roc => {
                results.push(AnalysisResult {
                    indicator_name: "ROC".to_string(),
                    description: Vec::new(),
                    score: warn_if_none(guard.get_roc_score(), "ROC"),
                });
            }
            ExtensionIndicator::Adx => {
                results.push(AnalysisResult {
                    indicator_name: "ADX".to_string(),
                    description: Vec::new(),
                    score: warn_if_none(guard.get_adx_score(), "ADX"),
                });
            }
            ExtensionIndicator::Stochastics => {
                results.push(AnalysisResult {
                    indicator_name: "Stochastics".to_string(),
                    description: Vec::new(),
                    score: warn_if_none(guard.get_stochastics_score(), "Stochastics"),
                });
            }
            ExtensionIndicator::Bollinger => {
                results.push(AnalysisResult {
                    indicator_name: "Bollinger".to_string(),
                    description: Vec::new(),
                    score: warn_if_none(guard.get_bollinger_score(), "Bollinger"),
                });
            }
            ExtensionIndicator::Fibonacci => {
                results.push(AnalysisResult {
                    indicator_name: "Fibonacci".to_string(),
                    description: Vec::new(),
                    score: warn_if_none(guard.get_fibonacci_score(), "Fibonacci"),
                });
            }
            ExtensionIndicator::Vwap => {
                results.push(AnalysisResult {
                    indicator_name: "VWAP".to_string(),
                    description: Vec::new(),
                    score: warn_if_none(guard.get_vwap_score(), "VWAP"),
                });
            }
            ExtensionIndicator::Ichimoku => {
                results.push(AnalysisResult {
                    indicator_name: "Ichimoku".to_string(),
                    description: Vec::new(),
                    score: warn_if_none(guard.get_ichimoku_score(), "Ichimoku"),
                });
            }
        }
    }

    if config.save_technical_log {
        crate::output::save_technical_log(config, guard)?;
    }

    if !config.silent {
        let renderer = TerminalRenderer;
        Renderer::render_to_terminal(&renderer, config, guard);
        if let Some(data) = fundamental_data {
            for line in crate::fundamental::render_fundamental_display(data, &config.lang) {
                println!("{}", line);
            }
        }
    }

    Ok(())
}

// Collects technical display lines for /show technical. No LLM task directives.
pub fn collect_display_lines(config: &Config, guard: &TechnicalDataGuard) -> Vec<String> {
    let renderer = TerminalRenderer;
    let mut lines = Vec::new();

    lines.extend(crate::render::collect_main_info_lines(config, guard));

    for result in crate::render::render_ranked(config, guard) {
        lines.extend(result.description);
        lines.push(String::new());
    }

    let snap = crate::technical::calculate_final_score_snapshot(config, guard);
    let score_lines =
        Renderer::compose_final_score_lines(&renderer, &snap, &config.stance, false, &config.lang);
    lines.extend(score_lines);

    lines
}
