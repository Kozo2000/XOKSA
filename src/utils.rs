//! Utility functions and helpers

use crate::config::{Config, ExtensionIndicator};
use crate::technical::TechnicalDataGuard;
use anyhow::{bail, Result};
use chrono::{NaiveDateTime, Timelike};
use std::fs::read_to_string;
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;
use zeroize::Zeroizing;

/// Explicit `--env-file` override, set once at startup (before subcommand dispatch).
static ENV_OVERRIDE: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Set the process-wide `--env-file` override once, at startup, AND perform the
/// one-time migration of an existing config into the canonical location (skipped
/// when `--env-file` names an explicit file). Idempotent.
pub fn init_env_path(override_path: Option<&str>) {
    let _ = ENV_OVERRIDE.set(override_path.map(PathBuf::from));
    if override_path.is_none() {
        // Import a pre-2.7.1 desktop config or a working-dir file into the canonical
        // location once, so an upgrade keeps its settings and the config can never
        // split by launch directory afterwards. Surface a failure — silently
        // swallowing it would look to the user like lost settings.
        match xoksa_paths::migrate_env_if_needed() {
            Ok(Some(src)) => crate::logging::info(
                "XK-CFG",
                &format!(
                    "imported existing config from {} into the canonical location",
                    src.display()
                ),
            ),
            Ok(None) => {}
            Err(e) => crate::logging::warn(
                "XK-CFG",
                &format!(
                    "config migration failed ({e}); existing settings may not be applied — check permissions on the config directory"
                ),
            ),
        }
    }
}

/// Pure resolution (table-tested): an explicit `--env-file` wins; otherwise the
/// single canonical path from `xoksa-paths`. There is NO implicit `./xoksa.env`
/// fallback — a working-directory file is only ever a one-time migration source or
/// named explicitly via `--env-file`, so the config cannot split by launch dir. The
/// literal `xoksa.env` last resort only covers a host with no OS config dir.
fn pick_env_path(override_path: Option<&Path>, canonical: Option<PathBuf>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    canonical.unwrap_or_else(|| PathBuf::from("xoksa.env"))
}

/// The single `xoksa.env` path for every read and write (SOT).
pub fn env_path() -> PathBuf {
    let ov = ENV_OVERRIDE.get().and_then(|o| o.as_deref());
    pick_env_path(ov, xoksa_paths::env_file())
}

/// Scan raw argv for `--env-file <PATH>` / `--env-file=<PATH>`. Used once in
/// `main()` before subcommand dispatch so serve / apply-config / the flag CLI all
/// resolve the same file.
pub fn env_file_flag(args: &[String]) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(v) = a.strip_prefix("--env-file=") {
            return Some(v.to_string());
        }
        if a == "--env-file" {
            return args.get(i + 1).cloned();
        }
        i += 1;
    }
    None
}

/// Build a reqwest Proxy with optional NO_PROXY exclusions.
/// `no_proxy` is a comma-separated list of hosts/domains (e.g. "127.0.0.1,localhost").
pub fn build_proxy(
    proxy_url: &str,
    no_proxy: Option<&str>,
) -> Result<reqwest::Proxy, reqwest::Error> {
    let mut proxy = reqwest::Proxy::all(proxy_url)?;
    if let Some(np) = no_proxy {
        proxy = proxy.no_proxy(reqwest::NoProxy::from_string(np));
    }
    Ok(proxy)
}

/// Parses the value side (right of `=`) of a .env file entry.
/// Strips double quotes and inline `#` comments.
/// Centralizes the duplicate parsing logic from bootstrap.rs / news.rs / llm.rs.
pub fn parse_env_value(raw_val: &str) -> String {
    let raw_val = raw_val.trim();
    if raw_val.starts_with('"') {
        match (
            raw_val.len() >= 2,
            raw_val.ends_with('"'),
            raw_val.rfind('"'),
        ) {
            (true, true, _) => raw_val[1..raw_val.len() - 1].to_string(),
            (_, _, Some(pos)) if pos >= 1 => raw_val[1..pos].to_string(),
            _ => raw_val.trim_matches('"').to_string(),
        }
    } else {
        match raw_val.find('#') {
            Some(pound) => raw_val[..pound].trim_end().to_string(),
            None => raw_val.to_string(),
        }
    }
}

/// Reason a single env/config line must be rejected: over-long (bytes), null byte,
/// control character, or an embedded BOM. `None` when the line is safe. Shared by
/// the strict and lenient readers so both apply identical rules.
fn env_line_issue(line_number: usize, line: &str) -> Option<String> {
    if line.len() > 500 {
        return Some(format!(
            "line {}: line too long ({} bytes)",
            line_number,
            line.len()
        ));
    }
    if line.contains('\0') {
        return Some(format!("line {}: contains null byte", line_number));
    }
    if line
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r')
    {
        return Some(format!("line {}: contains control character", line_number));
    }
    if line.contains('\u{FEFF}') {
        return Some(format!(
            "line {}: contains BOM (Byte Order Mark)",
            line_number
        ));
    }
    None
}

/// On the first line only, a leading BOM is a file marker (not content) — strip it.
fn strip_leading_bom(line_number: usize, line: &str) -> &str {
    if line_number == 1 {
        line.strip_prefix('\u{FEFF}').unwrap_or(line)
    } else {
        line
    }
}

