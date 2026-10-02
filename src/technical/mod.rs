//! Technical indicators and analysis engine

pub mod composite;
pub mod indicators;
pub mod types;

pub use composite::{calculate_final_score_snapshot, calculate_score_gauge};
pub use indicators::{
    build_basic_technical_entry, evaluate_all_selected_extensions,
    evaluate_all_selected_extensions_with_report, get_extension_evaluator,
    ExtensionEvaluationFailure, ExtensionEvaluationReport,
};
pub use types::{AnalysisResult, TechnicalDataEntry, TechnicalDataGuard};
