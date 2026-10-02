//! --init / --doctor: interactive setup wizard and configuration diagnostics

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::time::Duration;
use zeroize::Zeroizing;

// ── public entry points ────────────────────────────────────────────────────

/// Interactive wizard that generates a new xoksa.env file.
/// Asks about each feature (LLM, news, fundamental data, proxy) independently.
/// Backs up any existing xoksa.env immediately before writing the new file.
pub fn run_init(lang_override: Option<&str>) -> bool {
    // Returns false if the user pressed Ctrl-C during the wizard.
    // All local Zeroizing vars are dropped before the caller sees false.
    macro_rules! c {
        ($e:expr) => {
            match $e {
                Some(v) => v,
                None => return false,
            }
        };
    }
    use std::io::IsTerminal;
    // Ensure raw mode is off in case a previous session exited without restoring it.
    let _ = crossterm::terminal::disable_raw_mode();
    // Peek at lang_override early so error messages before lang_val/t!() can be localised.
    let ja = lang_override == Some("ja");
    if !std::io::stdin().is_terminal() {
        if ja {
            eprintln!("❌ --init にはインタラクティブなターミナル（TTY）が必要です。");
            eprintln!(
                "   パイプや CI ではなく、ターミナルから直接このコマンドを実行してください。"
            );
        } else {
            eprintln!("❌ --init requires an interactive terminal (TTY).");
            eprintln!("   Run this command directly in a terminal, not via pipe or CI.");
        }
        std::process::exit(1);
    }

    // ── Step 1: Output language ───────────────────────────────────────────
    // Resolved first so the banner and all subsequent messages can use t!().
    let lang_val: String = if let Some(lang) = lang_override {
        // --lang was explicitly passed on CLI; skip the interactive prompt.
        lang.to_string()
    } else {
        println!("--- Step 1: Output language / 出力言語 ---");
        println!("  1. English (en)  [default]");
        println!("  2. Japanese (ja) / 日本語");
        println!();
        let lang_choice: usize = loop {
            let raw = c!(prompt_line("Language [1]: "));
            let v = raw.trim();
            let n: usize = if v.is_empty() {
                1
            } else {
                v.parse().unwrap_or(0)
            };
            if (1..=2).contains(&n) {
                break n;
            }
            println!("  Please enter 1 or 2.");
        };
        if lang_choice == 2 {
            "ja".to_string()
        } else {
            "en".to_string()
        }
    };
    macro_rules! t {
        ($en:expr, $ja:expr $(,)?) => {
            if lang_val == "ja" {
                $ja
            } else {
                $en
            }
        };
    }

    println!("=== xoksa setup wizard ===");
    println!();
    println!(
        "{}",
        t!(
            "This wizard creates xoksa.env interactively.",
            "このウィザードは xoksa.env を対話形式で作成します。"
        )
    );
    // Existing config: don't march the user through a full re-entry that then
    // replaces it. Confirm first, and let them inspect the current file.
    let cfg_env = crate::utils::env_path();
    let cfg_path: &std::path::Path = &cfg_env;
    if cfg_path.exists() {
        loop {
            println!();
            println!(
                "{}",
                t!(
                    "⚠️  An xoksa.env already exists in this folder.",
                    "⚠️  このフォルダには既に xoksa.env があります。"
                )
            );
            println!(
                "{}",
                t!(
                    "   Recreating it replaces your current settings (the old file is backed up to xoksa.env.bak_NNN).",
                    "   作り直すと現在の設定は置き換わります（旧ファイルは xoksa.env.bak_NNN に退避）。"
                )
            );
            println!(
                "  1. {}",
                t!("Show the current xoksa.env", "現在の xoksa.env を表示")
            );
            println!("  2. {}", t!("Recreate it (continue)", "作り直す（続行）"));
            println!(
                "  3. {}",
                t!("Cancel — keep my settings", "中止 — 設定はそのまま")
            );
            println!();
            let choice = c!(prompt_line(t!("Choice [3]: ", "選択 [3]: ")));
            match choice.trim() {
                "1" => match std::fs::read_to_string(cfg_path) {
                    Ok(contents) => {
                        println!();
                        println!("──────── xoksa.env ────────");
                        println!("{}", contents.trim_end());
                        println!("───────────────────────────");
                    }
                    Err(e) => eprintln!(
                        "{} {}",
                        t!("Could not read xoksa.env:", "xoksa.env を読めません:"),
                        e
                    ),
                },
                "2" => break,
                "3" | "" => {
                    println!(
                        "{}",
                        t!(
                            "Cancelled — no changes made.",
                            "中止しました — 変更はありません。"
                        )
                    );
                    return true;
                }
                _ => println!(
                    "{}",
                    t!(
                        "  Please enter 1, 2, or 3.",
                        "  1・2・3 を入力してください。"
                    )
                ),
            }
        }
    }
    println!(
        "{}",
        t!(
            "Before starting, please have your API keys ready.",
            "開始前に、以下の API キーをご用意ください。"
        )
    );
    println!();
    println!("  Brave Search API   https://brave.com/search/api/");
    println!("  OpenAI             https://platform.openai.com/api-keys");
    println!("  Gemini             https://aistudio.google.com/app/apikey");
    println!("  Claude (Anthropic) https://console.anthropic.com/settings/keys");
    println!("  J-Quants           https://application.jpx-jquants.com/");
    println!(
        "  Ollama (local LLM) https://ollama.com/  {}",
        t!("(no API key needed)", "(APIキー不要)")
    );
    println!();
    println!(
        "{}",
        t!(
            "API keys are masked with * as you type. Press Enter to confirm each value.",
            "APIキーは入力中 * でマスクされます。各値を確認して Enter を押してください。"
        )
    );
    println!(
        "{}",
        t!(
            "Optional fields can be skipped by pressing Enter without typing.",
            "任意項目は何も入力せず Enter でスキップできます。"
        )
    );
    println!();

    // ── Install mode ──────────────────────────────────────────────────────
    println!(
        "{}",
        t!("--- Install mode ---", "--- インストールモード ---")
    );
    println!(
        "  1.  {}",
        t!(
            "Quick   (LLM / News / Fundamental / Proxy)",
            "クイック （LLM・ニュース・ファンダメンタル・プロキシ）"
        )
    );
    println!(
        "  2.  {}",
        t!(
            "Expert  (+ Indicators / Thresholds / Log)",
            "エキスパート （＋ 指標・閾値・ログ設定）"
        )
    );
    println!();
    let expert_mode: bool = loop {
        let raw = c!(prompt_line(t!("Mode [1]: ", "モード [1]: ")));
        match raw.trim() {
            "" | "1" => break false,
            "2" => break true,
            _ => println!(
                "  {}",
                t!("Please enter 1 or 2.", "1 または 2 を入力してください。")
            ),
        }
    };
    println!();

    // ── Secure Key Storage: OS Keychain ───────────────────────────────────

    // ── Step 2: LLM providers (loop) ──────────────────────────────────────
    let mut openai_key: Option<Zeroizing<String>> = None;
    let mut gemini_key: Option<Zeroizing<String>> = None;
    let mut claude_key: Option<Zeroizing<String>> = None;
    // (alias, model, host, port); alias="" means single-instance (no OLLAMA_INSTANCES)
    let mut ollama_instances: Vec<(String, String, String, String)> = Vec::new();
    let mut no_llm = false;
    let mut primary_provider: u8 = 0; // 1=openai 2=gemini 3=claude 4=ollama

    loop {
        let configured: Vec<&str> = {
            let mut v = Vec::new();
            if openai_key.is_some() {
                v.push("OpenAI");
            }
            if gemini_key.is_some() {
                v.push("Gemini");
            }
            if claude_key.is_some() {
                v.push("Claude");
            }
            if !ollama_instances.is_empty() {
                v.push("Ollama");
            }
            v
        };
        println!(
            "{}",
            t!(
                "--- Step 2: LLM provider settings ---",
                "--- ステップ 2: LLMプロバイダー設定 ---"
            )
        );
        if configured.is_empty() {
            println!("  {}", t!("Configured: none", "設定済み: なし"));
        } else {
            println!(
                "  {}: {}",
                t!("Configured", "設定済み"),
                configured.join(", ")
            );
        }
        println!();
        println!(
            "  1.  OpenAI     {}",
            if openai_key.is_some() {
                "✅"
            } else {
                "(cloud)"
            }
        );
        println!(
            "  2.  Gemini     {}",
            if gemini_key.is_some() {
                "✅"
            } else {
                "(cloud)"
            }
        );
        println!(
            "  3.  Claude     {}",
            if claude_key.is_some() {
                "✅"
            } else {
                "(cloud)"
            }
        );
        println!(
            "  4.  Ollama     {}",
            if ollama_instances.is_empty() {
                t!("(local, no API key)", "(ローカル・APIキー不要)")
            } else if ollama_instances.len() == 1 {
                "✅ (1 instance)"
            } else {
                "✅ (multi-instance)"
            }
        );
        println!("  5.  {}", t!("Done", "終了"));
        println!();
        let choice: usize = loop {
            let raw = c!(prompt_line(t!("Select [5]: ", "選択 [5]: ")));
            let v = raw.trim();
            let n: usize = if v.is_empty() {
                5
            } else {
                v.parse().unwrap_or(0)
            };
            if (1..=5).contains(&n) {
                break n;
            }
            println!(
                "  {}",
                t!("Please enter 1 to 5.", "1 から 5 を入力してください。")
            );
        };
        match choice {
            1 => {
                let k = c!(prompt_required_secret(
                    "OpenAI API key: ",
                    t!("OPENAI_API_KEY is required.", "OPENAI_API_KEY は必須です。"),
                ));
                if primary_provider == 0 {
                    primary_provider = 1;
                }
                openai_key = Some(k);
            }
            2 => {
                let k = c!(prompt_required_secret(
                    "Gemini API key: ",
                    t!("GEMINI_API_KEY is required.", "GEMINI_API_KEY は必須です。"),
                ));
                if primary_provider == 0 {
                    primary_provider = 2;
                }
                gemini_key = Some(k);
            }
            3 => {
                let k = c!(prompt_required_secret(
                    "Claude API key: ",
                    t!("CLAUDE_API_KEY is required.", "CLAUDE_API_KEY は必須です。"),
                ));
                if primary_provider == 0 {
                    primary_provider = 3;
                }
                claude_key = Some(k);
            }
            4 => {
                loop {
                    let instance_num = ollama_instances.len() + 1;
                    println!(
                        "  {}",
                        t!(
                            format!("--- Ollama instance {} ---", instance_num),
                            format!("--- Ollama インスタンス {} ---", instance_num)
                        )
                    );
                    let alias = loop {
                        let raw = c!(prompt_line(t!(
                            "  Alias (required): ",
                            "  エイリアス（必須）: "
                        )));
                        let v = raw.trim().to_string();
                        if v.is_empty() {
                            println!("  {}", t!("Alias is required.", "エイリアスは必須です。"));
                            continue;
                        }
                        if v.contains(':') || v.contains('@') || v.contains(',') {
                            println!(
                                "  {}",
                                t!("No ':', '@', or ',' allowed.", "':', '@', ',' は使用不可。")
                            );
                            continue;
                        }
                        break v;
                    };
                    let raw = c!(prompt_line(t!("  Model [llama3]: ", "  モデル [llama3]: ")));
                    let model = {
                        let v = raw.trim();
                        if v.is_empty() { "llama3" } else { v }.to_string()
                    };
                    let raw = c!(prompt_line(t!(
                        "  Host [127.0.0.1]: ",
                        "  ホスト [127.0.0.1]: "
                    )));
                    let host = {
                        let v = raw.trim();
                        if v.is_empty() { "127.0.0.1" } else { v }.to_string()
                    };
                    let port = loop {
                        let raw = c!(prompt_line(t!("  Port [11434]: ", "  ポート [11434]: ")));
                        let v = raw.trim().to_string();
                        if v.is_empty() {
                            break "11434".to_string();
                        }
                        if v.parse::<u16>().is_ok() {
                            break v;
                        }
                        println!(
                            "  {}",
                            t!(
                                "Please enter a valid port number (1-65535).",
                                "有効なポート番号 (1-65535) を入力してください。"
                            )
                        );
                    };
                    ollama_instances.push((alias, model, host, port));
                    let add_more = c!(prompt_line(t!(
                        "  Add another instance? [y/N]: ",
                        "  別のインスタンスを追加しますか？ [y/N]: "
                    )));
                    if add_more.trim().to_lowercase() != "y" {
                        break;
                    }
                    println!();
                }
                if primary_provider == 0 {
                    primary_provider = 4;
                }
            }
            _ => break, // 5 = Done
        }
        println!();
    }
    if primary_provider == 0 {
        no_llm = true;
    }
    println!();

    // ── Step 3: News (Brave) ──────────────────────────────────────────────
    println!(
        "{}",
        t!(
            "--- Step 3: News (Brave Search API) ---",
            "--- ステップ 3: ニュース (Brave Search API) ---"
        )
    );
    let news_raw = c!(prompt_line(t!(
        "Enable news collection? [Y/n]: ",
        "ニュース収集を有効にしますか？ [Y/n]: "
    )));
    let news_enabled = !matches!(news_raw.trim().to_ascii_lowercase().as_str(), "n" | "no");
    let brave_key: Option<Zeroizing<String>> = if news_enabled {
        let k = c!(prompt_required_secret(
            "Brave Search API key: ",
            t!(
                "BRAVE_API_KEY is required for news collection.",
                "BRAVE_API_KEY はニュース収集に必須です。"
            ),
        ));
        Some(k)
    } else {
        None
    };
    println!();

    // ── Step 4: Fundamental data ──────────────────────────────────────────
    println!(
        "{}",
        t!(
            "--- Step 4: Fundamental data ---",
            "--- ステップ 4: ファンダメンタルデータ ---"
        )
    );
    println!("  1. J-Quants   {}", t!("(Japanese stocks)", "(日本株)"));
    println!("  2. SEC EDGAR  {}", t!("(US stocks)", "(米国株)"));
    println!("  3. {}", t!("Both", "両方"));
    println!(
        "  4. {}       {}",
        t!("None", "なし"),
        t!("(skip)", "(スキップ)")
    );
    println!();
    let fund_choice: usize = loop {
        let raw = c!(prompt_line(t!(
            "Fundamental data [4]: ",
            "ファンダメンタルデータ [4]: "
        )));
        let v = raw.trim();
        let n: usize = if v.is_empty() {
            4
        } else {
            v.parse().unwrap_or(0)
        };
        if (1..=4).contains(&n) {
            break n;
        }
        println!(
            "  {}",
            t!(
                "Please enter a number between 1 and 4.",
                "1 から 4 の数字を入力してください。"
            )
        );
    };
    let jq_key: Option<Zeroizing<String>> = if matches!(fund_choice, 1 | 3) {
        let k = c!(prompt_required_secret(
            "J-Quants API key: ",
            t!(
                "JQUANTS_API_KEY is required for J-Quants data.",
                "JQUANTS_API_KEY は J-Quants データに必須です。"
            ),
        ));
        Some(k)
    } else {
        None
    };
    let sec_agent: Option<String> = if matches!(fund_choice, 2 | 3) {
        let v = c!(prompt_line_required_no_quote(
            t!(
                "SEC EDGAR user-agent (e.g. xoksa/1.0 your@email.com): ",
                "SEC EDGAR ユーザーエージェント (例: xoksa/1.0 あなたのメールアドレス): "
            ),
            t!(
                "SEC_USER_AGENT is required for SEC EDGAR data.",
                "SEC_USER_AGENT は SEC EDGAR データに必須です。"
            ),
        ));
        Some(v)
    } else {
        None
    };
    println!();

    // ── Step 5: Proxy ─────────────────────────────────────────────────────
    let proxy = c!(collect_proxy_settings(&lang_val));

    // ── Step 6: Expert settings ───────────────────────────────────────────
    let mut buy_rsi: f64 = 30.0;
    let mut sell_rsi: f64 = 70.0;
    let mut macd_diff_low: f64 = 2.0;
    let mut macd_diff_mid: f64 = 10.0;
    let mut macd_diff_extreme: f64 = 100.0;
    let mut macd_minus_ok = false;
    let mut ind_ema = true;
    let mut ind_sma = true;
    let mut ind_fibonacci = true;
    let mut ind_stochastics = true;
    let mut ind_adx = true;
    let mut ind_roc = true;
    let mut ind_bollinger = true;
    let mut ind_vwap = true;
    let mut ind_ichimoku = true;
    let mut chat_default_ticker = String::new();
    let mut chat_analysis_mode = String::new();
    let mut chat_memory = "mid".to_string();
    let mut chat_guard = "high".to_string();
    let debate = "summary".to_string();
    let mut autoreload = false;
    let mut autoreload_notify = false;
    let mut save_technical_log = false;
    let mut log_format = "json".to_string();

    if expert_mode {
        // A. Technical indicators
        println!(
            "{}",
            t!(
                "--- Step 6A: Technical indicators ---",
                "--- ステップ 6A: テクニカル指標 ---"
            )
        );
        println!(
            "  {}",
            t!(
                "Enter number to toggle ON/OFF. Enter 0 when done.",
                "番号を入力して ON/OFF を切替。0 で完了。"
            )
        );
        loop {
            println!();
            let on = t!("ON", "ON");
            let off = t!("OFF", "OFF");
            println!("  1.  EMA          [{}]", if ind_ema { on } else { off });
            println!("  2.  SMA          [{}]", if ind_sma { on } else { off });
            println!(
                "  3.  Fibonacci    [{}]",
                if ind_fibonacci { on } else { off }
            );
            println!(
                "  4.  Stochastics  [{}]",
                if ind_stochastics { on } else { off }
            );
            println!("  5.  ADX          [{}]", if ind_adx { on } else { off });
            println!("  6.  ROC          [{}]", if ind_roc { on } else { off });
            println!(
                "  7.  Bollinger    [{}]",
                if ind_bollinger { on } else { off }
            );
            println!("  8.  VWAP         [{}]", if ind_vwap { on } else { off });
            println!(
                "  9.  Ichimoku     [{}]",
                if ind_ichimoku { on } else { off }
            );
            println!("  0.  {}", t!("Done", "完了"));
            println!();
            let raw = c!(prompt_line(t!("Toggle [0]: ", "トグル [0]: ")));
            match raw.trim() {
                "" | "0" => break,
                "1" => ind_ema = !ind_ema,
                "2" => ind_sma = !ind_sma,
                "3" => ind_fibonacci = !ind_fibonacci,
                "4" => ind_stochastics = !ind_stochastics,
                "5" => ind_adx = !ind_adx,
                "6" => ind_roc = !ind_roc,
                "7" => ind_bollinger = !ind_bollinger,
                "8" => ind_vwap = !ind_vwap,
                "9" => ind_ichimoku = !ind_ichimoku,
                _ => println!(
                    "  {}",
                    t!("Please enter 0-9.", "0 から 9 を入力してください。")
                ),
            }
        }
        println!();

        // RSI / MACD thresholds
        println!(
            "{}",
            t!(
                "--- Step 6A: RSI / MACD thresholds ---",
                "--- ステップ 6A: RSI / MACD 閾値 ---"
            )
        );
        buy_rsi = c!(prompt_f64(t!("BUY_RSI [30.0]: ", "BUY_RSI [30.0]: "), 30.0));
        sell_rsi = c!(prompt_f64(
            t!("SELL_RSI [70.0]: ", "SELL_RSI [70.0]: "),
            70.0
        ));
        macd_diff_low = c!(prompt_f64(
            t!("MACD_DIFF_LOW [2.0]: ", "MACD_DIFF_LOW [2.0]: "),
            2.0
        ));
        macd_diff_mid = c!(prompt_f64(
            t!("MACD_DIFF_MID [10.0]: ", "MACD_DIFF_MID [10.0]: "),
            10.0
        ));
        macd_diff_extreme = c!(prompt_f64(
            t!("MACD_DIFF_EXTREME [100.0]: ", "MACD_DIFF_EXTREME [100.0]: "),
            100.0
        ));
        macd_minus_ok = c!(prompt_bool(
            t!(
                "MACD_MINUS_OK - allow buy signals when MACD<0 [false] (y/n): ",
                "MACD_MINUS_OK - MACDがマイナスでも買いシグナルを許可 [false] (y/n): "
            ),
            false,
        ));
        println!();

        // B. Log
        println!(
            "{}",
            t!(
                "--- Step 6B: Log settings ---",
                "--- ステップ 6B: ログ設定 ---"
            )
        );
        save_technical_log = c!(prompt_bool(
            t!(
                "SAVE_TECHNICAL_LOG - save analysis to file [false] (y/n): ",
                "SAVE_TECHNICAL_LOG - テクニカル分析をファイルに保存 [false] (y/n): "
            ),
            false,
        ));
        if save_technical_log {
            log_format = c!(prompt_choice(
                t!(
                    "LOG_FORMAT [json] (json/csv): ",
                    "LOG_FORMAT [json] (json/csv): "
                ),
                &["json", "csv"],
                "json",
            ));
        }
        println!();
    }

    // ── Chat mode wizard ──────────────────────────────────────────────────
    let configure_chat = c!(prompt_bool(
        t!(
            "Configure chat mode settings? (y/n): ",
            "チャットモードの設定を行いますか？ (y/n): "
        ),
        false,
    ));
    if configure_chat {
        println!(
            "{}",
            t!("--- Chat mode settings ---", "--- チャットモード設定 ---")
        );
        println!();

        // 銘柄固定
        let raw = c!(prompt_line(t!(
            "Fixed ticker(s) for chat (e.g. AAPL or AAPL,MSFT,NVDA) [Enter = skip]: ",
            "チャット固定銘柄 (例: AAPL または AAPL,MSFT,NVDA) [Enter でスキップ]: "
        )));
        chat_default_ticker = raw.trim().to_string();

        // 足モード
        let raw = c!(prompt_line(t!(
            "Bar mode (daily/60m/30m/15m/5m/1m/weekly/monthly) [Enter = same as analysis mode]: ",
            "足モード (daily/60m/30m/15m/5m/1m/weekly/monthly) [Enter = 解析モードと同じ]: "
        )));
        let v = raw.trim().to_ascii_lowercase();
        // Validity is decided by the single source of truth (AnalysisMode::from_value),
        // so this wizard never maintains its own list of accepted bar-mode tokens.
        chat_analysis_mode = if crate::config::AnalysisMode::from_value(&v).is_some() {
            v
        } else {
            String::new()
        };

        // チャットメモリ
        chat_memory = c!(prompt_choice(
            t!(
                "Chat memory [mid] (low/mid/high): ",
                "チャットメモリ [mid] (low/mid/high): "
            ),
            &["low", "mid", "high"],
            "mid",
        ));

        // チャットガード
        chat_guard = c!(prompt_choice(
            t!(
                "Chat guard [high] (high/mid/low): ",
                "チャットガード [high] (high/mid/low): "
            ),
            &["high", "mid", "low"],
            "high",
        ));

        // オートリロード
        autoreload = c!(prompt_bool(
            t!(
                "Auto-reload - automatically re-fetch analysis at a fixed interval in intraday chat? (y/n): ",
                "オートリロード - 分足モードのチャット中、一定間隔で分析データを自動更新しますか？ (y/n): "
            ),
            false,
        ));
        if autoreload {
            autoreload_notify = c!(prompt_bool(
                t!(
                    "  Show notification on each reload? (y/n): ",
                    "  リロード時に通知を表示しますか？ (y/n): "
                ),
                false,
            ));
        }
        println!();
    }

    let values = SetupValues {
        lang: lang_val.clone(),
        openai_key,
        gemini_key,
        claude_key,
        brave_key,
        jq_key,
        // Cloud LLM models are not prompted by --init; empty → build_env writes the
        // built-in defaults (kept identical to the desktop form).
        openai_model: String::new(),
        gemini_model: String::new(),
        claude_model: String::new(),
        no_llm,
        primary_provider,
        ollama_instances,
        notify_channels: Vec::new(),
        lan_access: false,
        serve_port: crate::server::DEFAULT_SERVE_PORT,
        sec_agent,
        alias_csv: None,
        proxy,
        buy_rsi,
        sell_rsi,
        macd_diff_low,
        macd_diff_mid,
        macd_diff_extreme,
        macd_minus_ok,
        ind_ema,
        ind_sma,
        ind_fibonacci,
        ind_stochastics,
        ind_adx,
        ind_roc,
        ind_bollinger,
        ind_vwap,
        ind_ichimoku,
        chat_default_ticker,
        chat_analysis_mode,
        chat_memory,
        chat_guard,
        debate,
        autoreload,
        autoreload_notify,
        news_enabled,
        fund_choice,
        save_technical_log,
        log_format,
        // Weights are not prompted by --init; the defaults preserve the prior
        // hardcoded WEIGHT_* output. (The desktop GUI can tune them.)
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
        // Calc params not prompted by --init; defaults keep them commented out.
        ema_short_period: 5,
        ema_long_period: 20,
        sma_short_period: 5,
        sma_long_period: 20,
        adx_period: 14,
        roc_period: 10,
        stochastics_period: 14,
        bollinger_period: 20,
        bollinger_stddev_multiplier: 2.0,
        bb_bandwidth_squeeze_pct: 8.0,
        ichimoku_tenkan_period: 9,
        ichimoku_kijun_period: 26,
        vwap_period: 14,
        fibonacci_neutral_ratio: 0.05,
    };
    apply_setup(&values)
}

