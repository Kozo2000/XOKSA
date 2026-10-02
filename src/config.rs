//! Configuration management and structure definitions

use clap::parser::ValueSource;
use clap::{ArgMatches, FromArgMatches, Parser};
use std::collections::HashMap;
use std::env;

/// Extension technical indicator enum (user selections stored in a Vec)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExtensionIndicator {
    Ema,
    Sma,
    Bollinger,
    Roc,
    Adx,
    Stochastics,
    Fibonacci,
    Vwap,
    Ichimoku,
}

/// Investment stance enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stance {
    Buyer,
    Seller,
    Holder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnalysisMode {
    Daily,
    Intraday1m,
    Intraday5m,
    Intraday15m,
    Intraday30m,
    Intraday60m,
    Weekly,
    Monthly,
}

impl AnalysisMode {
    /// All bar modes in canonical display order — the single source for any UI
    /// that lists the available timeframes (e.g. the Web dashboard dropdown).
    pub const ALL: [AnalysisMode; 8] = [
        Self::Daily,
        Self::Intraday1m,
        Self::Intraday5m,
        Self::Intraday15m,
        Self::Intraday30m,
        Self::Intraday60m,
        Self::Weekly,
        Self::Monthly,
    ];

    pub fn from_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "daily" | "day" | "1d" => Some(Self::Daily),
            "1m" | "1min" => Some(Self::Intraday1m),
            "5m" => Some(Self::Intraday5m),
            "15m" => Some(Self::Intraday15m),
            "intraday" | "short" | "30m" => Some(Self::Intraday30m),
            "60m" | "1h" | "hourly" => Some(Self::Intraday60m),
            "weekly" | "week" | "1wk" => Some(Self::Weekly),
            "monthly" | "month" | "1mo" => Some(Self::Monthly),
            _ => None,
        }
    }

    /// Canonical lowercase label (inverse of the primary `from_value` tokens).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Intraday1m => "1m",
            Self::Intraday5m => "5m",
            Self::Intraday15m => "15m",
            Self::Intraday30m => "30m",
            Self::Intraday60m => "60m",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
        }
    }

    pub fn data_interval(self) -> &'static str {
        match self {
            Self::Daily => "1d",
            Self::Intraday1m => "1m",
            Self::Intraday5m => "5m",
            Self::Intraday15m => "15m",
            Self::Intraday30m => "30m",
            Self::Intraday60m => "60m",
            Self::Weekly => "1wk",
            Self::Monthly => "1mo",
        }
    }

    pub fn data_range(self) -> &'static str {
        match self {
            Self::Daily => "3mo",
            // Yahoo allows the 1m interval only for a short window (~7 days).
            Self::Intraday1m => "5d",
            Self::Intraday5m => "5d",
            Self::Intraday15m => "1mo",
            Self::Intraday30m => "1mo",
            Self::Intraday60m => "2mo",
            Self::Weekly => "2y",
            Self::Monthly => "10y",
        }
    }

    /// Selectable backtest period tokens (Yahoo `range` values) for this timeframe,
    /// short→long, each WITHIN the provider's real max history for the interval
    /// (measured: 1m≈7d, 5m/15m/30m≈60d, 60m≈2y, daily/weekly/monthly = years).
    /// The GUI shows these when the timeframe is selected — no fixed default is
    /// imposed; the user picks. Single source (exposed via /api/config).
    pub fn backtest_period_ranges(self) -> &'static [&'static str] {
        match self {
            Self::Intraday1m => &["7d"],
            Self::Intraday5m => &["7d", "1mo", "60d"],
            Self::Intraday15m => &["7d", "1mo", "60d"],
            Self::Intraday30m => &["1mo", "60d"],
            Self::Intraday60m => &["1mo", "3mo", "6mo", "1y", "2y"],
            Self::Daily => &["3mo", "6mo", "1y", "2y", "5y", "10y"],
            Self::Weekly => &["1y", "2y", "5y", "10y"],
            Self::Monthly => &["5y", "10y", "max"],
        }
    }

    pub fn is_intraday(self) -> bool {
        self.intraday_minutes().is_some()
    }

    pub fn intraday_minutes(self) -> Option<u32> {
        match self {
            Self::Daily => None,
            Self::Intraday1m => Some(1),
            Self::Intraday5m => Some(5),
            Self::Intraday15m => Some(15),
            Self::Intraday30m => Some(30),
            Self::Intraday60m => Some(60),
            Self::Weekly | Self::Monthly => None,
        }
    }

    pub fn label_ja(self) -> &'static str {
        match self {
            Self::Daily => "日足分析",
            Self::Intraday1m => "1分足短期分析",
            Self::Intraday5m => "5分足短期分析",
            Self::Intraday15m => "15分足短期分析",
            Self::Intraday30m => "30分足短期分析",
            Self::Intraday60m => "1時間足分析",
            Self::Weekly => "週足分析",
            Self::Monthly => "月足分析",
        }
    }

    pub fn context_ja(self) -> &'static str {
        match self {
            Self::Daily => "日足データ / 中期分析",
            Self::Intraday1m => "1分足データ / 短期分析",
            Self::Intraday5m => "5分足データ / 短期分析",
            Self::Intraday15m => "15分足データ / 短期分析",
            Self::Intraday30m => "30分足データ / 短期分析",
            Self::Intraday60m => "1時間足データ / 短期分析",
            Self::Weekly => "週足データ / 上位足分析",
            Self::Monthly => "月足データ / 長期分析",
        }
    }

    pub fn period_context_ja(self, periods: usize) -> String {
        match self {
            Self::Daily
            | Self::Intraday1m
            | Self::Intraday5m
            | Self::Intraday15m
            | Self::Intraday30m
            | Self::Intraday60m
            | Self::Weekly
            | Self::Monthly => {
                format!("{}{periods}本", self.bar_label_ja())
            }
        }
    }

    pub fn bar_label_ja(self) -> &'static str {
        match self {
            Self::Daily => "日足",
            Self::Intraday1m => "1分足",
            Self::Intraday5m => "5分足",
            Self::Intraday15m => "15分足",
            Self::Intraday30m => "30分足",
            Self::Intraday60m => "1時間足",
            Self::Weekly => "週足",
            Self::Monthly => "月足",
        }
    }

    pub fn previous_close_label(self) -> &'static str {
        match self {
            Self::Daily => "前営業日終値",
            Self::Intraday1m
            | Self::Intraday5m
            | Self::Intraday15m
            | Self::Intraday30m
            | Self::Intraday60m => "前足終値",
            Self::Weekly => "前週終値",
            Self::Monthly => "前月終値",
        }
    }

    pub fn price_diff_label(self) -> &'static str {
        match self {
            Self::Daily => "前営業日比",
            Self::Intraday1m
            | Self::Intraday5m
            | Self::Intraday15m
            | Self::Intraday30m
            | Self::Intraday60m => "前足比",
            Self::Weekly => "前週比",
            Self::Monthly => "前月比",
        }
    }

    pub fn label(self, lang: &str) -> &'static str {
        if lang == "ja" {
            return self.label_ja();
        }
        match self {
            Self::Daily => "Daily analysis",
            Self::Intraday1m => "1min short-term analysis",
            Self::Intraday5m => "5min short-term analysis",
            Self::Intraday15m => "15min short-term analysis",
            Self::Intraday30m => "30min short-term analysis",
            Self::Intraday60m => "60min analysis",
            Self::Weekly => "Weekly analysis",
            Self::Monthly => "Monthly analysis",
        }
    }

    pub fn context(self, lang: &str) -> &'static str {
        if lang == "ja" {
            return self.context_ja();
        }
        match self {
            Self::Daily => "daily data / mid-term analysis",
            Self::Intraday1m => "1min data / short-term analysis",
            Self::Intraday5m => "5min data / short-term analysis",
            Self::Intraday15m => "15min data / short-term analysis",
            Self::Intraday30m => "30min data / short-term analysis",
            Self::Intraday60m => "60min data / short-term analysis",
            Self::Weekly => "weekly data / higher-timeframe analysis",
            Self::Monthly => "monthly data / long-term analysis",
        }
    }

    pub fn period_context(self, lang: &str, periods: usize) -> String {
        if lang == "ja" {
            return self.period_context_ja(periods);
        }
        match self {
            Self::Daily => format!("{periods} daily bars"),
            Self::Intraday1m => format!("{periods} 1min bars"),
            Self::Intraday5m => format!("{periods} 5min bars"),
            Self::Intraday15m => format!("{periods} 15min bars"),
            Self::Intraday30m => format!("{periods} 30min bars"),
            Self::Intraday60m => format!("{periods} 60min bars"),
            Self::Weekly => format!("{periods} weekly bars"),
            Self::Monthly => format!("{periods} monthly bars"),
        }
    }

    pub fn bar_label(self, lang: &str) -> &'static str {
        if lang == "ja" {
            return self.bar_label_ja();
        }
        match self {
            Self::Daily => "daily bar",
            Self::Intraday1m => "1min bar",
            Self::Intraday5m => "5min bar",
            Self::Intraday15m => "15min bar",
            Self::Intraday30m => "30min bar",
            Self::Intraday60m => "60min bar",
            Self::Weekly => "weekly bar",
            Self::Monthly => "monthly bar",
        }
    }

    pub fn previous_close_label_l(self, lang: &str) -> &'static str {
        if lang == "ja" {
            return self.previous_close_label();
        }
        match self {
            Self::Daily => "Prev close",
            Self::Intraday1m
            | Self::Intraday5m
            | Self::Intraday15m
            | Self::Intraday30m
            | Self::Intraday60m => "Prev bar close",
            Self::Weekly => "Prev weekly close",
            Self::Monthly => "Prev monthly close",
        }
    }

    pub fn price_diff_label_l(self, lang: &str) -> &'static str {
        if lang == "ja" {
            return self.price_diff_label();
        }
        match self {
            Self::Daily => "Change vs prev close",
            Self::Intraday1m
            | Self::Intraday5m
            | Self::Intraday15m
            | Self::Intraday30m
            | Self::Intraday60m => "Change vs prev bar",
            Self::Weekly => "Change vs prev week",
            Self::Monthly => "Change vs prev month",
        }
    }
}

impl std::fmt::Display for AnalysisMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Daily => write!(f, "daily"),
            Self::Intraday1m => write!(f, "1m"),
            Self::Intraday5m => write!(f, "5m"),
            Self::Intraday15m => write!(f, "15m"),
            Self::Intraday30m => write!(f, "30m"),
            Self::Intraday60m => write!(f, "60m"),
            Self::Weekly => write!(f, "weekly"),
            Self::Monthly => write!(f, "monthly"),
        }
    }
}

/// clap value parser for the `--analysis-mode` / `--chat-analysis-mode` flags.
/// Validation is delegated to `AnalysisMode::from_value` — the single source of
/// truth for accepted tokens — so adding a new bar mode there makes the flag
/// accept it with no list to maintain here (and nothing to drift out of sync).
/// The keeps the value as a `String` (resolved to a mode later) to match the
/// existing field types.
fn parse_analysis_mode_arg(s: &str) -> Result<String, String> {
    if AnalysisMode::from_value(s).is_some() {
        Ok(s.to_string())
    } else {
        let valid = AnalysisMode::ALL
            .iter()
            .map(|m| m.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Err(format!(
            "invalid bar mode '{s}' (valid: {valid}; common aliases like 1h/1d/1wk also accepted)"
        ))
    }
}

/// Display name (used for gauge heading)
impl std::fmt::Display for Stance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Stance::Buyer => write!(f, "Buyer"),
            Stance::Holder => write!(f, "Holder"),
            Stance::Seller => write!(f, "Seller"),
        }
    }
}

