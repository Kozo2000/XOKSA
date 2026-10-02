//! Confirmed-data reference used by the LLM output-integrity guard (R3).
//!
//! # Why this is not built from the prompt
//!
//! An earlier version reconstructed the reference by parsing the prompt text.
//! That cannot work: the prompt also carries the conversation, the user's own
//! turns and other models' opinions, and a label written by any of them is
//! indistinguishable from one the engine wrote — a number a user typed would
//! become "confirmed". The reference is therefore built **only** from the
//! structures that own confirmed values (`TechnicalDataGuard`, `FundamentalData`)
//! and handed to the checker. No string is trusted as a source.
//!
//! # What a verified citation means
//!
//! A number passes only when the claim matches a confirmed fact in every respect
//! that can change its meaning:
//!
//! * **role** — a number is first read as an observation, a period parameter
//!   (`RSI(14)`) or a date (`2026-09-08`); only observations are claims about a
//!   value, and only those are verified;
//! * **symbol** — bound per number to the instrument named nearest before it, so
//!   "AAPL is 168.8 and MSFT is 400" attributes each number to its own issuer;
//! * **indicator** — bound per number to the indicator nearest to it, so
//!   "RSI is 4.68 and MACD is 45.2" cannot pass by having both numbers present;
//! * **unit, currency and magnitude** — 168.80 円 and 168.80 万円 are different
//!   amounts, and a JPY figure is not a USD figure;
//! * **point in time** — a technical value stated for another bar year, or a
//!   fundamental value stated for another fiscal year, is not this one;
//! * **sign** — +4.6815 and −4.6815 are different values;
//! * **precision** — display rounding is accepted at the magnitude it was written
//!   in (13.7兆円 for a confirmed 13.704兆円), a changed value is not (13.8兆円).

use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

// ─── Units ───────────────────────────────────────────────────────────────────

/// The currency a money figure is expressed in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Currency {
    /// Code as reported by the provider ("JPY", "USD").
    Known(String),
    /// Not reported. Unknown is left unknown: a currency is never inferred.
    Unknown,
}

impl Currency {
    fn matches(&self, written: Option<&str>) -> bool {
        match (self, written) {
            // Nothing written: the reader takes the instrument's own currency.
            (_, None) => true,
            (Currency::Known(k), Some(w)) => k.eq_ignore_ascii_case(w),
            // The provider did not report a currency (the labelled Stooq
            // fallback does not carry one). An unknown cannot contradict what
            // was written, and refusing would assert knowledge the engine does
            // not have — so the currency is not checked here. The value itself
            // still is. Stated as a limitation rather than assumed away.
            (Currency::Unknown, Some(_)) => true,
        }
    }
}

/// What a number means. Identical digits are different facts when the kind
/// differs, so the kind is part of the identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Money,
    Percent,
    /// A bounded reading with no unit (RSI, %K, MACD).
    Index,
    Score,
    Ratio,
    Count,
}

/// What a number is doing in the sentence. Only an observation is a claim about
/// a confirmed value; the other two are how an indicator or a date is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// A stated value — the only role that is verified.
    Observation,
    /// A window length written as part of an indicator's name: `RSI(14)`,
    /// `14-day EMA`, `期間: 25`.
    PeriodParameter,
    /// Part of a date or a stated year: `2026-09-08`, `2019年`, `in 2019`.
    Date,
}

/// A number as the model wrote it, with everything the writing says about it.
#[derive(Debug, Clone)]
pub struct WrittenNumber {
    /// Value after applying the written magnitude (万/億/兆, thousand/million…).
    pub value: f64,
    /// Decimal places actually written, so display rounding can be allowed.
    pub decimals: usize,
    /// Magnitude the number was written in (1e12 for 13.7兆円). Rounding is
    /// judged at this scale: 13.7兆 stands for anything within ±0.05兆.
    pub magnitude: f64,
    /// Kind implied by the writing; `None` when nothing was written and the
    /// indicator named in the sentence decides.
    pub kind: Option<Kind>,
    /// Currency written next to the number, if any.
    pub currency: Option<String>,
    /// What the number is doing in the sentence.
    pub role: Role,
    /// The token carried a trailing suffix the reader does not know. Such a
    /// figure is never confirmed: dropping the suffix and matching the bare
    /// digits would accept `$168.8x` as `$168.8`.
    pub unknown_suffix: bool,
}

// ─── Facts ───────────────────────────────────────────────────────────────────

/// One confirmed value.
#[derive(Debug, Clone)]
pub struct ConfirmedFact {
    /// Canonical key, shared with the rule evaluator's vocabulary.
    pub key: &'static str,
    /// Value in the base unit (money: whole currency units, not 万 or 億).
    pub value: f64,
    pub kind: Kind,
    /// The year this particular value belongs to — the bar's year for a
    /// technical reading, the fiscal year for a fundamental one. A technical
    /// value and a fundamental value in the same answer are therefore checked
    /// against their own periods, not against a single shared one.
    pub as_of_year: Option<i32>,
    /// The window this value was computed over, where the indicator has one and
    /// the answer can name it (`The 20-day EMA`). Two legs of the same indicator
    /// differ only by this, so without it a sentence that states the period could
    /// still be matched against the other leg.
    pub window: Option<usize>,
}

/// Everything confirmed about one instrument, with its attribution.
#[derive(Debug, Clone)]
pub struct SymbolFacts {
    pub symbol: String,
    pub name: String,
    pub currency: Currency,
    /// Calendar year of the bar the indicators were computed on, when known.
    pub as_of_year: Option<i32>,
    /// Words that name the timeframe these values were computed on, when the
    /// caller loaded several timeframes of the same instrument (the Web
    /// multi-timeframe analysis). Empty when there is only one.
    timeframe_terms: Vec<String>,
    facts: Vec<ConfirmedFact>,
    /// Conditions the engine itself stated alongside the values — an alert's
    /// `score <= 5`. A condition is not a reading of the indicator, so it is kept
    /// apart from `facts`; but the engine wrote it, so restating it is a citation.
    /// The operator is kept with the value: a sentence that flips the direction or
    /// drops the equality is stating a different rule.
    bounds: Vec<(String, Comparison, f64)>,
}

/// The confirmed data available to a request — one entry per loaded instrument
/// (per instrument *and* timeframe where several timeframes are loaded).
#[derive(Debug, Clone, Default)]
pub struct ConfirmedFactSet {
    symbols: Vec<SymbolFacts>,
}

/// Why a numeric claim failed. Each variant names a distinct defect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimVerdict {
    Verified,
    /// The sentence names an indicator whose confirmed value is not this number.
    ValueDoesNotMatchIndicator,
    /// The value exists, but under a different indicator than the one claimed.
    AttributedToWrongIndicator,
    /// The sentence states a reading for an indicator that has no confirmed
    /// value at all — it was never computed, or is not part of this analysis.
    IndicatorNotComputed,
    /// Written in a unit, currency or magnitude the confirmed value does not have.
    UnitMismatch,
    /// Dated to a period other than the one the confirmed value belongs to.
    PeriodMismatch,
    /// Attributed to an instrument that is not loaded.
    ForeignSymbol,
    /// Written as a condition on an indicator the engine stated a rule for, but
    /// not the rule it stated — a different direction, equality or threshold.
    ConditionDoesNotMatchRule,
    /// The sentence attributes a figure written in a unit or currency to an
    /// instrument that has no such confirmed value — including a value that
    /// belongs to a different instrument in the same answer.
    ValueNotConfirmedForSymbol,
    /// No confirmed fact corresponds to this number at all, and the sentence
    /// does not present it as a reading of a known indicator. Not reported: an
    /// ordinary number ("3 reasons") is not a claim about confirmed data.
    NotInConfirmedData,
}

impl ClaimVerdict {
    pub fn is_verified(&self) -> bool {
        matches!(self, ClaimVerdict::Verified)
    }

    /// Whether this verdict is worth reporting. `NotInConfirmedData` is not: the
    /// number was never presented as a confirmed reading.
    pub fn is_reportable(&self) -> bool {
        !matches!(
            self,
            ClaimVerdict::Verified | ClaimVerdict::NotInConfirmedData
        )
    }

    pub fn reason_ja(&self) -> &'static str {
        match self {
            ClaimVerdict::Verified => "検証済み",
            ClaimVerdict::ValueDoesNotMatchIndicator => "その指標の確定値と一致しない",
            ClaimVerdict::AttributedToWrongIndicator => "別の指標の値の転用",
            ClaimVerdict::IndicatorNotComputed => "その指標は算出されていない",
            ClaimVerdict::UnitMismatch => "単位・通貨・桁が確定値と異なる",
            ClaimVerdict::PeriodMismatch => "確定値と異なる時点の主張",
            ClaimVerdict::ForeignSymbol => "読み込んでいない銘柄の値",
            ClaimVerdict::ConditionDoesNotMatchRule => "エンジンが述べた条件と一致しない",
            ClaimVerdict::ValueNotConfirmedForSymbol => "この銘柄の確定値に無い数値",
            ClaimVerdict::NotInConfirmedData => "確定データに対応が無い",
        }
    }

    /// Order used to pick the most informative failure when several instruments
    /// or facts were candidates. A more specific defect outranks a vaguer one.
    fn severity(&self) -> u8 {
        match self {
            ClaimVerdict::Verified => 0,
            ClaimVerdict::NotInConfirmedData => 1,
            ClaimVerdict::ValueNotConfirmedForSymbol => 2,
            ClaimVerdict::ValueDoesNotMatchIndicator => 3,
            ClaimVerdict::IndicatorNotComputed => 4,
            ClaimVerdict::PeriodMismatch => 5,
            ClaimVerdict::UnitMismatch => 6,
            ClaimVerdict::AttributedToWrongIndicator => 7,
            ClaimVerdict::ConditionDoesNotMatchRule => 8,
            ClaimVerdict::ForeignSymbol => 9,
        }
    }
}

// ─── Vocabulary (defined once, both languages) ───────────────────────────────

