//! LLM integration for analysis insights

use crate::config::{AnalysisMode, Config, Stance};

#[derive(Debug, Default, Clone, Copy)]
pub struct ChatTokenUsage {
    pub input: u32,
    pub output: u32,
}

impl ChatTokenUsage {
    /// Accumulate one turn's usage into a running total — the single place the
    /// per-turn `input`/`output` sums live (SOT §4.2), shared by chat, debate,
    /// council and the exec loop.
    pub fn accumulate(&mut self, other: ChatTokenUsage) {
        self.input += other.input;
        self.output += other.output;
    }
}
use crate::news::Article;
use crate::technical::TechnicalDataGuard;
use anyhow::{anyhow, bail, Result};
use regex::Regex;
use std::sync::LazyLock;
use std::time::Duration;

static RE_NUMERIC_FRAGMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[0-9０-９][0-9０-９,，.．]*").expect("valid regex"));
// Check 1 — a number carrying a unit. Japanese and English forms of the SAME
// check: yen/percent written in Japanese, and the English money/percent forms an
// `--lang en` answer uses. Written case-insensitively so "USD"/"usd" both match.
static RE_NUMERIC_UNIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:約|approx\.?|about|around)?\s*(?:[$¥€£]\s*)?[0-9０-９][0-9０-９,，.．]*\s*(?:兆円|億円|万円|円|%|％|percent|pct|trillion|billion|million|thousand|yen|usd|dollars?)",
    )
    .expect("valid regex")
});
static RE_BARE_PRICE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"約?\s*[0-9０-９][0-9０-９,，]{2,}(?:[.．][0-9０-９]+)?(?:\s*[〜~\-ー―]\s*[0-9０-９][0-9０-９,，]{2,}(?:[.．][0-9０-９]+)?)?").expect("valid regex")
});
// Check 5 — the stated direction of VWAP against the close. Each pattern carries
// the Japanese and the English way of saying the same relation, so the check
// cannot exist in one language only. `(?i)` folds the English case.
const PRICE_WORDS_JA: &str = "終値|現在値|現在価格|株価|指標計算最終足終値";
const PRICE_WORDS_EN: &str = "close|closing price|last price|price";
static RE_VWAP_PROMPT_BELOW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?:VWAPが.*(?:{ja}).*下|vwap\b[^.。]*\bbelow\b[^.。]*(?:{en})|(?:{en})[^.。]*\babove\b[^.。]*vwap)",
        ja = PRICE_WORDS_JA,
        en = PRICE_WORDS_EN
    ))
    .expect("valid regex")
});
static RE_VWAP_PROMPT_ABOVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?:VWAPが.*(?:{ja}).*上|vwap\b[^.。]*\babove\b[^.。]*(?:{en})|(?:{en})[^.。]*\bbelow\b[^.。]*vwap)",
        ja = PRICE_WORDS_JA,
        en = PRICE_WORDS_EN
    ))
    .expect("valid regex")
});
static RE_VWAP_CONTENT_ABOVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?:VWAPが.*(?:{ja}).*上回|vwap\b[^.。]*\b(?:above|higher than|exceeds?)\b[^.。]*(?:{en}))",
        ja = PRICE_WORDS_JA,
        en = PRICE_WORDS_EN
    ))
    .expect("valid regex")
});
static RE_VWAP_CONTENT_BELOW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?:VWAPが.*(?:{ja}).*下回|vwap\b[^.。]*\b(?:below|lower than|under)\b[^.。]*(?:{en}))",
        ja = PRICE_WORDS_JA,
        en = PRICE_WORDS_EN
    ))
    .expect("valid regex")
});
// ── Integrity-check vocabulary (single source; NOT per language) ─────────────
// The six checks are defined once. Each list holds the Japanese and English
// members of the SAME check, so a check can never exist in one language only.
// Matching is case-insensitive, so English entries are written in lower case.

/// Check 4 — trade-action context (entry / take-profit / stop-loss).
static TRADE_ACTION_TERMS: &[&str] = &[
    // 日本語
    "エントリー",
    "利確",
    "利益確定",
    "損切り",
    "撤退",
    // English
    "entry",
    "entry point",
    "take profit",
    "take-profit",
    "profit target",
    "stop loss",
    "stop-loss",
    "stoploss",
    "exit point",
    "cut loss",
];

/// Check 5 — comparisons / derived metrics that the input never supplied.
static DERIVED_METRIC_TERMS: &[&str] = &[
    // 日本語
    "業界平均",
    "業界の平均",
    "業界水準",
    "平均的な水準",
    "平均水準",
    "同業平均",
    "同業他社",
    "配当利回り",
    "営業利益率",
    "純利益率",
    "利益率",
    // English
    "industry average",
    "sector average",
    "industry level",
    "peer average",
    "peer group",
    "dividend yield",
    "operating margin",
    "net margin",
    "profit margin",
    "gross margin",
    "return on equity",
];

/// True when `value` contains any term of `terms`, in either language.
/// Japanese needs raw substring matching; English needs case folding — one
/// lowered haystack serves both.
/// Whether `value` contains any of `terms` **as a word**.
///
/// The gate lists carry the Japanese and the English wording of the same check,
/// so the boundary rule has to differ by script: Japanese has no word boundary
/// and is matched as-is, while a bare ASCII term must not sit inside a longer
/// word. A raw `contains` made "low" match *below* / *following*, "line" match
/// *decline* / *headline* and "close" match *closely*, which opened the
/// price-context gate on almost any English answer. The rule lives in one place
/// (`integrity::term_positions`), the same one the indicator binding uses.
fn contains_any_term(value: &str, terms: &[&str]) -> bool {
    let lowered = value.to_lowercase();
    terms
        .iter()
        .any(|term| !crate::integrity::term_positions(&lowered, term).is_empty())
}

static RE_LIST_LIKE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9０-９]+[.)．、]").expect("valid regex"));

// Source-of-truth reinforcement, delivered as a SYSTEM message IDENTICALLY by every
// provider builder (OpenAI/Ollama system role, Gemini system_instruction, Claude
// system field). This is the single frame — there is no per-provider prompt
// (design-philosophy / no-prompt-bias). A system message is obeyed far more reliably
// than the same text folded into the user turn — decisive for small local models. It
// restates the no-fabrication / quote-verbatim / titles+URLs-only constraints so any
// model, local or cloud, is held to the same rules.
const SOT_SYSTEM_PROMPT: &str = "\
あなたはXOKSAの投資分析補助LLMです。次の制約を必ず守ってください。\
1. 企業の資本関係・親子会社・事業内容など、広く確立して変わりにくい事実は、あなた自身の一般知識で用いてよい。その場合はXOKSAの確定データと混同させず、一般知識に基づく旨がわかるように述べる。一般的な市場・マクロ・季節性・地政学の概念による推論も用いてよい。ユーザーの発言は無条件に事実として飲まず、自らの知識で妥当性を判断し、誤りや不確かな点は指摘または保留する。\
2. 速報性・個別性の高い事実（直近の提携や合併の発表そのもの、決算の実数、事件、業績見通し、株主還元、規制、個別の製品情報、ニュースの具体的内容）は、入力に明記されたものだけを事実として扱い、入力になければ確定した事実として述べない。\
3. 入力にない数値、金額、割合、日付、イベント名、ニュースタイトルを創作したり、入力・確定データであるかのように提示しない。\
4. ニュース入力はタイトルとURLのみとして扱う。見出しが示す話題やセンチメントは、質問や解説がテクニカル中心の場合でも、材料環境として積極的に織り込む。ただし記事本文を読んだ前提の評価、本文内容の推測、価格影響の断定はしない。各ニュース記事にはURLを必ず併記し、省略しない。\
5. 数値は入力表記をそのまま引用する。単位変換・再計算・独自の価格目標算出をしない。\
6. ファンダメンタル値は入力された値だけを使い、利益率、配当利回り、業界平均比較など入力にない数値や比較を新規計算しない。\
7. 指摘や主張は、主語と述語を明確にした断定形で述べる。「可能性がある」「検討が必要」などの曖昧な逃げや、入力に無いことの繰り返しで濁さず、まずユーザーの問いに直接結論を述べる。\
8. 個別の質問に答えるときは、その質問の範囲で答える。問われていないテクニカル指標や分析を無関係に列挙して回答を埋めない（指標の網羅的な提示を求められた場合を除く）。\
9. 指定された見出し、順序、文字数制限を優先する。\
10. 回答は必ず全文を日本語で書く。英語や他言語を混在させない（見出し・箇条書き・結論を含む全文）。";

const SOT_SYSTEM_PROMPT_EN: &str = "\
You are XOKSA's investment analysis assistant LLM. You must follow these constraints:\
1. Stable, well-established facts — a company's ownership structure, parent/subsidiary relationships, and line of business — you may use from your own general knowledge (keep them distinct from XOKSA's confirmed data and make clear they rest on general knowledge). You may also reason with general market, macro, seasonal, and geopolitical concepts. Do not accept the user's statements as fact unconditionally; judge their validity with your own knowledge and flag or withhold on anything wrong or uncertain.\
2. High-velocity, ticker-specific facts (the announcement of a recent partnership or merger itself, actual earnings figures, incidents, guidance, shareholder returns, regulations, specific product information, and the concrete content of news) are treated as fact only when explicitly stated in the input; if absent from the input, do not state them as confirmed facts.\
3. Do not fabricate numbers, amounts, percentages, dates, event names, or news titles not present in the input, or present them as if they were input or confirmed data.\
4. Treat news input as titles and URLs only. Weave the topics and sentiment the headlines signal into the analysis proactively, as the surrounding material environment, even when the question or discussion is technicals-centric. Do not imply that article bodies were read, infer body content, or assert price impact. Always include each news article's URL and never omit it.\
5. Quote numbers exactly as in the input; do not convert units, recalculate, or derive independent price targets.\
6. Use only provided fundamental values; do not make new calculations or comparisons (e.g., profit margins, dividend yields, industry averages) not present in the input.\
7. State findings and claims in clear declarative sentences with an explicit subject and predicate. Do not hedge with \"may be\" or \"needs consideration,\" or pad by repeating what is \"not in the input\"; lead with a direct conclusion to the user's question.\
8. When answering a specific question, answer within that question's scope. Do not pad the answer by enumerating technical indicators or analysis that were not asked about (except when a comprehensive indicator rundown is requested).\
9. Prioritize the specified headings, order, and character limits.\
10. Write your entire response in English. Do not mix in other languages (headings, bullets, and conclusions included).";

/// The SOT reinforcement text for the given UI language (used for every provider).
fn sot_system_prompt(lang: &str) -> &'static str {
    match lang {
        "ja" => SOT_SYSTEM_PROMPT,
        _ => SOT_SYSTEM_PROMPT_EN,
    }
}

/// A final, forceful output-language directive placed as the very last line of an
/// analysis prompt (right before generation). Small local models otherwise drift to
/// English on English-heavy financial tasks despite the earlier instructions.
pub fn final_language_directive(lang: &str) -> &'static str {
    match lang {
        "ja" => "重要：回答は最初から最後まで、見出しも本文も日本語のみで書くこと。",
        _ => "Important: write the entire response, headings and body, start to finish, in English only.",
    }
}

const MAX_RETRIES: u32 = 3;
const FALLBACK_WAIT_SECS: u64 = 60;

/// Ollama model loading (especially a cold, large local model) can take far longer
/// than a cloud round-trip, so when `OLLAMA_TIMEOUT_SECONDS` is unset the ollama
/// request never times out below this floor — the general `llm_timeout_seconds`
/// default (120s) is not enough to load a 20B model on first use. This is a business-
/// rule *minimum* (env fully overrides it when set), NOT a `Config::default` field.
/// It is the SINGLE source for the 300: `setup.rs` writes the `--init` template value
/// from it, and `xoksa.env.sample` is bound to it by a value test — so 300 lives in
/// exactly one place (SOT §4.2).
pub(crate) const OLLAMA_MIN_TIMEOUT_SECS: u64 = 300;

/// Timeout for an ollama request: the explicit `OLLAMA_TIMEOUT_SECONDS` if set,
/// otherwise the general LLM timeout but never below `OLLAMA_MIN_TIMEOUT_SECS`.
fn ollama_timeout_secs(config: &Config) -> u64 {
    // Clamped here as well as where the settings are read: the `max` below can
    // raise the general timeout, so this is the last point at which the value
    // becomes a request timeout. See `config::LLM_TIMEOUT_SECS_MAX`.
    crate::config::clamp_llm_timeout_secs(
        config
            .ollama_timeout_secs
            .unwrap_or_else(|| config.llm_timeout_secs.max(OLLAMA_MIN_TIMEOUT_SECS)),
    )
}

pub fn news_triage_directive(lang: &str) -> String {
    match lang {
        "ja" => "以下の見出し群を、\
                    タイトルとURLだけを根拠に確認優先度 Tier A/B/C に仕分ける。\
                    Tier A（一次性・数量性・直接性・近接性・信頼性が高そうで確認優先度が高い）/ \
                    Tier B（中）/ Tier C（低＝論評・再掲など）。\
                    各記事に対し、確認優先度（高/中/低/参考）を付与し、URLを併記。\
                    Markdown表、罫線、ASCIIアートは禁止。ベタ打ちの短い箇条書きで出力し、URL行の前後には空行を入れる。\
                    コメントは確認観点に限定し、記事本文を読んだ前提の肯定/否定評価、価格影響断定、収益性評価は禁止。\
                    Tier C は最大3件まで、非採用理由を 1 語（再掲/論評/一次性なし 等）で添える。\
                    新規数値の創作は禁止。"
            .to_string(),
        _ => "Classify the following headlines and URLs by confirmation priority only. \
                    Tier A (title appears high in primacy, specificity, directness, recency, or reliability) / \
                    Tier B (medium) / Tier C (low = opinion, repost, etc.). \
                    For each article, assign confirmation priority (high/medium/low/reference) and include the URL. \
                    Do not use Markdown tables, ruled lines, or ASCII-art layouts; use short plain bullet lines. Put a blank line before and after each URL line. \
                    Limit comments to what should be verified; do not imply article bodies were read, assert price impact, or evaluate profitability from the title alone. \
                    Tier C: up to 3 items with a one-word reason (repost/opinion/not primary, etc.). \
                    Do not fabricate new numbers."
            .to_string(),
    }
}

fn shortterm_heading(config: &Config) -> String {
    let lang = config.lang.as_str();
    match (lang, config.analysis_mode) {
        ("ja", AnalysisMode::Daily) => "1週間の短期目線".to_string(),
        (
            "ja",
            AnalysisMode::Intraday1m
            | AnalysisMode::Intraday5m
            | AnalysisMode::Intraday15m
            | AnalysisMode::Intraday30m
            | AnalysisMode::Intraday60m,
        ) => {
            format!("{}ベースの短期目線", config.analysis_mode.bar_label_ja())
        }
        ("ja", AnalysisMode::Weekly) => "週足ベースの中期目線".to_string(),
        ("ja", AnalysisMode::Monthly) => "月足ベースの長期目線".to_string(),
        (_, AnalysisMode::Daily) => "Short-term outlook (1 week)".to_string(),
        (
            _,
            AnalysisMode::Intraday1m
            | AnalysisMode::Intraday5m
            | AnalysisMode::Intraday15m
            | AnalysisMode::Intraday30m
            | AnalysisMode::Intraday60m,
        ) => {
            format!(
                "{} short-term outlook",
                config.analysis_mode.bar_label(lang)
            )
        }
        (_, AnalysisMode::Weekly) => "Weekly-bar mid-term outlook".to_string(),
        (_, AnalysisMode::Monthly) => "Monthly-bar long-term outlook".to_string(),
    }
}

fn midterm_heading(config: &Config) -> &'static str {
    match (config.lang.as_str(), config.analysis_mode) {
        ("ja", AnalysisMode::Daily) => "1ヶ月の中期目線",
        (
            "ja",
            AnalysisMode::Intraday1m
            | AnalysisMode::Intraday5m
            | AnalysisMode::Intraday15m
            | AnalysisMode::Intraday30m
            | AnalysisMode::Intraday60m,
        ) => "数営業日目線",
        ("ja", AnalysisMode::Weekly) => "数週間〜数ヶ月目線",
        ("ja", AnalysisMode::Monthly) => "数ヶ月〜長期目線",
        (_, AnalysisMode::Daily) => "Mid-term outlook (1 month)",
        (
            _,
            AnalysisMode::Intraday1m
            | AnalysisMode::Intraday5m
            | AnalysisMode::Intraday15m
            | AnalysisMode::Intraday30m
            | AnalysisMode::Intraday60m,
        ) => "Several-day outlook",
        (_, AnalysisMode::Weekly) => "Several-week to multi-month outlook",
        (_, AnalysisMode::Monthly) => "Multi-month to long-term outlook",
    }
}

pub fn compose_llm_prompt_lines(
    config: &Config,
    guard: &TechnicalDataGuard,
    news_articles: Option<&[Article]>,
    basic_lines: &[String],
    extension_sections: &[Vec<String>],
    score_lines: &[String],
    fundamental_data: Option<&crate::fundamental::FundamentalData>,
) -> Vec<String> {
    let mut lines = Vec::new();
    let lang = config.lang.as_str();

    match &config.stance {
        Stance::Buyer => {
            match lang {
                "ja" => lines.push("私はこの株を持っておらず購入者を検討しています。買い手の視点でコメントください。".to_string()),
                _ => lines.push("I don't currently hold this stock and am considering buying. Please provide commentary from a buyer's perspective.".to_string()),
            }
            lines.push(String::new());
        }
        Stance::Seller => {
            match lang {
                "ja" => lines.push("私はこの株を売ろうと思っています。売り手の視点でコメントください。".to_string()),
                _ => lines.push("I am considering selling this stock. Please provide commentary from a seller's perspective.".to_string()),
            }
            lines.push(String::new());
        }
        Stance::Holder => {}
    }

    if config.macd_minus_ok {
        match lang {
            "ja" => lines.push("⚠️ MACDがマイナス圏かつシグナルより上回っている場合に、買いシグナルを許容する設定が有効です".to_string()),
            _ => lines.push("⚠️ Setting enabled: buy signals are permitted when MACD is in negative territory but above the signal line.".to_string()),
        }
        lines.push(String::new());
    }

    match lang {
        "ja" => {
            lines.push(format!(
                "📊 銘柄: {}（{}）",
                guard.get_name(),
                guard.get_ticker()
            ));
            if config.analysis_mode != AnalysisMode::Daily {
                lines.push(format!(
                    "🕒 分析モード: {}（{}）",
                    config.analysis_mode.label_ja(),
                    config.analysis_mode.context_ja()
                ));
                let note = if config.analysis_mode.is_intraday() {
                    format!(
                        "この分析は{}による短期分析として解釈してください。",
                        config.analysis_mode.bar_label_ja()
                    )
                } else {
                    format!(
                        "この分析は{}データによる上位足分析として解釈してください。",
                        config.analysis_mode.bar_label_ja()
                    )
                };
                lines.push(note);
            }
            let lbl = crate::utils::freshness_labels(lang);
            lines.push(format!(
                "{}: {}",
                lbl[0],
                crate::utils::format_analysis_time(guard, lang)
            ));
            lines.push(format!(
                "{}: {}",
                lbl[1],
                crate::utils::format_market_data_latest_time(guard, lang)
            ));
            lines.push(format!(
                "{}: {}",
                lbl[2],
                crate::utils::format_latest_observed_price_bar(config, guard)
            ));
            lines.push(format!(
                "{}: {}",
                lbl[3],
                crate::utils::format_indicator_latest_bar(config, guard)
            ));
            match guard.get_latest_observed_price() {
                Some(p) => lines.push(format!("💰 最新取得価格: {:.2}", p)),
                None => {
                    lines.push("💰 最新取得価格: 取得なし".to_string());
                    lines.push(format!("💰 指標計算最終足終値: {:.2}", guard.get_close()));
                }
            }
        }
        _ => {
            lines.push(format!(
                "📊 Ticker: {} ({})",
                guard.get_name(),
                guard.get_ticker()
            ));
            if config.analysis_mode != AnalysisMode::Daily {
                lines.push(format!(
                    "🕒 Analysis mode: {} ({})",
                    config.analysis_mode.label(lang),
                    config.analysis_mode.context(lang)
                ));
                let note = if config.analysis_mode.is_intraday() {
                    format!(
                        "Interpret this analysis as a {}-based short-term analysis.",
                        config.analysis_mode.bar_label(lang)
                    )
                } else {
                    format!(
                        "Interpret this analysis as a higher-timeframe analysis using {} data.",
                        config.analysis_mode.bar_label(lang)
                    )
                };
                lines.push(note);
            }
            let lbl = crate::utils::freshness_labels(lang);
            lines.push(format!(
                "{}: {}",
                lbl[0],
                crate::utils::format_analysis_time(guard, lang)
            ));
            lines.push(format!(
                "{}: {}",
                lbl[1],
                crate::utils::format_market_data_latest_time(guard, lang)
            ));
            lines.push(format!(
                "{}: {}",
                lbl[2],
                crate::utils::format_latest_observed_price_bar(config, guard)
            ));
            lines.push(format!(
                "{}: {}",
                lbl[3],
                crate::utils::format_indicator_latest_bar(config, guard)
            ));
            match guard.get_latest_observed_price() {
                Some(p) => lines.push(format!("💰 Latest fetched price: {:.2}", p)),
                None => {
                    lines.push("💰 Latest fetched price: not available".to_string());
                    lines.push(format!("💰 Indicator bar close: {:.2}", guard.get_close()));
                }
            }
        }
    }
    lines.push(format!(
        "💰 {}: {:.2}",
        config.analysis_mode.previous_close_label_l(&config.lang),
        guard.get_previous_close()
    ));
    let (diff, percent) = crate::utils::displayed_price_diff(guard);
    lines.push(format!(
        "📊 {}: {:+.2} ({:+.2}%)",
        config.analysis_mode.price_diff_label_l(&config.lang),
        diff,
        percent
    ));

    // Volume section
    match guard.get_latest_volume() {
        None => match lang {
            "ja" => {
                lines.push("📈 出来高: データなし（出来高に基づく解釈は行わないこと）".to_string())
            }
            _ => lines.push(
                "📈 Volume: unavailable (do not make volume-based interpretations)".to_string(),
            ),
        },
        Some(vol) => {
            match lang {
                "ja" => lines.push(format!(
                    "📈 最新出来高: {}株",
                    crate::utils::format_volume_display(vol)
                )),
                _ => lines.push(format!(
                    "📈 Latest volume: {} shares",
                    crate::utils::format_volume_display(vol)
                )),
            }
            if let Some(avg) = guard.get_avg_volume() {
                match lang {
                    "ja" => lines.push(format!(
                        "📈 平均出来高 ({}本): {}株",
                        config.sma_long_period,
                        crate::utils::format_volume_display(avg)
                    )),
                    _ => lines.push(format!(
                        "📈 Avg volume ({} bars): {} shares",
                        config.sma_long_period,
                        crate::utils::format_volume_display(avg)
                    )),
                }
            }
            if let Some(ratio) = guard.get_volume_ratio() {
                match lang {
                    "ja" => lines.push(format!("📈 出来高倍率: {:.2}x", ratio)),
                    _ => lines.push(format!("📈 Volume ratio: {:.2}x", ratio)),
                }
                let comment = crate::utils::volume_comment_str(ratio, guard.get_price_diff(), lang);
                match lang {
                    "ja" => lines.push(format!("📈 出来高参考: {}", comment)),
                    _ => lines.push(format!("📈 Volume context: {}", comment)),
                }
            }
            match lang {
                "ja" => lines.push("【出来高注意】出来高は価格変動の強さを補助的に見る材料です。出来高のみで売買判断を断定しないこと。上昇・下落方向は価格変化と合わせて読むこと。".to_string()),
                _    => lines.push("[Volume note] Volume is supplementary context for price momentum. Do not make trade decisions based solely on volume. Always interpret direction alongside price movement.".to_string()),
            }
        }
    }
    lines.push(String::new());

    lines.extend(basic_lines.iter().cloned());
    lines.push(String::new());

    for section in extension_sections {
        lines.extend(section.iter().cloned());
        lines.push(String::new());
    }

    for line in score_lines {
        if !line.is_empty() {
            lines.push(line.clone());
        }
    }
    lines.push(String::new());

    let mut news_task_directive = match lang {
        "ja" => "対象が0件なら『投資判断の確認候補ニュースはありません』と 1 行だけ記載。".to_string(),
        _ => "If no relevant articles, write only one line: 'No investment-relevant news candidates.'".to_string(),
    };

    if !config.no_news {
        match news_articles {
            None => {
                match lang {
                    "ja" => lines.push("【注記】ニュース取得に失敗したためスキップ。".to_string()),
                    _ => lines.push("[Note] News retrieval failed; skipped.".to_string()),
                }
                lines.push(String::new());
                news_task_directive = match lang {
                    "ja" => "この実行ではニュース取得に失敗しスキップ。ニュース節には『取得失敗によりスキップ』と 1 行だけ記載。".to_string(),
                    _ => "News retrieval failed in this run; skipped. In the news section, write only one line: 'Skipped due to retrieval failure.'".to_string(),
                };
            }
            Some([]) => {
                match lang {
                    "ja" => lines.push("【注記】対象期間に該当ニュースなし。".to_string()),
                    _ => lines
                        .push("[Note] No relevant news found for the target period.".to_string()),
                }
                lines.push(String::new());
                news_task_directive = match lang {
                    "ja" => "対象が0件なら『投資判断の確認候補ニュースはありません』と 1 行だけ記載。".to_string(),
                    _ => "If no relevant articles, write only one line: 'No investment-relevant news candidates.'".to_string(),
                };
            }
            Some(slice) => {
                let news_lines = crate::news::compose_news_lines(
                    &crate::news::NewsSubject::from_guard(guard),
                    config,
                    slice,
                );
                lines.extend(news_lines);
                lines.push(String::new());
                news_task_directive = news_triage_directive(lang);
            }
        }
    }

    if let Some(fd) = fundamental_data {
        let fd_lines = crate::fundamental::format_fundamental_for_llm(fd, &config.lang);
        lines.extend(fd_lines);
        lines.push(String::new());
        match lang {
            "ja" => {
                lines.push("【ファンダメンタル補助情報の扱い】".to_string());
                lines.push("- ファンダメンタル情報は四半期〜年次更新であり、短期テクニカルスコアを上書きしない。".to_string());
                lines.push("- PER/PBR/EPS/ROE/売上高等はRust側で計算済みの値のみ使用し、N/Aの項目を推測・補完しない。".to_string());
                lines.push("- テクニカル分析（短期売買タイミング）とファンダメンタル分析（業績・割高割安）の時間軸の違いを明示する。".to_string());
            }
            _ => {
                lines.push("[Fundamental Data Usage Notes]".to_string());
                lines.push("- Fundamental data is updated quarterly to annually and does not override short-term technical scores.".to_string());
                lines.push("- Use only Rust-computed values for P/E, P/B, EPS, ROE, revenue, etc.; do not infer or supplement N/A items.".to_string());
                lines.push("- Clearly distinguish the time horizons of technical analysis (short-term timing) and fundamental analysis (earnings/valuation).".to_string());
            }
        }
        lines.push(String::new());
    }

    let news_criteria = crate::news::news_filter_criteria(lang);
    match lang {
        "ja" => {
            lines.push("【タスク】".to_string());
            lines.push(format!(
                "1. 投資家が注意すべきポイント（{}文字以内）",
                config.max_note_length
            ));
            lines.push(format!(
                "2. {}（{}文字以内）",
                shortterm_heading(config),
                config.max_shortterm_length
            ));
            lines.push(format!(
                "3. {}（{}文字以内）",
                midterm_heading(config),
                config.max_midterm_length
            ));
            if fundamental_data.is_some() {
                lines.push("4. ファンダメンタル補助情報（800字以内）: 業績水準・収益性・割高割安の補助判断・短期テクニカルとの関係を記述。数値は提供されたもののみ使用し、N/Aの項目は推測しない。".to_string());
                lines.push(format!(
                    "5. ニュースハイライト（{}字以内、{}。{}）",
                    config.max_news_length, news_criteria, news_task_directive
                ));
                lines.push(format!("6. 総評（{}字以内）", config.max_review_length));
            } else {
                lines.push(format!(
                    "4. ニュースハイライト（{}字以内、{}。{}）",
                    config.max_news_length, news_criteria, news_task_directive
                ));
                lines.push(format!("5. 総評（{}字以内）", config.max_review_length));
            }
            lines.push("日本語で回答してください。".to_string());
        }
        _ => {
            lines.push("[Tasks]".to_string());
            lines.push(format!(
                "1. Key points for investors (within {} chars)",
                config.max_note_length
            ));
            lines.push(format!(
                "2. {} (within {} chars)",
                shortterm_heading(config),
                config.max_shortterm_length
            ));
            lines.push(format!(
                "3. {} (within {} chars)",
                midterm_heading(config),
                config.max_midterm_length
            ));
            if fundamental_data.is_some() {
                lines.push("4. Fundamental supplementary information (within 800 chars): describe earnings level, profitability, valuation assessment, and relationship to short-term technical analysis. Use only provided values; do not infer N/A items.".to_string());
                lines.push(format!(
                    "5. News highlights (within {} chars, {}. {})",
                    config.max_news_length, news_criteria, news_task_directive
                ));
                lines.push(format!(
                    "6. Overall assessment (within {} chars)",
                    config.max_review_length
                ));
            } else {
                lines.push(format!(
                    "4. News highlights (within {} chars, {}. {})",
                    config.max_news_length, news_criteria, news_task_directive
                ));
                lines.push(format!(
                    "5. Overall assessment (within {} chars)",
                    config.max_review_length
                ));
            }
            lines.push("Respond in English.".to_string());
        }
    }
    lines.push(String::new());

    let macd = guard.get_macd();
    let signal = guard.get_signal();
    match lang {
        "ja" => {
            lines.push("【執筆ガイド】".to_string());
            lines.push(
                "- 前置き・挨拶・メタ説明は出力しない。指定された見出しから直接開始する。"
                    .to_string(),
            );
            lines.push("- 日付・時制は入力に明示された情報のみ使用。不確かなら「〜を控えた局面」等にとどめる。".to_string());
        }
        _ => {
            lines.push("[Writing guide]".to_string());
            lines.push("- Output no preamble or meta-explanation. Start directly from the specified heading.".to_string());
            lines.push("- Use only dates/tenses explicitly stated in the input; when uncertain, use expressions like \"ahead of ~\".".to_string());
        }
    }
    lines.push(
        crate::utils::macd_minus_policy_label(
            config.macd_minus_ok,
            macd < 0.0 && macd > signal,
            &config.lang,
        )
        .to_string(),
    );
    match lang {
        "ja" => {
            lines.push("- ニュース0件時は「テクニカル主導」と明記。件数>0なら冒頭に要点の箇条書きから入る。".to_string());
            lines.push("- 少なくとも2つのシナリオ（例：短期反発/続落/レンジ）を提示し、各々「成立条件と、その場合に注目すべき水準・指標」を整理する。具体的な売買の推奨・エントリー/撤退/利確帯の指示は行わない。".to_string());
            lines.push("- 小数は原則2桁。桁飛び・丸め過ぎ・矛盾記述は禁止。".to_string());
            lines.push("- 誰にも分かりやすくするため指標の略称は禁止。例えば、ボリンジャーバンドと正しく出力し、「BB」というように略称を使わないこと".to_string());
            lines.push("【記述順序ルール】".to_string());
            match config.analysis_mode {
                AnalysisMode::Daily => {
                    lines.push("- 短期、中期の反転条件の順で共に与えられた各種指標の中で条件を想定し説明してください".to_string());
                }
                AnalysisMode::Intraday1m
                | AnalysisMode::Intraday5m
                | AnalysisMode::Intraday15m
                | AnalysisMode::Intraday30m
                | AnalysisMode::Intraday60m => {
                    lines.push(format!("- {}ベースの短期目線、数営業日目線の反転条件の順で、与えられた各種指標から条件を想定し説明してください", config.analysis_mode.bar_label_ja()));
                    lines.push(format!("- {}モードでは日足ベースの中期分析に見える時間軸表現を避け、「{}ベース」「短期レンジ内」「数営業日目線」を使ってください", config.analysis_mode.bar_label_ja(), config.analysis_mode.bar_label_ja()));
                }
                AnalysisMode::Weekly | AnalysisMode::Monthly => {
                    lines.push(format!("- {}ベースの流れ、上位足としての確認条件の順で、与えられた各種指標から条件を想定し説明してください", config.analysis_mode.bar_label_ja()));
                    lines.push(format!("- {}モードでは日足や分足の短期売買判断に寄せすぎず、「{}ベース」「上位足」「時間軸が長い指標解釈」を使ってください", config.analysis_mode.bar_label_ja(), config.analysis_mode.bar_label_ja()));
                }
            }
        }
        _ => {
            lines.push("- When news count is 0, state \"technicals-driven\". When count > 0, open with a bulleted summary of key points.".to_string());
            lines.push("- Present at least 2 scenarios (e.g., short-term rebound / continued decline / range-bound) and for each specify: the condition that would confirm it, and the levels or indicators to watch. Do not recommend specific entry, exit, or profit-taking actions.".to_string());
            lines.push("- Use 2 decimal places as a rule. Skipped digits, excessive rounding, and contradictory statements are prohibited.".to_string());
            lines.push("- For clarity, avoid indicator abbreviations. For example, write \"Bollinger Bands\" in full; do not abbreviate as \"BB\".".to_string());
            lines.push("[Writing order rules]".to_string());
            match config.analysis_mode {
                AnalysisMode::Daily => {
                    lines.push("- In the order of short-term then mid-term reversal conditions, explain assumed conditions using the given indicators.".to_string());
                }
                AnalysisMode::Intraday1m
                | AnalysisMode::Intraday5m
                | AnalysisMode::Intraday15m
                | AnalysisMode::Intraday30m
                | AnalysisMode::Intraday60m => {
                    lines.push(format!("- In the order of {}-based short-term outlook then several-day reversal conditions, explain conditions derived from the given indicators.", config.analysis_mode.bar_label(lang)));
                    lines.push(format!("- In {} mode, avoid time-horizon expressions that resemble daily-bar mid-term analysis; use \"{}-based\", \"within short-term range\", and \"several-day outlook\".", config.analysis_mode.bar_label(lang), config.analysis_mode.bar_label(lang)));
                }
                AnalysisMode::Weekly | AnalysisMode::Monthly => {
                    lines.push(format!("- In the order of {}-based trend then higher-timeframe confirmation conditions, explain conditions derived from the given indicators.", config.analysis_mode.bar_label(lang)));
                    lines.push(format!("- In {} mode, avoid forcing daily or intraday short-term wording; use \"{}-based\", \"higher timeframe\", and longer-horizon indicator interpretation.", config.analysis_mode.bar_label(lang), config.analysis_mode.bar_label(lang)));
                }
            }
        }
    }
    lines.push(String::new());

    if let Some(note) = &config.extra_note {
        if !note.trim().is_empty() {
            match lang {
                "ja" => lines.push(format!("📝 追加ノート: {}", note.trim())),
                _ => lines.push(format!("📝 Additional note: {}", note.trim())),
            }
        }
    }

    lines
}

