//! Technical indicator types and data structures

/// Each indicator's score ranges from -MAX_ABS_INDICATOR_SCORE to +MAX_ABS_INDICATOR_SCORE.
/// Used in clamp guards and total_weight calculation.
pub const MAX_ABS_INDICATOR_SCORE: f64 = 2.0;

/// Typed representation of the basic signal score produced by MACD/RSI logic.
/// Numeric values: StrongBuy=+2, Buy=+1, Neutral=0, Sell=-1, StrongSell=-2.
/// Use `to_f64()` when the value is needed for arithmetic (composite scoring).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalStrength {
    StrongBuy,
    Buy,
    Neutral,
    Sell,
    StrongSell,
}

impl SignalStrength {
    pub fn from_f64(v: f64) -> Option<Self> {
        match v.round() as i32 {
            2 => Some(Self::StrongBuy),
            1 => Some(Self::Buy),
            0 => Some(Self::Neutral),
            -1 => Some(Self::Sell),
            -2 => Some(Self::StrongSell),
            _ => None,
        }
    }

    pub fn to_f64(self) -> f64 {
        match self {
            Self::StrongBuy => 2.0,
            Self::Buy => 1.0,
            Self::Neutral => 0.0,
            Self::Sell => -1.0,
            Self::StrongSell => -2.0,
        }
    }
}

/// Holds the analysis result for a single technical indicator
pub struct AnalysisResult {
    pub indicator_name: String, // e.g. "Basic Technical Analysis", "EMA", "SMA"
    pub description: Vec<String>, // multi-line display text (newline-separated is fine)
    pub score: Option<f64>,     // raw score (-2 to +2 integer); None on calculation failure
}

/// Snapshot of the final composite score (single source of truth)
pub struct FinalScoreSnapshot {
    pub total_score: f64,  // Σ(score × weight) for all active indicators
    pub total_weight: f64, // 2 × Σ(weight of active indicators); each indicator range is [-2, 2]
    pub score_ratio: f64,  // total_score / total_weight (-1..+1)
}