/// One chat-notification channel to persist: the non-secret fields go to
/// `xoksa.env` (`NOTIFY_<n>_{KIND,NAME,TO}`); the secret is Class A and goes to
/// the OS keychain (`NOTIFY_<n>_SECRET`), never the env file. `<n>` is the 1-based
/// position in `SetupValues::notify_channels`, so the KIND and SECRET always agree.
pub struct NotifySetup {
    /// Platform token: `slack | discord | gchat | line`.
    pub kind: String,
    pub name: String,
    pub to: Option<String>,
    pub secret: Option<Zeroizing<String>>,
}

/// All values collected by the interactive `--init` wizard, bundled so the
/// build+install logic can be reused headlessly (e.g. the v2.3.0 desktop GUI
/// onboarding) without the TTY prompt loop. API keys stay `Zeroizing`.
pub struct SetupValues {
    pub lang: String,
    pub openai_key: Option<Zeroizing<String>>,
    pub gemini_key: Option<Zeroizing<String>>,
    pub claude_key: Option<Zeroizing<String>>,
    pub brave_key: Option<Zeroizing<String>>,
    pub jq_key: Option<Zeroizing<String>>,
    /// Cloud LLM model overrides; empty string → use the built-in default.
    pub openai_model: String,
    pub gemini_model: String,
    pub claude_model: String,
    pub no_llm: bool,
    pub primary_provider: u8,
    pub ollama_instances: Vec<(String, String, String, String)>,
    /// Chat-notification channels (`NOTIFY_<n>_*`); empty on the CLI wizard path.
    pub notify_channels: Vec<NotifySetup>,
    /// Desktop only: bind the in-process server to `0.0.0.0` (LAN-reachable)
    /// instead of loopback. Off by default — the dashboard has no built-in auth.
    pub lan_access: bool,
    /// Port the engine's dashboard listens on (`SERVE_PORT`). Remote devices and
    /// the settings app's restart both depend on it, so it is a visible setting
    /// rather than an env-file-only value.
    pub serve_port: u16,
    pub sec_agent: Option<String>,
    pub alias_csv: Option<String>,
    pub proxy: Zeroizing<String>,
    pub buy_rsi: f64,
    pub sell_rsi: f64,
    pub macd_diff_low: f64,
    pub macd_diff_mid: f64,
    pub macd_diff_extreme: f64,
    pub macd_minus_ok: bool,
    pub ind_ema: bool,
    pub ind_sma: bool,
    pub ind_fibonacci: bool,
    pub ind_stochastics: bool,
    pub ind_adx: bool,
    pub ind_roc: bool,
    pub ind_bollinger: bool,
    pub ind_vwap: bool,
    pub ind_ichimoku: bool,
    pub chat_default_ticker: String,
    pub chat_analysis_mode: String,
    pub chat_memory: String,
    pub chat_guard: String,
    pub debate: String,
    pub autoreload: bool,
    pub autoreload_notify: bool,
    pub news_enabled: bool,
    pub fund_choice: usize,
    pub save_technical_log: bool,
    pub log_format: String,
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
    pub ema_short_period: usize,
    pub ema_long_period: usize,
    pub sma_short_period: usize,
    pub sma_long_period: usize,
    pub adx_period: usize,
    pub roc_period: usize,
    pub stochastics_period: usize,
    pub bollinger_period: usize,
    pub bollinger_stddev_multiplier: f64,
    pub bb_bandwidth_squeeze_pct: f64,
    pub ichimoku_tenkan_period: usize,
    pub ichimoku_kijun_period: usize,
    pub vwap_period: usize,
    pub fibonacci_neutral_ratio: f64,
}

