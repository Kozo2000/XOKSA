use anyhow::Result;
use clap::{CommandFactory, FromArgMatches};
use csv::ReaderBuilder;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;

use crate::config::{self as libcfg, Args, Config};

pub type BuildCfgResult = Result<(Config, Option<String>, HashMap<String, String>, Vec<String>)>;

pub fn initialize_environment_and_config() -> BuildCfgResult {
    // Parse args first so --init can exit before any xoksa.env file I/O.
    let matches = Args::command().get_matches();
    let cli_sources = libcfg::CliValueSources::from_arg_matches(&matches);
    let mut args = Args::from_arg_matches(&matches)?;

    // Honor --env-file for the flag CLI (idempotent — main() already set it before
    // subcommand dispatch; this also keeps the option a live, read config field).
    crate::utils::init_env_path(args.env_file.as_deref());

    // --init creates xoksa.env from scratch — skip loading it entirely.
    if args.init {
        let lang_override = cli_sources.lang.then_some(args.lang.as_str());
        let completed = crate::setup::run_init(lang_override);
        std::process::exit(if completed { 0 } else { 130 });
    }

    // --update-key updates a stored API key in the OS Keychain.
    if args.update_key {
        let completed = crate::setup::run_update_key();
        std::process::exit(if completed { 0 } else { 130 });
    }

    // --check-keys reports stored key presence without displaying values.
    if args.check_keys {
        crate::setup::run_check_keys();
        std::process::exit(0);
    }

    let env_map = load_env_map();

    if args.show_log_header {
        let config = libcfg::build_config_with_value_sources(&args, &env_map, cli_sources);
        crate::output::generate_csv_header(&config);
        std::process::exit(0);
    }

    if args.doctor {
        crate::setup::run_doctor(&env_map);
        std::process::exit(0);
    }

    if args.ticker.is_none() && !args.chat {
        eprintln!("❌ --ticker is required");
        std::process::exit(1);
    }

    // Apply CHAT_DEFAULT_TICKER when --chat is used without --ticker
    let mut chat_default_extra_tickers: Vec<String> = Vec::new();
    if args.ticker.is_none() && args.chat {
        if let Some(raw_default) = env_map.get("CHAT_DEFAULT_TICKER") {
            let mut valid: Vec<String> = Vec::new();
            for raw in raw_default.split(',') {
                let raw = raw.trim();
                if raw.is_empty() {
                    continue;
                }
                let normalized = normalize_ticker_input(raw);
                match sanitize_ticker(&normalized) {
                    Ok(sanitized) => valid.push(normalize_ticker(&sanitized)),
                    Err(_) => eprintln!("⚠️ CHAT_DEFAULT_TICKER: '{}' is invalid (skipped)", raw),
                }
            }
            if valid.len() > 5 {
                eprintln!(
                    "⚠️ CHAT_DEFAULT_TICKER: {} tickers specified; maximum is 5. Using first 5.",
                    valid.len()
                );
                valid.truncate(5);
            }
            // All tickers go into chat_default_extra_tickers; args.ticker stays None so the
            // ticker-less chat path is taken and all tickers load via the same mechanism.
            chat_default_extra_tickers = valid;
        }
    }

    // Correct order: 1. nickname expansion (全米→VTI) → 2. validation → 3. normalization
    if args.ticker.is_some() {
        let raw_ticker = args.ticker.as_ref().unwrap().clone();
        args.ticker = Some(normalize_ticker_input(&raw_ticker));
        args.ticker = Some(
            sanitize_ticker(args.ticker.as_deref().unwrap_or("")).unwrap_or_else(|err| {
                eprintln!("{err}");
                std::process::exit(1);
            }),
        );
        args.ticker = Some(normalize_ticker(args.ticker.as_deref().unwrap_or("")));
    }

    if let Some(q) = &args.custom_news_query {
        args.custom_news_query = Some(sanitize_news_query(q).unwrap_or_else(|err| {
            eprintln!("{err}");
            std::process::exit(1);
        }));
    }
    if let Some(n) = &args.extra_note {
        args.extra_note = Some(sanitize_llm_note(n).unwrap_or_else(|err| {
            eprintln!("{err}");
            std::process::exit(1);
        }));
    }

    let mut config = libcfg::build_config_with_value_sources(&args, &env_map, cli_sources);

    // CLI path is already sanitized above; apply the same rules to extra_note from env.
    if args.extra_note.is_none() {
        if let Some(n) = config.extra_note.clone() {
            config.extra_note = Some(sanitize_llm_note(&n).unwrap_or_else(|err| {
                eprintln!("{err}");
                std::process::exit(1);
            }));
        }
    }

    if config.debug_args {
        eprintln!("Config= {}", config_debug_string(&config));
    }

    let ticker = args.ticker.as_ref().map(|_| config.ticker.clone());

    let ticker_name_map = match &config.alias_csv {
        Some(csv_path) => load_alias_csv(csv_path)?,
        None => HashMap::new(),
    };

    Ok((config, ticker, ticker_name_map, chat_default_extra_tickers))
}