pub fn save_prompt_to_file(prompt: &str) -> Result<()> {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let filename = format!("debug_prompt_{}.txt", ts);
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&filename)?
    };
    #[cfg(not(unix))]
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&filename)?;
    file.write_all(prompt.as_bytes())?;
    eprintln!("[debug] Prompt saved to {}", filename);
    Ok(())
}

pub struct OpenAiPromptSender;

impl crate::traits::PromptSender for OpenAiPromptSender {
    async fn send_prompt(
        &self,
        config: &Config,
        prompt: &str,
        facts: Option<&crate::integrity::ConfirmedFactSet>,
    ) -> Result<()> {
        send_openai_prompt(config, prompt, facts).await
    }
}

pub struct LlmDispatchSender;

impl crate::traits::PromptSender for LlmDispatchSender {
    async fn send_prompt(
        &self,
        config: &Config,
        prompt: &str,
        facts: Option<&crate::integrity::ConfirmedFactSet>,
    ) -> Result<()> {
        // The SOT reinforcement is delivered as a system message by each provider
        // builder — identically for every provider (single frame, no per-provider
        // divergence). Any model, cloud or local, is held to the same constraints.
        // See design-philosophy / no-prompt-bias.
        if !config.ollama_bench_models.is_empty() {
            return run_ollama_benchmark(config, prompt, facts).await;
        }

        match config.llm_provider.trim() {
            "gemini" => send_gemini_prompt(config, prompt, facts).await,
            "claude" => send_claude_prompt(config, prompt, facts).await,
            "ollama" => send_ollama_prompt(config, prompt, facts).await,
            "openai" => send_openai_prompt(config, prompt, facts).await,
            other => bail!("[LLM] Unsupported provider: {}", other),
        }
    }
}

async fn openai_request(config: &Config, prompt: &str) -> Result<(String, ChatTokenUsage)> {
    let openai_key = crate::utils::resolve_api_key("OPENAI_API_KEY")?.ok_or_else(|| {
        anyhow::anyhow!(
            "[openai] OPENAI_API_KEY is not set. Add it to xoksa.env or use --update-key."
        )
    })?;
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.llm_timeout_secs))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(url) = config.https_proxy.as_deref() {
        builder = builder.proxy(crate::utils::build_proxy(url, config.no_proxy.as_deref())?);
    }
    let client = builder.build()?;
    let sot = sot_system_prompt(&config.lang);

    let mut use_max_completion_tokens = true;
    let mut use_sampling_params = true;

    loop {
        let body = match (use_max_completion_tokens, use_sampling_params) {
            (true, true) => serde_json::json!({
                "model": config.llm_model,
                "messages": [{ "role": "system", "content": sot }, { "role": "user", "content": prompt }],
                "temperature": config.llm_temperature,
                "top_p": config.llm_top_p,
                "max_completion_tokens": config.llm_max_output_tokens,
            }),
            (false, true) => serde_json::json!({
                "model": config.llm_model,
                "messages": [{ "role": "system", "content": sot }, { "role": "user", "content": prompt }],
                "temperature": config.llm_temperature,
                "top_p": config.llm_top_p,
                "max_tokens": config.llm_max_output_tokens,
            }),
            (true, false) => serde_json::json!({
                "model": config.llm_model,
                "messages": [{ "role": "system", "content": sot }, { "role": "user", "content": prompt }],
                "max_completion_tokens": config.llm_max_output_tokens,
            }),
            (false, false) => serde_json::json!({
                "model": config.llm_model,
                "messages": [{ "role": "system", "content": sot }, { "role": "user", "content": prompt }],
                "max_tokens": config.llm_max_output_tokens,
            }),
        };

        let res = client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&*openai_key)
            .json(&body)
            .send()
            .await?;

        if res.status().is_success() {
            drop(openai_key);
            let json: serde_json::Value = res.json().await?;
            let content = json["choices"]
                .get(0)
                .and_then(|c| c["message"]["content"].as_str())
                .ok_or_else(|| anyhow!("[openai] Unexpected API response format"))?
                .to_string();
            let usage = ChatTokenUsage {
                input: json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                output: json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
            };
            return Ok((content, usage));
        }

        let status = res.status();
        let err_body = res.text().await.unwrap_or_default();

        if status.as_u16() == 400 {
            let err_lower = err_body.to_lowercase();
            if use_max_completion_tokens && err_lower.contains("max_completion_tokens") {
                use_max_completion_tokens = false;
                continue;
            }
            if use_sampling_params
                && (err_lower.contains("temperature") || err_lower.contains("top_p"))
            {
                use_sampling_params = false;
                continue;
            }
        }

        drop(openai_key);
        match status.as_u16() {
            400 => eprintln!("❌ [openai] Bad request (400). Check the model name and parameters."),
            401 => eprintln!(
                "❌ [openai] Authentication error (401). API key may be invalid or expired."
            ),
            403 => eprintln!(
                "⛔ [openai] Access denied (403). Insufficient permissions or feature disabled."
            ),
            429 => eprintln!("⏳ [openai] Rate limit (429). Wait and retry."),
            500..=599 => eprintln!("🛠️ [openai] Transient error ({}). Wait and retry.", status),
            _ => eprintln!("❌ [openai] Request failed ({}): {}", status, err_body),
        }
        bail!("[openai] request failed: {}", status);
    }
}

/// Shared analysis-prompt sender for every cloud provider (SOT §4.2 — one body,
/// not one per provider). The only per-provider difference is the request builder,
/// passed in as `request`.
async fn send_cloud_prompt<'a, Fut>(
    config: &'a Config,
    prompt: &'a str,
    request: impl FnOnce(&'a Config, &'a str) -> Fut,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> Result<()>
where
    Fut: std::future::Future<Output = Result<(String, ChatTokenUsage)>> + 'a,
{
    if config.no_llm {
        return Ok(());
    }
    let (content, _) = request(config, prompt).await?;
    println!(
        "\n{}\n",
        crate::chat::llm::llm_commentary_badge(config, &config.lang)
    );
    println!("{}", guard_llm_output(config, prompt, &content, facts));
    Ok(())
}

pub async fn send_openai_prompt(
    config: &Config,
    prompt: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> Result<()> {
    send_cloud_prompt(config, prompt, openai_request, facts).await
}

async fn gemini_request(config: &Config, prompt: &str) -> Result<(String, ChatTokenUsage)> {
    let gemini_key = crate::utils::resolve_api_key("GEMINI_API_KEY")?.ok_or_else(|| {
        anyhow::anyhow!(
            "[gemini] GEMINI_API_KEY is not set. Add it to xoksa.env or use --update-key."
        )
    })?;
    let model = &config.llm_model;
    // The model name is interpolated into the URL path and is user-controllable
    // via `/llm gemini:<model>`. Restrict it to a safe charset so it cannot inject
    // path segments (`/`, `..`) or otherwise manipulate the request URL.
    if model.is_empty()
        || !model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        anyhow::bail!("[gemini] invalid model name (allowed: alphanumeric . _ -)");
    }
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        model
    );
    let body = serde_json::json!({
        "system_instruction": {"parts": [{"text": sot_system_prompt(&config.lang)}]},
        "contents": [{"parts": [{"text": prompt}]}],
        "generationConfig": {
            "temperature": config.llm_temperature,
            "topP": config.llm_top_p,
            "maxOutputTokens": config.llm_max_output_tokens,
        },
    });
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.llm_timeout_secs))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(url) = config.https_proxy.as_deref() {
        builder = builder.proxy(crate::utils::build_proxy(url, config.no_proxy.as_deref())?);
    }
    let client = builder.build()?;

    let res = 'retry: {
        for attempt in 0..MAX_RETRIES {
            let r = client
                .post(&url)
                .header("x-goog-api-key", &*gemini_key)
                .json(&body)
                .send()
                .await?;
            let status_u16 = r.status().as_u16();
            if status_u16 != 429 && status_u16 != 503 {
                break 'retry r;
            }
            let wait_secs = r
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(FALLBACK_WAIT_SECS);
            let label = if status_u16 == 429 {
                "rate limit (429)"
            } else {
                "service overloaded (503)"
            };
            eprintln!(
                "⏳ [gemini] {}. Retrying in {}s... ({}/{})",
                label,
                wait_secs,
                attempt + 1,
                MAX_RETRIES
            );
            if attempt + 1 < MAX_RETRIES {
                tokio::time::sleep(Duration::from_secs(wait_secs)).await;
            } else {
                break 'retry r;
            }
        }
        unreachable!()
    };

    drop(gemini_key);

    if !res.status().is_success() {
        let status = res.status();
        let api_err = res
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|j| j["error"]["message"].as_str().map(|s| s.to_string()))
            .unwrap_or_default();
        let detail = if api_err.is_empty() {
            String::new()
        } else {
            format!(" — {}", api_err)
        };
        match status.as_u16() {
            400 => eprintln!(
                "❌ [gemini] Bad request (400){detail}. Check the model name and parameters."
            ),
            401 | 403 => eprintln!(
                "❌ [gemini] Authentication error ({}){detail}. Check GEMINI_API_KEY.",
                status.as_u16()
            ),
            429 => eprintln!(
                "⏳ [gemini] Rate limit (429). Retry limit ({} attempts) reached. Wait and retry.",
                MAX_RETRIES
            ),
            500..=599 => eprintln!(
                "🛠️ [gemini] Transient error ({}){detail}. Wait and retry.",
                status
            ),
            _ => eprintln!("❌ [gemini] Request failed ({}){detail}", status),
        }
        bail!("[gemini] request failed: {}{}", status, detail);
    }

    let json: serde_json::Value = res.json().await?;
    let content = json["candidates"]
        .get(0)
        .and_then(|c| c["content"]["parts"].as_array())
        .and_then(|parts| {
            let text: String = parts
                .iter()
                .filter(|p| p["thought"].as_bool() != Some(true))
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        })
        .ok_or_else(|| {
            let finish_reason = json["candidates"]
                .get(0)
                .and_then(|c| c["finishReason"].as_str())
                .unwrap_or("no candidates");
            eprintln!(
                "[gemini] Response diagnosis: finishReason={}",
                finish_reason
            );
            if config.debug_prompt {
                eprintln!("[gemini] Response JSON: {}", json);
            }
            anyhow!(
                "[gemini] Unexpected API response format (finishReason={})",
                finish_reason
            )
        })?;
    // A thinking model shares `maxOutputTokens` between its (hidden) reasoning and
    // the answer, so the answer above can be cut off. Gemini reports this only via
    // finishReason — surface it instead of returning a silently truncated reply.
    let content = if json["candidates"]
        .get(0)
        .and_then(|c| c["finishReason"].as_str())
        == Some("MAX_TOKENS")
    {
        eprintln!(
            "⚠️ [gemini] Output token limit reached; response was truncated. \
             Increasing llm_max_output_tokens in xoksa.env may help."
        );
        let note = if config.lang == "ja" {
            "\n\n⚠️ 出力上限に達したため回答が途中で終了しました（xoksa.env の llm_max_output_tokens を増やすと改善します）。"
        } else {
            "\n\n⚠️ Output token limit reached; the reply was cut off (raise llm_max_output_tokens in xoksa.env)."
        };
        format!("{content}{note}")
    } else {
        content
    };
    let usage = ChatTokenUsage {
        input: json["usageMetadata"]["promptTokenCount"]
            .as_u64()
            .unwrap_or(0) as u32,
        output: json["usageMetadata"]["candidatesTokenCount"]
            .as_u64()
            .unwrap_or(0) as u32,
    };
    Ok((content, usage))
}

async fn send_gemini_prompt(
    config: &Config,
    prompt: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> Result<()> {
    send_cloud_prompt(config, prompt, gemini_request, facts).await
}

async fn claude_request(config: &Config, prompt: &str) -> Result<(String, ChatTokenUsage)> {
    let claude_key = crate::utils::resolve_api_key("CLAUDE_API_KEY")?.ok_or_else(|| {
        anyhow::anyhow!(
            "[claude] CLAUDE_API_KEY is not set. Add it to xoksa.env or use --update-key."
        )
    })?;
    let model = &config.llm_model;
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.llm_timeout_secs))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(url) = config.https_proxy.as_deref() {
        builder = builder.proxy(crate::utils::build_proxy(url, config.no_proxy.as_deref())?);
    }
    let client = builder.build()?;

    let mut use_temperature = true;

    loop {
        let body = if use_temperature {
            serde_json::json!({
                "model": model,
                "max_tokens": config.claude_max_tokens,
                "temperature": config.llm_temperature,
                "top_p": config.llm_top_p,
                "system": sot_system_prompt(&config.lang),
                "messages": [{"role": "user", "content": prompt}],
            })
        } else {
            serde_json::json!({
                "model": model,
                "max_tokens": config.claude_max_tokens,
                "system": sot_system_prompt(&config.lang),
                "messages": [{"role": "user", "content": prompt}],
            })
        };

        let res = 'retry: {
            for attempt in 0..MAX_RETRIES {
                let r = client
                    .post("https://api.anthropic.com/v1/messages")
                    .header("x-api-key", &*claude_key)
                    .header("anthropic-version", "2023-06-01")
                    .header("content-type", "application/json")
                    .json(&body)
                    .send()
                    .await?;
                let status_u16 = r.status().as_u16();
                if status_u16 != 429 && status_u16 != 529 && status_u16 != 503 {
                    break 'retry r;
                }
                let wait_secs = r
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(FALLBACK_WAIT_SECS);
                let label = match status_u16 {
                    429 => "rate limit (429)",
                    529 => "overloaded (529)",
                    _ => "service overloaded (503)",
                };
                eprintln!(
                    "⏳ [claude] {}. Retrying in {}s... ({}/{})",
                    label,
                    wait_secs,
                    attempt + 1,
                    MAX_RETRIES
                );
                if attempt + 1 < MAX_RETRIES {
                    tokio::time::sleep(Duration::from_secs(wait_secs)).await;
                } else {
                    break 'retry r;
                }
            }
            unreachable!()
        };

        if res.status().is_success() {
            drop(claude_key);
            let json: serde_json::Value = res.json().await?;
            let stop_reason = json["stop_reason"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            let content = json["content"]
                .as_array()
                .and_then(|blocks| {
                    let text: String = blocks
                        .iter()
                        .filter_map(|b| b["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    if text.is_empty() {
                        None
                    } else {
                        Some(text)
                    }
                })
                .ok_or_else(|| {
                    eprintln!("[claude] Response diagnosis: stop_reason={}", stop_reason);
                    if config.debug_prompt {
                        eprintln!("[claude] Response JSON: {}", json);
                    }
                    anyhow!(
                        "[claude] Unexpected API response format (stop_reason={})",
                        stop_reason
                    )
                })?;
            let content = if stop_reason == "max_tokens" {
                eprintln!(
                    "⚠️ [claude] Output token limit reached; response was truncated. Increasing claude_max_tokens in xoksa.env may help."
                );
                let note = if config.lang == "ja" {
                    "\n\n⚠️ 出力上限に達したため回答が途中で終了しました（xoksa.env の claude_max_tokens を増やすと改善します）。"
                } else {
                    "\n\n⚠️ Output token limit reached; the reply was cut off (raise claude_max_tokens in xoksa.env)."
                };
                format!("{content}{note}")
            } else {
                content
            };
            let usage = ChatTokenUsage {
                input: json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
                output: json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
            };
            return Ok((content, usage));
        }

        let status = res.status();
        let api_message = res
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|b| b["error"]["message"].as_str().map(|s| s.to_string()))
            .unwrap_or_default();

        if use_temperature
            && status.as_u16() == 400
            && api_message.to_lowercase().contains("temperature")
        {
            use_temperature = false;
            continue;
        }

        drop(claude_key);
        match status.as_u16() {
            400 => eprintln!(
                "❌ [claude] Bad request (400): {}",
                if api_message.is_empty() {
                    "Check the model name and parameters.".to_string()
                } else {
                    api_message
                }
            ),
            401 => eprintln!("❌ [claude] Authentication error (401). Check CLAUDE_API_KEY."),
            403 => eprintln!("⛔ [claude] Access denied (403). Insufficient permissions or feature disabled."),
            429 | 529 => eprintln!(
                "⏳ [claude] Rate limit/overloaded. Retry limit ({} attempts) reached. Wait and retry.",
                MAX_RETRIES
            ),
            500..=599 => eprintln!(
                "🛠️ [claude] Transient error ({}). Wait and retry.",
                status
            ),
            _ => eprintln!("❌ [claude] Request failed ({})", status),
        }
        bail!("[claude] request failed: {}", status);
    }
}

async fn send_claude_prompt(
    config: &Config,
    prompt: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> Result<()> {
    send_cloud_prompt(config, prompt, claude_request, facts).await
}

fn format_debug_option<T: std::fmt::Display>(value: Option<T>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "<ollama default>".to_string())
}

fn format_debug_json_value(value: Option<&serde_json::Value>) -> String {
    match value {
        Some(value) if value.is_string() => value.as_str().unwrap_or_default().to_string(),
        Some(value) => value.to_string(),
        None => "<missing>".to_string(),
    }
}

fn format_ollama_duration(value: Option<&serde_json::Value>) -> String {
    value
        .and_then(|v| v.as_u64())
        .map(|ns| format!("{:.2}s", ns as f64 / 1_000_000_000.0))
        .unwrap_or_else(|| "<missing>".to_string())
}

fn resolve_ollama_think(config: &Config) -> Option<&str> {
    config.ollama_think.as_deref()
}

fn ollama_think_json_value(think: &str) -> serde_json::Value {
    match think {
        "true" => serde_json::Value::Bool(true),
        "false" => serde_json::Value::Bool(false),
        _ => serde_json::Value::String(think.to_string()),
    }
}

fn ollama_message_field<'a>(
    json: &'a serde_json::Value,
    field: &str,
) -> Option<&'a serde_json::Value> {
    json.get("message")
        .and_then(|message| message.get(field))
        .or_else(|| json.get(field))
}

fn ollama_text_field_chars(value: Option<&serde_json::Value>) -> usize {
    match value {
        Some(value) if value.is_string() => value.as_str().unwrap_or_default().chars().count(),
        Some(value) if !value.is_null() => value.to_string().chars().count(),
        _ => 0,
    }
}

// Returns None when the field is absent (model does not support thinking).
// Returns Some(n) when the field is present in the API response.
fn ollama_thinking_chars(value: Option<&serde_json::Value>) -> Option<usize> {
    match value {
        None => None,
        Some(v) if v.is_null() => None,
        Some(v) if v.is_string() => Some(v.as_str().unwrap_or_default().chars().count()),
        Some(v) => Some(v.to_string().chars().count()),
    }
}

fn print_ollama_request_debug(
    config: &Config,
    model: &str,
    prompt: &str,
    num_ctx: u32,
    num_predict: i32,
    seed: i64,
    think: Option<&str>,
) {
    eprintln!("\n=== Ollama Debug Request ===");
    eprintln!(
        "endpoint: http://{}:{}/api/chat",
        config.ollama_host, config.ollama_port
    );
    eprintln!("model: {}", model);
    eprintln!("stream: false");
    eprintln!("messages: system (SOT reinforcement) + user (body omitted)");
    eprintln!("user_prompt_chars: {}", prompt.chars().count());
    eprintln!("timeout_seconds: {}", ollama_timeout_secs(config));
    eprintln!(
        "temperature: {}",
        format_debug_option(config.ollama_temperature)
    );
    eprintln!("top_p: {}", format_debug_option(config.ollama_top_p));
    eprintln!("top_k: {}", format_debug_option(config.ollama_top_k));
    eprintln!(
        "repeat_penalty: {}",
        format_debug_option(config.ollama_repeat_penalty)
    );
    eprintln!("num_ctx: {}", num_ctx);
    eprintln!("num_predict: {}", num_predict);
    eprintln!("seed: {}", seed);
    eprintln!("think: {}", think.unwrap_or("<not sent>"));
    eprintln!(
        "keep_alive: {}",
        format_debug_option(config.ollama_keep_alive.as_deref())
    );
}