/// Holds the full analysis result (shared by screen output, logs, and LLM prompt)
#[derive(Debug, Clone)]
pub struct TechnicalDataEntry {
    pub ticker: String,           // ticker symbol (e.g. AAPL, MSFT, 7203.T)
    pub name: String,             // company name (e.g. NVIDIA Corp, SoftBank)
    pub date: String,             // analysis bar date (e.g. 2025-05-09)
    pub datetime: Option<String>, // analysis bar datetime (bar_time)
    pub timestamp: Option<i64>,   // analysis bar timestamp (bar_time UNIX seconds)
    pub timezone: String,         // IANA TZ (exchangeTimezoneName)
    /// ISO 4217 currency the prices are quoted in, as the provider reported
    /// it. `None` when the provider did not report one — never guessed.
    pub currency: Option<String>,
    pub latest_observed_price: Option<f64>, // latest observed price
    pub market_data_latest_time: Option<String>, // market data latest timestamp (human-readable)
    pub market_data_latest_timestamp: Option<i64>, // market data latest timestamp (UNIX seconds)
    pub analyzed_at: Option<String>,        // analysis execution time
    pub analyzed_at_timestamp: Option<i64>, // analysis execution time (UNIX seconds)
    pub source_note: Option<String>, // fallback-provenance note (Some when data came from a degraded fallback source)
    pub close: f64,                  // analysis bar close price
    pub previous_close: f64,         // previous analysis bar close price
    pub price_diff: f64,             // close price change from previous bar
    pub price_diff_percent: f64,     // close price change ratio from previous bar (%)
    pub macd: f64,                   // MACD value
    pub signal: f64,                 // MACD signal value
    pub prev_macd: f64,              // previous bar MACD value (for diff calculation)
    pub prev_signal: f64,            // previous bar MACD signal value (for diff calculation)
    pub rsi: f64,                    // RSI value
    pub ema_short: f64,              // short-term EMA (e.g. 5 periods)
    pub ema_long: f64,               // long-term EMA (e.g. 20 periods)
    pub sma_short: f64,              // short-term SMA
    pub sma_long: f64,               // long-term SMA
    pub roc: Option<f64>,            // ROC (Rate of Change); extension indicator
    pub adx: Option<f64>,            // ADX (trend strength); extension indicator
    pub stochastics_k: Option<f64>,  // Stochastics %K
    pub stochastics_d: Option<f64>,  // Stochastics %D
    pub bb_upper: f64,               // Bollinger Bands upper band
    pub bb_lower: f64,               // Bollinger Bands lower band
    pub bb_percent_b: f64,           // %B indicator
    pub bb_bandwidth: f64,           // Bandwidth (%) stored as 0–100 float
    pub fibo_38_2: Option<f64>,      // Fibonacci 38.2% level
    pub fibo_50_0: Option<f64>,      // Fibonacci 50.0% level
    pub fibo_61_8: Option<f64>,      // Fibonacci 61.8% level
    pub vwap: Option<f64>,           // VWAP
    pub tenkan_sen: Option<f64>,     // Ichimoku Tenkan-sen (conversion line)
    pub kijun_sen: Option<f64>,      // Ichimoku Kijun-sen (base line)
    pub ema_score: Option<f64>,      // score from EMA
    pub sma_score: Option<f64>,      // score from SMA
    pub roc_score: Option<f64>,      // score from ROC
    pub adx_score: Option<f64>,      // score from ADX
    pub stochastics_score: Option<f64>, // score from Stochastics
    pub bollinger_score: Option<f64>, // score from Bollinger Bands
    pub fibonacci_score: Option<f64>, // score from Fibonacci
    pub vwap_score: Option<f64>,     // score from VWAP
    pub ichimoku_score: Option<f64>, // score from Ichimoku
    pub signal_score: f64,           // basic signal score (from MACD/RSI)
    pub latest_volume: Option<f64>,  // latest bar volume (from price fetch; None if unavailable)
    pub avg_volume: Option<f64>,     // average volume over sma_long_period bars
    pub volume_ratio: Option<f64>, // latest_volume / avg_volume (None if avg is zero or unavailable)
}

/// Rule/alert indicator keys that a plain `f64` field cannot distinguish from a
/// legitimate zero. A key is present only when its setter ran with a finite value,
/// so "never computed", "computation failed" and "computed as 0.0" stay distinct.
/// The vocabulary is the one `backtest::guard_indicator_map` evaluates against.
pub type ComputedIndicators = std::collections::BTreeSet<&'static str>;

/// Wrapper struct that provides controlled access to TechnicalDataEntry.
/// `entry` is private: every write goes through a setter, so no caller outside
/// this module can put an unvalidated value into confirmed data.
#[derive(Debug)]
pub struct TechnicalDataGuard {
    entry: TechnicalDataEntry,
    /// Indicator keys actually computed (see `ComputedIndicators`).
    computed: ComputedIndicators,
}

impl TechnicalDataGuard {
    /// Record a computed indicator reading.
    ///
    /// A non-finite value means the computation did not produce a reading, so the
    /// previous one must not be left standing: both the value and its computed
    /// flag are cleared. Keeping a stale reading marked as computed would let an
    /// old figure enter rule evaluation as if it had just been recomputed (R1).
    fn record_computed(
        computed: &mut ComputedIndicators,
        slot: &mut f64,
        key: &'static str,
        value: f64,
    ) {
        if value.is_finite() {
            *slot = value;
            computed.insert(key);
        } else {
            *slot = 0.0;
            computed.remove(key);
        }
    }

    fn finite_or_zero(value: f64) -> f64 {
        if value.is_finite() {
            value
        } else {
            0.0
        }
    }

    fn finite_option(value: f64) -> Option<f64> {
        value.is_finite().then_some(value)
    }

