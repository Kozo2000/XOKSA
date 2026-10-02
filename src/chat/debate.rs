//! Debate Buffer & /criticize: keep bounded other-LLM opinions for critical
//! review against SOT. Operates on `ChatSession` via descendant-module access.

use super::llm::active_llm_model;
use super::*;

fn is_debate_storage_noise_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("The LLM response contained out-of-input")
        || trimmed.starts_with("The LLM response may contain numeric conversions")
        || trimmed.starts_with("Sentences/lines that preserved input notation")
        || trimmed.starts_with("Sentences/lines containing out-of-input")
        || trimmed.starts_with("* Sentences/lines containing out-of-input")
        || trimmed.starts_with("- All free-text was excluded due to out-of-input")
        || trimmed.starts_with("- For numbers and comparisons, refer to the XOKSA main output")
        || trimmed.starts_with("- For numbers, price levels, and fundamental values")
        || trimmed.starts_with("- For models that cannot preserve input notation")
        || trimmed == "Safe handling:"
        || trimmed.starts_with("Ollamaの回答")
        || trimmed.starts_with("※ 入力外")
        // Localized (ja) integrity notes emitted by `render_ollama_integrity_fallback`.
        || trimmed.starts_with("LLM の応答に")
        || trimmed.starts_with("入力表記を保った文/行は")
        || trimmed == "安全な取り扱い:"
        || trimmed.starts_with("- 数値・価格水準・ファンダメンタル値は")
        || trimmed.starts_with("- 入力表記を保てないモデルには")
        || trimmed.starts_with("- 入力に無い数値・比較を含むため自由記述")
        || trimmed.starts_with("- 数値・比較は XOKSA 本体の出力を正")
        || trimmed.starts_with("* 入力に無い数値・比較を含む文/行は")
}

pub(super) fn clean_debate_excerpt(response: &str) -> String {
    response
        .lines()
        .filter(|line| !is_debate_storage_noise_line(line))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn send_criticize_last_to_llm(
    session: &mut ChatSession,
    chat_config: &Config,
    cumulative_tokens: &mut ChatTokenUsage,
    parent_board_id: Option<u64>,
    lang: &str,
    interactive: bool,
    out: &mut ChatOut,
) {
    let current_model = active_llm_model(chat_config);

    // The confirmed data of the loaded instruments; the output guard verifies
    // the model's numbers against these structures, not against the prompt text.
    let facts = session.confirmed_facts();
    let prompt = session.build_criticize_prompt(
        &chat_config.llm_provider,
        &current_model,
        &chat_config.ollama_alias,
    );
    let Some(result) = super::exec::await_or_cancel(
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
            // A critique is an opinion (non-SOT), so name its author (the active LLM).
            out.line(super::llm::llm_critique_badge(chat_config, lang));
            out.line(format!("\n{}\n", response));
            cumulative_tokens.accumulate(usage);
            let model = active_llm_model(chat_config);
            session.add_board_entry(BoardEntryDraft {
                speaker_type: SpeakerType::Llm,
                role: EntryRole::Critique,
                provider: &chat_config.llm_provider,
                model: &model,
                engine_id: &chat_config.ollama_alias,
                content: &response,
                parent_entry_id: parent_board_id,
                round: 0,
            });
            session.add_turn("/criticize".to_string(), response);
        }
        Err(e) => out.err(format!("❌ {}", e)),
    }
}