/// Indicator terms as a model writes them. One row per confirmed key, holding
/// the Japanese and English members of the same indicator, so an indicator is
/// never recognised in one language only. Longer terms come first within a row
/// and more specific rows before their generic prefix.
///
/// Every key here must have a source in `SymbolFacts::from_sources`; otherwise a
/// legitimate citation of that indicator would be reported as not computed.
static INDICATOR_TERMS: &[(&str, &[&str])] = &[
    (
        "macd_signal",
        &[
            "macdシグナル",
            "シグナル線",
            "シグナル",
            "macd signal",
            "signal line",
            // A bare "signal" is an ordinary English word ("a bullish signal"),
            // so it is only safe now that a figure binds to a name written in
            // its own claim: a number beside an unrelated "signal" no longer
            // borrows this key. Without that, do not add this term.
            "signal",
        ],
    ),
    ("macd", &["macd", "移動平均収束拡散"]),
    ("rsi", &["rsi", "相対力指数", "relative strength index"]),
    (
        "prev_close",
        &["前営業日終値", "前日終値", "previous close", "prev close"],
    ),
    (
        "latest_price",
        &["最新取得価格", "最新価格", "latest price"],
    ),
    (
        "close",
        &[
            "終値",
            "現在値",
            "現在価格",
            "株価",
            "closing price",
            "close",
            "price",
        ],
    ),
    (
        "ema_s",
        &["短期ema", "短期指数平滑", "short ema", "short-term ema"],
    ),
    (
        "ema_l",
        &["長期ema", "長期指数平滑", "long ema", "long-term ema"],
    ),
    ("sma_s", &["短期sma", "短期単純移動平均", "short sma"]),
    ("sma_l", &["長期sma", "長期単純移動平均", "long sma"]),
    ("bb_u", &["バンド上限", "上限", "upper band"]),
    ("bb_l", &["バンド下限", "下限", "lower band"]),
    ("pct_b", &["%b", "パーセントb", "percent b"]),
    (
        "bandwidth",
        &["帯幅", "バンド幅", "bandwidth", "band width"],
    ),
    (
        "vwap",
        &["vwap", "出来高加重", "volume weighted average price"],
    ),
    (
        "adx",
        &["adx", "平均方向性指数", "average directional index"],
    ),
    ("roc", &["roc", "変化率", "rate of change"]),
    ("stoch_d", &["%d", "ストキャス%d", "stochastic %d"]),
    ("stoch_k", &["ストキャス", "stochastic", "%k"]),
    (
        "tenkan",
        &["転換線", "転換ライン", "tenkan", "conversion line"],
    ),
    (
        "kijun",
        &["基準線", "基準ライン", "kijun", "base line", "baseline"],
    ),
    (
        "fib_382",
        &["38.2%水準", "38.2%", "fib 38.2", "fibonacci 38.2"],
    ),
    (
        "fib_500",
        &["50.0%水準", "50.0%", "fib 50.0", "fibonacci 50.0"],
    ),
    (
        "fib_618",
        &["61.8%水準", "61.8%", "fib 61.8", "fibonacci 61.8"],
    ),
    ("score", &["総合スコア", "total score", "スコア", "score"]),
    ("avg_volume", &["平均出来高", "average volume"]),
    ("volume_ratio", &["出来高比", "volume ratio"]),
    ("volume", &["出来高", "volume"]),
    // The header writes one label with two figures — "前営業日比: -3.80 (-2.20%)"
    // — so the same words name both the amount and the percentage. Two keys
    // share the vocabulary, and the unit the model writes selects between them.
    ("price_diff", &PRICE_DIFF_TERMS),
    ("price_diff_pct", &PRICE_DIFF_TERMS),
    ("revenue", &["売上高", "売上", "revenue", "sales"]),
    ("operating_income", &["営業利益", "operating income"]),
    ("net_income", &["純利益", "当期純利益", "net income"]),
    ("eps", &["eps", "一株当たり利益", "earnings per share"]),
    (
        "bps",
        &[
            "bps",
            "一株当たり純資産",
            "一株当たりの純資産",
            "book value per share",
            "book value",
        ],
    ),
    ("equity", &["純資産", "自己資本", "equity"]),
    ("shares_outstanding", &["発行済株式", "shares outstanding"]),
    // The full labels come first so that longest-match-at-a-position picks them
    // over the bare "per" — "Dividend (per share)" is a dividend, not a PER (L4).
    (
        "lot_dividend",
        &[
            "配当（1単元あたり）",
            "配当(1単元あたり)",
            "dividend (per lot)",
            "dividend per lot",
            "1単元あたり配当",
        ],
    ),
    (
        "trading_unit",
        &[
            "最少単位株",
            "最小単位株",
            "単元株数",
            "単元株",
            "min. trading unit",
            "min trading unit",
            "trading unit",
        ],
    ),
    ("per", &["per", "株価収益率", "p/e"]),
    (
        "pbr",
        &["pbr", "株価純資産倍率", "price-to-book", "price to book"],
    ),
    ("roe", &["roe", "自己資本利益率", "return on equity"]),
    (
        "dividend",
        &[
            "配当（1株あたり）",
            "配当(1株あたり)",
            "dividend (per share)",
            "dividend per share",
            "1株あたり配当",
            "配当",
            "dividend",
        ],
    ),
    (
        "operating_margin",
        &["営業利益率", "operating margin", "operating profit margin"],
    ),
];

/// The change-versus-previous-bar label, in every timeframe wording the header
/// uses, plus the plainer forms a model writes.
static PRICE_DIFF_TERMS: [&str; 10] = [
    "前営業日比",
    "前足比",
    "前週比",
    "前月比",
    "前日比",
    "前日差",
    "change vs prev close",
    "change vs previous close",
    "change vs prev bar",
    "change",
];

/// Wording that does not distinguish the short from the long leg: such a
/// sentence may legitimately cite either, so both keys stay bound to the number.
static GENERIC_TERMS: &[(&str, &[&str])] = &[
    // Spelled out, these name no leg either, so they belong here rather than
    // beside the short/long rows: "the exponential moving average is 166.88"
    // may cite either leg, exactly as a bare "EMA" may.
    (
        "ema_s",
        &["ema", "指数平滑移動平均", "exponential moving average"],
    ),
    (
        "ema_l",
        &["ema", "指数平滑移動平均", "exponential moving average"],
    ),
    ("sma_s", &["sma", "単純移動平均", "simple moving average"]),
    ("sma_l", &["sma", "単純移動平均", "simple moving average"]),
    ("bb_u", &["ボリンジャー", "bollinger", "バンド", "band"]),
    ("bb_l", &["ボリンジャー", "bollinger", "バンド", "band"]),
];

/// Keys whose values come from the fundamental data. Used when a reload refreshes
/// only the technical side and the previously fetched fundamentals stay in place.
static FUNDAMENTAL_KEYS: &[&str] = &[
    "revenue",
    "operating_income",
    "net_income",
    "eps",
    "bps",
    "equity",
    "shares_outstanding",
    "dividend",
    "lot_dividend",
    "trading_unit",
    "per",
    "pbr",
    "roe",
    "operating_margin",
];

// ─── Number reading ──────────────────────────────────────────────────────────

/// A number with anything written around it that changes its meaning: a leading
/// currency symbol and sign, a trailing magnitude, and a trailing unit or
/// currency word. Magnitude and unit are separate groups so
/// `13704 billion dollars` is read as both a magnitude and a currency.
/// A number with everything written around it that changes its meaning: a
/// leading currency symbol and sign, a trailing magnitude, and a trailing unit
/// or currency word. Magnitude and unit are separate groups so
/// `13704 billion dollars` is read as both a magnitude and a currency, and the
/// short suffixes are here so that `$168.8m` is read as the amount it is
/// rather than as `$168.8` with a letter left over.
static RE_WRITTEN_NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        // A sign can stand on either side of a currency symbol: -$2.50 and
        // $-2.50 are the same amount, and reading only one order made one of
        // them a positive number.
        r"(?i)(?P<sign_pre>[+\-\u{2212}\u{25B2}\u{25B3}])?\s*",
        r"(?P<cur_pre>[$¥€£]|USD|JPY|EUR|GBP)?\s*",
        r"(?P<sign>[+\-\u{2212}\u{25B2}\u{25B3}])?",
        r"(?P<digits>[0-9\u{FF10}-\u{FF19}][0-9\u{FF10}-\u{FF19},\u{FF0C}]*",
        r"(?:[.\u{FF0E}][0-9\u{FF10}-\u{FF19}]+)?)",
        r"\s*(?P<mag>兆|十億|百万|億|万|千|trillion|billion|million|thousand",
        // No boundary in front: a digit and a letter are both word characters,
        // so "8m" has none, and requiring one made `$168.8m` unreadable while
        // `$168.8 m` read fine. The boundary after is what keeps `168.8bar`
        // from being read as billions.
        r"|(?:bn|tn|mm|k|m|b)(?-u:\b))?",
        r"\s*(?P<unit>円|yen|jpy|usd|dollars|dollar|eur|euros|euro|gbp|pounds|pound|",
        r"ドル|米ドル|ユーロ|ポンド|株|shares|share|%|％|percent|pct)?"
    ))
    .expect("valid regex")
});

/// A ticker as it appears in text. Two shapes are recognised:
///
/// * **unambiguous** — the Japanese `9432.T` form and the dotted US form
///   (`BRK.B`); nothing else is written that way, so these are always symbols;
/// * **bare uppercase** (`AAPL`) — indistinguishable in shape from an indicator
///   name (`RSI`), an English word (`I`, `IT`) or an ordinary acronym (`AI`), so
///   it is only read as a symbol after the checks in `is_symbol_token`.
static RE_TICKER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?-u:\b)(?:[0-9]{4}\.[A-Za-z]|[A-Z][A-Z0-9]{0,4}(?:\.[A-Z]{1,2})?)(?-u:\b)")
        .expect("valid regex")
});

/// Currency codes the engine itself writes; ticker-shaped, never a symbol here.
static CURRENCY_CODES: &[&str] = &[
    "JPY", "USD", "EUR", "GBP", "CNY", "KRW", "HKD", "AUD", "CAD",
];