// A calculation-parameter line: commented when the value equals the default (so
// `--init` output is unchanged — the default stays implicit), uncommented when
// the value was tuned (e.g. via the desktop GUI). `{:?}` keeps f64 trailing .0.
fn param_usize(key: &str, val: usize, def: usize) -> String {
    if val == def {
        format!("#{key}={def}\n")
    } else {
        format!("{key}={val}\n")
    }
}
fn param_f64(key: &str, val: f64, def: f64) -> String {
    if val == def {
        format!("#{key}={def:?}\n")
    } else {
        format!("{key}={val:?}\n")
    }
}

/// Build and atomically install `xoksa.env` and write API keys to the OS
/// keychain from already-collected values. Shared by `--init` (TTY) and the
/// desktop GUI onboarding so both write byte-identical files. Returns false on
/// any failure (message to stderr).
pub fn apply_setup(values: &SetupValues) -> bool {
    let lang_val = values.lang.as_str();
    macro_rules! t {
        ($en:expr, $ja:expr $(,)?) => {
            if lang_val == "ja" {
                $ja
            } else {
                $en
            }
        };
    }
    let env_pb = crate::utils::env_path();
    // Fresh machine: the canonical config dir (e.g. %APPDATA%\xoksa) may not exist
    // yet — create it before the atomic temp-write below.
    if let Some(dir) = env_pb.parent() {
        if !dir.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(dir);
        }
    }
    let path: &std::path::Path = &env_pb;

    // Single source of every default written below: `Config::default()`. The
    // template writes these values so a fresh xoksa.env matches the code default
    // exactly (SOT §4.2) — no literal is duplicated here.
    let dcfg = crate::config::Config::default();

    // ── Build complete xoksa.env ──────────────────────────────────────────
    let mut s = String::with_capacity(4096);
    s.push_str("# xoksa.env  --  Generated by --init\n\n");

    // Language — always written explicitly. English used to be expressed by
    // commenting the key out, which made "English" and "never chosen"
    // indistinguishable and let every surface guess differently.
    s.push_str("# ===== Language =====\n");
    if lang_val == "ja" {
        s.push_str("LANG=ja\n");
    } else {
        s.push_str("LANG=en\n");
    }
    s.push('\n');

    // API Keys — managed keys are stored in secure storage, not in xoksa.env
    s.push_str("# ===== API Keys =====\n");
    s.push_str("#BRAVE_API_KEY=\n");
    s.push_str("#OPENAI_API_KEY=\n");
    s.push_str("#GEMINI_API_KEY=\n");
    s.push_str("#CLAUDE_API_KEY=\n");
    s.push_str("#JQUANTS_API_KEY=\n");
    match values.sec_agent.as_deref() {
        Some(v) => {
            s.push_str(&format_env_assignment("SEC_USER_AGENT", v));
            s.push('\n');
        }
        None => s.push_str("#SEC_USER_AGENT=\n"),
    }
    s.push('\n');

    // LLM provider
    s.push_str("# ===== LLM provider =====\n");
    if values.no_llm {
        s.push_str(&format!(
            "NO_LLM=true\nllm_provider={}\nllm_timeout_seconds={}\nllm_temperature={:?}\nllm_top_p={:?}\nllm_max_output_tokens={}\n",
            dcfg.llm_provider,
            dcfg.llm_timeout_secs,
            dcfg.llm_temperature,
            dcfg.llm_top_p,
            dcfg.llm_max_output_tokens,
        ));
    } else {
        let provider_str = match values.primary_provider {
            1 => "openai",
            2 => "gemini",
            3 => "claude",
            4 => "ollama",
            _ => "openai",
        };
        s.push_str(&format!("llm_provider={}\n", provider_str));
        // Ollama's local generation is slower, so seed a longer request timeout for
        // it; every other provider starts at the code default.
        s.push_str(&if values.primary_provider == 4 {
            "llm_timeout_seconds=300\n".to_string()
        } else {
            format!("llm_timeout_seconds={}\n", dcfg.llm_timeout_secs)
        });
        s.push_str(&format!(
            "llm_temperature={:?}\nllm_top_p={:?}\nllm_max_output_tokens={}\nNO_LLM=false\n",
            dcfg.llm_temperature, dcfg.llm_top_p, dcfg.llm_max_output_tokens,
        ));
    }
    s.push('\n');

    // OpenAI. Empty override → the current balanced-tier default.
    let openai_model: String = if values.openai_model.trim().is_empty() {
        dcfg.openai_model.clone()
    } else {
        clean_env_value(values.openai_model.trim())
    };
    s.push_str("# ===== OpenAI =====\n");
    s.push_str(&format!("openai_model={}\n\n", openai_model));

    // Gemini
    let gemini_model: String = if values.gemini_model.trim().is_empty() {
        dcfg.gemini_model.clone()
    } else {
        clean_env_value(values.gemini_model.trim())
    };
    s.push_str("# ===== Gemini =====\n");
    s.push_str(&format!("gemini_model={}\n\n", gemini_model));

    // Claude
    let claude_model: String = if values.claude_model.trim().is_empty() {
        dcfg.claude_model.clone()
    } else {
        clean_env_value(values.claude_model.trim())
    };
    s.push_str("# ===== Claude =====\n");
    s.push_str(&format!(
        "claude_model={}\nclaude_max_tokens={}\n\n",
        claude_model, dcfg.claude_max_tokens
    ));

    // Ollama
    s.push_str("# ===== Ollama =====\n");
    if values.ollama_instances.is_empty() {
        s.push_str("#OLLAMA_1_ALIAS=gpu1\n#OLLAMA_1_HOST=127.0.0.1\n#OLLAMA_1_PORT=11434\n#OLLAMA_1_MODEL=llama3\n");
        s.push_str("#OLLAMA_TEMPERATURE=0.2\n#OLLAMA_TOP_P=0.9\n#OLLAMA_TOP_K=40\n");
        s.push_str(&format!(
            "#OLLAMA_REPEAT_PENALTY=1.1\n#OLLAMA_NUM_CTX={}\n#OLLAMA_NUM_PREDICT={}\n",
            dcfg.ollama_num_ctx, dcfg.ollama_num_predict
        ));
        s.push_str(&format!(
            "#OLLAMA_SEED={}\n#OLLAMA_KEEP_ALIVE=10m\n#OLLAMA_NO_GUARD=false\n#OLLAMA_THINK=low\n",
            dcfg.ollama_seed
        ));
    } else {
        for (i, (alias, model, host, port)) in values.ollama_instances.iter().enumerate() {
            let n = i + 1;
            s.push_str(&format!(
                "{}\n",
                format_env_assignment(&format!("OLLAMA_{}_ALIAS", n), alias)
            ));
            s.push_str(&format!(
                "{}\n",
                format_env_assignment(&format!("OLLAMA_{}_HOST", n), host)
            ));
            s.push_str(&format!(
                "{}\n",
                format_env_assignment(&format!("OLLAMA_{}_PORT", n), port)
            ));
            s.push_str(&format!(
                "{}\n",
                format_env_assignment(&format!("OLLAMA_{}_MODEL", n), model)
            ));
            s.push('\n');
        }
        s.push_str("OLLAMA_TEMPERATURE=0.2\nOLLAMA_TOP_P=0.9\nOLLAMA_TOP_K=40\n");
        s.push_str(&format!(
            "OLLAMA_REPEAT_PENALTY=1.1\nOLLAMA_NUM_CTX={}\nOLLAMA_NUM_PREDICT={}\n",
            dcfg.ollama_num_ctx, dcfg.ollama_num_predict
        ));
        s.push_str(&format!(
            "OLLAMA_SEED={}\nOLLAMA_KEEP_ALIVE=10m\nOLLAMA_NO_GUARD=false\n#OLLAMA_THINK=low\n",
            dcfg.ollama_seed
        ));
    }
    // Advanced / operational (commented = use defaults). The ollama timeout floor is
    // the single source in llm.rs (SOT §4.2), not a literal here.
    s.push_str(&format!(
        "#OLLAMA_TIMEOUT_SECONDS={}\n#OLLAMA_BENCH_MODELS=\n#OLLAMA_BENCH_FORMAT=table\n#OLLAMA_DEBUG=false\n",
        crate::llm::OLLAMA_MIN_TIMEOUT_SECS
    ));
    s.push('\n');

    // Chat notification channels. Only the non-secret fields are written here; the
    // secret (NOTIFY_<n>_SECRET) is Class A and lives in the OS keychain. `<n>` is
    // the 1-based position, matching the keychain entry stored below.
    s.push_str("# ===== Chat notification (alerts) =====\n");
    if values.notify_channels.is_empty() {
        s.push_str("#NOTIFY_1_KIND=slack\n#NOTIFY_1_NAME=team\n");
        s.push_str("#NOTIFY_2_KIND=line\n#NOTIFY_2_NAME=me\n#NOTIFY_2_TO=U0123456789abcdef\n");
    } else {
        for (i, ch) in values.notify_channels.iter().enumerate() {
            let n = i + 1;
            s.push_str(&format!(
                "NOTIFY_{}_KIND={}\n",
                n,
                clean_env_value(&ch.kind)
            ));
            if !ch.name.trim().is_empty() {
                s.push_str(&format!(
                    "NOTIFY_{}_NAME={}\n",
                    n,
                    clean_env_value(&ch.name)
                ));
            }
            if let Some(to) = ch.to.as_deref().filter(|v| !v.trim().is_empty()) {
                s.push_str(&format!("NOTIFY_{}_TO={}\n", n, clean_env_value(to)));
            }
            s.push('\n');
        }
    }
    s.push('\n');

    // Desktop app: bind the in-process server to the LAN (0.0.0.0) instead of
    // loopback. Off by default — the dashboard has NO auth/TLS, so enabling this
    // exposes it (and the configured LLM budget) to every host on the network.
    s.push_str("# ===== Desktop =====\n");
    s.push_str(&format!("DESKTOP_LAN_ACCESS={}\n", values.lan_access));
    s.push_str(&format!("SERVE_PORT={}\n\n", values.serve_port));

    // Technical thresholds
    s.push_str("# ===== Technical thresholds =====\n");
    s.push_str(&format!(
        "BUY_RSI={}\nSELL_RSI={}\nMACD_DIFF_LOW={}\nMACD_DIFF_MID={}\nMACD_DIFF_EXTREME={}\nMACD_MINUS_OK={}\n\n",
        values.buy_rsi, values.sell_rsi, values.macd_diff_low, values.macd_diff_mid, values.macd_diff_extreme, values.macd_minus_ok
    ));

    // Indicators
    s.push_str("# ===== Extended technical indicators =====\n");
    for (name, val) in [
        ("EMA", values.ind_ema),
        ("SMA", values.ind_sma),
        ("FIBONACCI", values.ind_fibonacci),
        ("STOCHASTICS", values.ind_stochastics),
        ("ADX", values.ind_adx),
        ("ROC", values.ind_roc),
        ("BOLLINGER", values.ind_bollinger),
        ("VWAP", values.ind_vwap),
        ("ICHIMOKU", values.ind_ichimoku),
    ] {
        s.push_str(&format!(
            "{}={}\n",
            name,
            if val { "True" } else { "False" }
        ));
    }
    s.push('\n');

    // Weights
    s.push_str("# ===== Indicator weights =====\n");
    // {:?} (Debug) keeps the trailing .0 (e.g. 2.0), matching the previous literal
    // strings; {} would print `2` and break byte-identity with the old --init output.
    s.push_str(&format!(
        "WEIGHT_BASIC={:?}\nWEIGHT_EMA={:?}\nWEIGHT_SMA={:?}\nWEIGHT_BOLLINGER={:?}\n",
        values.weight_basic, values.weight_ema, values.weight_sma, values.weight_bollinger
    ));
    s.push_str(&format!(
        "WEIGHT_ROC={:?}\nWEIGHT_ADX={:?}\nWEIGHT_STOCHASTICS={:?}\n",
        values.weight_roc, values.weight_adx, values.weight_stochastics
    ));
    s.push_str(&format!(
        "WEIGHT_FIBONACCI={:?}\nWEIGHT_VWAP={:?}\nWEIGHT_ICHIMOKU={:?}\n\n",
        values.weight_fibonacci, values.weight_vwap, values.weight_ichimoku
    ));

    // Indicator calculation parameters. Each line stays commented (implicit
    // default) unless tuned, so the untuned output matches the previous defaults.
    s.push_str("# ===== Indicator calculation parameters =====\n");
    s.push_str(&param_usize(
        "EMA_SHORT_PERIOD",
        values.ema_short_period,
        dcfg.ema_short_period,
    ));
    s.push_str(&param_usize(
        "EMA_LONG_PERIOD",
        values.ema_long_period,
        dcfg.ema_long_period,
    ));
    s.push_str(&param_usize(
        "SMA_SHORT_PERIOD",
        values.sma_short_period,
        dcfg.sma_short_period,
    ));
    s.push_str(&param_usize(
        "SMA_LONG_PERIOD",
        values.sma_long_period,
        dcfg.sma_long_period,
    ));
    s.push_str(&param_usize(
        "ADX_PERIOD",
        values.adx_period,
        dcfg.adx_period,
    ));
    s.push_str(&param_usize(
        "ROC_PERIOD",
        values.roc_period,
        dcfg.roc_period,
    ));
    s.push_str(&param_usize(
        "STOCHASTICS_PERIOD",
        values.stochastics_period,
        dcfg.stochastics_period,
    ));
    s.push_str(&param_usize(
        "BOLLINGER_PERIOD",
        values.bollinger_period,
        dcfg.bollinger_period,
    ));
    s.push_str(&param_f64(
        "BOLLINGER_STDDEV_MULTIPLIER",
        values.bollinger_stddev_multiplier,
        dcfg.bollinger_stddev_multiplier,
    ));
    s.push_str(&param_f64(
        "BB_BANDWIDTH_SQUEEZE_PCT",
        values.bb_bandwidth_squeeze_pct,
        dcfg.bb_bandwidth_squeeze_pct,
    ));
    s.push_str(&param_usize(
        "ICHIMOKU_TENKAN_PERIOD",
        values.ichimoku_tenkan_period,
        dcfg.ichimoku_tenkan_period,
    ));
    s.push_str(&param_usize(
        "ICHIMOKU_KIJUN_PERIOD",
        values.ichimoku_kijun_period,
        dcfg.ichimoku_kijun_period,
    ));
    s.push_str(&param_usize(
        "VWAP_PERIOD",
        values.vwap_period,
        dcfg.vwap_period,
    ));
    s.push_str(&param_f64(
        "FIBONACCI_NEUTRAL_RATIO",
        values.fibonacci_neutral_ratio,
        dcfg.fibonacci_neutral_ratio,
    ));
    s.push('\n');

    // Stance
    s.push_str("# ===== Investment stance =====\nSTANCE=holder\n\n");

    // Chat mode
    s.push_str("# ===== Chat mode settings =====\n");
    if values.chat_default_ticker.is_empty() {
        s.push_str("#CHAT_DEFAULT_TICKER=\n");
    } else {
        s.push_str(&format!(
            "CHAT_DEFAULT_TICKER={}\n",
            values.chat_default_ticker
        ));
    }
    if values.chat_analysis_mode.is_empty() {
        s.push_str("#CHAT_ANALYSIS_MODE=daily\n");
    } else {
        s.push_str(&format!(
            "CHAT_ANALYSIS_MODE={}\n",
            values.chat_analysis_mode
        ));
    }
    s.push_str(&format!(
        "CHAT_MEMORY={}\nCHAT_GUARD={}\nDEBATE={}\n",
        values.chat_memory, values.chat_guard, values.debate
    ));
    s.push_str(&format!(
        "AUTORELOAD={}\nAUTORELOAD_NOTIFY={}\n",
        values.autoreload, values.autoreload_notify
    ));
    // Response style session defaults (override at runtime with /answer-tone, /read-depth, etc.)
    s.push_str(
        "#READ_DEPTH=mid\n#KNOWLEDGE_SCOPE=mid\n#RESPONSE_SHAPE=talk\n#FORECAST_MODE=soft\n\n",
    );

    // LLM extra note (appended to every analysis prompt)
    s.push_str("# ===== LLM extra note =====\n#EXTRA_NOTE=\n\n");

    // Analysis mode
    s.push_str("# ===== Analysis mode =====\n");
    s.push_str("#ANALYSIS_MODE=daily\n\n");

    // News
    s.push_str("# ===== News =====\n");
    s.push_str(if values.news_enabled {
        "NO_NEWS=false\n"
    } else {
        "NO_NEWS=true\n"
    });
    s.push_str(&format!(
        "SHOW_NEWS=false\nNEWS_FILTER=true\nNEWS_COUNT={}\nNEWS_FRESHNESS={}\n#CUSTOM_NEWS_QUERY=\nNO_ALIAS=false\n\n",
        dcfg.news_count, dcfg.news_freshness
    ));

    // Fundamental
    let fundamental_enabled = matches!(values.fund_choice, 1..=3);
    s.push_str(&format!(
        "# ===== Fundamental data =====\nFUNDAMENTAL={}\n\n",
        if fundamental_enabled { "true" } else { "false" }
    ));

    // Log
    s.push_str("# ===== Log settings =====\n");
    s.push_str(&format!(
        "SAVE_TECHNICAL_LOG={}\nLOG_FORMAT={}\nLOG_DIR=log\nCSV_APPEND=false\nLOG_FLAT=false\n\n",
        values.save_technical_log, values.log_format
    ));

    // Character limits
    s.push_str("# ===== Character limits =====\n");
    s.push_str(&format!(
        "MAX_NOTE_LENGTH={}\nMAX_SHORTTERM_LENGTH={}\nMAX_MIDTERM_LENGTH={}\nMAX_NEWS_LENGTH={}\nMAX_REVIEW_LENGTH={}\n\n",
        dcfg.max_note_length,
        dcfg.max_shortterm_length,
        dcfg.max_midterm_length,
        dcfg.max_news_length,
        dcfg.max_review_length,
    ));

    // Proxy
    let proxy_str = values.proxy.as_str();
    if proxy_str.is_empty() {
        s.push_str("# ===== Proxy =====\n#HTTPS_PROXY=\n#HTTP_PROXY=\n#NO_PROXY=\n\n");
    } else {
        s.push_str(proxy_str);
        if !proxy_str.contains("NO_PROXY=") {
            s.push_str("#NO_PROXY=\n");
        }
        s.push('\n');
    }

    // Debug
    s.push_str("# ===== Debug =====\nDEBUG_PROMPT=false\n\n");

    // Alias CSV (GUI-only setting; the CLI wizard leaves it as a commented
    // placeholder). Written like SEC_USER_AGENT: value when present, else a
    // commented stub.
    s.push_str("# ===== Alias CSV =====\n");
    match values.alias_csv.as_deref() {
        Some(v) if !v.is_empty() => {
            s.push_str(&format_env_assignment("ALIAS_CSV", v));
            s.push('\n');
        }
        _ => s.push_str("#ALIAS_CSV=\n"),
    }

    let content = Zeroizing::new(s);

    // ── Write to temp file first (atomic: original untouched until swap) ──
    let tmp_path = format!("{}.tmp", path.display());
    {
        let mut tmp_opts = OpenOptions::new();
        tmp_opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            tmp_opts.mode(0o600);
        }
        let mut tmp_file = match tmp_opts.open(&tmp_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("❌ Failed to create temp file: {}", e);
                return false;
            }
        };
        if let Err(e) = tmp_file.write_all(content.as_bytes()) {
            let _ = std::fs::remove_file(&tmp_path);
            eprintln!("❌ Failed to write config: {}", e);
            return false;
        }
    }

    // ── Save API keys to OS Keychain before activating the file ─────────
    // Snapshot old values first. A read error here means we cannot safely
    // roll back, so abort before writing anything.
    let mut kc_names: Vec<(String, Option<&Zeroizing<String>>)> = vec![
        ("BRAVE_API_KEY".to_string(), values.brave_key.as_ref()),
        ("OPENAI_API_KEY".to_string(), values.openai_key.as_ref()),
        ("GEMINI_API_KEY".to_string(), values.gemini_key.as_ref()),
        ("CLAUDE_API_KEY".to_string(), values.claude_key.as_ref()),
        ("JQUANTS_API_KEY".to_string(), values.jq_key.as_ref()),
    ];
    // Chat-notification channel secrets (Class A) — same keychain path, same
    // rollback protection. `<n>` matches the env KIND written above.
    for (i, ch) in values.notify_channels.iter().enumerate() {
        kc_names.push((format!("NOTIFY_{}_SECRET", i + 1), ch.secret.as_ref()));
    }
    let mut kc_old: Vec<(&str, Option<Zeroizing<String>>)> = Vec::new();
    for (name, val_opt) in &kc_names {
        if val_opt.is_some() {
            match crate::keystore::get_key(name) {
                Ok(v) => kc_old.push((name.as_str(), v)),
                Err(e) => {
                    let _ = std::fs::remove_file(&tmp_path);
                    eprintln!("❌ Cannot read existing {name} from OS Keychain: {e}");
                    eprintln!(
                        "{}",
                        t!(
                            "   Aborting — xoksa.env was not created.",
                            "   中断します — xoksa.env は作成されませんでした。"
                        )
                    );
                    return false;
                }
            }
        }
    }
    // On any subsequent failure, restore each key to its snapshot value;
    // new entries (old = None) are deleted. Failures during rollback are
    // reported so the user knows manual recovery may be needed.
    let kc_rollback = |saved_count: usize| {
        for (name, old_opt) in kc_old.iter().take(saved_count) {
            let result = match old_opt {
                Some(v) => crate::keystore::set_key(name, v.as_str()),
                None => crate::keystore::delete_key(name),
            };
            if let Err(e) = result {
                eprintln!("⚠️  Rollback failed for {name}: {e}  (manual recovery may be needed)");
            }
        }
    };
    let mut kc_saved = 0usize;
    for (name, val_opt) in &kc_names {
        if let Some(val) = val_opt {
            if let Err(e) = crate::keystore::set_key(name, val.as_str()) {
                eprintln!("❌ Failed to store {name} in OS Keychain: {e}");
                kc_rollback(kc_saved);
                let _ = std::fs::remove_file(&tmp_path);
                eprintln!(
                    "{}",
                    t!(
                        "❌ OS Keychain write failed; xoksa.env was not created.",
                        "❌ OS キーチェーンへの書き込みに失敗しました。xoksa.env は作成されませんでした。"
                    )
                );
                return false;
            }
            kc_saved += 1;
        }
    }

    // ── Back up existing xoksa.env, then rename temp into place ──────────
    let mut bak_name: Option<String> = None;
    if path.exists() {
        let bak = 'find_bak: {
            for n in 1u32.. {
                let candidate = match path.parent().filter(|d| !d.as_os_str().is_empty()) {
                    Some(dir) => dir
                        .join(format!("xoksa.env.bak_{:03}", n))
                        .to_string_lossy()
                        .into_owned(),
                    None => format!("xoksa.env.bak_{:03}", n),
                };
                match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&candidate)
                {
                    Ok(_) => break 'find_bak candidate,
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => {
                        let _ = std::fs::remove_file(&tmp_path);
                        kc_rollback(kc_saved);
                        eprintln!("❌ Failed to reserve backup slot: {}", e);
                        return false;
                    }
                }
            }
            unreachable!()
        };
        // On Windows, rename fails when the destination already exists.
        // Remove the empty placeholder created by create_new before renaming.
        #[cfg(windows)]
        let _ = std::fs::remove_file(&bak);
        if let Err(e) = std::fs::rename(path, &bak) {
            let _ = std::fs::remove_file(&bak);
            let _ = std::fs::remove_file(&tmp_path);
            kc_rollback(kc_saved);
            eprintln!("❌ Failed to back up xoksa.env → {}: {}", bak, e);
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = std::fs::set_permissions(&bak, std::fs::Permissions::from_mode(0o600)) {
                if let Err(re) = std::fs::rename(&bak, path) {
                    eprintln!("⚠️  Failed to restore xoksa.env from {}: {re}", bak);
                }
                let _ = std::fs::remove_file(&tmp_path);
                kc_rollback(kc_saved);
                eprintln!("❌ Failed to set permissions on backup {}: {}", bak, e);
                return false;
            }
        }
        println!(
            "{}",
            t!(
                format!("ℹ️  Backed up existing xoksa.env → {}", bak),
                format!("ℹ️  既存の xoksa.env を {} にバックアップしました。", bak),
            )
        );
        bak_name = Some(bak);
    }
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        if let Some(ref bak) = bak_name {
            if let Err(re) = std::fs::rename(bak, path) {
                eprintln!("⚠️  Failed to restore xoksa.env from {}: {re}", bak);
                eprintln!("   Manual recovery: rename {} to xoksa.env", bak);
            }
        }
        kc_rollback(kc_saved);
        eprintln!("❌ Failed to install xoksa.env: {}", e);
        return false;
    }

    println!();
    println!(
        "✅ {}",
        t!("xoksa.env has been created.", "xoksa.env を作成しました。")
    );
    println!(
        "   {}",
        t!(
            "Review the file and adjust any values before running xoksa.",
            "実行前にファイルを確認し、必要に応じて値を調整してください。"
        )
    );

    true
}