/// CLI argument structure definition
#[derive(Parser, Debug)]
#[command(
    name = "xoksa",
    version,
    about = "Bridging Classic Rigor with Future Intelligence Stock Technical 'AI' Analysis Engine. Created & Designed by Kozo2000"
)]
pub struct Args {
    #[arg(
        short = 't',
        long,
        required_unless_present_any = [
            "show_log_header", "init", "doctor", "chat", "update_key", "check_keys",
        ],
        help = "Specify ticker symbol (e.g., AAPL, MSFT, 7203.T). Note: If the ticker contains special characters (e.g., '&'), enclose it in quotes. Example: 'S&P500'"
    )]
    pub ticker: Option<String>,
    #[arg(
        long,
        default_value = "daily",
        value_parser = parse_analysis_mode_arg,
        help = "Analysis data mode: daily=1d/3mo, 60m=60m/2mo, intraday|short|30m=30m/1mo, 15m=15m/1mo, 5m=5m/5d, 1m=1m/5d, weekly=1wk/2y, monthly=1mo/10y"
    )]
    pub analysis_mode: String,
    #[arg(
        long,
        help = "Do NOT read indicator/scoring-related settings from xoksa.env (thresholds/extensions/stance/weights)."
    )]
    #[arg(
        short = 'I',
        long,
        help = "Do NOT read indicator/scoring-related settings from xoksa.env (thresholds/extensions/stance/weights)."
    )]
    pub no_env_indicators: bool,
    #[arg(
        long,
        default_value_t = Config::default().buy_rsi,
        help = "RSI threshold to detect oversold (default: 30)"
    )]
    pub buy_rsi: f64,
    #[arg(
        long,
        default_value_t = Config::default().sell_rsi,
        help = "RSI threshold to detect overbought (default: 70)"
    )]
    pub sell_rsi: f64,
    #[arg(
        long,
        default_value_t = Config::default().macd_diff_low,
        help = "Threshold for MACD small difference (default: 2)"
    )]
    pub macd_diff_low: f64,
    #[arg(
        long,
        default_value_t = Config::default().macd_diff_mid,
        help = "Threshold for MACD medium difference (default: 10)"
    )]
    pub macd_diff_mid: f64,
    #[arg(
        long,
        default_value_t = Config::default().macd_diff_extreme,
        help = "Threshold for MACD extreme divergence with overbought RSI (default: 100)"
    )]
    pub macd_diff_extreme: f64,
    #[arg(short = 'O', long, help = "Skip LLM access completely")]
    pub no_llm: bool,
    #[arg(
        long,
        value_name = "FILE",
        help = "Write the analysis report to a file (format by extension: .html / .md / otherwise plain text)"
    )]
    pub out: Option<String>,
    #[arg(long,
    default_value = "openai",
    value_parser = ["openai", "gemini", "claude", "ollama"],
    help = "LLM provider to use (openai|gemini|claude|ollama)")]
    pub llm_provider: String,
    #[arg(
        short = 'm',
        long,
        help = "Allow buy signals even if MACD is in negative zone"
    )]
    pub macd_minus_ok: bool,
    #[arg(
        short = 'd',
        long,
        help = "Save the LLM prompt to a file (debug_prompt_<timestamp>.txt); add --no-llm to skip the LLM call"
    )]
    pub debug_prompt: bool,
    #[arg(
        long,
        help = "Print Ollama request options and response metadata for tuning"
    )]
    pub debug_ollama: bool,
    #[arg(
        long = "ollama-bench",
        value_delimiter = ',',
        help = "Comma-separated Ollama models to benchmark (enables benchmark mode)"
    )]
    pub ollama_bench_models: Vec<String>,
    #[arg(
        long = "ollama-bench-format",
        default_value = "table",
        value_parser = ["table", "csv", "json"],
        help = "Benchmark output format (table|csv|json)"
    )]
    pub ollama_bench_format: String,
    #[arg(
        long,
        help = "Disable the LLM output integrity check for all providers (trust the model)"
    )]
    pub no_ollama_guard: bool,
    #[arg(
        long,
        value_parser = ["false", "true", "low", "medium", "high"],
        help = "Set Ollama thinking mode (false|true|low|medium|high)"
    )]
    pub ollama_think: Option<String>,
    #[arg(
        short = 'M',
        long,
        default_value_t = Config::default().openai_model,
        help = "Specify OpenAI model to use"
    )]
    pub openai_model: String,
    #[arg(
        long,
        help = "LLM model name to use (overrides provider-specific model setting)"
    )]
    pub llm_model: Option<String>,
    #[arg(
        short = 'a',
        long,
        help = "Skip alias expansion and search news using ticker/formal name only"
    )]
    pub no_alias: bool,
    #[arg(
        short = 'n',
        long,
        help = "Skip news search entirely (technical analysis only)"
    )]
    pub no_news: bool,
    #[clap(long)]
    pub alias_csv: Option<String>,
    #[arg(long, help = "Enable EMA (Exponential Moving Average) analysis")]
    pub ema: bool,
    #[arg(long, help = "Enable SMA (Simple Moving Average) analysis")]
    pub sma: bool,
    #[arg(long, help = "Enable Bollinger Bands analysis")]
    pub bollinger: bool,
    #[arg(long, help = "Enable Fibonacci retracement analysis")]
    pub fibonacci: bool,
    #[arg(long, help = "Enable Stochastics (%K and %D) analysis")]
    pub stochastics: bool,
    #[arg(long, help = "Enable ADX (trend strength) analysis")]
    pub adx: bool,
    #[arg(long, help = "Enable ROC (Rate of Change) analysis")]
    pub roc: bool,
    #[arg(long, help = "Enable VWAP analysis")]
    pub vwap: bool,
    #[arg(long, help = "Enable Ichiomku analysis")]
    pub ichimoku: bool,
    #[arg(
        long,
        default_value_t = Config::default().bb_bandwidth_squeeze_pct,
        help = "Bollinger Bandwidth threshold (%) for squeeze detection"
    )]
    pub bb_bandwidth_squeeze_pct: f64,
    #[arg(long, default_value_t = Config::default().ema_short_period, help = "EMA short period (default: 5)")]
    pub ema_short_period: usize,
    #[arg(long, default_value_t = Config::default().ema_long_period, help = "EMA long period (default: 20)")]
    pub ema_long_period: usize,
    #[arg(long, default_value_t = Config::default().sma_short_period, help = "SMA short period (default: 5)")]
    pub sma_short_period: usize,
    #[arg(long, default_value_t = Config::default().sma_long_period, help = "SMA long period (default: 20)")]
    pub sma_long_period: usize,
    #[arg(long, default_value_t = Config::default().roc_period, help = "ROC lookback period (default: 10)")]
    pub roc_period: usize,
    #[arg(long, default_value_t = Config::default().adx_period, help = "ADX period (default: 14)")]
    pub adx_period: usize,
    #[arg(
        long,
        default_value_t = Config::default().stochastics_period,
        help = "Stochastics %K period (default: 14)"
    )]
    pub stochastics_period: usize,
    #[arg(long, default_value_t = Config::default().bollinger_period, help = "Bollinger period (default: 20)")]
    pub bollinger_period: usize,
    #[arg(
        long,
        default_value_t = Config::default().bollinger_stddev_multiplier,
        help = "Bollinger stddev multiplier (default: 2.0)"
    )]
    pub bollinger_stddev_multiplier: f64,
    #[arg(long, default_value_t = Config::default().vwap_period, help = "VWAP period (default: 14)")]
    pub vwap_period: usize,
    #[arg(
        long,
        default_value_t = Config::default().ichimoku_tenkan_period,
        help = "Ichimoku tenkan period (default: 9)"
    )]
    pub ichimoku_tenkan_period: usize,
    #[arg(
        long,
        default_value_t = Config::default().ichimoku_kijun_period,
        help = "Ichimoku kijun period (default: 26)"
    )]
    pub ichimoku_kijun_period: usize,
    #[arg(
        long,
        default_value_t = Config::default().fibonacci_neutral_ratio,
        help = "Fibonacci neutral band, as a share of the 50%→38.2% distance (0.0-0.5, default: 0.05)"
    )]
    pub fibonacci_neutral_ratio: f64,
    #[arg(
        long,
        default_value_t = Config::default().weight_basic,
        help = "Weight multiplier for Basic score (0.5-3.0)"
    )]
    pub weight_basic: f64,
    #[arg(
        long,
        default_value_t = Config::default().weight_ema,
        help = "Weight multiplier for EMA score (0.5-3.0)"
    )]
    pub weight_ema: f64,
    #[arg(
        long,
        default_value_t = Config::default().weight_sma,
        help = "Weight multiplier for SMA score (0.5-3.0)"
    )]
    pub weight_sma: f64,
    #[arg(
        long,
        default_value_t = Config::default().weight_bollinger,
        help = "Weight multiplier for Bollinger score (0.5-3.0)"
    )]
    pub weight_bollinger: f64,
    #[arg(
        long,
        default_value_t = Config::default().weight_roc,
        help = "Weight multiplier for ROC score (0.5-3.0)"
    )]
    pub weight_roc: f64,
    #[arg(
        long,
        default_value_t = Config::default().weight_adx,
        help = "Weight multiplier for ADX score (0.5-3.0)"
    )]
    pub weight_adx: f64,
    #[arg(
        long,
        default_value_t = Config::default().weight_stochastics,
        help = "Weight multiplier for Stochastics score (0.5-3.0)"
    )]
    pub weight_stochastics: f64,
    #[arg(
        long,
        default_value_t = Config::default().weight_fibonacci,
        help = "Weight multiplier for Fibonacci score (0.5-3.0)"
    )]
    pub weight_fibonacci: f64,
    #[arg(
        long,
        default_value_t = Config::default().weight_vwap,
        help = "Weight multiplier for VWAP score (0.5-3.0)"
    )]
    pub weight_vwap: f64,
    #[arg(
        long,
        default_value_t = Config::default().weight_ichimoku,
        help = "Weight multiplier for Ichimoku score (0.5-3.0)"
    )]
    pub weight_ichimoku: f64,
    #[arg(long, value_parser = ["buyer","seller","holder"], default_value = "holder",
      help = "Analysis stance: buyer|seller|holder (default: holder)")]
    pub stance: String,
    #[arg(short = 'q', long, help = "Specify a custom news search query")]
    pub custom_news_query: Option<String>,
    #[arg(
        long,
        help = "Filter news query with finance terms (default: False; set NEWS_FILTER=True to enable)"
    )]
    pub news_filter: bool,
    #[arg(long, value_parser = clap::value_parser!(usize), help = "Max news items to fetch (1..50). Defaults: OFF=30, ON=20, or NEWS_COUNT")]
    pub news_count: Option<usize>,
    #[arg(long, value_parser = ["pd","pw","pm","py","all"], help = "News freshness (pd|pw|pm|py|all). Defaults: OFF=pm, ON=pw, or NEWS_FRESHNESS")]
    pub news_freshness: Option<String>,
    #[arg(long, help = "Show news in terminal output")]
    pub show_news: bool,
    #[arg(
        short = 'x',
        long,
        alias = "openai-extra-note",
        help = "Add an extra note to the LLM prompt"
    )]
    pub extra_note: Option<String>,
    #[arg(long, help = "Save technical analysis log (CSV or JSON)")]
    pub save_technical_log: bool,
    #[arg(
        long,
        default_value = "json",
        value_parser = ["csv", "json"],
        help = "Technical log format (csv or json)"
    )]
    pub log_format: String,
    #[arg(
        long,
        help = "Specify directory to save technical logs",
        default_value_t = Config::default().log_dir
    )]
    pub log_dir: String,
    #[arg(
        long,
        default_value_t = Config::default().max_note_length,
        help = "Max characters for key points section"
    )]
    pub max_note_length: usize,
    #[arg(
        long,
        default_value_t = Config::default().max_shortterm_length,
        help = "Max characters for report section 2"
    )]
    pub max_shortterm_length: usize,
    #[arg(
        long,
        default_value_t = Config::default().max_midterm_length,
        help = "Max characters for report section 3"
    )]
    pub max_midterm_length: usize,
    #[arg(
        long,
        default_value_t = Config::default().max_news_length,
        help = "Max characters for news highlights section"
    )]
    pub max_news_length: usize,
    #[arg(
        long,
        default_value_t = Config::default().max_review_length,
        help = "Max characters for overall assessment section"
    )]
    pub max_review_length: usize,
    #[arg(
        long,
        help = "Append to existing CSV file instead of creating a new one"
    )]
    pub data_append: bool,
    #[arg(long, help = "Omit ticker-based subdirectory for log file")]
    pub log_flat: bool,
    #[arg(
        long,
        help = "Output log (CSV or JSON) to standard output instead of file"
    )]
    pub stdout_log: bool,
    #[arg(
        long,
        help = "Show only CSV header row based on current options and exit"
    )]
    pub show_log_header: bool,
    #[arg(
        long,
        help = "Suppress all normal output (except for errors) for batch execution"
    )]
    pub silent: bool,
    #[arg(
        long,
        help = "Display parsed command-line arguments (for debugging purposes)"
    )]
    pub debug_args: bool,
    #[arg(
        long,
        help = "Fetch and display fundamental data (Japan: J-Quants, US: SEC EDGAR)"
    )]
    pub fundamental: bool,
    #[arg(
        long,
        conflicts_with = "doctor",
        help = "Run interactive setup wizard to generate xoksa.env"
    )]
    pub init: bool,
    #[arg(
        long,
        conflicts_with = "init",
        help = "Run configuration diagnostics and report xoksa.env status"
    )]
    pub doctor: bool,
    #[arg(
        long,
        conflicts_with_all = &["init", "doctor", "check_keys", "chat", "ollama_bench_models"],
        help = "Update a stored API key in the OS Keychain"
    )]
    pub update_key: bool,
    #[arg(
        long,
        conflicts_with_all = &["init", "doctor", "update_key", "chat", "ollama_bench_models"],
        help = "Show which API keys are stored (values are never displayed)"
    )]
    pub check_keys: bool,
    #[arg(
        long,
        value_name = "PATH",
        help = "Use this xoksa.env instead of the canonical app-data path (there is no implicit ./xoksa.env)"
    )]
    pub env_file: Option<String>,
    #[arg(
        long,
        conflicts_with_all = &["init", "doctor", "ollama_bench_models"],
        help = "Start interactive chat after analysis completes"
    )]
    pub chat: bool,
    #[arg(
        long,
        default_value = "mid",
        value_parser = ["low", "mid", "high"],
        help = "Chat context retention level: low|mid|high (default: mid)"
    )]
    pub chat_memory: String,
    #[arg(
        long,
        default_value = "high",
        value_parser = ["low", "mid", "high"],
        help = "Chat investment-advice guard level: high|mid|low (default: high)"
    )]
    pub chat_guard: String,
    #[arg(
        long,
        default_value = "summary",
        value_parser = ["off", "summary", "claims"],
        help = "LLM debate buffer mode in chat: off|summary|claims (default: summary)"
    )]
    pub debate: String,
    #[arg(
        long,
        help = "Enable auto-reload timer in intraday chat mode (disabled for non-intraday modes)"
    )]
    pub autoreload: bool,
    #[arg(
        long,
        help = "Print notification when auto-reload fires (default: silent)"
    )]
    pub autoreload_notify: bool,
    #[arg(
        long,
        default_value = "daily",
        value_parser = parse_analysis_mode_arg,
        help = "Analysis mode for chat mode: daily|60m|30m|15m|5m|1m|weekly|monthly (default: falls back to --mode)"
    )]
    pub chat_mode: String,
    #[arg(
        long,
        default_value = "en",
        value_parser = ["en", "ja"],
        help = "Output language: en|ja (default: en)"
    )]
    pub lang: String,
}

#[derive(Debug, Clone)]
pub struct OllamaInstance {
    pub alias: String,
    pub model: String,
    pub host: String,
    pub port: u16,
}

/// One chat-notification channel (`NOTIFY_<n>_*`). The secret is deliberately NOT
/// held here — it is Class A (`NOTIFY_<n>_SECRET`) and resolved from the keychain
/// only at dispatch time, never carried in the config or the env map.
#[derive(Debug, Clone)]
pub struct NotifyChannel {
    /// The `<n>` suffix (1..=16); an alert references a channel by this number.
    pub n: u8,
    pub kind: crate::notify::NotifierKind,
    /// A human label for `/alert` listings (`NOTIFY_<n>_NAME`).
    pub name: String,
    /// LINE destination id (`NOTIFY_<n>_TO`); unused by the webhook kinds.
    pub to: Option<String>,
}

/// One alert rule (`ALERT_<n>_*`): watch `ticker` on `mode`, and fire channel
/// `notify` when the single condition `when` holds on the latest fetched bar.
/// That bar is not necessarily closed: on Japanese intraday timeframes it is the
/// still-forming one carrying the real-time quote, so a crossing is caught within
/// the bar rather than only after it closes — see [`crate::server::monitor`].
#[derive(Debug, Clone)]
pub struct AlertRule {
    /// The `<n>` suffix (1..=16).
    pub n: u8,
    pub ticker: String,
    /// Bar mode to watch; `None` lets the monitor fall back to its default.
    pub mode: Option<AnalysisMode>,
    /// The single trigger condition, reusing the backtest evaluator (SOT).
    pub when: crate::backtest::Condition,
    /// The `NOTIFY_<n>` channel number to dispatch to.
    pub notify: u8,
    /// Whether the LLM appends a one-line note (default off; §1 output guard applies).
    pub explain: bool,
}

