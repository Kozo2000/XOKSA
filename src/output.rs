use anyhow::{bail, Result};
use serde_json::json;
use std::fs::{create_dir_all, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::config::{Config, ExtensionIndicator};
use crate::technical::calculate_final_score_snapshot;
use crate::technical::types::{FinalScoreSnapshot, TechnicalDataGuard};

pub trait TechnicalLogFormatter {
    fn csv_header(&self, config: &Config) -> String;
    fn csv_row(
        &self,
        config: &Config,
        guard: &TechnicalDataGuard,
        snap: &FinalScoreSnapshot,
    ) -> Result<String>;
    fn json_row(
        &self,
        config: &Config,
        guard: &TechnicalDataGuard,
        snap: &FinalScoreSnapshot,
    ) -> Result<String>;
}

pub trait TechnicalLogWriter {
    fn write(
        &self,
        config: &Config,
        guard: &TechnicalDataGuard,
        snap: &FinalScoreSnapshot,
    ) -> Result<()>;
}

pub struct DefaultTechnicalLogFormatter;

impl TechnicalLogFormatter for DefaultTechnicalLogFormatter {
    fn csv_header(&self, config: &Config) -> String {
        // Columns come from the single field definition — identical membership/order
        // to the JSON (`technical_json_value`). No hand-kept second list.
        analysis_fields(config)
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(",")
    }

    fn csv_row(
        &self,
        config: &Config,
        guard: &TechnicalDataGuard,
        snap: &FinalScoreSnapshot,
    ) -> Result<String> {
        // Row values come from the SAME `analysis_fields` the JSON uses, rendered raw
        // (no rounding) — so CSV and JSON hold byte-for-byte the same numbers.
        let row = analysis_fields(config)
            .iter()
            .map(|(_, val)| value_to_csv_cell(&val(guard, snap)))
            .collect::<Vec<_>>()
            .join(",");
        Ok(row)
    }

    fn json_row(
        &self,
        config: &Config,
        guard: &TechnicalDataGuard,
        snap: &FinalScoreSnapshot,
    ) -> Result<String> {
        // Single source of truth: the JSON object is built by `technical_json_value`,
        // which the REST server also serves verbatim (CLI `--log-format json` and the
        // API return byte-identical analysis JSON).
        Ok(serde_json::to_string(&technical_json_value(
            config, guard, snap,
        ))?)
    }
}

/// A field extractor for the canonical analysis record. Returns the raw value as a
/// `serde_json::Value` — numbers are UNROUNDED (f64 is the SOT; no display rounding).
type FieldVal = fn(&TechnicalDataGuard, &FinalScoreSnapshot) -> serde_json::Value;

/// **The single source of truth for the analysis record's fields.** The ordered
/// `(name, extractor)` list that BOTH the CSV log (`csv_header`/`csv_row`) and the
/// JSON (`technical_json_value`, served by CLI `--log-format json` AND the API `data`
/// field) fold over. Membership, names, and values are defined here ONCE, so the CSV
/// and JSON can never disagree on which fields exist or what they hold. Adding an
/// indicator = one edit here, and every serializer follows. The set depends on
/// `config.enabled_extensions`.
fn analysis_fields(config: &Config) -> Vec<(&'static str, FieldVal)> {
    let mut f: Vec<(&'static str, FieldVal)> = vec![
        ("ticker", |g, _| json!(g.get_ticker())),
        ("bar_date", |g, _| json!(g.get_date())),
        ("datetime", |g, _| json!(g.get_datetime())),
        ("timestamp", |g, _| json!(g.get_timestamp())),
        ("bar_time", |g, _| json!(g.get_bar_time())),
        ("bar_timestamp", |g, _| json!(g.get_bar_timestamp())),
        ("timezone", |g, _| json!(g.get_timezone())),
        ("latest_observed_price", |g, _| {
            json!(g.get_latest_observed_price())
        }),
        ("market_data_latest_time", |g, _| {
            json!(g.get_market_data_latest_time())
        }),
        ("market_data_latest_timestamp", |g, _| {
            json!(g.get_market_data_latest_timestamp())
        }),
        ("analyzed_at", |g, _| json!(g.get_analyzed_at())),
        ("analyzed_at_timestamp", |g, _| {
            json!(g.get_analyzed_at_timestamp())
        }),
        ("bar_close", |g, _| json!(g.get_close())),
        ("previous_bar_close", |g, _| json!(g.get_previous_close())),
        ("bar_diff", |g, _| json!(g.get_price_diff())),
        ("bar_diff_pct", |g, _| json!(g.get_price_diff_percent())),
        ("macd", |g, _| json!(g.get_macd())),
        ("signal", |g, _| json!(g.get_signal())),
        ("rsi", |g, _| json!(g.get_rsi())),
        ("score", |g, _| json!(g.get_signal_score())),
    ];

    for ext in &config.enabled_extensions {
        match ext {
            ExtensionIndicator::Ema => f.extend([
                ("ema_short", (|g, _| json!(g.get_ema_short())) as FieldVal),
                ("ema_long", |g, _| json!(g.get_ema_long())),
                ("ema_score", |g, _| json!(g.get_ema_score())),
            ]),
            ExtensionIndicator::Sma => f.extend([
                ("sma_short", (|g, _| json!(g.get_sma_short())) as FieldVal),
                ("sma_long", |g, _| json!(g.get_sma_long())),
                ("sma_score", |g, _| json!(g.get_sma_score())),
            ]),
            ExtensionIndicator::Roc => f.extend([
                ("roc", (|g, _| json!(g.get_roc())) as FieldVal),
                ("roc_score", |g, _| json!(g.get_roc_score())),
            ]),
            ExtensionIndicator::Adx => f.extend([
                ("adx", (|g, _| json!(g.get_adx())) as FieldVal),
                ("adx_score", |g, _| json!(g.get_adx_score())),
            ]),
            ExtensionIndicator::Stochastics => f.extend([
                ("stoch_k", (|g, _| json!(g.get_stochastics_k())) as FieldVal),
                ("stoch_d", |g, _| json!(g.get_stochastics_d())),
                ("stoch_score", |g, _| json!(g.get_stochastics_score())),
            ]),
            ExtensionIndicator::Bollinger => f.extend([
                ("bb_upper", (|g, _| json!(g.get_bb_upper())) as FieldVal),
                ("bb_lower", |g, _| json!(g.get_bb_lower())),
                ("percent_b", |g, _| json!(g.get_bb_percent_b())),
                ("bandwidth_%", |g, _| json!(g.get_bb_bandwidth())),
                ("bb_score", |g, _| json!(g.get_bollinger_score())),
            ]),
            ExtensionIndicator::Fibonacci => f.extend([
                ("fibo_38_2", (|g, _| json!(g.get_fibo_38_2())) as FieldVal),
                ("fibo_50_0", |g, _| json!(g.get_fibo_50_0())),
                ("fibo_61_8", |g, _| json!(g.get_fibo_61_8())),
                ("fibo_score", |g, _| json!(g.get_fibonacci_score())),
            ]),
            ExtensionIndicator::Vwap => f.extend([
                ("vwap", (|g, _| json!(g.get_vwap())) as FieldVal),
                ("vwap_score", |g, _| json!(g.get_vwap_score())),
            ]),
            ExtensionIndicator::Ichimoku => f.extend([
                ("tenkan", (|g, _| json!(g.get_tenkan_sen())) as FieldVal),
                ("kijun", |g, _| json!(g.get_kijun_sen())),
                ("ichimoku_score", |g, _| json!(g.get_ichimoku_score())),
            ]),
        }
    }

    f.extend([
        (
            "latest_volume",
            (|g, _| json!(g.get_latest_volume())) as FieldVal,
        ),
        ("avg_volume", |g, _| json!(g.get_avg_volume())),
        ("volume_ratio", |g, _| json!(g.get_volume_ratio())),
        ("final_score", |_, s| json!(s.total_score)),
    ]);
    f
}

/// Canonical analysis → JSON object, built from `analysis_fields` (the single field
/// definition). Both CLI `--log-format json` (`json_row`, which just stringifies this)
/// and the REST `/api/symbol/{symbol}/summary` (`data` field) return this exact value
/// — there is only ONE serializer.
pub fn technical_json_value(
    config: &Config,
    guard: &TechnicalDataGuard,
    snap: &FinalScoreSnapshot,
) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    for (key, val) in analysis_fields(config) {
        obj.insert(key.to_string(), val(guard, snap));
    }
    serde_json::Value::Object(obj)
}