/// Load `xoksa.env` into an `env_map` of general settings. API keys are
/// deliberately excluded (resolved on-demand via the keychain elsewhere). A
/// missing file is not an error. Shared by the CLI init path and the Web UI
/// server so the parsing/exclusion rules live in one place.
pub fn load_env_map() -> HashMap<String, String> {
    let env_path = crate::utils::env_path();
    let mut env_map: HashMap<String, String> = HashMap::new();
    match crate::utils::sanitize_env_file_lines_lenient(&env_path) {
        Ok((lines, skipped)) => {
            if env_path.exists() {
                for note in &skipped {
                    eprintln!(
                        "⚠️ xoksa.env: skipped malformed {} ({})",
                        env_path.display(),
                        note
                    );
                }
            }
            let content = lines.join("\n");
            for raw in content.lines() {
                let Some((key, raw_val)) = crate::utils::parse_env_line(raw) else {
                    continue;
                };
                // Class A credentials must never enter env_map (Class B/C). The API
                // keys live in the OS keychain; NOTIFY_<n>_SECRET is likewise
                // keychain-only by design — but if one is mistakenly placed in
                // xoksa.env, drop it here (defense-in-depth) so a secret can never sit
                // in the config map (security-design §0.1). The value is parsed only
                // AFTER this filter, so a Class A value is never materialized.
                let key_upper = key.to_ascii_uppercase();
                let is_notify_secret =
                    key_upper.starts_with("NOTIFY_") && key_upper.ends_with("_SECRET");
                if key.eq_ignore_ascii_case("OPENAI_API_KEY")
                    || key.eq_ignore_ascii_case("BRAVE_API_KEY")
                    || key.eq_ignore_ascii_case("GEMINI_API_KEY")
                    || key.eq_ignore_ascii_case("CLAUDE_API_KEY")
                    || key.eq_ignore_ascii_case("JQUANTS_API_KEY")
                    || is_notify_secret
                {
                    continue;
                }
                env_map.insert(key.to_string(), crate::utils::parse_env_value(raw_val));
            }
        }
        Err(e) => {
            // Only warn when the file exists but could not be read — a missing file is expected
            // on first run and is already reported by --doctor.
            if env_path.exists() {
                eprintln!("⚠️ xoksa.env load/sanitize failed (ignored): {}", e);
            }
        }
    }
    env_map
}

fn config_debug_string(cfg: &Config) -> String {
    let mut s = format!("{:?}", cfg);
    if let Some(ref agent) = cfg.sec_user_agent {
        if !agent.is_empty() {
            s = s.replace(agent.as_str(), "***");
        }
    }
    if let Some(ref proxy) = cfg.https_proxy {
        if !proxy.is_empty() {
            s = s.replace(proxy.as_str(), "***");
        }
    }
    s
}