/// Runtime configuration
#[derive(Debug, Clone)]
pub struct Config {
    pub analysis_mode: AnalysisMode,
    pub no_env_indicators: bool,
    pub buy_rsi: f64,
    pub sell_rsi: f64,
    pub macd_diff_low: f64,
    pub macd_diff_mid: f64,
    pub macd_diff_extreme: f64,
    pub macd_minus_ok: bool,
    /// Indicators COMPUTED and STORED (the log/CSV baseline, from env). Never changed
    /// at runtime — the analysis-record column set stays stable.
    pub enabled_extensions: Vec<ExtensionIndicator>,
    /// Indicators ACTIVE for this session's analysis — the subset of
    /// `enabled_extensions` used by the composite score, the display, and the LLM
    /// prompt. `None` means "all enabled" (the default); `/set indicator <name> off/on`
    /// sets an explicit subset at runtime WITHOUT touching computation or the log/CSV
    /// columns (a deactivated indicator is still computed and stored, just not
    /// scored/shown/sent). Read it via `Config::analysis_extensions()`.
    pub active_extensions: Option<Vec<ExtensionIndicator>>,
    pub bb_bandwidth_squeeze_pct: f64,
    pub ema_short_period: usize,
    pub ema_long_period: usize,
    pub sma_short_period: usize,
    pub sma_long_period: usize,
    pub roc_period: usize,
    pub adx_period: usize,
    pub stochastics_period: usize,
    pub bollinger_period: usize,
    pub bollinger_stddev_multiplier: f64,
    pub vwap_period: usize,
    pub ichimoku_tenkan_period: usize,
    pub ichimoku_kijun_period: usize,
    /// Width of the Fibonacci neutral band, as a **share of the 50% → 38.2%
    /// distance** — the room the ±1 bands have. A share rather than a price, so it
    /// means the same thing on every instrument, and capped at
    /// [`FIBONACCI_NEUTRAL_RATIO_MAX`] so it can never swallow those bands. It was
    /// a price difference (default 0.5) up to 2.9.8: measured on daily bars, that
    /// took 0.2% of the room on 9984.T and **more than all of it on F**, whose ±1
    /// bands were unreachable — `+1` required a close both above 15.080 and below
    /// 14.984.
    ///
    /// フィボナッチの中立帯の幅。**50% 水準から 38.2% 水準までの距離**——`±1` の帯に
    /// 使える幅——に対する割合で持つ。価格ではなく割合なのでどの銘柄でも同じ意味になり、
    /// [`FIBONACCI_NEUTRAL_RATIO_MAX`] で上限を切って `±1` を食い潰せないようにしている。
    /// 2.9.8 までは価格の絶対差（既定 0.5）だった。日足での実測では、9984.T ではその幅の
    /// 0.2% に過ぎず、**F では幅を超えていて `±1` が到達不能**だった（`+1` の条件が
    /// 「終値が 15.080 超かつ 14.984 未満」という空区間になっていた）。
    pub fibonacci_neutral_ratio: f64,
    pub stance: Stance,
    pub weight_basic: f64,
    pub weight_ema: f64,
    pub weight_sma: f64,
    pub weight_bollinger: f64,
    pub weight_roc: f64,
    pub weight_adx: f64,
    pub weight_stochastics: f64,
    pub weight_fibonacci: f64,
    pub weight_vwap: f64,
    pub weight_ichimoku: f64,
    pub llm_provider: String,
    pub openai_model: String,
    pub gemini_model: String,
    pub claude_model: String,
    pub llm_model: String,
    pub llm_timeout_secs: u64,
    pub llm_temperature: f64,
    pub llm_top_p: f64,
    pub llm_max_output_tokens: u32,
    pub claude_max_tokens: u32,
    pub ollama_host: String,
    pub ollama_port: u16,
    pub ollama_alias: String,
    pub ollama_timeout_secs: Option<u64>,
    pub ollama_temperature: Option<f64>,
    pub ollama_top_p: Option<f64>,
    pub ollama_top_k: Option<u32>,
    pub ollama_repeat_penalty: Option<f64>,
    pub ollama_num_ctx: u32,
    pub ollama_num_predict: i32,
    pub ollama_seed: i64,
    pub ollama_think: Option<String>,
    pub ollama_keep_alive: Option<String>,
    pub ollama_instances: Vec<OllamaInstance>,
    pub extra_note: Option<String>,
    pub no_news: bool,
    pub custom_news_query: Option<String>,
    pub news_filter: bool,
    pub news_count: usize,
    pub news_freshness: String,
    pub show_news: bool,
    pub save_technical_log: bool,
    pub log_format: String,
    pub log_dir: String,
    pub silent: bool,
    pub stdout_log: bool,
    pub max_note_length: usize,
    pub max_shortterm_length: usize,
    pub max_midterm_length: usize,
    pub max_news_length: usize,
    pub max_review_length: usize,
    pub ticker: String,
    pub alias_csv: Option<String>,
    pub no_alias: bool,
    pub no_llm: bool,
    /// Path to write the analysis report to (`--out`); format chosen by extension.
    pub out: Option<String>,
    pub debug_prompt: bool,
    pub debug_ollama: bool,
    pub ollama_bench_models: Vec<String>,
    pub ollama_bench_format: String,
    pub no_ollama_guard: bool,
    pub data_append: bool,
    pub log_flat: bool,
    pub debug_args: bool,
    pub fundamental: bool,
    pub sec_user_agent: Option<String>,
    pub https_proxy: Option<String>,
    pub no_proxy: Option<String>,
    pub chat: bool,
    pub chat_memory: String,
    pub chat_guard: String,
    pub debate: String,
    /// Interpretation depth of the analysis: shallow | mid | deep. How far the
    /// reply reads INTO the given data (not whether it uses outside knowledge).
    pub read_depth: String,
    /// Knowledge scope: narrow | mid | wide. How far the reply may reach OUTSIDE
    /// the input for general knowledge/reasoning (confirmed values stay input-only).
    pub knowledge_scope: String,
    pub response_shape: String,
    pub forecast_mode: String,
    pub autoreload: bool,
    pub autoreload_notify: bool,
    /// Chat-notification channels defined in `xoksa.env` (`NOTIFY_<n>_*`).
    pub notify_channels: Vec<NotifyChannel>,
    /// Alert rules defined in `xoksa.env` (`ALERT_<n>_*`), watched by the serve monitor.
    pub alert_rules: Vec<AlertRule>,
    pub chat_analysis_mode: AnalysisMode,
    pub lang: String,
}

impl Config {
    /// The indicators ACTIVE for analysis — the composite score, the display, and the
    /// LLM prompt use this. It is `active_extensions` when a runtime subset was set
    /// (`/set indicator off/on`), otherwise every `enabled_extensions` (the default).
    /// Computation and the log/CSV columns always use `enabled_extensions` directly, so
    /// narrowing this never changes what is computed or the stored column set.
    pub fn analysis_extensions(&self) -> &[ExtensionIndicator] {
        self.active_extensions
            .as_deref()
            .unwrap_or(&self.enabled_extensions)
    }

    /// The cloud LLM providers as `(id, key-env-name, effective-model)` rows — the
    /// single table shared by the chat `/llm` list and the Web `/api/llm` options
    /// (SOT §4.2). The model comes from config/default; the list never invents one.
    pub fn cloud_provider_rows(&self) -> [(&'static str, &'static str, &str); 3] {
        [
            ("openai", "OPENAI_API_KEY", self.openai_model.as_str()),
            ("gemini", "GEMINI_API_KEY", self.gemini_model.as_str()),
            ("claude", "CLAUDE_API_KEY", self.claude_model.as_str()),
        ]
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            analysis_mode: AnalysisMode::Daily,
            no_env_indicators: false,
            buy_rsi: 30.0,
            sell_rsi: 70.0,
            macd_diff_low: 2.0,
            macd_diff_mid: 10.0,
            macd_diff_extreme: 100.0,
            macd_minus_ok: false,
            enabled_extensions: Vec::new(),
            active_extensions: None,
            bb_bandwidth_squeeze_pct: 8.0,
            ema_short_period: 5,
            ema_long_period: 20,
            sma_short_period: 5,
            sma_long_period: 20,
            roc_period: 10,
            adx_period: 14,
            stochastics_period: 14,
            bollinger_period: 20,
            bollinger_stddev_multiplier: 2.0,
            vwap_period: 14,
            ichimoku_tenkan_period: 9,
            ichimoku_kijun_period: 26,
            fibonacci_neutral_ratio: 0.05,
            stance: Stance::Holder,
            weight_basic: 2.0,
            weight_ema: 1.0,
            weight_sma: 1.0,
            weight_bollinger: 1.0,
            weight_roc: 1.0,
            weight_adx: 1.0,
            weight_stochastics: 1.0,
            weight_fibonacci: 1.0,
            weight_vwap: 1.0,
            weight_ichimoku: 1.0,
            llm_provider: "openai".to_string(),
            openai_model: "gpt-5.6-terra".to_string(),
            gemini_model: "gemini-3.5-flash".to_string(),
            claude_model: "claude-sonnet-5".to_string(),
            llm_model: String::new(),
            llm_timeout_secs: 120,
            llm_temperature: 0.7,
            llm_top_p: 0.9,
            llm_max_output_tokens: 16384,
            claude_max_tokens: 16384,
            ollama_host: "127.0.0.1".to_string(),
            ollama_port: 11434,
            ollama_alias: String::new(),
            ollama_timeout_secs: None,
            ollama_temperature: None,
            ollama_top_p: None,
            ollama_top_k: None,
            ollama_repeat_penalty: None,
            ollama_num_ctx: 32_768,
            ollama_num_predict: 8_192,
            ollama_seed: 42,
            ollama_think: None,
            ollama_keep_alive: None,
            ollama_instances: Vec::new(),
            extra_note: None,
            no_news: false,
            custom_news_query: None,
            news_filter: false,
            news_count: 30,
            news_freshness: "pm".to_string(),
            show_news: false,
            save_technical_log: false,
            log_format: "json".to_string(),
            log_dir: "log".to_string(),
            silent: false,
            stdout_log: false,
            max_note_length: 400,
            max_shortterm_length: 200,
            max_midterm_length: 200,
            max_news_length: 1000,
            max_review_length: 2000,
            ticker: "SPY".to_string(),
            alias_csv: None,
            no_alias: false,
            no_llm: false,
            out: None,
            debug_prompt: false,
            debug_ollama: false,
            ollama_bench_models: Vec::new(),
            ollama_bench_format: "table".to_string(),
            no_ollama_guard: false,
            data_append: false,
            log_flat: false,
            debug_args: false,
            fundamental: false,
            sec_user_agent: None,
            https_proxy: None,
            no_proxy: None,
            chat: false,
            chat_memory: "mid".to_string(),
            chat_guard: "high".to_string(),
            debate: "summary".to_string(),
            read_depth: "mid".to_string(),
            knowledge_scope: "mid".to_string(),
            response_shape: "talk".to_string(),
            forecast_mode: "soft".to_string(),
            autoreload: false,
            autoreload_notify: false,
            notify_channels: Vec::new(),
            alert_rules: Vec::new(),
            chat_analysis_mode: AnalysisMode::Daily,
            lang: "en".to_string(),
        }
    }
}

/// Retrieves a value from env_map (xoksa.env) preferring it over the system environment variable
fn get_env_val(key: &str, env_map: &HashMap<String, String>) -> Option<String> {
    env_map.get(key).cloned().or_else(|| env::var(key).ok())
}

/// Resolves an enum-style env value against an allow-list, warning and falling
/// back to `default` on an unrecognized value or when the key is unset.
fn get_choice_env(
    key: &str,
    env_map: &HashMap<String, String>,
    valid: &[&str],
    default: &str,
) -> String {
    match get_env_val(key, env_map) {
        Some(v) => {
            let trimmed = v.trim();
            if valid.contains(&trimmed) {
                trimmed.to_string()
            } else {
                eprintln!("⚠️ {key} value not supported: {trimmed}. Using {default}.");
                default.to_string()
            }
        }
        None => default.to_string(),
    }
}

fn get_bool_env(key: &str, env_map: &HashMap<String, String>) -> bool {
    get_env_val(key, env_map)
        .map(|v| v.trim().to_lowercase() == "true")
        .unwrap_or(false)
}

pub fn scan_ollama_instances(env_map: &HashMap<String, String>) -> Vec<OllamaInstance> {
    let mut instances = Vec::new();
    for n in 1u8..=16 {
        let alias = match get_env_val(&format!("OLLAMA_{}_ALIAS", n), env_map) {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => continue,
        };
        let host = get_env_val(&format!("OLLAMA_{}_HOST", n), env_map)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let port = get_env_val(&format!("OLLAMA_{}_PORT", n), env_map)
            .and_then(|v| v.trim().parse::<u16>().ok())
            .unwrap_or(11434);
        let model = get_env_val(&format!("OLLAMA_{}_MODEL", n), env_map)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "llama3".to_string());
        instances.push(OllamaInstance {
            alias,
            model,
            host,
            port,
        });
    }
    instances
}

/// Parse a single-condition alert expression (`rsi<=30`, `score >= 4`) into a
/// backtest `Condition` whose right-hand side is a constant. Only the four value
/// comparisons are accepted (`<=`, `>=`, `<`, `>`) — the exact ops the backtest
/// evaluator applies to a constant RHS, so no rule can be armed that never fires.
/// The left side must be a known indicator key (`rule_indicator_catalog`);
/// anything else (typo, unknown op, non-numeric RHS) returns `None`.
pub(crate) fn parse_alert_condition(s: &str) -> Option<crate::backtest::Condition> {
    let s = s.trim();
    // Two-char operators MUST be tested before their one-char prefixes.
    let (token, idx) = ["<=", ">=", "<", ">"]
        .iter()
        .find_map(|t| s.find(t).map(|i| (*t, i)))?;
    let op = match token {
        "<=" => "le",
        ">=" => "ge",
        "<" => "lt",
        ">" => "gt",
        _ => return None,
    };
    let left = s[..idx].trim().to_ascii_lowercase();
    let value: f64 = s[idx + token.len()..].trim().parse().ok()?;
    let known = crate::backtest::rule_indicator_catalog()
        .iter()
        .any(|(k, _, _)| *k == left);
    if !known {
        return None;
    }
    Some(crate::backtest::Condition {
        left,
        op: op.to_string(),
        right_kind: "value".to_string(),
        value: Some(value),
        right: None,
    })
}

/// Scan `NOTIFY_<n>_*` channels (n = 1..=16). A channel needs a valid `KIND`
/// (`slack|discord|gchat|line`); rows with a missing or unknown kind are skipped.
/// The secret (`NOTIFY_<n>_SECRET`) is NOT read here — it is Class A and stays in
/// the keychain until dispatch.
pub fn scan_notify_channels(env_map: &HashMap<String, String>) -> Vec<NotifyChannel> {
    let mut channels = Vec::new();
    for n in 1u8..=16 {
        let kind = match get_env_val(&format!("NOTIFY_{}_KIND", n), env_map)
            .and_then(|v| crate::notify::NotifierKind::parse(&v))
        {
            Some(k) => k,
            None => continue,
        };
        let name = get_env_val(&format!("NOTIFY_{}_NAME", n), env_map)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| format!("notify{}", n));
        let to = get_env_val(&format!("NOTIFY_{}_TO", n), env_map)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        channels.push(NotifyChannel { n, kind, name, to });
    }
    channels
}

