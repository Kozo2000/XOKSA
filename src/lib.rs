//! xoksa - Stock Technical 'AI' Analysis Engine
//!
//! A modular Rust library for technical stock analysis with configurable indicators,
//! news integration, and LLM-powered insights.

// The engine contains no `unsafe` — enforce it at compile time, not just by claim.
#![forbid(unsafe_code)]

pub mod app;
pub mod backtest;
pub mod bootstrap;
pub mod chat;
pub mod config;
pub mod context;
pub mod fundamental;
pub mod integrity;
pub mod keystore;
pub mod llm;
pub mod logging;
pub mod market;
pub mod news;
pub mod notify;
pub mod output;
pub mod private;
pub mod prompt;
pub mod render;
pub(crate) mod render_indicators;
pub mod report;
pub mod server;
pub mod setup;
pub mod strategies;
pub mod technical;
pub mod traits;
pub mod utils;

// Re-export key types
pub use config::{AnalysisMode, Args, CliValueSources, Config};
#[allow(deprecated)]
pub use market::{MarketData, MarketDataSnapshot, MarketLatestObservation};
pub use technical::types::{TechnicalDataEntry, TechnicalDataGuard};