/// Uppercase tokens that are ordinary English or ordinary acronyms, never a
/// ticker. An enumeration, not a heuristic: these are grammar words and widely
/// used abbreviations that a model writes in normal prose, and reading one of
/// them as an instrument would drop a legitimate sentence.
static NOT_A_TICKER: &[&str] = &[
    "I", "A", "AN", "AS", "AT", "BE", "BY", "DO", "GO", "HE", "IF", "IN", "IS", "IT", "ME", "MY",
    "NO", "OF", "OK", "ON", "OR", "SO", "TO", "UP", "US", "WE", "ALL", "AND", "ARE", "BUT", "CAN",
    "DID", "FOR", "HAS", "HAD", "ITS", "MAY", "NEW", "NOT", "NOW", "ONE", "OUR", "OUT", "SEE",
    "THE", "TWO", "USE", "WAS", "WHY", "YES", "YOU", "AI", "API", "CEO", "CFO", "ESG", "ETF",
    "FAQ", "GDP", "IPO", "LLM", "NOTE", "OTC", "PDF", "TBD", "URL", "USA", "YOY", "QOQ",
];

/// Calendar dates, in either language. A number inside one of these is not a
/// reading of an indicator.
/// Calendar dates, in either language. A number inside one of these is not a
/// reading of an indicator. The English month-name forms are here so that one
/// date reads the same however it is spelled: `2026-09-08`, `2026年9月8日` and
/// `September 8, 2026` must all stamp the observation rather than date the claim.
/// Calendar dates, in either language. A number inside one of these is not a
/// reading of an indicator. The English month-name forms are here so that one
/// date reads the same however it is spelled: `2026-09-08`, `2026年9月8日` and
/// `September 8, 2026` must all stamp the observation rather than date the claim.
///
/// Month names are the real ones, whole: an open-ended `mar[a-z]*` also spells
/// "margin" and `dec[a-z]*` spells "declined", and reading those as dates would
/// exempt the figure beside them from verification. Each name is anchored on
/// both sides and must sit in an actual date construction.
static RE_DATE_SPAN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)(?:19|20)[0-9]{2}\s*[-/]\s*[0-9]{1,2}\s*[-/]\s*[0-9]{1,2}",
        r"|(?:19|20)[0-9]{2}\s*年\s*[0-9]{1,2}\s*月(?:\s*[0-9]{1,2}\s*日)?",
        r"|(?-u:\b)[0-9]{1,2}\s*[-/]\s*[0-9]{1,2}\s*[-/]\s*(?:19|20)[0-9]{2}(?-u:\b)",
        r"|[0-9]{1,2}\s*月\s*[0-9]{1,2}\s*日",
        r"|(?-u:\b)(?:january|february|march|april|may|june|july|august|september|october|november|december|jan|feb|mar|apr|jun|jul|aug|sept|sep|oct|nov|dec)(?-u:\b)\.?\s*",
        r"[0-9]{1,2}(?:st|nd|rd|th)?(?:\s*,)?\s*(?:19|20)[0-9]{2}",
        r"|(?-u:\b)[0-9]{1,2}(?:st|nd|rd|th)?\s*",
        r"(?:january|february|march|april|may|june|july|august|september|october|november|december|jan|feb|mar|apr|jun|jul|aug|sept|sep|oct|nov|dec)(?-u:\b)\.?\s*(?:19|20)[0-9]{2}",
        // Without a year, "May" is left out: "the price may 5% higher" is
        // prose, and reading it as a date would exempt the figure entirely.
        r"|(?-u:\b)(?:january|february|march|april|june|july|august|september|october|november|december|jan|feb|mar|apr|jun|jul|aug|sept|sep|oct|nov|dec)(?-u:\b)\.?\s+",
        r"[0-9]{1,2}(?:st|nd|rd|th)?(?-u:\b)"
    ))
    .expect("valid regex")
});

/// A year written *as* a year: with 年/年度, as a fiscal year, or after a
/// date preposition. Kept separate from `RE_DATE_SPAN` because alternation is
/// leftmost-first: "As of 2026-09-08" must not be consumed by the preposition
/// branch before the full date is seen.
static RE_STATED_YEAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)(?:19|20)[0-9]{2}\s*(?:年度|年)",
        r"|fy\s*(?:19|20)[0-9]{2}",
        r"|(?:19|20)[0-9]{2}\s*年度",
        r"|(?:in|as of|since|until|by|during|for|through)\s+(?:19|20)[0-9]{2}(?-u:\b)"
    ))
    .expect("valid regex")
});

/// A year inside a date span, for the period comparison.
static RE_YEAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:19|20)[0-9]{2}").expect("valid regex"));

/// The indicator names a window can be attached to, and the units a window may
/// be written with. Defined once and used by both readers below: the one that
/// *excludes* a window from verification and the one that *uses* it to pick a
/// leg must recognise the same shapes, or a leg mix-up slips through whichever
/// spelling only one of them knows.
const WINDOWED_NAME: &str = concat!(
    r"(?:ema|sma|wma|rsi|adx|atr|roc|stoch[a-z]*|bb|bollinger|",
    r"ボリンジャー|移動平均|バンド|一目|moving average|band)"
);
const WINDOW_UNIT: &str = r"(?:\s*(?:日|本|期間|週|月|bars?|days?|periods?|weeks?|months?))?";
/// The same units as `WINDOW_UNIT`, for the English form that puts the window
/// in front of the name (`20-month EMA`). It is a separate string only because
/// that form requires the unit rather than allowing it — the list itself must
/// not diverge, which is why both readers take it from here.
const WINDOW_UNIT_EN: &str = r"(?:days?|periods?|bars?|weeks?|months?)";

/// A window length written as part of an indicator's name. The number here is a
/// parameter, not a reading: `RSI(14) is 45.2` states one value, not two.
///
/// The parenthesised form covers only what is actually a period specification —
/// whole numbers, optionally several of them (`MACD(12,26,9)`), optionally with a
/// unit of bars. Anything else inside the brackets is ordinary content and is
/// verified: `RSI (value: 99.99)` and `RSI（99.99）` state a reading.
static RE_PERIOD_PARAM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        concat!(
            r"(?i)(?:macd|{name})\s*[\(（]\s*",
            r"[0-9]{{1,4}}(?:\s*[,、，]\s*[0-9]{{1,4}})*{unit}\s*[\)）]",
            r"|[0-9]{{1,4}}{unit}\s*(?:の)?\s*{name}",
            r"|[0-9]{{1,4}}[- ]?{en_unit}\s+{name}",
            r"|(?:期間|パラメータ|period|length)\s*[:：=]?\s*[0-9]{{1,4}}",
            // "1株あたり" / "per share" names a basis, not a reading: the 1 is
            // part of the phrase.
            r"|1\s*(?:株|単元)\s*(?:あたり|当たり)"
        ),
        name = WINDOWED_NAME,
        unit = WINDOW_UNIT,
        en_unit = WINDOW_UNIT_EN
    ))
    .expect("valid regex")
});

/// An indicator named together with the window it was computed over, in either
/// language: `20-day EMA`, `20日EMA`, `EMA(20)`, `EMA(20 months)`, `EMA（20日）`.
///
/// The indicator name is captured too, because a window belongs to the
/// indicator it was written on: an `RSI(14)` in the same sentence must not
/// select a leg of the EMA beside it.
static RE_WINDOWED_INDICATOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        concat!(
            r"(?i)(?P<w1>[0-9]{{1,4}}){unit}\s*(?:の)?\s*(?P<n1>{name})",
            r"|(?P<w2>[0-9]{{1,4}})[- ]?{en_unit}\s+(?P<n2>{name})",
            r"|(?P<n3>{name})\s*[\(（]\s*(?P<w3>[0-9]{{1,4}}){unit}\s*[\)）]"
        ),
        name = WINDOWED_NAME,
        unit = WINDOW_UNIT,
        en_unit = WINDOW_UNIT_EN
    ))
    .expect("valid regex")
});

/// A comparison written between an indicator and a number. A stated threshold
/// is quotable only in this shape: "score <= 5 の条件が成立" restates the rule the
/// engine reported, while "score is 5" asserts a value the engine never produced.
/// A comparison direction, with whether the bound itself is included. A rule and
/// a sentence restating it must agree on both: `score <= 5` is not `score >= 5`,
/// and it is not `score < 5` either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Le,
    Lt,
    Ge,
    Gt,
}

impl Comparison {
    /// The engine's own operator codes (`rule_operator_catalog`). `cross_up` and
    /// `cross_down` are not comparisons against a value and yield `None`.
    pub fn from_op(op: &str) -> Option<Comparison> {
        match op.trim().to_ascii_lowercase().as_str() {
            "le" => Some(Comparison::Le),
            "lt" => Some(Comparison::Lt),
            "ge" => Some(Comparison::Ge),
            "gt" => Some(Comparison::Gt),
            _ => None,
        }
    }
}

/// A comparison written *before* the number (`score <= 5`, `at or below 5`).
/// Longest first, so `<=` is not read as `<` and "at or below" beats "below".
static COMPARISON_BEFORE: &[(&str, Comparison)] = &[
    ("<=", Comparison::Le),
    ("=<", Comparison::Le),
    ("≤", Comparison::Le),
    ("≦", Comparison::Le),
    (">=", Comparison::Ge),
    ("=>", Comparison::Ge),
    ("≥", Comparison::Ge),
    ("≧", Comparison::Ge),
    ("<", Comparison::Lt),
    ("＜", Comparison::Lt),
    (">", Comparison::Gt),
    ("＞", Comparison::Gt),
    ("at or below", Comparison::Le),
    ("at or above", Comparison::Ge),
    ("no higher than", Comparison::Le),
    ("no lower than", Comparison::Ge),
    ("less than", Comparison::Lt),
    ("greater than", Comparison::Gt),
    ("below", Comparison::Lt),
    ("under", Comparison::Lt),
    ("above", Comparison::Gt),
    ("over", Comparison::Gt),
];

/// A comparison written *after* the number, which is how Japanese states one
/// (`5以下`). Longest first for the same reason.
static COMPARISON_AFTER: &[(&str, Comparison)] = &[
    ("以下", Comparison::Le),
    ("以上", Comparison::Ge),
    ("未満", Comparison::Lt),
    ("を下回", Comparison::Lt),
    ("を上回", Comparison::Gt),
    ("より小さい", Comparison::Lt),
    ("より大きい", Comparison::Gt),
    ("超", Comparison::Gt),
];