/// How many alert rules may exist at once. The `<n>` suffix of `ALERT_<n>_*`
/// runs `1..=ALERT_MAX`; this is the single definition, so the scanner and the
/// runtime `add` path cannot drift apart.
pub const ALERT_MAX: u8 = 16;

/// Upper bound on [`Config::fibonacci_neutral_ratio`]. The neutral band is that
/// share of the room the ±1 bands have, so a cap below 1.0 guarantees each of them
/// keeps at least half its room — the ±1 bands can never become unreachable, which
/// is what an uncapped price difference did to F. The single definition, applied
/// wherever the value is accepted (CLI / env / chat `/set`).
///
/// [`Config::fibonacci_neutral_ratio`] の上限。中立帯は `±1` の帯が持つ幅に対する割合
/// なので、1.0 未満で切れば `±1` は必ず半分以上の幅を保つ——上限の無い価格差が F で
/// 起こした「`±1` が到達不能」が構造的に起こらない。定義はここ 1 箇所で、値を受け取る
/// すべての経路（CLI・環境変数・チャットの `/set`）に適用する。
pub const FIBONACCI_NEUTRAL_RATIO_MAX: f64 = 0.5;

/// Ceiling on any LLM request timeout, in seconds.
///
/// A permit from the server's admission semaphore is held for as long as its
/// handler runs, and there is no server-side per-handler timeout — so the
/// outbound client timeout is what bounds the hold. Leaving that unbounded left
/// a configured value able to pin a permit indefinitely, which matters now that
/// the pool is finite rather than `u16::MAX`. Fifteen minutes is far past any
/// answer a person waits for, and above the 600 the existing tests treat as a
/// legitimate setting. As with [`FIBONACCI_NEUTRAL_RATIO_MAX`], the value is
/// defined once here and applied wherever a timeout is accepted.
///
/// LLM のタイムアウトの上限（秒）。サーバの受入セマフォの許可証はハンドラが走る間ずっと
/// 保持され、ハンドラ単位のサーバ側タイムアウトは無いため、保持時間を決めるのは外向き
/// クライアントのタイムアウトである。上限が無いと、設定値ひとつで許可証を事実上無期限に
/// 占有できた——枠が `u16::MAX` ではなく有限になった以上これは放置できない。15 分は人が
/// 待つ時間をはるかに超え、既存の試験が正当な設定として扱う 600 も上回る。
/// [`FIBONACCI_NEUTRAL_RATIO_MAX`] と同じく、定義はここ 1 箇所で、タイムアウトを受け取る
/// すべての経路に適用する。
pub const LLM_TIMEOUT_SECS_MAX: u64 = 900;

/// Accept an LLM timeout from any surface, bounded.
pub(crate) fn clamp_llm_timeout_secs(value: u64) -> u64 {
    value.min(LLM_TIMEOUT_SECS_MAX)
}

/// Accept a neutral-band ratio from any surface, bounded. A non-finite value falls
/// back to no band at all rather than poisoning the comparison.
pub(crate) fn clamp_fibonacci_neutral_ratio(value: f64) -> f64 {
    match value.is_finite() {
        true => value.clamp(0.0, FIBONACCI_NEUTRAL_RATIO_MAX),
        false => 0.0,
    }
}

/// Rule numbers above [`ALERT_MAX`] that something tried to define, sorted.
///
/// Pure, so the detection is testable on its own. A test that only checks such a
/// row is unmonitored passes just as well with the warning deleted — and the
/// warning is the point: the row is ignored either way, and silence reads as
/// "applied".
///
/// Both inputs the resolver accepts are scanned. `get_env_val` falls back to the
/// process environment, so `ALERT_17_TICKER` given only as an environment
/// variable is as much a definition as one written in the file; reporting one but
/// not the other would be arbitrary.
fn alert_rule_numbers_over_limit<'a>(
    file_keys: impl Iterator<Item = &'a str>,
    env_keys: impl Iterator<Item = String>,
) -> Vec<u32> {
    let mut over: Vec<u32> = file_keys
        .map(str::to_string)
        .chain(env_keys)
        .filter_map(|k| {
            let rest = k.strip_prefix("ALERT_")?.strip_suffix("_TICKER")?;
            rest.parse::<u32>()
                .ok()
                .filter(|n| *n > u32::from(ALERT_MAX))
        })
        .collect();
    over.sort_unstable();
    over.dedup();
    over
}

/// Report `ALERT_<n>_TICKER` definitions numbered above [`ALERT_MAX`]. Hand-editing
/// `xoksa.env`, and setting the variable in the environment, are both supported
/// ways to define a rule, so a definition the scanner will never reach has to say
/// so.
fn warn_alert_rules_over_limit(env_map: &HashMap<String, String>) {
    // `vars()` converts the VALUES too and panics on one that is not Unicode, so
    // an unrelated variable holding such a value would take the whole startup
    // down. Only the names matter here, and only the ones that convert.
    let over = alert_rule_numbers_over_limit(
        env_map.keys().map(String::as_str),
        env::vars_os().filter_map(|(k, _)| k.into_string().ok()),
    );
    if over.is_empty() {
        return;
    }
    let list = over
        .iter()
        .map(|n| format!("ALERT_{n}_*"))
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!(
        "⚠️ Alert rules are limited to {ALERT_MAX}. Ignored (not monitored): {list}. \
         Free a slot by removing a rule numbered 1-{ALERT_MAX}."
    );
}

/// Scan `ALERT_<n>_*` rules (n = 1..=[`ALERT_MAX`]). A rule needs a `TICKER` and
/// a parseable single-condition `WHEN`; rows missing either are skipped.
/// `NOTIFY` is the channel number to fire (default 1); `EXPLAIN` toggles the
/// optional LLM note. Rows numbered above the limit are reported rather than
/// dropped in silence — a hand-edited `ALERT_17_*` would otherwise look applied.
pub fn scan_alert_rules(env_map: &HashMap<String, String>) -> Vec<AlertRule> {
    warn_alert_rules_over_limit(env_map);
    let mut rules = Vec::new();
    for n in 1u8..=ALERT_MAX {
        let ticker = match get_env_val(&format!("ALERT_{}_TICKER", n), env_map) {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => continue,
        };
        let when = match get_env_val(&format!("ALERT_{}_WHEN", n), env_map)
            .and_then(|v| parse_alert_condition(&v))
        {
            Some(c) => c,
            None => continue,
        };
        let mode = get_env_val(&format!("ALERT_{}_MODE", n), env_map)
            .and_then(|v| AnalysisMode::from_value(v.trim()));
        let notify = get_env_val(&format!("ALERT_{}_NOTIFY", n), env_map)
            .and_then(|v| v.trim().parse::<u8>().ok())
            .unwrap_or(1);
        let explain = get_bool_env(&format!("ALERT_{}_EXPLAIN", n), env_map);
        rules.push(AlertRule {
            n,
            ticker,
            mode,
            when,
            notify,
            explain,
        });
    }
    rules
}

#[cfg(test)]
mod alert_limit_tests {
    use super::{scan_alert_rules, ALERT_MAX};
    use std::collections::HashMap;

    fn rule(n: u32) -> [(String, String); 3] {
        [
            (format!("ALERT_{n}_TICKER"), "9432.T".to_string()),
            (format!("ALERT_{n}_WHEN"), "rsi<=30".to_string()),
            (format!("ALERT_{n}_MODE"), "5m".to_string()),
        ]
    }

    // The limit lives in one place. A scanner that walked a different range than
    // the `add` path would accept a rule the monitor never reads.
    #[test]
    fn scanner_reads_up_to_the_limit_and_no_further() {
        let mut env = HashMap::new();
        for n in [1u32, u32::from(ALERT_MAX), u32::from(ALERT_MAX) + 1, 99] {
            env.extend(rule(n));
        }
        let got = scan_alert_rules(&env);
        let mut numbers: Vec<u8> = got.iter().map(|r| r.n).collect();
        numbers.sort_unstable();
        assert_eq!(
            numbers,
            vec![1, ALERT_MAX],
            "only 1..=ALERT_MAX may be monitored"
        );
    }

    // A row above the limit is ignored, so it has to be REPORTED; silence reads as
    // "applied" to someone who hand-edited xoksa.env. Asserting only that the row
    // is unmonitored would pass with the warning deleted, so what this pins is the
    // detection itself — including that a definition given only as an environment
    // variable counts, since `get_env_val` reads those too.
    #[test]
    fn rows_over_the_limit_are_detected_for_reporting() {
        use super::alert_rule_numbers_over_limit;
        let file_keys = [
            "ALERT_1_TICKER",
            "ALERT_16_TICKER",
            "ALERT_17_TICKER",
            "ALERT_99_TICKER",
            "ALERT_17_WHEN",
            "NOTIFY_17_KIND",
            "ALERT_X_TICKER",
        ];
        assert_eq!(
            alert_rule_numbers_over_limit(
                file_keys.into_iter(),
                ["ALERT_20_TICKER".to_string(), "ALERT_2_TICKER".to_string()].into_iter(),
            ),
            vec![17, 20, 99],
            "only ALERT_<n>_TICKER above the limit, from either input, deduped and sorted"
        );

        let mut env = HashMap::new();
        env.extend(rule(u32::from(ALERT_MAX) + 1));
        assert!(scan_alert_rules(&env).is_empty(), "and it is not monitored");
    }

    // Scanning the environment must not be able to stop the program starting.
    // `env::vars()` converts the VALUES too and panics on one that is not Unicode,
    // so a single unrelated variable — anyone's, from anywhere in the environment —
    // would take startup down with it. Only the names are wanted here.
    #[cfg(windows)]
    #[test]
    fn an_unrelated_non_unicode_variable_does_not_stop_the_scan() {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        // Mutating the process environment is global; keep these tests off each
        // other's toes and put it back afterwards.
        static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _g = GATE.lock().unwrap_or_else(|e| e.into_inner());

        // A lone high surrogate: valid UTF-16, not valid Unicode.
        let bad = OsString::from_wide(&[0xD800]);
        std::env::set_var("XOKSA_TEST_NON_UNICODE", &bad);
        std::env::set_var("ALERT_17_TICKER", "PROBE");

        let mut map = HashMap::new();
        map.extend(rule(1));
        let got = scan_alert_rules(&map);

        std::env::remove_var("XOKSA_TEST_NON_UNICODE");
        std::env::remove_var("ALERT_17_TICKER");

        assert_eq!(got.len(), 1, "the scan completes and reads rule 1");
        assert!(
            !super::alert_rule_numbers_over_limit(
                std::iter::empty(),
                std::env::vars_os().filter_map(|(k, _)| k.into_string().ok()),
            )
            .contains(&17),
            "the probe variable is cleaned up"
        );
    }
}

fn normalize_ollama_think(value: &str) -> Option<String> {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "" => None,
        "false" | "true" | "low" | "medium" | "high" => Some(normalized),
        _ => {
            eprintln!(
                "⚠️ OLLAMA_THINK value not supported: {}. Treating as unset.",
                value
            );
            None
        }
    }
}

fn normalize_ollama_benchmark_models(values: &[String]) -> Vec<String> {
    values
        .iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn normalize_ollama_benchmark_format(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "table" | "csv" | "json" => Some(normalized),
        "" => None,
        other => {
            eprintln!(
                "⚠️ OLLAMA_BENCHMARK_FORMAT value not supported: {}. Using table.",
                other
            );
            None
        }
    }
}

/// Resolves an f64 value from CLI arg, env var, or default, and sanitizes it to 0.5–3.0
fn get_f64_from_args_or_env(
    arg_val: f64,
    env_key: &str,
    default: f64,
    env_map: &HashMap<String, String>,
) -> f64 {
    let value = if (arg_val - default).abs() > f64::EPSILON {
        arg_val
    } else {
        match get_env_val(env_key, env_map) {
            Some(env_val) => {
                match env_val.parse::<f64>() {
                    Ok(parsed) => parsed,
                    Err(_) => {
                        eprintln!("⚠️ Invalid value for env var {} (f64 parse failed). Using default ({}).", env_key, default);
                        default
                    }
                }
            }
            None => default,
        }
    };

    if !value.is_finite() {
        eprintln!(
            "⚠️ Invalid weight value (NaN/inf detected): {}. Using default ({}).",
            value, default
        );
        return default;
    }

    if value < 0.0 {
        eprintln!(
            "⚠️ Invalid weight value (negative): {}. Using default ({}).",
            value, default
        );
        return default;
    }

    if !(0.5..=3.0).contains(&value) {
        eprintln!(
            "⚠️ Invalid weight value (out of range): {}. Using default ({}).",
            value, default
        );
        return default;
    }

    value
}

/// Clamps a percentage value to the range 0.0–100.0
fn sanitize_percent(value: f64, min: f64, max: f64, label: &str) -> f64 {
    if !value.is_finite() {
        eprintln!(
            "⚠️ Invalid {} (NaN/inf): {} -> clamped to {}",
            label, value, min
        );
        return min;
    }
    let clamped_value = value.clamp(min, max);
    if (clamped_value - value).abs() > f64::EPSILON {
        eprintln!(
            "⚠️ {} out of range: {} -> clamped to {}..={} (used {})",
            label, value, min, max, clamped_value
        );
    }
    clamped_value
}

/// Returns the CLI arg if it differs from the default; otherwise reads from the env var
fn get_usize_from_args_or_env(
    arg_val: usize,
    env_key: &str,
    default: usize,
    env_map: &HashMap<String, String>,
) -> usize {
    if arg_val != default {
        arg_val
    } else {
        get_usize_env(env_key, default, env_map)
    }
}

/// Reads a numeric env value, WARNING (not silently ignoring) on a malformed value
/// so a typo like `OLLAMA_NUM_PREDICT=8192x` is visible instead of silently reverting
/// to the default (errors-are-actionable). Returns `None` when unset; `None` + a
/// warning when set but unparseable. Behaviour (final value) is unchanged vs a bare
/// `.parse().ok()` — only the warning is added.
fn parse_num_env<T: std::str::FromStr>(key: &str, env_map: &HashMap<String, String>) -> Option<T> {
    let raw = get_env_val(key, env_map)?;
    match raw.parse::<T>() {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("⚠️ {key} value is not a valid number: {raw}. Using default.");
            None
        }
    }
}

