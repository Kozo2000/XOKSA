//! Multi-timeframe analysis context builder.
//!
//! Takes a `ContextSpec` (which timeframes/indicators to look at) and produces a
//! Rust-built **summary** for the LLM — never raw bulk data (SOT + token control).
//! Each requested timeframe is analyzed once through the shared engine
//! (`app::build_analyzed_guard`), so the values are identical to what the CLI/Web
//! would show for that timeframe.
//!
//! Scope (Phase 4): builds the "now" context (latest data per timeframe). A
//! historical `as_of` (reading bars as-of a past point) is recorded but not yet
//! honored — that arrives with the backtest data feed.

use crate::config::{AnalysisMode, Config, ExtensionIndicator};
use crate::technical::types::TechnicalDataGuard;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

#[derive(Deserialize)]
pub struct ContextSpec {
    pub symbol: String,
    #[serde(default)]
    pub as_of: Option<String>,
    #[serde(default)]
    pub contexts: Vec<ContextItem>,
    /// GUI language ("ja"/"en"), so the pack text + LLM answer match the browser.
    #[serde(default)]
    pub lang: Option<String>,
    /// The header timeframe — used only to find the persisted chat session so the
    /// LLM commentary uses the same provider/model a `/llm` switch selected.
    #[serde(default)]
    pub timeframe: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ContextItem {
    /// "indicator" | "price" (others reserved for later phases).
    #[serde(rename = "type")]
    pub kind: String,
    pub timeframe: String,
    #[serde(default)]
    pub indicator_name: Option<String>,
    #[serde(default)]
    pub lookback: Option<i64>,
    #[serde(default)]
    pub complete_only: Option<bool>,
}

#[derive(Serialize)]
pub struct ContextPack {
    pub symbol: String,
    pub as_of: Option<String>,
    /// The LLM-facing summary (plain text).
    pub context_pack_text: String,
    /// The same content, structured.
    pub context_pack_json: Value,
    /// The same content as confirmed data, one entry per analyzed timeframe, so
    /// the output-integrity guard verifies the model's numbers against the
    /// structures rather than against the pack text. Each entry names its
    /// timeframe, so a value stated for the daily bar is not accepted as the
    /// hourly one. Not part of the JSON response.
    #[serde(skip)]
    pub facts: crate::integrity::ConfirmedFactSet,
}

impl ContextPack {
    /// Frame the LLM prompt that asks the model to interpret this pack: the chat
    /// guard + the pack (confirmed data / SOT) + a neutral task. This is a preset
    /// with no user question, so no conversational style axes apply. Kept in the
    /// engine — the single place pack prompts are assembled — so CLI and Web never
    /// fork this framing (design-philosophy §4.2 / §6).
    pub fn interpretation_prompt(&self, chat_guard: &str, lang: &str) -> String {
        let instruction = match lang {
            "ja" => "上記は xoksa が算出した確定済みのマルチタイムフレーム・データです。これを解釈してください。",
            _ => "The above is confirmed multi-timeframe data computed by xoksa. Interpret it.",
        };
        format!(
            "{}\n\n{}\n\n{}",
            crate::chat::guard::constraint_text(chat_guard, lang),
            self.context_pack_text,
            instruction
        )
    }
}

/// Map an indicator name to the extension that must be enabled to compute it.
/// RSI/MACD are always computed (basic), so they map to `None`.
fn indicator_to_ext(name: &str) -> Option<ExtensionIndicator> {
    use ExtensionIndicator::*;
    match name.to_ascii_uppercase().as_str() {
        "EMA" => Some(Ema),
        "SMA" => Some(Sma),
        "BBANDS" | "BOLLINGER" | "BB" => Some(Bollinger),
        "ROC" => Some(Roc),
        "ADX" => Some(Adx),
        "STOCH" | "STOCHASTICS" => Some(Stochastics),
        "FIB" | "FIBONACCI" => Some(Fibonacci),
        "VWAP" => Some(Vwap),
        "ICHIMOKU" | "ICHI" => Some(Ichimoku),
        _ => None,
    }
}

/// Structured indicator values from a guard, or `None` if unavailable.
fn indicator_values(g: &TechnicalDataGuard, name: &str) -> Option<Value> {
    match name.to_ascii_uppercase().as_str() {
        "RSI" => Some(json!({ "rsi": g.get_rsi() })),
        "MACD" => Some(json!({ "macd": g.get_macd() })),
        "EMA" => Some(json!({ "short": g.get_ema_short(), "long": g.get_ema_long() })),
        "SMA" => Some(json!({ "short": g.get_sma_short(), "long": g.get_sma_long() })),
        "BBANDS" | "BOLLINGER" | "BB" => Some(json!({
            "upper": g.get_bb_upper(), "lower": g.get_bb_lower(),
            "percent_b": g.get_bb_percent_b(), "bandwidth": g.get_bb_bandwidth(),
        })),
        "ROC" => g.get_roc().map(|v| json!({ "roc": v })),
        "ADX" => g.get_adx().map(|v| json!({ "adx": v })),
        "STOCH" | "STOCHASTICS" => match (g.get_stochastics_k(), g.get_stochastics_d()) {
            (Some(k), Some(d)) => Some(json!({ "k": k, "d": d })),
            _ => None,
        },
        "FIB" | "FIBONACCI" => g.get_fibo_50_0().map(|v| json!({ "fib50": v })),
        "VWAP" => g.get_vwap().map(|v| json!({ "vwap": v })),
        "ICHIMOKU" | "ICHI" => match (g.get_tenkan_sen(), g.get_kijun_sen()) {
            (Some(t), Some(k)) => Some(json!({ "tenkan": t, "kijun": k })),
            _ => None,
        },
        _ => None,
    }
}

/// Format an f64 readably (2 dp for prices, 4 dp for small values).
fn fmt_num(v: f64) -> String {
    if v.abs() >= 100.0 {
        format!("{v:.2}")
    } else {
        format!("{v:.4}")
    }
}

/// Render a JSON object of numeric values as "key: value, …".
fn render_values(v: &Value) -> String {
    v.as_object()
        .map(|o| {
            o.iter()
                .map(|(k, val)| match val.as_f64() {
                    Some(n) => format!("{k}: {}", fmt_num(n)),
                    None => format!("{k}: {val}"),
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

/// Build the multi-timeframe context pack. `base` is the per-symbol config; each
/// requested timeframe is analyzed once (with exactly the requested indicators
/// enabled) through the shared engine.
pub async fn build_context_pack(
    base: &Config,
    spec: &ContextSpec,
    name_map: &HashMap<String, String>,
) -> Result<ContextPack> {
    // Which extensions each timeframe needs (so the engine computes them).
    let mut needed: HashMap<&'static str, (AnalysisMode, HashSet<ExtensionIndicator>)> =
        HashMap::new();
    for it in &spec.contexts {
        if let Some(mode) = AnalysisMode::from_value(&it.timeframe) {
            let e = needed
                .entry(mode.as_str())
                .or_insert((mode, HashSet::new()));
            if it.kind == "indicator" {
                if let Some(ext) = it.indicator_name.as_deref().and_then(indicator_to_ext) {
                    e.1.insert(ext);
                }
            }
        }
    }

    // Analyze each timeframe once. `configs` keeps the per-timeframe config so
    // the confirmed data is derived with the same settings the analysis used.
    let mut guards: HashMap<&'static str, TechnicalDataGuard> = HashMap::new();
    let mut configs: HashMap<&'static str, (AnalysisMode, Config)> = HashMap::new();
    for (tok, (mode, exts)) in &needed {
        let mut cfg = base.clone();
        cfg.analysis_mode = *mode;
        cfg.enabled_extensions = exts.iter().cloned().collect();
        cfg.no_news = true;
        cfg.no_llm = true;
        cfg.silent = true;
        if let Ok(g) = crate::app::build_analyzed_guard(&cfg, &spec.symbol, name_map).await {
            guards.insert(tok, g);
            configs.insert(tok, (*mode, cfg));
        }
    }

    // Confirmed data, per timeframe, straight from the guards just built.
    let facts =
        crate::integrity::facts_from_parts(configs.iter().filter_map(|(tok, (mode, cfg))| {
            guards.get(tok).map(|g| {
                crate::integrity::SymbolFacts::from_sources(g, cfg, None, None)
                    .with_timeframe(*mode)
            })
        }));

    let lang = base.lang.as_str();
    // Base time: the latest market-data time of any analyzed timeframe.
    let basis = guards
        .values()
        .find_map(|g| g.get_market_data_latest_time().map(str::to_string))
        .or_else(|| spec.as_of.clone())
        .unwrap_or_default();

    let mut text = if lang == "ja" {
        format!("銘柄: {}\n基準時刻: {}\n", spec.symbol, basis)
    } else {
        format!("Symbol: {}\nAs of: {}\n", spec.symbol, basis)
    };
    let mut items_json: Vec<Value> = Vec::new();

    for it in &spec.contexts {
        let Some(mode) = AnalysisMode::from_value(&it.timeframe) else {
            continue;
        };
        let Some(g) = guards.get(mode.as_str()) else {
            continue;
        };
        let tf_label = mode.bar_label(lang);
        match it.kind.as_str() {
            "indicator" => {
                let Some(name) = it.indicator_name.as_deref() else {
                    continue;
                };
                if let Some(values) = indicator_values(g, name) {
                    text.push_str(&format!(
                        "\n【{tf_label} {name}】\n- {}\n",
                        render_values(&values)
                    ));
                    items_json.push(json!({
                        "type": "indicator", "timeframe": mode.as_str(),
                        "indicator_name": name.to_ascii_uppercase(), "values": values,
                    }));
                }
            }
            "price" => {
                // Same price/percent basis as the terminal and LLM headers (SOT §4.2).
                let close = crate::utils::display_price_for_diff(g);
                let prev = g.get_previous_close();
                let (_, pct) = crate::utils::displayed_price_diff(g);
                if lang == "ja" {
                    text.push_str(&format!(
                        "\n【{tf_label} 価格】\n- 最新: {close:.2}（前足: {prev:.2}, 前足比 {pct:+.2}%）\n"
                    ));
                } else {
                    text.push_str(&format!(
                        "\n【{tf_label} price】\n- latest: {close:.2} (prev: {prev:.2}, change {pct:+.2}%)\n"
                    ));
                }
                items_json.push(json!({
                    "type": "price", "timeframe": mode.as_str(),
                    "close": close, "previous_close": prev, "change_pct": pct,
                }));
            }
            _ => {}
        }
    }

    Ok(ContextPack {
        symbol: spec.symbol.clone(),
        as_of: spec.as_of.clone(),
        context_pack_text: text,
        context_pack_json: json!({ "symbol": spec.symbol, "as_of": basis, "items": items_json }),
        facts,
    })
}