/// The comparison a span states, if it states exactly one.
///
/// An ASCII word must stand as a word: `over` inside "overall" and `below`
/// inside a longer token are not comparisons, and reading them as one turned an
/// ordinary claim ("The score overall is 5.0") into a restatement of a rule.
fn comparison_in(span: &str) -> Option<(Comparison, usize)> {
    let low = span.to_lowercase();
    let mut found: Vec<(usize, usize, Comparison)> = Vec::new();
    for (token, cmp) in COMPARISON_BEFORE {
        for at in term_positions(&low, token) {
            found.push((at, token.len(), *cmp));
        }
    }
    // "below" sits inside "at or below" and starts later, so a plain
    // nearest-wins would read the loose form out of the inclusive one. A match
    // contained in another is part of that one, not a comparison of its own.
    let outer: Vec<&(usize, usize, Comparison)> = found
        .iter()
        .filter(|(p, l, _)| {
            !found
                .iter()
                .any(|(q, m, _)| (*q, *m) != (*p, *l) && *q <= *p && p + l <= q + m)
        })
        .collect();
    outer
        .iter()
        .max_by_key(|(p, _, _)| *p)
        .map(|(p, l, c)| (*c, p + l))
}

/// The comparison written immediately after a number, skipping spaces.
fn comparison_after(rest: &str) -> Option<Comparison> {
    let trimmed = rest.trim_start();
    COMPARISON_AFTER
        .iter()
        .find(|(token, _)| trimmed.starts_with(token))
        .map(|(_, cmp)| *cmp)
}

/// A Fibonacci retracement written as the level it names. The `38.2` in
/// "38.2%: 165.00" identifies the level; the price is the reading. Only the
/// canonical ratios count, and only where the writing marks them as a level, so
/// an ordinary percentage ("上昇率は50.0%です") stays an observation.
static RE_LEVEL_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)(?:fib(?:onacci)?|フィボナッチ)\s*[:：]?\s*",
        r"(?:23\.6|38\.2|50\.0|61\.8|78\.6)\s*%?",
        r"|(?:23\.6|38\.2|50\.0|61\.8|78\.6)\s*%\s*",
        r"(?:水準|ライン|の水準|level|line|retracement|[:：])"
    ))
    .expect("valid regex")
});

/// A bar size written as a timeframe: the number names the bar, not a reading.
/// A bar size written as a timeframe: the number names the bar, not a reading.
///
/// The ASCII forms are the intervals the engine actually analyses, enumerated
/// rather than "any number followed by m". `$21m` is twenty-one million, not a
/// twenty-one-minute bar, and reading it as a bar would exempt the figure from
/// verification altogether.
static RE_TIMEFRAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)[0-9]{1,4}\s*(?:分足|時間足|日足|週足|月足|営業日足)",
        r"|(?-u:\b)(?:1|5|15|30|60)\s*(?:mins?|minutes?|m)(?-u:\b)",
        r"|(?-u:\b)(?:1|2|4|6|8|12|24)\s*(?:hrs?|hours?|h)(?-u:\b)",
        r"(?:\s*(?:bar|bars|chart|candle|candles))?"
    ))
    .expect("valid regex")
});

/// Where one claim ends and the next begins inside a sentence. A sentence may
/// carry several claims — "日足は168.8円、1時間足は170.2円" / "revenue was X in
/// FY2025 and Y in FY2024" — and each qualifier (the instrument, the bar, the
/// year) belongs to the claim it was written in, whether it stands before the
/// figure (Japanese) or after it (English). Binding by clause reads both.
static RE_CLAUSE_BREAK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)[、，,;；]",
        r"|(?-u:\b)(?:and|while|whereas|but|though|whereas)(?-u:\b)"
    ))
    .expect("valid regex")
});

/// Every bar the engine can analyse, as a model writes it. Used to notice a bar
/// that was not analysed at all — a value cannot belong to one.
fn all_timeframe_terms() -> Vec<String> {
    let mut out = Vec::new();
    for mode in crate::config::AnalysisMode::ALL {
        out.push(mode.as_str().to_string());
        out.push(mode.bar_label_ja().to_string());
        out.push(mode.bar_label("en").to_string());
    }
    out
}

/// Where the numbers, dates, period specifications and tickers of a piece of
/// text are, settled once and shared by everything that reads it.
///
/// Order matters, and getting it wrong changes verdicts by notation alone: a
/// thousands separator inside a figure must not split a claim, a currency-marked
/// amount must not be read as a bar size, and the year inside a full date must
/// not date the claim the way a fiscal year does. Deciding all of this in one
/// place is what makes `13,704,000,000,000` and `13704000000000`, and the ISO,
/// Japanese and English spellings of one date, behave alike.
pub struct Scan {
    /// Digit runs, separators included, so a comma inside a figure is not a break.
    numbers: Vec<Range<usize>>,
    /// Calendar dates (`2026-09-08`, `2026年9月8日`).
    dates: Vec<Range<usize>>,
    /// Years written *as* years, with the year itself. Never overlapping a date:
    /// a full date stamps when something was observed, it does not name a period.
    years: Vec<(Range<usize>, i32)>,
    /// Window lengths and level identifiers — `RSI(14)`, `38.2%水準`.
    params: Vec<Range<usize>>,
    /// Bar sizes — `1時間足`, `60m`.
    timeframes: Vec<Range<usize>>,
    tickers: Vec<Range<usize>>,
    /// Windows the text states, each with the span of the indicator name it was
    /// written on, so it is applied to that indicator only.
    windows: Vec<(Range<usize>, usize)>,
}

impl Scan {
    /// Settle where everything is in `text`. Build this once for a sentence and
    /// hand it to every step that reads that sentence, so the claim-splitting and
    /// the attribution can never disagree about where a figure or a date is.
    pub fn of(text: &str) -> Scan {
        let dates = spans_of(&RE_DATE_SPAN, text);
        let years = RE_STATED_YEAR
            .find_iter(text)
            .filter(|m| !dates.iter().any(|d| m.start() < d.end && d.start < m.end()))
            .filter_map(|m| {
                let year = RE_YEAR.find(m.as_str())?.as_str().parse::<i32>().ok()?;
                Some((m.range(), year))
            })
            .collect();
        let mut params = spans_of(&RE_PERIOD_PARAM, text);
        params.extend(spans_of(&RE_LEVEL_ID, text));
        Scan {
            numbers: RE_WRITTEN_NUMBER
                .captures_iter(text)
                .filter_map(|c| c.name("digits").map(|m| m.range()))
                .collect(),
            dates,
            years,
            params,
            timeframes: spans_of(&RE_TIMEFRAME, text),
            tickers: spans_of(&RE_TICKER, text),
            windows: RE_WINDOWED_INDICATOR
                .captures_iter(text)
                .filter_map(|c| {
                    let (w, n) = c
                        .name("w1")
                        .zip(c.name("n1"))
                        .or_else(|| c.name("w2").zip(c.name("n2")))
                        .or_else(|| c.name("w3").zip(c.name("n3")))?;
                    Some((n.range(), w.as_str().parse::<usize>().ok()?))
                })
                .collect(),
        }
    }

    /// Stretches a clause break may not fall inside: a separator written *within*
    /// a figure, a date, a period specification or a ticker is punctuation of that
    /// token, not the end of a claim.
    fn protected(&self) -> Vec<Range<usize>> {
        let mut out = self.numbers.clone();
        out.extend(self.dates.iter().cloned());
        out.extend(self.params.iter().cloned());
        out.extend(self.tickers.iter().cloned());
        out
    }
}

/// The stretch of `sentence` that holds the claim `at` belongs to.
fn clause_bounds(sentence: &str, at: usize, scan: &Scan) -> Range<usize> {
    let protected = scan.protected();
    let mut start = 0usize;
    let mut end = sentence.len();
    for m in RE_CLAUSE_BREAK.find_iter(sentence) {
        if inside(&protected, m.start()) {
            continue;
        }
        if m.end() <= at {
            start = m.end();
        } else if m.start() > at {
            end = m.start();
            break;
        }
    }
    if start > end {
        return 0..sentence.len();
    }
    start..end
}

/// Pick the mention that qualifies a figure at `at`: one written in the same
/// clause, else — when the clause names none — the nearest one in the sentence.
///
/// The fallback is right for the qualifiers a sentence states once and carries
/// across its claims. "AAPL: RSI is 45.2, MACD is 2.39" names its instrument in
/// the first clause only, and the same holds for the bar and the year, so
/// refusing to look outside the clause would leave every later claim unqualified.
///
/// It is **not** right for the indicator: §1 binds a figure to "the one named in
/// **the claim**", and a number whose own claim names no indicator is a reading
/// of nothing the sentence said. Borrowing the neighbour's name let a false claim
/// pass on a true value ("RSI is 56.93 and the signal is 56.93") and refused the
/// true one beside it. `qualifier_in_clause` is that reading; see `bound_keys`.
fn qualifier_at<T: Copy>(
    marks: &[(usize, T)],
    sentence: &str,
    at: usize,
    scan: &Scan,
) -> Option<(usize, T)> {
    qualifier_impl(marks, sentence, at, scan, true)
}

/// `qualifier_at` without the sentence-wide fallback: the figure is qualified by
/// a mention written in its own claim, or by nothing.
fn qualifier_in_clause<T: Copy>(
    marks: &[(usize, T)],
    sentence: &str,
    at: usize,
    scan: &Scan,
) -> Option<(usize, T)> {
    qualifier_impl(marks, sentence, at, scan, false)
}

