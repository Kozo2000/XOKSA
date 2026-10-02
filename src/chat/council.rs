//! Council / Board mode: multi-LLM parallel deliberation with a Facilitator.
//!
//! These functions operate on `ChatSession` (defined in the parent module);
//! as a descendant module, `council` can access the parent's private items.

use super::exec::await_or_cancel;
use super::llm::{active_llm_model, live_ollama_instances, resolve_llm_default_model};
use super::*;

fn normalize_round_digits(input: &str) -> Option<String> {
    let mut out = String::new();
    for ch in input.trim().chars() {
        match ch {
            '0'..='9' => out.push(ch),
            '０'..='９' => {
                let digit = (ch as u32).saturating_sub('０' as u32);
                out.push(char::from_digit(digit, 10)?);
            }
            _ => return None,
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

pub(super) fn parse_council_rounds(input: &str) -> Result<u32, &'static str> {
    let normalized = normalize_round_digits(input).ok_or("invalid")?;
    let rounds = normalized.parse::<u32>().map_err(|_| "invalid")?;
    if rounds == 0 {
        Err("zero")
    } else if rounds > COUNCIL_MAX_ROUNDS {
        Err("too_large")
    } else {
        Ok(rounds)
    }
}

async fn send_council_summary_to_llm(
    session: &mut ChatSession,
    chat_config: &Config,
    cumulative_tokens: &mut ChatTokenUsage,
    lang: &str,
    interactive: bool,
    out: &mut ChatOut,
) {
    // The confirmed data of the loaded instruments; the output guard verifies
    // the model's numbers against these structures, not against the prompt text.
    let facts = session.confirmed_facts();
    let Some(prompt) = session.build_council_summary_prompt() else {
        out.err(match lang {
            "ja" => "❌ Debate Buffer が空です。/forum sum には見解データが必要です。",
            _ => "❌ Debate Buffer is empty. /forum sum requires opinion data.",
        });
        return;
    };
    let Some(result) = await_or_cancel(
        crate::llm::send_chat_turn_with_usage(chat_config, &prompt, Some(&facts)),
        interactive,
        lang,
        out,
    )
    .await
    else {
        return;
    };
    match result {
        Ok((response, usage)) => {
            out.line(format!("\n{}\n", response));
            cumulative_tokens.accumulate(usage);
            let model = active_llm_model(chat_config);
            session.add_board_entry(BoardEntryDraft {
                speaker_type: SpeakerType::Chair,
                role: EntryRole::Summary,
                provider: &chat_config.llm_provider,
                model: &model,
                engine_id: &chat_config.ollama_alias,
                content: &response,
                parent_entry_id: None,
                round: 0,
            });
            session.add_turn("/forum sum".to_string(), response);
        }
        Err(e) => out.err(format!("❌ {}", e)),
    }
}

pub(super) fn resolve_facilitator_config(session: &ChatSession, chat_config: &Config) -> Config {
    let mut cfg = chat_config.clone();
    if let Some(ref f) = session.council_facilitator {
        cfg.llm_provider = f.provider.clone();
        cfg.ollama_alias = String::new(); // reset; re-set below only if ollama
        cfg.llm_model = if f.model.is_empty() {
            resolve_llm_default_model(&f.provider, chat_config)
        } else {
            f.model.clone()
        };
        if f.provider == "ollama" && !f.ollama_host.is_empty() {
            cfg.ollama_host = f.ollama_host.clone();
            cfg.ollama_port = f.ollama_port;
            cfg.ollama_alias = f.alias.clone();
        }
    }
    if cfg.llm_provider != "ollama" {
        cfg.ollama_alias.clear();
    }
    cfg
}

#[allow(clippy::too_many_arguments)]
async fn send_council_ask(
    session: &mut ChatSession,
    chat_config: &Config,
    cumulative_tokens: &mut ChatTokenUsage,
    question: &str,
    rounds: u32,
    lang: &str,
    interactive: bool,
    out: &mut ChatOut,
) {
    let empty_session = session.ticker_labels().len() == 1 && session.ticker_labels()[0].is_empty();
    if empty_session {
        out.err(match lang {
            "ja" => "❌ 銘柄がロードされていません。先に /sym add <銘柄> を実行してください。",
            _ => "❌ No ticker loaded. Use /sym add <symbol> first.",
        });
        return;
    }
    let user_question_id = session.add_board_entry(BoardEntryDraft {
        speaker_type: SpeakerType::User,
        role: EntryRole::UserQuestion,
        provider: "user",
        model: "",
        engine_id: "",
        content: question,
        parent_entry_id: None,
        round: 0,
    });
    let participants = session.council_participants.clone();

    // The confirmed data of the loaded instruments; the output guard verifies
    // the model's numbers against these structures, not against the prompt text.
    let facts = session.confirmed_facts();

    // Each round = all participants in parallel → Facilitator summary.
    // On the final round, Facilitator issues a closing summary instead of a mid-session synthesis.
    for round in 1..=rounds {
        let is_final = round == rounds;

        // --- Step 1: query all participants in parallel (count through array to MAX) ---
        let prior_facilitator = session
            .board_entries
            .iter()
            .rev()
            .find(|e| e.role == EntryRole::FacilitatorReport && e.id > user_question_id)
            .map(|e| e.content.clone());
        out.line(match lang {
            "ja" => format!(
                "\n⏳ Round {} — {} LLM に並列問い合わせ中...",
                round,
                participants.len()
            ),
            _ => format!(
                "\n⏳ Round {} — querying {} LLMs in parallel...",
                round,
                participants.len()
            ),
        });
        type BoxFut = std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = anyhow::Result<(String, crate::llm::ChatTokenUsage)>,
                    > + Send,
            >,
        >;
        let mut task_meta: Vec<(String, String, String, String)> = Vec::new();
        let mut futs: Vec<BoxFut> = Vec::new();
        for participant in &participants {
            let mut pcfg = chat_config.clone();
            pcfg.llm_provider = participant.provider.clone();
            pcfg.llm_model = if participant.model.is_empty() {
                resolve_llm_default_model(&participant.provider, chat_config)
            } else {
                participant.model.clone()
            };
            if participant.provider == "ollama" && !participant.ollama_host.is_empty() {
                pcfg.ollama_host = participant.ollama_host.clone();
                pcfg.ollama_port = participant.ollama_port;
                pcfg.ollama_alias = participant.alias.clone();
            }
            let display = if !participant.alias.is_empty() {
                format!("ollama:{}/{}", participant.alias, pcfg.llm_model)
            } else if pcfg.llm_model.is_empty() {
                participant.provider.clone()
            } else {
                format!("{}/{}", participant.provider, pcfg.llm_model)
            };
            task_meta.push((
                display,
                participant.provider.clone(),
                pcfg.llm_model.clone(),
                participant.alias.clone(),
            ));
            let p = session.build_council_prompt(
                question,
                prior_facilitator.as_deref(),
                user_question_id,
            );
            let pfacts = facts.clone();
            futs.push(Box::pin(async move {
                crate::llm::send_chat_turn_with_usage(&pcfg, &p, Some(&pfacts)).await
            }));
        }
        // All participants queried — collect results
        let Some(results) =
            await_or_cancel(futures::future::join_all(futs), interactive, lang, out).await
        else {
            return;
        };
        let mut participant_success_count: usize = 0;
        let mut participant_failed_count: usize = 0;
        for ((display, provider, model, engine_id), result) in task_meta.iter().zip(results) {
            match result {
                Ok((response, usage)) => {
                    let participant_label = match lang {
                        "ja" => format!("[R{} | 参加者: {}]", round, display),
                        _ => format!("[R{} | Participant: {}]", round, display),
                    };
                    out.line(format!("\n{}\n{}\n", participant_label, response));
                    cumulative_tokens.accumulate(usage);
                    session.add_board_entry(BoardEntryDraft {
                        speaker_type: SpeakerType::Llm,
                        role: EntryRole::CouncilProposal,
                        provider,
                        model,
                        engine_id,
                        content: &response,
                        parent_entry_id: None,
                        round,
                    });
                    participant_success_count += 1;
                }
                Err(e) => {
                    out.err(format!("❌ [R{} | {}] {}", round, display, e));
                    participant_failed_count += 1;
                }
            }
        }

        // --- Step 2: all participants done → Facilitator starts (only if at least one succeeded) ---
        if participant_success_count == 0 {
            out.err(match lang {
                "ja" => format!(
                    "⚠️ Round {} — 参加者の回答がすべて失敗したため、Facilitatorをスキップします。",
                    round
                ),
                _ => format!(
                    "⚠️ Round {} — all participant calls failed; skipping Facilitator.",
                    round
                ),
            });
            continue;
        }
        let fcfg = resolve_facilitator_config(session, chat_config);
        let f_display = if fcfg.llm_provider == "ollama" && !fcfg.ollama_alias.is_empty() {
            format!("ollama:{}/{}", fcfg.ollama_alias, fcfg.llm_model)
        } else if fcfg.llm_model.is_empty() {
            fcfg.llm_provider.clone()
        } else {
            format!("{}/{}", fcfg.llm_provider, fcfg.llm_model)
        };
        if is_final {
            out.line(match lang {
                "ja" => format!(
                    "\n⏳ Final — ファシリテータ ({}) が最終まとめを作成中...",
                    f_display
                ),
                _ => format!(
                    "\n⏳ Final — facilitator ({}) preparing closing summary...",
                    f_display
                ),
            });
        } else {
            out.line(match lang {
                "ja" => format!(
                    "\n⏳ Round {} — ファシリテータ ({}) が整理中...",
                    round, f_display
                ),
                _ => format!(
                    "\n⏳ Round {} — facilitator ({}) synthesizing...",
                    round, f_display
                ),
            });
        }
        let prompt = session.build_facilitator_prompt(
            question,
            round,
            user_question_id,
            is_final,
            participant_failed_count,
        );
        let Some(result) = await_or_cancel(
            crate::llm::send_chat_turn_with_usage(&fcfg, &prompt, Some(&facts)),
            interactive,
            lang,
            out,
        )
        .await
        else {
            return;
        };
        match result {
            Ok((response, usage)) => {
                let facilitator_label = if is_final {
                    match lang {
                        "ja" => format!("[Final | ファシリテータ: {}]", f_display),
                        _ => format!("[Final | Facilitator: {}]", f_display),
                    }
                } else {
                    match lang {
                        "ja" => format!("[R{} | ファシリテータ: {}]", round, f_display),
                        _ => format!("[R{} | Facilitator: {}]", round, f_display),
                    }
                };
                out.line(format!("\n{}\n{}\n", facilitator_label, response));
                cumulative_tokens.accumulate(usage);
                let f_engine_id = if fcfg.llm_provider == "ollama" {
                    fcfg.ollama_alias.as_str()
                } else {
                    ""
                };
                session.add_board_entry(BoardEntryDraft {
                    speaker_type: SpeakerType::Chair,
                    role: EntryRole::FacilitatorReport,
                    provider: &fcfg.llm_provider,
                    model: &fcfg.llm_model,
                    engine_id: f_engine_id,
                    content: &response,
                    parent_entry_id: None,
                    round,
                });
            }
            Err(e) => {
                let err_label = if is_final {
                    "Final".to_string()
                } else {
                    format!("R{}", round)
                };
                out.err(format!(
                    "❌ [{} | Facilitator: {}] {}",
                    err_label, f_display, e
                ));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_forum_command(
    raw_input: &str,
    session: &mut ChatSession,
    chat_config: &mut Config,
    cumulative_tokens: &mut ChatTokenUsage,
    lang: &str,
    interactive: bool,
    out: &mut ChatOut,
) {
    let arg = raw_input.strip_prefix("/forum ").unwrap_or("").trim();
    if arg.is_empty() || arg == "show" {
        if session.council_participants.is_empty() {
            out.line(match lang {
                "ja" => "参加者は未設定です。/forum set <provider,...> で設定。",
                _ => "No participants. Configure with /forum set <provider,...>.",
            });
        } else {
            out.line(match lang {
                "ja" => format!("参加者 ({}):", session.council_participants.len()),
                _ => format!("Participants ({}):", session.council_participants.len()),
            });
            for (i, p) in session.council_participants.iter().enumerate() {
                let display = if !p.alias.is_empty() {
                    format!("ollama:{}/{}", p.alias, p.model)
                } else if p.model.is_empty() {
                    p.provider.clone()
                } else {
                    format!("{}/{}", p.provider, p.model)
                };
                out.line(format!("  {}. {}", i + 1, display));
            }
        }
        let f_display = match &session.council_facilitator {
            None => match lang {
                "ja" => format!(
                    "(アクティブLLM: {}/{})",
                    chat_config.llm_provider,
                    active_llm_model(chat_config)
                ),
                _ => format!(
                    "(active LLM: {}/{})",
                    chat_config.llm_provider,
                    active_llm_model(chat_config)
                ),
            },
            Some(f) => {
                if !f.alias.is_empty() {
                    format!("ollama:{}/{}", f.alias, f.model)
                } else if f.model.is_empty() {
                    f.provider.clone()
                } else {
                    format!("{}/{}", f.provider, f.model)
                }
            }
        };
        out.line(format!("Chair: {}", f_display));
        out.line(format!(
            "keep: {} / entries: {} / board: {}",
            session.debate_mode,
            session.debate_entries.len(),
            session.board_entries.len()
        ));
        out.line(match lang {
            "ja" => "使い方: set <p,...> / ask [rN] <質問> / sum / log / chair <p> / clear",
            _ => "Usage: set <p,...> / ask [rN] <q> / sum / log / chair <p> / clear",
        });
    } else if arg.starts_with("set ") || arg == "set" {
        let spec = arg.strip_prefix("set ").unwrap_or("").trim();
        if spec.is_empty() {
            session.council_participants.clear();
            out.line(match lang {
                "ja" => "✅ 参加者をクリアしました。",
                _ => "✅ Participants cleared.",
            });
        } else {
            const VALID_PROVIDERS: &[&str] = &["openai", "gemini", "claude", "ollama"];
            let ollama_instances = live_ollama_instances();
            let mut parsed: Vec<CouncilParticipant> = Vec::new();
            let mut error: Option<String> = None;
            for part in spec.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                let (provider, model_or_alias) = if let Some((p, m)) = part.split_once(':') {
                    (p.trim(), m.trim())
                } else {
                    (part, "")
                };
                if !VALID_PROVIDERS.contains(&provider) {
                    error = Some(match lang {
                        "ja" => format!(
                            "❌ 不明なプロバイダー: {}。有効: openai, gemini, claude, ollama",
                            provider
                        ),
                        _ => format!(
                            "❌ Unknown provider: {}. Valid: openai, gemini, claude, ollama",
                            provider
                        ),
                    });
                    break;
                }
                if provider == "ollama" {
                    if model_or_alias.is_empty() {
                        error = Some(match lang {
                            "ja" => "❌ ollama はエイリアスの指定が必須です。例: ollama:gpu1"
                                .to_string(),
                            _ => "❌ ollama requires an alias. Example: ollama:gpu1".to_string(),
                        });
                        break;
                    }
                    if let Some(inst) = ollama_instances.iter().find(|i| i.alias == model_or_alias)
                    {
                        parsed.push(CouncilParticipant {
                            provider: "ollama".to_string(),
                            model: inst.model.clone(),
                            alias: model_or_alias.to_string(),
                            ollama_host: inst.host.clone(),
                            ollama_port: inst.port,
                        });
                        continue;
                    }
                    let available = if ollama_instances.is_empty() {
                        match lang {
                            "ja" => "（未設定）".to_string(),
                            _ => "(none configured)".to_string(),
                        }
                    } else {
                        ollama_instances
                            .iter()
                            .map(|i| i.alias.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    error = Some(match lang {
                        "ja" => format!(
                            "❌ ollama エイリアス「{}」が見つかりません。利用可能: {}",
                            model_or_alias, available
                        ),
                        _ => format!(
                            "❌ ollama alias \"{}\" not found. Available: {}",
                            model_or_alias, available
                        ),
                    });
                    break;
                }
                parsed.push(CouncilParticipant {
                    provider: provider.to_string(),
                    model: model_or_alias.to_string(),
                    alias: String::new(),
                    ollama_host: String::new(),
                    ollama_port: 0,
                });
            }
            if let Some(err) = error {
                out.err(err);
            } else if parsed.is_empty() {
                out.err(match lang {
                    "ja" => "❌ /forum set には provider[:model] のリストを指定してください。",
                    _ => "❌ /forum set requires a list of provider[:model] entries.",
                });
            } else {
                session.council_participants = parsed;
                out.line(match lang {
                    "ja" => format!(
                        "✅ 参加者を {} 名設定しました。",
                        session.council_participants.len()
                    ),
                    _ => format!(
                        "✅ Participants set ({}).",
                        session.council_participants.len()
                    ),
                });
            }
        }
    } else if arg == "sum" {
        let empty_session =
            session.ticker_labels().len() == 1 && session.ticker_labels()[0].is_empty();
        if empty_session {
            out.err(match lang {
                "ja" => "❌ 銘柄がロードされていません。先に /sym add <銘柄> を実行してください。",
                _ => "❌ No ticker loaded. Use /sym add <symbol> first.",
            });
        } else {
            send_council_summary_to_llm(
                session,
                chat_config,
                cumulative_tokens,
                lang,
                interactive,
                out,
            )
            .await;
        }
    } else if arg.starts_with("ask ") || arg == "ask" {
        let ask_arg = arg.strip_prefix("ask ").unwrap_or("").trim();
        // Optional leading round token: `rN` or a bare `N` (e.g. `r2` / `2`),
        // default 1. A non-numeric or out-of-range leading token is treated as
        // part of the question (forgiving — no error).
        let (rounds, question) = match ask_arg.split_once(' ') {
            Some((first, rest)) => {
                let digits = first.strip_prefix('r').unwrap_or(first);
                match parse_council_rounds(digits) {
                    Ok(n) if !digits.is_empty() => (n, rest.trim()),
                    _ => (1, ask_arg),
                }
            }
            None => (1, ask_arg),
        };
        if question.is_empty() {
            out.err(match lang {
                "ja" => "❌ /forum ask には質問を指定してください。",
                _ => "❌ /forum ask requires a question.",
            });
        } else if session.council_participants.is_empty() {
            out.err(match lang {
                "ja" => "❌ 参加者が未設定です。先に /forum set を実行してください。",
                _ => "❌ No participants. Use /forum set first.",
            });
        } else {
            send_council_ask(
                session,
                chat_config,
                cumulative_tokens,
                question,
                rounds,
                lang,
                interactive,
                out,
            )
            .await;
        }
    } else if arg.starts_with("chair") && (arg.len() == 5 || arg.as_bytes().get(5) == Some(&b' ')) {
        const VALID_PROVIDERS: &[&str] = &["openai", "gemini", "claude", "ollama"];
        let fac_arg = arg.strip_prefix("chair ").unwrap_or("").trim();
        if fac_arg.is_empty() || fac_arg == "show" {
            let display = match &session.council_facilitator {
                None => match lang {
                    "ja" => format!(
                        "(アクティブLLM: {}/{})",
                        chat_config.llm_provider,
                        active_llm_model(chat_config)
                    ),
                    _ => format!(
                        "(active LLM: {}/{})",
                        chat_config.llm_provider,
                        active_llm_model(chat_config)
                    ),
                },
                Some(f) => {
                    if !f.alias.is_empty() {
                        format!("ollama:{}/{}", f.alias, f.model)
                    } else if f.model.is_empty() {
                        f.provider.clone()
                    } else {
                        format!("{}/{}", f.provider, f.model)
                    }
                }
            };
            out.line(format!("Chair: {}", display));
        } else if fac_arg == "reset" {
            session.council_facilitator = None;
            out.line(match lang {
                "ja" => "✅ Chairをアクティブ LLM にリセットしました。",
                _ => "✅ Chair reset to active LLM.",
            });
        } else {
            let (provider, model_or_alias) = if let Some((p, m)) = fac_arg.split_once(':') {
                (p.trim(), m.trim())
            } else {
                (fac_arg, "")
            };
            if !VALID_PROVIDERS.contains(&provider) {
                out.err(match lang {
                    "ja" => format!(
                        "❌ 不明なプロバイダー: {}。有効: openai, gemini, claude, ollama",
                        provider
                    ),
                    _ => format!(
                        "❌ Unknown provider: {}. Valid: openai, gemini, claude, ollama",
                        provider
                    ),
                });
            } else if provider == "ollama" {
                if model_or_alias.is_empty() {
                    out.err(match lang {
                        "ja" => {
                            "❌ ollama はエイリアスの指定が必須です。例: /forum chair ollama:gpu1"
                        }
                        _ => "❌ ollama requires an alias. Example: /forum chair ollama:gpu1",
                    });
                } else {
                    let ollama_instances = live_ollama_instances();
                    if let Some(inst) = ollama_instances.iter().find(|i| i.alias == model_or_alias)
                    {
                        let display = format!("ollama:{}/{}", model_or_alias, inst.model);
                        session.council_facilitator = Some(CouncilParticipant {
                            provider: "ollama".to_string(),
                            model: inst.model.clone(),
                            alias: model_or_alias.to_string(),
                            ollama_host: inst.host.clone(),
                            ollama_port: inst.port,
                        });
                        out.line(match lang {
                            "ja" => format!("✅ Chairを {} に設定しました。", display),
                            _ => format!("✅ Chair set to {}.", display),
                        });
                    } else {
                        let available = if ollama_instances.is_empty() {
                            match lang {
                                "ja" => "（未設定）".to_string(),
                                _ => "(none configured)".to_string(),
                            }
                        } else {
                            ollama_instances
                                .iter()
                                .map(|i| i.alias.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        };
                        out.err(match lang {
                            "ja" => format!(
                                "❌ ollama エイリアス「{}」が見つかりません。利用可能: {}",
                                model_or_alias, available
                            ),
                            _ => format!(
                                "❌ ollama alias \"{}\" not found. Available: {}",
                                model_or_alias, available
                            ),
                        });
                    }
                }
            } else {
                let display = if model_or_alias.is_empty() {
                    provider.to_string()
                } else {
                    format!("{}/{}", provider, model_or_alias)
                };
                session.council_facilitator = Some(CouncilParticipant {
                    provider: provider.to_string(),
                    model: model_or_alias.to_string(),
                    alias: String::new(),
                    ollama_host: String::new(),
                    ollama_port: 0,
                });
                out.line(match lang {
                    "ja" => format!("✅ Chairを {} に設定しました。", display),
                    _ => format!("✅ Chair set to {}.", display),
                });
            }
        }
    } else if arg == "log" {
        // Board (meeting minutes).
        session.show_board(out, lang);
    } else if arg == "clear" {
        session.debate_entries.clear();
        session.board_entries.clear();
        session.board_entry_counter = 0;
        out.line(match lang {
            "ja" => "✅ Debate Buffer と Board をクリアしました。",
            _ => "✅ Debate Buffer and Board cleared.",
        });
    } else {
        out.err(match lang {
            "ja" => "❌ /forum: set / ask / sum / log / chair / clear",
            _ => "❌ /forum: set / ask / sum / log / chair / clear",
        });
    }
}

/// `/crit` — critically review the latest other-LLM opinion in the Debate Buffer
/// against SOT (split out of `/forum crit`). Requires keep mode summary/claims and
/// at least one entry from a different LLM.
pub(super) async fn handle_crit(
    session: &mut ChatSession,
    chat_config: &Config,
    cumulative_tokens: &mut ChatTokenUsage,
    lang: &str,
    interactive: bool,
    out: &mut ChatOut,
) {
    if session.debate_mode == "off" {
        out.err(match lang {
            "ja" => "❌ crit には /keep summary または claims が必要です。",
            _ => "❌ crit requires /keep summary or claims.",
        });
    } else if session.debate_entries.is_empty() {
        out.err(match lang {
            "ja" => "❌ Debate Buffer が空です。",
            _ => "❌ Debate Buffer is empty.",
        });
    } else {
        let cur_model = active_llm_model(chat_config);
        let has_other = session.debate_entries.iter().any(|e| {
            e.provider != chat_config.llm_provider
                || e.model != cur_model
                || e.engine_id != chat_config.ollama_alias
        });
        if !has_other {
            out.err(match lang {
                "ja" => format!(
                    "❌ Debate Buffer に別LLM（{}/{} 以外）の見解がありません。\n   別LLM/モデルで回答を追加してから crit を実行してください。",
                    chat_config.llm_provider, cur_model
                ),
                _ => format!(
                    "❌ No entries from a different LLM ({}/{}) in the Debate Buffer.\n   Add a response from another LLM/model before running crit.",
                    chat_config.llm_provider, cur_model
                ),
            });
        } else if session.ticker_labels().len() == 1 && session.ticker_labels()[0].is_empty() {
            out.err(match lang {
                "ja" => "❌ 銘柄がロードされていません。先に /sym add <銘柄> を実行してください。",
                _ => "❌ No ticker loaded. Use /sym add <symbol> first.",
            });
        } else {
            let parent_board_id = session.board_entries.last().map(|e| e.id);
            super::debate::send_criticize_last_to_llm(
                session,
                chat_config,
                cumulative_tokens,
                parent_board_id,
                lang,
                interactive,
                out,
            )
            .await;
        }
    }
}

/// `/keep off|summary|claims` — other-LLM opinion (Debate Buffer) retention mode
/// (split out of `/forum keep`). No arg shows the current buffer.
pub(super) fn handle_keep(
    session: &mut ChatSession,
    chat_config: &mut Config,
    arg: &str,
    lang: &str,
    out: &mut ChatOut,
) {
    match arg {
        "" => session.show_debate(out, lang),
        "off" | "summary" | "claims" => {
            session.set_debate_mode(arg);
            chat_config.debate = arg.to_string();
            if arg == "off" {
                session.debate_entries.clear();
            }
            out.line(match lang {
                "ja" => format!("✅ keep を {} に変更しました。", arg),
                _ => format!("✅ keep set to {}.", arg),
            });
        }
        _ => out.err(match lang {
            "ja" => "❌ /keep には off / summary / claims を指定してください。",
            _ => "❌ /keep accepts off, summary, or claims.",
        }),
    }
}