/// Configuration diagnostics: reads xoksa.env and reports status without
/// displaying any API key values.
pub fn run_doctor(env_map: &HashMap<String, String>) {
    println!("=== xoksa doctor ===");
    println!();

    // 1. File presence
    let env_pb = crate::utils::env_path();
    let env_path: &std::path::Path = &env_pb;
    if env_path.exists() {
        println!("✅ xoksa.env found");
    } else {
        println!("❌ xoksa.env not found  (run --init to create one)");
    }

    println!();

    // 2. LLM provider setting
    let no_llm = env_map
        .get("NO_LLM")
        .map(|v| v.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if no_llm {
        println!("--- LLM ---");
        println!("   ℹ️  NO_LLM=true  (AI analysis disabled)");
    } else {
        let provider = env_map
            .get("llm_provider")
            .map(String::as_str)
            .unwrap_or("openai");
        println!("--- LLM provider: {} ---", provider);

        match provider {
            "openai" => {
                check_key_line("OPENAI_API_KEY", "OpenAI API key");
            }
            "gemini" => {
                check_key_line("GEMINI_API_KEY", "Gemini API key");
            }
            "claude" => {
                check_key_line("CLAUDE_API_KEY", "Claude API key");
            }
            "ollama" => {
                let instances = crate::config::scan_ollama_instances(env_map);
                let (host, port) = instances
                    .first()
                    .map(|i| (i.host.as_str(), i.port))
                    .unwrap_or(("127.0.0.1", 11434));
                print!("   Ollama reachability ({}:{}) ... ", host, port);
                let _ = io::stdout().flush();
                match check_ollama_reachable(host.to_string(), port) {
                    Some(models) if models.is_empty() => {
                        println!("✅ reachable (no models loaded)");
                    }
                    Some(models) => {
                        println!("✅ reachable");
                        for m in &models {
                            println!("      model: {}", m);
                        }
                    }
                    None => {
                        println!("❌ not reachable");
                    }
                }
            }
            other => {
                println!("⚠️  Unknown provider: {}", other);
            }
        }
    }

    println!();

    // 3. News (Brave)
    println!("--- News (Brave) ---");
    let no_news = env_map
        .get("NO_NEWS")
        .map(|v| v.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if no_news {
        println!("   ⚠️  NO_NEWS=true  (news search disabled)");
    } else {
        check_key_line("BRAVE_API_KEY", "Brave Search API key");
    }

    println!();

    // 4. Fundamental data
    println!("--- Fundamental data ---");
    // J-Quants is v2-only: a single API key (V1's token auth was retired 2026-06-01).
    use crate::keystore::KeyPresence;
    match crate::keystore::resolve_key_presence("JQUANTS_API_KEY") {
        KeyPresence::Found(_) => println!("✅ J-Quants: JQUANTS_API_KEY is set"),
        KeyPresence::KeyringError => {
            println!("⚠️  J-Quants: keyring access error (JQUANTS_API_KEY could not be read)")
        }
        KeyPresence::NotFound => {
            println!(
                "   ℹ️  J-Quants: no credentials set  (use --fundamental -t TICKER.T to enable)"
            )
        }
    }

    // SEC EDGAR
    let agent = env_map
        .get("SEC_USER_AGENT")
        .map(String::as_str)
        .unwrap_or("");
    if !agent.is_empty() {
        println!("✅ SEC EDGAR: SEC_USER_AGENT is set");
    } else {
        println!(
            "   ℹ️  SEC EDGAR: SEC_USER_AGENT not set  (use --fundamental -t TICKER to enable)"
        );
    }

    println!();

    // 5. Proxy
    println!("--- Proxy ---");
    let has_https_proxy = crate::utils::key_present_in_env_file("HTTPS_PROXY")
        || crate::utils::key_present_in_env_file("HTTP_PROXY");
    if has_https_proxy {
        println!("✅ HTTPS_PROXY / HTTP_PROXY: set (value masked)");
    } else {
        println!("   ℹ️  HTTPS_PROXY / HTTP_PROXY: not set");
    }
    let has_no_proxy = crate::utils::key_present_in_env_file("NO_PROXY");
    if has_no_proxy {
        println!("✅ NO_PROXY: set");
    } else {
        println!("   ℹ️  NO_PROXY: not set");
    }
    println!("   ℹ️  Ollama always bypasses proxy (no_proxy enforced in code)");

    println!();

    // 6. ALIAS_CSV file presence
    println!("--- Files ---");
    let lang_val = env_map
        .get("LANG")
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let ja = lang_val == "ja";
    let alias_csv_val = env_map.get("ALIAS_CSV");
    match alias_csv_val {
        Some(path) if !path.is_empty() => {
            if std::path::Path::new(path.as_str()).exists() {
                println!("✅ ALIAS_CSV: {} (found)", path);
            } else {
                println!(
                    "⚠️  ALIAS_CSV: {} ({})",
                    path,
                    if ja {
                        "ファイルが見つかりません — 銘柄名表示が無効になります"
                    } else {
                        "file not found — company name display disabled"
                    }
                );
            }
        }
        _ => {
            println!(
                "   ℹ️  ALIAS_CSV: not set ({})",
                if ja {
                    "銘柄名表示は無効"
                } else {
                    "company name display disabled"
                }
            );
        }
    }

    println!();
    println!("=== doctor complete ===");
}

// ── internal helpers ───────────────────────────────────────────────────────

fn prompt_line(label: &str) -> Option<String> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
    print!("{}", label);
    let _ = io::stdout().flush();
    if enable_raw_mode().is_err() {
        let mut buf = String::new();
        io::stdin().read_line(&mut buf).unwrap_or(0);
        return Some(buf);
    }
    let mut input = String::new();
    while let Ok(Event::Key(key)) = event::read() {
        // Windows delivers both Press and Release events for each keystroke;
        // process Press only so a single tap is not counted twice (e.g. "1"→"11").
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Enter => {
                print!("\r\n");
                let _ = io::stdout().flush();
                break;
            }
            KeyCode::Backspace | KeyCode::Delete => {
                if !input.is_empty() {
                    input.pop();
                    print!("\x08 \x08");
                    let _ = io::stdout().flush();
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let _ = disable_raw_mode();
                eprint!("\r\nAborted.\r\n");
                return None;
            }
            KeyCode::Char(c) => {
                input.push(c);
                print!("{}", c);
                let _ = io::stdout().flush();
            }
            _ => {}
        }
    }
    let _ = disable_raw_mode();
    Some(input)
}

/// Prompt for a secret value, echoing `*` for each character typed.
/// Backspace erases the last `*`. Ctrl-C restores the terminal and exits.
/// Returns a Zeroizing wrapper so the key material is wiped on drop.
fn prompt_secret(label: &str) -> Option<Zeroizing<String>> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
    print!("{}", label);
    let _ = io::stdout().flush();
    if let Err(e) = enable_raw_mode() {
        eprintln!("❌ Failed to enable terminal raw mode: {}", e);
        return None;
    }
    // Reserved up front, because `Zeroizing` wipes the buffer a String owns when
    // it drops — not the ones it abandoned on the way. Growing from empty
    // reallocates at 8, 16, 32 … and each old buffer is freed still holding the
    // leading characters of the key. A capacity no realistic secret exceeds means
    // there is only ever one buffer, and that one is wiped.
    let mut input = Zeroizing::new(String::with_capacity(256));
    while let Ok(Event::Key(key)) = event::read() {
        // Windows delivers both Press and Release events for each keystroke;
        // process Press only so a single tap is not counted twice (e.g. "1"→"11").
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Enter => {
                print!("\r\n");
                let _ = io::stdout().flush();
                break;
            }
            KeyCode::Backspace | KeyCode::Delete => {
                if !input.is_empty() {
                    (*input).pop();
                    print!("\x08 \x08");
                    let _ = io::stdout().flush();
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let _ = disable_raw_mode();
                eprint!("\r\nAborted.\r\n");
                return None;
            }
            KeyCode::Char(c) => {
                (*input).push(c);
                print!("*");
                let _ = io::stdout().flush();
            }
            _ => {}
        }
    }
    let _ = disable_raw_mode();
    Some(input)
}