/// Strict reader: fails the whole file on the first offending line. Use for files
/// where one malformed line should reject the file (e.g. alias_csv).
pub fn sanitize_ascii_file_lines(path: &Path) -> Result<Vec<String>> {
    let content = read_to_string(path)
        .map_err(|e| anyhow::anyhow!("❌ File read failed: {} ({})", path.display(), e))?;
    let mut result = Vec::new();
    for (i, raw) in content.lines().enumerate() {
        let line_number = i + 1;
        let line = strip_leading_bom(line_number, raw);
        if let Some(issue) = env_line_issue(line_number, line) {
            bail!("❌ File {} {}", path.display(), issue);
        }
        result.push(line.to_string());
    }
    Ok(result)
}

/// Lenient reader for `xoksa.env`: skips offending lines instead of failing the
/// whole file, returning `(kept_lines, skipped_notes)`. A single malformed line
/// (e.g. a pasted zero-width / BOM character in an Ollama value) must not wipe
/// every other setting — the bad line is dropped and the rest still load.
pub fn sanitize_env_file_lines_lenient(path: &Path) -> std::io::Result<(Vec<String>, Vec<String>)> {
    let content = read_to_string(path)?;
    let mut kept = Vec::new();
    let mut skipped = Vec::new();
    for (i, raw) in content.lines().enumerate() {
        let line_number = i + 1;
        let line = strip_leading_bom(line_number, raw);
        match env_line_issue(line_number, line) {
            Some(issue) => skipped.push(issue),
            None => kept.push(line.to_string()),
        }
    }
    Ok((kept, skipped))
}

/// Lookup table mapping score ratio thresholds to labels and colors.
/// Entries must be ordered descending (highest threshold first).
/// Format: (threshold, label, color). The first entry where ratio >= threshold is applied.
/// 0.0 = neutral watch. Symmetric 4-level structure for buy and sell sides.
/// Ratios below -0.8 fall through to the classify_score / get_color_for_score fallback.
const SCORE_LEVELS: &[(f64, &str, &str)] = &[
    (0.8, "🟢 強い買い", "green"),
    (0.6, "🟢 買い優勢", "green"),
    (0.4, "🟢 買い傾向あり", "green"),
    (0.2, "🟡 やや買い寄り", "yellow"),
    (0.0, "⚪️ 様子見（中立）", "white"),
    (-0.2, "🟠 やや売り寄り", "orange"),
    (-0.4, "🟠 売り傾向あり", "orange"),
    (-0.6, "🔴 売り優勢", "red"),
    (-0.8, "🔴 強い売り", "red"), // hits when score_ratio >= -0.8; below -0.8 uses fallback
];

const SCORE_LEVELS_EN: &[(f64, &str, &str)] = &[
    (0.8, "🟢 Strong buy", "green"),
    (0.6, "🟢 Buy-leaning", "green"),
    (0.4, "🟢 Mild buy", "green"),
    (0.2, "🟡 Slightly bullish", "yellow"),
    (0.0, "⚪️ Neutral", "white"),
    (-0.2, "🟠 Slightly bearish", "orange"),
    (-0.4, "🟠 Mild sell", "orange"),
    (-0.6, "🔴 Sell-leaning", "red"),
    (-0.8, "🔴 Strong sell", "red"),
];

/// The label for a score ratio in `-1.0..=1.0`, in the configured language.
///
/// Thresholds live in one table per language, so the wording and the cut points
/// cannot diverge between them. A ratio below every threshold falls through to
/// the strongest sell label rather than returning nothing.
///
/// `-1.0..=1.0` のスコア比率に対するラベルを、設定言語で返す。
///
/// 閾値は言語ごとに 1 つの表にまとめてあり、文言と区切り位置が言語間でずれない。
/// どの閾値にも届かない比率は、何も返さずに終わるのではなく最も強い売りのラベルに落ちる。
pub fn classify_score(score_ratio: f64, lang: &str) -> &'static str {
    let table = if lang == "ja" {
        SCORE_LEVELS
    } else {
        SCORE_LEVELS_EN
    };
    for &(threshold, label, _) in table {
        if score_ratio >= threshold {
            return label;
        }
    }
    if lang == "ja" {
        "🔴 強い売り"
    } else {
        "🔴 Strong sell"
    }
}

/// The ANSI colour name for a score ratio, read from the same threshold table
/// [`classify_score`] uses — so the colour and the label always agree.
///
/// スコア比率に対する ANSI カラー名。[`classify_score`] と同じ閾値表から読むので、
/// 色とラベルが食い違うことはない。
pub fn get_color_for_score(score_ratio: f64) -> &'static str {
    for &(threshold, _, color) in SCORE_LEVELS {
        if score_ratio >= threshold {
            return color;
        }
    }
    "red"
}

// Per-indicator score description tables, defined as (score, description) pairs.
// To add a new indicator: add the table constant + UNKNOWN constant,
// then add one match arm to get_score_description.
const SMA_SCORE_TABLE: &[(i32, &str)] = &[
    (
        2,
        "🟢 短期SMAが長期より大幅に上 → 強い上昇トレンド → スコア+2加点",
    ),
    (
        1,
        "🟢 短期SMAが長期よりやや上 → 上昇トレンド → スコア+1加点",
    ),
    (0, "➡️ SMAが同値圏 → スコア変動なし"),
    (
        -1,
        "🔴 短期SMAが長期よりやや下 → 下降トレンド → スコア-1減点",
    ),
    (
        -2,
        "🔴 短期SMAが長期より大幅に下 → 強い下降トレンド → スコア-2減点",
    ),
];
const SMA_SCORE_UNKNOWN: &str = "⚠️ SMAスコア情報なし";