fn print_ollama_response_debug(json: &serde_json::Value, content: &str, num_ctx: u32) {
    eprintln!("\n=== Ollama Debug Response ===");
    for key in ["model", "created_at", "done", "done_reason"] {
        eprintln!("{}: {}", key, format_debug_json_value(json.get(key)));
    }

    let prompt_eval_count = json.get("prompt_eval_count").and_then(|v| v.as_u64());
    let eval_count = json.get("eval_count").and_then(|v| v.as_u64());
    let eval_duration = json.get("eval_duration").and_then(|v| v.as_u64());
    let thinking_chars = ollama_text_field_chars(ollama_message_field(json, "thinking"));

    eprintln!(
        "total_duration: {}",
        format_ollama_duration(json.get("total_duration"))
    );
    eprintln!(
        "load_duration: {}",
        format_ollama_duration(json.get("load_duration"))
    );
    eprintln!(
        "prompt_eval_duration: {}",
        format_ollama_duration(json.get("prompt_eval_duration"))
    );
    eprintln!(
        "eval_duration: {}",
        format_ollama_duration(json.get("eval_duration"))
    );
    eprintln!(
        "prompt_eval_count: {}",
        format_debug_json_value(json.get("prompt_eval_count"))
    );
    if let Some(count) = prompt_eval_count {
        eprintln!(
            "prompt_ctx_usage: {:.1}% ({}/{})",
            count as f64 / num_ctx as f64 * 100.0,
            count,
            num_ctx
        );
    }
    eprintln!(
        "eval_count: {}",
        format_debug_json_value(json.get("eval_count"))
    );
    if let (Some(count), Some(duration_ns)) = (eval_count, eval_duration) {
        if duration_ns > 0 {
            let tokens_per_sec = count as f64 / (duration_ns as f64 / 1_000_000_000.0);
            eprintln!("eval_tokens_per_second: {:.2}", tokens_per_sec);
        }
    }
    eprintln!("response_chars: {}", content.chars().count());
    eprintln!("thinking_chars: {}", thinking_chars);
}

fn warn_ollama_empty_content_if_needed(json: &serde_json::Value, content: &str, num_predict: i32) {
    let done_reason = json
        .get("done_reason")
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    if done_reason == "length" && content.trim().is_empty() {
        let eval_count = format_debug_json_value(json.get("eval_count"));
        let thinking_chars = ollama_text_field_chars(ollama_message_field(json, "thinking"));
        eprintln!(
            "⚠️ [ollama] Response body is empty. done_reason=length / eval_count={} / num_predict={} / thinking_chars={}. Thinking may have consumed the entire generation budget. Consider --ollama-think false / OLLAMA_THINK=false, or increasing OLLAMA_NUM_PREDICT.",
            eval_count, num_predict, thinking_chars
        );
    }
}

fn should_suppress_ollama_incomplete_response(json: &serde_json::Value) -> bool {
    json.get("done_reason").and_then(|value| value.as_str()) == Some("length")
}

fn warn_ollama_incomplete_response(json: &serde_json::Value, content: &str, num_predict: i32) {
    if !should_suppress_ollama_incomplete_response(json) {
        return;
    }

    let eval_count = format_debug_json_value(json.get("eval_count"));
    let thinking_chars = ollama_text_field_chars(ollama_message_field(json, "thinking"));
    eprintln!(
        "⚠️ [ollama] Response truncated at generation limit. done_reason=length / eval_count={} / num_predict={} / response_chars={} / thinking_chars={}. Suppressing incomplete LLM response.",
        eval_count,
        num_predict,
        content.chars().count(),
        thinking_chars
    );
    eprintln!(
        "  Suggestion: Increase OLLAMA_NUM_PREDICT, set OLLAMA_TEMPERATURE=0.0, or adjust --ollama-think low/false."
    );
}

fn normalize_for_ollama_integrity_check(value: &str) -> String {
    value
        .replace('，', ",")
        .replace('．', ".")
        .replace('％', "%")
        .replace(['　', ' '], "")
}

/// Same normalisation, but spaces are kept (runs collapsed to one). Japanese
/// matching wants spaces gone; English needs them, because word boundaries carry
/// the meaning. The direction check runs against both forms so a single set of
/// patterns covers both languages — the check itself is not duplicated.
fn normalize_keeping_spaces(value: &str) -> String {
    let unified = value
        .replace('，', ",")
        .replace('．', ".")
        .replace('％', "%")
        .replace('　', " ");
    unified.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_numeric_chars(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '０' => '0',
            '１' => '1',
            '２' => '2',
            '３' => '3',
            '４' => '4',
            '５' => '5',
            '６' => '6',
            '７' => '7',
            '８' => '8',
            '９' => '9',
            '，' => ',',
            '．' => '.',
            other => other,
        })
        .collect()
}

fn numeric_value_from_text(value: &str) -> Option<f64> {
    let numeric = RE_NUMERIC_FRAGMENT.find(value)?.as_str();
    normalize_numeric_chars(numeric)
        .replace(',', "")
        .parse::<f64>()
        .ok()
}

fn numeric_values_from_text(value: &str) -> Vec<f64> {
    RE_NUMERIC_FRAGMENT
        .find_iter(value)
        .filter_map(|candidate| {
            normalize_numeric_chars(candidate.as_str())
                .replace(',', "")
                .parse::<f64>()
                .ok()
        })
        .collect()
}

fn prompt_contains_equivalent_number(prompt: &str, number: f64) -> bool {
    let found = RE_NUMERIC_FRAGMENT.find_iter(prompt).any(|candidate| {
        numeric_value_from_text(candidate.as_str())
            .map(|value| (value - number).abs() < 0.005)
            .unwrap_or(false)
    });
    found
}

fn is_numeric_fragment_in_prompt(prompt: &str, prompt_norm: &str, fragment: &str) -> bool {
    let fragment_norm = normalize_for_ollama_integrity_check(fragment);
    let without_about = fragment_norm.strip_prefix('約').unwrap_or(&fragment_norm);
    if prompt_norm.contains(&fragment_norm) || prompt_norm.contains(without_about) {
        return true;
    }

    if fragment_norm.contains("兆円")
        || fragment_norm.contains("億円")
        || fragment_norm.contains("万円")
    {
        return false;
    }

    numeric_value_from_text(fragment)
        .map(|number| prompt_contains_equivalent_number(prompt, number))
        .unwrap_or(false)
}

fn neighbor_chars(value: &str, start: usize, end: usize) -> (Option<char>, Option<char>) {
    let prev = value[..start].chars().rev().find(|ch| !ch.is_whitespace());
    let next = value[end..].chars().find(|ch| !ch.is_whitespace());
    (prev, next)
}

/// The numbers one sentence carries, as `written_numbers` read them.
type NumbersOf = Vec<(std::ops::Range<usize>, crate::integrity::WrittenNumber)>;

/// The numbers of `sentence` that begin inside `span`.
///
/// A number's role — observation, period parameter, date — is decided by the
/// words around it, so it has to be read from the sentence and not from the
/// captured fragment: `since 2019.` is a date only when `since` is visible, and
/// `The 200-day EMA` states a window only when `-day` is. Classifying an
/// isolated fragment made both of them observations with no confirmed
/// counterpart, and the sentence was removed. Each number is then verified at
/// its own position, not at the span's.
fn numbers_in<'a>(
    numbers: &'a NumbersOf,
    span: &'a std::ops::Range<usize>,
) -> impl Iterator<
    Item = (
        &'a std::ops::Range<usize>,
        &'a crate::integrity::WrittenNumber,
    ),
> {
    numbers
        .iter()
        .filter(move |(r, _)| span.start <= r.start && r.start < span.end)
        .map(|(r, n)| (r, n))
}

fn should_skip_bare_numeric_candidate(content: &str, start: usize, end: usize) -> bool {
    let (prev, next) = neighbor_chars(content, start, end);
    matches!(next, Some('円' | '%' | '％' | '年' | '月' | '日' | '株'))
        || matches!(prev, Some('.' | '第'))
}

/// The sentence `pos` falls in, with its start offset. Attribution belongs to the
/// sentence making the claim, not to the whole answer, so verification is done
/// per sentence.
fn sentence_at(content: &str, pos: usize) -> (usize, &str) {
    // One splitter for both languages, shared with the verifier: an English `.`
    // ends a sentence only when whitespace or the end follows, so a decimal point
    // and a ticker's dot do not split one.
    crate::integrity::sentence_bounds(content, pos)
}