fn prompt_f64(label: &str, default: f64) -> Option<f64> {
    loop {
        let raw = prompt_line(label)?;
        let v = raw.trim();
        if v.is_empty() {
            return Some(default);
        }
        if let Ok(n) = v.parse::<f64>() {
            return Some(n);
        }
        println!("  Please enter a valid number.");
    }
}

fn prompt_bool(label: &str, default: bool) -> Option<bool> {
    loop {
        let raw = prompt_line(label)?;
        match raw.trim().to_ascii_lowercase().as_str() {
            "" => return Some(default),
            "y" | "yes" | "true" | "1" => return Some(true),
            "n" | "no" | "false" | "0" => return Some(false),
            _ => println!("  Please enter y or n."),
        }
    }
}

fn prompt_choice(label: &str, choices: &[&str], default: &str) -> Option<String> {
    loop {
        let raw = prompt_line(label)?;
        let v = raw.trim().to_ascii_lowercase();
        if v.is_empty() {
            return Some(default.to_string());
        }
        if choices.contains(&v.as_str()) {
            return Some(v);
        }
        println!("  Valid options: {}", choices.join(" / "));
    }
}

/// Write `key=value` as a valid xoksa.env assignment.
/// Values containing `#` or surrounding whitespace are double-quoted so that
/// parse_env_value() does not truncate at an inline `#` comment marker.
/// Callers must ensure `val` does not contain `"` (use prompt_secret_validated).
/// Neutralise a user-supplied value before it is written into xoksa.env: strip
/// control characters (a newline could otherwise inject a second env line) and cap
/// the length. The onboarding form path is local-only, but this keeps the written
/// config well-formed regardless of pasted input.
fn clean_env_value(val: &str) -> String {
    // Drop control chars AND zero-width / BOM characters (e.g. U+FEFF pasted from a
    // web page) and `"`: the strict env reader rejects control/BOM chars, and one
    // such char in a single value would otherwise make the loader discard the whole
    // file. Then cap by BYTES (not chars) so the written `KEY=VALUE` line cannot
    // exceed the reader's 500-byte per-line limit even for multibyte (e.g. Japanese)
    // values. See sanitize_env_file_lines_lenient / env_line_issue in utils.rs.
    let cleaned: String = val
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(
                    *c,
                    '\u{FEFF}' | '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '"'
                )
        })
        .collect();
    const MAX_BYTES: usize = 400;
    if cleaned.len() <= MAX_BYTES {
        cleaned
    } else {
        let mut end = MAX_BYTES;
        while end > 0 && !cleaned.is_char_boundary(end) {
            end -= 1;
        }
        cleaned[..end].to_string()
    }
}

fn format_env_assignment(key: &str, val: &str) -> String {
    let val = clean_env_value(val);
    if val.contains('#') || val != val.trim() {
        format!("{}=\"{}\"", key, val)
    } else {
        format!("{}={}", key, val)
    }
}

/// Prompt for a plain (non-secret) value, re-prompting if it contains `"`.
/// Enforces the same no-double-quote contract as prompt_secret_validated so
/// that format_env_assignment() can safely quote values containing `#`.
fn prompt_line_no_quote(label: &str) -> Option<String> {
    loop {
        let val = prompt_line(label)?;
        if !val.contains('"') {
            return Some(val);
        }
        println!("  Input must not contain double-quote characters. Please try again.");
    }
}

/// Prompt for a required plain value, re-prompting until non-empty and without `"`.
fn prompt_line_required_no_quote(label: &str, empty_msg: &str) -> Option<String> {
    loop {
        let val = prompt_line_no_quote(label)?;
        let v = val.trim().to_string();
        if v.is_empty() {
            println!("  {}", empty_msg);
        } else {
            return Some(v);
        }
    }
}

/// Prompt for a required secret, re-prompting until a non-empty value without `"` is entered.
fn prompt_required_secret(label: &str, empty_msg: &str) -> Option<Zeroizing<String>> {
    loop {
        let val = prompt_secret(label)?;
        if val.contains('"') {
            println!("  Input must not contain double-quote characters. Please try again.");
        } else if val.is_empty() {
            println!("  {}", empty_msg);
        } else {
            return Some(val);
        }
    }
}

/// Prompt for a secret, re-prompting if the value contains `"`.
/// Values with `"` cannot be safely quoted for parse_env_value() round-trip.
fn prompt_secret_validated(label: &str) -> Option<Zeroizing<String>> {
    loop {
        let val = prompt_secret(label)?;
        if !val.contains('"') {
            return Some(val);
        }
        println!("  Input must not contain double-quote characters. Please try again.");
    }
}