const SMA_SCORE_TABLE_EN: &[(i32, &str)] = &[
    (
        2,
        "🟢 Short SMA well above long → Strong uptrend → score +2",
    ),
    (1, "🟢 Short SMA slightly above long → Uptrend → score +1"),
    (0, "➡️ SMAs near equal → no score change"),
    (
        -1,
        "🔴 Short SMA slightly below long → Downtrend → score -1",
    ),
    (
        -2,
        "🔴 Short SMA well below long → Strong downtrend → score -2",
    ),
];
const SMA_SCORE_UNKNOWN_EN: &str = "⚠️ SMA score unavailable";

const ADX_SCORE_TABLE: &[(i32, &str)] = &[
    (
        2,
        "🟢 ADXが非常に強い（50以上）→ 強いトレンド継続 → スコア+2加点",
    ),
    (
        1,
        "🟢 ADXがやや強い（30以上50未満）→ トレンド発生 → スコア+1加点",
    ),
    (0, "➡️ ADXが中立（20以上30未満）→ 様子見"),
    (
        -1,
        "🔴 ADXがやや弱い（10以上20未満）→ トレンド弱まる → スコア-1減点",
    ),
    (
        -2,
        "🔴 ADXが非常に弱い（10未満）→ トレンド消失 → スコア-2減点",
    ),
];
const ADX_SCORE_UNKNOWN: &str = "⚠️ ADXスコア情報なし";

const ADX_SCORE_TABLE_EN: &[(i32, &str)] = &[
    (
        2,
        "🟢 ADX very strong (≥50) → Strong trend continues → score +2",
    ),
    (
        1,
        "🟢 ADX moderately strong (30–50) → Trend forming → score +1",
    ),
    (0, "➡️ ADX neutral (20–30) → Watch and wait"),
    (-1, "🔴 ADX weakening (10–20) → Trend fading → score -1"),
    (-2, "🔴 ADX very weak (<10) → Trend absent → score -2"),
];
const ADX_SCORE_UNKNOWN_EN: &str = "⚠️ ADX score unavailable";

const ROC_SCORE_TABLE: &[(i32, &str)] = &[
    (2, "🟢 ROCが大幅上昇 → 強い上昇トレンド → スコア+2加点"),
    (1, "🟢 ROCが上昇傾向 → スコア+1加点"),
    (0, "➡️ ROCが安定圏（±3%）→ スコア変動なし"),
    (-1, "🔴 ROCがやや下降 → スコア-1減点"),
    (-2, "🔴 ROCが大幅下降 → 強い下降トレンド → スコア-2減点"),
];
const ROC_SCORE_UNKNOWN: &str = "⚠️ ROCスコア情報なし";

const ROC_SCORE_TABLE_EN: &[(i32, &str)] = &[
    (2, "🟢 ROC rising sharply → Strong uptrend → score +2"),
    (1, "🟢 ROC rising → score +1"),
    (0, "➡️ ROC stable (±3%) → no score change"),
    (-1, "🔴 ROC declining slightly → score -1"),
    (-2, "🔴 ROC falling sharply → Strong downtrend → score -2"),
];
const ROC_SCORE_UNKNOWN_EN: &str = "⚠️ ROC score unavailable";

const STOCHASTICS_SCORE_TABLE: &[(i32, &str)] = &[
    (
        2,
        "🟢 %Kが10%以下 → 強い売られすぎと判断 → 買いシグナル → スコア+2加点",
    ),
    (
        1,
        "🟢 %Kが20%以下 → 売られすぎと判断 → 買いシグナル → スコア+1加点",
    ),
    (
        0,
        "➡️ %Kが中立圏（20〜80%） → シグナルなし → スコア変動なし",
    ),
    (
        -1,
        "🔴 %Kが80%以上 → 買われすぎと判断 → 売りシグナル → スコア-1減点",
    ),
    (
        -2,
        "🔴 %Kが90%以上 → 強い買われすぎと判断 → 売りシグナル → スコア-2減点",
    ),
];
const STOCHASTICS_SCORE_UNKNOWN: &str = "⚠️ ストキャスティクススコア情報なし";

const STOCHASTICS_SCORE_TABLE_EN: &[(i32, &str)] = &[
    (2, "🟢 %K ≤10% → Strongly oversold → buy signal → score +2"),
    (1, "🟢 %K ≤20% → Oversold → buy signal → score +1"),
    (0, "➡️ %K neutral (20–80%) → no signal → no score change"),
    (-1, "🔴 %K ≥80% → Overbought → sell signal → score -1"),
    (
        -2,
        "🔴 %K ≥90% → Strongly overbought → sell signal → score -2",
    ),
];
const STOCHASTICS_SCORE_UNKNOWN_EN: &str = "⚠️ Stochastics score unavailable";

const VWAP_SCORE_TABLE: &[(i32, &str)] = &[
    (
        2,
        "🟢 VWAPが指標計算最終足終値より大幅に下 → 強い買いシグナル → スコア+2加点",
    ),
    (
        1,
        "🟢 VWAPが指標計算最終足終値よりやや下 → 買いシグナル → スコア+1加点",
    ),
    (0, "➡️ VWAPと同水準（乖離率±1.0%以内）→ スコア変動なし"),
    (
        -1,
        "🔴 VWAPが指標計算最終足終値よりやや上 → 売りシグナル → スコア-1減点",
    ),
    (
        -2,
        "🔴 VWAPが指標計算最終足終値より大幅に上 → 強い売りシグナル → スコア-2減点",
    ),
];
const VWAP_SCORE_UNKNOWN: &str = "⚠️ VWAPスコア情報なし";