    fn score_or_zero(value: f64) -> f64 {
        Self::finite_or_zero(value).clamp(-MAX_ABS_INDICATOR_SCORE, MAX_ABS_INDICATOR_SCORE)
    }

    fn score_option(value: f64) -> Option<f64> {
        value
            .is_finite()
            .then_some(value.clamp(-MAX_ABS_INDICATOR_SCORE, MAX_ABS_INDICATOR_SCORE))
    }

    /// Creates a new guard-wrapped entry with default field values
    pub fn new(ticker: String, date: String) -> Self {
        TechnicalDataGuard {
            computed: ComputedIndicators::new(),
            entry: TechnicalDataEntry {
                ticker,
                name: String::new(),
                date,
                datetime: None,
                timestamp: None,
                timezone: "UTC".to_string(), // default when timezone is not yet fetched
                currency: None,
                latest_observed_price: None,
                market_data_latest_time: None,
                market_data_latest_timestamp: None,
                analyzed_at: None,
                analyzed_at_timestamp: None,
                source_note: None,
                close: 0.0,
                previous_close: 0.0,
                price_diff: 0.0,
                price_diff_percent: 0.0,
                macd: 0.0,
                signal: 0.0,
                prev_macd: 0.0,
                prev_signal: 0.0,
                rsi: 0.0,
                ema_short: 0.0,
                ema_long: 0.0,
                sma_short: 0.0,
                sma_long: 0.0,
                bb_upper: 0.0,
                bb_lower: 0.0,
                bb_percent_b: 0.0,
                bb_bandwidth: 0.0,
                roc: None,
                adx: None,
                stochastics_k: None,
                stochastics_d: None,
                fibo_38_2: None,
                fibo_50_0: None,
                fibo_61_8: None,
                vwap: None,
                tenkan_sen: None,
                kijun_sen: None,
                ema_score: None,
                sma_score: None,
                adx_score: None,
                roc_score: None,
                stochastics_score: None,
                bollinger_score: None,
                fibonacci_score: None,
                vwap_score: None,
                ichimoku_score: None,
                signal_score: 0.0,
                latest_volume: None,
                avg_volume: None,
                volume_ratio: None,
            },
        }
    }