fn qualifier_impl<T: Copy>(
    marks: &[(usize, T)],
    sentence: &str,
    at: usize,
    scan: &Scan,
    fall_back_to_sentence: bool,
) -> Option<(usize, T)> {
    if marks.is_empty() {
        return None;
    }
    let clause = clause_bounds(sentence, at, scan);
    let in_clause: Vec<&(usize, T)> = marks
        .iter()
        .filter(|(p, _)| *p >= clause.start && *p < clause.end)
        .collect();
    let pick = |list: &[&(usize, T)]| -> Option<(usize, T)> {
        list.iter()
            .rev()
            .find(|(p, _)| *p <= at)
            .or_else(|| list.iter().find(|(p, _)| *p > at))
            .map(|(p, v)| (*p, *v))
    };
    if let Some(found) = pick(&in_clause) {
        return Some(found);
    }
    if !fall_back_to_sentence {
        return None;
    }
    let all: Vec<&(usize, T)> = marks.iter().collect();
    pick(&all)
}

/// Where `term` occurs in `low` (already lowercased) *as a word*.
///
/// An all-ASCII term must not be part of a longer word: "ema" is inside
/// "remains" and "per" is inside "period", and binding a figure to an indicator
/// found that way attributes it to something the sentence never named. A single
/// trailing `s` is allowed, so ordinary plurals ("prices", "bands") still count.
/// Japanese terms are matched as-is — they need no boundary and have none.
pub(crate) fn term_positions(low: &str, term: &str) -> Vec<usize> {
    let t = term.to_lowercase();
    let mut out = Vec::new();
    if t.is_empty() {
        return out;
    }
    let ascii = t.is_ascii();
    let mut from = 0usize;
    while let Some(i) = low[from..].find(&t) {
        let at = from + i;
        let ok = !ascii || {
            // The boundary is against *letters*, not digits: a Japanese sentence
            // writes "VWAP168.96" with nothing between the name and its value, so
            // requiring a non-alphanumeric there would lose the indicator.
            let before = low[..at]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_ascii_alphabetic());
            let rest = &low[at + t.len()..];
            let after = match rest.chars().next() {
                None => true,
                Some(c) if !c.is_ascii_alphabetic() => true,
                // An English plural or possessive is the same word.
                Some('s') => rest[1..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_ascii_alphabetic()),
                Some(_) => false,
            };
            before && after
        };
        if ok {
            out.push(at);
        }
        from = at + t.len().max(1);
        if from >= low.len() {
            break;
        }
    }
    out
}

fn spans_of(re: &Regex, text: &str) -> Vec<Range<usize>> {
    re.find_iter(text).map(|m| m.range()).collect()
}

fn inside(spans: &[Range<usize>], at: usize) -> bool {
    spans.iter().any(|s| at >= s.start && at < s.end)
}

/// True when `token` names something in the guard's own vocabulary — an
/// indicator, a currency code, a unit. Such a word is never a symbol, whatever
/// its shape. Using the same vocabulary the number-binding uses keeps the two
/// from disagreeing about what a word means.
fn is_vocabulary_token(token: &str) -> bool {
    let low = token.to_lowercase();
    if CURRENCY_CODES.iter().any(|c| c.eq_ignore_ascii_case(token)) {
        return true;
    }
    INDICATOR_TERMS
        .iter()
        .chain(GENERIC_TERMS.iter())
        .any(|(_, terms)| terms.iter().any(|t| *t == low))
}

/// Whether a ticker-shaped token may be read as naming an instrument.
///
/// A token containing a dot is unambiguous. A bare uppercase token is not: it is
/// read as a symbol only in a session whose own instruments are written that way
/// (a US-style listing), only when it is at least two characters, and only when
/// it is neither part of the guard's vocabulary nor ordinary English. The
/// alternative — reading every uppercase token as a symbol — drops legitimate
/// sentences ("I see RSI at 45.2"), which is a worse failure than the residual
/// gap: a bare foreign US symbol quoted inside a Japanese-listing session is not
/// flagged as foreign, and its number is still verified against the confirmed
/// values.
fn is_symbol_token(token: &str, bare_symbols_loaded: bool) -> bool {
    if token.contains('.') {
        return true;
    }
    if token.chars().count() < 2 {
        return false;
    }
    if NOT_A_TICKER.iter().any(|w| w.eq_ignore_ascii_case(token)) {
        return false;
    }
    bare_symbols_loaded && !is_vocabulary_token(token)
}

fn to_ascii_digits(value: &str) -> String {
    value
        .chars()
        .filter_map(|c| match c {
            '０'..='９' => char::from_u32('0' as u32 + (c as u32 - '０' as u32)),
            '，' | ',' => None,
            '．' => Some('.'),
            other => Some(other),
        })
        .collect()
}

/// The multiplier a magnitude word carries — a scale, and nothing else.
///
/// A magnitude says nothing about *what* is counted: "1000万株" is a share count
/// and "13.7兆円" is an amount, and the difference is the unit word, not the
/// 万/兆. Reading a magnitude as money let a share count be written as a dollar
/// amount, so the kind and the currency come from the unit word or the currency
/// symbol alone.
fn magnitude_of(word: Option<&str>) -> f64 {
    let Some(w) = word else {
        return 1.0;
    };
    match w.to_lowercase().as_str() {
        "兆" | "trillion" | "tn" => 1e12,
        "十億" | "billion" | "bn" | "b" => 1e9,
        "億" => 1e8,
        "百万" | "million" | "mm" | "m" => 1e6,
        "万" => 1e4,
        "千" | "thousand" | "k" => 1e3,
        _ => 1.0,
    }
}

/// The kind and currency a trailing unit word carries.
fn unit_of(word: Option<&str>) -> (Option<Kind>, Option<String>) {
    let Some(w) = word else {
        return (None, None);
    };
    match w.to_lowercase().as_str() {
        "円" | "yen" | "jpy" => (Some(Kind::Money), Some("JPY".into())),
        "usd" | "dollar" | "dollars" | "ドル" | "米ドル" => {
            (Some(Kind::Money), Some("USD".into()))
        }
        "eur" | "euro" | "euros" | "ユーロ" => (Some(Kind::Money), Some("EUR".into())),
        "gbp" | "pound" | "pounds" | "ポンド" => (Some(Kind::Money), Some("GBP".into())),
        "株" | "share" | "shares" => (Some(Kind::Count), None),
        "%" | "％" | "percent" | "pct" => (Some(Kind::Percent), None),
        _ => (None, None),
    }
}

fn currency_of_symbol(sym: &str) -> Option<String> {
    match sym.to_ascii_uppercase().as_str() {
        "$" | "USD" => Some("USD".into()),
        "¥" | "JPY" => Some("JPY".into()),
        "€" | "EUR" => Some("EUR".into()),
        "£" | "GBP" => Some("GBP".into()),
        _ => None,
    }
}

/// Read every number in `text`, keeping sign, magnitude, unit, currency and the
/// role it plays, plus the byte range it occupies. Digits that are part of a
/// ticker are skipped: the "9432" in "9432.T" is an identifier, not a reading.
pub fn written_numbers(text: &str) -> Vec<(Range<usize>, WrittenNumber)> {
    let scan = Scan::of(text);
    let mut out = Vec::new();
    for caps in RE_WRITTEN_NUMBER.captures_iter(text) {
        let whole = caps.get(0).expect("group 0 always present");
        let digits = caps.name("digits").expect("digits group is required");
        if inside(&scan.tickers, digits.start()) {
            continue;
        }
        let raw = to_ascii_digits(digits.as_str());
        let Ok(mut value) = raw.parse::<f64>() else {
            continue;
        };
        let decimals = raw.split_once('.').map(|(_, d)| d.len()).unwrap_or(0);
        let is_minus = |m: Option<regex::Match<'_>>| {
            matches!(
                m.map(|m| m.as_str()),
                Some("-") | Some("\u{2212}") | Some("\u{25B2}") | Some("\u{25B3}")
            )
        };
        let negative = is_minus(caps.name("sign_pre")) || is_minus(caps.name("sign"));
        let magnitude = magnitude_of(caps.name("mag").map(|m| m.as_str()));
        let (unit_kind, unit_currency) = unit_of(caps.name("unit").map(|m| m.as_str()));
        let pre_currency = caps
            .name("cur_pre")
            .and_then(|m| currency_of_symbol(m.as_str()));
        value *= magnitude;
        if negative {
            value = -value;
        }
        // A currency symbol in front says "this is money" just as a unit word does.
        let kind = unit_kind.or_else(|| pre_currency.as_ref().map(|_| Kind::Money));
        let currency = pre_currency.or(unit_currency);
        // A figure carrying money or a unit is an amount, whatever else the
        // characters around it could spell: `$5m` is five million dollars, not a
        // five-minute bar, and reading it as a bar would exempt it from
        // verification entirely. Only a bare number can name a bar size.
        let bare = kind.is_none() && currency.is_none();
        let role = if inside(&scan.params, digits.start())
            || (bare && inside(&scan.timeframes, digits.start()))
        {
            Role::PeriodParameter
        } else if inside(&scan.dates, digits.start())
            || scan
                .years
                .iter()
                .any(|(r, _)| digits.start() >= r.start && digits.start() < r.end)
        {
            Role::Date
        } else {
            Role::Observation
        };
        // Anything alphabetic still attached to the token was not understood.
        // An ordinal ("the 1st quarter") counts a position rather than a value;
        // any other leftover means the figure was written in something this
        // reader cannot check, and a figure that cannot be checked must not be
        // reported as confirmed.
        let consumed_end = caps
            .name("unit")
            .or_else(|| caps.name("mag"))
            .map_or(digits.end(), |m| m.end());
        let rest = &text[consumed_end..];
        let trailing: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        let ordinal = matches!(trailing.to_lowercase().as_str(), "st" | "nd" | "rd" | "th");
        let unknown_suffix = !trailing.is_empty() && !ordinal;
        let role = if ordinal { Role::PeriodParameter } else { role };
        out.push((
            whole.range(),
            WrittenNumber {
                value,
                decimals,
                magnitude,
                kind,
                currency,
                role,
                unknown_suffix,
            },
        ));
    }
    out
}

/// The sentence `pos` falls in, and where that sentence starts. Attribution is a
/// property of the sentence making the claim, not of the whole answer.
///
/// A `.` ends a sentence only when whitespace or the end of the text follows it,
/// so neither a decimal point (`168.8`) nor a ticker's dot (`9432.T`) splits one.
pub fn sentence_end(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    for (i, ch) in text.char_indices() {
        let ends = match ch {
            '。' | '\n' | '！' | '？' => true,
            '.' | '!' | '?' => {
                let next = i + ch.len_utf8();
                next >= bytes.len() || (bytes[next] as char).is_ascii_whitespace()
            }
            _ => false,
        };
        if ends {
            return Some(i + ch.len_utf8());
        }
    }
    None
}