const VWAP_SCORE_TABLE_EN: &[(i32, &str)] = &[
    (2, "🟢 VWAP well below close → Strong buy signal → score +2"),
    (1, "🟢 VWAP slightly below close → Buy signal → score +1"),
    (
        0,
        "➡️ VWAP near close (within ±1.0% deviation) → no score change",
    ),
    (-1, "🔴 VWAP slightly above close → Sell signal → score -1"),
    (
        -2,
        "🔴 VWAP well above close → Strong sell signal → score -2",
    ),
];
const VWAP_SCORE_UNKNOWN_EN: &str = "⚠️ VWAP score unavailable";

const ICHIMOKU_SCORE_TABLE: &[(i32, &str)] = &[
    (
        2,
        "🟢 転換線が基準線より大幅に上 → 強い買い圧力 → スコア+2加点",
    ),
    (1, "🟢 転換線が基準線よりやや上 → 買い優勢 → スコア+1加点"),
    (
        0,
        "➡️ 転換線と基準線が同値圏 → トレンドなし → スコア変動なし",
    ),
    (-1, "🔴 転換線が基準線よりやや下 → 売り優勢 → スコア-1減点"),
    (
        -2,
        "🔴 転換線が基準線より大幅に下 → 強い売り圧力 → スコア-2減点",
    ),
];
const ICHIMOKU_SCORE_UNKNOWN: &str = "⚠️ 一目均衡表スコア情報なし";

const ICHIMOKU_SCORE_TABLE_EN: &[(i32, &str)] = &[
    (
        2,
        "🟢 Tenkan well above kijun → Strong buying pressure → score +2",
    ),
    (1, "🟢 Tenkan slightly above kijun → Buy-leaning → score +1"),
    (
        0,
        "➡️ Tenkan and kijun near equal → no trend → no score change",
    ),
    (
        -1,
        "🔴 Tenkan slightly below kijun → Sell-leaning → score -1",
    ),
    (
        -2,
        "🔴 Tenkan well below kijun → Strong selling pressure → score -2",
    ),
];
const ICHIMOKU_SCORE_UNKNOWN_EN: &str = "⚠️ Ichimoku score unavailable";

fn score_table_lookup(
    table: &[(i32, &'static str)],
    score: Option<i32>,
    unknown: &'static str,
) -> &'static str {
    match score {
        Some(s) => table
            .iter()
            .find(|(v, _)| *v == s)
            .map(|(_, d)| *d)
            .unwrap_or(unknown),
        None => unknown,
    }
}

/// The sentence explaining one indicator's integer score, in the configured
/// language.
///
/// Each indicator owns a static table (`SMA_SCORE_TABLE` and friends) and one
/// shared lookup reads them all, so adding an indicator means adding a table
/// rather than another branch here. A score with no entry returns the "unknown"
/// wording instead of an empty string — the display never shows a blank where an
/// explanation belongs.
///
/// 1 指標の整数スコアを説明する文を、設定言語で返す。
///
/// 指標ごとに静的テーブル（`SMA_SCORE_TABLE` など）を持ち、共通のルックアップが
/// それらを読む。指標を足す作業は、ここに分岐を足すことではなくテーブルを足すことになる。
/// 該当のないスコアは空文字ではなく「不明」の文言を返す。説明が入るべき場所が
/// 空白のまま表示されることはない。
pub fn get_score_description(
    indicator: &ExtensionIndicator,
    score: Option<i32>,
    lang: &str,
) -> &'static str {
    let ja = lang == "ja";
    match indicator {
        ExtensionIndicator::Sma => {
            if ja {
                score_table_lookup(SMA_SCORE_TABLE, score, SMA_SCORE_UNKNOWN)
            } else {
                score_table_lookup(SMA_SCORE_TABLE_EN, score, SMA_SCORE_UNKNOWN_EN)
            }
        }
        ExtensionIndicator::Adx => {
            if ja {
                score_table_lookup(ADX_SCORE_TABLE, score, ADX_SCORE_UNKNOWN)
            } else {
                score_table_lookup(ADX_SCORE_TABLE_EN, score, ADX_SCORE_UNKNOWN_EN)
            }
        }
        ExtensionIndicator::Roc => {
            if ja {
                score_table_lookup(ROC_SCORE_TABLE, score, ROC_SCORE_UNKNOWN)
            } else {
                score_table_lookup(ROC_SCORE_TABLE_EN, score, ROC_SCORE_UNKNOWN_EN)
            }
        }
        ExtensionIndicator::Stochastics => {
            if ja {
                score_table_lookup(STOCHASTICS_SCORE_TABLE, score, STOCHASTICS_SCORE_UNKNOWN)
            } else {
                score_table_lookup(
                    STOCHASTICS_SCORE_TABLE_EN,
                    score,
                    STOCHASTICS_SCORE_UNKNOWN_EN,
                )
            }
        }
        ExtensionIndicator::Vwap => {
            if ja {
                score_table_lookup(VWAP_SCORE_TABLE, score, VWAP_SCORE_UNKNOWN)
            } else {
                score_table_lookup(VWAP_SCORE_TABLE_EN, score, VWAP_SCORE_UNKNOWN_EN)
            }
        }
        ExtensionIndicator::Ichimoku => {
            if ja {
                score_table_lookup(ICHIMOKU_SCORE_TABLE, score, ICHIMOKU_SCORE_UNKNOWN)
            } else {
                score_table_lookup(ICHIMOKU_SCORE_TABLE_EN, score, ICHIMOKU_SCORE_UNKNOWN_EN)
            }
        }
        _ => {
            if ja {
                "⚠️ スコア情報なし"
            } else {
                "⚠️ Score unavailable"
            }
        }
    }
}