    // Setter methods
    pub fn set_name(&mut self, value: &str) {
        self.entry.name = value.to_string();
    }
    pub fn set_datetime(&mut self, value: &str) {
        self.entry.datetime = Some(value.to_string());
    }
    pub fn set_timestamp(&mut self, value: i64) {
        self.entry.timestamp = Some(value);
    }
    pub fn set_timezone(&mut self, value: &str) {
        self.entry.timezone = value.to_string();
    }
    pub fn set_latest_observed_price(&mut self, value: f64) {
        self.entry.latest_observed_price = Self::finite_option(value);
    }
    pub fn set_market_data_latest_time(&mut self, value: &str) {
        self.entry.market_data_latest_time = Some(value.to_string());
    }
    pub fn set_market_data_latest_timestamp(&mut self, value: i64) {
        self.entry.market_data_latest_timestamp = Some(value);
    }
    pub fn set_analyzed_at(&mut self, value: &str) {
        self.entry.analyzed_at = Some(value.to_string());
    }
    pub fn set_source_note(&mut self, value: &str) {
        self.entry.source_note = Some(value.to_string());
    }
    pub fn set_analyzed_at_timestamp(&mut self, value: i64) {
        self.entry.analyzed_at_timestamp = Some(value);
    }
    pub fn set_close(&mut self, value: f64) {
        self.entry.close = Self::finite_or_zero(value);
    }
    pub fn set_previous_close(&mut self, value: f64) {
        self.entry.previous_close = Self::finite_or_zero(value);
    }
    pub fn set_price_diff(&mut self, value: f64) {
        self.entry.price_diff = Self::finite_or_zero(value);
    }
    pub fn set_price_diff_percent(&mut self, value: f64) {
        self.entry.price_diff_percent = Self::finite_or_zero(value);
    }
    pub fn set_rsi(&mut self, value: f64) {
        Self::record_computed(&mut self.computed, &mut self.entry.rsi, "rsi", value);
    }
    pub fn set_macd(&mut self, value: f64) {
        Self::record_computed(&mut self.computed, &mut self.entry.macd, "macd", value);
    }
    pub fn set_signal(&mut self, value: f64) {
        Self::record_computed(
            &mut self.computed,
            &mut self.entry.signal,
            "macd_signal",
            value,
        );
    }
    pub fn set_prev_macd(&mut self, value: f64) {
        self.entry.prev_macd = Self::finite_or_zero(value);
    }
    pub fn set_prev_signal(&mut self, value: f64) {
        self.entry.prev_signal = Self::finite_or_zero(value);
    }
    pub fn set_signal_score(&mut self, value: f64) {
        self.entry.signal_score = Self::score_or_zero(value);
    }
    pub fn set_ema_short(&mut self, value: f64) {
        Self::record_computed(
            &mut self.computed,
            &mut self.entry.ema_short,
            "ema_s",
            value,
        );
    }
    pub fn set_ema_long(&mut self, value: f64) {
        Self::record_computed(&mut self.computed, &mut self.entry.ema_long, "ema_l", value);
    }
    pub fn set_ema_score(&mut self, value: f64) {
        self.entry.ema_score = Self::score_option(value);
    }
    pub fn set_sma_short(&mut self, value: f64) {
        Self::record_computed(
            &mut self.computed,
            &mut self.entry.sma_short,
            "sma_s",
            value,
        );
    }
    pub fn set_sma_long(&mut self, value: f64) {
        Self::record_computed(&mut self.computed, &mut self.entry.sma_long, "sma_l", value);
    }
    pub fn set_sma_score(&mut self, value: f64) {
        self.entry.sma_score = Self::score_option(value);
    }
    pub fn set_adx(&mut self, value: f64) {
        self.entry.adx = Self::finite_option(value);
    }
    pub fn set_adx_score(&mut self, value: f64) {
        self.entry.adx_score = Self::score_option(value);
    }
    pub fn set_roc(&mut self, value: f64) {
        self.entry.roc = Self::finite_option(value);
    }
    pub fn set_roc_score(&mut self, value: f64) {
        self.entry.roc_score = Self::score_option(value);
    }
    pub fn set_stochastics_k(&mut self, value: f64) {
        self.entry.stochastics_k = Self::finite_option(value);
    }
    pub fn set_stochastics_d(&mut self, value: f64) {
        self.entry.stochastics_d = Self::finite_option(value);
    }
    pub fn set_stochastics_score(&mut self, value: f64) {
        self.entry.stochastics_score = Self::score_option(value);
    }
    pub fn set_bb_upper(&mut self, value: f64) {
        Self::record_computed(&mut self.computed, &mut self.entry.bb_upper, "bb_u", value);
    }
    pub fn set_bb_lower(&mut self, value: f64) {
        Self::record_computed(&mut self.computed, &mut self.entry.bb_lower, "bb_l", value);
    }
    pub fn set_bb_percent_b(&mut self, value: f64) {
        Self::record_computed(
            &mut self.computed,
            &mut self.entry.bb_percent_b,
            "pct_b",
            value,
        );
    }
    pub fn set_bb_bandwidth(&mut self, value: f64) {
        self.entry.bb_bandwidth = Self::finite_or_zero(value);
    }
    pub fn set_bollinger_score(&mut self, value: f64) {
        self.entry.bollinger_score = Self::score_option(value);
    }
    pub fn set_fibo_38_2(&mut self, value: f64) {
        self.entry.fibo_38_2 = Self::finite_option(value);
    }
    pub fn set_fibo_50_0(&mut self, value: f64) {
        self.entry.fibo_50_0 = Self::finite_option(value);
    }
    pub fn set_fibo_61_8(&mut self, value: f64) {
        self.entry.fibo_61_8 = Self::finite_option(value);
    }
    pub fn set_fibonacci_score(&mut self, value: f64) {
        self.entry.fibonacci_score = Self::score_option(value);
    }
    pub fn set_vwap(&mut self, value: f64) {
        self.entry.vwap = Self::finite_option(value);
    }
    pub fn set_vwap_score(&mut self, value: f64) {
        self.entry.vwap_score = Self::score_option(value);
    }
    pub fn set_tenkan_sen(&mut self, value: f64) {
        self.entry.tenkan_sen = Self::finite_option(value);
    }
    pub fn set_kijun_sen(&mut self, value: f64) {
        self.entry.kijun_sen = Self::finite_option(value);
    }
    pub fn set_ichimoku_score(&mut self, value: f64) {
        self.entry.ichimoku_score = Self::score_option(value);
    }
    pub fn set_latest_volume(&mut self, value: f64) {
        self.entry.latest_volume = if value.is_finite() && value >= 0.0 {
            Some(value)
        } else {
            None
        };
    }
    pub fn set_avg_volume(&mut self, value: f64) {
        self.entry.avg_volume = if value.is_finite() && value > 0.0 {
            Some(value)
        } else {
            None
        };
    }
    pub fn set_volume_ratio(&mut self, value: f64) {
        self.entry.volume_ratio = if value.is_finite() && value >= 0.0 {
            Some(value)
        } else {
            None
        };
    }

