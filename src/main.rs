#![forbid(unsafe_code)]

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Web UI / HTTP server mode: `xoksa serve --ui --port 8787`.
    // Intercepted before the main analysis CLI parser so the existing flag-based
    // CLI (config::Args) and chat mode are left completely untouched.
    let raw_args: Vec<String> = std::env::args().collect();
    // SOT: resolve the single xoksa.env path once (honoring --env-file) before any
    // subcommand dispatch, so serve / apply-config / the flag CLI all read+write the
    // same file (canonical app-data path by default; see utils::env_path).
    xoksa::utils::init_env_path(xoksa::utils::env_file_flag(&raw_args).as_deref());
    if raw_args.get(1).map(String::as_str) == Some("serve") {
        return xoksa::server::run_serve_cli(&raw_args[2..]).await;
    }

    // Non-interactive subcommand for the desktop UI shell: print installed Ollama
    // model names as JSON. Intercepted here like `serve`, before the flag CLI.
    if raw_args.get(1).map(String::as_str) == Some("ollama-models") {
        return xoksa::setup::run_ollama_models_cli(&raw_args[2..]);
    }

    // Non-interactive subcommand for the desktop UI shell: write config + keychain
    // from a JSON payload on stdin (keys never touch argv). Like `serve` above.
    if raw_args.get(1).map(String::as_str) == Some("apply-config") {
        return xoksa::setup::run_apply_config_cli();
    }

    // Non-interactive subcommand for the desktop UI shell: send one test
    // notification to a channel described by a JSON payload on stdin (the secret
    // never touches argv). Like `serve` above.
    if raw_args.get(1).map(String::as_str) == Some("test-notify") {
        return xoksa::notify::run_test_notify_cli().await;
    }

    // Non-interactive subcommand for the desktop UI shell: print the CURRENT config
    // (from the single canonical xoksa.env) as JSON so the settings form can recall
    // it. API key VALUES are never emitted — only booleans for whether each key is
    // set (env or keychain). The desktop is presentation-only; all config reading
    // lives here (SOT). Like `serve` above.
    if raw_args.get(1).map(String::as_str) == Some("config-json") {
        return xoksa::setup::run_config_json_cli();
    }

    // Non-interactive subcommand for the settings app: verify or set the settings-app
    // password (value on stdin, never argv). Like `serve` above.
    if raw_args.get(1).map(String::as_str) == Some("settings-password") {
        return xoksa::setup::run_settings_password_cli(&raw_args[2..]);
    }

    // Non-interactive subcommand for the desktop: get/set its connection token (the
    // token it presents to a remote engine). Value on stdin, never argv.
    if raw_args.get(1).map(String::as_str) == Some("conn-token") {
        return xoksa::setup::run_conn_token_cli(&raw_args[2..]);
    }

    // Do not call from_filename; xoksa.env is loaded as env_map inside
    // initialize_environment_and_config() and passed to Config construction.

    // ✅ Initialization (config, CSV alias)
    let (config, ticker_opt, ticker_name_map, chat_default_extra_tickers) =
        xoksa::bootstrap::initialize_environment_and_config()?;

    // Chat mode without a ticker: skip the market data pipeline entirely.
    if config.chat && ticker_opt.is_none() {
        xoksa::chat::run_chat_loop(xoksa::chat::ChatLoopInput {
            config: &config,
            base_context: "",
            technical_display: "",
            fundamental_display: "",
            ticker_label: "",
            articles: &[],
            ticker_name_map: &ticker_name_map,
            initial_data_as_of: "",
            default_extra_tickers: &chat_default_extra_tickers,
            // No ticker loaded: nothing confirmed to verify the model against.
            facts: None,
        })
        .await?;
        return Ok(());
    }

    let ticker = ticker_opt.unwrap();

    // ✅ Market data → technical analysis pipeline (shared with the Web UI server).
    let guard = xoksa::app::build_analyzed_guard(&config, &ticker, &ticker_name_map).await?;

    // ✅ Fetch fundamental supplementary data
    let fundamental_data = if config.fundamental {
        let latest_price = Some(
            guard
                .get_latest_observed_price()
                .unwrap_or_else(|| guard.get_close()),
        );
        match xoksa::fundamental::fetch_fundamental_data(&ticker, &config, latest_price).await {
            Ok(data) => Some(data),
            Err(e) => {
                eprintln!("⚠️ Fundamental data fetch failed (technical analysis continues): {e}");
                None
            }
        }
    } else {
        None
    };

    // ✅ Screen output (read-only from struct)
    xoksa::app::run_output_pipeline(&config, &guard, fundamental_data.as_ref())?;
    // Display output + article fetch
    let news_fetcher = xoksa::news::BraveArticleFetcher {
        proxy_url: config.https_proxy.clone(),
        no_proxy: config.no_proxy.clone(),
    };
    let articles = if config.no_news {
        Vec::new()
    } else {
        xoksa::news::news_flow_controller(
            &xoksa::news::NewsSubject::from_guard(&guard),
            &config,
            &news_fetcher,
        )
        .await?
    };

    // Build analysis prompt and display text for chat base context (before LLM flow)
    let (analysis_prompt, chat_technical_display, chat_fundamental_display, chat_ticker_label) =
        if config.chat {
            let news_arg = if config.no_news {
                None
            } else {
                Some(articles.as_slice())
            };
            // Fundamental data is injected via session.fundamental_texts (build_fundamental_text)
            // with budget management; do NOT also pass it into the base context here.
            let prompt = xoksa::prompt::build_analysis_prompt(&config, &guard, news_arg, None);
            let technical = xoksa::app::collect_display_lines(&config, &guard).join("\n");
            let fundamental = fundamental_data
                .as_ref()
                .map(|d| xoksa::fundamental::render_fundamental_display(d, &config.lang).join("\n"))
                .unwrap_or_default();
            let label = format!("{} ({})", guard.get_ticker(), guard.get_name());
            (prompt, technical, fundamental, label)
        } else {
            (String::new(), String::new(), String::new(), String::new())
        };

    // Analysis prompt: built once, then two INDEPENDENT options act on it —
    // `--debug-prompt` writes the prompt to a file, `--no-llm` skips the LLM call.
    // Neither affects the other. (`--chat` runs the LLM per-turn in its own loop,
    // so single-shot dispatch is excluded here.) When writing a report (`--out`),
    // the LLM call captures the commentary for the report and echoes it.
    let mut report_commentary = String::new();
    if !config.chat {
        let news_arg = if config.no_news {
            None
        } else {
            Some(articles.as_slice())
        };
        let prompt = xoksa::prompt::build_analysis_prompt(
            &config,
            &guard,
            news_arg,
            fundamental_data.as_ref(),
        );

        // --debug-prompt: write the prompt to a file.
        if config.debug_prompt {
            xoksa::llm::save_prompt_to_file(&prompt)?;
        }

        // The confirmed values of this instrument, taken from the guard and the
        // fundamental data — not parsed back out of the prompt. The output-integrity
        // guard verifies each number the model writes against these.
        let facts =
            xoksa::integrity::facts_from_parts([xoksa::integrity::SymbolFacts::from_sources(
                &guard,
                &config,
                None,
                fundamental_data.as_ref(),
            )]);

        // --no-llm: do not contact the LLM. (--silent also skips the call.)
        if !config.no_llm && !config.silent {
            if config.out.is_some() {
                let text = xoksa::llm::send_chat_turn_facts(&config, &prompt, Some(&facts)).await?;
                println!("\n{}\n", text.trim_end());
                report_commentary = text;
            } else {
                use xoksa::traits::PromptSender;
                xoksa::llm::LlmDispatchSender
                    .send_prompt(&config, &prompt, Some(&facts))
                    .await?;
            }
        }
    }

    // Write the analysis report if requested (`--out`). Single-analysis only —
    // not for chat mode. Format is chosen by the file extension (html/md/text).
    if let Some(path) = config.out.as_deref() {
        if !config.chat {
            xoksa::report::write_report(
                path,
                &config,
                &guard,
                fundamental_data.as_ref(),
                &report_commentary,
            )?;
            println!("📝 Report written: {path}");
        }
    }

    // Chat mode
    if config.chat {
        let news_for_chat = if config.no_news {
            &[][..]
        } else {
            articles.as_slice()
        };
        let initial_data_as_of = guard
            .get_market_data_latest_time()
            .or_else(|| guard.get_analyzed_at())
            .unwrap_or("")
            .to_string();
        xoksa::chat::run_chat_loop(xoksa::chat::ChatLoopInput {
            config: &config,
            base_context: &analysis_prompt,
            technical_display: &chat_technical_display,
            fundamental_display: &chat_fundamental_display,
            ticker_label: &chat_ticker_label,
            articles: news_for_chat,
            ticker_name_map: &ticker_name_map,
            initial_data_as_of: &initial_data_as_of,
            default_extra_tickers: &chat_default_extra_tickers,
            // Confirmed values of the primary instrument, from the guard and the
            // fundamental data — not re-read out of the prompt text.
            facts: Some(xoksa::integrity::SymbolFacts::from_sources(
                &guard,
                &config,
                None,
                fundamental_data.as_ref(),
            )),
        })
        .await?;
    }

    Ok(())
}