/// Groups an integer with thousands separators, e.g. `1234500 → "1,234,500"`,
/// `-1235 → "-1,235"`. The single implementation of the comma-insertion logic
/// (SOT §4.2); each caller does its own `f64 → i64` conversion first (truncate for
/// volume, round for currency), so this is display-only and never touches SOT values.
pub(crate) fn group_thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let len = digits.len();
    let mut out = String::with_capacity(len + len / 3 + 1);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// Formats a volume value with thousands separators (e.g. 1234500 → "1,234,500").
pub fn format_volume_display(vol: f64) -> String {
    // Volume is non-negative; truncate (matching the previous `as u64`) then group.
    group_thousands(vol as i64)
}

const VOL_HIGH_THRESHOLD: f64 = 1.1;
const VOL_LOW_THRESHOLD: f64 = 0.9;

/// Returns a short volume context string for display and LLM prompts.
/// `ratio` is latest_volume / avg_volume; `price_diff` is the bar close price change.
pub fn volume_comment_str(ratio: f64, price_diff: f64, lang: &str) -> &'static str {
    if ratio == 0.0 {
        return match lang {
            "ja" => "出来高ゼロ（直近バー取引なし）",
            _ => "Zero volume (no trades in this bar)",
        };
    }
    let vol_high = ratio >= VOL_HIGH_THRESHOLD;
    let vol_low = ratio < VOL_LOW_THRESHOLD;
    let price_up = price_diff > 0.0;
    let price_down = price_diff < 0.0;
    match lang {
        "ja" => match (vol_high, vol_low, price_up, price_down) {
            (true, _, true, _) => "出来高増 + 価格上昇: 上昇の勢いを伴う可能性",
            (true, _, _, true) => "出来高増 + 価格下落: 売り圧力が強まっている可能性",
            (true, _, false, false) => "出来高増（価格変動なし）",
            (_, true, true, _) => "出来高減 + 価格上昇: 反発の持続性には確認が必要",
            (_, true, _, true) => "出来高減 + 価格下落: 商い薄の下落の可能性",
            (_, true, false, false) => "出来高減（価格変動なし）",
            _ => "出来高変化なし（直近平均比 ±10%以内）",
        },
        _ => match (vol_high, vol_low, price_up, price_down) {
            (true, _, true, _) => "High vol + up: possible upward momentum",
            (true, _, _, true) => "High vol + down: possible selling pressure",
            (true, _, false, false) => "High vol, price unchanged",
            (_, true, true, _) => "Low vol + up: sustainability uncertain",
            (_, true, _, true) => "Low vol + down: possible thin-volume decline",
            (_, true, false, false) => "Low vol, price unchanged",
            _ => "Volume near average (within ±10%)",
        },
    }
}

/// Splits one `xoksa.env` line into `(key, raw_value)` slices, applying the shared
/// rules (trim, skip blank/`#`, strip a leading `export`, split at the first `=`).
/// Returns `None` for a blank/comment line, one without `=`, or an empty key. The
/// value is returned UNPARSED so a caller can filter by key BEFORE running
/// `parse_env_value` — e.g. `load_env_map` drops Class A keys without ever
/// materializing their value (security-design §0.1). Single line-parser (SOT §4.2).
pub(crate) fn parse_env_line(raw: &str) -> Option<(&str, &str)> {
    let mut line = raw.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    if let Some(rest) = line.strip_prefix("export ") {
        line = rest.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
    }
    let idx = line.find('=')?;
    let key = line[..idx].trim();
    if key.is_empty() {
        return None;
    }
    Some((key, line[idx + 1..].trim()))
}

/// Reads a specific key's value directly from `xoksa.env`. API keys are intentionally
/// excluded from env_map, so they must be read here directly.
pub fn read_key_from_env_file(key_name: &str) -> Option<Zeroizing<String>> {
    let path = env_path();
    let (lines, _skipped) = sanitize_env_file_lines_lenient(&path).ok()?;
    for raw in lines {
        let Some((k, raw_val)) = parse_env_line(&raw) else {
            continue;
        };
        if k.eq_ignore_ascii_case(key_name) {
            let parsed = parse_env_value(raw_val);
            if !parsed.is_empty() {
                return Some(Zeroizing::new(parsed));
            }
        }
    }
    None
}

/// Resolves an API key: checks xoksa.env first, then falls back to the OS Keychain.
/// Returns `Ok(Some(key))` if found, `Ok(None)` if not set, `Err` on keyring access failure.
pub fn resolve_api_key(key_name: &str) -> anyhow::Result<Option<Zeroizing<String>>> {
    if let Some(key) = read_key_from_env_file(key_name) {
        return Ok(Some(key));
    }
    crate::keystore::get_key(key_name)
}

/// Return true if `key_name` is set to a non-empty value in `xoksa.env`.
/// Delegates to `read_key_from_env_file` (SOT §4.2 — one parser, not a second copy),
/// so `--doctor` and the runtime agree on whether a key is present, and the value
/// read for the check is zeroized rather than lingering in a plain `String`.
pub fn key_present_in_env_file(key_name: &str) -> bool {
    read_key_from_env_file(key_name).is_some()
}

/// The price a diff is measured from: the latest observed price when there is
/// one, else the confirmed close.
///
/// On a Japanese intraday timeframe the observed price is the live quote, so the
/// diff on screen moves with the market; where no observation exists the close
/// stands in, and the figure is still a confirmed value rather than a blank.
///
/// 差分の起点にする価格。最新取得価格があればそれ、無ければ確定した終値。
///
/// 日本株の分足では最新取得価格がリアルタイムの気配なので、画面の差分が市場に合わせて
/// 動く。観測値が無い場合は終値が代わりに立ち、数字は空白ではなく確定値のままになる。
pub(crate) fn display_price_for_diff(guard: &TechnicalDataGuard) -> f64 {
    guard
        .get_latest_observed_price()
        .unwrap_or_else(|| guard.get_close())
}