    // Getter methods
    pub fn get_name(&self) -> &str {
        &self.entry.name
    }
    pub fn get_datetime(&self) -> Option<&str> {
        self.entry.datetime.as_deref()
    }
    pub fn get_bar_time(&self) -> Option<&str> {
        self.entry.datetime.as_deref()
    }
    pub fn get_timestamp(&self) -> Option<i64> {
        self.entry.timestamp
    }
    pub fn get_bar_timestamp(&self) -> Option<i64> {
        self.entry.timestamp
    }
    /// The currency the prices are quoted in, when the provider reported one.
    /// Confirmed data carries this so a money figure written in another
    /// currency is refused even when the fundamental block is absent.
    pub fn get_currency(&self) -> Option<&str> {
        self.entry.currency.as_deref()
    }
    pub fn set_currency(&mut self, value: &str) {
        if !value.trim().is_empty() {
            self.entry.currency = Some(value.trim().to_string());
        }
    }
    pub fn get_timezone(&self) -> &str {
        &self.entry.timezone
    }
    pub fn get_latest_observed_price(&self) -> Option<f64> {
        self.entry.latest_observed_price
    }
    pub fn get_market_data_latest_time(&self) -> Option<&str> {
        self.entry.market_data_latest_time.as_deref()
    }
    pub fn get_source_note(&self) -> Option<&str> {
        self.entry.source_note.as_deref()
    }
    pub fn get_market_data_latest_timestamp(&self) -> Option<i64> {
        self.entry.market_data_latest_timestamp
    }
    pub fn get_analyzed_at(&self) -> Option<&str> {
        self.entry.analyzed_at.as_deref()
    }
    pub fn get_analyzed_at_timestamp(&self) -> Option<i64> {
        self.entry.analyzed_at_timestamp
    }
    pub fn get_ticker(&self) -> &str {
        &self.entry.ticker
    }
    pub fn get_date(&self) -> &str {
        &self.entry.date
    }
    pub fn get_close(&self) -> f64 {
        self.entry.close
    }
    pub fn get_previous_close(&self) -> f64 {
        self.entry.previous_close
    }
    pub fn get_price_diff(&self) -> f64 {
        self.entry.price_diff
    }
    pub fn get_price_diff_percent(&self) -> f64 {
        self.entry.price_diff_percent
    }
    /// Whether `key` (a rule/alert indicator key) was actually computed.
    /// A key absent here must never be evaluated as if it held a real value.
    pub fn is_computed(&self, key: &str) -> bool {
        self.computed.contains(key)
    }