pub fn sentence_bounds(text: &str, pos: usize) -> (usize, &str) {
    let is_boundary = |bytes: &[u8], i: usize, ch: char| -> bool {
        match ch {
            '。' | '\n' | '！' | '？' => true,
            '.' | '!' | '?' => {
                let next = i + ch.len_utf8();
                next >= bytes.len() || (bytes[next] as char).is_ascii_whitespace()
            }
            _ => false,
        }
    };
    let bytes = text.as_bytes();
    let mut start = 0usize;
    let mut end = text.len();
    for (i, ch) in text.char_indices() {
        if i >= pos {
            break;
        }
        if is_boundary(bytes, i, ch) {
            start = i + ch.len_utf8();
        }
    }
    for (i, ch) in text[pos..].char_indices() {
        let abs = pos + i;
        if is_boundary(bytes, abs, ch) {
            end = abs;
            break;
        }
    }
    let start = start.min(end);
    (start, text[start..end].trim_end())
}

// ─── Building the reference from confirmed structures ────────────────────────

impl SymbolFacts {
    fn push_at(&mut self, key: &'static str, value: Option<f64>, kind: Kind, year: Option<i32>) {
        if let Some(v) = value.filter(|v| v.is_finite()) {
            self.facts.push(ConfirmedFact {
                key,
                value: v,
                kind,
                as_of_year: year,
                window: None,
            });
        }
    }

    fn push(&mut self, key: &'static str, value: Option<f64>, kind: Kind) {
        let year = self.as_of_year;
        self.push_at(key, value, kind, year);
    }

    /// Same, recording the window the value was computed over.
    fn push_windowed(&mut self, key: &'static str, value: Option<f64>, kind: Kind, window: usize) {
        let before = self.facts.len();
        self.push(key, value, kind);
        if self.facts.len() > before {
            self.facts[before].window = Some(window);
        }
    }

    /// Record a threshold the engine stated for `key` — the bound in an alert's
    /// condition, say. The model may restate the rule it was told about
    /// ("score <= 5 のアラート条件が成立") without that 5 being read as a claim
    /// that the score *is* 5: the value is accepted for that indicator, and only
    /// that exact value is.
    pub fn push_bound(&mut self, key: &str, op: Comparison, value: f64) {
        if value.is_finite() {
            self.bounds.push((key.to_string(), op, value));
        }
    }

    /// Name the timeframe these values were computed on, so an answer that
    /// discusses several timeframes of one instrument attributes each number to
    /// the right one.
    pub fn with_timeframe(mut self, mode: crate::config::AnalysisMode) -> SymbolFacts {
        self.timeframe_terms = vec![
            mode.as_str().to_string(),
            mode.bar_label_ja().to_string(),
            mode.bar_label("en").to_string(),
        ];
        self
    }

    /// Keep the fundamental values of a previous load.
    ///
    /// A periodic or `/reload` refresh re-fetches the technical side only and
    /// leaves the fundamental block the session already holds in place. The
    /// confirmed data must describe what the session actually presents, so the
    /// previously confirmed fundamental values (including the ratios and the
    /// price they were derived from) travel with it.
    pub fn carry_over_fundamentals(&mut self, previous: &SymbolFacts) {
        if self.facts.iter().any(|f| FUNDAMENTAL_KEYS.contains(&f.key)) {
            return; // this load fetched its own fundamentals
        }
        self.facts.extend(
            previous
                .facts
                .iter()
                .filter(|f| FUNDAMENTAL_KEYS.contains(&f.key))
                .cloned(),
        );
        if self.currency == Currency::Unknown {
            self.currency = previous.currency.clone();
        }
    }

    /// Build one instrument's confirmed data from the structures that own it.
    /// Nothing here reads a rendered string.
    pub fn from_sources(
        guard: &crate::technical::types::TechnicalDataGuard,
        config: &crate::config::Config,
        currency: Option<&str>,
        fundamental: Option<&crate::fundamental::FundamentalData>,
    ) -> SymbolFacts {
        let mut s = SymbolFacts {
            symbol: guard.get_ticker().to_string(),
            name: guard.get_name().to_string(),
            currency: match currency
                .filter(|c| !c.is_empty())
                .or_else(|| guard.get_currency().filter(|c| !c.is_empty()))
            {
                Some(c) => Currency::Known(c.to_string()),
                // The provider did not report one. A `.T` ticker is a Tokyo
                // listing, so its currency is known from the symbol itself;
                // otherwise it stays unknown rather than being guessed.
                None if guard.get_ticker().to_ascii_uppercase().ends_with(".T") => {
                    Currency::Known("JPY".to_string())
                }
                None => Currency::Unknown,
            },
            as_of_year: guard
                .get_date()
                .get(0..4)
                .and_then(|y| y.parse::<i32>().ok()),
            timeframe_terms: Vec::new(),
            facts: Vec::new(),
            bounds: Vec::new(),
        };

        s.push("close", Some(guard.get_close()), Kind::Money);
        s.push("prev_close", Some(guard.get_previous_close()), Kind::Money);
        s.push(
            "latest_price",
            guard.get_latest_observed_price(),
            Kind::Money,
        );
        // The header shows the change as an amount and as a percentage under one
        // label, so both are confirmed values (H6).
        let (diff, diff_pct) = crate::utils::displayed_price_diff(guard);
        s.push("price_diff", Some(diff), Kind::Money);
        s.push("price_diff_pct", Some(diff_pct), Kind::Percent);
        // Only indicators actually computed (R1) become confirmed facts.
        for (key, kind) in [
            ("rsi", Kind::Index),
            ("macd", Kind::Index),
            ("macd_signal", Kind::Index),
        ] {
            s.push(key, guard.computed_indicator(key), kind);
        }
        // The moving-average legs and the bands differ from each other only by
        // their window, so the window travels with the value (L3).
        for (key, kind, window) in [
            ("ema_s", Kind::Money, config.ema_short_period),
            ("ema_l", Kind::Money, config.ema_long_period),
            ("sma_s", Kind::Money, config.sma_short_period),
            ("sma_l", Kind::Money, config.sma_long_period),
            ("bb_u", Kind::Money, config.bollinger_period),
            ("bb_l", Kind::Money, config.bollinger_period),
            ("pct_b", Kind::Ratio, config.bollinger_period),
        ] {
            s.push_windowed(key, guard.computed_indicator(key), kind, window);
        }
        // Bandwidth is derived from the band edges, so it is confirmed exactly
        // when they are. Without this the model could not cite it at all.
        if guard.is_computed("bb_u") && guard.is_computed("bb_l") {
            s.push("bandwidth", Some(guard.get_bb_bandwidth()), Kind::Percent);
        }
        s.push("adx", guard.get_adx(), Kind::Index);
        s.push("roc", guard.get_roc(), Kind::Percent);
        s.push("stoch_k", guard.get_stochastics_k(), Kind::Index);
        s.push("stoch_d", guard.get_stochastics_d(), Kind::Index);
        s.push("tenkan", guard.get_tenkan_sen(), Kind::Money);
        s.push("kijun", guard.get_kijun_sen(), Kind::Money);
        s.push("fib_382", guard.get_fibo_38_2(), Kind::Money);
        s.push("fib_500", guard.get_fibo_50_0(), Kind::Money);
        s.push("fib_618", guard.get_fibo_61_8(), Kind::Money);
        s.push("vwap", guard.get_vwap(), Kind::Money);
        s.push("volume", guard.get_latest_volume(), Kind::Count);
        s.push("avg_volume", guard.get_avg_volume(), Kind::Count);
        s.push("volume_ratio", guard.get_volume_ratio(), Kind::Ratio);
        s.push(
            "score",
            Some(crate::technical::calculate_final_score_snapshot(config, guard).total_score),
            Kind::Score,
        );

        if let Some(f) = fundamental {
            // Fundamentals belong to their reporting period, not to the bar the
            // indicators were computed on, so they carry their own year.
            let fy = f
                .fiscal_period()
                .or_else(|| f.reported_date())
                .and_then(|t| RE_YEAR.find(t).map(|m| m.as_str().to_string()))
                .and_then(|y| y.parse::<i32>().ok());
            for (key, value, kind) in [
                ("revenue", f.revenue(), Kind::Money),
                ("operating_income", f.operating_income(), Kind::Money),
                ("net_income", f.net_income(), Kind::Money),
                ("eps", f.eps(), Kind::Money),
                ("bps", f.bps(), Kind::Money),
                ("equity", f.equity(), Kind::Money),
                ("shares_outstanding", f.shares_outstanding(), Kind::Count),
                ("dividend", f.dividend(), Kind::Money),
                ("lot_dividend", f.lot_dividend(), Kind::Money),
                (
                    "trading_unit",
                    f.trading_unit().map(|u| u as f64),
                    Kind::Count,
                ),
                ("per", f.per(), Kind::Index),
                ("pbr", f.pbr(), Kind::Index),
                ("roe", f.roe_pct(), Kind::Percent),
                ("operating_margin", f.operating_margin_pct(), Kind::Percent),
            ] {
                s.push_at(key, value, kind, fy);
            }
            if s.currency == Currency::Unknown && !f.currency().is_empty() {
                s.currency = Currency::Known(f.currency().to_string());
            }
        }
        s
    }
}

impl ConfirmedFactSet {
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    pub fn symbols(&self) -> &[SymbolFacts] {
        &self.symbols
    }

    pub fn push_symbol(&mut self, facts: SymbolFacts) {
        self.symbols.push(facts);
    }

    /// Record a threshold the engine stated for `key` on every loaded instrument.
    /// Used by the alert monitor, whose message states the rule it is reporting.
    pub fn push_bound(&mut self, key: &str, op: Comparison, value: f64) {
        for s in self.symbols.iter_mut() {
            s.push_bound(key, op, value);
        }
    }