/// The `(difference, percent)` pair shown beside the price, measured from
/// [`display_price_for_diff`] against the previous close.
///
/// A previous close of zero yields a percent of `0.0` rather than an infinity —
/// a division that cannot be trusted produces no ratio, as elsewhere in the
/// engine.
///
/// 価格の横に出す `(差分, 変化率)`。[`display_price_for_diff`] を前日終値と比べて求める。
///
/// 前日終値が 0 の場合、変化率は無限大ではなく `0.0` を返す。信頼できない割り算は比を
/// 作らない——エンジンの他の箇所と同じ扱い。
pub(crate) fn displayed_price_diff(guard: &TechnicalDataGuard) -> (f64, f64) {
    let current = display_price_for_diff(guard);
    let previous = guard.get_previous_close();
    let diff = current - previous;
    let percent = if previous != 0.0 {
        diff / previous * 100.0
    } else {
        0.0
    };
    (diff, percent)
}

/// The four data-freshness header labels, localized. Single source of truth so
/// the on-screen display (render.rs) and the LLM context (llm.rs) always use the
/// exact same wording. Order: analysis time / latest data time / bar of the
/// latest price / indicator (confirmed) bar — paired with `format_analysis_time`,
/// `format_market_data_latest_time`, `format_latest_observed_price_bar`,
/// `format_indicator_latest_bar` respectively.
pub(crate) fn freshness_labels(lang: &str) -> [&'static str; 4] {
    match lang {
        "ja" => [
            "📅 分析時刻",
            "🕒 データの最新時刻",
            "🕯️ 最新価格が入る足",
            "📊 指標を計算した足",
        ],
        _ => [
            "📅 Analysis time",
            "🕒 Latest data time",
            "🕯️ Bar of latest price",
            "📊 Indicator bar",
        ],
    }
}

/// When the analysis ran, with its timezone, in the configured language.
///
/// A missing time still renders the timezone — "unknown time JST" says more than
/// an empty field, and keeps the four freshness lines the same shape.
///
/// 分析を実行した時刻と、そのタイムゾーン。設定言語で返す。
///
/// 時刻が無い場合もタイムゾーンは出す。「時刻不明 JST」のほうが空欄より情報があり、
/// 鮮度を示す 4 行の形も揃う。
pub(crate) fn format_analysis_time(guard: &TechnicalDataGuard, lang: &str) -> String {
    let unknown = if lang == "ja" {
        "時刻不明"
    } else {
        "unknown time"
    };
    guard
        .get_analyzed_at()
        .map(|value| format!("{} {}", value, guard.get_timezone()))
        .unwrap_or_else(|| format!("{unknown} {}", guard.get_timezone()))
}

/// A time with its timezone appended, in the configured language; the shared
/// shape behind the freshness lines.
///
/// As with [`format_analysis_time`], an absent time keeps the timezone so every
/// line reads the same way whether or not the value arrived.
///
/// 時刻にタイムゾーンを付けた文字列を、設定言語で返す。鮮度の各行が共有する形。
///
/// [`format_analysis_time`] と同じく、時刻が無くてもタイムゾーンは残す。値が来たかどうかに
/// 関わらず、どの行も同じ読み方になる。
pub(crate) fn format_time_with_timezone(value: Option<&str>, timezone: &str, lang: &str) -> String {
    let unknown = if lang == "ja" {
        "時刻不明"
    } else {
        "unknown time"
    };
    value
        .map(|time| format!("{time} {timezone}"))
        .unwrap_or_else(|| format!("{unknown} {timezone}"))
}

/// The time the market data itself is from — the latest bar's time on an
/// intraday timeframe, the latest date on a daily one.
///
/// Read straight from the guard rather than recomputed, so display and prompt
/// quote the same instant.
///
/// 市場データそのものの時刻。分足なら最新足の時刻、日足なら最新の日付。
///
/// 計算し直さず guard からそのまま読むので、画面とプロンプトが同じ時点を引用する。
pub(crate) fn market_data_latest_time(guard: &TechnicalDataGuard) -> Option<&str> {
    guard.get_market_data_latest_time()
}

/// Floors a `YYYY-MM-DD HH:MM` string down to the bar interval (5, 15, 30 …
/// minutes), so a time lands on the bucket that contains it.
///
/// Returns `None` when the string does not parse, rather than guessing a time —
/// an unparseable input must not become a confident-looking bar label.
///
/// `YYYY-MM-DD HH:MM` の文字列を足の刻み（5・15・30 分など）に切り捨て、その時刻が
/// 属するバケットに落とす。
///
/// パースできない文字列には時刻を推測せず `None` を返す。読めない入力が、いかにも
/// 確からしい足のラベルになってはならないため。
pub(crate) fn floor_time_to_interval(value: &str, minutes: u32) -> Option<String> {
    let parsed = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M").ok()?;
    let floor_minute = parsed.minute() - (parsed.minute() % minutes);
    let floored = parsed
        .with_minute(floor_minute)?
        .with_second(0)?
        .with_nanosecond(0)?;
    Some(floored.format("%Y-%m-%d %H:%M").to_string())
}