    /// The computed value for `key`, or `None` when it was never computed or the
    /// computation failed. This is the accessor rule evaluation must use: a plain
    /// getter cannot tell a missing indicator from a legitimate 0.0.
    pub fn computed_indicator(&self, key: &str) -> Option<f64> {
        if !self.computed.contains(key) {
            return None;
        }
        let v = match key {
            "rsi" => self.entry.rsi,
            "macd" => self.entry.macd,
            "macd_signal" => self.entry.signal,
            "ema_s" => self.entry.ema_short,
            "ema_l" => self.entry.ema_long,
            "sma_s" => self.entry.sma_short,
            "sma_l" => self.entry.sma_long,
            "bb_u" => self.entry.bb_upper,
            "bb_l" => self.entry.bb_lower,
            "pct_b" => self.entry.bb_percent_b,
            _ => return None,
        };
        v.is_finite().then_some(v)
    }

    pub fn get_rsi(&self) -> f64 {
        self.entry.rsi
    }
    pub fn get_macd(&self) -> f64 {
        self.entry.macd
    }
    pub fn get_signal(&self) -> f64 {
        self.entry.signal
    }
    pub fn get_prev_macd(&self) -> f64 {
        self.entry.prev_macd
    }
    pub fn get_prev_signal(&self) -> f64 {
        self.entry.prev_signal
    }
    pub fn get_signal_score(&self) -> f64 {
        self.entry.signal_score
    }

    pub fn get_signal_strength(&self) -> SignalStrength {
        SignalStrength::from_f64(self.entry.signal_score).unwrap_or(SignalStrength::Neutral)
    }
    pub fn get_ema_short(&self) -> f64 {
        self.entry.ema_short
    }
    pub fn get_ema_long(&self) -> f64 {
        self.entry.ema_long
    }
    pub fn get_ema_score(&self) -> Option<f64> {
        self.entry.ema_score
    }
    pub fn get_sma_short(&self) -> f64 {
        self.entry.sma_short
    }
    pub fn get_sma_long(&self) -> f64 {
        self.entry.sma_long
    }
    pub fn get_sma_score(&self) -> Option<f64> {
        self.entry.sma_score
    }
    pub fn get_adx(&self) -> Option<f64> {
        self.entry.adx
    }
    pub fn get_adx_score(&self) -> Option<f64> {
        self.entry.adx_score
    }
    pub fn get_roc(&self) -> Option<f64> {
        self.entry.roc
    }
    pub fn get_roc_score(&self) -> Option<f64> {
        self.entry.roc_score
    }
    pub fn get_stochastics_k(&self) -> Option<f64> {
        self.entry.stochastics_k
    }
    pub fn get_stochastics_d(&self) -> Option<f64> {
        self.entry.stochastics_d
    }
    pub fn get_stochastics_score(&self) -> Option<f64> {
        self.entry.stochastics_score
    }
    pub fn get_bb_upper(&self) -> f64 {
        self.entry.bb_upper
    }
    pub fn get_bb_lower(&self) -> f64 {
        self.entry.bb_lower
    }
    pub fn get_bb_percent_b(&self) -> f64 {
        self.entry.bb_percent_b
    }
    pub fn get_bb_bandwidth(&self) -> f64 {
        self.entry.bb_bandwidth
    }
    pub fn get_bollinger_score(&self) -> Option<f64> {
        self.entry.bollinger_score
    }
    pub fn get_fibo_38_2(&self) -> Option<f64> {
        self.entry.fibo_38_2
    }
    pub fn get_fibo_50_0(&self) -> Option<f64> {
        self.entry.fibo_50_0
    }
    pub fn get_fibo_61_8(&self) -> Option<f64> {
        self.entry.fibo_61_8
    }
    pub fn get_fibonacci_score(&self) -> Option<f64> {
        self.entry.fibonacci_score
    }
    pub fn get_vwap(&self) -> Option<f64> {
        self.entry.vwap
    }
    pub fn get_vwap_score(&self) -> Option<f64> {
        self.entry.vwap_score
    }
    pub fn get_tenkan_sen(&self) -> Option<f64> {
        self.entry.tenkan_sen
    }
    pub fn get_kijun_sen(&self) -> Option<f64> {
        self.entry.kijun_sen
    }
    pub fn get_ichimoku_score(&self) -> Option<f64> {
        self.entry.ichimoku_score
    }
    pub fn get_latest_volume(&self) -> Option<f64> {
        self.entry.latest_volume
    }
    pub fn get_avg_volume(&self) -> Option<f64> {
        self.entry.avg_volume
    }
    pub fn get_volume_ratio(&self) -> Option<f64> {
        self.entry.volume_ratio
    }
}

