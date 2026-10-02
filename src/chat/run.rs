//! The interactive chat command loop: input handling, slash-command
//! dispatch, and autoreload. Entry point re-exported from `chat`.

use super::exec::{Dispatch, ExecCtx};
use super::help::{print_help_en, print_help_ja};
use super::llm::send_chat_input_to_llm;
use super::ticker::{
    apply_ticker_add, apply_ticker_switch, autoreload_notice_lines, load_ticker_analysis,
    next_autoreload_deadline, reload_chat_tickers, TickerSwitchData,
};
use super::*;

pub async fn run_chat_loop(input: ChatLoopInput<'_>) -> Result<()> {
    let ChatLoopInput {
        config,
        base_context,
        technical_display,
        fundamental_display,
        ticker_label,
        articles,
        ticker_name_map,
        initial_data_as_of,
        default_extra_tickers,
        facts,
    } = input;

    if config.no_llm {
        eprintln!("⚠️ --no-llm is set; chat mode requires LLM access.");
        return Ok(());
    }

    // Session construction + config-derived defaults are shared with the Web UI
    // server via exec::new_configured (single source).
    let mut session = super::exec::new_configured(
        ChatSessionSeed {
            base_context: base_context.to_string(),
            technical_text: technical_display.to_string(),
            fundamental_text: fundamental_display.to_string(),
            ticker_label: ticker_label.to_string(),
            data_as_of: initial_data_as_of.to_string(),
            facts,
        },
        articles,
        config,
    );
    // `None` in a no-trace session: the file is neither read nor written.
    let history_path = history_file_path();
    let lang = config.lang.as_str();
    let mut chat_config = config.clone();
    chat_config.analysis_mode = config.chat_analysis_mode;
    let mut cumulative_tokens = ChatTokenUsage::default();

    if let Some(p) = history_path.as_deref() {
        ensure_history_permissions(p);
    }
    let mut rl = DefaultEditor::new()?;
    if let Some(p) = history_path.as_deref() {
        rl.load_history(p).ok();
    }
    // External printer: lets the autoreload timer print notices above the active
    // prompt without corrupting the line the user is editing.
    let mut ext_printer = rl.create_external_printer()?;

    // Autoreload state
    let mut autoreload_enabled = chat_config.autoreload && chat_config.analysis_mode.is_intraday();
    let mut autoreload_notice = chat_config.autoreload_notify;
    let mut next_reload_at: Option<tokio::time::Instant> = if autoreload_enabled {
        next_autoreload_deadline(chat_config.analysis_mode)
    } else {
        None
    };
    // Prices from the last completed reload; used to compute inter-reload diff in notice
    let mut autoreload_last_prices: Vec<Option<f64>> = Vec::new();

    match lang {
        "ja" => {
            println!("\n=== チャットモード ===");
            print_help_ja();
        }
        _ => {
            println!("\n=== Chat Mode ===");
            print_help_en();
        }
    }
    println!();

    // Auto-load tickers from CHAT_DEFAULT_TICKER — same logic as /ticker add
    for extra_ticker in default_extra_tickers {
        let is_empty = session.ticker_labels().len() == 1 && session.ticker_labels()[0].is_empty();
        match load_ticker_analysis(
            &chat_config,
            extra_ticker,
            ticker_name_map,
            !chat_config.no_news,
            true,
        )
        .await
        {
            Ok(ctx) => {
                if is_empty {
                    apply_ticker_switch(
                        &mut session,
                        &mut chat_config,
                        TickerSwitchData {
                            ticker: extra_ticker.clone(),
                            ticker_label: ctx.label,
                            base_context: ctx.context,
                            technical_text: ctx.technical_text,
                            fundamental_text: ctx.fundamental_text,
                            news: ctx.news_items.unwrap_or_default(),
                            data_as_of: ctx.data_as_of,
                            facts: Some(ctx.facts),
                        },
                    );
                } else {
                    apply_ticker_add(
                        &mut session,
                        ctx.label,
                        ctx.context,
                        ctx.technical_text,
                        ctx.fundamental_text,
                        ctx.news_items.unwrap_or_default(),
                        ctx.data_as_of,
                        Some(ctx.facts),
                    );
                }
            }
            Err(e) => eprintln!(
                "⚠️ CHAT_DEFAULT_TICKER: failed to load {} — {}",
                extra_ticker, e
            ),
        }
    }

    // The rustyline editor ping-pongs through spawn_blocking so the (blocking)
    // input wait can race the autoreload timer. The editor is returned with each
    // line, so history is updated back on the main task.
    let mut rl_idle: Option<DefaultEditor> = Some(rl);
    let mut readline_task: Option<
        tokio::task::JoinHandle<(DefaultEditor, Result<String, ReadlineError>)>,
    > = None;

    loop {
        // Ensure a readline is in flight (this prints the "> " prompt once).
        if readline_task.is_none() {
            let editor = rl_idle
                .take()
                .expect("editor is idle when no readline is pending");
            readline_task = Some(tokio::task::spawn_blocking(move || {
                let mut editor = editor;
                let res = editor.readline("> ");
                (editor, res)
            }));
        }

        // Wait for input, racing the autoreload timer when one is scheduled.
        // On a timer tick we reload and print the notice above the active prompt
        // via the external printer, then keep the same pending readline.
        let join_result = match (autoreload_enabled, next_reload_at) {
            (true, Some(deadline)) => {
                let task = readline_task.as_mut().expect("readline task in flight");
                tokio::select! {
                    joined = task => joined,
                    _ = tokio::time::sleep_until(deadline) => {
                        let new_prices = reload_chat_tickers(
                            &mut session,
                            &chat_config,
                            ticker_name_map,
                            false,
                            false,
                            lang,
                            &mut ChatOut::Stdout,
                            false,
                        )
                        .await;
                        if autoreload_notice {
                            for line in autoreload_notice_lines(
                                &session,
                                &new_prices,
                                &autoreload_last_prices,
                                lang,
                            ) {
                                let _ = ext_printer.print(line);
                            }
                        }
                        autoreload_last_prices = new_prices;
                        next_reload_at = next_autoreload_deadline(chat_config.analysis_mode);
                        continue;
                    }
                }
            }
            _ => {
                readline_task
                    .as_mut()
                    .expect("readline task in flight")
                    .await
            }
        };

        readline_task = None;
        let (editor, readline_res) = join_result.expect("readline task panicked");
        rl_idle = Some(editor);

        let raw_input = match readline_res {
            Ok(line) => {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    let editor = rl_idle.as_mut().expect("editor returned from readline");
                    // Arrow-key recall is in-memory and always works; writing the
                    // file is a disk trace, so a private session skips it.
                    editor.add_history_entry(&trimmed).ok();
                    if let Some(p) = history_path.as_deref() {
                        editor.save_history(p).ok();
                        ensure_history_permissions(p);
                    }
                }
                trimmed
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                match lang {
                    "ja" => println!("チャット終了。"),
                    _ => println!("Chat ended."),
                }
                break;
            }
            Err(e) => {
                eprintln!("❌ Input error: {e}");
                break;
            }
        };

        if raw_input.is_empty() {
            continue;
        }

        // Unified dispatch: EVERY command runs through `session.execute` (the single
        // CLI/Web dispatch; CLI prints via the Stdout sink). There are no CLI-only
        // command arms — `Passthrough` means "not a command", i.e. free text, which
        // falls through to the shared `send_chat_input_to_llm` below (Web uses it too).
        {
            let mut ctx = ExecCtx {
                config: &mut chat_config,
                ticker_name_map,
                cumulative_tokens: &mut cumulative_tokens,
                autoreload_enabled: &mut autoreload_enabled,
                autoreload_notice: &mut autoreload_notice,
                next_reload_at: &mut next_reload_at,
                autoreload_last_prices: &mut autoreload_last_prices,
                lang,
                interactive: true,
            };
            let mut out = ChatOut::Stdout;
            match session.execute(&raw_input, &mut ctx, &mut out).await {
                Dispatch::Exit => break,
                Dispatch::Handled => continue,
                Dispatch::Passthrough => {}
            }
        }

        // Reject unknown slash commands rather than sending them to the LLM
        if raw_input.starts_with('/') {
            match lang {
                "ja" => eprintln!("❌ 不明なコマンドです: {}", raw_input),
                _ => eprintln!("❌ Unknown command: {}", raw_input),
            }
            continue;
        }

        // Guard: reject normal input when no ticker is loaded
        {
            let empty_session =
                session.ticker_labels().len() == 1 && session.ticker_labels()[0].is_empty();
            if empty_session {
                match lang {
                    "ja" => eprintln!("❌ 銘柄がロードされていません。先に /ticker add <銘柄> を実行してください。"),
                    _ => eprintln!("❌ No ticker loaded. Use /ticker add <symbol> first."),
                }
                continue;
            }
        }

        // Enforce input length limit
        let user_input = if raw_input.chars().count() > CHAT_MAX_INPUT_CHARS {
            match lang {
                "ja" => eprintln!(
                    "⚠️ 入力が{}文字を超えています。先頭{}文字のみ使用します。",
                    CHAT_MAX_INPUT_CHARS, CHAT_MAX_INPUT_CHARS
                ),
                _ => eprintln!(
                    "⚠️ Input exceeds {} characters. Only the first {} will be used.",
                    CHAT_MAX_INPUT_CHARS, CHAT_MAX_INPUT_CHARS
                ),
            }
            raw_input
                .chars()
                .take(CHAT_MAX_INPUT_CHARS)
                .collect::<String>()
        } else {
            raw_input
        };

        // Reject injection characters consistent with the CLI -x path
        if user_input.contains([';', '|', '`']) {
            match lang {
                "ja" => eprintln!("❌ 入力に使用できない文字 (; | `) が含まれています。"),
                _ => eprintln!("❌ Input contains disallowed characters (; | `)."),
            }
            continue;
        }

        send_chat_input_to_llm(
            &mut session,
            &chat_config,
            &mut cumulative_tokens,
            user_input,
            lang,
            true, // interactive CLI: stream + Ctrl-C cancellation
            &mut ChatOut::Stdout,
        )
        .await;
    }

    Ok(())
}