/// A bar's time labelled for the analysis timeframe — "5-minute bar", "daily
/// bar" and so on — in the configured language.
///
/// The label comes from [`AnalysisMode::bar_label`], the one place that names a
/// timeframe, so screen and prompt cannot call the same bar by two names. Where
/// no time exists the date stands in, which is what a daily bar has anyway.
///
/// 足の時刻に、分析足に応じたラベル（「5分足」「日足」など）を付けて返す。設定言語。
///
/// ラベルは [`AnalysisMode::bar_label`]——足を名付ける唯一の場所——から取る。同じ足を
/// 画面とプロンプトが別の名前で呼ぶことはない。時刻が無い場合は日付が代わりに立つ。
/// 日足はもともと日付しか持たない。
pub(crate) fn format_bar_time_for_mode(
    config: &Config,
    value: Option<&str>,
    fallback_date: &str,
) -> String {
    let label = config.analysis_mode.bar_label(&config.lang);
    let unknown = if config.lang == "ja" {
        "時刻不明"
    } else {
        "unknown time"
    };
    let bar_time = if let Some(minutes) = config.analysis_mode.intraday_minutes() {
        match value.and_then(|time| floor_time_to_interval(time, minutes)) {
            Some(floored) => floored,
            None => return format!("{unknown} ({label})"),
        }
    } else {
        value
            .and_then(|t| t.get(..10))
            .unwrap_or(fallback_date)
            .to_string()
    };
    if config.lang == "ja" {
        format!("{bar_time} （{label}）")
    } else {
        format!("{bar_time} ({label})")
    }
}

/// The freshness line naming the bar the **latest price** sits in.
///
/// One of the four lines built here rather than at each call site, so the screen
/// and the LLM context state the same bar in the same words.
///
/// **最新価格**がどの足に入っているかを示す鮮度の行。
///
/// 呼び出し側ごとではなくここで組む 4 行のうちの 1 つ。画面と LLM 文脈が、同じ足を
/// 同じ言い回しで述べるようにするため。
pub(crate) fn format_latest_observed_price_bar(
    config: &Config,
    guard: &TechnicalDataGuard,
) -> String {
    let label = config.analysis_mode.bar_label(&config.lang);
    let unknown = if config.lang == "ja" {
        "時刻不明"
    } else {
        "unknown time"
    };
    let no_data = if config.lang == "ja" {
        "取得なし"
    } else {
        "not available"
    };
    if guard.get_latest_observed_price().is_none() {
        no_data.to_string()
    } else if let Some(minutes) = config.analysis_mode.intraday_minutes() {
        match market_data_latest_time(guard).and_then(|t| floor_time_to_interval(t, minutes)) {
            Some(bar) => {
                if config.lang == "ja" {
                    format!("{bar} （{label}）")
                } else {
                    format!("{bar} ({label})")
                }
            }
            None => format!("{unknown} ({label})"),
        }
    } else if market_data_latest_time(guard).is_none() {
        format!("{unknown} ({label})")
    } else {
        format_bar_time_for_mode(config, market_data_latest_time(guard), guard.get_date())
    }
}

/// The freshness line naming the bar the **indicators** were computed on.
///
/// Distinct from [`format_latest_observed_price_bar`] on purpose: on a Japanese
/// intraday timeframe the latest price sits in a bar that is still forming,
/// while the indicators are computed on the last confirmed one. Saying both
/// keeps that gap visible instead of implying one bar.
///
/// **指標**を計算した足を示す鮮度の行。
///
/// [`format_latest_observed_price_bar`] と分けてあるのは意図的である。日本株の分足では
/// 最新価格は形成中の足に入り、指標は確定した最後の足で計算される。両方を述べることで、
/// その差を 1 本の足であるかのように見せず、見えるままにする。
pub(crate) fn format_indicator_latest_bar(config: &Config, guard: &TechnicalDataGuard) -> String {
    format_bar_time_for_mode(config, guard.get_bar_time(), guard.get_date())
}

/// The freshness line naming when the market data itself is from, in the
/// configured language.
///
/// 市場データ自体がいつのものかを示す鮮度の行。設定言語で返す。
pub(crate) fn format_market_data_latest_time(guard: &TechnicalDataGuard, lang: &str) -> String {
    format_time_with_timezone(market_data_latest_time(guard), guard.get_timezone(), lang)
}

/// Serializes every change this process makes to `xoksa.env`.
///
/// Each writer rewrites the whole file, so two running at once each build on the
/// same starting text and the later replace erases the other's change — with both
/// callers told it succeeded. The alert rules and the dashboard's language
/// selector write the same file, so the lock has to live here, beside the path,
/// rather than inside either feature: a lock one writer does not take is not a
/// lock.
///
/// Held from the read to the replace. `server::monitor` also holds it across its
/// store mutation, so a rule number cannot be handed out against a file the write
/// has not landed in; lock order there is ENV then STORE, never the other way.
static ENV_WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) fn env_write_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_WRITE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Replace `target` with `f(current text)` as one serialized, atomic step.
pub(crate) fn rewrite_env_file(
    target: &std::path::Path,
    f: impl FnOnce(&str) -> String,
) -> std::io::Result<()> {
    let _g = env_write_lock();
    rewrite_env_file_locked(target, f)
}