/// Render a canonical field value as a CSV cell — RAW (no rounding, matching the JSON
/// exactly): numbers as their plain decimal, strings/times as-is, null → empty.
fn value_to_csv_cell(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub struct FileOrStdoutTechnicalLogWriter<F> {
    formatter: F,
}

impl<F> FileOrStdoutTechnicalLogWriter<F> {
    pub fn new(formatter: F) -> Self {
        Self { formatter }
    }
}

impl<F: TechnicalLogFormatter> TechnicalLogWriter for FileOrStdoutTechnicalLogWriter<F> {
    fn write(
        &self,
        config: &Config,
        guard: &TechnicalDataGuard,
        snap: &FinalScoreSnapshot,
    ) -> Result<()> {
        match config.log_format.to_lowercase().as_str() {
            "csv" => {
                let row = self.formatter.csv_row(config, guard, snap)?;
                if config.stdout_log {
                    println!("{}", row);
                    return Ok(());
                }

                let base_dir = Path::new(&config.log_dir);
                let dir_path = if config.log_flat {
                    base_dir.to_path_buf()
                } else {
                    base_dir.join(guard.get_ticker())
                };
                create_dir_all(&dir_path)?;

                let file_path = dir_path.join(format!("{}.csv", guard.get_ticker()));

                if config.data_append && file_path.exists() {
                    let expected_cols = row.split(',').count();
                    if let Ok(existing) = std::fs::read_to_string(&file_path) {
                        if let Some(first_line) = existing.lines().next() {
                            let existing_cols = first_line.split(',').count();
                            if existing_cols != expected_cols {
                                eprintln!(
                                    "⚠️ [{}] CSV column count mismatch (existing: {}, current: {}). Skipping append to avoid data corruption. Remove --data-append to overwrite.",
                                    guard.get_ticker(), existing_cols, expected_cols
                                );
                                return Ok(());
                            }
                        }
                    }
                }

                let file = OpenOptions::new()
                    .create(true)
                    .append(config.data_append)
                    .write(true)
                    .truncate(!config.data_append)
                    .open(&file_path)?;
                let mut writer = BufWriter::new(file);
                writeln!(writer, "{}", row)?;
                Ok(())
            }
            "json" => {
                let json_str = self.formatter.json_row(config, guard, snap)?;
                if config.stdout_log {
                    println!("{}", json_str);
                    return Ok(());
                }

                let base_dir = Path::new(&config.log_dir);
                let dir_path = if config.log_flat {
                    base_dir.to_path_buf()
                } else {
                    base_dir.join(guard.get_ticker())
                };
                create_dir_all(&dir_path)?;

                let file = OpenOptions::new()
                    .create(true)
                    .append(config.data_append)
                    .write(true)
                    .truncate(!config.data_append)
                    .open(dir_path.join(format!("{}.json", guard.get_ticker())))?;
                let mut writer = BufWriter::new(file);
                writeln!(writer, "{}", json_str)?;
                Ok(())
            }
            other => bail!("❌ Unsupported log format: {}", other),
        }
    }
}

pub fn generate_csv_header(config: &Config) {
    let formatter = DefaultTechnicalLogFormatter;
    println!("{}", formatter.csv_header(config));
}

pub fn save_technical_log(config: &Config, guard: &TechnicalDataGuard) -> Result<()> {
    let snap = calculate_final_score_snapshot(config, guard);
    let writer = FileOrStdoutTechnicalLogWriter::new(DefaultTechnicalLogFormatter);
    writer.write(config, guard, &snap)
}

#[cfg(test)]
mod sot_gate_tests {
    use super::{
        technical_json_value, value_to_csv_cell, DefaultTechnicalLogFormatter,
        TechnicalLogFormatter,
    };
    use crate::config::{Config, ExtensionIndicator};
    use crate::technical::calculate_final_score_snapshot;
    use crate::technical::types::TechnicalDataGuard;
    use std::collections::BTreeSet;

    fn sample() -> (Config, TechnicalDataGuard) {
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
        (config, guard)
    }

    /// **SOT gate (deterministic).** The CSV log columns and the JSON `data` keys are
    /// the SAME field set — both fold over `analysis_fields`. If a future edit hand-lists
    /// CSV columns again (or the two drift), this fails the build.
    #[test]
    fn csv_columns_match_json_keys() {
        let (config, guard) = sample();
        let snap = calculate_final_score_snapshot(&config, &guard);
        let header: BTreeSet<String> = DefaultTechnicalLogFormatter
            .csv_header(&config)
            .split(',')
            .map(str::to_string)
            .collect();
        let json_keys: BTreeSet<String> = technical_json_value(&config, &guard, &snap)
            .as_object()
            .expect("object")
            .keys()
            .cloned()
            .collect();
        assert_eq!(
            header, json_keys,
            "SOT VIOLATION: CSV columns and JSON keys diverged — both MUST come from analysis_fields."
        );
    }

    /// **SOT gate (deterministic).** Every CSV cell equals the JSON value rendered raw
    /// (no rounding drift): CSV and JSON hold the SAME numbers. Injecting a `{:.2}` back
    /// into one path would make a cell differ from the raw JSON value here.
    #[test]
    fn csv_values_match_json_raw() {
        let (config, guard) = sample();
        let snap = calculate_final_score_snapshot(&config, &guard);
        let cols: Vec<String> = DefaultTechnicalLogFormatter
            .csv_header(&config)
            .split(',')
            .map(str::to_string)
            .collect();
        let cells: Vec<String> = DefaultTechnicalLogFormatter
            .csv_row(&config, &guard, &snap)
            .expect("csv_row")
            .split(',')
            .map(str::to_string)
            .collect();
        let json = technical_json_value(&config, &guard, &snap);
        for (col, cell) in cols.iter().zip(cells.iter()) {
            assert_eq!(
                *cell,
                value_to_csv_cell(&json[col]),
                "SOT VIOLATION: CSV cell for `{col}` differs from the raw JSON value (rounding/format drift)."
            );
        }
    }
}