    /// The confirmed value of `key` for the single loaded instrument, or `None`
    /// when several are loaded (the reading would be ambiguous) or the indicator
    /// was not computed.
    pub fn single_value(&self, key: &str) -> Option<f64> {
        match self.symbols.as_slice() {
            [only] => only.facts.iter().find(|f| f.key == key).map(|f| f.value),
            _ => None,
        }
    }

    /// How the confirmed `a` compares with the confirmed `b`: `Some(Ordering)`
    /// when both are known, `None` when either is not. Used where a direction has
    /// to be read from the data rather than from a sentence describing it — the
    /// label wording in the prompt then cannot change the verdict.
    pub fn compare(&self, a: &str, b: &str) -> Option<std::cmp::Ordering> {
        let (x, y) = (self.single_value(a)?, self.single_value(b)?);
        x.partial_cmp(&y)
    }

    /// Indicator keys mentioned in `text`, with the byte offset of each mention,
    /// so a number can be bound to the nearest one. Several keys may share one
    /// mention — "EMA" without a leg names both the short and the long one.
    fn mentions(text: &str) -> Vec<(usize, &'static str)> {
        let low = text.to_lowercase();
        let mut found: Vec<(usize, usize, &'static str)> = Vec::new();
        for (key, terms) in INDICATOR_TERMS {
            for term in *terms {
                for at in term_positions(&low, term) {
                    found.push((at, term.len(), key));
                }
            }
        }
        // Generic wording is searched at every position too, not only when the
        // sentence contains no specific term: "RSI is 45.2 and EMA is 166.88"
        // names one of each, and skipping the generic pass would leave the EMA
        // unbound. A generic hit that falls inside a specific one is dropped, so
        // "長期EMA" is not also read as a bare "EMA".
        // Only the *specific* spans mask a generic hit. Two generic rows may name
        // the same span (an unqualified "EMA" is both legs), and that must survive.
        let specific: Vec<(usize, usize)> = found.iter().map(|(p, l, _)| (*p, *l)).collect();
        for (key, terms) in GENERIC_TERMS {
            for term in *terms {
                for at in term_positions(&low, term) {
                    let covered = specific
                        .iter()
                        .any(|(p, l)| at >= *p && at + term.len() <= *p + *l);
                    if !covered {
                        found.push((at, term.len(), key));
                    }
                }
            }
        }
        // At the same position the longest term wins, so "MACDシグナル" is not
        // read as "MACD". Alternative readings of the *same* span are all kept,
        // so an unqualified "EMA" stays bound to both legs.
        found.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        let mut out: Vec<(usize, &'static str)> = Vec::new();
        let mut taken: Option<(usize, usize)> = None;
        for (pos, len, key) in found {
            match taken {
                Some((tp, tl)) if pos == tp && len == tl => {}
                Some((tp, tl)) if pos < tp + tl => continue,
                _ => taken = Some((pos, len)),
            }
            out.push((pos, key));
        }
        out
    }

    /// Positions of instrument mentions in the sentence, each resolved to the
    /// loaded facts it names or to `None` when it names something not loaded.
    fn symbol_mentions(&self, sentence: &str) -> Vec<(usize, Option<String>)> {
        let bare_loaded = self
            .symbols
            .iter()
            .any(|s| !s.symbol.contains('.') && s.symbol.chars().all(|c| c.is_ascii_alphabetic()));
        let mut out: Vec<(usize, Option<String>)> = Vec::new();
        for m in RE_TICKER.find_iter(sentence) {
            let token = m.as_str();
            // A loaded instrument named explicitly is a symbol mention whatever
            // the token looks like: a one-letter code and a code that collides
            // with an ordinary word are still this session's own instruments,
            // and matching them first is what keeps their values apart.
            let resolved = self
                .symbols
                .iter()
                .find(|s| {
                    s.symbol.eq_ignore_ascii_case(token)
                        || s.symbol.split('.').next().is_some_and(|c| c == token)
                })
                .map(|s| s.symbol.clone());
            if resolved.is_none() && !is_symbol_token(token, bare_loaded) {
                continue;
            }
            // A token that is part of the instrument's own name is not another
            // instrument.
            if resolved.is_none() && self.symbols.iter().any(|s| s.name.contains(token)) {
                continue;
            }
            out.push((m.start(), resolved));
        }
        for s in &self.symbols {
            if s.name.is_empty() {
                continue;
            }
            if let Some(i) = sentence.find(&s.name) {
                out.push((i, Some(s.symbol.clone())));
            }
        }
        out.sort_by_key(|(p, _)| *p);
        out
    }

    /// The instruments a number written at `at` may be about: the one named
    /// nearest before it, else the one named nearest after it, else every loaded
    /// instrument. Narrowed to one timeframe when a bar is named for *this*
    /// number — the same nearest-mention rule as the symbol, so a sentence that
    /// discusses two bars attributes each figure to the bar it was written under.
    fn subject_at(
        &self,
        sentence: &str,
        at: usize,
        scan: &Scan,
    ) -> Result<Vec<&SymbolFacts>, ClaimVerdict> {
        let mentions: Vec<(usize, usize)> = self
            .symbol_mentions(sentence)
            .into_iter()
            .enumerate()
            .map(|(i, (p, _))| (p, i))
            .collect();
        let resolved: Vec<Option<String>> = self
            .symbol_mentions(sentence)
            .into_iter()
            .map(|(_, r)| r)
            .collect();
        let chosen = qualifier_at(&mentions, sentence, at, scan).and_then(|(_, i)| resolved.get(i));
        let candidates: Vec<&SymbolFacts> = match chosen {
            Some(None) => return Err(ClaimVerdict::ForeignSymbol),
            Some(Some(symbol)) => self
                .symbols
                .iter()
                .filter(|s| s.symbol.eq_ignore_ascii_case(symbol))
                .collect(),
            None => self.symbols.iter().collect(),
        };
        if candidates.is_empty() {
            return Ok(candidates);
        }
        Self::narrow_to_timeframe(candidates, sentence, at, scan)
    }

    /// Keep only the candidates whose bar is the one named for this figure.
    ///
    /// With no bar named, every candidate stays: an answer that does not say
    /// which bar it means is not making a claim about one. A bar that was named
    /// but never analysed carries no confirmed value at all, so a figure written
    /// under it is refused rather than matched against some other bar.
    fn narrow_to_timeframe<'a>(
        candidates: Vec<&'a SymbolFacts>,
        sentence: &str,
        at: usize,
        scan: &Scan,
    ) -> Result<Vec<&'a SymbolFacts>, ClaimVerdict> {
        // Only meaningful where the caller recorded which bar each entry is.
        if candidates.iter().all(|s| s.timeframe_terms.is_empty()) {
            return Ok(candidates);
        }
        let low = sentence.to_lowercase();
        // An ASCII bar name must stand on its own: "1m" inside "21m" is part of a
        // number, not a mention of the one-minute bar.
        let find_all = |term: &str, out: &mut Vec<usize>| {
            let t = term.to_lowercase();
            if t.is_empty() {
                return;
            }
            let ascii = t.is_ascii();
            let mut from = 0usize;
            while let Some(i) = low[from..].find(&t) {
                let at = from + i;
                let standalone = !ascii || {
                    let before_ok = low[..at]
                        .chars()
                        .next_back()
                        .is_none_or(|c| !c.is_ascii_alphanumeric());
                    let after_ok = low[at + t.len()..]
                        .chars()
                        .next()
                        .is_none_or(|c| !c.is_ascii_alphanumeric());
                    before_ok && after_ok
                };
                if standalone {
                    out.push(at);
                }
                from = at + t.len().max(1);
                if from >= low.len() {
                    break;
                }
            }
        };
        // Positions naming a loaded bar, and positions naming any bar at all.
        let mut mine: Vec<(usize, usize)> = Vec::new(); // (position, candidate index)
        for (idx, s) in candidates.iter().enumerate() {
            for term in &s.timeframe_terms {
                let mut hits = Vec::new();
                find_all(term, &mut hits);
                mine.extend(hits.into_iter().map(|p| (p, idx)));
            }
        }
        let mut any: Vec<(usize, ())> = Vec::new();
        for term in all_timeframe_terms() {
            let mut hits = Vec::new();
            find_all(&term, &mut hits);
            any.extend(hits.into_iter().map(|p| (p, ())));
        }
        mine.sort_by_key(|(p, _)| *p);
        any.sort_by_key(|(p, _)| *p);

        let named_any = qualifier_at(&any, sentence, at, scan);
        let named_mine = qualifier_at(&mine, sentence, at, scan);
        match (named_any, named_mine) {
            // A bar was named for this figure, and it is not one that was
            // analysed: no confirmed value belongs to it.
            (Some((pos_any, _)), None) => {
                let _ = pos_any;
                Err(ClaimVerdict::PeriodMismatch)
            }
            (Some((pos_any, _)), Some((pos_mine, _))) if pos_any != pos_mine => {
                Err(ClaimVerdict::PeriodMismatch)
            }
            (_, Some((pos, _))) => Ok(candidates
                .into_iter()
                .enumerate()
                .filter(|(i, _)| mine.iter().any(|(p, j)| *p == pos && j == i))
                .map(|(_, s)| s)
                .collect()),
            (None, None) => Ok(candidates),
        }
    }

    /// True when `written` equals `fact` at the precision the model displayed.
    ///
    /// Rounding is judged at the magnitude the number was written in: 13.7兆円
    /// stands for anything within ±0.05兆円, so a confirmed 13.704兆円 passes and
    /// 13.8兆円 does not. Sign is part of the comparison.
    fn same_number(fact: f64, n: &WrittenNumber) -> bool {
        if (fact < 0.0) != (n.value < 0.0) && fact != 0.0 && n.value != 0.0 {
            return false;
        }
        let magnitude = if n.magnitude.is_finite() && n.magnitude > 0.0 {
            n.magnitude
        } else {
            1.0
        };
        let tolerance = 0.5 * magnitude / 10f64.powi(n.decimals.min(10) as i32);
        let diff = (fact - n.value).abs();
        if !diff.is_finite() || !tolerance.is_finite() {
            return fact == n.value;
        }
        // `<=` would accept a value exactly on the boundary in both directions;
        // `<` keeps a single reading for each written figure.
        diff < tolerance
    }