/// As [`rewrite_env_file`], for a caller that already holds [`env_write_lock`].
///
/// The new content is written to a temporary file in the same directory and
/// renamed over the original, so an interrupted or failing write cannot leave the
/// file truncated — rewriting in place would put every unrelated setting at risk
/// for the sake of one change.
///
/// The temporary file is created restricted (0600 on Unix) and then given the
/// original's permissions, so a replacement neither widens access to a config the
/// user tightened nor leaves a fresh one world-readable at the moment it is
/// created. A missing file reads as empty.
pub(crate) fn rewrite_env_file_locked(
    target: &std::path::Path,
    f: impl FnOnce(&str) -> String,
) -> std::io::Result<()> {
    let existing = match std::fs::read_to_string(target) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let out = f(existing.as_deref().unwrap_or(""));
    if let Some(dir) = target.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    let tmp = target.with_extension(format!("tmp{}", std::process::id()));
    // Remove a leftover from an interrupted run: `create_new` would refuse it.
    let _ = std::fs::remove_file(&tmp);
    {
        use std::io::Write;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&tmp)?;
        if let Err(e) = file
            .write_all(out.as_bytes())
            .and_then(|()| file.sync_all())
        {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    }
    // Carry the original's permissions over, so the replacement is no more open
    // than what it replaces. A new file keeps the 0600 it was created with.
    if target.exists() {
        if let Ok(meta) = std::fs::metadata(target) {
            let _ = std::fs::set_permissions(&tmp, meta.permissions());
        }
    }
    match std::fs::rename(&tmp, target) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Set (or add) a single `KEY=value` line in `xoksa.env`, leaving every other
/// line untouched.
///
/// The config file is the single source of truth for settings, so a UI that lets
/// the user change one — the dashboard's language selector — writes it here
/// rather than keeping a private copy of its own. Values are constrained by the
/// caller (the language endpoint accepts only `ja` / `en`); this helper refuses
/// anything with a control character or `=`/newline in the key.
///
/// No-op under `--private`: a no-trace session writes nothing to disk.
pub fn set_env_value(key: &str, value: &str) -> std::io::Result<()> {
    if crate::private::is_private() {
        return Ok(());
    }
    if key.is_empty()
        || key.contains(['=', '\n', '\r'])
        || value.contains(['\n', '\r'])
        || key.chars().any(char::is_control)
        || value.chars().any(char::is_control)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid env key/value",
        ));
    }
    // Through the shared writer: the alert rules change the same file, and a
    // language change that raced one used to be silently dropped.
    rewrite_env_file(&env_path(), |text| {
        let prefix = format!("{key}=");
        let mut replaced = false;
        let mut out: Vec<String> = text
            .lines()
            .map(|line| {
                if line.trim_start().starts_with(&prefix) {
                    replaced = true;
                    format!("{key}={value}")
                } else {
                    line.to_string()
                }
            })
            .collect();
        if !replaced {
            out.push(format!("{key}={value}"));
        }
        format!("{}\n", out.join("\n"))
    })
}

/// The label stating whether a negative MACD is allowed to score, in the
/// configured language.
///
/// Shared by `render.rs` and `llm.rs` so the screen and the prompt describe the
/// same setting with the same words — a policy the user reads one way on screen
/// and another in the AI's answer would look like two settings.
///
/// マイナスの MACD を採点対象にするかどうかを示すラベル。設定言語で返す。
///
/// `render.rs` と `llm.rs` が共有する。画面とプロンプトが同じ設定を同じ言葉で述べる
/// ようにするため。画面と AI の回答で読み方が違えば、設定が 2 つあるように見えてしまう。
pub(crate) fn macd_minus_policy_label(
    macd_minus_ok: bool,
    applied: bool,
    lang: &str,
) -> &'static str {
    if lang == "ja" {
        match (macd_minus_ok, applied) {
            (true, true) => "※『MACDマイナス許容』設定: 有効（今回「適用対象」）",
            (true, false) => "※『MACDマイナス許容』設定: 有効（今回「未適用」）",
            (false, _) => "※『MACDマイナス許容』設定: 無効",
        }
    } else {
        match (macd_minus_ok, applied) {
            (true, true) => "* MACD-minus-allow setting: enabled (applied this run)",
            (true, false) => "* MACD-minus-allow setting: enabled (not applied this run)",
            (false, _) => "* MACD-minus-allow setting: disabled",
        }
    }
}

#[cfg(test)]
mod env_path_tests {
    use super::pick_env_path;
    use std::path::{Path, PathBuf};

    #[test]
    fn explicit_env_file_wins_over_canonical() {
        let got = pick_env_path(
            Some(Path::new("/tmp/custom.env")),
            Some(PathBuf::from("/cfg/xoksa/xoksa.env")),
        );
        assert_eq!(got, PathBuf::from("/tmp/custom.env"));
    }

    #[test]
    fn canonical_used_when_no_override() {
        let got = pick_env_path(None, Some(PathBuf::from("/cfg/xoksa/xoksa.env")));
        assert_eq!(got, PathBuf::from("/cfg/xoksa/xoksa.env"));
    }

    #[test]
    fn no_implicit_cwd_fallback_only_last_resort_literal() {
        // No override and no OS config dir → the degenerate last-resort literal,
        // NOT an implicit ./xoksa.env candidate selected by cwd contents.
        let got = pick_env_path(None, None);
        assert_eq!(got, PathBuf::from("xoksa.env"));
    }

    #[test]
    fn env_line_issue_flags_control_bom_and_overlong_lines() {
        use super::env_line_issue;
        // A normal Ollama assignment is accepted.
        assert!(env_line_issue(2, "OLLAMA_1_MODEL=gemma4:e4b").is_none());
        // A pasted BOM / zero-width char on a non-first line is rejected (this is the
        // single bad line that used to wipe the whole config).
        assert!(env_line_issue(2, "OLLAMA_1_MODEL=gemma4:e4b\u{FEFF}").is_some());
        // Control chars and >500-byte lines are rejected too (multibyte counts bytes).
        assert!(env_line_issue(2, "OLLAMA_1_ALIAS=a\u{7}b").is_some());
        assert!(env_line_issue(2, &format!("K={}", "あ".repeat(200))).is_some());
    }
}