/// Prompt for optional proxy settings.
/// HTTPS_PROXY may contain credentials — secret masking is applied.
/// Returns a Zeroizing block of lines to append to the config, or an empty wrapper if skipped.
fn collect_proxy_settings(lang_val: &str) -> Option<Zeroizing<String>> {
    macro_rules! t {
        ($en:expr, $ja:expr $(,)?) => {
            if lang_val == "ja" {
                $ja
            } else {
                $en
            }
        };
    }
    println!();
    println!(
        "{}",
        t!("--- Step 5: Proxy ---", "--- ステップ 5: プロキシ ---")
    );
    println!();
    let yn = prompt_line(t!(
        "Use a proxy? [y/N]: ",
        "プロキシを使用しますか？ [y/N]: "
    ))?;
    if !matches!(yn.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        return Some(Zeroizing::new(String::new()));
    }
    println!();
    println!(
        "  {}",
        t!(
            "Enter the proxy server URL.",
            "プロキシサーバーの URL を入力してください。"
        )
    );
    println!(
        "  {}",
        t!(
            "Note: the setting name is HTTPS_PROXY but the URL typically starts with http://.",
            "※ 設定名は HTTPS_PROXY ですが、URL は通常 http:// で始まります。",
        )
    );
    println!("  e.g.  http://192.168.0.10:8080");
    println!("        socks5://proxy.corp.example:1080");
    println!();
    println!(
        "  {}",
        t!(
            "Note: authenticated proxy URLs (user:password@...) are not supported. Configure proxy authentication at the OS level.",
            "※ 認証付きプロキシURL（user:password@...）は非対応です。プロキシ認証はOSレベルで設定してください。",
        )
    );
    println!();
    let https_proxy = prompt_secret_validated("HTTPS_PROXY: ")?;
    if https_proxy.is_empty() {
        return Some(Zeroizing::new(String::new()));
    }
    if https_proxy.contains('@') {
        println!();
        println!(
            "  {}",
            t!(
                "❌ Authenticated proxy URLs are not supported. Aborting proxy setup.",
                "❌ 認証付きプロキシURLは非対応です。プロキシ設定を中断します。",
            )
        );
        return Some(Zeroizing::new(String::new()));
    }
    let no_proxy_raw = prompt_line_no_quote(t!(
        "NO_PROXY hosts to bypass, comma-separated (blank to skip): ",
        "NO_PROXY に除外するホスト（カンマ区切り、スキップする場合は空欄）: ",
    ))?;
    let mut s = String::new();
    s.push_str("\n# ===== Proxy =====\n");
    let proxy_line = Zeroizing::new(format_env_assignment("HTTPS_PROXY", &https_proxy));
    s.push_str(&proxy_line);
    s.push('\n');
    // HTTP_PROXY gets the same value — most corporate proxies require both.
    let http_proxy_line = Zeroizing::new(format_env_assignment("HTTP_PROXY", &https_proxy));
    s.push_str(&http_proxy_line);
    s.push('\n');
    let no_proxy = no_proxy_raw.trim();
    if !no_proxy.is_empty() {
        s.push_str(&format_env_assignment("NO_PROXY", no_proxy));
        s.push('\n');
    }
    Some(Zeroizing::new(s))
}

// ── check-keys ─────────────────────────────────────────────────────────────

/// Report stored key presence for all managed keys. Values are never displayed.
pub fn run_check_keys() {
    const KEYS: &[(&str, &str)] = &[
        ("OPENAI_API_KEY", "OpenAI"),
        ("GEMINI_API_KEY", "Gemini"),
        ("CLAUDE_API_KEY", "Claude"),
        ("BRAVE_API_KEY", "Brave Search"),
        ("JQUANTS_API_KEY", "J-Quants (API key)"),
    ];

    println!();
    println!("--- Stored Key Status ---");
    for (key, label) in KEYS {
        match crate::keystore::resolve_key_presence(key) {
            crate::keystore::KeyPresence::Found(src) => {
                println!("  {:<18} ✅ set ({})", label, src)
            }
            crate::keystore::KeyPresence::NotFound => {
                println!("  {:<18} ❌ not set", label)
            }
            crate::keystore::KeyPresence::KeyringError => {
                println!("  {:<18} ⚠️  keyring access error", label)
            }
        }
    }
    println!();
}

// ── update-key ─────────────────────────────────────────────────────────────

/// Interactive wizard to update one stored secret in the OS keychain: an API key
/// (OpenAI / Gemini / Claude / Brave / J-Quants) or a chat-notification channel
/// secret (a Slack/Discord/Google Chat webhook URL, or a LINE bot token).
pub fn run_update_key() -> bool {
    macro_rules! c {
        ($e:expr) => {
            match $e {
                Some(v) => v,
                None => return false,
            }
        };
    }
    use std::io::IsTerminal;
    let _ = crossterm::terminal::disable_raw_mode();
    if !std::io::stdin().is_terminal() {
        eprintln!("❌ --update-key requires an interactive terminal (TTY).");
        std::process::exit(1);
    }

    const KEYS: &[(&str, &str)] = &[
        ("OPENAI_API_KEY", "OpenAI"),
        ("GEMINI_API_KEY", "Gemini"),
        ("CLAUDE_API_KEY", "Claude"),
        ("BRAVE_API_KEY", "Brave Search"),
        ("JQUANTS_API_KEY", "J-Quants (API key)"),
    ];

    // The four supported chat-notification platforms are ALWAYS listed (like the
    // API keys), each shown set / not-set. Selecting one registers or updates that
    // platform's channel secret; if no channel of that platform exists yet, it is
    // created (`NOTIFY_<n>_{KIND,NAME[,TO]}` appended to xoksa.env) before the
    // secret is stored in the keychain.
    use crate::notify::NotifierKind;
    const PLATFORMS: &[(NotifierKind, &str)] = &[
        (NotifierKind::Slack, "Slack"),
        (NotifierKind::Discord, "Discord"),
        (NotifierKind::GoogleChat, "Google Chat"),
        (NotifierKind::Line, "LINE"),
    ];

    enum Item {
        Key {
            keyname: &'static str,
            label: &'static str,
        },
        Chat {
            kind: NotifierKind,
            label: &'static str,
        },
    }

    loop {
        // Re-scan each iteration so a channel just created shows up immediately.
        let env_map = crate::bootstrap::load_env_map();
        let channels = crate::config::scan_notify_channels(&env_map);
        let channel_of = |kind: NotifierKind| channels.iter().find(|c| c.kind == kind).cloned();

        let mut items: Vec<Item> = KEYS
            .iter()
            .map(|&(keyname, label)| Item::Key { keyname, label })
            .collect();
        for &(kind, label) in PLATFORMS {
            items.push(Item::Chat { kind, label });
        }

        println!();
        println!("--- Update Stored Secrets ---");
        for (i, item) in items.iter().enumerate() {
            if i == 0 {
                println!("  API keys");
            } else if i == KEYS.len() {
                println!();
                println!("  Chat notifications");
            }
            let (label, presence) = match item {
                Item::Key { keyname, label } => {
                    (*label, crate::keystore::resolve_key_presence(keyname))
                }
                Item::Chat { kind, label } => {
                    let presence = match channel_of(*kind) {
                        Some(ch) => crate::keystore::resolve_key_presence(&format!(
                            "NOTIFY_{}_SECRET",
                            ch.n
                        )),
                        None => crate::keystore::KeyPresence::NotFound,
                    };
                    (*label, presence)
                }
            };
            match presence {
                crate::keystore::KeyPresence::Found(src) => {
                    println!("  {}.  {:<30} ✅ set ({})", i + 1, label, src)
                }
                crate::keystore::KeyPresence::NotFound => {
                    println!("  {}.  {:<30} ❌ not set", i + 1, label)
                }
                crate::keystore::KeyPresence::KeyringError => {
                    println!("  {}.  {:<30} ⚠️  keyring access error", i + 1, label)
                }
            }
        }
        println!("  0.  Exit");
        println!();

        let raw = c!(prompt_line("Select item to update [0]: "));
        let choice: usize = match raw.trim() {
            "" | "0" => break,
            v => v.parse().unwrap_or(99),
        };
        if choice < 1 || choice > items.len() {
            println!("  Please enter 0–{}.", items.len());
            continue;
        }

        match &items[choice - 1] {
            Item::Key { keyname, label } => {
                let val = c!(prompt_required_secret(
                    &format!("{}: ", label),
                    &format!("{} is required.", keyname),
                ));
                match crate::keystore::set_key(keyname, val.as_str()) {
                    Ok(()) => println!("✅ {} updated.", label),
                    Err(e) => eprintln!("❌ Failed to store {}: {}", keyname, e),
                }
            }
            Item::Chat { kind, label } => {
                // Use the existing channel of this platform, or create a new one.
                let (n, to) = match channel_of(*kind) {
                    Some(ch) => (ch.n, ch.to.clone()),
                    None => {
                        let Some(n) = (1u8..=16).find(|n| channels.iter().all(|c| c.n != *n))
                        else {
                            eprintln!("❌ No free notification channel slot (max 16).");
                            continue;
                        };
                        // No channel-name prompt: the label is always the platform
                        // name. A separate, unmasked "name" prompt here invited users
                        // to paste the webhook URL (the secret) — leaking it in
                        // plaintext to xoksa.env. The only input for a webhook channel
                        // is its secret (masked), prompted below.
                        let name = kind.as_str().to_string();
                        let to = if *kind == NotifierKind::Line {
                            Some(c!(prompt_line_required_no_quote(
                                "LINE destination id (to): ",
                                "A LINE destination id is required.",
                            )))
                        } else {
                            None
                        };
                        if let Err(e) =
                            append_notify_channel(n, kind.as_str(), &name, to.as_deref())
                        {
                            eprintln!("❌ Failed to write the channel to xoksa.env: {e}");
                            continue;
                        }
                        println!(
                            "✅ Added {} channel “{}” (NOTIFY_{}) to xoksa.env.",
                            label, name, n
                        );
                        (n, to)
                    }
                };

                let val = c!(prompt_required_secret(
                    &format!("{} secret (webhook URL / LINE token): ", label),
                    "A secret is required.",
                ));
                // Validate up front (host allowlist / https, or a LINE destination)
                // so a wrong-platform value is rejected now, not at dispatch time.
                if let Err(e) = crate::notify::build_notifier(*kind, val.clone(), to.clone()) {
                    eprintln!("❌ Invalid secret for {}: {}", label, e);
                    continue;
                }
                match crate::keystore::set_key(&format!("NOTIFY_{}_SECRET", n), val.as_str()) {
                    Ok(()) => println!("✅ {} updated.", label),
                    Err(e) => eprintln!("❌ Failed to store the {} secret: {}", label, e),
                }
            }
        }
    }
    true
}

/// Append a notification channel's non-secret keys (`NOTIFY_<n>_{KIND,NAME[,TO]}`)
/// to `xoksa.env` in the current directory, preserving existing content. The
/// secret is NOT written here — it goes to the keychain.
fn append_notify_channel(n: u8, kind: &str, name: &str, to: Option<&str>) -> std::io::Result<()> {
    use std::io::Write;
    let target = crate::utils::env_path();
    if let Some(dir) = target.parent() {
        if !dir.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(dir);
        }
    }
    let mut f = OpenOptions::new().create(true).append(true).open(&target)?;
    writeln!(f)?;
    writeln!(f, "NOTIFY_{}_KIND={}", n, clean_env_value(kind))?;
    writeln!(f, "NOTIFY_{}_NAME={}", n, clean_env_value(name))?;
    if let Some(to) = to {
        writeln!(f, "NOTIFY_{}_TO={}", n, clean_env_value(to))?;
    }
    Ok(())
}

// ── doctor utilities ───────────────────────────────────────────────────────

/// Print a ✅/❌ line for a key's presence (never prints the value).
/// Delegates to key_present_in_env_file() so export, case, and sanitize rules
/// match the runtime reader.
fn check_key_line(key: &str, label: &str) {
    match crate::keystore::resolve_key_presence(key) {
        crate::keystore::KeyPresence::Found(src) => println!("✅ {}: set ({})", label, src),
        crate::keystore::KeyPresence::NotFound => println!("❌ {}: not set", label),
        crate::keystore::KeyPresence::KeyringError => {
            println!("⚠️  {}: keyring access error", label)
        }
    }
}

/// Check Ollama /api/tags reachability and return model names if successful.
/// Uses a blocking client in a spawned thread to avoid tokio runtime conflicts.
/// Reused by the desktop GUI to offer the user's installed models.
pub fn check_ollama_reachable(host: String, port: u16) -> Option<Vec<String>> {
    let url = format!("http://{}:{}/api/tags", host, port);
    let handle = std::thread::spawn(move || -> Option<Vec<String>> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .ok()?;
        // Same bound as the chat path, for the same reason: this body arrives
        // decoded (see crate::llm::OLLAMA_MAX_RESPONSE_BYTES). `take` caps what
        // is read rather than trusting the declared length, and reading one byte
        // past the limit is what distinguishes "at the limit" from "over it".
        let res = client.get(&url).send().ok()?;
        let cap = crate::llm::OLLAMA_MAX_RESPONSE_BYTES;
        if res.content_length().is_some_and(|len| len > cap) {
            return None;
        }
        use std::io::Read as _;
        let mut buf: Vec<u8> = Vec::new();
        res.take(cap + 1).read_to_end(&mut buf).ok()?;
        if buf.len() as u64 > cap {
            return None;
        }
        let json: serde_json::Value = serde_json::from_slice(&buf).ok()?;
        json["models"].as_array().map(|arr| {
            arr.iter()
                .filter_map(|m| m["name"].as_str().map(str::to_owned))
                .collect()
        })
    });
    handle.join().ok().flatten()
}