/// Verify every number the model wrote against the confirmed fact its own
/// sentence attributes it to: symbol, indicator, unit, magnitude, period, sign.
///
/// This replaces "do these digits occur in the prompt?", which could not tell a
/// citation from a value taken off another symbol, indicator, unit or period.
/// Numbers with no confirmed counterpart at all are left to the unit and
/// bare-price checks, which decide whether such a figure was a claim; this check
/// reports misattribution.
fn validate_confirmed_number_attribution(
    content: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> Vec<String> {
    let Some(f) = facts.filter(|f| !f.is_empty()) else {
        return Vec::new();
    };
    let mut issues = Vec::new();
    // One scan per sentence, shared by every number in it: where the figures,
    // dates and period specifications are is settled once, and the claim
    // splitting and the attribution both read that one result.
    let mut scanned: Option<(usize, crate::integrity::Scan)> = None;
    for (range, number) in crate::integrity::written_numbers(content) {
        let (offset, sentence) = sentence_at(content, range.start);
        if scanned.as_ref().is_none_or(|(o, _)| *o != offset) {
            scanned = Some((offset, crate::integrity::Scan::of(sentence)));
        }
        let scan = &scanned.as_ref().expect("just set").1;
        let local = range.start.saturating_sub(offset)..range.end.saturating_sub(offset);
        // Every verdict that says something about the claim is reported. Only
        // `NotInConfirmedData` is silent: that number was never presented as a
        // reading of a known indicator, so it is not a claim to verify.
        let verdict = f.verify_with(scan, sentence, &local, &number);
        if verdict.is_reportable() {
            issues.push(format!(
                "確定データと対応しない数値: {} ({})",
                content[range.clone()].trim(),
                verdict.reason_ja()
            ));
        }
    }
    issues
}

fn validate_ollama_bare_price_numbers(
    prompt: &str,
    content: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> Vec<String> {
    // The price-context word used to gate this check, and a fabricated level
    // rarely carries one: of twenty sentences a model would plausibly write
    // ("It could reach 250.", "Fair value is 250."), seventeen passed untouched.
    // §1 says this check detects bare price ranges and independently-derived
    // levels, which that gate could not reach. The threshold that remains —
    // three digits and a value of 100 or more, in `RE_BARE_PRICE` — is what
    // keeps ordinary small numbers in prose out of it.
    let mut issues = Vec::new();
    // One read per sentence, shared by every capture in it — the same caching the
    // attribution check uses, and the reason the role is right: `written_numbers`
    // classifies against the whole sentence.
    let mut scanned: Option<(usize, NumbersOf, crate::integrity::Scan)> = None;
    for capture in RE_BARE_PRICE.find_iter(content) {
        if should_skip_bare_numeric_candidate(content, capture.start(), capture.end()) {
            continue;
        }

        let fragment = capture.as_str();
        let values = numeric_values_from_text(fragment);
        if values.is_empty() || values.iter().any(|value| value.abs() < 100.0) {
            continue;
        }

        // R3: a bare price must belong to the indicator its own sentence names.
        if let Some(f) = facts.filter(|f| !f.is_empty()) {
            let (offset, sentence) = sentence_at(content, capture.start());
            if scanned.as_ref().is_none_or(|(o, _, _)| *o != offset) {
                scanned = Some((
                    offset,
                    crate::integrity::written_numbers(sentence),
                    crate::integrity::Scan::of(sentence),
                ));
            }
            let (_, numbers, scan) = scanned.as_ref().expect("just set above");
            let span = capture.start().saturating_sub(offset)..capture.end().saturating_sub(offset);
            let orphan = numbers_in(numbers, &span).any(|(r, n)| {
                f.verify_with(scan, sentence, r, n)
                    == crate::integrity::ClaimVerdict::NotInConfirmedData
            });
            if orphan {
                issues.push(format!("確定データに無い価格・レンジ表現: {}", fragment));
            }
            continue;
        }

        if values
            .iter()
            .all(|value| prompt_contains_equivalent_number(prompt, *value))
        {
            continue;
        }

        issues.push(format!("入力表記にない裸の価格・レンジ表現: {}", fragment));
    }
    issues
}

fn has_trade_action_context(value: &str) -> bool {
    contains_any_term(value, TRADE_ACTION_TERMS)
}

fn validate_ollama_trade_action_price_targets(content: &str) -> Vec<String> {
    if !has_trade_action_context(content) {
        return Vec::new();
    }

    let has_price_sized_number = numeric_values_from_text(content)
        .iter()
        .any(|value| value.abs() >= 100.0);
    if has_price_sized_number {
        return vec!["入力数値の独自売買水準化: エントリー/利確/損切り等".to_string()];
    }

    Vec::new()
}

fn validate_ollama_directional_contradictions(
    prompt: &str,
    content: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> Vec<String> {
    let prompt_norm = normalize_for_ollama_integrity_check(prompt);
    let content_norm = normalize_for_ollama_integrity_check(content);

    let prompt_spaced = normalize_keeping_spaces(prompt);
    let content_spaced = normalize_keeping_spaces(content);
    let any = |re: &Regex, a: &str, b: &str| re.is_match(a) || re.is_match(b);

    // The direction comes from the confirmed values when they are available, so
    // the verdict does not depend on how the prompt happens to word the VWAP
    // line (`VWAP:` vs `VWAP値:`) or on which language wrote it. The sentence
    // patterns are the fallback for a call with no confirmed data.
    let confirmed = facts.and_then(|f| f.compare("vwap", "close"));
    let (prompt_vwap_below, prompt_vwap_above) = match confirmed {
        Some(std::cmp::Ordering::Less) => (true, false),
        Some(std::cmp::Ordering::Greater) => (false, true),
        // Equal: neither direction is claimed, so nothing contradicts.
        Some(std::cmp::Ordering::Equal) => (false, false),
        None => (
            any(&RE_VWAP_PROMPT_BELOW, &prompt_norm, &prompt_spaced),
            any(&RE_VWAP_PROMPT_ABOVE, &prompt_norm, &prompt_spaced),
        ),
    };
    let content_vwap_above = any(&RE_VWAP_CONTENT_ABOVE, &content_norm, &content_spaced);
    let content_vwap_below = any(&RE_VWAP_CONTENT_BELOW, &content_norm, &content_spaced);

    let mut issues = Vec::new();
    if prompt_vwap_below && content_vwap_above {
        issues.push("入力と逆方向のVWAP比較: VWAP上回り".to_string());
    }
    if prompt_vwap_above && content_vwap_below {
        issues.push("入力と逆方向のVWAP比較: VWAP下回り".to_string());
    }
    issues
}

fn validate_ollama_output_integrity(
    prompt: &str,
    content: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> Vec<String> {
    let prompt_norm = normalize_for_ollama_integrity_check(prompt);
    let mut issues = Vec::new();
    let has_facts = facts.is_some_and(|f| !f.is_empty());

    // R3: with confirmed data present, every number is verified against the fact
    // its sentence attributes it to. Without confirmed data there is nothing to
    // attribute against, so the older presence test remains and that limitation
    // is stated in the design document rather than claimed as verification.
    issues.extend(validate_confirmed_number_attribution(content, facts));

    for capture in RE_NUMERIC_UNIT.find_iter(content) {
        let fragment = capture.as_str();
        if has_facts {
            // Misattribution is already reported above; what remains for a
            // unit-carrying figure is having no counterpart at all.
            let (offset, sentence) = sentence_at(content, capture.start());
            let span = capture.start().saturating_sub(offset)..capture.end().saturating_sub(offset);
            let numbers = crate::integrity::written_numbers(sentence);
            let orphan = numbers_in(&numbers, &span).any(|(r, n)| {
                facts.expect("has_facts").verify(sentence, r, n)
                    == crate::integrity::ClaimVerdict::NotInConfirmedData
            });
            if orphan {
                issues.push(format!("確定データに無い数値表現: {}", fragment));
            }
            continue;
        }
        if !is_numeric_fragment_in_prompt(prompt, &prompt_norm, fragment) {
            issues.push(format!("入力表記にない数値表現: {}", fragment));
        }
    }

    issues.extend(validate_ollama_bare_price_numbers(prompt, content, facts));
    issues.extend(validate_ollama_trade_action_price_targets(content));
    issues.extend(validate_ollama_directional_contradictions(
        prompt, content, facts,
    ));

    for term in DERIVED_METRIC_TERMS {
        if content.to_lowercase().contains(&term.to_lowercase())
            && !prompt.to_lowercase().contains(&term.to_lowercase())
        {
            issues.push(format!("入力にない比較・派生指標表現: {}", term));
        }
    }

    issues.sort();
    issues.dedup();
    issues
}

fn warn_ollama_output_integrity_issues(issues: &[String]) {
    if issues.is_empty() {
        return;
    }
    // The exclusion is already surfaced to the user in the chat panel, so this is a
    // file-only record (INFO = no console noise) for later review, with a stable code.
    let extra = if issues.len() > 12 {
        format!(" (+{} more)", issues.len() - 12)
    } else {
        String::new()
    };
    let detail = issues
        .iter()
        .take(12)
        .cloned()
        .collect::<Vec<_>>()
        .join("; ");
    crate::logging::info(
        "XK-LLM-INTEGRITY",
        &format!(
            "ollama output: excluded {} line(s) with out-of-input numbers/comparisons: {}{}",
            issues.len(),
            detail,
            extra
        ),
    );
}

#[derive(Debug)]
struct OllamaResponseSection {
    heading: Option<String>,
    body: Vec<String>,
}

fn ollama_section_number(line: &str) -> Option<u8> {
    let normalized = line.trim().trim_matches('*').trim_start_matches('#').trim();
    let mut chars = normalized.chars();
    let digit = chars.next()?.to_digit(10)?;
    if chars.next()? == '.' {
        return u8::try_from(digit).ok();
    }
    None
}

fn split_ollama_response_sections(content: &str) -> Vec<OllamaResponseSection> {
    let mut sections = Vec::new();
    let mut current = OllamaResponseSection {
        heading: None,
        body: Vec::new(),
    };

    for line in content.lines() {
        if ollama_section_number(line).is_some() {
            if current.heading.is_some() || current.body.iter().any(|line| !line.trim().is_empty())
            {
                sections.push(current);
            }
            current = OllamaResponseSection {
                heading: Some(line.to_string()),
                body: Vec::new(),
            };
        } else {
            current.body.push(line.to_string());
        }
    }

    if current.heading.is_some() || current.body.iter().any(|line| !line.trim().is_empty()) {
        sections.push(current);
    }
    sections
}

/// Split a line into the sentences the guard judges, using the **same** rule the
/// verifier uses (`integrity::sentence_bounds`).
///
/// The two must agree: attribution decides per sentence, and removal excludes per
/// sentence, so a splitter that does not end an English sentence at `.` would
/// drop a correct sentence together with the wrong one next to it. A `.` ends a
/// sentence only when whitespace or the end follows it, so neither a decimal
/// point nor a ticker's dot splits one.
fn split_ollama_line_sentences(line: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut at = 0usize;
    while at < line.len() {
        let (start, _) = crate::integrity::sentence_bounds(line, at);
        let start = start.max(at);
        // The trimmed sentence excludes its terminator; take the raw span up to
        // and including it so the rebuilt line keeps the original text.
        let rest = &line[start..];
        let end = crate::integrity::sentence_end(rest)
            .map(|e| start + e)
            .unwrap_or(line.len());
        if end <= start {
            break;
        }
        let piece = &line[start..end];
        if !piece.trim().is_empty() {
            sentences.push(piece.to_string());
        }
        at = end;
    }
    if sentences.is_empty() && !line.trim().is_empty() {
        sentences.push(line.to_string());
    }
    sentences
}

fn is_list_like_ollama_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("・")
        || trimmed.starts_with("**")
        || RE_LIST_LIKE.is_match(trimmed)
}

/// Describes what happened to a single line during Ollama output sanitization.
#[derive(PartialEq)]
enum RemovalState {
    /// Line passed validation and was kept as-is.
    Kept,
    /// Line had unsafe sentences; remaining safe sentences were kept.
    PartiallyFiltered,
    /// Line was fully removed (list-like, single-sentence, or all sentences unsafe).
    FullyRemoved,
}

fn sanitize_ollama_line(
    prompt: &str,
    line: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> (Option<String>, RemovalState) {
    if line.trim().is_empty() || validate_ollama_output_integrity(prompt, line, facts).is_empty() {
        return (Some(line.to_string()), RemovalState::Kept);
    }

    if line.trim_start().starts_with('|') || is_list_like_ollama_line(line) {
        return (None, RemovalState::FullyRemoved);
    }

    let sentences = split_ollama_line_sentences(line);
    if sentences.len() <= 1 {
        return (None, RemovalState::FullyRemoved);
    }

    let safe_sentences = sentences
        .into_iter()
        .filter(|sentence| validate_ollama_output_integrity(prompt, sentence, facts).is_empty())
        .collect::<Vec<_>>();

    if safe_sentences.is_empty() {
        (None, RemovalState::FullyRemoved)
    } else {
        (
            Some(safe_sentences.join("")),
            RemovalState::PartiallyFiltered,
        )
    }
}

/// Replacement for a heading whose text the integrity guard dropped. The section number is
/// preserved so the response keeps its structure — and so the `has_safe_body` check below still
/// recognises the first line as a heading.
fn excluded_ollama_heading(heading: &str, lang: &str) -> String {
    let number = ollama_section_number(heading)
        .map(|n| format!("{n}. "))
        .unwrap_or_default();
    if lang == "ja" {
        format!("{number}（入力に無い数値・比較を含むため見出しを除外しました）")
    } else {
        format!("{number}(heading excluded: contains out-of-input numbers or comparisons)")
    }
}

fn render_guarded_ollama_section(
    prompt: &str,
    section: OllamaResponseSection,
    lang: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> String {
    let mut lines = Vec::new();
    let mut any_removed = false;
    if let Some(heading) = section.heading {
        // A heading is sanitized like any other line: an out-of-input figure reaches the screen
        // just as readily from a heading as from body text. Previously headings were emitted
        // unchecked, so a fabricated number in one was counted as an issue yet still displayed.
        let (safe_heading, state) = sanitize_ollama_line(prompt, &heading, facts);
        if state != RemovalState::Kept {
            any_removed = true;
        }
        lines.push(safe_heading.unwrap_or_else(|| excluded_ollama_heading(&heading, lang)));
    }

    for line in section.body {
        let (safe_line, state) = sanitize_ollama_line(prompt, &line, facts);
        if state != RemovalState::Kept {
            any_removed = true;
        }
        if let Some(safe_line) = safe_line {
            lines.push(safe_line);
        }
    }

    let has_safe_body = lines
        .iter()
        .skip(usize::from(
            lines
                .first()
                .is_some_and(|line| ollama_section_number(line).is_some()),
        ))
        .any(|line| !line.trim().is_empty());

    if !has_safe_body {
        if lang == "ja" {
            lines.push("- 入力に無い数値・比較を含むため自由記述を全て除外しました。この節に使える内容は残っていません。".to_string());
            lines.push("- 数値・比較は XOKSA 本体の出力を正としてください。".to_string());
        } else {
            lines.push("- All free-text was excluded due to out-of-input numbers or comparisons; no usable content remains for this section.".to_string());
            lines.push(
                "- For numbers and comparisons, refer to the XOKSA main output as authoritative."
                    .to_string(),
            );
        }
    } else if any_removed {
        lines.push(if lang == "ja" {
            "* 入力に無い数値・比較を含む文/行は分析から除外しました。".to_string()
        } else {
            "* Sentences/lines containing out-of-input numbers or comparisons have been excluded from analysis.".to_string()
        });
    }

    lines.join("\n")
}

fn count_ollama_removed_chars(
    prompt: &str,
    content: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> usize {
    let sections = split_ollama_response_sections(content);
    let mut removed = 0usize;
    for section in sections {
        // Headings are sanitized too (see `render_guarded_ollama_section`), so their removal is
        // measured here as well; counting only bodies understated what was withheld.
        for line in section.heading.into_iter().chain(section.body) {
            let (safe_line, state) = sanitize_ollama_line(prompt, &line, facts);
            if state != RemovalState::Kept {
                let original = line.chars().count();
                let safe = safe_line.map_or(0, |s| s.chars().count());
                removed += original.saturating_sub(safe);
            }
        }
    }
    removed
}

fn render_ollama_integrity_fallback(
    prompt: &str,
    content: &str,
    lang: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> String {
    let sections = split_ollama_response_sections(content);
    if sections.is_empty() {
        return if lang == "ja" {
            [
                "LLM の応答には入力に無い数値変換・比較が含まれる可能性があるため、自由記述本文は使用しません。",
                "",
                "安全な取り扱い:",
                "- 数値・価格水準・ファンダメンタル値は、上記の XOKSA 本体出力を正としてください。",
                "- 入力表記を保てないモデルには、ファンダメンタル値・価格目標・派生比較を尋ねないでください。",
            ]
            .join("\n")
        } else {
            [
                "The LLM response may contain numeric conversions or comparisons not present in the input; the free-text body is not used.",
                "",
                "Safe handling:",
                "- For numbers, price levels, and fundamental values, treat the XOKSA main output above as authoritative.",
                "- For models that cannot preserve input notation, do not ask about fundamental values, price targets, or derived comparisons.",
            ]
            .join("\n")
        };
    }

    let mut lines = if lang == "ja" {
        vec![
            "LLM の応答に入力に無い数値変換・比較が含まれていたため、該当する文/行のみ除外しました。".to_string(),
            "入力表記を保った文/行は同じ整合性チェックの下で表示しています。".to_string(),
            String::new(),
        ]
    } else {
        vec![
            "The LLM response contained out-of-input numeric conversions or comparisons; only the affected sentences/lines are excluded.".to_string(),
            "Sentences/lines that preserved input notation are displayed under the same integrity check.".to_string(),
            String::new(),
        ]
    };

    for section in sections {
        lines.push(render_guarded_ollama_section(prompt, section, lang, facts));
        lines.push(String::new());
    }

    lines.join("\n").trim_end().to_string()
}

#[derive(Debug, Clone)]
struct OllamaRequestSettings {
    num_ctx: u32,
    num_predict: i32,
    seed: i64,
    think: Option<String>,
}

#[derive(Debug, Clone)]
struct OllamaChatResult {
    json: serde_json::Value,
    content: String,
    settings: OllamaRequestSettings,
}

fn resolve_ollama_request_settings(config: &Config) -> OllamaRequestSettings {
    OllamaRequestSettings {
        num_ctx: config.ollama_num_ctx,
        num_predict: config.ollama_num_predict,
        seed: config.ollama_seed,
        think: resolve_ollama_think(config).map(ToOwned::to_owned),
    }
}

fn build_ollama_request_body(
    config: &Config,
    model: &str,
    prompt: &str,
    settings: &OllamaRequestSettings,
) -> serde_json::Value {
    let mut options = serde_json::Map::new();
    if let Some(temp) = config.ollama_temperature {
        options.insert("temperature".to_string(), serde_json::json!(temp));
    }
    if let Some(top_p) = config.ollama_top_p {
        options.insert("top_p".to_string(), serde_json::json!(top_p));
    }
    if let Some(top_k) = config.ollama_top_k {
        options.insert("top_k".to_string(), serde_json::json!(top_k));
    }
    if let Some(repeat_penalty) = config.ollama_repeat_penalty {
        options.insert(
            "repeat_penalty".to_string(),
            serde_json::json!(repeat_penalty),
        );
    }
    options.insert("num_ctx".to_string(), serde_json::json!(settings.num_ctx));
    options.insert(
        "num_predict".to_string(),
        serde_json::json!(settings.num_predict),
    );
    options.insert("seed".to_string(), serde_json::json!(settings.seed));

    // The SOT reinforcement is delivered as a system message — identically for every
    // provider (single frame, no per-provider divergence). Small local models obey a
    // system message far more reliably than the same text folded into the user turn.
    let mut body = serde_json::json!({
        "model": model,
        "stream": false,
        "messages": [
            {"role": "system", "content": sot_system_prompt(&config.lang)},
            {"role": "user", "content": prompt}
        ],
    });
    if let Some(think) = settings.think.as_deref() {
        body["think"] = ollama_think_json_value(think);
    }
    if !options.is_empty() {
        body["options"] = serde_json::Value::Object(options);
    }
    if let Some(keep_alive) = &config.ollama_keep_alive {
        body["keep_alive"] = serde_json::json!(keep_alive);
    }
    body
}

/// Upper bound on an Ollama response body, counted **after decompression**.
///
/// `reqwest` 0.13 asks for compressed bodies by default — `ClientBuilder::new`
/// takes `Accepts::default()`, which is every enabled codec — so every client
/// here sends `Accept-Encoding: gzip,br` and inflates what comes back, whether
/// or not the builder says `.gzip(true)`. Measured: a 406 KB gzip body that
/// expands to 202 MB takes the process to 784 MB RSS, because the body is read
/// whole and then parsed. **`Content-Length` cannot prevent that** — it counts
/// the bytes on the wire (406 KB here), so the limit has to count the bytes that
/// arrive decoded. The pre-check below is kept anyway: it costs nothing and
/// stops an oversize *uncompressed* body before a single byte is read.
///
/// Ollama is the path this is reachable on, and the only one bounded here:
/// plaintext HTTP to a host the user configures, which is where a response can
/// come from something other than the server the user meant (security-design
/// §0.2). Every other provider is TLS to a host fixed in code, so those read
/// paths are deliberately left alone.
///
/// Real sizes, measured 2026-10-01 against a local instance: `/api/tags` 2,502
/// bytes for 6 models; `/api/chat` 8,170 bytes at `num_predict` 2,048, so about
/// 33 KB at the engine's default 8,192. 16 MiB leaves two orders of magnitude.
pub(crate) const OLLAMA_MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;

/// Read an Ollama JSON response, refusing one that exceeds
/// [`OLLAMA_MAX_RESPONSE_BYTES`] once decoded. Dropping the response at the
/// limit ends the transfer, so an endless body is not read to its end.
async fn read_ollama_json(res: reqwest::Response) -> Result<serde_json::Value> {
    if let Some(len) = res.content_length() {
        if len > OLLAMA_MAX_RESPONSE_BYTES {
            bail!(
                "[ollama] Response too large ({} bytes declared, limit {}).",
                len,
                OLLAMA_MAX_RESPONSE_BYTES
            );
        }
    }
    let mut res = res;
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = res.chunk().await? {
        if buf.len() as u64 + chunk.len() as u64 > OLLAMA_MAX_RESPONSE_BYTES {
            bail!(
                "[ollama] Response exceeded the {}-byte limit while reading; aborted.",
                OLLAMA_MAX_RESPONSE_BYTES
            );
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&buf)?)
}

async fn post_ollama_request(
    config: &Config,
    client: &reqwest::Client,
    url: &str,
    body: &serde_json::Value,
) -> Result<reqwest::Response> {
    client.post(url).json(body).send().await.map_err(|e| {
        if e.is_connect() {
            anyhow!(
                "[ollama] Connection failed ({}:{}). Verify that Ollama is running.",
                config.ollama_host,
                config.ollama_port
            )
        } else if e.is_timeout() {
            anyhow!(
                "[ollama] Timeout ({}s). Model loading may be taking too long. Try increasing OLLAMA_TIMEOUT_SECONDS or llm_timeout_seconds.",
                ollama_timeout_secs(config)
            )
        } else {
            anyhow!("[ollama] Request error: {}", e)
        }
    })
}

async fn read_ollama_error_response(res: reqwest::Response) -> (reqwest::StatusCode, String) {
    let status = res.status();
    let api_err = read_ollama_json(res)
        .await
        .ok()
        .and_then(|body| body["error"].as_str().map(|value| value.to_string()))
        .unwrap_or_default();
    (status, api_err)
}

fn is_ollama_think_unsupported_error(status: reqwest::StatusCode, api_err: &str) -> bool {
    let normalized = api_err.to_ascii_lowercase();
    status.as_u16() == 400
        && normalized.contains("support")
        && (normalized.contains("think") || normalized.contains("thinking"))
}

fn warn_ollama_request_failure(status: reqwest::StatusCode, model: &str, api_err: &str) {
    match status.as_u16() {
        404 => eprintln!(
            "❌ [ollama] Model not found: {}. Run `ollama pull {}` to download it.",
            model, model
        ),
        500..=599 => eprintln!(
            "🛠️ [ollama] Ollama internal error ({}). {}",
            status,
            if api_err.is_empty() {
                "Check the Ollama log for details.".to_string()
            } else {
                api_err.to_string()
            }
        ),
        _ => eprintln!("❌ [ollama] Request failed ({}): {}", status, api_err),
    }
}

// True when Ollama refused because the model + its KV cache (driven by num_ctx)
// does not fit in memory, e.g. "model requires 65.4 GiB but only 20.8 GiB are
// available" — typically a large num_ctx while another model is still resident.
fn is_ollama_memory_error(status: reqwest::StatusCode, api_err: &str) -> bool {
    let n = api_err.to_ascii_lowercase();
    status.is_server_error()
        && ((n.contains("requires") && n.contains("available"))
            || n.contains("out of memory")
            || n.contains("not enough memory"))
}

// Next smaller context length to retry with, or None once at the floor. The KV
// cache scales with num_ctx, so stepping down is what actually frees memory.
fn next_smaller_num_ctx(current: u32) -> Option<u32> {
    [16384u32, 8192, 4096, 2048]
        .into_iter()
        .find(|&tier| tier < current)
}

async fn request_ollama_chat(
    config: &Config,
    model: &str,
    prompt: &str,
) -> Result<OllamaChatResult> {
    let url = format!(
        "http://{}:{}/api/chat",
        config.ollama_host, config.ollama_port
    );
    let mut settings = resolve_ollama_request_settings(config);
    let timeout_secs = ollama_timeout_secs(config);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    // Retry loop: drop the thinking mode if unsupported, step reasoning effort
    // down (think …→low→false) if thinking ate into the answer, and step num_ctx
    // down on an out-of-memory error. Each branch strictly reduces state (think
    // descends through each level at most once; num_ctx monotonically down to a
    // floor), so the loop always terminates.
    loop {
        let body = build_ollama_request_body(config, model, prompt, &settings);
        if config.debug_ollama {
            print_ollama_request_debug(
                config,
                model,
                prompt,
                settings.num_ctx,
                settings.num_predict,
                settings.seed,
                settings.think.as_deref(),
            );
        }

        let res = post_ollama_request(config, &client, &url, &body).await?;
        if res.status().is_success() {
            let json: serde_json::Value = read_ollama_json(res).await?;
            let content = json["message"]["content"]
                .as_str()
                .ok_or_else(|| anyhow!("[ollama] Unexpected API response format"))?
                .to_string();
            // A reasoning model can spend the `num_predict` budget on hidden
            // thinking and leave the answer empty OR cut off at the generation
            // limit (done_reason=length). Step the reasoning effort down so the
            // answer fits the same budget: none/high/medium → low → false. This
            // covers both an empty answer and a partially-truncated one; the
            // decision is the runtime response, not the model name. Each level is
            // tried at most once (…→low→false→give up), so the loop terminates.
            let thought =
                ollama_thinking_chars(ollama_message_field(&json, "thinking")).unwrap_or(0) > 0;
            if should_suppress_ollama_incomplete_response(&json) && thought {
                let next_think = match settings.think.as_deref() {
                    Some("low") => Some("false"),
                    Some("false") => None,
                    _ => Some("low"),
                };
                if let Some(level) = next_think {
                    eprintln!(
                        "⚠️ [ollama] Response cut off at the generation limit (thinking consumed the budget). Retrying with think={level}."
                    );
                    settings.think = Some(level.to_string());
                    continue;
                }
            }
            if config.debug_ollama {
                print_ollama_response_debug(&json, &content, settings.num_ctx);
            }
            return Ok(OllamaChatResult {
                json,
                content,
                settings,
            });
        }

        let (status, api_err) = read_ollama_error_response(res).await;
        if settings.think.is_some() && is_ollama_think_unsupported_error(status, &api_err) {
            eprintln!(
                "⚠️ [ollama] {} does not support the specified thinking mode. Retrying without think.",
                model
            );
            settings.think = None;
            continue;
        }
        if is_ollama_memory_error(status, &api_err) {
            if let Some(smaller) = next_smaller_num_ctx(settings.num_ctx) {
                eprintln!(
                    "⚠️ [ollama] Not enough memory for num_ctx={} ({}). Retrying with num_ctx={}.",
                    settings.num_ctx, api_err, smaller
                );
                settings.num_ctx = smaller;
                continue;
            }
        }
        warn_ollama_request_failure(status, model, &api_err);
        let detail = if api_err.is_empty() {
            String::new()
        } else {
            format!(" — {}", api_err)
        };
        bail!("[ollama] request failed: {}{}", status, detail);
    }
}

#[derive(Debug, Clone)]
struct OllamaBenchmarkRow {
    model: String,
    status: String,
    done_reason: String,
    prompt_eval_count: Option<u64>,
    num_ctx: Option<u32>,
    prompt_ctx_usage_pct: Option<f64>,
    eval_count: Option<u64>,
    total_duration_secs: Option<f64>,
    eval_tokens_per_second: Option<f64>,
    response_chars: usize,
    thinking_chars: Option<usize>,
    integrity_issue_count: usize,
    removed_chars: usize,
    error: Option<String>,
}

fn json_u64(json: &serde_json::Value, key: &str) -> Option<u64> {
    json.get(key).and_then(|value| value.as_u64())
}

fn json_duration_secs(json: &serde_json::Value, key: &str) -> Option<f64> {
    json_u64(json, key).map(|ns| ns as f64 / 1_000_000_000.0)
}

fn format_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn format_optional_f64(value: Option<f64>, digits: usize) -> String {
    value
        .map(|value| format!("{value:.digits$}"))
        .unwrap_or_else(|| "-".to_string())
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn resolve_ollama_benchmark_models(config: &Config) -> Result<Vec<String>> {
    let candidates = if config.ollama_bench_models.is_empty() {
        if config.llm_provider.trim() == "ollama" {
            vec![config.llm_model.clone()]
        } else {
            Vec::new()
        }
    } else {
        config.ollama_bench_models.clone()
    };

    let mut models = Vec::new();
    for candidate in candidates {
        let model = candidate.trim();
        if !model.is_empty() && !models.iter().any(|existing| existing == model) {
            models.push(model.to_string());
        }
    }

    if models.is_empty() {
        bail!(
            "[ollama benchmark] No model specified. Use --ollama-bench <models> or --llm-provider ollama --llm-model."
        );
    }

    Ok(models)
}

fn build_ollama_benchmark_row(
    model: &str,
    prompt: &str,
    result: OllamaChatResult,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> OllamaBenchmarkRow {
    let json = &result.json;
    let content = result.content.as_str();
    let prompt_eval_count = json_u64(json, "prompt_eval_count");
    let eval_count = json_u64(json, "eval_count");
    let eval_duration_secs = json_duration_secs(json, "eval_duration");
    let eval_tokens_per_second = match (eval_count, eval_duration_secs) {
        (Some(count), Some(duration)) if duration > 0.0 => Some(count as f64 / duration),
        _ => None,
    };
    let prompt_ctx_usage_pct =
        prompt_eval_count.map(|count| count as f64 / result.settings.num_ctx as f64 * 100.0);
    let done_reason = json
        .get("done_reason")
        .and_then(|value| value.as_str())
        .unwrap_or("<missing>")
        .to_string();
    let thinking_chars = ollama_thinking_chars(ollama_message_field(json, "thinking"));
    let integrity_issues = validate_ollama_output_integrity(prompt, content, facts);
    // `status` reports completion only. Whether the guard removed anything is an orthogonal
    // axis, carried by `integrity_issue_count` / `removed_chars`: a detection is not a defect,
    // and folding it in here made an answer that states nothing score best.
    let status = if should_suppress_ollama_incomplete_response(json) {
        "length"
    } else {
        "ok"
    };
    let response_chars = content.chars().count();
    // Measured independently of `status`. Gating this on the old "guarded" value discarded the
    // removal count for truncated answers, which still carry whatever the guard found.
    let removed_chars = count_ollama_removed_chars(prompt, content, facts);

    OllamaBenchmarkRow {
        model: model.to_string(),
        status: status.to_string(),
        done_reason,
        prompt_eval_count,
        num_ctx: Some(result.settings.num_ctx),
        prompt_ctx_usage_pct,
        eval_count,
        total_duration_secs: json_duration_secs(json, "total_duration"),
        eval_tokens_per_second,
        response_chars,
        thinking_chars,
        integrity_issue_count: integrity_issues.len(),
        removed_chars,
        error: None,
    }
}

fn build_ollama_benchmark_error_row(model: &str, error: anyhow::Error) -> OllamaBenchmarkRow {
    OllamaBenchmarkRow {
        model: model.to_string(),
        status: "error".to_string(),
        done_reason: "-".to_string(),
        prompt_eval_count: None,
        num_ctx: None,
        prompt_ctx_usage_pct: None,
        eval_count: None,
        total_duration_secs: None,
        eval_tokens_per_second: None,
        response_chars: 0,
        thinking_chars: None,
        integrity_issue_count: 0,
        removed_chars: 0,
        error: Some(error.to_string()),
    }
}

fn print_ollama_benchmark_table(rows: &[OllamaBenchmarkRow]) {
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("--- {} ---", row.model);
        println!("status        : {}", row.status);
        println!("done_reason   : {}", row.done_reason);
        println!(
            "ctx_usage     : {}% ({}/{})",
            format_optional_f64(row.prompt_ctx_usage_pct, 1),
            format_optional_u64(row.prompt_eval_count),
            format_optional_u64(row.num_ctx.map(|value| value as u64)),
        );
        println!("eval_tokens   : {}", format_optional_u64(row.eval_count));
        println!(
            "speed         : {} tok/s",
            format_optional_f64(row.eval_tokens_per_second, 2)
        );
        println!(
            "duration      : {} sec",
            format_optional_f64(row.total_duration_secs, 2)
        );
        println!(
            "chars         : response={} / thinking={}",
            row.response_chars,
            row.thinking_chars
                .map(|n| n.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
        );
        // Share of what the model actually wrote that never reached the screen. An operational
        // fact, not a score: an answer stating nothing would score 0% here.
        let detected = if row.response_chars > 0 {
            format!(
                "{:.1}%",
                row.removed_chars as f64 / row.response_chars as f64 * 100.0
            )
        } else {
            "-".to_string()
        };
        println!(
            "integrity     : removed={} / {} issue(s) (detected {})",
            row.removed_chars, row.integrity_issue_count, detected
        );
        if let Some(error) = &row.error {
            println!("error         : {}", error);
        }
    }
}

fn print_ollama_benchmark_csv(rows: &[OllamaBenchmarkRow]) {
    println!(
        "model,status,done_reason,prompt_eval_count,num_ctx,prompt_ctx_usage_pct,eval_count,total_duration_secs,eval_tokens_per_second,response_chars,thinking_chars,integrity_issue_count,removed_chars,error"
    );
    for row in rows {
        println!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            csv_escape(&row.model),
            csv_escape(&row.status),
            csv_escape(&row.done_reason),
            format_optional_u64(row.prompt_eval_count),
            format_optional_u64(row.num_ctx.map(|value| value as u64)),
            format_optional_f64(row.prompt_ctx_usage_pct, 1),
            format_optional_u64(row.eval_count),
            format_optional_f64(row.total_duration_secs, 2),
            format_optional_f64(row.eval_tokens_per_second, 2),
            row.response_chars,
            row.thinking_chars
                .map(|n| n.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            row.integrity_issue_count,
            row.removed_chars,
            csv_escape(row.error.as_deref().unwrap_or("")),
        );
    }
}

fn print_ollama_benchmark_json(rows: &[OllamaBenchmarkRow]) -> Result<()> {
    let rows = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "model": row.model,
                "status": row.status,
                "done_reason": row.done_reason,
                "prompt_eval_count": row.prompt_eval_count,
                "num_ctx": row.num_ctx,
                "prompt_ctx_usage_pct": row.prompt_ctx_usage_pct,
                "eval_count": row.eval_count,
                "total_duration_secs": row.total_duration_secs,
                "eval_tokens_per_second": row.eval_tokens_per_second,
                "response_chars": row.response_chars,
                "thinking_chars": row.thinking_chars,
                "integrity_issue_count": row.integrity_issue_count,
                "removed_chars": row.removed_chars,
                "error": row.error,
            })
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "benchmark": "ollama",
            "rows": rows,
        }))?
    );
    Ok(())
}

fn print_ollama_benchmark_rows(config: &Config, rows: &[OllamaBenchmarkRow]) -> Result<()> {
    match config.ollama_bench_format.as_str() {
        "csv" => print_ollama_benchmark_csv(rows),
        "json" => print_ollama_benchmark_json(rows)?,
        _ => print_ollama_benchmark_table(rows),
    }
    Ok(())
}

async fn run_ollama_benchmark(
    config: &Config,
    prompt: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> Result<()> {
    let models = resolve_ollama_benchmark_models(config)?;
    eprintln!(
        "[ollama benchmark] models={} prompt_chars={} format={}",
        models.len(),
        prompt.chars().count(),
        config.ollama_bench_format
    );

    let mut rows = Vec::new();
    for (index, model) in models.iter().enumerate() {
        eprintln!(
            "[ollama benchmark] running {}/{}: {}",
            index + 1,
            models.len(),
            model
        );
        match request_ollama_chat(config, model, prompt).await {
            Ok(result) => {
                warn_ollama_empty_content_if_needed(
                    &result.json,
                    &result.content,
                    result.settings.num_predict,
                );
                rows.push(build_ollama_benchmark_row(model, prompt, result, facts));
            }
            Err(error) => rows.push(build_ollama_benchmark_error_row(model, error)),
        }
    }

    println!("\n=== Ollama Benchmark ===\n");
    print_ollama_benchmark_rows(config, &rows)?;
    Ok(())
}

async fn send_ollama_prompt(
    config: &Config,
    prompt: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> Result<()> {
    let model = config.llm_model.trim();
    if model.is_empty() {
        bail!("[ollama] Model name is not set. Specify a model via --llm-model or llm_model / ollama_model in xoksa.env.");
    }

    let result = request_ollama_chat(config, model, prompt).await?;
    let json = &result.json;
    let content = result.content.as_str();

    warn_ollama_empty_content_if_needed(json, content, result.settings.num_predict);
    if should_suppress_ollama_incomplete_response(json) {
        warn_ollama_incomplete_response(json, content, result.settings.num_predict);
        println!(
            "\n{}\n",
            crate::chat::llm::llm_commentary_badge_stated(
                config,
                &config.lang,
                "抑止: 不完全な回答",
                "suppressed: incomplete response"
            )
        );
        println!(
            "The Ollama response was truncated at the generation limit and has been suppressed. Check done_reason / eval_count / thinking_chars via --debug-ollama."
        );
        return Ok(());
    }
    if config.no_ollama_guard {
        eprintln!(
            "⚠️ [ollama] Integrity check disabled (--no-ollama-guard / OLLAMA_NO_GUARD=true). Numeric conversion, price ranges, and trade-target detection are off. Cross-check output against XOKSA main display."
        );
        println!(
            "\n{}\n",
            crate::chat::llm::llm_commentary_badge_stated(
                config,
                &config.lang,
                "ガード無効",
                "unguarded"
            )
        );
    } else {
        let integrity_issues = validate_ollama_output_integrity(prompt, content, facts);
        if !integrity_issues.is_empty() {
            warn_ollama_output_integrity_issues(&integrity_issues);
            println!(
                "\n{}\n",
                crate::chat::llm::llm_commentary_badge_stated(
                    config,
                    &config.lang,
                    "ガード: 整合性チェック",
                    "guarded: integrity check"
                )
            );
            println!(
                "{}",
                render_ollama_integrity_fallback(prompt, content, &config.lang, facts)
            );
            return Ok(());
        }
        println!(
            "\n{}\n",
            crate::chat::llm::llm_commentary_badge(config, &config.lang)
        );
    }
    println!("{}", content);
    Ok(())
}

// --- Chat turn functions (return String instead of printing) ---

pub async fn send_chat_turn(config: &Config, prompt: &str) -> Result<String> {
    send_chat_turn_facts(config, prompt, None).await
}

/// Same, with the confirmed data the caller holds. Passing it is what lets the
/// output guard verify attribution rather than mere presence.
pub async fn send_chat_turn_facts(
    config: &Config,
    prompt: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> Result<String> {
    send_chat_turn_with_usage(config, prompt, facts)
        .await
        .map(|(s, _)| s)
}

/// Output-integrity guard, provider-agnostic. Confirmed values (price/score/each
/// indicator/volume) live in the prompt, so any out-of-input number, bare price
/// range, independent trade level, or reversed comparison in the output is a
/// hallucination or an injection effect — detected here and the offending
/// sentence/line excluded. Runs for EVERY provider (defense-in-depth), gated by
/// `no_ollama_guard` (the flag that disables the integrity check).
fn guard_llm_output(
    config: &Config,
    prompt: &str,
    content: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> String {
    if config.no_ollama_guard {
        return content.to_string();
    }
    let integrity_issues = validate_ollama_output_integrity(prompt, content, facts);
    if integrity_issues.is_empty() {
        return content.to_string();
    }
    warn_ollama_output_integrity_issues(&integrity_issues);
    render_ollama_integrity_fallback(prompt, content, &config.lang, facts)
}

/// Provider dispatch for a chat turn — the single table mapping provider→builder,
/// shared by `send_chat_turn_with_usage` and `alert_explain_note` (SOT §4.2). Returns
/// `None` for an unknown provider so each caller applies its own policy (bail vs.
/// silently skip).
async fn send_chat_turn_dispatch(
    config: &Config,
    prompt: &str,
) -> Option<Result<(String, ChatTokenUsage)>> {
    Some(match config.llm_provider.trim() {
        "gemini" => send_chat_turn_gemini(config, prompt).await,
        "claude" => send_chat_turn_claude(config, prompt).await,
        "ollama" => send_chat_turn_ollama(config, prompt).await,
        "openai" => send_chat_turn_openai(config, prompt).await,
        _ => return None,
    })
}

pub async fn send_chat_turn_with_usage(
    config: &Config,
    prompt: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> Result<(String, ChatTokenUsage)> {
    // The SOT reinforcement is delivered as a system message by each provider
    // builder (identical for every provider; mirrors `LlmDispatchSender`).
    let (content, usage) = match send_chat_turn_dispatch(config, prompt).await {
        Some(result) => result?,
        None => bail!("[LLM] Unsupported provider: {}", config.llm_provider.trim()),
    };
    // Apply the integrity guard uniformly to every provider's output.
    Ok((guard_llm_output(config, prompt, &content, facts), usage))
}

/// A short, §1-guarded plain-text note for an alert EXPLAIN. Returns `None` when
/// the LLM call fails, the reply is empty, or it introduces anything absent from
/// `prompt` — a fabricated note is DROPPED, never sent (containment appropriate
/// for a push notification, stricter than the chat fallback which keeps the safe
/// sentences). `prompt` already carries every confirmed number, so the integrity
/// guard has its reference. The result is one trimmed line, length-capped.
pub async fn alert_explain_note(
    config: &Config,
    prompt: &str,
    facts: Option<&crate::integrity::ConfirmedFactSet>,
) -> Option<String> {
    let content = send_chat_turn_dispatch(config, prompt).await?.ok()?.0;

    if !config.no_ollama_guard {
        let issues = validate_ollama_output_integrity(prompt, &content, facts);
        if !issues.is_empty() {
            warn_ollama_output_integrity_issues(&issues);
            return None;
        }
    }

    let line = content.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(line.chars().take(200).collect())
}

async fn send_chat_turn_openai(config: &Config, prompt: &str) -> Result<(String, ChatTokenUsage)> {
    openai_request(config, prompt).await
}

async fn send_chat_turn_gemini(config: &Config, prompt: &str) -> Result<(String, ChatTokenUsage)> {
    gemini_request(config, prompt).await
}

async fn send_chat_turn_claude(config: &Config, prompt: &str) -> Result<(String, ChatTokenUsage)> {
    claude_request(config, prompt).await
}

async fn send_chat_turn_ollama(config: &Config, prompt: &str) -> Result<(String, ChatTokenUsage)> {
    let model = config.llm_model.trim();
    if model.is_empty() {
        bail!("[ollama] Model name is not set. Specify a model via --llm-model or llm_model / ollama_model in xoksa.env.");
    }

    let result = request_ollama_chat(config, model, prompt).await?;
    let json = &result.json;
    let content = result.content.as_str();
    let usage = ChatTokenUsage {
        input: json
            .get("prompt_eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        output: json.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
    };

    warn_ollama_empty_content_if_needed(json, content, result.settings.num_predict);

    if should_suppress_ollama_incomplete_response(json) {
        warn_ollama_incomplete_response(json, content, result.settings.num_predict);
        return Ok((
            "The Ollama response was truncated at the generation limit. Consider increasing OLLAMA_NUM_PREDICT."
                .to_string(),
            usage,
        ));
    }

    // The output-integrity guard is applied uniformly by `send_chat_turn_with_usage`
    // for every provider — Ollama no longer guards here (avoids double application).
    Ok((content.to_string(), usage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AnalysisMode;

    // --no-llm must short-circuit every cloud provider before any network call.
    // Regression for the v1.6.4 fix that added the early return to gemini/claude.
    #[tokio::test]
    async fn no_llm_skips_network_for_all_providers() {
        let config = crate::config::Config {
            no_llm: true,
            ..crate::config::Config::default()
        };
        assert!(send_openai_prompt(&config, "p", None).await.is_ok());
        assert!(send_gemini_prompt(&config, "p", None).await.is_ok());
        assert!(send_claude_prompt(&config, "p", None).await.is_ok());
    }

    fn sample_guard() -> TechnicalDataGuard {
        let mut guard = TechnicalDataGuard::new("SPY".to_string(), "2026-05-12".to_string());
        guard.set_name("SPY");
        guard.set_close(100.0);
        guard.set_previous_close(99.0);
        guard
    }

    fn intraday_guard_with_market_data_latest_time() -> TechnicalDataGuard {
        let mut guard = TechnicalDataGuard::new("3774.T".to_string(), "2026-05-12".to_string());
        guard.set_name("インターネットイニシアティブ");
        guard.set_datetime("2026-05-12 13:30");
        guard.set_latest_observed_price(2955.50);
        guard.set_market_data_latest_time("2026-05-12 14:11");
        guard.set_previous_close(2953.50);
        guard
    }

    #[test]
    fn sot_system_prompt_forbids_ticker_fact_fabrication_but_allows_reasoning() {
        // Applied identically to every provider (via the dispatcher), not ollama-only.
        // The guard forbids fabricating ticker-specific facts, yet permits general
        // reasoning (the "safety scissor that can't cut" fix).
        assert!(SOT_SYSTEM_PROMPT
            .contains("確立して変わりにくい事実は、あなた自身の一般知識で用いてよい"));
        assert!(SOT_SYSTEM_PROMPT.contains("ユーザーの発言は無条件に事実として飲まず"));
        assert!(SOT_SYSTEM_PROMPT.contains("速報性・個別性の高い事実"));
        assert!(SOT_SYSTEM_PROMPT.contains("主語と述語を明確にした断定形"));
        assert!(SOT_SYSTEM_PROMPT.contains("問われていないテクニカル指標"));
        assert!(SOT_SYSTEM_PROMPT_EN.contains("reason with general market"));
        assert!(SOT_SYSTEM_PROMPT_EN.contains("declarative sentences"));
        assert!(SOT_SYSTEM_PROMPT.contains("タイトルとURLのみ"));
        assert!(SOT_SYSTEM_PROMPT.contains("記事本文を読んだ前提"));
        assert!(SOT_SYSTEM_PROMPT.contains("新規計算しない"));
        assert!(SOT_SYSTEM_PROMPT.contains("数値は入力表記をそのまま引用する"));
        assert!(SOT_SYSTEM_PROMPT.contains("単位変換"));
        // News URL must always be included (never omitted) — both languages.
        assert!(SOT_SYSTEM_PROMPT.contains("URLを必ず併記"));
        assert!(SOT_SYSTEM_PROMPT_EN.contains("include each news article's URL"));
        assert_eq!(sot_system_prompt("ja"), SOT_SYSTEM_PROMPT);
        assert_eq!(sot_system_prompt("en"), SOT_SYSTEM_PROMPT_EN);
    }

    #[test]
    fn ollama_timeout_floors_at_300_when_unset() {
        // No OLLAMA_TIMEOUT_SECONDS + a low general timeout → floored at 300s
        // (a cold local model needs more than the 120s llm default).
        let c = Config {
            ollama_timeout_secs: None,
            llm_timeout_secs: 120,
            ..Config::default()
        };
        assert_eq!(ollama_timeout_secs(&c), 300);
        // A higher general timeout is respected.
        let c = Config {
            ollama_timeout_secs: None,
            llm_timeout_secs: 600,
            ..Config::default()
        };
        assert_eq!(ollama_timeout_secs(&c), 600);
        // An explicit override wins, even below the floor.
        let c = Config {
            ollama_timeout_secs: Some(90),
            llm_timeout_secs: 120,
            ..Config::default()
        };
        assert_eq!(ollama_timeout_secs(&c), 90);
    }

    /// A permit is held for as long as its handler runs and nothing on the server
    /// side cuts a handler short, so an unbounded timeout could pin one of the
    /// 256 permits indefinitely. Every surface that supplies a timeout is capped.
    #[test]
    fn a_timeout_cannot_exceed_the_ceiling_whichever_surface_sets_it() {
        let max = crate::config::LLM_TIMEOUT_SECS_MAX;
        // Set explicitly for ollama.
        let c = Config {
            ollama_timeout_secs: Some(86_400),
            llm_timeout_secs: 120,
            ..Config::default()
        };
        assert_eq!(ollama_timeout_secs(&c), max);
        // Reached through the general timeout, which the floor below can raise.
        let c = Config {
            ollama_timeout_secs: None,
            llm_timeout_secs: 86_400,
            ..Config::default()
        };
        assert_eq!(ollama_timeout_secs(&c), max);
        // A value under the ceiling is untouched — this caps, it does not pin.
        let c = Config {
            ollama_timeout_secs: Some(600),
            llm_timeout_secs: 120,
            ..Config::default()
        };
        assert_eq!(ollama_timeout_secs(&c), 600);
    }

    #[test]
    fn news_triage_directive_uses_confirmation_priority_not_price_impact() {
        let ja = news_triage_directive("ja");
        let en = news_triage_directive("en");

        assert!(ja.contains("確認優先度"));
        assert!(ja.contains("URL"));
        assert!(ja.contains("記事本文を読んだ前提"));
        assert!(ja.contains("Markdown表"));
        assert!(ja.contains("ベタ打ち"));
        assert!(!ja.contains("価格影響度"));

        assert!(en.contains("confirmation priority"));
        assert!(en.contains("URLs"));
        assert!(en.contains("article bodies"));
        assert!(en.contains("Markdown tables"));
        assert!(en.contains("plain bullet"));
        assert!(!en.contains("price impact ("));
    }

    /// Serve a response body on loopback and hand it to `read_ollama_json`.
    /// `headers` is sent verbatim, then `body_blocks` 64 KiB blocks of filler,
    /// then the connection closes — so the reader sees a body whose end is the
    /// connection, which is what a chunked or compressed response also gives it.
    async fn ollama_response_from(
        headers: &'static str,
        body_blocks: u64,
    ) -> Result<serde_json::Value> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            // Read the request before answering. Closing a socket that still has
            // unread bytes in its receive buffer makes the kernel send RST rather
            // than FIN, and an RST discards whatever response is still in flight —
            // measured as a 1-in-30 failure of the test below before this read.
            let mut scratch = [0u8; 1024];
            let _ = sock.read(&mut scratch).await;
            if sock.write_all(headers.as_bytes()).await.is_err() {
                return;
            }
            let block = vec![b'A'; 64 * 1024];
            for _ in 0..body_blocks {
                if sock.write_all(&block).await.is_err() {
                    return;
                }
            }
            // Hold the socket open long enough for a reader that stops at the
            // headers (the declared-length case) to make its decision.
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        let res = reqwest::Client::new()
            .get(format!("http://{addr}/api/chat"))
            .send()
            .await
            .expect("response headers");
        read_ollama_json(res).await
    }

    /// The limit counts bytes as they arrive. That is the only thing that bounds
    /// a body whose declared length says nothing about its size — one ended by
    /// the connection, a chunked one, or a compressed one that inflates
    /// (measured: 406 KB of gzip expands to 202 MB and takes the process to
    /// 784 MB RSS). One block past the cap must be refused mid-read.
    #[tokio::test]
    async fn an_ollama_response_past_the_cap_is_refused_while_reading() {
        let blocks = OLLAMA_MAX_RESPONSE_BYTES / (64 * 1024) + 1;
        let err = ollama_response_from(
            "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n",
            blocks,
        )
        .await
        .expect_err("a body past the cap must be refused");
        assert!(
            err.to_string().contains("exceeded"),
            "expected the read to stop at the limit, got: {err}"
        );
    }

    /// A declared length over the cap is refused before the body is read at all
    /// — free, and it does not depend on the sender actually sending that much.
    #[tokio::test]
    async fn an_ollama_response_declaring_more_than_the_cap_is_refused_unread() {
        let err = ollama_response_from(
            "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nContent-Length: 1073741824\r\n\r\n",
            0,
        )
        .await
        .expect_err("a declared length over the cap must be refused");
        assert!(
            err.to_string().contains("too large"),
            "expected the declared length to be refused, got: {err}"
        );
    }

    /// The cap must not change what an ordinary response does. A real
    /// `/api/chat` body measured 8,170 bytes, so this is the shape that has to
    /// keep working.
    #[tokio::test]
    async fn an_ollama_response_within_the_cap_is_read_whole() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let content = "x".repeat(8_000);
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            // See the helper above: drain the request first, or the close can RST
            // the response away.
            let mut scratch = [0u8; 1024];
            let _ = sock.read(&mut scratch).await;
            let body = serde_json::json!({"message": {"content": content}}).to_string();
            let head = format!(
                "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                body.len()
            );
            let _ = sock.write_all(head.as_bytes()).await;
            let _ = sock.write_all(body.as_bytes()).await;
            // Hold the socket open long enough for the reader to drain it.
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        let res = reqwest::Client::new()
            .get(format!("http://{addr}/api/chat"))
            .send()
            .await
            .expect("response headers");
        let json = read_ollama_json(res)
            .await
            .expect("an ordinary body must be read");
        assert_eq!(
            json["message"]["content"].as_str().map(str::len),
            Some(8_000),
            "the body must arrive whole"
        );
    }

    #[test]
    fn ollama_think_is_sent_only_when_configured() {
        let config = Config::default();

        assert_eq!(resolve_ollama_think(&config), None);
    }

    #[test]
    fn ollama_think_config_is_serialized_when_present() {
        let config = Config {
            ollama_think: Some("false".to_string()),
            ..Config::default()
        };

        assert_eq!(resolve_ollama_think(&config), Some("false"));
        assert_eq!(
            ollama_think_json_value("false"),
            serde_json::Value::Bool(false)
        );
        assert_eq!(
            ollama_think_json_value("low"),
            serde_json::Value::String("low".to_string())
        );
    }

    #[test]
    fn ollama_think_unsupported_error_is_detected_without_model_name() {
        assert!(is_ollama_think_unsupported_error(
            reqwest::StatusCode::BAD_REQUEST,
            "\"llama3\" does not support thinking"
        ));
        assert!(is_ollama_think_unsupported_error(
            reqwest::StatusCode::BAD_REQUEST,
            "model does not support think"
        ));
        assert!(!is_ollama_think_unsupported_error(
            reqwest::StatusCode::NOT_FOUND,
            "model does not support thinking"
        ));
        assert!(!is_ollama_think_unsupported_error(
            reqwest::StatusCode::BAD_REQUEST,
            "invalid option"
        ));
    }

    #[test]
    fn ollama_integrity_check_detects_untrusted_numeric_conversion() {
        let prompt = "営業利益: ¥577,156,000,000（円）\nROE: 6.90%\nEPS: ¥96.95（円/株）";
        let content = "営業利益約5.77億円、ROEは6.90%。配当利回りは約3%。EPSは96.95円。";

        let issues = validate_ollama_output_integrity(prompt, content, None);

        assert!(issues.iter().any(|issue| issue.contains("約5.77億円")));
        assert!(issues.iter().any(|issue| issue.contains("約3%")));
        assert!(issues.iter().any(|issue| issue.contains("配当利回り")));
        assert!(!issues.iter().any(|issue| issue.contains("6.90%")));
        assert!(!issues.iter().any(|issue| issue.contains("96.95円")));
    }

    #[test]
    fn ollama_integrity_check_allows_prompt_news_unit() {
        let prompt = "KDDI、1.2兆円投資の日本列島「デジタルベルト構想」";
        let content = "KDDI、1.2兆円投資の日本列島「デジタルベルト構想」";

        let issues = validate_ollama_output_integrity(prompt, content, None);

        assert!(issues.is_empty());
    }

    #[test]
    fn ollama_integrity_check_detects_bare_untrusted_price_ranges() {
        let prompt = "現在価格: 2724.00\n上限 2691.88 / 下限 2481.87\nADX: 22.41";
        let content = "価格は2600〜2700のレンジで推移し、ADXが30を超えれば上昇。";

        let issues = validate_ollama_output_integrity(prompt, content, None);

        assert!(issues.iter().any(|issue| issue.contains("2600〜2700")));
        assert!(!issues.iter().any(|issue| issue.contains("30")));
    }

    #[test]
    fn ollama_integrity_check_allows_in_input_comparison_but_flags_external() {
        // (2) A comparison whose operands are both in the input is reading the data,
        // not fabrication — it is allowed.
        let prompt = "現在価格: 2724.00\nニュース: 1株2325円で自己株TOB";
        let content = "1株2325円は現在価格2724円に比べ小さい。";
        assert!(validate_ollama_output_integrity(prompt, content, None).is_empty());

        // A comparison against an out-of-input derived metric is still flagged.
        let prompt2 = "ROE: 6.90%";
        let content2 = "ROEは6.90%で、業界平均に比べ高い。";
        assert!(!validate_ollama_output_integrity(prompt2, content2, None).is_empty());
    }

    #[test]
    fn output_guard_applies_to_every_provider_and_respects_flag() {
        // Confirmed values live in the prompt, so a fabricated price range in the
        // output must be caught even when the provider is NOT ollama.
        let prompt = "現在価格: 2724.00\n上限 2691.88 / 下限 2481.87\nADX: 22.41";
        let content = "価格は2600〜2700のレンジで推移し、ADXが30を超えれば上昇。";
        let mut config = crate::config::Config {
            llm_provider: "gemini".to_string(),
            ..crate::config::Config::default()
        };
        // Guard on: the fabricated range is filtered, so the output changes.
        assert_ne!(guard_llm_output(&config, prompt, content, None), content);
        // Guard disabled: content passes through untouched.
        config.no_ollama_guard = true;
        assert_eq!(guard_llm_output(&config, prompt, content, None), content);
    }

    #[test]
    fn ollama_integrity_check_detects_industry_average_paraphrase() {
        let prompt = "ROE: 6.90%";
        let content = "ROEは6.90%で、通信業界の平均的な水準に近い。";

        let issues = validate_ollama_output_integrity(prompt, content, None);

        assert!(issues.iter().any(|issue| issue.contains("平均的な水準")));
    }

    #[test]
    fn ollama_integrity_check_detects_trade_action_price_targets() {
        let prompt = "現在価格: 2724.00\n上限 2691.88 / 下限 2481.87";
        let content = "エントリーは上限2691.88を下回った瞬間に検討し、利益確定は2724.00付近。";

        let issues = validate_ollama_output_integrity(prompt, content, None);

        assert!(issues.iter().any(|issue| issue.contains("独自売買水準化")));
    }

    #[test]
    fn ollama_integrity_check_detects_vwap_direction_reversal() {
        let prompt = "VWAPが指標計算最終足終値より大幅に下 → 強い買いシグナル";
        let content = "VWAPが2571.65で終値2724.00を大きく上回っている。";

        let issues = validate_ollama_output_integrity(prompt, content, None);

        assert!(issues.iter().any(|issue| issue.contains("VWAP")));
    }

    #[test]
    fn ollama_integrity_fallback_keeps_safe_sections_only() {
        let prompt = "MACD: 1.00\n営業利益: ¥577,156,000,000（円）";
        let content = "\
**1. 投資家が注意すべきポイント**
MACDは1.00で、XOKSA本体の表示を参照する。

**4. ファンダメンタル補助情報**
短期テクニカルとは時間軸が異なる補助情報です。営業利益は約5.77億円で、配当利回りは約3%。
";

        let fallback = render_ollama_integrity_fallback(prompt, content, "en", None);

        assert!(fallback.contains("MACDは1.00"));
        assert!(fallback.contains("only the affected sentences/lines are excluded"));
        assert!(fallback.contains("短期テクニカルとは時間軸が異なる補助情報です。"));
        assert!(fallback.contains("have been excluded from analysis"));
        assert!(!fallback.contains("約5.77億円"));
        assert!(!fallback.contains("配当利回りは約3%"));
    }

    #[test]
    fn ollama_integrity_fallback_drops_unsafe_bullet_line_without_orphan_sentence() {
        let prompt = "VWAPが指標計算最終足終値より大幅に下 → 強い買いシグナル";
        let content = "\
**1. 投資家が注意すべきポイント**
- **VWAPとの乖離**：VWAPが2571.65で終値2724.00を大きく上回っている。強い買いシグナルだが、VWAPを下回ると売り圧力が増す。
- ADXは中立域。
";

        let fallback = render_ollama_integrity_fallback(prompt, content, "en", None);

        assert!(fallback.contains("- ADXは中立域。"));
        assert!(fallback.contains("have been excluded from analysis"));
        assert!(!fallback.contains("強い買いシグナルだが"));
        assert!(!fallback.contains("VWAPとの乖離"));
    }

    #[test]
    fn ollama_integrity_fallback_removes_only_bare_price_sentence() {
        let prompt = "現在価格: 2724.00\n上限 2691.88 / 下限 2481.87";
        let content = "\
**6. 総評**
短期テクニカルは過熱感と売り圧力が強い。価格は2600〜2700のレンジで推移する可能性がある。
";

        let fallback = render_ollama_integrity_fallback(prompt, content, "en", None);

        assert!(fallback.contains("短期テクニカルは過熱感と売り圧力が強い。"));
        assert!(fallback.contains("have been excluded from analysis"));
        assert!(!fallback.contains("2600〜2700"));
    }

    #[test]
    fn ollama_incomplete_response_is_suppressed_on_length() {
        let json = serde_json::json!({
            "done_reason": "length",
            "eval_count": 5120,
            "message": {
                "content": "partial",
                "thinking": "hidden"
            }
        });

        assert!(should_suppress_ollama_incomplete_response(&json));
    }

    #[test]
    fn ollama_benchmark_models_use_unique_config_models_or_llm_model() {
        let config = Config {
            llm_provider: "ollama".to_string(),
            llm_model: "llama3".to_string(),
            ..Config::default()
        };
        assert_eq!(
            resolve_ollama_benchmark_models(&config).unwrap(),
            vec!["llama3".to_string()]
        );

        let config = Config {
            llm_provider: "openai".to_string(),
            llm_model: "gpt-5.4-nano".to_string(),
            ..Config::default()
        };
        assert!(resolve_ollama_benchmark_models(&config).is_err());

        let config = Config {
            ollama_bench_models: vec![
                " llama3 ".to_string(),
                "gpt-oss:20b".to_string(),
                "llama3".to_string(),
            ],
            ..Config::default()
        };
        assert_eq!(
            resolve_ollama_benchmark_models(&config).unwrap(),
            vec!["llama3".to_string(), "gpt-oss:20b".to_string()]
        );
    }

    #[test]
    fn ollama_benchmark_row_marks_integrity_issues() {
        let prompt = "営業利益: ¥577,156,000,000（円）";
        let result = OllamaChatResult {
            json: serde_json::json!({
                "done_reason": "stop",
                "prompt_eval_count": 100_u64,
                "eval_count": 50_u64,
                "eval_duration": 1_000_000_000_u64,
                "total_duration": 2_000_000_000_u64,
                "message": {
                    "content": "営業利益は約5.77億円。",
                    "thinking": "ok"
                }
            }),
            content: "営業利益は約5.77億円。".to_string(),
            settings: OllamaRequestSettings {
                num_ctx: 1_000,
                num_predict: 4_096,
                seed: 42,
                think: None,
            },
        };

        let row = build_ollama_benchmark_row("llama3", prompt, result, None);

        assert_eq!(row.status, "ok");
        assert_eq!(row.done_reason, "stop");
        assert_eq!(row.prompt_ctx_usage_pct, Some(10.0));
        assert_eq!(row.eval_tokens_per_second, Some(50.0));
        assert_eq!(row.integrity_issue_count, 1);
        assert!(row.removed_chars > 0);
    }

    #[test]
    fn ollama_integrity_fallback_sanitizes_section_headings() {
        // Regression: headings were emitted unchecked, so a fabricated figure in a heading was
        // counted as an issue yet still reached the screen, and never showed up in removed_chars.
        let prompt = "営業利益: ¥577,156,000,000（円）";
        let content = "1. 営業利益は約5.77億円
入力どおりの記述です。";

        let rendered = render_ollama_integrity_fallback(prompt, content, "ja", None);
        assert!(
            !rendered.contains("5.77億円"),
            "an out-of-input figure in a heading must not be displayed: {rendered}"
        );
        assert!(
            rendered.contains("1. "),
            "the section number must survive so the structure holds: {rendered}"
        );
        assert!(
            rendered.contains("入力どおりの記述です。"),
            "safe body lines must be kept: {rendered}"
        );
        assert!(
            count_ollama_removed_chars(prompt, content, None) > 0,
            "removal of a heading must be measured"
        );
    }

    #[test]
    fn ollama_benchmark_row_measures_removal_on_truncated_response() {
        // Regression: `removed_chars` used to be computed only when `status` was "guarded", so a
        // run truncated at the generation limit reported "N issue(s) / removed=0" — a
        // contradiction that silently lost the measurement for exactly those runs.
        let prompt = "営業利益: ¥577,156,000,000（円）";
        let result = OllamaChatResult {
            json: serde_json::json!({
                "done_reason": "length",
                "prompt_eval_count": 100_u64,
                "eval_count": 50_u64,
                "eval_duration": 1_000_000_000_u64,
                "total_duration": 2_000_000_000_u64,
                "message": { "content": "営業利益は約5.77億円。", "thinking": "ok" }
            }),
            content: "営業利益は約5.77億円。".to_string(),
            settings: OllamaRequestSettings {
                num_ctx: 1_000,
                num_predict: 4_096,
                seed: 42,
                think: None,
            },
        };

        let row = build_ollama_benchmark_row("llama3", prompt, result, None);

        assert_eq!(row.status, "length");
        assert_eq!(row.integrity_issue_count, 1);
        assert!(
            row.removed_chars > 0,
            "a truncated response must still report what the guard removed"
        );
    }

    #[test]
    fn daily_prompt_preserves_existing_timeframe_headings() {
        let config = Config::default();
        let guard = sample_guard();

        let lines = compose_llm_prompt_lines(&config, &guard, None, &[], &[], &[], None);

        assert!(lines
            .iter()
            .any(|line| line.starts_with("2. Short-term outlook (1 week)")));
        assert!(lines
            .iter()
            .any(|line| line.starts_with("3. Mid-term outlook (1 month)")));
    }

    #[test]
    fn intraday_prompt_uses_30m_timeframe_headings() {
        let config = Config {
            analysis_mode: AnalysisMode::Intraday30m,
            ..Config::default()
        };
        let guard = sample_guard();

        let lines = compose_llm_prompt_lines(&config, &guard, None, &[], &[], &[], None);

        assert!(lines
            .iter()
            .any(|line| line.starts_with("2. 30min bar short-term outlook")));
        assert!(lines
            .iter()
            .any(|line| line.starts_with("3. Several-day outlook")));
        assert!(!lines
            .iter()
            .any(|line| line.starts_with("2. Short-term outlook (1 week)")));
        assert!(!lines
            .iter()
            .any(|line| line.starts_with("3. Mid-term outlook (1 month)")));
    }

    #[test]
    fn intraday_prompt_uses_mode_specific_timeframe_headings() {
        for (analysis_mode, expected) in [
            (AnalysisMode::Intraday5m, "2. 5min bar short-term outlook"),
            (AnalysisMode::Intraday15m, "2. 15min bar short-term outlook"),
        ] {
            let config = Config {
                analysis_mode,
                ..Config::default()
            };
            let guard = sample_guard();

            let lines = compose_llm_prompt_lines(&config, &guard, None, &[], &[], &[], None);

            assert!(lines.iter().any(|line| line.starts_with(expected)));
            assert!(lines
                .iter()
                .any(|line| line.starts_with("3. Several-day outlook")));
            assert!(lines.iter().any(|line| line.contains(&format!(
                "Interpret this analysis as a {}-based short-term analysis.",
                analysis_mode.bar_label("en")
            ))));
        }
    }

    #[test]
    fn higher_timeframe_prompt_uses_mode_specific_headings() {
        for (analysis_mode, second, third, note) in [
            (
                AnalysisMode::Weekly,
                "2. Weekly-bar mid-term outlook",
                "3. Several-week to multi-month outlook",
                "Interpret this analysis as a higher-timeframe analysis using weekly bar data.",
            ),
            (
                AnalysisMode::Monthly,
                "2. Monthly-bar long-term outlook",
                "3. Multi-month to long-term outlook",
                "Interpret this analysis as a higher-timeframe analysis using monthly bar data.",
            ),
        ] {
            let config = Config {
                analysis_mode,
                ..Config::default()
            };
            let guard = sample_guard();

            let lines = compose_llm_prompt_lines(&config, &guard, None, &[], &[], &[], None);

            assert!(lines.iter().any(|line| line.starts_with(second)));
            assert!(lines.iter().any(|line| line.starts_with(third)));
            assert!(lines.iter().any(|line| line == note));
        }
    }

    #[test]
    fn intraday_prompt_separates_latest_price_bar_from_indicator_bar() {
        let config = Config {
            analysis_mode: AnalysisMode::Intraday30m,
            ..Config::default()
        };
        let guard = intraday_guard_with_market_data_latest_time();

        let lines = compose_llm_prompt_lines(&config, &guard, None, &[], &[], &[], None);

        assert!(lines
            .iter()
            .any(|line| line == "🕒 Latest data time: 2026-05-12 14:11 UTC"));
        assert!(lines
            .iter()
            .any(|line| line == "🕯️ Bar of latest price: 2026-05-12 14:00 (30min bar)"));
        assert!(lines
            .iter()
            .any(|line| line == "📊 Indicator bar: 2026-05-12 13:30 (30min bar)"));
    }

    #[test]
    fn intraday_5m_prompt_buckets_latest_price_bar_by_mode() {
        let config = Config {
            analysis_mode: AnalysisMode::Intraday5m,
            ..Config::default()
        };
        let guard = intraday_guard_with_market_data_latest_time();

        let lines = compose_llm_prompt_lines(&config, &guard, None, &[], &[], &[], None);

        assert!(lines
            .iter()
            .any(|line| line == "🕯️ Bar of latest price: 2026-05-12 14:10 (5min bar)"));
        assert!(lines
            .iter()
            .any(|line| line == "📊 Indicator bar: 2026-05-12 13:30 (5min bar)"));
    }

    #[test]
    fn intraday_prompt_does_not_invent_latest_observed_price_bar_when_price_is_missing() {
        let config = Config {
            analysis_mode: AnalysisMode::Intraday30m,
            ..Config::default()
        };
        let mut guard = TechnicalDataGuard::new("3774.T".to_string(), "2026-05-12".to_string());
        guard.set_name("インターネットイニシアティブ");
        guard.set_datetime("2026-05-12 13:30");
        guard.set_market_data_latest_time("2026-05-12 14:11");
        guard.set_close(2955.50);
        guard.set_previous_close(2953.50);

        let lines = compose_llm_prompt_lines(&config, &guard, None, &[], &[], &[], None);

        assert!(lines
            .iter()
            .any(|line| line == "🕯️ Bar of latest price: not available"));
        assert!(lines
            .iter()
            .any(|line| line == "💰 Latest fetched price: not available"));
        assert!(lines
            .iter()
            .any(|line| line == "💰 Indicator bar close: 2955.50"));
        assert!(lines
            .iter()
            .any(|line| line == "📊 Indicator bar: 2026-05-12 13:30 (30min bar)"));
    }
}

#[cfg(test)]
mod bilingual_integrity_tests {
    //! R2: every check must fire on the same meaning in Japanese and in English.
    //! Each case is a JP/EN pair: the same input, the same output meaning, and the
    //! same expected verdict. A check that only exists in one language fails here.
    use super::validate_ollama_output_integrity;

    fn fires(prompt: &str, content: &str) -> bool {
        !validate_ollama_output_integrity(prompt, content, None).is_empty()
    }

    /// The two languages must agree; `expected` is what both must produce.
    fn pair(expected: bool, ja: (&str, &str), en: (&str, &str), what: &str) {
        assert_eq!(fires(ja.0, ja.1), expected, "{what}: 日本語側が不一致");
        assert_eq!(fires(en.0, en.1), expected, "{what}: 英語側が不一致");
    }

    #[test]
    fn check1_numeric_unit() {
        pair(
            true,
            ("終値は 172.60 です。", "売上高は 12兆円 に達します。"),
            ("The close is 172.60.", "Revenue reaches 12 trillion yen."),
            "入力にない数値単位",
        );
        pair(
            false,
            ("売上高は 12兆円 です。", "売上高は 12兆円 でした。"),
            (
                "Revenue is 12 trillion yen.",
                "Revenue was 12 trillion yen.",
            ),
            "入力にある数値単位",
        );
    }

    #[test]
    fn check2_bare_price_range() {
        pair(
            true,
            ("終値は 172.60 です。", "株価は 2600 まで戻ります。"),
            ("The close is 172.60.", "The price recovers to 2600."),
            "入力にない価格水準",
        );
        pair(
            false,
            ("終値は 2600 です。", "株価は 2600 です。"),
            ("The close is 2600.", "The price is 2600."),
            "入力にある価格水準",
        );
    }

    #[test]
    fn check3_trade_action_level() {
        pair(
            true,
            ("終値は 2600 です。", "エントリーは 2600 が妥当です。"),
            ("The close is 2600.", "A reasonable entry is 2600."),
            "入力値の売買水準化",
        );
        pair(
            false,
            ("終値は 2600 です。", "終値は 2600 でした。"),
            ("The close is 2600.", "The close was 2600."),
            "売買文脈なし",
        );
    }

    #[test]
    fn check4_derived_comparison() {
        pair(
            true,
            ("総合スコアは 13.0 です。", "業界平均を上回っています。"),
            (
                "The total score is 13.0.",
                "It is above the industry average.",
            ),
            "入力にない派生比較",
        );
        pair(
            false,
            ("業界平均は 10 です。", "業界平均を上回っています。"),
            (
                "The industry average is 10.",
                "It is above the industry average.",
            ),
            "入力にある派生比較",
        );
    }

    #[test]
    fn check5_direction_contradiction() {
        pair(
            true,
            (
                "VWAPが終値を下回っています。",
                "VWAPが終値を上回っています。",
            ),
            ("VWAP is below the close.", "VWAP is above the close."),
            "入力と逆方向のVWAP",
        );
        pair(
            false,
            (
                "VWAPが終値を下回っています。",
                "VWAPが終値を下回っています。",
            ),
            ("VWAP is below the close.", "VWAP is below the close."),
            "入力と同方向のVWAP",
        );
    }

    #[test]
    fn required_regressions_still_caught() {
        // 架空価格 / 入力価格のエントリー水準化 / VWAPの上下逆転 — 必須回帰。
        assert!(fires("終値は 172.60 です。", "目標は 2600〜2700 です。"));
        assert!(fires("終値は 2600 です。", "利確は 2600 に置きます。"));
        assert!(fires(
            "VWAPが終値を上回っています。",
            "VWAPが終値を下回っています。"
        ));
    }
}

#[cfg(test)]
mod sign_and_word_boundary_tests {
    //! A sign may stand on either side of a currency symbol, and an indicator
    //! name inside an ordinary English word is not a mention of that indicator.
    use super::validate_ollama_output_integrity;
    use crate::technical::types::TechnicalDataGuard;

    fn config() -> crate::config::Config {
        crate::config::Config {
            ticker: "AAPL".to_string(),
            fundamental: true,
            enabled_extensions: vec![crate::config::ExtensionIndicator::Ema],
            ..crate::config::Config::default()
        }
    }

    fn guard() -> TechnicalDataGuard {
        let mut g = TechnicalDataGuard::new("AAPL".to_string(), "2026-09-08".to_string());
        g.set_name("Apple Inc.");
        g.set_currency("USD");
        g.set_close(168.8);
        g
    }

    /// Confirmed data whose EPS is `eps`.
    fn with_eps(eps: f64) -> crate::integrity::ConfirmedFactSet {
        let f = crate::fundamental::FundamentalData::build(
            crate::fundamental::FundamentalInputs {
                currency: "USD".to_string(),
                eps: Some(eps),
                ..Default::default()
            },
            Some(168.8),
        );
        crate::integrity::facts_for(&guard(), &config(), None, Some(&f))
    }

    fn issues(f: &crate::integrity::ConfirmedFactSet, content: &str) -> Vec<String> {
        validate_ollama_output_integrity("", content, Some(f))
    }

    fn survives(f: &crate::integrity::ConfirmedFactSet, content: &str) {
        assert!(
            issues(f, content).is_empty(),
            "must survive: {content} -> {:?}",
            issues(f, content)
        );
    }

    fn caught(f: &crate::integrity::ConfirmedFactSet, content: &str) {
        assert!(!issues(f, content).is_empty(), "must be caught: {content}");
    }

    // ── L1: the sign may precede or follow the currency symbol ──────────────

    #[test]
    fn a_negative_amount_reads_the_same_however_the_sign_is_placed() {
        let neg = with_eps(-2.50);
        for text in [
            "EPS is -$2.50.",
            "EPS is $-2.50.",
            "EPSは-2.50ドルです。",
            "EPSは−2.50ドルです。",
        ] {
            survives(&neg, text);
        }
    }

    #[test]
    fn a_sign_flip_is_caught_however_the_sign_is_placed() {
        let pos = with_eps(2.50);
        survives(&pos, "EPS is $2.50.");
        for text in ["EPS is -$2.50.", "EPS is $-2.50.", "EPSは-2.50ドルです。"] {
            caught(&pos, text);
        }
        // …and the mirror case: a confirmed negative may not be written positive.
        let neg = with_eps(-2.50);
        caught(&neg, "EPS is $2.50.");
        caught(&neg, "EPSは2.50ドルです。");
    }

    // ── L1b: a number's role is read from its sentence, not from the capture ──

    /// A year and a window length are not readings, and saying so must not depend
    /// on which fragment a check happened to capture. Before the fix the bare-price
    /// check re-read the number from the captured fragment alone, where `since` and
    /// `-day` are not visible, so both became observations with no confirmed
    /// counterpart and the whole sentence was removed.
    #[test]
    fn a_year_or_a_window_in_a_price_sentence_is_not_a_fabricated_price() {
        let f = rsi_and_ema();
        survives(&f, "Support has held since 2019.");
        survives(&f, "The 200-day moving average has acted as support.");
    }

    /// The same sentence shape must still lose a price the confirmed data never
    /// stated — the fix narrows the role, it does not disarm the check.
    #[test]
    fn a_bare_price_with_no_confirmed_counterpart_is_still_caught() {
        let f = rsi_and_ema();
        caught(&f, "Support sits at 2600 on this chart.");
        caught(&f, "サポートは2600です。");
    }

    /// The separator fix must not break a thousands separator, which is the
    /// reason the digits group accepts commas at all.
    #[test]
    fn a_thousands_separator_is_still_one_number() {
        let f = rsi_and_ema();
        // 13,704,000,000,000 and 13704000000000 must behave alike; neither is a
        // confirmed figure here, so both are caught, and as one number each.
        let with_sep = issues(&f, "Revenue was 13,704,000,000,000 yen.");
        let without = issues(&f, "Revenue was 13704000000000 yen.");
        assert_eq!(
            with_sep.len(),
            without.len(),
            "a thousands separator must not change how many claims are read: {with_sep:?} vs {without:?}"
        );
    }

    // ── L2: an indicator name inside a word is not a mention ────────────────

    fn rsi_and_ema() -> crate::integrity::ConfirmedFactSet {
        let mut g = guard();
        g.set_rsi(45.2);
        g.set_ema_short(171.45);
        g.set_ema_long(166.88);
        crate::integrity::facts_for(&g, &config(), None, None)
    }

    #[test]
    fn an_indicator_name_inside_an_english_word_is_not_a_mention() {
        let f = rsi_and_ema();
        // "remains" contains "ema"; binding to it stole the number from RSI.
        survives(&f, "RSI remains at 45.2.");
        caught(&f, "RSI remains at 166.88.");
        // The same in the other direction: "period" contains "per" (PER).
        survives(&f, "Over the period the RSI is 45.2.");
        assert_eq!(
            issues(&f, "RSIは45.2のままです。").is_empty(),
            issues(&f, "RSI remains at 45.2.").is_empty(),
            "the two languages must agree"
        );
    }

    #[test]
    fn a_standalone_indicator_name_is_still_recognised() {
        let f = rsi_and_ema();
        survives(&f, "EMA is 166.88.");
        survives(&f, "The EMA is 171.45.");
        caught(&f, "EMA is 999.99.");
        // A Japanese sentence writes the value straight after the name.
        survives(&f, "EMAは166.88です。");
        caught(&f, "EMAは999.99です。");
    }
}

#[cfg(test)]
mod indicator_window_tests {
    //! Two legs of one indicator differ only by their window, so a sentence that
    //! names the window must be held to the leg it names.
    use super::validate_ollama_output_integrity;
    use crate::technical::types::TechnicalDataGuard;

    fn facts() -> crate::integrity::ConfirmedFactSet {
        let config = crate::config::Config {
            ticker: "AAPL".to_string(),
            ema_short_period: 5,
            ema_long_period: 20,
            enabled_extensions: vec![crate::config::ExtensionIndicator::Ema],
            ..crate::config::Config::default()
        };
        let mut g = TechnicalDataGuard::new("AAPL".to_string(), "2026-09-08".to_string());
        g.set_name("Apple Inc.");
        g.set_currency("USD");
        g.set_close(168.8);
        g.set_rsi(45.2);
        g.set_ema_short(171.45);
        g.set_ema_long(166.88);
        crate::integrity::facts_for(&g, &config, None, None)
    }

    fn issues(content: &str) -> Vec<String> {
        validate_ollama_output_integrity("", content, Some(&facts()))
    }

    fn survives(content: &str) {
        assert!(
            issues(content).is_empty(),
            "must survive: {content} -> {:?}",
            issues(content)
        );
    }

    fn caught(content: &str) {
        assert!(!issues(content).is_empty(), "must be caught: {content}");
    }

    #[test]
    fn a_named_window_selects_its_own_leg() {
        survives("The 20-day EMA is 166.88.");
        survives("The 5-day EMA is 171.45.");
        survives("20日EMAは166.88です。");
        survives("5日EMAは171.45です。");
        survives("EMA(20) is 166.88.");
    }

    #[test]
    fn a_named_window_may_not_carry_the_other_legs_value() {
        caught("The 20-day EMA is 171.45.");
        caught("The 5-day EMA is 166.88.");
        caught("20日EMAは171.45です。");
        caught("5日EMAは166.88です。");
        caught("EMA(20) is 171.45.");
    }

    #[test]
    fn a_window_that_was_never_computed_carries_nothing() {
        caught("The 25-day EMA is 171.45.");
        caught("The 25-day EMA is 166.88.");
        caught("25日EMAは166.88です。");
    }

    #[test]
    fn every_spelling_of_a_window_selects_the_same_leg() {
        // The reader that excludes a window from verification and the reader that
        // uses it to pick a leg share one vocabulary, so a bracket with a unit
        // behaves as a bracket without one, and as the leading form.
        for correct in [
            "EMA(20) is 166.88.",
            "EMA(20 days) is 166.88.",
            "EMA (20 days) is 166.88.",
            "The 20-day EMA is 166.88.",
            "EMA（20日）は166.88です。",
            "EMA(20日)は166.88です。",
            "20日EMAは166.88です。",
            "20期間EMAは166.88です。",
        ] {
            survives(correct);
        }
        for swapped in [
            "EMA(20) is 171.45.",
            "EMA(20 days) is 171.45.",
            "EMA (20 days) is 171.45.",
            "The 20-day EMA is 171.45.",
            "EMA（20日）は171.45です。",
            "EMA(20日)は171.45です。",
            "20日EMAは171.45です。",
            "20期間EMAは171.45です。",
        ] {
            caught(swapped);
        }
    }

    #[test]
    fn every_window_unit_reads_the_same_in_the_bracket_and_leading_forms() {
        // The unit list is shared, so a unit that works inside brackets works in
        // front of the name too. Each unit is checked in both shapes, on the
        // correct value, on the other leg's value, and on a window that was never
        // computed.
        for unit in [
            "day", "days", "period", "periods", "bar", "bars", "week", "weeks", "month", "months",
        ] {
            let bracket_ok = format!("EMA(20 {unit}) is 166.88.");
            let leading_ok = format!("The 20-{unit} EMA is 166.88.");
            let bracket_bad = format!("EMA(20 {unit}) is 171.45.");
            let leading_bad = format!("The 20-{unit} EMA is 171.45.");
            let bracket_absent = format!("EMA(25 {unit}) is 166.88.");
            let leading_absent = format!("The 25-{unit} EMA is 166.88.");
            survives(&bracket_ok);
            survives(&leading_ok);
            caught(&bracket_bad);
            caught(&leading_bad);
            caught(&bracket_absent);
            caught(&leading_absent);
            assert_eq!(
                issues(&bracket_bad).is_empty(),
                issues(&leading_bad).is_empty(),
                "the bracket and leading forms must agree for {unit}"
            );
        }
    }

    #[test]
    fn a_window_belongs_to_the_indicator_it_was_written_on() {
        // RSI(14) must not select a leg of the EMA beside it, in either order and
        // in either language.
        for text in [
            "RSI(14) is 45.2 and EMA is 166.88.",
            "EMA is 166.88 and RSI(14) is 45.2.",
            "RSI(14)は45.2、EMAは166.88です。",
            "The 14-day RSI is 45.2 and the long EMA is 166.88.",
            "RSI(14) is 45.2 and the 20-day EMA is 166.88.",
        ] {
            survives(text);
        }
        // The EMA's own window is still enforced in the same sentence.
        caught("RSI(14) is 45.2 and the 20-day EMA is 171.45.");
        caught("RSI(14) is 45.2 and the long EMA is 171.45.");
    }

    #[test]
    fn an_unqualified_name_may_still_be_either_leg() {
        survives("EMA is 166.88.");
        survives("EMA is 171.45.");
    }

    #[test]
    fn a_window_on_an_indicator_without_legs_is_not_a_claim_about_the_window() {
        // RSI has one value; the engine does not track a per-window RSI, so
        // naming a window must not turn a correct citation into a mismatch.
        survives("RSI(14) is 45.2.");
        survives("The 14-day RSI is 45.2.");
        survives("The 21-day RSI is 45.2.");
    }
}

#[cfg(test)]
mod per_unit_fundamental_tests {
    //! The engine's own fundamental lines must survive being quoted back. The
    //! expected text is taken from the production formatter, not hand-written,
    //! so a label change breaks this test rather than the guard.
    use super::validate_ollama_output_integrity;
    use crate::technical::types::TechnicalDataGuard;

    fn fixture(lang: &str) -> (Vec<String>, crate::integrity::ConfirmedFactSet) {
        let config = crate::config::Config {
            ticker: "AAPL".to_string(),
            lang: lang.to_string(),
            fundamental: true,
            ..crate::config::Config::default()
        };
        let mut g = TechnicalDataGuard::new("AAPL".to_string(), "2026-09-08".to_string());
        g.set_name("Apple Inc.");
        g.set_currency("USD");
        g.set_close(168.8);
        let f = crate::fundamental::FundamentalData::build(
            crate::fundamental::FundamentalInputs {
                currency: "USD".to_string(),
                dividend: Some(3.0),
                trading_unit: Some(100),
                ..Default::default()
            },
            Some(168.8),
        );
        let lines = crate::fundamental::render_fundamental_display(&f, lang);
        let facts = crate::integrity::facts_for(&g, &config, None, Some(&f));
        (lines, facts)
    }

    /// The engine's own line containing `needle`, with its emoji prefix removed.
    fn line_with(lines: &[String], needle: &str) -> String {
        lines
            .iter()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("the formatter no longer emits a line for {needle}"))
            .trim_start_matches(|c: char| !c.is_ascii_alphanumeric() && !"配最単".contains(c))
            .to_string()
    }

    #[test]
    fn the_engines_own_per_share_and_per_lot_lines_survive_being_quoted() {
        for (lang, share, lot, unit) in [
            (
                "ja",
                "配当（1株あたり）",
                "配当（1単元あたり）",
                "最少単位株",
            ),
            (
                "en",
                "Dividend (per share)",
                "Dividend (per lot)",
                "Min. Trading Unit",
            ),
        ] {
            let (lines, facts) = fixture(lang);
            for needle in [share, lot, unit] {
                let text = line_with(&lines, needle);
                let found = validate_ollama_output_integrity("", &text, Some(&facts));
                assert!(
                    found.is_empty(),
                    "[{lang}] the engine's own line must survive: {text} -> {found:?}"
                );
            }
        }
    }

    #[test]
    fn a_changed_per_lot_or_unit_figure_is_caught() {
        let (_, facts) = fixture("en");
        for text in [
            "Dividend (per share): $4.00",
            "Dividend (per lot): $400",
            "Min. Trading Unit: 200 shares",
        ] {
            assert!(
                !validate_ollama_output_integrity("", text, Some(&facts)).is_empty(),
                "must be caught: {text}"
            );
        }
        let (_, facts_ja) = fixture("ja");
        for text in [
            "配当（1株あたり）: $4.00",
            "配当（1単元あたり）: $400",
            "最少単位株: 200株",
        ] {
            assert!(
                !validate_ollama_output_integrity("", text, Some(&facts_ja)).is_empty(),
                "must be caught: {text}"
            );
        }
    }

    #[test]
    fn per_share_is_not_read_as_per() {
        // "per" inside "per share" must not bind the dividend to PER.
        let (_, facts) = fixture("en");
        assert!(
            validate_ollama_output_integrity("", "Dividend (per share): $3.00", Some(&facts))
                .is_empty()
        );
    }
}

#[cfg(test)]
mod sentence_removal_tests {
    //! Removal excludes the offending sentence and keeps the rest — in both
    //! languages, using the same sentence boundaries the verifier uses.
    use super::guard_llm_output;
    use crate::technical::types::TechnicalDataGuard;

    fn facts() -> crate::integrity::ConfirmedFactSet {
        let config = crate::config::Config {
            ticker: "AAPL".to_string(),
            ..crate::config::Config::default()
        };
        let mut g = TechnicalDataGuard::new("AAPL".to_string(), "2026-09-08".to_string());
        g.set_name("Apple Inc.");
        g.set_currency("USD");
        g.set_close(168.8);
        g.set_rsi(45.2);
        crate::integrity::facts_for(&g, &config, None, Some(&fundamental()))
    }

    fn fundamental() -> crate::fundamental::FundamentalData {
        crate::fundamental::FundamentalData::build(
            crate::fundamental::FundamentalInputs {
                currency: "USD".to_string(),
                ..Default::default()
            },
            Some(168.8),
        )
    }

    fn guarded(content: &str) -> String {
        let config = crate::config::Config {
            llm_provider: "gemini".to_string(),
            ..crate::config::Config::default()
        };
        guard_llm_output(&config, "", content, Some(&facts()))
    }

    #[test]
    fn a_correct_sentence_survives_beside_a_wrong_one_in_both_languages() {
        let en = guarded("RSI is 45.2. The price is $999.");
        assert!(
            en.contains("RSI is 45.2."),
            "the correct English sentence must be kept: {en}"
        );
        assert!(
            !en.contains("$999"),
            "the fabricated price must be removed: {en}"
        );

        let ja = guarded("RSIは45.2です。株価は999ドルです。");
        assert!(
            ja.contains("RSIは45.2です。"),
            "the correct Japanese sentence must be kept: {ja}"
        );
        assert!(
            !ja.contains("999"),
            "the fabricated price must be removed: {ja}"
        );
    }

    #[test]
    fn a_decimal_point_does_not_split_a_sentence() {
        // Splitting on every '.' would cut "45.2" in half and lose the reading.
        let out = guarded("RSI is 45.2 today.");
        assert!(out.contains("RSI is 45.2 today."), "{out}");
    }
}

#[cfg(test)]
mod stated_threshold_tests {
    //! An alert's message states the rule that fired, so its threshold is part of
    //! what the engine told the model. Restating the rule is a citation; claiming
    //! the indicator *is* that number is not.
    use super::validate_ollama_output_integrity;
    use crate::technical::types::TechnicalDataGuard;

    /// Confirmed data for a firing alert on `score <= 5`, built the way the
    /// monitor builds it.
    fn facts(with_threshold: bool) -> crate::integrity::ConfirmedFactSet {
        let config = crate::config::Config {
            ticker: "7203.T".to_string(),
            ..crate::config::Config::default()
        };
        let mut g = TechnicalDataGuard::new("7203.T".to_string(), "2026-09-04".to_string());
        g.set_name("トヨタ自動車");
        g.set_currency("JPY");
        g.set_close(3091.00);
        g.set_rsi(28.0);
        let mut f = crate::integrity::facts_for(&g, &config, None, None);
        if with_threshold {
            f.push_bound("score", crate::integrity::Comparison::Le, 5.0);
        }
        f
    }

    fn issues(f: &crate::integrity::ConfirmedFactSet, content: &str) -> Vec<String> {
        validate_ollama_output_integrity("", content, Some(f))
    }

    #[test]
    fn the_rule_the_alert_reported_may_be_quoted_back() {
        let f = facts(true);
        let score = f.single_value("score").expect("the score is confirmed");
        for note in [
            format!("トヨタ自動車は終値3091.00時点でスコア{score:.1}となり、score <= 5のアラート条件が成立しているため、短期的には弱い状態を示している。"),
            format!("7203.T is at a close of 3091.00 with a score of {score:.1}, and the alert condition score <= 5 holds."),
        ] {
            let found = issues(&f, &note);
            assert!(found.is_empty(), "the engine's own rule must be quotable: {note} -> {found:?}");
        }
    }

    #[test]
    fn a_threshold_that_was_never_stated_is_still_refused() {
        // Without the rule in the confirmed data the same sentence has no basis,
        // which is what made the recorded notifications fail.
        let f = facts(false);
        let score = f.single_value("score").expect("the score is confirmed");
        let note = format!("スコア{score:.1}となり、score <= 5のアラート条件が成立している。");
        assert!(!issues(&f, &note).is_empty());
    }

    #[test]
    fn a_different_threshold_is_refused() {
        let f = facts(true);
        let score = f.single_value("score").expect("the score is confirmed");
        for note in [
            format!("スコア{score:.1}となり、score <= 9のアラート条件が成立している。"),
            format!("The score is {score:.1} and the alert condition score <= 9 holds."),
        ] {
            assert!(!issues(&f, &note).is_empty(), "must be caught: {note}");
        }
    }

    #[test]
    fn an_ordinary_claim_is_not_a_restatement_of_the_rule() {
        // A comparison is parsed, not guessed from a word that happens to occur:
        // "over" inside "overall" and "recovered" is not a comparison, and these
        // sentences claim the score *is* 5.0 — which it is not.
        let f = facts(true);
        for note in [
            "The score overall is 5.0.",
            "The score recovered to 5.0.",
            "The score is 5.0.",
            "スコアは5.0まで戻りました。",
        ] {
            assert!(!issues(&f, note).is_empty(), "must be caught: {note}");
        }
    }

    #[test]
    fn a_flipped_or_loosened_comparison_is_a_different_rule() {
        // The stated rule is `score <= 5`. Any other direction or strictness is
        // a rule the engine never reported.
        let f = facts(true);
        for note in [
            "The alert condition score >= 5 holds.",
            "The alert condition score > 5 holds.",
            "The alert condition score < 5 holds.",
            "スコアは5以上の条件が成立しています。",
            "スコアは5未満の条件が成立しています。",
        ] {
            assert!(!issues(&f, note).is_empty(), "must be caught: {note}");
        }
    }

    #[test]
    fn a_restatement_reads_the_same_in_both_languages() {
        let f = facts(true);
        for note in [
            "The alert condition score <= 5 holds.",
            "The alert condition score <= 5.0 holds.",
            "The score is at or below 5.",
            "スコアは5以下の条件が成立しています。",
            "スコアが5以下になったため通知しました。",
        ] {
            let found = issues(&f, note);
            assert!(found.is_empty(), "must survive: {note} -> {found:?}");
        }
    }

    #[test]
    fn a_quoted_rule_is_still_held_to_its_unit_and_its_value() {
        // Restating the rule quotes it as the engine wrote it: a bare number.
        // A unit, a currency or an unknown suffix states something the rule does
        // not, and a value the rule does not carry is not the rule.
        let f = facts(true);
        for note in [
            "The alert condition score <= $5 holds.",
            "The alert condition score <= 5% holds.",
            "The alert condition score <= 5foo holds.",
            "The alert condition score <= 5.4 holds.",
            "The alert condition score <= 4 holds.",
        ] {
            assert!(!issues(&f, note).is_empty(), "must be caught: {note}");
        }
    }

    #[test]
    fn a_rounded_threshold_is_a_different_rule() {
        // With a threshold of 5.4, "score <= 5" is not the rule that fired —
        // display rounding may not change a condition.
        let config = crate::config::Config {
            ticker: "7203.T".to_string(),
            ..crate::config::Config::default()
        };
        let mut g = TechnicalDataGuard::new("7203.T".to_string(), "2026-09-04".to_string());
        g.set_name("トヨタ自動車");
        g.set_currency("JPY");
        g.set_close(3091.00);
        let mut f = crate::integrity::facts_for(&g, &config, None, None);
        f.push_bound("score", crate::integrity::Comparison::Le, 5.4);
        assert!(!issues(&f, "The alert condition score <= 5 holds.").is_empty());
        assert!(issues(&f, "The alert condition score <= 5.4 holds.").is_empty());
    }

    #[test]
    fn a_comparison_qualifies_only_the_number_that_follows_it() {
        // "at or below 5" states the rule; the reading after it is a separate
        // claim and is checked as one. The confirmed score is 0.
        let f = facts(true);
        for survives in [
            "The score is at or below 5 (current value: 0.0).",
            "The score is at or below 5 with a current reading of 0.0.",
            "スコアは5以下の条件が成立しています（現在のスコアは0.0）。",
        ] {
            let found = issues(&f, survives);
            assert!(found.is_empty(), "must survive: {survives} -> {found:?}");
        }
        for caught in [
            "The score is at or below 5 (current value: 5.0).",
            "The score is at or below 5 with a current reading of 5.0.",
            "The score is at or below 5 with a current reading of 9.0.",
            "スコアは5以下の条件が成立しています（現在のスコアは5.0）。",
        ] {
            assert!(!issues(&f, caught).is_empty(), "must be caught: {caught}");
        }
    }

    /// A rule whose threshold equals the indicator's current value — the case
    /// where a wrong condition could otherwise be rescued by the reading check.
    fn facts_at_the_boundary() -> crate::integrity::ConfirmedFactSet {
        let config = crate::config::Config {
            ticker: "7203.T".to_string(),
            ..crate::config::Config::default()
        };
        let mut g = TechnicalDataGuard::new("7203.T".to_string(), "2026-09-04".to_string());
        g.set_name("トヨタ自動車");
        g.set_currency("JPY");
        g.set_close(3091.00);
        let mut f = crate::integrity::facts_for(&g, &config, None, None);
        // The composite score of this fixture is 0.0, so the rule sits exactly on it.
        f.push_bound("score", crate::integrity::Comparison::Le, 0.0);
        f
    }

    #[test]
    fn a_wrong_condition_is_not_rescued_by_the_reading_matching() {
        let f = facts_at_the_boundary();
        assert_eq!(
            f.single_value("score"),
            Some(0.0),
            "the fixture must sit on the threshold for this test to mean anything"
        );
        // The stated rule is `score <= 0`; these are other rules.
        for caught in [
            "The alert condition score > 0 holds.",
            "The alert condition score >= 0 holds.",
            "The alert condition score < 0 holds.",
            "スコアは0未満の条件が成立しています。",
            "スコアは0以上の条件が成立しています。",
        ] {
            assert!(!issues(&f, caught).is_empty(), "must be caught: {caught}");
        }
        // The rule itself, and an ordinary reading, both stand.
        for survives in [
            "The alert condition score <= 0 holds.",
            "スコアは0以下の条件が成立しています。",
            "スコアは0です。",
            "The score is 0.0.",
        ] {
            let found = issues(&f, survives);
            assert!(found.is_empty(), "must survive: {survives} -> {found:?}");
        }
    }

    /// A rule on a money indicator, whose threshold equals the confirmed close —
    /// the case where a currency-marked condition could be rescued by the
    /// reading check.
    fn facts_on_a_price_rule() -> crate::integrity::ConfirmedFactSet {
        let config = crate::config::Config {
            ticker: "7203.T".to_string(),
            ..crate::config::Config::default()
        };
        let mut g = TechnicalDataGuard::new("7203.T".to_string(), "2026-09-04".to_string());
        g.set_name("トヨタ自動車");
        g.set_currency("JPY");
        g.set_close(3091.00);
        let mut f = crate::integrity::facts_for(&g, &config, None, None);
        f.push_bound("close", crate::integrity::Comparison::Le, 3091.0);
        f
    }

    #[test]
    fn a_currency_marked_condition_is_judged_as_a_condition() {
        let f = facts_on_a_price_rule();
        // The rule, quoted as the engine wrote it.
        for survives in [
            "The alert condition close <= 3091 holds.",
            "終値は3091以下の条件が成立しています。",
        ] {
            let found = issues(&f, survives);
            assert!(found.is_empty(), "must survive: {survives} -> {found:?}");
        }
        // Other conditions on the same indicator — with or without a currency.
        // The threshold equals the confirmed close, so the reading check would
        // have accepted every one of these.
        for caught in [
            "The alert condition close > 3091 holds.",
            "The alert condition close > ¥3091 holds.",
            "The alert condition close <= ¥3091 holds.",
            "株価が3091円を上回ったため通知しました。",
            "株価が3091を上回ったため通知しました。",
            "終値は3091円以上の条件が成立しています。",
        ] {
            assert!(!issues(&f, caught).is_empty(), "must be caught: {caught}");
        }
    }

    #[test]
    fn an_ordinary_currency_reading_still_stands() {
        // Not a comparison: an ordinary statement of the confirmed close, in the
        // instrument's own currency, in either language.
        let f = facts_on_a_price_rule();
        for survives in [
            "株価は3091円です。",
            "終値は3091円です。",
            "The close is 3091.",
            "The close is ¥3091.",
        ] {
            let found = issues(&f, survives);
            assert!(found.is_empty(), "must survive: {survives} -> {found:?}");
        }
        // …and a wrong reading is still a wrong reading.
        assert!(!issues(&f, "株価は3092円です。").is_empty());
    }

    #[test]
    fn a_comparison_on_an_indicator_with_no_stated_rule_is_read_as_before() {
        // Nothing was stated for the RSI, so a comparison naming it is judged the
        // way any other sentence is — the guard does not invent a rule to fail.
        let f = facts(true);
        assert!(!issues(&f, "RSI is below 30.").is_empty());
    }

    #[test]
    fn the_threshold_does_not_become_a_reading_of_the_indicator() {
        // The bound is quotable, but it is not the indicator's value: an answer
        // that states the score *is* 5 is still a fabrication.
        let f = facts(true);
        assert!(!issues(&f, "スコアは5.0です。").is_empty());
        assert!(!issues(&f, "The score is 5.0.").is_empty());
        // …and the bound does not leak to another indicator.
        assert!(!issues(&f, "RSIは5です。").is_empty());
    }
}

#[cfg(test)]
mod token_reading_tests {
    //! A token is only exempt from verification when it really is what it looks
    //! like. An English word that begins like a month is not a date, and a
    //! figure with a magnitude suffix is not the bare figure.
    use super::validate_ollama_output_integrity;
    use crate::technical::types::TechnicalDataGuard;

    fn facts() -> crate::integrity::ConfirmedFactSet {
        let config = crate::config::Config {
            ticker: "AAPL".to_string(),
            fundamental: true,
            ..crate::config::Config::default()
        };
        let mut g = TechnicalDataGuard::new("AAPL".to_string(), "2026-09-08".to_string());
        g.set_name("Apple Inc.");
        g.set_currency("USD");
        g.set_close(168.8);
        g.set_previous_close(168.8);
        let f = crate::fundamental::FundamentalData::build(
            crate::fundamental::FundamentalInputs {
                currency: "USD".to_string(),
                revenue: Some(1_000.0),
                operating_income: Some(100.0),
                ..Default::default()
            },
            Some(168.8),
        );
        crate::integrity::facts_for(&g, &config, None, Some(&f))
    }

    fn issues(content: &str) -> Vec<String> {
        validate_ollama_output_integrity("", content, Some(&facts()))
    }

    fn survives(content: &str) {
        assert!(
            issues(content).is_empty(),
            "must survive: {content} -> {:?}",
            issues(content)
        );
    }

    fn caught(content: &str) {
        assert!(!issues(content).is_empty(), "must be caught: {content}");
    }

    // ── J1: only real month names are months ────────────────────────────────

    #[test]
    fn an_english_word_that_starts_like_a_month_is_not_a_date() {
        // The confirmed operating margin is 10%; 99 is not a date and not a
        // confirmed reading.
        caught("Operating margin 99%.");
        caught("The price declined 99%.");
        caught("Margin 99% and declining.");
        caught("Deceleration to 99% is possible.");
        caught("The market may 99% recover.");
    }

    #[test]
    fn a_real_month_name_still_reads_as_a_date() {
        survives("As of September 8, 2026, the price is 168.8.");
        survives("As of Sep 8, 2026, the price is 168.8.");
        survives("As of 8 September 2026, the price is 168.8.");
        survives("On March 3, 2026 the price was 168.8.");
        // A wrong value inside a dated sentence is still refused.
        caught("As of September 8, 2026, the price is 170.0.");
    }

    // ── J2: a magnitude suffix is part of the amount ────────────────────────

    #[test]
    fn a_magnitude_suffix_is_read_as_part_of_the_amount() {
        // The confirmed price is $168.8. Every scaled spelling is a different
        // amount and must be refused; the short and the long form agree.
        for text in [
            "The price is $168.8m.",
            "The price is $168.8 million.",
            "The price is $168.8bn.",
            "The price is $168.8 billion.",
            "The price is $168.8k.",
            "The price is $168.8 thousand.",
            "The price is $168.8mm.",
            "The price is $168.8tn.",
        ] {
            assert!(!issues(text).is_empty(), "must be caught: {text}");
        }
        survives("The price is $168.8.");
        survives("株価は168.8ドルです。");
    }

    /// An instrument whose confirmed revenue is $168,800,000 — a value a model
    /// naturally writes as `$168.8m`, `$168.8 m` or `$168.8 million`.
    fn scaled_revenue_facts() -> crate::integrity::ConfirmedFactSet {
        let config = crate::config::Config {
            ticker: "AAPL".to_string(),
            fundamental: true,
            ..crate::config::Config::default()
        };
        let mut g = TechnicalDataGuard::new("AAPL".to_string(), "2026-09-08".to_string());
        g.set_name("Apple Inc.");
        g.set_currency("USD");
        g.set_close(168.8);
        let f = crate::fundamental::FundamentalData::build(
            crate::fundamental::FundamentalInputs {
                currency: "USD".to_string(),
                revenue: Some(168_800_000.0),
                ..Default::default()
            },
            Some(168.8),
        );
        crate::integrity::facts_for(&g, &config, None, Some(&f))
    }

    #[test]
    fn a_correct_amount_survives_in_every_spelling_of_its_magnitude() {
        let f = scaled_revenue_facts();
        for text in [
            "Revenue is $168.8m.",
            "Revenue is $168.8 m.",
            "Revenue is $168.8mm.",
            "Revenue is $168.8 million.",
            "Revenue is $168,800,000.",
            "Revenue is 168.8 million dollars.",
            "売上高は168.8百万ドルです。",
        ] {
            let found = validate_ollama_output_integrity("", text, Some(&f));
            assert!(found.is_empty(), "must survive: {text} -> {found:?}");
        }
    }

    #[test]
    fn a_wrong_value_or_magnitude_is_refused_in_every_spelling() {
        let f = scaled_revenue_facts();
        for text in [
            // Wrong value, right magnitude.
            "Revenue is $168.9m.",
            "Revenue is $168.9 m.",
            "Revenue is $168.9 million.",
            // Right digits, wrong magnitude.
            "Revenue is $168.8bn.",
            "Revenue is $168.8 bn.",
            "Revenue is $168.8 billion.",
            "Revenue is $168.8k.",
            "Revenue is $168.8 thousand.",
            "Revenue is $168.8tn.",
            // The bare figure is three orders of magnitude away from the value.
            "Revenue is $168.8.",
        ] {
            assert!(
                !validate_ollama_output_integrity("", text, Some(&f)).is_empty(),
                "must be caught: {text}"
            );
        }
    }

    #[test]
    fn an_unknown_suffix_is_still_refused_next_to_a_scaled_amount() {
        let f = scaled_revenue_facts();
        for text in [
            "Revenue is $168.8x.",
            "Revenue is $168.8bar.",
            "Revenue is $168.8km.",
        ] {
            assert!(
                !validate_ollama_output_integrity("", text, Some(&f)).is_empty(),
                "must be caught: {text}"
            );
        }
    }

    #[test]
    fn a_suffix_the_reader_does_not_know_is_not_dropped() {
        // Matching the bare digits and ignoring the leftover would accept this.
        caught("The price is $168.8x.");
        caught("The price is 168.8foo.");
    }

    #[test]
    fn an_ordinal_is_not_read_as_a_value() {
        survives("The 1st reading of the price is 168.8.");
        survives("On the 3rd attempt the price is 168.8.");
    }
}

#[cfg(test)]
mod notation_invariance_tests {
    //! The verdict must come from what a sentence claims, not from how it is
    //! typed. Each case writes the same claim several ways — with and without
    //! thousands separators, and in ISO, Japanese and English date form — and
    //! asserts they agree.
    use super::validate_ollama_output_integrity;
    use crate::technical::types::TechnicalDataGuard;

    fn guard(ticker: &str, currency: &str, close: f64) -> TechnicalDataGuard {
        let mut g = TechnicalDataGuard::new(ticker.to_string(), "2026-09-08".to_string());
        g.set_name("日本電信電話");
        g.set_currency(currency);
        g.set_close(close);
        g
    }

    fn config(ticker: &str) -> crate::config::Config {
        crate::config::Config {
            ticker: ticker.to_string(),
            fundamental: true,
            ..crate::config::Config::default()
        }
    }

    /// One instrument whose fundamentals belong to FY2025.
    fn fy2025() -> crate::integrity::ConfirmedFactSet {
        let g = guard("9432.T", "JPY", 168.80);
        let f = crate::fundamental::FundamentalData::build(
            crate::fundamental::FundamentalInputs {
                currency: "JPY".to_string(),
                fiscal_period: Some("FY (FY ends 2025-03-31)".to_string()),
                revenue: Some(13_704_000_000_000.0),
                ..Default::default()
            },
            Some(168.80),
        );
        crate::integrity::facts_for(&g, &config("9432.T"), None, Some(&f))
    }

    fn issues(f: &crate::integrity::ConfirmedFactSet, content: &str) -> Vec<String> {
        validate_ollama_output_integrity("", content, Some(f))
    }

    fn survives(f: &crate::integrity::ConfirmedFactSet, content: &str) {
        assert!(
            issues(f, content).is_empty(),
            "must survive: {content} -> {:?}",
            issues(f, content)
        );
    }

    fn caught(f: &crate::integrity::ConfirmedFactSet, content: &str) {
        assert!(!issues(f, content).is_empty(), "must be caught: {content}");
    }

    // ── I1: a thousands separator is punctuation of the figure ──────────────

    #[test]
    fn a_thousands_separator_does_not_split_a_claim() {
        let f = fy2025();
        // Grouped and ungrouped must agree, both for the correct citation…
        survives(&f, "Revenue was 13,704,000,000,000 yen in FY2025.");
        survives(&f, "Revenue was 13704000000000 yen in FY2025.");
        // …and for the figure reused under another fiscal year.
        caught(
            &f,
            "Revenue was 13,704,000,000,000 yen in FY2025 and 13,704,000,000,000 yen in FY2024.",
        );
        caught(
            &f,
            "Revenue was 13704000000000 yen in FY2025 and 13704000000000 yen in FY2024.",
        );
    }

    #[test]
    fn a_grouped_figure_is_still_verified_against_its_own_value() {
        let f = fy2025();
        survives(&f, "売上高は13,704,000,000,000円です。");
        caught(&f, "売上高は13,705,000,000,000円です。");
    }

    #[test]
    fn a_comma_inside_a_period_specification_does_not_split_a_claim() {
        let mut g = guard("9432.T", "JPY", 168.80);
        g.set_macd(4.6815);
        g.set_rsi(45.2);
        let f = crate::integrity::facts_for(&g, &config("9432.T"), None, None);
        survives(&f, "MACD(12,26,9)は4.6815、RSIは45.2です。");
        caught(&f, "MACD(12,26,9)は45.2、RSIは4.6815です。");
    }

    // ── I2: a currency-marked amount is never a bar size ─────────────────────

    #[test]
    fn a_currency_amount_is_verified_for_every_bar_sized_number() {
        // Each of these digits also names a bar the engine analyses. With a
        // currency in front they are amounts, and none of them is confirmed.
        let f =
            crate::integrity::facts_for(&guard("AAPL", "USD", 168.8), &config("AAPL"), None, None);
        for text in [
            "The price is $1m.",
            "The price is $5m.",
            "The price is $15m.",
            "The price is $30m.",
            "The price is $60m.",
            "The price is $21m.",
            "The price is $1h.",
            "株価は$5mです。",
        ] {
            assert!(
                !issues(&f, text).is_empty(),
                "a currency amount must stay verifiable: {text}"
            );
        }
    }

    #[test]
    fn a_bar_size_without_a_currency_is_still_a_bar_size() {
        let mut g = guard("9432.T", "JPY", 168.80);
        g.set_rsi(45.2);
        let f = crate::integrity::facts_for(&g, &config("9432.T"), None, None);
        for text in [
            "5m bar: RSI is 45.2.",
            "60m bar: RSI is 45.2.",
            "5分足のRSIは45.2です。",
            "1時間足のRSIは45.2です。",
        ] {
            survives(&f, text);
        }
    }

    // ── I3: one date, three spellings, one verdict ───────────────────────────

    #[test]
    fn a_full_date_stamps_the_observation_and_does_not_date_the_claim() {
        let f = fy2025();
        // The bar is from 2026 and the fundamentals from FY2025. Naming the date
        // the analysis ran must not refuse the annual figure — in any spelling.
        for text in [
            "2026-09-08時点の売上高は13.704兆円です。",
            "2026年9月8日時点の売上高は13.704兆円です。",
            "As of 2026-09-08, revenue is 13.704 trillion yen.",
            "As of September 8, 2026, revenue is 13.704 trillion yen.",
        ] {
            survives(&f, text);
        }
    }

    #[test]
    fn a_fiscal_year_still_dates_the_claim_in_every_spelling() {
        let f = fy2025();
        survives(&f, "FY2025の売上高は13.704兆円です。");
        survives(&f, "2025年度の売上高は13.704兆円です。");
        survives(&f, "In FY2025 revenue was 13.704 trillion yen.");
        caught(&f, "FY2024の売上高は13.704兆円です。");
        caught(&f, "2024年度の売上高は13.704兆円です。");
        caught(&f, "In FY2024 revenue was 13.704 trillion yen.");
    }

    #[test]
    fn a_full_date_does_not_stop_a_wrong_value_from_being_caught() {
        let f = fy2025();
        caught(&f, "2026-09-08時点の売上高は13.8兆円です。");
        caught(&f, "As of 2026-09-08, revenue is 13.8 trillion yen.");
    }
}

#[cfg(test)]
mod paired_claim_tests {
    //! Several claims in one sentence. Every case is a pair: the same sentence
    //! shape once with each figure under its own subject (must survive) and once
    //! with two of them exchanged (must be caught). A rule that only looks at the
    //! sentence as a whole passes the first and the second alike, which is what
    //! these pairs exist to prevent.
    use super::validate_ollama_output_integrity;
    use crate::config::AnalysisMode;
    use crate::technical::types::TechnicalDataGuard;

    fn guard(ticker: &str, name: &str, currency: Option<&str>, close: f64) -> TechnicalDataGuard {
        let mut g = TechnicalDataGuard::new(ticker.to_string(), "2026-09-08".to_string());
        g.set_name(name);
        g.set_close(close);
        if let Some(c) = currency {
            g.set_currency(c);
        }
        g
    }

    fn config(ticker: &str) -> crate::config::Config {
        crate::config::Config {
            ticker: ticker.to_string(),
            ..crate::config::Config::default()
        }
    }

    fn bar(mode: AnalysisMode, close: f64) -> crate::integrity::SymbolFacts {
        let g = guard("9432.T", "日本電信電話", Some("JPY"), close);
        let mut cfg = config("9432.T");
        cfg.analysis_mode = mode;
        crate::integrity::SymbolFacts::from_sources(&g, &cfg, None, None).with_timeframe(mode)
    }

    fn two_bars() -> crate::integrity::ConfirmedFactSet {
        crate::integrity::facts_from_parts([
            bar(AnalysisMode::Daily, 168.8),
            bar(AnalysisMode::Intraday60m, 170.2),
        ])
    }

    fn issues(f: &crate::integrity::ConfirmedFactSet, content: &str) -> Vec<String> {
        validate_ollama_output_integrity("", content, Some(f))
    }

    fn survives(f: &crate::integrity::ConfirmedFactSet, content: &str) {
        assert!(
            issues(f, content).is_empty(),
            "must survive: {content} -> {:?}",
            issues(f, content)
        );
    }

    fn caught(f: &crate::integrity::ConfirmedFactSet, content: &str) {
        assert!(!issues(f, content).is_empty(), "must be caught: {content}");
    }

    // ── H1: the bar each figure is written under ────────────────────────────

    #[test]
    fn two_bars_in_one_sentence_keep_their_own_values() {
        let f = two_bars();
        survives(&f, "日足の終値は168.8円、1時間足の終値は170.2円です。");
        survives(&f, "The daily close is 168.8 and the 60m close is 170.2.");
    }

    #[test]
    fn two_bars_with_their_values_exchanged_are_caught() {
        let f = two_bars();
        caught(&f, "日足の終値は170.2円、1時間足の終値は168.8円です。");
        caught(&f, "The daily close is 170.2 and the 60m close is 168.8.");
    }

    #[test]
    fn a_bar_that_was_not_analysed_cannot_carry_a_value() {
        let f = two_bars();
        caught(&f, "週足の終値は168.8円です。");
        caught(&f, "The weekly close is 168.8.");
    }

    // ── H1: the year each figure is written under ───────────────────────────

    fn two_fiscal_years() -> crate::integrity::ConfirmedFactSet {
        // One instrument; the fundamentals belong to FY2025.
        let g = guard("9432.T", "日本電信電話", Some("JPY"), 168.80);
        let mut cfg = config("9432.T");
        cfg.fundamental = true;
        let fundamental = crate::fundamental::FundamentalData::build(
            crate::fundamental::FundamentalInputs {
                currency: "JPY".to_string(),
                fiscal_period: Some("FY (FY ends 2025-03-31)".to_string()),
                revenue: Some(13_704_000_000_000.0),
                ..Default::default()
            },
            Some(168.80),
        );
        crate::integrity::facts_for(&g, &cfg, None, Some(&fundamental))
    }

    #[test]
    fn a_year_stated_later_in_the_sentence_applies_from_there() {
        let f = two_fiscal_years();
        survives(&f, "FY2025の売上高は13.704兆円です。");
        // The same figure repeated under another fiscal year is a different claim.
        caught(
            &f,
            "FY2025の売上高は13.704兆円で、FY2024でも13.704兆円でした。",
        );
        caught(
            &f,
            "Revenue was 13.704 trillion yen in FY2025 and 13.704 trillion yen in FY2024.",
        );
    }

    #[test]
    fn a_shorthand_amount_is_not_read_as_a_bar_size() {
        // "$21m" is twenty-one million, not a twenty-one-minute bar. Reading it
        // as a bar would exempt the figure from verification entirely.
        let f = two_bars();
        caught(&f, "The close is $21m.");
        caught(&f, "Revenue is $21m.");
    }

    #[test]
    fn a_japanese_fiscal_year_pair_is_checked_per_claim() {
        let f = two_fiscal_years();
        survives(&f, "2025年度の売上高は13.704兆円です。");
        caught(
            &f,
            "2025年度の売上高は13.704兆円で、2024年度も13.704兆円でした。",
        );
    }

    // ── H2: brackets are not a blanket exemption ────────────────────────────

    #[test]
    fn a_period_specification_in_brackets_is_still_a_parameter() {
        let mut g = guard("9432.T", "日本電信電話", Some("JPY"), 168.80);
        g.set_rsi(45.2);
        g.set_macd(4.6815);
        g.set_signal(4.6713);
        let f = crate::integrity::facts_for(&g, &config("9432.T"), None, None);
        survives(&f, "RSI(14)は45.2です。");
        survives(&f, "RSI(14) is 45.2.");
        survives(&f, "MACD(12,26,9)は4.6815です。");
    }

    #[test]
    fn a_fabricated_reading_inside_brackets_is_caught() {
        let mut g = guard("9432.T", "日本電信電話", Some("JPY"), 168.80);
        g.set_rsi(45.2);
        let f = crate::integrity::facts_for(&g, &config("9432.T"), None, None);
        caught(&f, "RSI（99.99）は買われ過ぎです。");
        caught(&f, "RSI (value: 99.99) indicates overbought conditions.");
        caught(&f, "RSI (99.99) indicates overbought conditions.");
        // The true reading written the same way still passes.
        survives(&f, "RSI（45.20）は中立です。");
    }

    // ── H3: a generic name beside a specific one ────────────────────────────

    #[test]
    fn a_generic_indicator_name_is_recognised_beside_a_specific_one() {
        let mut g = guard("9432.T", "日本電信電話", Some("JPY"), 168.80);
        g.set_rsi(45.2);
        g.set_ema_short(171.45);
        g.set_ema_long(166.88);
        let mut cfg = config("9432.T");
        cfg.enabled_extensions = vec![crate::config::ExtensionIndicator::Ema];
        let f = crate::integrity::facts_for(&g, &cfg, None, None);
        survives(&f, "RSI is 45.2 and EMA is 166.88.");
        survives(&f, "RSIは45.2、EMAは171.45です。");
        caught(&f, "RSI is 45.2 and EMA is 45.2.");
        caught(&f, "RSIは166.88、EMAは166.88です。");
    }

    // ── H4: a loaded instrument is named however short its code ─────────────

    fn one_letter_symbols() -> crate::integrity::ConfirmedFactSet {
        crate::integrity::facts_from_parts([
            crate::integrity::SymbolFacts::from_sources(
                &guard("A", "Agilent", Some("USD"), 168.8),
                &config("A"),
                None,
                None,
            ),
            crate::integrity::SymbolFacts::from_sources(
                &guard("T", "AT&T", Some("USD"), 400.0),
                &config("T"),
                None,
                None,
            ),
        ])
    }

    #[test]
    fn one_letter_codes_keep_their_own_values() {
        let f = one_letter_symbols();
        survives(&f, "A price is $168.8 and T price is $400.");
        caught(&f, "A price is $400 and T price is $168.8.");
    }

    #[test]
    fn a_code_that_collides_with_an_ordinary_word_is_still_the_loaded_instrument() {
        let f = crate::integrity::facts_from_parts([
            crate::integrity::SymbolFacts::from_sources(
                &guard("IT", "Gartner", Some("USD"), 168.8),
                &config("IT"),
                None,
                None,
            ),
            crate::integrity::SymbolFacts::from_sources(
                &guard("AAPL", "Apple Inc.", Some("USD"), 400.0),
                &config("AAPL"),
                None,
                None,
            ),
        ]);
        survives(&f, "IT price is $168.8 and AAPL price is $400.");
        caught(&f, "IT price is $400 and AAPL price is $168.8.");
    }

    #[test]
    fn ordinary_english_still_survives_when_no_such_code_is_loaded() {
        let f = crate::integrity::facts_from_parts([crate::integrity::SymbolFacts::from_sources(
            &guard("AAPL", "Apple Inc.", Some("USD"), 168.8),
            &config("AAPL"),
            None,
            None,
        )]);
        survives(&f, "I see the price at $168.8.");
        survives(&f, "IT spending aside, the price is $168.8.");
    }

    // ── H5: a quantity is not an amount ─────────────────────────────────────

    #[test]
    fn a_share_count_may_not_be_written_as_money() {
        let mut g = guard("9432.T", "日本電信電話", Some("JPY"), 168.80);
        g.set_latest_volume(1000.0);
        let f = crate::integrity::facts_for(&g, &config("9432.T"), None, None);
        survives(&f, "出来高は1000です。");
        survives(&f, "Volume is 1000.");
        caught(&f, "Volume is $1000.");
        caught(&f, "出来高は1000円です。");
    }

    #[test]
    fn a_magnitude_on_a_quantity_is_still_a_quantity() {
        let mut g = guard("9432.T", "日本電信電話", Some("JPY"), 168.80);
        g.set_latest_volume(12_000_000.0);
        let f = crate::integrity::facts_for(&g, &config("9432.T"), None, None);
        survives(&f, "Volume is 12 million.");
        survives(&f, "出来高は1200万です。");
        caught(&f, "Volume is $12 million.");
    }

    // ── H6: the change label carries two figures; a level is not a price ────

    #[test]
    fn the_change_is_confirmed_as_an_amount_and_as_a_percentage() {
        let mut g = guard("9432.T", "日本電信電話", Some("JPY"), 168.80);
        g.set_previous_close(172.60);
        let f = crate::integrity::facts_for(&g, &config("9432.T"), None, None);
        survives(&f, "前営業日比は-3.80円です。");
        survives(&f, "前営業日比は-2.20%です。");
        survives(&f, "The change vs prev close is -2.20%.");
        caught(&f, "前営業日比は-2.50%です。");
        caught(&f, "The change vs prev close is -4.00.");
    }

    #[test]
    fn a_fibonacci_level_identifier_is_not_a_price_claim() {
        let mut g = guard("9432.T", "日本電信電話", Some("JPY"), 168.80);
        g.set_fibo_38_2(165.0);
        g.set_fibo_50_0(170.0);
        g.set_fibo_61_8(175.0);
        let mut cfg = config("9432.T");
        cfg.enabled_extensions = vec![crate::config::ExtensionIndicator::Fibonacci];
        let f = crate::integrity::facts_for(&g, &cfg, None, None);
        survives(&f, "38.2%水準は165.00円。");
        survives(&f, "38.2%: 165.00 / 50.0%: 170.00 / 61.8%: 175.00");
        survives(&f, "The 38.2% level is 165.00.");
        caught(&f, "38.2%水準は166.00円。");
        caught(&f, "The 38.2% level is 166.00.");
    }

    #[test]
    fn an_ordinary_percentage_is_not_read_as_a_level_identifier() {
        let mut g = guard("9432.T", "日本電信電話", Some("JPY"), 168.80);
        g.set_fibo_50_0(170.0);
        let mut cfg = config("9432.T");
        cfg.enabled_extensions = vec![crate::config::ExtensionIndicator::Fibonacci];
        let f = crate::integrity::facts_for(&g, &cfg, None, None);
        // Not marked as a level, so it stays a claim — and an unconfirmed one.
        caught(&f, "上昇率は50.0%です。");
        caught(&f, "The gain is 50.0%.");
    }
}

#[cfg(test)]
mod multi_timeframe_tests {
    //! The Web multi-timeframe analysis loads one instrument several times, once
    //! per bar. A value confirmed on one bar must not pass as another bar's.
    use super::validate_ollama_output_integrity;
    use crate::config::AnalysisMode;
    use crate::technical::types::TechnicalDataGuard;

    fn timeframe(mode: AnalysisMode, close: f64) -> crate::integrity::SymbolFacts {
        let config = crate::config::Config {
            ticker: "9432.T".to_string(),
            analysis_mode: mode,
            ..crate::config::Config::default()
        };
        let mut g = TechnicalDataGuard::new("9432.T".to_string(), "2026-09-08".to_string());
        g.set_name("日本電信電話");
        g.set_currency("JPY");
        g.set_close(close);
        crate::integrity::SymbolFacts::from_sources(&g, &config, None, None).with_timeframe(mode)
    }

    fn facts() -> crate::integrity::ConfirmedFactSet {
        crate::integrity::facts_from_parts([
            timeframe(AnalysisMode::Daily, 168.8),
            timeframe(AnalysisMode::Intraday60m, 170.2),
        ])
    }

    fn issues(content: &str) -> Vec<String> {
        validate_ollama_output_integrity("", content, Some(&facts()))
    }

    fn caught(content: &str) -> bool {
        !issues(content).is_empty()
    }

    #[test]
    fn each_bars_value_is_accepted_for_its_own_bar() {
        for text in [
            "日足の終値は168.8円です。",
            "1時間足の終値は170.2円です。",
            "The daily close is 168.8.",
            "The 60m close is 170.2.",
        ] {
            assert!(!caught(text), "{text}: {:?}", issues(text));
        }
    }

    #[test]
    fn one_bars_value_stated_for_another_bar_is_caught() {
        assert!(caught("1時間足の終値は168.8円です。"));
        assert!(caught("日足の終値は170.2円です。"));
        assert!(caught("The daily close is 170.2."));
    }

    #[test]
    fn a_value_with_no_bar_named_may_be_either_bar() {
        assert!(!caught("終値は168.8円です。"));
        assert!(!caught("終値は170.2円です。"));
        // A value belonging to neither bar is still refused.
        assert!(caught("終値は999.9円です。"));
    }
}

#[cfg(test)]
mod multi_symbol_attribution_tests {
    //! A number belongs to the instrument its own sentence names, in the unit
    //! that instrument's values are quoted in, and only when the indicator it is
    //! offered as a reading of actually has a confirmed value.
    use super::validate_ollama_output_integrity;
    use crate::technical::types::TechnicalDataGuard;

    /// One instrument, priced in `currency`, with the readings given.
    fn symbol(
        ticker: &str,
        name: &str,
        currency: Option<&str>,
        close: f64,
        rsi: Option<f64>,
    ) -> crate::integrity::SymbolFacts {
        let config = crate::config::Config {
            ticker: ticker.to_string(),
            ..crate::config::Config::default()
        };
        let mut g = TechnicalDataGuard::new(ticker.to_string(), "2026-09-08".to_string());
        g.set_name(name);
        g.set_close(close);
        if let Some(c) = currency {
            g.set_currency(c);
        }
        if let Some(v) = rsi {
            g.set_rsi(v);
        }
        crate::integrity::SymbolFacts::from_sources(&g, &config, None, None)
    }

    fn two_symbols() -> crate::integrity::ConfirmedFactSet {
        crate::integrity::facts_from_parts([
            symbol("AAPL", "Apple Inc.", Some("USD"), 168.8, Some(45.2)),
            symbol("MSFT", "Microsoft Corp.", Some("USD"), 400.0, Some(52.0)),
        ])
    }

    fn issues(facts: &crate::integrity::ConfirmedFactSet, content: &str) -> Vec<String> {
        validate_ollama_output_integrity("", content, Some(facts))
    }

    fn caught(facts: &crate::integrity::ConfirmedFactSet, content: &str) -> bool {
        !issues(facts, content).is_empty()
    }

    // ── G3: each number belongs to the instrument its sentence names ─────────

    #[test]
    fn a_correct_two_symbol_comparison_survives() {
        let f = two_symbols();
        assert!(
            !caught(&f, "AAPL price is $168.8. MSFT price is $400."),
            "{:?}",
            issues(&f, "AAPL price is $168.8. MSFT price is $400.")
        );
        assert!(
            !caught(&f, "AAPL is at $168.8 while MSFT is at $400."),
            "{:?}",
            issues(&f, "AAPL is at $168.8 while MSFT is at $400.")
        );
    }

    #[test]
    fn one_symbols_value_presented_as_anothers_is_caught() {
        let f = two_symbols();
        assert!(caught(&f, "AAPL price is $168.8. MSFT price is $168.8."));
        assert!(caught(&f, "AAPL is at $400 while MSFT is at $168.8."));
    }

    #[test]
    fn a_value_attributed_to_an_unloaded_symbol_is_caught() {
        let f = two_symbols();
        assert!(caught(&f, "NVDA price is $168.8."));
    }

    #[test]
    fn an_english_period_ends_a_sentence_but_a_decimal_point_does_not() {
        let f = two_symbols();
        // Without correct splitting the second sentence would inherit AAPL and
        // the correct $400 would be refused.
        let text = "AAPL closed at $168.8. MSFT closed at $400.";
        assert!(!caught(&f, text), "{:?}", issues(&f, text));
    }

    // ── G4: a reading of an indicator that has no confirmed value ────────────

    #[test]
    fn a_reading_of_an_uncomputed_indicator_is_caught() {
        // This instrument has no RSI at all.
        let f = crate::integrity::facts_from_parts([symbol(
            "AAPL",
            "Apple Inc.",
            Some("USD"),
            168.8,
            None,
        )]);
        assert!(caught(&f, "RSI is 99."));
        assert!(caught(&f, "RSIは99です。"));
    }

    #[test]
    fn a_computed_indicator_is_still_citable() {
        let f = crate::integrity::facts_from_parts([symbol(
            "AAPL",
            "Apple Inc.",
            Some("USD"),
            168.8,
            Some(45.2),
        )]);
        assert!(
            !caught(&f, "RSI is 45.2."),
            "{:?}",
            issues(&f, "RSI is 45.2.")
        );
    }

    #[test]
    fn an_ordinary_number_is_not_treated_as_a_reading() {
        let f = two_symbols();
        // Not offered as a reading of any indicator: not a claim to verify.
        assert!(!caught(&f, "There are 3 reasons to be cautious."));
        assert!(!caught(&f, "注意すべき理由は3つあります。"));
    }

    // ── G6: currency and unit ───────────────────────────────────────────────

    #[test]
    fn a_foreign_currency_symbol_is_caught() {
        let f = crate::integrity::facts_from_parts([symbol(
            "9432.T",
            "日本電信電話",
            Some("JPY"),
            168.8,
            Some(45.2),
        )]);
        assert!(caught(&f, "The price is €168.8."));
        assert!(caught(&f, "The price is $168.8."));
        assert!(!caught(&f, "The price is ¥168.8."));
    }

    #[test]
    fn a_currency_symbol_on_a_unitless_reading_is_caught() {
        let f = crate::integrity::facts_from_parts([symbol(
            "9432.T",
            "日本電信電話",
            Some("JPY"),
            168.8,
            Some(45.2),
        )]);
        // RSI is an index, not money.
        assert!(caught(&f, "RSI is $45.2."));
        assert!(!caught(&f, "RSI is 45.2."));
    }

    // ── G7: the market currency reaches the confirmed data on its own ────────

    #[test]
    fn a_dollar_price_is_accepted_for_a_us_listing_without_fundamentals() {
        let f = crate::integrity::facts_from_parts([symbol(
            "AAPL",
            "Apple Inc.",
            Some("USD"),
            168.8,
            Some(45.2),
        )]);
        let text = "The price is $168.8.";
        assert!(!caught(&f, text), "{:?}", issues(&f, text));
    }

    #[test]
    fn an_unreported_currency_is_not_used_to_refuse_a_written_one() {
        // The labelled Stooq fallback reports no currency. An unknown cannot
        // contradict what the model wrote, so the currency is not checked — the
        // value still is.
        let f = crate::integrity::facts_from_parts([symbol(
            "AAPL",
            "Apple Inc.",
            None,
            168.8,
            Some(45.2),
        )]);
        assert!(!caught(&f, "The price is $168.8."));
        assert!(caught(&f, "The price is $170.0."));
    }

    // ── G9: ordinary English is not a ticker ────────────────────────────────

    #[test]
    fn an_english_pronoun_is_not_read_as_a_symbol() {
        let f = crate::integrity::facts_from_parts([symbol(
            "AAPL",
            "Apple Inc.",
            Some("USD"),
            168.8,
            Some(45.2),
        )]);
        for text in [
            "I see RSI at 45.2.",
            "We see RSI at 45.2.",
            "AI aside, RSI is 45.2.",
            "IT services aside, RSI is 45.2.",
        ] {
            assert!(!caught(&f, text), "{text}: {:?}", issues(&f, text));
        }
    }
}

#[cfg(test)]
mod number_role_tests {
    //! A number is first read for what it is doing in the sentence. A window
    //! length and a date are not readings, and a price is not a year.
    use super::validate_ollama_output_integrity;
    use crate::technical::types::TechnicalDataGuard;

    fn facts(close: f64) -> crate::integrity::ConfirmedFactSet {
        let config = crate::config::Config {
            ticker: "9432.T".to_string(),
            ..crate::config::Config::default()
        };
        let mut g = TechnicalDataGuard::new("9432.T".to_string(), "2026-09-08".to_string());
        g.set_name("日本電信電話");
        g.set_currency("JPY");
        g.set_close(close);
        g.set_rsi(45.2);
        crate::integrity::facts_for(&g, &config, None, None)
    }

    fn issues(close: f64, content: &str) -> Vec<String> {
        validate_ollama_output_integrity("", content, Some(&facts(close)))
    }

    fn caught(close: f64, content: &str) -> bool {
        !issues(close, content).is_empty()
    }

    #[test]
    fn a_period_parameter_is_not_a_reading() {
        for text in [
            "RSI(14) is 45.2.",
            "RSI（14）は45.2です。",
            "The 14-day RSI is 45.2.",
            "14日RSIは45.2です。",
        ] {
            assert!(!caught(168.8, text), "{text}: {:?}", issues(168.8, text));
        }
    }

    #[test]
    fn a_date_is_not_a_reading() {
        for text in [
            "As of 2026-09-08, RSI is 45.2.",
            "2026年9月8日時点で、RSIは45.2です。",
        ] {
            assert!(!caught(168.8, text), "{text}: {:?}", issues(168.8, text));
        }
    }

    #[test]
    fn a_price_that_looks_like_a_year_is_still_a_price() {
        // The confirmed close is 2025; "2025" must be read as the price it is.
        let text = "終値は2025円です。";
        assert!(!caught(2025.0, text), "{:?}", issues(2025.0, text));
        let text_en = "The close is 2025.";
        assert!(!caught(2025.0, text_en), "{:?}", issues(2025.0, text_en));
        // A different value for the same indicator is still refused.
        assert!(caught(2025.0, "終値は2026円です。"));
    }

    #[test]
    fn a_claim_dated_to_another_year_is_still_caught() {
        assert!(caught(168.8, "2019年の終値は168.80円でした。"));
        assert!(caught(168.8, "In 2019 the close was 168.80."));
    }
}

#[cfg(test)]
mod fundamental_period_tests {
    //! A fundamental value belongs to its reporting period, not to the bar the
    //! indicators were computed on.
    use super::validate_ollama_output_integrity;
    use crate::technical::types::TechnicalDataGuard;

    fn facts() -> crate::integrity::ConfirmedFactSet {
        let config = crate::config::Config {
            ticker: "9432.T".to_string(),
            fundamental: true,
            ..crate::config::Config::default()
        };
        let mut g = TechnicalDataGuard::new("9432.T".to_string(), "2026-09-08".to_string());
        g.set_name("日本電信電話");
        g.set_currency("JPY");
        g.set_close(168.80);
        let f = crate::fundamental::FundamentalData::build(
            crate::fundamental::FundamentalInputs {
                currency: "JPY".to_string(),
                fiscal_period: Some("FY (FY ends 2027-03-31)".to_string()),
                revenue: Some(13_704_000_000_000.0),
                ..Default::default()
            },
            Some(168.80),
        );
        crate::integrity::facts_for(&g, &config, None, Some(&f))
    }

    fn caught(content: &str) -> bool {
        !validate_ollama_output_integrity("", content, Some(&facts())).is_empty()
    }

    #[test]
    fn a_fundamental_value_is_checked_against_its_own_fiscal_year() {
        // The fiscal year is 2027; the bar year is 2026. Stating the fiscal year
        // must not be refused just because the bar is from another year.
        assert!(!caught("FY2027の売上高は13.704兆円です。"));
        assert!(!caught("In FY2027 revenue was 13.704 trillion yen."));
        // A different fiscal year is refused.
        assert!(caught("FY2019の売上高は13.704兆円です。"));
    }

    // ── G8: rounding at the magnitude it was written in ──────────────────────

    #[test]
    fn a_rounded_magnitude_is_accepted_and_a_changed_one_is_not() {
        assert!(!caught("売上高は約13.7兆円です。"));
        assert!(!caught("Revenue is about 13.7 trillion yen."));
        assert!(caught("売上高は約13.8兆円です。"));
        assert!(caught("Revenue is about 13.8 trillion yen."));
    }

    #[test]
    fn a_magnitude_with_a_foreign_currency_word_is_caught() {
        // Same digits, another currency: 13704 billion dollars is not 13.704兆円.
        assert!(caught("Revenue is 13704 billion dollars."));
    }
}

#[cfg(test)]
mod generic_indicator_terms_tests {
    //! An unqualified indicator name may cite either leg; a qualified one may not
    //! cite the other.
    use super::validate_ollama_output_integrity;
    use crate::technical::types::TechnicalDataGuard;

    fn facts() -> crate::integrity::ConfirmedFactSet {
        let config = crate::config::Config {
            ticker: "9432.T".to_string(),
            enabled_extensions: vec![crate::config::ExtensionIndicator::Ema],
            ..crate::config::Config::default()
        };
        let mut g = TechnicalDataGuard::new("9432.T".to_string(), "2026-09-08".to_string());
        g.set_name("日本電信電話");
        g.set_currency("JPY");
        g.set_close(168.80);
        g.set_ema_short(171.45);
        g.set_ema_long(166.88);
        crate::integrity::facts_for(&g, &config, None, None)
    }

    fn issues(content: &str) -> Vec<String> {
        validate_ollama_output_integrity("", content, Some(&facts()))
    }

    fn caught(content: &str) -> bool {
        !issues(content).is_empty()
    }

    #[test]
    fn an_unqualified_ema_may_be_either_leg() {
        for text in [
            "EMA is 166.88.",
            "EMA is 171.45.",
            "EMAは166.88です。",
            "EMAは171.45です。",
        ] {
            assert!(!caught(text), "{text}: {:?}", issues(text));
        }
    }

    #[test]
    fn a_qualified_ema_may_not_cite_the_other_leg() {
        assert!(caught("長期EMAは171.45です。"));
        assert!(caught("The long EMA is 171.45."));
        assert!(!caught("長期EMAは166.88です。"));
        assert!(!caught("The long EMA is 166.88."));
    }
}

#[cfg(test)]
mod attribution_tests {
    //! R3: a number passes only when it belongs to the symbol, indicator, unit,
    //! sign and period the sentence claims. Digits occurring somewhere in the
    //! prompt is not a citation.
    //!
    //! The reference is built the way production builds it — from the guard and
    //! the fundamental data — and the prompt is the real `build_analysis_prompt`
    //! output, so a change to either label set breaks these tests rather than
    //! silently weakening the guard. Every case runs in **both** languages (R2).
    use super::validate_ollama_output_integrity;
    use crate::technical::types::TechnicalDataGuard;

    /// The confirmed values used by every case here.
    fn fixture(lang: &str) -> (String, crate::integrity::ConfirmedFactSet) {
        let config = crate::config::Config {
            ticker: "9432.T".to_string(),
            lang: lang.to_string(),
            fundamental: true,
            enabled_extensions: vec![
                crate::config::ExtensionIndicator::Ema,
                crate::config::ExtensionIndicator::Sma,
                crate::config::ExtensionIndicator::Bollinger,
                crate::config::ExtensionIndicator::Vwap,
            ],
            ..crate::config::Config::default()
        };
        let mut g = TechnicalDataGuard::new("9432.T".to_string(), "2026-09-08".to_string());
        g.set_name("日本電信電話");
        g.set_close(168.80);
        g.set_previous_close(172.60);
        g.set_rsi(45.20);
        g.set_macd(4.6815);
        g.set_signal(4.6713);
        g.set_ema_short(171.45);
        g.set_ema_long(166.88);
        g.set_sma_short(173.08);
        g.set_sma_long(166.51);
        g.set_bb_upper(180.00);
        g.set_bb_lower(160.00);
        g.set_bb_percent_b(0.69);
        g.set_vwap(168.96);
        g.set_adx(22.41);

        let fundamental = crate::fundamental::FundamentalData::build(
            crate::fundamental::FundamentalInputs {
                currency: "JPY".to_string(),
                revenue: Some(13_704_000_000_000.0),
                operating_income: Some(1_692_000_000_000.0),
                net_income: Some(1_100_000_000_000.0),
                eps: Some(12.9),
                equity: Some(9_000_000_000_000.0),
                ..Default::default()
            },
            Some(168.80),
        );

        let prompt = crate::prompt::build_analysis_prompt(&config, &g, None, Some(&fundamental));
        let facts = crate::integrity::facts_for(&g, &config, Some("JPY"), Some(&fundamental));
        (prompt, facts)
    }

    /// Run one sentence in one language.
    fn caught_in(lang: &str, content: &str) -> bool {
        let (prompt, facts) = fixture(lang);
        !validate_ollama_output_integrity(&prompt, content, Some(&facts)).is_empty()
    }

    /// R2: the same claim must get the same verdict in both languages. `ja` and
    /// `en` are the sentence in each language; `expected` is what both must give.
    fn both(ja: &str, en: &str, expected: bool) {
        assert_eq!(
            caught_in("ja", ja),
            expected,
            "ja verdict differs from the expectation: {ja}"
        );
        assert_eq!(
            caught_in("en", en),
            expected,
            "en verdict differs from the expectation: {en}"
        );
    }

    #[test]
    fn a_correct_citation_survives_in_both_languages() {
        both("RSIは45.20です。", "RSI is 45.20.", false);
        both("VWAPは168.96です。", "VWAP is 168.96.", false);
        // Display rounding is not a changed value.
        both("終値は168.8円です。", "The close is 168.8.", false);
    }

    #[test]
    fn a_wrong_value_for_a_named_indicator_is_caught_in_both_languages() {
        both("RSIは99.99です。", "RSI is 99.99.", true);
        both("ADXは30.00です。", "ADX is 30.00.", true);
    }

    #[test]
    fn a_value_taken_from_another_indicator_is_caught_in_both_languages() {
        // 168.96 is the VWAP, not the close.
        both("終値は168.96円です。", "The close is 168.96.", true);
        // 4.6713 is the signal, not the MACD line.
        both("MACDは4.6713です。", "MACD is 4.6713.", true);
    }

    #[test]
    fn two_indicators_swapped_in_one_sentence_are_caught() {
        // Both numbers exist in the confirmed data, each under the other's
        // indicator. Presence is not attribution.
        both(
            "RSIは4.6815、MACDは45.20です。",
            "RSI is 4.6815 and MACD is 45.20.",
            true,
        );
        both(
            "RSIは45.20、MACDは4.6815です。",
            "RSI is 45.20 and MACD is 4.6815.",
            false,
        );
    }

    #[test]
    fn a_derived_bollinger_reading_is_confirmed_with_its_bands() {
        // Bandwidth is derived from the band edges, so it is citable exactly when
        // they are — and a fabricated one is refused rather than passing for want
        // of a confirmed counterpart.
        let (prompt, facts) = fixture("ja");
        let confirmed = facts
            .single_value("bandwidth")
            .expect("bandwidth is confirmed when the bands are");
        let good = format!("バンド幅は{confirmed:.2}%です。");
        assert!(
            validate_ollama_output_integrity(&prompt, &good, Some(&facts)).is_empty(),
            "the computed bandwidth must be citable"
        );
        assert!(
            !validate_ollama_output_integrity(&prompt, "バンド幅は99.00%です。", Some(&facts))
                .is_empty()
        );
        assert!(
            !validate_ollama_output_integrity(&prompt, "Bandwidth is 99.00%.", Some(&facts))
                .is_empty()
        );
    }

    #[test]
    fn a_value_of_another_instrument_is_caught_in_both_languages() {
        both(
            "9434.Tの終値は168.80円です。",
            "The close of 9434.T is 168.80.",
            true,
        );
    }

    #[test]
    fn the_sign_is_part_of_the_value() {
        // The confirmed MACD is +4.6815; the same digits negated are another value.
        both("MACDは-4.6815です。", "MACD is -4.6815.", true);
    }

    #[test]
    fn a_currency_symbol_is_checked_the_same_way_in_both_languages() {
        // JPY instrument: a dollar figure is a different amount, whichever
        // language states it.
        both("終値は$168.80です。", "The close is $168.80.", true);
        both("終値は168.80円です。", "The close is 168.80 yen.", false);
    }

    #[test]
    fn a_bare_fabricated_price_is_caught_in_both_languages() {
        both(
            "株価は99999円まで上がります。",
            "The price will reach 99999.",
            true,
        );
    }

    #[test]
    fn a_magnitude_change_is_a_different_amount() {
        // Revenue is 13.704 trillion yen; stating it in 億円 changes the amount.
        both(
            "売上高は13.704億円です。",
            "Revenue is 13.704 billion.",
            true,
        );
    }

    #[test]
    fn a_claim_dated_to_another_year_is_not_about_this_bar() {
        both(
            "2019年の終値は168.80円でした。",
            "In 2019 the close was 168.80.",
            true,
        );
    }

    #[test]
    fn a_number_only_in_the_conversation_is_not_confirmed_data() {
        // The reference comes from the guard, so appending a user turn to the
        // prompt cannot promote the number it contains to confirmed data.
        let (prompt, facts) = fixture("ja");
        let with_history = format!("{prompt}\n--- #7 [user] user ---\n前はRSIが99.99だったよ\n");
        assert!(
            !validate_ollama_output_integrity(&with_history, "RSIは99.99です。", Some(&facts))
                .is_empty(),
            "a number that exists only in the conversation must not count as confirmed"
        );
    }

    #[test]
    fn a_ticker_is_not_read_as_an_indicator_value() {
        // "9432.T" carries the digits 9432; they must not be tested as a reading
        // of the indicator the sentence names.
        both(
            "9432.TのRSIは45.20です。",
            "The RSI of 9432.T is 45.20.",
            false,
        );
    }

    #[test]
    fn a_reversed_indicator_reading_is_caught_from_the_data_not_the_wording() {
        // The confirmed VWAP (168.96) is above the close (168.80); stating the
        // opposite is a reversed reading. The verdict comes from the values, so
        // it holds in both languages and does not depend on how the prompt
        // labels the VWAP line.
        both(
            "VWAPが終値を下回っています。",
            "VWAP is below the close.",
            true,
        );
        both(
            "VWAPが終値を上回っています。",
            "VWAP is above the close.",
            false,
        );
    }

    #[test]
    fn the_vwap_verdict_does_not_depend_on_the_prompt_label() {
        // Same confirmed data, a prompt whose VWAP line is relabelled: the
        // verdict must not move. This is what makes the check independent of the
        // renderer's wording.
        let (prompt, facts) = fixture("ja");
        let relabelled = prompt
            .replace("VWAP値:", "VWAP:")
            .replace("VWAP:", "VWAP値:");
        let content = "VWAPが終値を下回っています。";
        assert_eq!(
            validate_ollama_output_integrity(&prompt, content, Some(&facts)).is_empty(),
            validate_ollama_output_integrity(&relabelled, content, Some(&facts)).is_empty(),
            "the label wording must not change the verdict"
        );
        assert!(
            !validate_ollama_output_integrity(&relabelled, content, Some(&facts)).is_empty(),
            "the reversed reading must still be caught"
        );
    }

    #[test]
    fn a_full_correct_answer_survives_untouched_in_both_languages() {
        // Attribution has to be strict without becoming unusable: an answer that
        // cites the confirmed values correctly, in ordinary prose, must produce
        // no issue at all. This is the counterweight to every rejection test.
        let ja = "\
9432.Tの終値は168.80円で、前営業日終値172.60円から下落しました。\
RSI(14)は45.20と中立圏にあり、MACDは4.6815でシグナル4.6713をわずかに上回っています。\
短期EMA171.45が長期EMA166.88を上回る一方、終値はVWAP168.96を下回っています。\
ADXは22.41でトレンドは弱く、方向感に乏しい展開です。\
売上高13.704兆円、営業利益1.692兆円という規模を踏まえると、当面は様子見が妥当でしょう。";
        let en = "\
The close of 9432.T is 168.80, down from a previous close of 172.60. \
RSI(14) is 45.20, a neutral reading, and MACD is 4.6815 against a signal line of 4.6713. \
The short EMA at 171.45 sits above the long EMA at 166.88, while the close is under VWAP at 168.96. \
ADX is 22.41, so the trend is weak. \
With revenue of 13.704 trillion yen and operating income of 1.692 trillion yen, \
the picture is one of scale rather than momentum.";

        for (lang, content) in [("ja", ja), ("en", en)] {
            let (prompt, facts) = fixture(lang);
            let issues = validate_ollama_output_integrity(&prompt, content, Some(&facts));
            assert!(issues.is_empty(), "[{lang}] false positives: {issues:?}");
        }
    }

    #[test]
    fn without_confirmed_data_the_guard_falls_back_to_the_presence_test() {
        // No facts passed: attribution cannot be checked, so the older
        // does-it-occur-in-the-input test remains (and the limitation is stated
        // in the remaining-limitations note, not silently assumed away).
        assert!(
            !validate_ollama_output_integrity("ただの会話です。", "終値は2600円です。", None)
                .is_empty()
        );
    }
}