/// Expands the one nickname that names the instrument it resolves to. The input is
/// trimmed and upper-cased only to match against the table; anything that does not
/// match is returned exactly as typed, and [`sanitize_ticker`] is what trims and
/// upper-cases what is finally used.
///
/// **An index name is never silently answered with a fund.** Up to 2.9.8 this
/// table mapped `S&P500` → `SPY`, `NASDAQ100` → `QQQ`, `DOW` → `DIA`, `日経平均` →
/// `1321.T` and `TOPIX` → `1306.T`: the user asked for an index and every
/// indicator, score and LLM comment was computed on a tracking fund instead, with
/// nothing saying so. Worse, `FANG+` resolved to `FNGU` — a **3× leveraged ETN**,
/// measured at 3.1× QQQ's daily move — so the readings were of an instrument whose
/// swings are tripled. `オールカントリー` pointed at the iShares ACWI ETF although
/// it is the retail nickname of a different product. Those entries are gone: an
/// index name xoksa cannot serve now fails as an unknown ticker rather than being
/// answered with something else.
///
/// 行き先そのものを指す通称だけを展開する。入力の trim・大文字化は表との照合のために
/// 行うだけで、該当しなければ打たれたまま返す。最終的に使う値の trim・大文字化は
/// [`sanitize_ticker`] が行う。
///
/// **指数名に黙ってファンドを返すことはしない。** 2.9.8 まではこの表が `S&P500` →
/// `SPY`、`NASDAQ100` → `QQQ`、`DOW` → `DIA`、`日経平均` → `1321.T`、`TOPIX` →
/// `1306.T` と差し替えており、利用者は指数を求めたのに、全指標・全スコア・LLM の解説が
/// 連動ファンドに対して計算されていた——しかもその旨はどこにも出ない。さらに `FANG+` は
/// `FNGU`（**3 倍レバレッジ ETN**。実測で QQQ の 3.1 倍の値動き）に解決されており、値動きが
/// 3 倍に増幅された別物の数値を読んでいた。`オールカントリー` は別商品の通称なのに iShares
/// ACWI ETF を指していた。これらは削除した。xoksa が扱えない指数名は、別のもので答える
/// のではなく未知のティッカーとして失敗する。
pub fn normalize_ticker_input(raw: &str) -> String {
    match raw.trim().to_uppercase().as_str() {
        // VTI *is* the total-market ETF these nicknames mean, so this expansion
        // names the instrument rather than substituting for a different one.
        "全米" | "トータルマーケット" => "VTI".to_string(),
        _ => raw.to_string(),
    }
}

pub fn sanitize_ticker(t: &str) -> Result<String, &'static str> {
    let cleaned = t.trim().to_uppercase();
    if !cleaned
        .chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
    {
        crate::logging::warn("XK-TICKER-INVALID", &format!("invalid ticker format: {t}"));
        return Err("❌ Ticker may only contain alphanumeric characters and . -");
    }
    Ok(cleaned)
}

pub fn sanitize_news_query(q: &str) -> Result<String, &'static str> {
    if q.len() > 200 {
        return Err("❌ News query must be 200 characters or fewer");
    }
    if q.contains([';', '|', '`']) {
        return Err("❌ News query contains disallowed characters");
    }
    Ok(q.trim().to_string())
}

pub fn sanitize_llm_note(note: &str) -> Result<String, &'static str> {
    if note.contains([';', '|', '`']) {
        return Err("❌ Input contains disallowed characters (; | `)");
    }
    let cleaned = note
        .trim()
        .replace('\n', " ")
        .replace(|c: char| c.is_whitespace(), " ");
    if cleaned.len() > 2000 {
        return Err("❌ Input too long (max 2000 characters)");
    }
    Ok(cleaned)
}

pub fn normalize_ticker(raw: &str) -> String {
    let up = raw.trim().to_ascii_uppercase();
    // `up` is already uppercase so a `.t` branch is unnecessary (removed as dead code)
    match (
        up.ends_with(".T"),
        up.len() == 4 && up.chars().all(|c| c.is_ascii_digit()),
    ) {
        (true, _) => up,
        (false, true) => format!("{up}.T"),
        _ => up,
    }
}