/// Non-interactive subcommand: `xoksa ollama-models --host <h> --port <p>`.
/// Prints the reachable instance's model names as a JSON array to stdout (an
/// empty array when unreachable). The desktop UI shell calls this via a child
/// process instead of linking the engine; host/port are Class B (non-secret),
/// so passing them as arguments is acceptable.
pub fn run_ollama_models_cli(args: &[String]) -> anyhow::Result<()> {
    let mut host = "127.0.0.1".to_string();
    let mut port: u16 = 11434;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--host" => {
                if let Some(v) = it.next() {
                    host = v.clone();
                }
            }
            "--port" => {
                if let Some(v) = it.next() {
                    port = v.parse().unwrap_or(11434);
                }
            }
            _ => {}
        }
    }
    let models = check_ollama_reachable(host, port).unwrap_or_default();
    println!("{}", serde_json::to_string(&models)?);
    Ok(())
}

// Defaults for the optional Advanced fields of the apply-config payload — the
// `--init` Quick-mode values, so a payload that omits them writes the same
// config as before. Mirrors the desktop onboarding form's defaults.
// Every scalar default below derives from `Config::default()` — the single source
// of truth (SOT §4.2) — so the apply-config JSON defaults can never drift from the
// engine's own defaults. The shared helpers (`ac_w_one`, `ac_u5`, …) return a
// representative field that carries the canonical value for that group.
fn ac_serve_port() -> u16 {
    crate::server::DEFAULT_SERVE_PORT
}

fn ac_true() -> bool {
    true
}
fn ac_buy_rsi() -> f64 {
    crate::config::Config::default().buy_rsi
}
fn ac_sell_rsi() -> f64 {
    crate::config::Config::default().sell_rsi
}
fn ac_macd_low() -> f64 {
    crate::config::Config::default().macd_diff_low
}
fn ac_macd_mid() -> f64 {
    crate::config::Config::default().macd_diff_mid
}
fn ac_macd_extreme() -> f64 {
    crate::config::Config::default().macd_diff_extreme
}
fn ac_w_basic() -> f64 {
    crate::config::Config::default().weight_basic
}
fn ac_w_one() -> f64 {
    crate::config::Config::default().weight_ema
}
fn ac_u5() -> usize {
    crate::config::Config::default().ema_short_period
}
fn ac_u9() -> usize {
    crate::config::Config::default().ichimoku_tenkan_period
}
fn ac_u10() -> usize {
    crate::config::Config::default().roc_period
}
fn ac_u14() -> usize {
    crate::config::Config::default().adx_period
}
fn ac_u20() -> usize {
    crate::config::Config::default().ema_long_period
}
fn ac_u26() -> usize {
    crate::config::Config::default().ichimoku_kijun_period
}
fn ac_stddev() -> f64 {
    crate::config::Config::default().bollinger_stddev_multiplier
}
fn ac_bbpct() -> f64 {
    crate::config::Config::default().bb_bandwidth_squeeze_pct
}
fn ac_fibratio() -> f64 {
    crate::config::Config::default().fibonacci_neutral_ratio
}

#[derive(serde::Deserialize)]
struct ApplyConfigOllama {
    alias: String,
    model: String,
    host: String,
    port: String,
}

/// One chat-notification channel from the form. `kind` is `slack|discord|gchat|line`;
/// `secret` (webhook URL or LINE bot token) is Class A and stored in the keychain,
/// never written to `xoksa.env`.
#[derive(serde::Deserialize)]
struct ApplyConfigNotify {
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    to: String,
    #[serde(default)]
    secret: String,
}

/// Non-interactive config payload received as JSON on stdin by
/// `xoksa apply-config`. Field-for-field identical to the desktop onboarding
/// form so both write byte-identical `xoksa.env`/keychain entries.
#[derive(serde::Deserialize)]
struct ApplyConfigInput {
    lang: String,
    #[serde(default)]
    openai_key: String,
    #[serde(default)]
    gemini_key: String,
    #[serde(default)]
    claude_key: String,
    #[serde(default)]
    brave_key: String,
    #[serde(default)]
    jquants_key: String,
    #[serde(default)]
    openai_model: String,
    #[serde(default)]
    gemini_model: String,
    #[serde(default)]
    claude_model: String,
    #[serde(default)]
    no_llm: bool,
    primary_provider: u8,
    #[serde(default)]
    ollama: Vec<ApplyConfigOllama>,
    #[serde(default)]
    notify: Vec<ApplyConfigNotify>,
    #[serde(default)]
    sec_user_agent: String,
    #[serde(default)]
    alias_csv: String,
    #[serde(default)]
    news_enabled: bool,
    #[serde(default)]
    fundamental_enabled: bool,
    #[serde(default)]
    lan_access: bool,
    #[serde(default = "ac_serve_port")]
    serve_port: u16,
    #[serde(default = "ac_true")]
    ind_ema: bool,
    #[serde(default = "ac_true")]
    ind_sma: bool,
    #[serde(default = "ac_true")]
    ind_fibonacci: bool,
    #[serde(default = "ac_true")]
    ind_stochastics: bool,
    #[serde(default = "ac_true")]
    ind_adx: bool,
    #[serde(default = "ac_true")]
    ind_roc: bool,
    #[serde(default = "ac_true")]
    ind_bollinger: bool,
    #[serde(default = "ac_true")]
    ind_vwap: bool,
    #[serde(default = "ac_true")]
    ind_ichimoku: bool,
    #[serde(default)]
    macd_minus_ok: bool,
    #[serde(default = "ac_buy_rsi")]
    buy_rsi: f64,
    #[serde(default = "ac_sell_rsi")]
    sell_rsi: f64,
    #[serde(default = "ac_macd_low")]
    macd_diff_low: f64,
    #[serde(default = "ac_macd_mid")]
    macd_diff_mid: f64,
    #[serde(default = "ac_macd_extreme")]
    macd_diff_extreme: f64,
    #[serde(default = "ac_w_basic")]
    weight_basic: f64,
    #[serde(default = "ac_w_one")]
    weight_ema: f64,
    #[serde(default = "ac_w_one")]
    weight_sma: f64,
    #[serde(default = "ac_w_one")]
    weight_bollinger: f64,
    #[serde(default = "ac_w_one")]
    weight_roc: f64,
    #[serde(default = "ac_w_one")]
    weight_adx: f64,
    #[serde(default = "ac_w_one")]
    weight_stochastics: f64,
    #[serde(default = "ac_w_one")]
    weight_fibonacci: f64,
    #[serde(default = "ac_w_one")]
    weight_vwap: f64,
    #[serde(default = "ac_w_one")]
    weight_ichimoku: f64,
    #[serde(default = "ac_u5")]
    ema_short_period: usize,
    #[serde(default = "ac_u20")]
    ema_long_period: usize,
    #[serde(default = "ac_u5")]
    sma_short_period: usize,
    #[serde(default = "ac_u20")]
    sma_long_period: usize,
    #[serde(default = "ac_u14")]
    adx_period: usize,
    #[serde(default = "ac_u10")]
    roc_period: usize,
    #[serde(default = "ac_u14")]
    stochastics_period: usize,
    #[serde(default = "ac_u20")]
    bollinger_period: usize,
    #[serde(default = "ac_stddev")]
    bollinger_stddev_multiplier: f64,
    #[serde(default = "ac_bbpct")]
    bb_bandwidth_squeeze_pct: f64,
    #[serde(default = "ac_u9")]
    ichimoku_tenkan_period: usize,
    #[serde(default = "ac_u26")]
    ichimoku_kijun_period: usize,
    #[serde(default = "ac_u14")]
    vwap_period: usize,
    #[serde(default = "ac_fibratio")]
    fibonacci_neutral_ratio: f64,
}

fn ac_opt_key(mut s: String) -> Option<Zeroizing<String>> {
    // Copy the trimmed value into a zeroizing buffer, then wipe the original String
    // (serde-deserialized from the stdin JSON) so no un-zeroized copy of a Class A
    // key is left in freed memory (security-design §2). The `Zeroizing` result is
    // wiped on drop; the empty-input branch carries no key material.
    let result = {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(Zeroizing::new(t.to_string()))
        }
    };
    zeroize::Zeroize::zeroize(&mut s);
    result
}

fn ac_opt_str(s: String) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

impl ApplyConfigInput {
    fn into_setup_values(self) -> SetupValues {
        SetupValues {
            lang: if self.lang == "ja" { "ja" } else { "en" }.to_string(),
            openai_key: ac_opt_key(self.openai_key),
            gemini_key: ac_opt_key(self.gemini_key),
            claude_key: ac_opt_key(self.claude_key),
            brave_key: ac_opt_key(self.brave_key),
            jq_key: ac_opt_key(self.jquants_key),
            openai_model: self.openai_model.trim().to_string(),
            gemini_model: self.gemini_model.trim().to_string(),
            claude_model: self.claude_model.trim().to_string(),
            no_llm: self.no_llm,
            primary_provider: self.primary_provider,
            ollama_instances: self
                .ollama
                .into_iter()
                .map(|o| (o.alias, o.model, o.host, o.port))
                .collect(),
            // Keep only channels with a supported KIND; the 1-based position then
            // becomes `<n>` for both the env KIND line and the keychain SECRET.
            notify_channels: self
                .notify
                .into_iter()
                .filter(|c| crate::notify::NotifierKind::parse(&c.kind).is_some())
                .map(|c| NotifySetup {
                    kind: crate::notify::NotifierKind::parse(&c.kind)
                        .map(|k| k.as_str().to_string())
                        .unwrap_or_default(),
                    name: c.name.trim().to_string(),
                    to: ac_opt_str(c.to),
                    secret: ac_opt_key(c.secret),
                })
                .collect(),
            lan_access: self.lan_access,
            serve_port: self.serve_port,
            sec_agent: ac_opt_str(self.sec_user_agent),
            alias_csv: ac_opt_str(self.alias_csv),
            proxy: Zeroizing::new(String::new()),
            buy_rsi: self.buy_rsi,
            sell_rsi: self.sell_rsi,
            macd_diff_low: self.macd_diff_low,
            macd_diff_mid: self.macd_diff_mid,
            macd_diff_extreme: self.macd_diff_extreme,
            macd_minus_ok: self.macd_minus_ok,
            ind_ema: self.ind_ema,
            ind_sma: self.ind_sma,
            ind_fibonacci: self.ind_fibonacci,
            ind_stochastics: self.ind_stochastics,
            ind_adx: self.ind_adx,
            ind_roc: self.ind_roc,
            ind_bollinger: self.ind_bollinger,
            ind_vwap: self.ind_vwap,
            ind_ichimoku: self.ind_ichimoku,
            chat_default_ticker: String::new(),
            chat_analysis_mode: String::new(),
            chat_memory: crate::config::Config::default().chat_memory,
            chat_guard: crate::config::Config::default().chat_guard,
            debate: crate::config::Config::default().debate,
            autoreload: false,
            autoreload_notify: false,
            news_enabled: self.news_enabled,
            fund_choice: if self.fundamental_enabled { 3 } else { 4 },
            save_technical_log: false,
            log_format: crate::config::Config::default().log_format,
            weight_basic: self.weight_basic,
            weight_ema: self.weight_ema,
            weight_sma: self.weight_sma,
            weight_bollinger: self.weight_bollinger,
            weight_roc: self.weight_roc,
            weight_adx: self.weight_adx,
            weight_stochastics: self.weight_stochastics,
            weight_fibonacci: self.weight_fibonacci,
            weight_vwap: self.weight_vwap,
            weight_ichimoku: self.weight_ichimoku,
            ema_short_period: self.ema_short_period,
            ema_long_period: self.ema_long_period,
            sma_short_period: self.sma_short_period,
            sma_long_period: self.sma_long_period,
            adx_period: self.adx_period,
            roc_period: self.roc_period,
            stochastics_period: self.stochastics_period,
            bollinger_period: self.bollinger_period,
            bollinger_stddev_multiplier: self.bollinger_stddev_multiplier,
            bb_bandwidth_squeeze_pct: self.bb_bandwidth_squeeze_pct,
            ichimoku_tenkan_period: self.ichimoku_tenkan_period,
            ichimoku_kijun_period: self.ichimoku_kijun_period,
            vwap_period: self.vwap_period,
            fibonacci_neutral_ratio: self.fibonacci_neutral_ratio,
        }
    }
}

/// Non-interactive subcommand: `xoksa apply-config` reads a config JSON payload
/// from stdin (same shape as the desktop onboarding form), writes `xoksa.env`
/// and the OS keychain via the shared `apply_setup`, then prints `{"ok":true}`.
/// Keys arrive over stdin (never argv/HTTP) and the buffer is zeroized after use.
pub fn run_apply_config_cli() -> anyhow::Result<()> {
    use std::io::Read;
    use zeroize::Zeroize;
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    let parsed = serde_json::from_str::<ApplyConfigInput>(&buf);
    buf.zeroize();
    let input = parsed.map_err(|e| anyhow::anyhow!("invalid config JSON on stdin: {e}"))?;
    let values = input.into_setup_values();
    if !apply_setup(&values) {
        anyhow::bail!("failed to write configuration (check OS keychain access)");
    }
    println!("{{\"ok\":true}}");
    Ok(())
}