/// Reads a usize value from the env map, warning on a malformed value and returning
/// the default (see `parse_num_env`).
fn get_usize_env(key: &str, default: usize, env_map: &HashMap<String, String>) -> usize {
    parse_num_env(key, env_map).unwrap_or(default)
}

fn parse_stance(stance: &str) -> Stance {
    match stance {
        "buyer" => Stance::Buyer,
        "seller" => Stance::Seller,
        _ => Stance::Holder,
    }
}

/// Reads from env when CLI arg matches the default (used for thresholds like RSI / MACD diff)
fn resolve_threshold_f64(
    arg_val: f64,
    env_key: &str,
    default: f64,
    env_map: &HashMap<String, String>,
    no_env_indicators: bool,
) -> f64 {
    if (arg_val - default).abs() > f64::EPSILON {
        arg_val
    } else if no_env_indicators {
        default
    } else {
        parse_num_env(env_key, env_map).unwrap_or(default)
    }
}

fn parse_env_analysis_mode(value: &str) -> Option<AnalysisMode> {
    let parsed = AnalysisMode::from_value(value);
    if parsed.is_none() {
        eprintln!(
            "⚠️ ANALYSIS_MODE value not supported: {}. Using daily.",
            value
        );
    }
    parsed
}

fn resolve_analysis_mode(
    args: &Args,
    env_map: &HashMap<String, String>,
    cli_sources: CliValueSources,
) -> AnalysisMode {
    if cli_sources.analysis_mode || args.analysis_mode != "daily" {
        return AnalysisMode::from_value(&args.analysis_mode).unwrap_or(AnalysisMode::Daily);
    }

    get_env_val("ANALYSIS_MODE", env_map)
        .and_then(|v| parse_env_analysis_mode(&v))
        .unwrap_or(AnalysisMode::Daily)
}

fn resolve_chat_analysis_mode(
    args: &Args,
    env_map: &HashMap<String, String>,
    cli_sources: CliValueSources,
    fallback: AnalysisMode,
) -> AnalysisMode {
    if cli_sources.chat_mode {
        return AnalysisMode::from_value(&args.chat_mode).unwrap_or(fallback);
    }

    get_env_val("CHAT_ANALYSIS_MODE", env_map)
        .and_then(|v| {
            let parsed = AnalysisMode::from_value(&v);
            if parsed.is_none() {
                eprintln!(
                    "⚠️ CHAT_ANALYSIS_MODE value not supported: {}. Falling back to ANALYSIS_MODE.",
                    v
                );
            }
            parsed
        })
        .unwrap_or(fallback)
}