    /// Verify one number written inside `sentence` at byte range `at`.
    pub fn verify(&self, sentence: &str, at: &Range<usize>, n: &WrittenNumber) -> ClaimVerdict {
        self.verify_with(&Scan::of(sentence), sentence, at, n)
    }

    /// Same, against a `Scan` the caller already built for this sentence. Every
    /// number in one sentence shares one scan, and so do all four bindings.
    pub fn verify_with(
        &self,
        scan: &Scan,
        sentence: &str,
        at: &Range<usize>,
        n: &WrittenNumber,
    ) -> ClaimVerdict {
        // A window length or a date is not a claim about a value.
        if n.role != Role::Observation {
            return ClaimVerdict::Verified;
        }
        let candidates = match self.subject_at(sentence, at.start, scan) {
            Err(v) => return v,
            Ok(c) if c.is_empty() => return ClaimVerdict::NotInConfirmedData,
            Ok(c) => c,
        };
        let (bound_at, bound) = Self::bound_keys(sentence, at.start, scan);
        let stated_year = Self::stated_year_at(sentence, at.start, scan);
        // A window applies to the indicator it was written on, so it is taken
        // only when it sits on the very name this number was bound to.
        let stated_window = bound_at.and_then(|pos| {
            scan.windows
                .iter()
                .find(|(name, _)| pos >= name.start && pos < name.end)
                .map(|(_, w)| *w)
        });

        // A condition the engine itself stated is quotable — but the sentence has
        // to restate *that* condition. The comparison is parsed, not guessed from
        // a word appearing somewhere, and the parsed indicator, direction,
        // equality and value must all match the rule the engine reported.
        if let Some(written) = Self::written_comparison(sentence, at, bound_at) {
            // A restatement quotes the rule as the engine wrote it: a bare
            // number. `close <= ¥3091`, `score <= 5%` and `score <= 5foo` state
            // something the rule does not — they are still conditions, so they
            // are refused as conditions rather than handed to the reading check.
            let quotable = !n.unknown_suffix && n.kind.is_none() && n.currency.is_none();
            let mut states_a_rule = false;
            for subject in &candidates {
                for (key, op, value) in &subject.bounds {
                    if !bound.contains(&key.as_str()) {
                        continue;
                    }
                    states_a_rule = true;
                    if quotable && *op == written && *value == n.value {
                        return ClaimVerdict::Verified;
                    }
                }
            }
            // The sentence writes a condition on an indicator the engine stated a
            // rule for, and it is not that rule. It must not fall through to the
            // reading check: where the threshold happens to equal the current
            // value (a rule of `score <= 0` on a score of 0), that check would
            // accept `score > 0` — a condition the engine never reported.
            if states_a_rule {
                return ClaimVerdict::ConditionDoesNotMatchRule;
            }
        }

        let mut worst = ClaimVerdict::NotInConfirmedData;
        for subject in candidates {
            let verdict = Self::verify_against(subject, &bound, stated_year, stated_window, n);
            if verdict.is_verified() {
                return ClaimVerdict::Verified;
            }
            if verdict.severity() > worst.severity() {
                worst = verdict;
            }
        }
        worst
    }

    /// The comparison this number is written in, when it is written in one that
    /// could restate a rule.
    ///
    /// This answers only "is it written as a comparison?" — never "may it stand".
    /// Conflating the two sent a condition the engine did not state back to the
    /// reading check, where `close > ¥3091` was accepted because 3091 happens to
    /// be the close. Whether a recognised comparison may restate the rule is
    /// decided separately, in `verify`.
    ///
    /// The comparison is taken from between the indicator and the number
    /// (`score <= 5`, `at or below 5`) or from immediately after it (`5以下`),
    /// which is how the two languages write the same thing.
    fn written_comparison(
        sentence: &str,
        at: &Range<usize>,
        bound_at: Option<usize>,
    ) -> Option<Comparison> {
        if let Some(after) = sentence.get(at.end..).and_then(comparison_after) {
            return Some(after);
        }
        let pos = bound_at?;
        if pos >= at.start {
            return None;
        }
        let span = sentence.get(pos..at.start)?;
        let (cmp, end) = comparison_in(span)?;
        // The operator belongs to the number it compares — the one that follows
        // it. In "score is at or below 5 (current value: 5.0)" the 5 is the
        // rule's right-hand side and the 5.0 is a reading; only whitespace may
        // stand between the operator and the number it qualifies.
        let between = span.get(end..)?;
        between.trim().is_empty().then_some(cmp)
    }

    /// The indicator keys the number at `at` is a reading of, and where that
    /// indicator was named — the position is what ties a stated window to the
    /// indicator it was written on rather than to whichever number is nearest.
    fn bound_keys(sentence: &str, at: usize, scan: &Scan) -> (Option<usize>, Vec<&'static str>) {
        let mentions = Self::mentions(sentence);
        // Only a name written in this number's own claim binds it. A figure whose
        // claim names no indicator is left unbound rather than attributed to the
        // neighbouring claim's name — §1, "the one named in the claim".
        match qualifier_in_clause(&mentions, sentence, at, scan) {
            Some((pos, _)) => (
                Some(pos),
                mentions
                    .iter()
                    .filter(|(p, _)| *p == pos)
                    .map(|(_, k)| *k)
                    .collect(),
            ),
            None => (None, Vec::new()),
        }
    }

    /// The year the claim at `at` is dated to: the year-only expression written
    /// nearest before it, else the one nearest after it.
    ///
    /// Only a year *written as a year* counts — 2019年, 年度, FY2025, "in 2019".
    /// A full calendar date (`2026-09-08`) stamps when the analysis ran; reading
    /// it as a period claim would refuse an annual figure whose fiscal year
    /// legitimately differs from the bar's. And a price that happens to look like
    /// a year is not a date at all.
    fn stated_year_at(sentence: &str, at: usize, scan: &Scan) -> Option<i32> {
        let mut marks: Vec<(usize, i32)> = scan.years.iter().map(|(r, y)| (r.start, *y)).collect();
        marks.sort_by_key(|(p, _)| *p);
        qualifier_at(&marks, sentence, at, scan).map(|(_, y)| y)
    }

    fn verify_against(
        subject: &SymbolFacts,
        bound: &[&'static str],
        stated_year: Option<i32>,
        stated_window: Option<usize>,
        n: &WrittenNumber,
    ) -> ClaimVerdict {
        let named: Vec<&ConfirmedFact> = if bound.is_empty() {
            subject.facts.iter().collect()
        } else {
            subject
                .facts
                .iter()
                .filter(|f| bound.contains(&f.key))
                .collect()
        };
        // A stated window picks the leg it names. A value whose window is not
        // recorded is left alone — only indicators that actually differ by window
        // can be selected this way, so `RSI(14)` does not become a claim about a
        // window the engine never tracked.
        let named: Vec<&ConfirmedFact> = match stated_window {
            Some(w) if named.iter().any(|f| f.window.is_some()) => named
                .into_iter()
                .filter(|f| f.window.is_none_or(|fw| fw == w))
                .collect(),
            _ => named,
        };
        // A figure written with a suffix the reader does not understand cannot be
        // confirmed by its digits alone.
        if n.unknown_suffix {
            return if bound.is_empty() {
                ClaimVerdict::ValueNotConfirmedForSymbol
            } else {
                ClaimVerdict::UnitMismatch
            };
        }
        let mut soft: Option<ClaimVerdict> = None;
        for f in &named {
            if !Self::same_number(f.value, n) {
                continue;
            }
            if let Some(k) = n.kind {
                if k != f.kind {
                    soft = Some(ClaimVerdict::UnitMismatch);
                    continue;
                }
            }
            // A currency can only be written on an amount. "Volume is $1000"
            // states money for a share count, so the currency is checked against
            // the kind before it is checked against the instrument.
            if n.currency.is_some() && f.kind != Kind::Money {
                soft = Some(ClaimVerdict::UnitMismatch);
                continue;
            }
            if f.kind == Kind::Money && !subject.currency.matches(n.currency.as_deref()) {
                soft = Some(ClaimVerdict::UnitMismatch);
                continue;
            }
            if let (Some(said), Some(actual)) = (stated_year, f.as_of_year) {
                if said != actual {
                    soft = Some(ClaimVerdict::PeriodMismatch);
                    continue;
                }
            }
            return ClaimVerdict::Verified;
        }

        if bound.is_empty() {
            // No indicator named. A bare number is not a claim — "3 reasons" is
            // not a reading. A number written with a unit or a currency is: it
            // states an amount for the instrument the sentence is about, so it
            // must correspond to one of that instrument's confirmed values.
            if let Some(v) = soft {
                return v;
            }
            return if n.kind.is_some() || n.currency.is_some() {
                ClaimVerdict::ValueNotConfirmedForSymbol
            } else {
                ClaimVerdict::NotInConfirmedData
            };
        }
        if named.is_empty() {
            // The sentence states a reading for an indicator with no confirmed
            // value — never computed, or not part of this analysis at all.
            return ClaimVerdict::IndicatorNotComputed;
        }
        if subject
            .facts
            .iter()
            .any(|f| !bound.contains(&f.key) && Self::same_number(f.value, n))
        {
            return ClaimVerdict::AttributedToWrongIndicator;
        }
        soft.unwrap_or(ClaimVerdict::ValueDoesNotMatchIndicator)
    }
}

/// Build a set for the single-instrument analysis path.
pub fn facts_for(
    guard: &crate::technical::types::TechnicalDataGuard,
    config: &crate::config::Config,
    currency: Option<&str>,
    fundamental: Option<&crate::fundamental::FundamentalData>,
) -> ConfirmedFactSet {
    let mut set = ConfirmedFactSet::default();
    set.push_symbol(SymbolFacts::from_sources(
        guard,
        config,
        currency,
        fundamental,
    ));
    set
}

/// Merge per-instrument facts (the multi-ticker chat path).
pub fn facts_from_parts(parts: impl IntoIterator<Item = SymbolFacts>) -> ConfirmedFactSet {
    let mut set = ConfirmedFactSet::default();
    for p in parts {
        set.push_symbol(p);
    }
    set
}