/// Non-interactive subcommand for the settings app: verify or set the settings-app
/// password. The password arrives on stdin (never argv/HTTP); the buffer is
/// zeroized after use. Exactly one of `--verify` / `--set` is required. `--verify`
/// prints `{"ok":<bool>}`; `--set` stores the value and prints `{"ok":true}`.
pub fn run_settings_password_cli(args: &[String]) -> anyhow::Result<()> {
    use std::io::Read;
    use zeroize::Zeroize;
    let set = args.iter().any(|a| a == "--set");
    let verify = args.iter().any(|a| a == "--verify");
    if set == verify {
        anyhow::bail!("settings-password requires exactly one of --verify or --set");
    }
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    let password = Zeroizing::new(buf.trim_end_matches(['\r', '\n']).to_string());
    buf.zeroize();
    if set {
        crate::keystore::set_settings_password(password.as_str())?;
    } else if !crate::keystore::verify_settings_password(password.as_str())? {
        println!("{{\"ok\":false}}");
        return Ok(());
    }
    println!("{{\"ok\":true}}");
    Ok(())
}

/// Non-interactive subcommand for the desktop: get/set its connection token (the
/// token it presents to a remote engine). `--show` prints `DESKTOP_CONN_TOKEN: <v>`
/// (nothing if unset); `--set` reads the value on stdin (never argv) and stores it
/// in the OS keychain (empty clears it).
pub fn run_conn_token_cli(args: &[String]) -> anyhow::Result<()> {
    use std::io::Read;
    use zeroize::Zeroize;
    if args.iter().any(|a| a == "--show") {
        if let Some(t) = crate::keystore::get_conn_token()? {
            println!("DESKTOP_CONN_TOKEN: {}", t.as_str());
        }
        return Ok(());
    }
    if args.iter().any(|a| a == "--set") {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        let token = Zeroizing::new(buf.trim_end_matches(['\r', '\n']).to_string());
        buf.zeroize();
        crate::keystore::set_conn_token(token.as_str())?;
        println!("{{\"ok\":true}}");
        return Ok(());
    }
    anyhow::bail!("conn-token requires --show or --set");
}

#[derive(serde::Serialize)]
struct CfgOllama {
    alias: String,
    model: String,
    host: String,
    port: String,
}

#[derive(serde::Serialize)]
struct CfgNotify {
    n: u8,
    kind: String,
    name: String,
    to: String,
    /// The secret (NOTIFY_<n>_SECRET) is Class A and never leaves the engine; the
    /// form shows a "configured" marker from this boolean only.
    secret_set: bool,
}

/// Config recalled for the desktop settings form. API key VALUES are intentionally
/// absent — only `*_key_set` booleans cross the boundary (§4/§6). Field names match
/// what the form reads so the desktop can forward this JSON verbatim (SOT).
#[derive(serde::Serialize)]
struct CfgJson {
    lang: String,
    primary_provider: u8,
    no_llm: bool,
    openai_model: String,
    gemini_model: String,
    claude_model: String,
    openai_key_set: bool,
    gemini_key_set: bool,
    claude_key_set: bool,
    brave_key_set: bool,
    jquants_key_set: bool,
    ollama: Vec<CfgOllama>,
    notify: Vec<CfgNotify>,
    news_enabled: bool,
    fundamental_enabled: bool,
    lan_access: bool,
    serve_port: u16,
    sec_user_agent: String,
    alias_csv: String,
    ind_ema: bool,
    ind_sma: bool,
    ind_fibonacci: bool,
    ind_stochastics: bool,
    ind_adx: bool,
    ind_roc: bool,
    ind_bollinger: bool,
    ind_vwap: bool,
    ind_ichimoku: bool,
    macd_minus_ok: bool,
    buy_rsi: f64,
    sell_rsi: f64,
    macd_diff_low: f64,
    macd_diff_mid: f64,
    macd_diff_extreme: f64,
    weight_basic: f64,
    weight_ema: f64,
    weight_sma: f64,
    weight_bollinger: f64,
    weight_roc: f64,
    weight_adx: f64,
    weight_stochastics: f64,
    weight_fibonacci: f64,
    weight_vwap: f64,
    weight_ichimoku: f64,
    ema_short_period: usize,
    ema_long_period: usize,
    sma_short_period: usize,
    sma_long_period: usize,
    adx_period: usize,
    roc_period: usize,
    stochastics_period: usize,
    bollinger_period: usize,
    bollinger_stddev_multiplier: f64,
    bb_bandwidth_squeeze_pct: f64,
    ichimoku_tenkan_period: usize,
    ichimoku_kijun_period: usize,
    vwap_period: usize,
    fibonacci_neutral_ratio: f64,
}

/// `xoksa config-json`: print the current effective config (from the single
/// canonical `xoksa.env`, via the shared loader) as JSON for the desktop settings
/// form to recall. API key values never appear — only `*_key_set` presence flags
/// (env or keychain), so the WebView can show a "configured" marker without a
/// secret crossing the browser boundary (§4/§6). Keeping all config reading here
/// makes the engine the single source; the desktop only presents this JSON.
pub fn run_config_json_cli() -> anyhow::Result<()> {
    let m = crate::bootstrap::load_env_map();
    let b = |k: &str, d: bool| {
        m.get(k)
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(d)
    };
    let f = |k: &str, d: f64| m.get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
    let u = |k: &str, d: usize| m.get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
    let s = |k: &str| m.get(k).cloned().unwrap_or_default();
    let key_set = |name: &str| crate::keystore::resolve_key_presence(name).is_found();
    // Single source of every scalar default (SOT §4.2): the engine's own defaults.
    let dc = crate::config::Config::default();

    let primary_provider = match m.get("llm_provider").map(|v| v.as_str()) {
        Some("gemini") => 2,
        Some("claude") => 3,
        Some("ollama") => 4,
        _ => 1,
    };
    let mut ollama = Vec::new();
    for n in 1..=16 {
        if let Some(alias) = m.get(&format!("OLLAMA_{n}_ALIAS")) {
            ollama.push(CfgOllama {
                alias: alias.clone(),
                model: s(&format!("OLLAMA_{n}_MODEL")),
                host: s(&format!("OLLAMA_{n}_HOST")),
                port: s(&format!("OLLAMA_{n}_PORT")),
            });
        }
    }
    let mut notify = Vec::new();
    for n in 1..=16u8 {
        if let Some(kind) = m.get(&format!("NOTIFY_{n}_KIND")) {
            notify.push(CfgNotify {
                n,
                kind: kind.clone(),
                name: s(&format!("NOTIFY_{n}_NAME")),
                to: s(&format!("NOTIFY_{n}_TO")),
                secret_set: key_set(&format!("NOTIFY_{n}_SECRET")),
            });
        }
    }
    let cfg = CfgJson {
        lang: if m.get("LANG").map(|v| v == "ja").unwrap_or(false) {
            "ja"
        } else {
            "en"
        }
        .to_string(),
        primary_provider,
        no_llm: b("NO_LLM", false),
        openai_model: s("openai_model"),
        gemini_model: s("gemini_model"),
        claude_model: s("claude_model"),
        openai_key_set: key_set("OPENAI_API_KEY"),
        gemini_key_set: key_set("GEMINI_API_KEY"),
        claude_key_set: key_set("CLAUDE_API_KEY"),
        brave_key_set: key_set("BRAVE_API_KEY"),
        jquants_key_set: key_set("JQUANTS_API_KEY"),
        ollama,
        notify,
        news_enabled: !b("NO_NEWS", false),
        fundamental_enabled: b("FUNDAMENTAL", false),
        lan_access: b("DESKTOP_LAN_ACCESS", false),
        serve_port: s("SERVE_PORT")
            .parse()
            .unwrap_or(crate::server::DEFAULT_SERVE_PORT),
        sec_user_agent: s("SEC_USER_AGENT"),
        alias_csv: s("ALIAS_CSV"),
        ind_ema: b("EMA", true),
        ind_sma: b("SMA", true),
        ind_fibonacci: b("FIBONACCI", true),
        ind_stochastics: b("STOCHASTICS", true),
        ind_adx: b("ADX", true),
        ind_roc: b("ROC", true),
        ind_bollinger: b("BOLLINGER", true),
        ind_vwap: b("VWAP", true),
        ind_ichimoku: b("ICHIMOKU", true),
        macd_minus_ok: b("MACD_MINUS_OK", false),
        buy_rsi: f("BUY_RSI", dc.buy_rsi),
        sell_rsi: f("SELL_RSI", dc.sell_rsi),
        macd_diff_low: f("MACD_DIFF_LOW", dc.macd_diff_low),
        macd_diff_mid: f("MACD_DIFF_MID", dc.macd_diff_mid),
        macd_diff_extreme: f("MACD_DIFF_EXTREME", dc.macd_diff_extreme),
        weight_basic: f("WEIGHT_BASIC", dc.weight_basic),
        weight_ema: f("WEIGHT_EMA", dc.weight_ema),
        weight_sma: f("WEIGHT_SMA", dc.weight_sma),
        weight_bollinger: f("WEIGHT_BOLLINGER", dc.weight_bollinger),
        weight_roc: f("WEIGHT_ROC", dc.weight_roc),
        weight_adx: f("WEIGHT_ADX", dc.weight_adx),
        weight_stochastics: f("WEIGHT_STOCHASTICS", dc.weight_stochastics),
        weight_fibonacci: f("WEIGHT_FIBONACCI", dc.weight_fibonacci),
        weight_vwap: f("WEIGHT_VWAP", dc.weight_vwap),
        weight_ichimoku: f("WEIGHT_ICHIMOKU", dc.weight_ichimoku),
        ema_short_period: u("EMA_SHORT_PERIOD", dc.ema_short_period),
        ema_long_period: u("EMA_LONG_PERIOD", dc.ema_long_period),
        sma_short_period: u("SMA_SHORT_PERIOD", dc.sma_short_period),
        sma_long_period: u("SMA_LONG_PERIOD", dc.sma_long_period),
        adx_period: u("ADX_PERIOD", dc.adx_period),
        roc_period: u("ROC_PERIOD", dc.roc_period),
        stochastics_period: u("STOCHASTICS_PERIOD", dc.stochastics_period),
        bollinger_period: u("BOLLINGER_PERIOD", dc.bollinger_period),
        bollinger_stddev_multiplier: f(
            "BOLLINGER_STDDEV_MULTIPLIER",
            dc.bollinger_stddev_multiplier,
        ),
        bb_bandwidth_squeeze_pct: f("BB_BANDWIDTH_SQUEEZE_PCT", dc.bb_bandwidth_squeeze_pct),
        ichimoku_tenkan_period: u("ICHIMOKU_TENKAN_PERIOD", dc.ichimoku_tenkan_period),
        ichimoku_kijun_period: u("ICHIMOKU_KIJUN_PERIOD", dc.ichimoku_kijun_period),
        vwap_period: u("VWAP_PERIOD", dc.vwap_period),
        fibonacci_neutral_ratio: f("FIBONACCI_NEUTRAL_RATIO", dc.fibonacci_neutral_ratio),
    };
    println!("{}", serde_json::to_string(&cfg)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ApplyConfigInput;

    #[test]
    fn clean_env_value_strips_zero_width_and_caps_bytes() {
        use super::clean_env_value;
        // BOM / zero-width chars removed — otherwise one pasted char makes the strict
        // env loader reject the whole file (see utils::env_line_issue).
        assert_eq!(clean_env_value("gemma4:e4b\u{FEFF}"), "gemma4:e4b");
        assert_eq!(clean_env_value("a\u{200B}b"), "ab");
        // Double-quote removed so format_env_assignment quoting stays well-formed.
        assert_eq!(clean_env_value("a\"b"), "ab");
        // Capped by bytes, never splitting a multibyte char.
        let out = clean_env_value(&"あ".repeat(500));
        assert!(out.len() <= 400);
        assert!(out.chars().all(|c| c == 'あ'));
    }

    #[test]
    fn apply_config_maps_notify_channels_and_drops_unknown_kinds() {
        // A LINE channel, a Slack channel, and one unsupported kind (dropped).
        let json = r#"{
            "lang": "en",
            "primary_provider": 1,
            "notify": [
                {"kind": "line", "name": "me", "to": "U123", "secret": "linetoken"},
                {"kind": "SLACK", "name": "team", "secret": "https://hooks.slack.com/services/x"},
                {"kind": "telegram", "name": "nope", "secret": "z"}
            ]
        }"#;
        let input: ApplyConfigInput = serde_json::from_str(json).expect("parses");
        let values = input.into_setup_values();

        assert_eq!(values.notify_channels.len(), 2, "unknown kind is dropped");
        let line = &values.notify_channels[0];
        assert_eq!(line.kind, "line"); // normalized via NotifierKind::as_str
        assert_eq!(line.name, "me");
        assert_eq!(line.to.as_deref(), Some("U123"));
        assert_eq!(line.secret.as_ref().map(|s| s.as_str()), Some("linetoken"));
        let slack = &values.notify_channels[1];
        assert_eq!(slack.kind, "slack"); // "SLACK" normalized to lowercase token
        assert_eq!(slack.to, None);
        assert_eq!(
            slack.secret.as_ref().map(|s| s.as_str()),
            Some("https://hooks.slack.com/services/x")
        );
    }
}