#[cfg(test)]
mod tests {
    use super::TechnicalDataGuard;

    #[test]
    fn guard_does_not_store_non_finite_optional_values() {
        let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-01-01".to_string());

        guard.set_adx(f64::NAN);
        guard.set_roc(f64::INFINITY);
        guard.set_ema_score(f64::NEG_INFINITY);

        assert!(guard.get_adx().is_none());
        assert!(guard.get_roc().is_none());
        assert!(guard.get_ema_score().is_none());
    }

    #[test]
    fn guard_sanitizes_non_finite_required_values() {
        let mut guard = TechnicalDataGuard::new("T".to_string(), "2026-01-01".to_string());

        guard.set_close(f64::NAN);
        guard.set_signal_score(99.0);

        assert_eq!(guard.get_close(), 0.0);
        assert_eq!(guard.get_signal_score(), 2.0);
    }
}
#[cfg(test)]
mod recompute_clears_stale_readings {
    //! R1: "computed" must describe the current bar, not a previous success. A
    //! recomputation that fails has to clear the reading it replaces, otherwise a
    //! stale figure keeps entering rule evaluation as if it were fresh.
    use super::TechnicalDataGuard;

    fn guard() -> TechnicalDataGuard {
        TechnicalDataGuard::new("TEST".to_string(), "2026-09-08".to_string())
    }

    #[test]
    fn a_failed_recomputation_clears_the_previous_reading() {
        type Setter = (fn(&mut TechnicalDataGuard, f64), &'static str);
        let setters: [Setter; 10] = [
            (TechnicalDataGuard::set_rsi, "rsi"),
            (TechnicalDataGuard::set_macd, "macd"),
            (TechnicalDataGuard::set_signal, "macd_signal"),
            (TechnicalDataGuard::set_ema_short, "ema_s"),
            (TechnicalDataGuard::set_ema_long, "ema_l"),
            (TechnicalDataGuard::set_sma_short, "sma_s"),
            (TechnicalDataGuard::set_sma_long, "sma_l"),
            (TechnicalDataGuard::set_bb_upper, "bb_u"),
            (TechnicalDataGuard::set_bb_lower, "bb_l"),
            (TechnicalDataGuard::set_bb_percent_b, "pct_b"),
        ];
        for (set, key) in setters {
            let mut g = guard();
            set(&mut g, 42.0);
            assert_eq!(g.computed_indicator(key), Some(42.0), "{key}: first value");

            set(&mut g, f64::NAN);
            assert_eq!(
                g.computed_indicator(key),
                None,
                "{key}: a failed recomputation must not leave the old reading"
            );
            assert!(!g.is_computed(key), "{key}: the flag must be cleared too");
        }
    }

    #[test]
    fn a_never_computed_indicator_is_not_a_zero_reading() {
        let g = guard();
        for key in [
            "rsi",
            "macd",
            "macd_signal",
            "ema_s",
            "ema_l",
            "sma_s",
            "sma_l",
            "bb_u",
            "bb_l",
            "pct_b",
        ] {
            assert_eq!(g.computed_indicator(key), None, "{key}");
        }
    }

    #[test]
    fn a_successful_recomputation_replaces_the_reading() {
        let mut g = guard();
        g.set_rsi(30.0);
        g.set_rsi(70.0);
        assert_eq!(g.computed_indicator("rsi"), Some(70.0));
    }
}