pub fn jp_code_from_ticker(t: &str) -> Option<String> {
    let up = t.trim().to_ascii_uppercase();
    if let Some(code) = up.strip_suffix(".T") {
        return (code.len() == 4 && code.chars().all(|c| c.is_ascii_digit()))
            .then(|| code.to_string());
    }
    (up.len() == 4 && up.chars().all(|c| c.is_ascii_digit())).then_some(up)
}

/// Load the optional ticker→company-name map. Accepts the JPX "Listed Securities
/// List" spreadsheet (`data_j.xls`/`.xlsx`) directly — so users need not open Excel
/// and convert to CSV — as well as a plain CSV. Column 2 = security code, column 3
/// = company name (the JPX layout); the header row is skipped.
pub fn load_alias_csv(path: &str) -> Result<HashMap<String, String>> {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(ext.as_str(), "xls" | "xlsx" | "xlsm" | "xlsb" | "ods") {
        return load_alias_spreadsheet(path);
    }
    let lines = crate::utils::sanitize_ascii_file_lines(Path::new(path))?;
    let content = lines.join("\n");
    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(Cursor::new(content));

    let mut map = HashMap::new();
    for result in rdr.records() {
        let record = result?;
        let code = record
            .get(1)
            .ok_or_else(|| anyhow::anyhow!("❌ CSV column 2 (security code) not found"))?
            .trim();
        let name = record
            .get(2)
            .ok_or_else(|| anyhow::anyhow!("❌ CSV column 3 (company name) not found"))?
            .trim();
        if !code.is_empty() && !name.is_empty() {
            map.insert(code.to_string(), name.to_string());
        }
    }

    Ok(map)
}

/// Read the ticker→company-name map from a spreadsheet (`.xls`/`.xlsx`/…) via
/// calamine, so the raw JPX `data_j.xls` can be used without an Excel conversion.
/// Same layout as the CSV path: column 2 = code, column 3 = name, header skipped.
/// A malformed file returns `Err` (calamine is memory-safe — it never crashes).
fn load_alias_spreadsheet(path: &str) -> Result<HashMap<String, String>> {
    use calamine::{open_workbook_auto, Reader};
    let mut wb = open_workbook_auto(path)
        .map_err(|e| anyhow::anyhow!("❌ Spreadsheet read failed: {path} ({e})"))?;
    let range = wb
        .worksheet_range_at(0)
        .ok_or_else(|| anyhow::anyhow!("❌ Spreadsheet has no sheet: {path}"))?
        .map_err(|e| anyhow::anyhow!("❌ Spreadsheet sheet read failed: {path} ({e})"))?;
    let mut map = HashMap::new();
    for row in range.rows().skip(1) {
        let code = row.get(1).map(cell_to_string).unwrap_or_default();
        let name = row.get(2).map(cell_to_string).unwrap_or_default();
        let (code, name) = (code.trim(), name.trim());
        if !code.is_empty() && !name.is_empty() {
            map.insert(code.to_string(), name.to_string());
        }
    }
    Ok(map)
}