pub fn build_config(args: &Args, env_map: &HashMap<String, String>) -> Config {
    build_config_with_value_sources(args, env_map, CliValueSources::default())
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CliValueSources {
    pub analysis_mode: bool,
    pub log_format: bool,
    pub llm_provider: bool,
    pub ollama_bench_format: bool,
    pub chat_memory: bool,
    pub chat_guard: bool,
    pub debate: bool,
    pub chat_mode: bool,
    pub lang: bool,
}

impl CliValueSources {
    pub fn from_arg_matches(matches: &ArgMatches) -> Self {
        Self {
            analysis_mode: matches.value_source("analysis_mode") == Some(ValueSource::CommandLine),
            log_format: matches.value_source("log_format") == Some(ValueSource::CommandLine),
            llm_provider: matches.value_source("llm_provider") == Some(ValueSource::CommandLine),
            ollama_bench_format: matches.value_source("ollama_bench_format")
                == Some(ValueSource::CommandLine),
            chat_memory: matches.value_source("chat_memory") == Some(ValueSource::CommandLine),
            chat_guard: matches.value_source("chat_guard") == Some(ValueSource::CommandLine),
            debate: matches.value_source("debate") == Some(ValueSource::CommandLine),
            chat_mode: matches.value_source("chat_mode") == Some(ValueSource::CommandLine),
            lang: matches.value_source("lang") == Some(ValueSource::CommandLine),
        }
    }
}

pub fn build_config_from_arg_matches(
    matches: &ArgMatches,
    env_map: &HashMap<String, String>,
) -> Result<Config, clap::Error> {
    let args = Args::from_arg_matches(matches)?;
    Ok(build_config_with_value_sources(
        &args,
        env_map,
        CliValueSources::from_arg_matches(matches),
    ))
}

pub fn build_config_with_value_sources(
    args: &Args,
    env_map: &HashMap<String, String>,
    cli_sources: CliValueSources,
) -> Config {
    // Single source of every hardcoded default: `Config::default()`. Each field
    // below overlays CLI > env on top of `d.<field>`, so a default lives in exactly
    // one place (SOT §4.2) and is always overwritable by env/CLI (design §3).
    let d = Config::default();
    let resolved_provider: String = if cli_sources.llm_provider {
        args.llm_provider.clone()
    } else {
        match get_env_val("llm_provider", env_map).filter(|v| !v.trim().is_empty()) {
            Some(v) => match v.trim() {
                "openai" | "gemini" | "claude" | "ollama" => v.trim().to_string(),
                other => {
                    eprintln!(
                        "⚠️ llm_provider value not supported: {}. Using openai.",
                        other
                    );
                    d.llm_provider.clone()
                }
            },
            None => args.llm_provider.clone(),
        }
    };

    let resolved_ollama_instances: Vec<OllamaInstance> = scan_ollama_instances(env_map);

    let resolved_llm_model: String = if let Some(m) = &args.llm_model {
        m.clone()
    } else if let Some(m) = get_env_val("llm_model", env_map).filter(|v| !v.trim().is_empty()) {
        m
    } else {
        match resolved_provider.as_str() {
            "gemini" => {
                get_env_val("gemini_model", env_map).unwrap_or_else(|| d.gemini_model.clone())
            }
            "claude" => {
                get_env_val("claude_model", env_map).unwrap_or_else(|| d.claude_model.clone())
            }
            "ollama" => resolved_ollama_instances
                .first()
                .map(|i| i.model.clone())
                .unwrap_or_default(),
            _ => {
                if args.openai_model != d.openai_model {
                    args.openai_model.clone()
                } else {
                    get_env_val("openai_model", env_map).unwrap_or_else(|| d.openai_model.clone())
                }
            }
        }
    };

    let analysis_mode = resolve_analysis_mode(args, env_map, cli_sources);
    Config {
        analysis_mode,
        chat_analysis_mode: resolve_chat_analysis_mode(args, env_map, cli_sources, analysis_mode),
        debug_args: args.debug_args,
        out: args.out.clone(),
        no_env_indicators: args.no_env_indicators,
        buy_rsi: resolve_threshold_f64(
            args.buy_rsi,
            "BUY_RSI",
            d.buy_rsi,
            env_map,
            args.no_env_indicators,
        ),
        sell_rsi: resolve_threshold_f64(
            args.sell_rsi,
            "SELL_RSI",
            d.sell_rsi,
            env_map,
            args.no_env_indicators,
        ),
        macd_diff_low: resolve_threshold_f64(
            args.macd_diff_low,
            "MACD_DIFF_LOW",
            d.macd_diff_low,
            env_map,
            args.no_env_indicators,
        ),
        macd_diff_mid: resolve_threshold_f64(
            args.macd_diff_mid,
            "MACD_DIFF_MID",
            d.macd_diff_mid,
            env_map,
            args.no_env_indicators,
        ),
        macd_diff_extreme: resolve_threshold_f64(
            args.macd_diff_extreme,
            "MACD_DIFF_EXTREME",
            d.macd_diff_extreme,
            env_map,
            args.no_env_indicators,
        ),
        macd_minus_ok: if args.no_env_indicators {
            args.macd_minus_ok
        } else {
            args.macd_minus_ok || get_bool_env("MACD_MINUS_OK", env_map)
        },
        stance: {
            let stance_source = match (args.no_env_indicators, args.stance.as_str()) {
                (true, _) => args.stance.clone(),
                (false, "holder") => {
                    get_env_val("STANCE", env_map).unwrap_or_else(|| "holder".to_string())
                }
                (false, _) => args.stance.clone(),
            };
            parse_stance(&stance_source)
        },
        weight_basic: if args.no_env_indicators {
            args.weight_basic
        } else {
            get_f64_from_args_or_env(args.weight_basic, "WEIGHT_BASIC", d.weight_basic, env_map)
        },
        weight_ema: if args.no_env_indicators {
            args.weight_ema
        } else {
            get_f64_from_args_or_env(args.weight_ema, "WEIGHT_EMA", d.weight_ema, env_map)
        },
        weight_sma: if args.no_env_indicators {
            args.weight_sma
        } else {
            get_f64_from_args_or_env(args.weight_sma, "WEIGHT_SMA", d.weight_sma, env_map)
        },
        weight_bollinger: if args.no_env_indicators {
            args.weight_bollinger
        } else {
            get_f64_from_args_or_env(
                args.weight_bollinger,
                "WEIGHT_BOLLINGER",
                d.weight_bollinger,
                env_map,
            )
        },
        weight_roc: if args.no_env_indicators {
            args.weight_roc
        } else {
            get_f64_from_args_or_env(args.weight_roc, "WEIGHT_ROC", d.weight_roc, env_map)
        },
        weight_adx: if args.no_env_indicators {
            args.weight_adx
        } else {
            get_f64_from_args_or_env(args.weight_adx, "WEIGHT_ADX", d.weight_adx, env_map)
        },
        weight_stochastics: if args.no_env_indicators {
            args.weight_stochastics
        } else {
            get_f64_from_args_or_env(
                args.weight_stochastics,
                "WEIGHT_STOCHASTICS",
                d.weight_stochastics,
                env_map,
            )
        },
        weight_fibonacci: if args.no_env_indicators {
            args.weight_fibonacci
        } else {
            get_f64_from_args_or_env(
                args.weight_fibonacci,
                "WEIGHT_FIBONACCI",
                d.weight_fibonacci,
                env_map,
            )
        },
        weight_vwap: if args.no_env_indicators {
            args.weight_vwap
        } else {
            get_f64_from_args_or_env(args.weight_vwap, "WEIGHT_VWAP", d.weight_vwap, env_map)
        },
        weight_ichimoku: if args.no_env_indicators {
            args.weight_ichimoku
        } else {
            get_f64_from_args_or_env(
                args.weight_ichimoku,
                "WEIGHT_ICHIMOKU",
                d.weight_ichimoku,
                env_map,
            )
        },
        enabled_extensions: {
            let mut extensions = Vec::new();
            if args.ema || (!args.no_env_indicators && get_bool_env("EMA", env_map)) {
                extensions.push(ExtensionIndicator::Ema);
            }
            if args.sma || (!args.no_env_indicators && get_bool_env("SMA", env_map)) {
                extensions.push(ExtensionIndicator::Sma);
            }
            if args.roc || (!args.no_env_indicators && get_bool_env("ROC", env_map)) {
                extensions.push(ExtensionIndicator::Roc);
            }
            if args.adx || (!args.no_env_indicators && get_bool_env("ADX", env_map)) {
                extensions.push(ExtensionIndicator::Adx);
            }
            if args.stochastics || (!args.no_env_indicators && get_bool_env("STOCHASTICS", env_map))
            {
                extensions.push(ExtensionIndicator::Stochastics);
            }
            if args.bollinger || (!args.no_env_indicators && get_bool_env("BOLLINGER", env_map)) {
                extensions.push(ExtensionIndicator::Bollinger);
            }
            if args.fibonacci || (!args.no_env_indicators && get_bool_env("FIBONACCI", env_map)) {
                extensions.push(ExtensionIndicator::Fibonacci);
            }
            if args.vwap || (!args.no_env_indicators && get_bool_env("VWAP", env_map)) {
                extensions.push(ExtensionIndicator::Vwap);
            }
            if args.ichimoku || (!args.no_env_indicators && get_bool_env("ICHIMOKU", env_map)) {
                extensions.push(ExtensionIndicator::Ichimoku);
            }
            extensions
        },
        // None = "all enabled"; `/set indicator off/on` sets an explicit subset.
        active_extensions: None,
        bb_bandwidth_squeeze_pct: sanitize_percent(
            if args.bb_bandwidth_squeeze_pct == d.bb_bandwidth_squeeze_pct {
                if args.no_env_indicators {
                    d.bb_bandwidth_squeeze_pct
                } else {
                    get_env_val("BB_BANDWIDTH_SQUEEZE_PCT", env_map)
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(d.bb_bandwidth_squeeze_pct)
                }
            } else {
                args.bb_bandwidth_squeeze_pct
            },
            0.0,
            100.0,
            "Bollinger bandwidth squeeze threshold (%)",
        ),
        ema_short_period: if args.no_env_indicators {
            args.ema_short_period.max(1)
        } else {
            get_usize_from_args_or_env(
                args.ema_short_period,
                "EMA_SHORT_PERIOD",
                d.ema_short_period,
                env_map,
            )
            .max(1)
        },
        ema_long_period: if args.no_env_indicators {
            args.ema_long_period.max(2)
        } else {
            get_usize_from_args_or_env(
                args.ema_long_period,
                "EMA_LONG_PERIOD",
                d.ema_long_period,
                env_map,
            )
            .max(2)
        },
        sma_short_period: if args.no_env_indicators {
            args.sma_short_period.max(1)
        } else {
            get_usize_from_args_or_env(
                args.sma_short_period,
                "SMA_SHORT_PERIOD",
                d.sma_short_period,
                env_map,
            )
            .max(1)
        },
        sma_long_period: if args.no_env_indicators {
            args.sma_long_period.max(2)
        } else {
            get_usize_from_args_or_env(
                args.sma_long_period,
                "SMA_LONG_PERIOD",
                d.sma_long_period,
                env_map,
            )
            .max(2)
        },
        roc_period: if args.no_env_indicators {
            args.roc_period.max(1)
        } else {
            get_usize_from_args_or_env(args.roc_period, "ROC_PERIOD", d.roc_period, env_map).max(1)
        },
        adx_period: if args.no_env_indicators {
            args.adx_period.max(2)
        } else {
            get_usize_from_args_or_env(args.adx_period, "ADX_PERIOD", d.adx_period, env_map).max(2)
        },
        stochastics_period: if args.no_env_indicators {
            args.stochastics_period.max(2)
        } else {
            get_usize_from_args_or_env(
                args.stochastics_period,
                "STOCHASTICS_PERIOD",
                d.stochastics_period,
                env_map,
            )
            .max(2)
        },
        bollinger_period: if args.no_env_indicators {
            args.bollinger_period.max(2)
        } else {
            get_usize_from_args_or_env(
                args.bollinger_period,
                "BOLLINGER_PERIOD",
                d.bollinger_period,
                env_map,
            )
            .max(2)
        },
        bollinger_stddev_multiplier: match args.no_env_indicators {
            true => args.bollinger_stddev_multiplier.max(0.1),
            false => match (args.bollinger_stddev_multiplier - d.bollinger_stddev_multiplier).abs()
                > f64::EPSILON
            {
                true => args.bollinger_stddev_multiplier.max(0.1),
                false => get_env_val("BOLLINGER_STDDEV_MULTIPLIER", env_map)
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .unwrap_or(d.bollinger_stddev_multiplier),
            },
        },
        vwap_period: if args.no_env_indicators {
            args.vwap_period.max(1)
        } else {
            get_usize_from_args_or_env(args.vwap_period, "VWAP_PERIOD", d.vwap_period, env_map)
                .max(1)
        },
        ichimoku_tenkan_period: if args.no_env_indicators {
            args.ichimoku_tenkan_period.max(1)
        } else {
            get_usize_from_args_or_env(
                args.ichimoku_tenkan_period,
                "ICHIMOKU_TENKAN_PERIOD",
                d.ichimoku_tenkan_period,
                env_map,
            )
            .max(1)
        },
        ichimoku_kijun_period: if args.no_env_indicators {
            args.ichimoku_kijun_period.max(2)
        } else {
            get_usize_from_args_or_env(
                args.ichimoku_kijun_period,
                "ICHIMOKU_KIJUN_PERIOD",
                d.ichimoku_kijun_period,
                env_map,
            )
            .max(2)
        },
        fibonacci_neutral_ratio: match args.no_env_indicators {
            true => clamp_fibonacci_neutral_ratio(args.fibonacci_neutral_ratio),
            false => {
                match (args.fibonacci_neutral_ratio - d.fibonacci_neutral_ratio).abs()
                    > f64::EPSILON
                {
                    true => clamp_fibonacci_neutral_ratio(args.fibonacci_neutral_ratio),
                    false => get_env_val("FIBONACCI_NEUTRAL_RATIO", env_map)
                        .and_then(|v| v.parse::<f64>().ok())
                        .filter(|v| v.is_finite() && *v >= 0.0)
                        .map(clamp_fibonacci_neutral_ratio)
                        .unwrap_or(d.fibonacci_neutral_ratio),
                }
            }
        },
        llm_provider: resolved_provider.clone(),
        openai_model: if args.openai_model == d.openai_model {
            get_env_val("openai_model", env_map).unwrap_or_else(|| d.openai_model.clone())
        } else {
            args.openai_model.clone()
        },
        gemini_model: get_env_val("gemini_model", env_map)
            .unwrap_or_else(|| d.gemini_model.clone()),
        claude_model: get_env_val("claude_model", env_map)
            .unwrap_or_else(|| d.claude_model.clone()),
        llm_model: resolved_llm_model,
        llm_timeout_secs: clamp_llm_timeout_secs(
            parse_num_env("llm_timeout_seconds", env_map).unwrap_or(d.llm_timeout_secs),
        ),
        llm_temperature: parse_num_env("llm_temperature", env_map).unwrap_or(d.llm_temperature),
        llm_top_p: parse_num_env("llm_top_p", env_map).unwrap_or(d.llm_top_p),
        llm_max_output_tokens: parse_num_env("llm_max_output_tokens", env_map)
            .unwrap_or(d.llm_max_output_tokens),
        claude_max_tokens: parse_num_env("claude_max_tokens", env_map)
            .or_else(|| parse_num_env("llm_max_output_tokens", env_map))
            .unwrap_or(d.claude_max_tokens),
        ollama_host: resolved_ollama_instances
            .first()
            .map(|i| i.host.clone())
            .unwrap_or_else(|| d.ollama_host.clone()),
        ollama_port: resolved_ollama_instances
            .first()
            .map(|i| i.port)
            .unwrap_or(d.ollama_port),
        ollama_alias: if resolved_provider == "ollama" {
            resolved_ollama_instances
                .first()
                .map(|i| i.alias.clone())
                .unwrap_or_default()
        } else {
            String::new()
        },
        ollama_timeout_secs: parse_num_env("OLLAMA_TIMEOUT_SECONDS", env_map)
            .map(clamp_llm_timeout_secs),
        ollama_temperature: parse_num_env::<f64>("OLLAMA_TEMPERATURE", env_map)
            .filter(|v| v.is_finite()),
        ollama_top_p: parse_num_env::<f64>("OLLAMA_TOP_P", env_map).filter(|v| v.is_finite()),
        ollama_top_k: parse_num_env("OLLAMA_TOP_K", env_map),
        ollama_repeat_penalty: parse_num_env::<f64>("OLLAMA_REPEAT_PENALTY", env_map)
            .filter(|v| v.is_finite()),
        ollama_num_ctx: parse_num_env("OLLAMA_NUM_CTX", env_map).unwrap_or(d.ollama_num_ctx),
        ollama_num_predict: parse_num_env("OLLAMA_NUM_PREDICT", env_map)
            .unwrap_or(d.ollama_num_predict),
        ollama_seed: parse_num_env("OLLAMA_SEED", env_map).unwrap_or(d.ollama_seed),
        ollama_think: args
            .ollama_think
            .as_deref()
            .and_then(normalize_ollama_think)
            .or_else(|| {
                get_env_val("OLLAMA_THINK", env_map)
                    .and_then(|value| normalize_ollama_think(&value))
            }),
        ollama_keep_alive: get_env_val("OLLAMA_KEEP_ALIVE", env_map)
            .filter(|v| !v.trim().is_empty()),
        ollama_instances: resolved_ollama_instances,
        extra_note: args
            .extra_note
            .clone()
            .or_else(|| get_env_val("EXTRA_NOTE", env_map)),
        no_news: args.no_news || get_bool_env("NO_NEWS", env_map),
        custom_news_query: args
            .custom_news_query
            .clone()
            .or_else(|| get_env_val("CUSTOM_NEWS_QUERY", env_map))
            .filter(|s| !s.trim().is_empty()),
        news_filter: args.news_filter || get_bool_env("NEWS_FILTER", env_map),
        show_news: args.show_news || get_bool_env("SHOW_NEWS", env_map),
        news_count: match args.news_count {
            Some(n_count) => n_count.clamp(1, 50),
            None => get_env_val("NEWS_COUNT", env_map)
                .and_then(|v| v.parse::<usize>().ok())
                .map(|n_count| n_count.clamp(1, 50))
                .unwrap_or(
                    if args.news_filter || get_bool_env("NEWS_FILTER", env_map) {
                        20
                    } else {
                        d.news_count
                    },
                ),
        },
        news_freshness: match args.news_freshness.clone() {
            Some(s) => s,
            None => get_env_val("NEWS_FRESHNESS", env_map).unwrap_or_else(|| {
                if args.news_filter || get_bool_env("NEWS_FILTER", env_map) {
                    "pw".to_string()
                } else {
                    d.news_freshness.clone()
                }
            }),
        },
        save_technical_log: args.save_technical_log || get_bool_env("SAVE_TECHNICAL_LOG", env_map),
        log_format: if cli_sources.log_format {
            args.log_format.clone()
        } else {
            get_env_val("LOG_FORMAT", env_map)
                .and_then(|v| {
                    let normalized = v.trim().to_ascii_lowercase();
                    match normalized.as_str() {
                        "csv" | "json" => Some(normalized),
                        other => {
                            eprintln!(
                                "⚠️ LOG_FORMAT value not supported: {}. Using {}.",
                                other, d.log_format
                            );
                            None
                        }
                    }
                })
                .unwrap_or_else(|| d.log_format.clone())
        },
        log_dir: if args.log_dir == d.log_dir {
            get_env_val("LOG_DIR", env_map).unwrap_or_else(|| d.log_dir.clone())
        } else {
            args.log_dir.clone()
        },
        data_append: args.data_append || get_bool_env("CSV_APPEND", env_map),
        log_flat: args.log_flat || get_bool_env("LOG_FLAT", env_map),
        stdout_log: args.stdout_log,
        silent: args.silent,
        max_note_length: get_usize_from_args_or_env(
            args.max_note_length,
            "MAX_NOTE_LENGTH",
            d.max_note_length,
            env_map,
        ),
        max_shortterm_length: get_usize_from_args_or_env(
            args.max_shortterm_length,
            "MAX_SHORTTERM_LENGTH",
            d.max_shortterm_length,
            env_map,
        ),
        max_midterm_length: get_usize_from_args_or_env(
            args.max_midterm_length,
            "MAX_MIDTERM_LENGTH",
            d.max_midterm_length,
            env_map,
        ),
        max_news_length: get_usize_from_args_or_env(
            args.max_news_length,
            "MAX_NEWS_LENGTH",
            d.max_news_length,
            env_map,
        ),
        max_review_length: get_usize_from_args_or_env(
            args.max_review_length,
            "MAX_REVIEW_LENGTH",
            d.max_review_length,
            env_map,
        ),
        ticker: args.ticker.clone().unwrap_or_else(|| d.ticker.clone()),
        alias_csv: args
            .alias_csv
            .clone()
            .or_else(|| get_env_val("ALIAS_CSV", env_map)),
        no_alias: args.no_alias || get_bool_env("NO_ALIAS", env_map),
        no_llm: args.no_llm || get_bool_env("NO_LLM", env_map),
        debug_prompt: args.debug_prompt || get_bool_env("DEBUG_PROMPT", env_map),
        debug_ollama: args.debug_ollama || get_bool_env("OLLAMA_DEBUG", env_map),
        ollama_bench_models: {
            let cli_models = normalize_ollama_benchmark_models(&args.ollama_bench_models);
            if !cli_models.is_empty() {
                cli_models
            } else {
                get_env_val("OLLAMA_BENCH_MODELS", env_map)
                    .map(|value| normalize_ollama_benchmark_models(&[value]))
                    .unwrap_or_default()
            }
        },
        ollama_bench_format: if cli_sources.ollama_bench_format {
            args.ollama_bench_format.clone()
        } else {
            get_env_val("OLLAMA_BENCH_FORMAT", env_map)
                .and_then(|value| normalize_ollama_benchmark_format(&value))
                .unwrap_or_else(|| args.ollama_bench_format.clone())
        },
        no_ollama_guard: args.no_ollama_guard || get_bool_env("OLLAMA_NO_GUARD", env_map),
        fundamental: args.fundamental || get_bool_env("FUNDAMENTAL", env_map),
        sec_user_agent: get_env_val("SEC_USER_AGENT", env_map).filter(|s| !s.trim().is_empty()),
        https_proxy: {
            let raw = get_env_val("HTTPS_PROXY", env_map)
                .filter(|s| !s.is_empty())
                .or_else(|| get_env_val("HTTP_PROXY", env_map).filter(|s| !s.is_empty()));
            match raw {
                Some(url) if url.contains('@') => {
                    eprintln!("⚠️  Authenticated proxy URL ignored: user:password@... is not supported. Configure proxy authentication at the OS level.");
                    None
                }
                other => other,
            }
        },
        no_proxy: get_env_val("NO_PROXY", env_map).filter(|s| !s.is_empty()),
        chat: args.chat,
        chat_memory: if cli_sources.chat_memory {
            args.chat_memory.clone()
        } else {
            match get_env_val("CHAT_MEMORY", env_map) {
                Some(v) => match v.trim() {
                    "low" | "mid" | "high" => v.trim().to_string(),
                    other => {
                        eprintln!("⚠️ CHAT_MEMORY value not supported: {}. Using mid.", other);
                        d.chat_memory.clone()
                    }
                },
                None => args.chat_memory.clone(),
            }
        },
        chat_guard: if cli_sources.chat_guard {
            args.chat_guard.clone()
        } else {
            match get_env_val("CHAT_GUARD", env_map) {
                Some(v) => match v.trim() {
                    "low" | "mid" | "high" => v.trim().to_string(),
                    other => {
                        eprintln!("⚠️ CHAT_GUARD value not supported: {}. Using high.", other);
                        d.chat_guard.clone()
                    }
                },
                None => args.chat_guard.clone(),
            }
        },
        debate: if cli_sources.debate {
            args.debate.clone()
        } else {
            match get_env_val("DEBATE", env_map) {
                Some(v) => match v.trim() {
                    "off" | "summary" | "claims" => v.trim().to_string(),
                    other => {
                        eprintln!("⚠️ DEBATE value not supported: {}. Using summary.", other);
                        d.debate.clone()
                    }
                },
                None => args.debate.clone(),
            }
        },
        read_depth: get_choice_env(
            "READ_DEPTH",
            env_map,
            &["shallow", "mid", "deep"],
            &d.read_depth,
        ),
        knowledge_scope: get_choice_env(
            "KNOWLEDGE_SCOPE",
            env_map,
            &["narrow", "mid", "wide"],
            &d.knowledge_scope,
        ),
        response_shape: get_choice_env(
            "RESPONSE_SHAPE",
            env_map,
            &["talk", "points", "scenario"],
            &d.response_shape,
        ),
        forecast_mode: get_choice_env(
            "FORECAST_MODE",
            env_map,
            &["off", "soft", "bold"],
            &d.forecast_mode,
        ),
        lang: if cli_sources.lang {
            args.lang.clone()
        } else {
            // Read LANG only from env_map (xoksa.env) to avoid collision with the system LANG variable
            match env_map.get("LANG").map(|s| s.trim().to_ascii_lowercase()) {
                Some(v) if v == "ja" => "ja".to_string(),
                Some(v) if v == "en" => "en".to_string(),
                _ => args.lang.clone(),
            }
        },
        autoreload: args.autoreload || get_bool_env("AUTORELOAD", env_map),
        autoreload_notify: args.autoreload_notify || get_bool_env("AUTORELOAD_NOTIFY", env_map),
        notify_channels: scan_notify_channels(env_map),
        alert_rules: scan_alert_rules(env_map),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_config, build_config_from_arg_matches, build_config_with_value_sources, AnalysisMode,
        Args, CliValueSources,
    };
    use clap::{CommandFactory, Parser};

    #[test]
    fn parse_alert_condition_maps_ops_and_rejects_bad_input() {
        use super::parse_alert_condition;
        let c = parse_alert_condition("rsi<=30").expect("rsi<=30 parses");
        assert_eq!(
            (c.left.as_str(), c.op.as_str(), c.value),
            ("rsi", "le", Some(30.0))
        );
        assert_eq!(c.right_kind, "value");
        // Surrounding whitespace and the other three ops.
        assert_eq!(
            parse_alert_condition(" score >= 4 ").map(|c| (c.left, c.op, c.value)),
            Some(("score".to_string(), "ge".to_string(), Some(4.0)))
        );
        assert_eq!(
            parse_alert_condition("adx>25").map(|c| c.op),
            Some("gt".to_string())
        );
        assert_eq!(
            parse_alert_condition("close<100").map(|c| c.op),
            Some("lt".to_string())
        );
        // Unknown indicator, unsupported operator, non-numeric RHS, no operator → None.
        assert!(parse_alert_condition("bogus<=30").is_none());
        assert!(parse_alert_condition("rsi==30").is_none());
        assert!(parse_alert_condition("rsi<=abc").is_none());
        assert!(parse_alert_condition("rsi").is_none());
    }

    #[test]
    fn scan_notify_channels_skips_unknown_kind() {
        use super::scan_notify_channels;
        let mut env = std::collections::HashMap::new();
        env.insert("NOTIFY_1_KIND".to_string(), "slack".to_string());
        env.insert("NOTIFY_1_NAME".to_string(), "team".to_string());
        env.insert("NOTIFY_2_KIND".to_string(), "telegram".to_string()); // unsupported → skipped
        env.insert("NOTIFY_3_KIND".to_string(), "line".to_string());
        env.insert("NOTIFY_3_TO".to_string(), "U123".to_string());
        let ch = scan_notify_channels(&env);
        assert_eq!(ch.len(), 2);
        assert_eq!(ch[0].n, 1);
        assert_eq!(ch[0].kind, crate::notify::NotifierKind::Slack);
        assert_eq!(ch[0].name, "team");
        assert_eq!(ch[1].n, 3);
        assert_eq!(ch[1].kind, crate::notify::NotifierKind::Line);
        assert_eq!(ch[1].to.as_deref(), Some("U123"));
    }

    #[test]
    fn scan_alert_rules_parses_and_skips_incomplete() {
        use super::scan_alert_rules;
        let mut env = std::collections::HashMap::new();
        env.insert("ALERT_1_TICKER".to_string(), "7203.T".to_string());
        env.insert("ALERT_1_WHEN".to_string(), "rsi<=30".to_string());
        env.insert("ALERT_1_MODE".to_string(), "5m".to_string());
        env.insert("ALERT_1_NOTIFY".to_string(), "2".to_string());
        env.insert("ALERT_1_EXPLAIN".to_string(), "true".to_string());
        env.insert("ALERT_2_TICKER".to_string(), "AAPL".to_string()); // missing WHEN → skipped
        env.insert("ALERT_3_WHEN".to_string(), "score>=4".to_string()); // missing TICKER → skipped
        let rules = scan_alert_rules(&env);
        assert_eq!(rules.len(), 1);
        let r = &rules[0];
        assert_eq!(r.n, 1);
        assert_eq!(r.ticker, "7203.T");
        assert_eq!((r.when.left.as_str(), r.when.op.as_str()), ("rsi", "le"));
        assert_eq!(r.notify, 2);
        assert!(r.explain);
        assert!(r.mode.is_some());
    }

    #[test]
    fn every_timeframe_has_backtest_periods_within_provider_limits() {
        for m in AnalysisMode::ALL {
            let ranges = m.backtest_period_ranges();
            assert!(
                !ranges.is_empty(),
                "{:?} must offer at least one backtest period",
                m
            );
            // Intraday caps: 1m ~7d, 5m/15m/30m ~60d — must not offer year ranges.
            let has_years = ranges.iter().any(|r| r.ends_with('y') || *r == "max");
            if m.is_intraday() && !matches!(m, AnalysisMode::Intraday60m) {
                assert!(
                    !has_years,
                    "{:?} is capped at ~days by the provider; must not offer year ranges: {:?}",
                    m, ranges
                );
            }
        }
    }
    use std::collections::HashMap;

    // Drift guard: every env key the app reads (in config.rs / bootstrap.rs) must be
    // documented in xoksa.env.sample. When a new option is added but the sample is not
    // updated, this test fails — keeping the user-facing reference complete without a
    // hand-maintained key list (keys are auto-discovered from the source).
    #[test]
    fn xoksa_env_sample_documents_all_config_env_keys() {
        use regex::Regex;
        use std::collections::BTreeSet;

        let sources = [include_str!("config.rs"), include_str!("bootstrap.rs")];
        // Pattern 1: key is the first argument of the env-reading helper.
        let direct = Regex::new(
            r#"(?:get_env_val|get_bool_env|get_choice_env|get_usize_env|env_map\.get)\(\s*"([A-Z][A-Z0-9_]+)""#,
        )
        .unwrap();
        // Pattern 2: *_from_args_or_env(args.x, "KEY", ...) — key is the second argument.
        let args_form =
            Regex::new(r#"get_(?:usize|f64)_from_args_or_env\([^,]*,\s*"([A-Z][A-Z0-9_]+)""#)
                .unwrap();

        let mut keys: BTreeSet<String> = BTreeSet::new();
        for src in sources {
            for cap in direct.captures_iter(src) {
                keys.insert(cap[1].to_string());
            }
            for cap in args_form.captures_iter(src) {
                keys.insert(cap[1].to_string());
            }
        }

        // Guard against a vacuous pass if the extraction regex ever stops matching.
        assert!(
            keys.len() >= 50,
            "env-key extraction looks broken: only {} keys found",
            keys.len()
        );

        // Keys intentionally absent from xoksa.env.sample. Class A credentials read in
        // fundamental.rs are out of scope here; API keys read in config.rs are present as
        // commented placeholders. Add a key here only with a clear reason.
        const EXCLUDE: &[&str] = &[];

        let sample = include_str!("../xoksa.env.sample");
        let documented = |key: &str| -> bool {
            sample.lines().any(|line| {
                let l = line.trim_start_matches('#').trim_start();
                l.starts_with(&format!("{key}="))
            })
        };

        let missing: Vec<&String> = keys
            .iter()
            .filter(|k| !EXCLUDE.contains(&k.as_str()) && !documented(k))
            .collect();

        assert!(
            missing.is_empty(),
            "xoksa.env.sample is missing env keys read by the code: {missing:?}. \
             Add each as a commented entry (e.g. #KEY=default), or add it to EXCLUDE \
             if intentionally undocumented (e.g. a Keychain-only credential)."
        );
    }

    // Value drift guard: every default documented in xoksa.env.sample must equal
    // the corresponding `Config::default()` field. `Config::default()` is the single
    // source of truth (SOT §4.2); this test fails if the sample drifts from it.
    #[test]
    fn xoksa_env_sample_values_match_config_default() {
        let sample = include_str!("../xoksa.env.sample");
        let d = super::Config::default();

        // Value token for `key` from the sample (commented lines count — the leading
        // `#` and spaces are stripped), up to the first whitespace or trailing `#`.
        fn val(sample: &str, key: &str) -> String {
            sample
                .lines()
                .find_map(|line| {
                    let l = line.trim_start_matches('#').trim_start();
                    let rest = l.strip_prefix(&format!("{key}="))?;
                    Some(
                        rest.chars()
                            .take_while(|c| !c.is_whitespace() && *c != '#')
                            .collect::<String>(),
                    )
                })
                .unwrap_or_else(|| panic!("xoksa.env.sample has no documented value for {key}"))
        }
        fn f(sample: &str, key: &str) -> f64 {
            val(sample, key)
                .parse()
                .unwrap_or_else(|_| panic!("{key} in sample is not a number"))
        }
        fn i(sample: &str, key: &str) -> i64 {
            val(sample, key)
                .parse()
                .unwrap_or_else(|_| panic!("{key} in sample is not an integer"))
        }

        // f64 defaults
        assert_eq!(f(sample, "BUY_RSI"), d.buy_rsi);
        assert_eq!(f(sample, "SELL_RSI"), d.sell_rsi);
        assert_eq!(f(sample, "MACD_DIFF_LOW"), d.macd_diff_low);
        assert_eq!(f(sample, "MACD_DIFF_MID"), d.macd_diff_mid);
        assert_eq!(f(sample, "MACD_DIFF_EXTREME"), d.macd_diff_extreme);
        assert_eq!(f(sample, "WEIGHT_BASIC"), d.weight_basic);
        assert_eq!(f(sample, "WEIGHT_EMA"), d.weight_ema);
        assert_eq!(f(sample, "WEIGHT_SMA"), d.weight_sma);
        assert_eq!(f(sample, "WEIGHT_BOLLINGER"), d.weight_bollinger);
        assert_eq!(f(sample, "WEIGHT_ROC"), d.weight_roc);
        assert_eq!(f(sample, "WEIGHT_ADX"), d.weight_adx);
        assert_eq!(f(sample, "WEIGHT_STOCHASTICS"), d.weight_stochastics);
        assert_eq!(f(sample, "WEIGHT_FIBONACCI"), d.weight_fibonacci);
        assert_eq!(f(sample, "WEIGHT_VWAP"), d.weight_vwap);
        assert_eq!(f(sample, "WEIGHT_ICHIMOKU"), d.weight_ichimoku);
        assert_eq!(
            f(sample, "BOLLINGER_STDDEV_MULTIPLIER"),
            d.bollinger_stddev_multiplier
        );
        assert_eq!(
            f(sample, "BB_BANDWIDTH_SQUEEZE_PCT"),
            d.bb_bandwidth_squeeze_pct
        );
        assert_eq!(
            f(sample, "FIBONACCI_NEUTRAL_RATIO"),
            d.fibonacci_neutral_ratio
        );
        assert_eq!(f(sample, "llm_temperature"), d.llm_temperature);
        assert_eq!(f(sample, "llm_top_p"), d.llm_top_p);

        // integer defaults
        assert_eq!(i(sample, "EMA_SHORT_PERIOD") as usize, d.ema_short_period);
        assert_eq!(i(sample, "EMA_LONG_PERIOD") as usize, d.ema_long_period);
        assert_eq!(i(sample, "SMA_SHORT_PERIOD") as usize, d.sma_short_period);
        assert_eq!(i(sample, "SMA_LONG_PERIOD") as usize, d.sma_long_period);
        assert_eq!(i(sample, "ADX_PERIOD") as usize, d.adx_period);
        assert_eq!(i(sample, "ROC_PERIOD") as usize, d.roc_period);
        assert_eq!(
            i(sample, "STOCHASTICS_PERIOD") as usize,
            d.stochastics_period
        );
        assert_eq!(i(sample, "BOLLINGER_PERIOD") as usize, d.bollinger_period);
        assert_eq!(
            i(sample, "ICHIMOKU_TENKAN_PERIOD") as usize,
            d.ichimoku_tenkan_period
        );
        assert_eq!(
            i(sample, "ICHIMOKU_KIJUN_PERIOD") as usize,
            d.ichimoku_kijun_period
        );
        assert_eq!(i(sample, "VWAP_PERIOD") as usize, d.vwap_period);
        assert_eq!(i(sample, "NEWS_COUNT") as usize, d.news_count);
        assert_eq!(i(sample, "MAX_NOTE_LENGTH") as usize, d.max_note_length);
        assert_eq!(
            i(sample, "MAX_SHORTTERM_LENGTH") as usize,
            d.max_shortterm_length
        );
        assert_eq!(
            i(sample, "MAX_MIDTERM_LENGTH") as usize,
            d.max_midterm_length
        );
        assert_eq!(i(sample, "MAX_NEWS_LENGTH") as usize, d.max_news_length);
        assert_eq!(i(sample, "MAX_REVIEW_LENGTH") as usize, d.max_review_length);
        assert_eq!(i(sample, "llm_timeout_seconds") as u64, d.llm_timeout_secs);
        assert_eq!(
            i(sample, "llm_max_output_tokens") as u32,
            d.llm_max_output_tokens
        );
        assert_eq!(i(sample, "claude_max_tokens") as u32, d.claude_max_tokens);
        assert_eq!(i(sample, "OLLAMA_NUM_CTX") as u32, d.ollama_num_ctx);
        assert_eq!(i(sample, "OLLAMA_NUM_PREDICT") as i32, d.ollama_num_predict);
        assert_eq!(i(sample, "OLLAMA_SEED"), d.ollama_seed);
        // The ollama timeout floor is single-sourced in llm.rs (a business-rule
        // minimum, not a Config::default field); bind the sample to it.
        assert_eq!(
            i(sample, "OLLAMA_TIMEOUT_SECONDS") as u64,
            crate::llm::OLLAMA_MIN_TIMEOUT_SECS
        );

        // string / enum defaults
        assert_eq!(val(sample, "LOG_FORMAT"), d.log_format);
        assert_eq!(val(sample, "LOG_DIR"), d.log_dir);
        assert_eq!(val(sample, "NEWS_FRESHNESS"), d.news_freshness);
        assert_eq!(val(sample, "llm_provider"), d.llm_provider);
        assert_eq!(val(sample, "openai_model"), d.openai_model);
        assert_eq!(val(sample, "gemini_model"), d.gemini_model);
        assert_eq!(val(sample, "claude_model"), d.claude_model);
        assert_eq!(val(sample, "LANG"), d.lang);
        assert_eq!(val(sample, "CHAT_MEMORY"), d.chat_memory);
        assert_eq!(val(sample, "CHAT_GUARD"), d.chat_guard);
        assert_eq!(val(sample, "DEBATE"), d.debate);
        assert_eq!(val(sample, "READ_DEPTH"), d.read_depth);
        assert_eq!(val(sample, "KNOWLEDGE_SCOPE"), d.knowledge_scope);
        assert_eq!(val(sample, "RESPONSE_SHAPE"), d.response_shape);
        assert_eq!(val(sample, "FORECAST_MODE"), d.forecast_mode);
        assert_eq!(super::parse_stance(&val(sample, "STANCE")), d.stance);
    }

    // Structural guard that the CLI/clap layer and the env-resolution layer both
    // derive their defaults from `Config::default()`: a bare parse with an empty
    // env must reproduce `Config::default()` for every scalar default.
    #[test]
    fn bare_build_config_scalar_defaults_match_config_default() {
        let args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
        let cfg = build_config(&args, &HashMap::new());
        let d = super::Config::default();

        assert_eq!(cfg.buy_rsi, d.buy_rsi);
        assert_eq!(cfg.sell_rsi, d.sell_rsi);
        assert_eq!(cfg.macd_diff_low, d.macd_diff_low);
        assert_eq!(cfg.macd_diff_mid, d.macd_diff_mid);
        assert_eq!(cfg.macd_diff_extreme, d.macd_diff_extreme);
        assert_eq!(cfg.weight_basic, d.weight_basic);
        assert_eq!(cfg.weight_ema, d.weight_ema);
        assert_eq!(cfg.weight_ichimoku, d.weight_ichimoku);
        assert_eq!(cfg.bb_bandwidth_squeeze_pct, d.bb_bandwidth_squeeze_pct);
        assert_eq!(
            cfg.bollinger_stddev_multiplier,
            d.bollinger_stddev_multiplier
        );
        assert_eq!(cfg.fibonacci_neutral_ratio, d.fibonacci_neutral_ratio);
        assert_eq!(cfg.ema_short_period, d.ema_short_period);
        assert_eq!(cfg.ema_long_period, d.ema_long_period);
        assert_eq!(cfg.sma_short_period, d.sma_short_period);
        assert_eq!(cfg.sma_long_period, d.sma_long_period);
        assert_eq!(cfg.roc_period, d.roc_period);
        assert_eq!(cfg.adx_period, d.adx_period);
        assert_eq!(cfg.stochastics_period, d.stochastics_period);
        assert_eq!(cfg.bollinger_period, d.bollinger_period);
        assert_eq!(cfg.vwap_period, d.vwap_period);
        assert_eq!(cfg.ichimoku_tenkan_period, d.ichimoku_tenkan_period);
        assert_eq!(cfg.ichimoku_kijun_period, d.ichimoku_kijun_period);
        assert_eq!(cfg.max_note_length, d.max_note_length);
        assert_eq!(cfg.max_shortterm_length, d.max_shortterm_length);
        assert_eq!(cfg.max_midterm_length, d.max_midterm_length);
        assert_eq!(cfg.max_news_length, d.max_news_length);
        assert_eq!(cfg.max_review_length, d.max_review_length);
        assert_eq!(cfg.llm_timeout_secs, d.llm_timeout_secs);
        assert_eq!(cfg.llm_temperature, d.llm_temperature);
        assert_eq!(cfg.llm_top_p, d.llm_top_p);
        assert_eq!(cfg.llm_max_output_tokens, d.llm_max_output_tokens);
        assert_eq!(cfg.claude_max_tokens, d.claude_max_tokens);
        assert_eq!(cfg.ollama_num_ctx, d.ollama_num_ctx);
        assert_eq!(cfg.ollama_num_predict, d.ollama_num_predict);
        assert_eq!(cfg.ollama_seed, d.ollama_seed);
        assert_eq!(cfg.news_count, d.news_count);
        assert_eq!(cfg.news_freshness, d.news_freshness);
        assert_eq!(cfg.log_format, d.log_format);
        assert_eq!(cfg.log_dir, d.log_dir);
        assert_eq!(cfg.stance, d.stance);
        assert_eq!(cfg.llm_provider, d.llm_provider);
    }

    #[test]
    fn log_format_uses_env_when_cli_is_unspecified() {
        let args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
        let env_map = HashMap::from([("LOG_FORMAT".to_string(), "json".to_string())]);

        let config = build_config(&args, &env_map);

        assert_eq!(config.log_format, "json");
    }

    #[test]
    fn log_format_non_default_cli_overrides_env() {
        let args = Args::parse_from(["xoksa", "--ticker", "SPY", "--log-format", "json"]);
        let env_map = HashMap::from([("LOG_FORMAT".to_string(), "csv".to_string())]);

        let config = build_config_with_value_sources(
            &args,
            &env_map,
            CliValueSources {
                log_format: true,
                ..CliValueSources::default()
            },
        );

        assert_eq!(config.log_format, "json");
    }

    #[test]
    fn log_format_explicit_csv_cli_overrides_env() {
        let args = Args::parse_from(["xoksa", "--ticker", "SPY", "--log-format", "csv"]);
        let env_map = HashMap::from([("LOG_FORMAT".to_string(), "json".to_string())]);

        let config = build_config_with_value_sources(
            &args,
            &env_map,
            CliValueSources {
                log_format: true,
                ..CliValueSources::default()
            },
        );

        assert_eq!(config.log_format, "csv");
    }

    #[test]
    fn macd_diff_extreme_uses_env_when_cli_is_unspecified() {
        let args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
        let env_map = HashMap::from([("MACD_DIFF_EXTREME".to_string(), "55.5".to_string())]);

        let config = build_config(&args, &env_map);

        assert_eq!(config.macd_diff_extreme, 55.5);
    }

    #[test]
    fn macd_diff_extreme_cli_overrides_env() {
        let args = Args::parse_from(["xoksa", "--ticker", "SPY", "--macd-diff-extreme", "77.7"]);
        let env_map = HashMap::from([("MACD_DIFF_EXTREME".to_string(), "55.5".to_string())]);

        let config = build_config(&args, &env_map);

        assert_eq!(config.macd_diff_extreme, 77.7);
    }

    #[test]
    fn public_arg_matches_builder_preserves_explicit_default_log_format() {
        let matches = Args::command()
            .try_get_matches_from(["xoksa", "--ticker", "SPY", "--log-format", "csv"])
            .expect("valid args");
        let env_map = HashMap::from([("LOG_FORMAT".to_string(), "json".to_string())]);

        let config = build_config_from_arg_matches(&matches, &env_map).expect("valid config");

        assert_eq!(config.log_format, "csv");
    }

    #[test]
    fn analysis_mode_defaults_to_daily() {
        let args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
        let env_map = HashMap::new();

        let config = build_config(&args, &env_map);

        assert_eq!(config.analysis_mode, AnalysisMode::Daily);
    }

    #[test]
    fn analysis_mode_uses_env_when_cli_is_unspecified() {
        let args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
        let env_map = HashMap::from([("ANALYSIS_MODE".to_string(), "30m".to_string())]);

        let config = build_config(&args, &env_map);

        assert_eq!(config.analysis_mode, AnalysisMode::Intraday30m);
    }

    #[test]
    fn analysis_mode_accepts_15m_from_env() {
        let args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
        let env_map = HashMap::from([("ANALYSIS_MODE".to_string(), "15m".to_string())]);

        let config = build_config(&args, &env_map);

        assert_eq!(config.analysis_mode, AnalysisMode::Intraday15m);
    }

    #[test]
    fn public_arg_matches_builder_preserves_explicit_daily_analysis_mode() {
        let matches = Args::command()
            .try_get_matches_from(["xoksa", "--ticker", "SPY", "--analysis-mode", "daily"])
            .expect("valid args");
        let env_map = HashMap::from([("ANALYSIS_MODE".to_string(), "30m".to_string())]);

        let config = build_config_from_arg_matches(&matches, &env_map).expect("valid config");

        assert_eq!(config.analysis_mode, AnalysisMode::Daily);
    }

    #[test]
    fn analysis_mode_accepts_short_alias() {
        let args = Args::parse_from(["xoksa", "--ticker", "SPY", "--analysis-mode", "short"]);
        let env_map = HashMap::new();

        let config = build_config(&args, &env_map);

        assert_eq!(config.analysis_mode, AnalysisMode::Intraday30m);
    }

    #[test]
    fn analysis_mode_accepts_5m_cli() {
        let args = Args::parse_from(["xoksa", "--ticker", "SPY", "--analysis-mode", "5m"]);
        let env_map = HashMap::new();

        let config = build_config(&args, &env_map);

        assert_eq!(config.analysis_mode, AnalysisMode::Intraday5m);
    }

    #[test]
    fn analysis_mode_accepts_weekly_cli() {
        let args = Args::parse_from(["xoksa", "--ticker", "SPY", "--analysis-mode", "weekly"]);
        let env_map = HashMap::new();

        let config = build_config(&args, &env_map);

        assert_eq!(config.analysis_mode, AnalysisMode::Weekly);
    }

    #[test]
    fn analysis_mode_accepts_monthly_env() {
        let args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
        let env_map = HashMap::from([("ANALYSIS_MODE".to_string(), "monthly".to_string())]);

        let config = build_config(&args, &env_map);

        assert_eq!(config.analysis_mode, AnalysisMode::Monthly);
    }

    #[test]
    fn ollama_seed_uses_env_when_present() {
        // Use a non-default seed so the assertion proves the env value was read
        // (the default is 7 apart from this).
        let args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
        let env_map = HashMap::from([("OLLAMA_SEED".to_string(), "7".to_string())]);

        let config = build_config(&args, &env_map);

        assert_eq!(config.ollama_seed, 7);
    }

    #[test]
    fn parse_num_env_returns_none_and_warns_on_invalid() {
        // A malformed numeric value returns None (caller falls back to the default)
        // AND emits a stderr warning, so a typo is visible instead of silently
        // reverting (errors-are-actionable). Valid parses through unchanged.
        let env = HashMap::from([
            ("XOKSA_TEST_VALID".to_string(), "8192".to_string()),
            ("XOKSA_TEST_BAD".to_string(), "8192x".to_string()),
        ]);
        assert_eq!(
            super::parse_num_env::<i32>("XOKSA_TEST_VALID", &env),
            Some(8192)
        );
        assert_eq!(super::parse_num_env::<i32>("XOKSA_TEST_BAD", &env), None);
        assert_eq!(super::parse_num_env::<i32>("XOKSA_TEST_UNSET", &env), None);
    }

    #[test]
    fn ollama_alias_is_set_only_for_ollama_provider() {
        let args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
        let env_map = HashMap::from([
            ("llm_provider".to_string(), "openai".to_string()),
            ("OLLAMA_1_ALIAS".to_string(), "gpu1".to_string()),
            ("OLLAMA_1_MODEL".to_string(), "gpt-oss:20b".to_string()),
        ]);

        let config = build_config(&args, &env_map);

        assert_eq!(config.llm_provider, "openai");
        assert_eq!(config.ollama_alias, "");

        let env_map = HashMap::from([
            ("llm_provider".to_string(), "ollama".to_string()),
            ("OLLAMA_1_ALIAS".to_string(), "gpu1".to_string()),
            ("OLLAMA_1_MODEL".to_string(), "gpt-oss:20b".to_string()),
        ]);

        let config = build_config(&args, &env_map);

        assert_eq!(config.llm_provider, "ollama");
        assert_eq!(config.llm_model, "gpt-oss:20b");
        assert_eq!(config.ollama_alias, "gpu1");
    }

    #[test]
    fn debug_ollama_can_be_enabled_by_cli_or_env() {
        let cli_args = Args::parse_from(["xoksa", "--ticker", "SPY", "--debug-ollama"]);
        let env_map = HashMap::new();
        let cli_config = build_config(&cli_args, &env_map);
        assert!(cli_config.debug_ollama);

        let env_args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
        let env_map = HashMap::from([("OLLAMA_DEBUG".to_string(), "true".to_string())]);
        let env_config = build_config(&env_args, &env_map);
        assert!(env_config.debug_ollama);
    }

    #[test]
    fn autoreload_can_be_enabled_by_cli_or_env() {
        let cli_args = Args::parse_from([
            "xoksa",
            "--ticker",
            "SPY",
            "--chat",
            "--autoreload",
            "--autoreload-notify",
        ]);
        let cli_config = build_config(&cli_args, &HashMap::new());
        assert!(cli_config.autoreload);
        assert!(cli_config.autoreload_notify);

        let env_args = Args::parse_from(["xoksa", "--ticker", "SPY", "--chat"]);
        let env_map = HashMap::from([
            ("AUTORELOAD".to_string(), "true".to_string()),
            ("AUTORELOAD_NOTIFY".to_string(), "true".to_string()),
        ]);
        let env_config = build_config(&env_args, &env_map);
        assert!(env_config.autoreload);
        assert!(env_config.autoreload_notify);
    }

    #[test]
    fn debate_can_be_set_by_cli_or_env() {
        let cli_args =
            Args::parse_from(["xoksa", "--ticker", "SPY", "--chat", "--debate", "claims"]);
        let cli_config = build_config(&cli_args, &HashMap::new());
        assert_eq!(cli_config.debate, "claims");

        let env_args = Args::parse_from(["xoksa", "--ticker", "SPY", "--chat"]);
        let env_map = HashMap::from([("DEBATE".to_string(), "summary".to_string())]);
        let env_config = build_config(&env_args, &env_map);
        assert_eq!(env_config.debate, "summary");

        let matches = Args::command()
            .try_get_matches_from(["xoksa", "--ticker", "SPY", "--chat", "--debate", "summary"])
            .unwrap();
        let env_map = HashMap::from([("DEBATE".to_string(), "claims".to_string())]);
        let config = build_config_from_arg_matches(&matches, &env_map).unwrap();
        assert_eq!(config.debate, "summary");
    }

    #[test]
    fn ollama_think_can_be_set_by_cli_or_env() {
        let cli_args = Args::parse_from(["xoksa", "--ticker", "SPY", "--ollama-think", "low"]);
        let env_map = HashMap::from([("OLLAMA_THINK".to_string(), "high".to_string())]);
        let cli_config = build_config(&cli_args, &env_map);
        assert_eq!(cli_config.ollama_think.as_deref(), Some("low"));

        let env_args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
        let env_map = HashMap::from([("OLLAMA_THINK".to_string(), "false".to_string())]);
        let env_config = build_config(&env_args, &env_map);
        assert_eq!(env_config.ollama_think.as_deref(), Some("false"));
    }

    #[test]
    fn no_ollama_guard_can_be_enabled_by_cli_or_env() {
        let default_args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
        let default_config = build_config(&default_args, &HashMap::new());
        assert!(!default_config.no_ollama_guard);

        let cli_args = Args::parse_from(["xoksa", "--ticker", "SPY", "--no-ollama-guard"]);
        let cli_config = build_config(&cli_args, &HashMap::new());
        assert!(cli_config.no_ollama_guard);

        let env_args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
        let env_map = HashMap::from([("OLLAMA_NO_GUARD".to_string(), "true".to_string())]);
        let env_config = build_config(&env_args, &env_map);
        assert!(env_config.no_ollama_guard);
    }

    #[test]
    fn ollama_bench_models_can_be_set_by_cli_or_env() {
        let cli_args = Args::parse_from([
            "xoksa",
            "--ticker",
            "SPY",
            "--ollama-bench",
            "llama3,gpt-oss:20b",
        ]);
        let cli_config = build_config(&cli_args, &HashMap::new());
        assert_eq!(
            cli_config.ollama_bench_models,
            vec!["llama3".to_string(), "gpt-oss:20b".to_string()]
        );

        let env_args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
        let env_map = HashMap::from([(
            "OLLAMA_BENCH_MODELS".to_string(),
            "mistral, qwen2.5".to_string(),
        )]);
        let env_config = build_config(&env_args, &env_map);
        assert_eq!(
            env_config.ollama_bench_models,
            vec!["mistral".to_string(), "qwen2.5".to_string()]
        );
    }

    #[test]
    fn ollama_bench_format_uses_cli_over_env() {
        let matches = Args::command()
            .try_get_matches_from(["xoksa", "--ticker", "SPY", "--ollama-bench-format", "json"])
            .unwrap();
        let env_map = HashMap::from([("OLLAMA_BENCH_FORMAT".to_string(), "csv".to_string())]);

        let config = build_config_from_arg_matches(&matches, &env_map).unwrap();

        assert_eq!(config.ollama_bench_format, "json");
    }

    #[test]
    fn debate_invalid_env_value_falls_back_to_summary() {
        let args = Args::parse_from(["xoksa", "--ticker", "SPY", "--chat"]);
        let env_map = HashMap::from([("DEBATE".to_string(), "bogus".to_string())]);
        let config = build_config(&args, &env_map);
        assert_eq!(config.debate, "summary");
    }
}