/// A spreadsheet cell as a string. Codes may be stored as text or as a number
/// (`7203` / `7203.0`) — render a whole float without a decimal so the key matches
/// the ticker.
fn cell_to_string(cell: &calamine::Data) -> String {
    match cell {
        calamine::Data::String(s) => s.clone(),
        calamine::Data::Int(i) => i.to_string(),
        calamine::Data::Float(f) if f.fract() == 0.0 => (*f as i64).to_string(),
        calamine::Data::Float(f) => f.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_ticker, normalize_ticker_input, sanitize_llm_note, sanitize_news_query,
        sanitize_ticker,
    };

    // ── sanitize_ticker ───────────────────────────────────────────
    #[test]
    fn sanitize_ticker_accepts_alpha() {
        match sanitize_ticker("aapl") {
            Ok(v) => assert_eq!(v, "AAPL"),
            Err(e) => panic!("expected Ok, got Err: {e}"),
        }
    }
    #[test]
    fn sanitize_ticker_accepts_jp_code() {
        match sanitize_ticker("7203.T") {
            Ok(v) => assert_eq!(v, "7203.T"),
            Err(e) => panic!("expected Ok, got Err: {e}"),
        }
    }
    #[test]
    fn sanitize_ticker_rejects_semicolon() {
        assert!(sanitize_ticker("AAPL;DROP").is_err());
    }
    #[test]
    fn sanitize_ticker_rejects_pipe() {
        assert!(sanitize_ticker("AAPL|echo").is_err());
    }
    #[test]
    fn sanitize_ticker_rejects_backtick() {
        assert!(sanitize_ticker("AAPL`cmd`").is_err());
    }

    // ── normalize_ticker_input ───────────────────────────────────
    /// An index name must not come back as a fund. These three used to: `日経平均`
    /// answered `1321.T`, `S&P500` answered `SPY` and `ナスダック100` answered `QQQ`,
    /// so every reading was of a tracking fund while the user had asked for the
    /// index. They are returned unchanged now, and fail later as unknown tickers.
    #[test]
    fn an_index_name_is_never_answered_with_a_fund() {
        for name in ["日経平均", "NIKKEI225", "S&P500", "SP500", "SNP500"] {
            assert_eq!(normalize_ticker_input(name), name);
        }
        for name in [
            "ナスダック100",
            "NASDAQ100",
            "DOW",
            "DJIA",
            "ダウ平均",
            "TOPIX",
        ] {
            assert_eq!(normalize_ticker_input(name), name);
        }
    }

    /// `FANG+` resolved to `FNGU`, a 3x leveraged ETN — measured at 3.1x QQQ's daily
    /// move — so the readings were of an instrument whose swings are tripled.
    #[test]
    fn a_leveraged_product_is_never_substituted_for_an_index_name() {
        for name in ["FANG+", "FANGプラス"] {
            assert_eq!(normalize_ticker_input(name), name);
        }
    }

    /// `オールカントリー` is the retail nickname of a different product, so it no
    /// longer answers with the iShares ACWI ETF.
    #[test]
    fn a_nickname_of_another_product_is_not_expanded() {
        for name in ["オールカントリー", "全世界"] {
            assert_eq!(normalize_ticker_input(name), name);
        }
    }

    /// The one expansion kept: VTI *is* the total-market ETF these nicknames mean.
    #[test]
    fn normalize_ticker_input_expands_only_a_naming_nickname() {
        assert_eq!(normalize_ticker_input("全米"), "VTI");
        assert_eq!(normalize_ticker_input("トータルマーケット"), "VTI");
        assert_eq!(normalize_ticker_input("vti"), "vti");
    }

    // ── normalize_ticker ─────────────────────────────────────────
    #[test]
    fn normalize_ticker_appends_t_to_4digit() {
        assert_eq!(normalize_ticker("7203"), "7203.T");
    }
    #[test]
    fn normalize_ticker_keeps_existing_t_suffix() {
        assert_eq!(normalize_ticker("7203.T"), "7203.T");
    }

    // ── sanitize_news_query ─────────────────────────────────────
    #[test]
    fn sanitize_news_query_accepts_normal() {
        assert!(sanitize_news_query("NVDA earnings Q1").is_ok());
    }
    #[test]
    fn sanitize_news_query_rejects_semicolon() {
        assert!(sanitize_news_query("AAPL;DROP").is_err());
    }
    #[test]
    fn sanitize_news_query_rejects_pipe() {
        assert!(sanitize_news_query("AAPL|echo").is_err());
    }
    #[test]
    fn sanitize_news_query_rejects_too_long() {
        let long = "a".repeat(201);
        assert!(sanitize_news_query(&long).is_err());
    }

    // ── sanitize_llm_note ───────────────────────────────────────
    #[test]
    fn sanitize_llm_note_rejects_backtick() {
        assert!(sanitize_llm_note("use `rm -rf`").is_err());
    }
    #[test]
    fn sanitize_llm_note_rejects_pipe() {
        assert!(sanitize_llm_note("echo|bash").is_err());
    }
}
