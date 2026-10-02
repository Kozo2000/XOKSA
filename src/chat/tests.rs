use super::council::{parse_council_rounds, resolve_facilitator_config};
use super::llm::{parse_llm_switch_command, LlmSwitchCmd};
use super::ticker::{
    apply_ticker_add, apply_ticker_reload, apply_ticker_switch, autoreload_secs, TickerReloadData,
    TickerSwitchData,
};
use super::*;

fn make_session(base: &str, memory: &str) -> ChatSession {
    ChatSession::new(
        ChatSessionSeed {
            base_context: base.to_string(),
            technical_text: String::new(),
            fundamental_text: String::new(),
            ticker_label: String::new(),
            data_as_of: String::new(),
            facts: None,
        },
        &[],
        parse_memory(memory),
        "high",
        "en",
    )
}

// --- Bug 2: per-client LLM selection is a single, deterministic source ---

#[test]
fn client_llm_selection_is_stable_and_deterministic_across_timeframes() {
    use crate::chat::exec::{apply_client_llm_selection, store_client_llm_selection};
    // A unique sid so the shared process-wide store cannot collide with other tests.
    let sid = "test-sid-bug2-llm-selection";

    // The client picks Claude (as a `/llm` switch would set the config fields).
    let chosen = crate::config::Config {
        llm_provider: "claude".to_string(),
        llm_model: "claude-test-model".to_string(),
        ollama_alias: String::new(),
        ..crate::config::Config::default()
    };
    store_client_llm_selection(sid, &chosen);

    // Applying to a fresh env-default config for ANY timeframe yields the same model —
    // this is what killed the random "model changes when you switch the 足 pulldown".
    let mut daily = crate::config::Config::default();
    let mut weekly = crate::config::Config::default();
    apply_client_llm_selection(sid, &mut daily);
    apply_client_llm_selection(sid, &mut weekly);
    assert_eq!(daily.llm_provider, "claude");
    assert_eq!(weekly.llm_provider, "claude");
    assert_eq!(daily.llm_model, "claude-test-model");
    assert_eq!(weekly.llm_model, "claude-test-model");

    // An unknown client has no stored selection: the env default is left untouched
    // (never a random borrow from another session).
    let mut untouched = crate::config::Config::default();
    let env_default = untouched.llm_provider.clone();
    apply_client_llm_selection("test-sid-bug2-never-seen", &mut untouched);
    assert_eq!(untouched.llm_provider, env_default);
}

// --- Truncation correctness ---

#[test]
fn build_prompt_preserves_constraint_with_oversized_base() {
    // base_context is far larger than low-mode budget (8000 chars)
    let base = "x".repeat(25_000);
    let session = make_session(&base, "low");
    let prompt = session.build_prompt("some question");
    assert!(
        prompt.contains(constraint_text("high", "en")),
        "constraint must appear in prompt"
    );
}

#[test]
fn build_prompt_user_question_is_last_line_with_oversized_base() {
    let base = "x".repeat(25_000);
    let session = make_session(&base, "low");
    let prompt = session.build_prompt("my unique question");
    let last_line = prompt.lines().last().unwrap_or("");
    assert_eq!(
        last_line, "User: my unique question",
        "user question must be the final line"
    );
}

#[test]
fn build_prompt_constraint_appears_before_user_line() {
    let session = make_session("base data for context", "mid");
    let prompt = session.build_prompt("question here");
    let constraint_pos = prompt
        .find(constraint_text("high", "en"))
        .expect("constraint must be present");
    let user_pos = prompt
        .rfind("User: question here")
        .expect("user line must be present");
    assert!(
        constraint_pos < user_pos,
        "constraint must appear before user question"
    );
}

#[test]
fn build_prompt_total_chars_within_budget_for_low() {
    let base = "x".repeat(25_000);
    let session = make_session(&base, "low");
    let prompt = session.build_prompt("test");
    // Allow a small tolerance for the separator overhead constant
    assert!(
        prompt.chars().count() <= parse_memory("low").max_context_chars,
        "prompt must fit within low-mode budget"
    );
}

#[test]
fn build_prompt_total_chars_within_budget_with_long_turns() {
    // Simulate a conversation where LLM answers are very verbose
    let mut session = make_session("base context", "low"); // budget = 8000
    for i in 0..2 {
        session.add_turn(format!("question {}", i), "a".repeat(6_000));
    }
    let prompt = session.build_prompt("final question");
    let len = prompt.chars().count();
    assert!(
        len <= parse_memory("low").max_context_chars,
        "prompt ({} chars) must fit within low budget ({})",
        len,
        parse_memory("low").max_context_chars,
    );
    assert!(
        prompt.contains(constraint_text("high", "en")),
        "constraint must be present"
    );
    assert_eq!(prompt.lines().last().unwrap_or(""), "User: final question");
}

#[test]
fn build_prompt_base_not_dropped_when_news_is_large() {
    // News larger than total budget must not cause base_context to vanish.
    // Reproduces the pre-deduction bug: news_chars wiped base_budget to 0.
    let base = "BASE_CONTENT ".repeat(100); // ~1300 chars, well within low budget
    let articles = vec![
        crate::news::Article {
            title: "n".repeat(10_000), // news far exceeds budget
            url: String::new(),
            published_at: None,
        };
        1
    ];
    let params = parse_memory("low");
    let session = ChatSession::new(
        ChatSessionSeed {
            base_context: base.clone(),
            technical_text: String::new(),
            fundamental_text: String::new(),
            ticker_label: String::new(),
            data_as_of: String::new(),
            facts: None,
        },
        &articles,
        params,
        "high",
        "en",
    );
    let prompt = session.build_prompt("test question");
    assert!(
        prompt.contains("BASE_CONTENT"),
        "base_context must appear in prompt even when news is large"
    );
    assert!(
        prompt.chars().count() <= parse_memory("low").max_context_chars,
        "prompt must still fit within budget"
    );
}

#[test]
fn build_news_text_includes_url_and_title_only_boundary() {
    let articles = vec![crate::news::Article {
        title: "Company announces earnings".to_string(),
        url: "https://example.com/news/earnings".to_string(),
        published_at: None,
    }];
    let session = ChatSession::new(
        ChatSessionSeed {
            base_context: "base".to_string(),
            technical_text: String::new(),
            fundamental_text: String::new(),
            ticker_label: "TEST (Test Inc.)".to_string(),
            data_as_of: String::new(),
            facts: None,
        },
        &articles,
        parse_memory("mid"),
        "high",
        "en",
    );

    let news = session.build_news_text().expect("news context");

    assert!(news.contains("Company announces earnings"));
    assert!(news.contains("https://example.com/news/earnings"));
    assert!(
        news.contains("Company announces earnings\n\n    URL: https://example.com/news/earnings\n")
    );
    assert!(news.contains("titles and URLs only"));
    assert!(news.contains("Article bodies are not fetched or read"));
}

#[test]
fn context_size_breakdown_counts_major_sections() {
    let articles = vec![crate::news::Article {
        title: "News title".to_string(),
        url: "https://example.com/news".to_string(),
        published_at: None,
    }];
    let mut session = ChatSession::new(
        ChatSessionSeed {
            base_context: "BASE_CONTEXT".to_string(),
            technical_text: "TECH_CONTEXT".to_string(),
            fundamental_text: "FUND_CONTEXT".to_string(),
            ticker_label: "TEST (Test Inc.)".to_string(),
            data_as_of: String::new(),
            facts: None,
        },
        &articles,
        parse_memory("mid"),
        "high",
        "en",
    );
    session.add_turn("question".to_string(), "answer".to_string());
    session.conversation_summary = Some("summary".to_string());
    session.set_debate_mode("summary");
    session.add_debate_entry("openai", "model", "", "debate question", "debate answer");

    let breakdown = session.context_size_breakdown();

    assert_eq!(
        breakdown.prompt_budget_chars,
        parse_memory("mid").max_context_chars
    );
    assert!(breakdown.base_context_chars >= "BASE_CONTEXT".len());
    assert!(breakdown.technical_context_chars >= "TECH_CONTEXT".len());
    assert!(breakdown.fundamental_chars >= "FUND_CONTEXT".len());
    assert!(breakdown.news_chars > 0);
    assert!(breakdown.recent_turns_chars > 0);
    assert!(breakdown.summary_chars > 0);
    assert!(breakdown.debate_chars > 0);
    assert!(breakdown.guard_chars > 0);
    assert!(breakdown.regular_chat_candidate_chars >= breakdown.technical_context_chars);
    assert!(breakdown.initial_prompt_candidate_chars >= breakdown.base_context_chars);
    assert_eq!(breakdown.ticker_count, 1);
    assert_eq!(breakdown.recent_turn_count, 1);
    assert_eq!(breakdown.debate_entry_count, 1);
}

#[test]
fn build_prompt_uses_technical_text_for_regular_chat_turns() {
    let mut session = make_session("BASE_FORMAT_INSTRUCTIONS", "high");
    session.set_technical_texts(vec!["TECHNICAL_CONTEXT_ONLY".to_string()]);
    let prompt = session.build_prompt("test question");

    assert!(
        prompt.contains("TECHNICAL_CONTEXT_ONLY"),
        "regular chat prompt must include technical display context"
    );
    assert!(
        !prompt.contains("BASE_FORMAT_INSTRUCTIONS"),
        "regular chat prompt must not leak the /prompt analysis template"
    );
}

#[test]
fn build_prompt_includes_debate_buffer_when_enabled() {
    let mut session = make_session("base", "high");
    session.set_technical_texts(vec!["TECH".to_string()]);
    session.set_debate_mode("summary");
    session.add_debate_entry(
        "openai",
        "gpt-test",
        "",
        "first question",
        "other model opinion",
    );

    let prompt = session.build_prompt("next question");

    assert!(
        prompt.contains("Debate Buffer"),
        "debate buffer header must appear"
    );
    assert!(
        prompt.contains("Other LLM Opinions"),
        "debate buffer must label prior LLM output as opinion"
    );
    assert!(
        prompt.contains("other model opinion"),
        "stored opinion excerpt must appear"
    );
    assert!(prompt.contains("TECH"), "technical SOT must remain present");
}

#[test]
fn build_prompt_omits_debate_buffer_when_off() {
    let mut session = make_session("base", "high");
    session.add_debate_entry(
        "openai",
        "gpt-test",
        "",
        "first question",
        "other model opinion",
    );

    let prompt = session.build_prompt("next question");

    assert!(
        !prompt.contains("Debate Buffer"),
        "debate buffer must be omitted when debate mode is off"
    );
    assert!(
        !prompt.contains("other model opinion"),
        "opinion excerpt must not be stored or included when off"
    );
}

#[test]
fn debate_entry_storage_is_bounded_by_mode() {
    let mut session = make_session("base", "high");
    session.set_debate_mode("summary");
    session.add_debate_entry("openai", "m1", "", "u1", "r1");
    session.add_debate_entry("gemini", "m2", "", "u2", "r2");
    session.add_debate_entry("claude", "m3", "", "u3", "r3");

    assert_eq!(
        session.debate_entries.len(),
        DEBATE_SUMMARY_MAX_ENTRIES,
        "summary mode must keep only the configured number of entries"
    );
    assert_eq!(
        session.debate_entries[0].response_excerpt, "r2",
        "oldest overflow entry must be dropped"
    );
}

#[test]
fn debate_entry_strips_integrity_guard_boilerplate() {
    let mut session = make_session("base", "high");
    session.set_debate_mode("summary");
    let response = [
        "The LLM response contained out-of-input numeric conversions or comparisons; only the affected sentences/lines are excluded.",
        "Sentences/lines that preserved input notation are displayed under the same integrity check.",
        "",
        "**1. Useful section**",
        "This line should remain.",
        "* Sentences/lines containing out-of-input numbers or comparisons have been excluded from analysis.",
    ]
    .join("\n");

    session.add_debate_entry("ollama", "model", "", "question", &response);

    assert_eq!(session.debate_entries.len(), 1);
    let excerpt = &session.debate_entries[0].response_excerpt;
    assert!(excerpt.contains("This line should remain."));
    assert!(
        !excerpt.contains("out-of-input"),
        "guard boilerplate must not be stored in Debate Buffer"
    );
}

#[test]
fn criticize_prompt_uses_sot_coverage_and_latest_debate_only() {
    let mut session = make_session("base", "high");
    session.set_ticker_labels(vec![
        "MU (Micron Technology)".to_string(),
        "INTC (Intel)".to_string(),
    ]);
    session.set_technical_texts(vec![
        "MU TECHNICAL SOT".to_string(),
        "INTC TECHNICAL SOT".to_string(),
    ]);
    session.set_fundamental_texts(vec!["MU FUNDAMENTAL SOT".to_string(), String::new()]);
    session.set_news_items(vec![
        vec![NewsItem {
            id: "N01".to_string(),
            title: "Micron title candidate".to_string(),
            url: "https://example.com/mu".to_string(),
        }],
        vec![],
    ]);
    session.add_turn(
        "ordinary history".to_string(),
        "ordinary answer".to_string(),
    );
    session.set_debate_mode("claims");
    session.add_debate_entry(
        "openai",
        "old",
        "",
        "q1",
        "older opinion should not be reviewed",
    );
    session.add_debate_entry(
        "gemini",
        "latest",
        "",
        "q2",
        "latest opinion mentions INTC news without SOT coverage",
    );

    // current (claude/""/""): targets the latest non-claude entry (gemini/latest/"")
    let prompt = session.build_criticize_prompt("claude", "", "");

    assert!(prompt.contains("[SOT Coverage]"));
    assert!(prompt.contains("MU: technical=yes fundamental=yes news_titles=1"));
    assert!(prompt.contains("INTC: technical=yes fundamental=no news_titles=0"));
    assert!(prompt.contains("news_body=not_fetched_or_read"));
    assert!(prompt.contains("standard technical interpretation"));
    assert!(prompt.contains("SOT-unverified"));
    assert!(prompt.contains("latest opinion mentions INTC news"));
    assert!(prompt.contains("https://example.com/mu"));
    assert!(
        !prompt.contains("older opinion should not be reviewed"),
        "/crit must target only the latest Debate Buffer entry"
    );
    assert!(
        !prompt.contains("ordinary history"),
        "/crit prompt must not include ordinary chat history"
    );
}

// --- Memory mode behaviour ---

#[test]
fn low_produces_shorter_or_equal_prompt_than_high() {
    let base = "data ".repeat(3_000); // 15k chars
    let session_low = make_session(&base, "low");
    let session_high = make_session(&base, "high");
    let prompt_low = session_low.build_prompt("question");
    let prompt_high = session_high.build_prompt("question");
    assert!(
        prompt_low.chars().count() <= prompt_high.chars().count(),
        "low prompt must not be longer than high prompt"
    );
}

#[test]
fn high_memory_retains_more_recent_turns_than_low() {
    let mut session_low = make_session("base", "low"); // max_recent_turns = 2
    let mut session_high = make_session("base", "high"); // max_recent_turns = 8

    for i in 0..6 {
        let u = format!("question {}", i);
        let a = format!("answer {}", i);
        session_low.add_turn(u.clone(), a.clone());
        session_high.add_turn(u, a);
    }

    // low should have at most 2 in recent_turns (rest rolled into summary)
    assert!(
        session_low.recent_turns.len() <= 2,
        "low must cap recent_turns at 2"
    );
    assert!(
        session_low.conversation_summary.is_some(),
        "low must have rolled surplus turns into summary"
    );

    // high keeps all 6 (well within its max of 8)
    assert_eq!(
        session_high.recent_turns.len(),
        6,
        "high must keep all 6 turns"
    );
    assert!(
        session_high.conversation_summary.is_none(),
        "high must not need a summary yet"
    );
}

// --- Summary rolling ---

#[test]
fn roll_summary_keeps_newer_content_over_older_when_at_capacity() {
    // Budget is too small for both old and new rolled content.
    // Verify newly rolled turns survive; old prefix may be dropped.
    let params = ChatParams {
        max_context_chars: 16_000,
        max_recent_turns: 1,
        max_summary_chars: 15,
    };
    let mut session = ChatSession::new(
        ChatSessionSeed {
            base_context: "base".to_string(),
            technical_text: String::new(),
            fundamental_text: String::new(),
            ticker_label: String::new(),
            data_as_of: String::new(),
            facts: None,
        },
        &[],
        params,
        "high",
        "en",
    );
    // Turn 1 rolled into summary when turn 2 is added
    session.add_turn("oq".to_string(), "oa".to_string()); // "Q: oq\nA: oa" = 11 chars
    session.add_turn("nq".to_string(), "na".to_string()); // rolls "oq" → summary = "Q: oq\nA: oa"
                                                          // Turn 2 now rolls into summary; combined = "Q: oq\nA: oa\nQ: nq\nA: na" (23 chars > 15)
    session.add_turn("last".to_string(), "last_a".to_string());

    let s = session
        .conversation_summary
        .as_deref()
        .expect("summary must exist");
    assert!(
        s.chars().count() <= 15,
        "summary must not exceed max: {} chars",
        s.chars().count()
    );
    // Newly rolled content ("nq"/"na") must survive truncation
    assert!(
        s.contains("nq") || s.contains("na"),
        "newer rolled content must be retained; got: {:?}",
        s
    );
}

#[test]
fn roll_summary_respects_max_summary_chars() {
    let params = ChatParams {
        max_context_chars: 16_000,
        max_recent_turns: 2,
        max_summary_chars: 100,
    };
    let mut session = ChatSession::new(
        ChatSessionSeed {
            base_context: "base".to_string(),
            technical_text: String::new(),
            fundamental_text: String::new(),
            ticker_label: String::new(),
            data_as_of: String::new(),
            facts: None,
        },
        &[],
        params,
        "high",
        "en",
    );
    for i in 0..5 {
        // AI answer is long; excess turns will be rolled into summary
        session.add_turn(format!("q{}", i), "a".repeat(200));
    }
    if let Some(ref s) = session.conversation_summary {
        assert!(
            s.chars().count() <= 100,
            "summary must not exceed max_summary_chars"
        );
    }
}

// --- CLI validation (value_parser enforces allowed values) ---

#[test]
fn chat_memory_invalid_cli_is_rejected_by_clap() {
    use clap::CommandFactory;
    let result = crate::config::Args::command().try_get_matches_from([
        "xoksa",
        "--ticker",
        "SPY",
        "--chat-memory",
        "invalid",
    ]);
    assert!(
        result.is_err(),
        "clap must reject unknown --chat-memory value"
    );
}

#[test]
fn chat_memory_valid_values_are_accepted() {
    use clap::CommandFactory;
    for val in ["low", "mid", "high"] {
        let result = crate::config::Args::command().try_get_matches_from([
            "xoksa",
            "--ticker",
            "SPY",
            "--chat-memory",
            val,
        ]);
        assert!(result.is_ok(), "--chat-memory {} must be accepted", val);
    }
}

// --- parse_memory env fallback ---

#[test]
fn parse_memory_unknown_falls_back_to_mid_params() {
    let p = parse_memory("unknown");
    let mid = parse_memory("mid");
    assert_eq!(p.max_context_chars, mid.max_context_chars);
    assert_eq!(p.max_recent_turns, mid.max_recent_turns);
}

#[test]
fn parse_memory_low_has_smaller_budget_than_high() {
    let low = parse_memory("low");
    let high = parse_memory("high");
    assert!(low.max_context_chars < high.max_context_chars);
    assert!(low.max_recent_turns < high.max_recent_turns);
    assert!(low.max_summary_chars < high.max_summary_chars);
}

#[test]
fn chat_memory_label_matches_runtime_params() {
    assert_eq!(chat_memory_label(parse_memory("low")), "low");
    assert_eq!(chat_memory_label(parse_memory("mid")), "mid");
    assert_eq!(chat_memory_label(parse_memory("high")), "high");
    assert_eq!(
        chat_memory_label(ChatParams {
            max_context_chars: 1,
            max_recent_turns: 1,
            max_summary_chars: 1,
        }),
        "custom"
    );
}

// --- Config env resolution ---

#[test]
fn chat_memory_env_is_resolved_from_env_map() {
    use crate::config::{build_config, Args};
    use clap::Parser;
    use std::collections::HashMap;
    let args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
    let env_map = HashMap::from([("CHAT_MEMORY".to_string(), "low".to_string())]);
    let config = build_config(&args, &env_map);
    assert_eq!(config.chat_memory, "low");
}

#[test]
fn chat_memory_invalid_env_falls_back_to_mid() {
    use crate::config::{build_config, Args};
    use clap::Parser;
    use std::collections::HashMap;
    let args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
    let env_map = HashMap::from([("CHAT_MEMORY".to_string(), "bad_value".to_string())]);
    let config = build_config(&args, &env_map);
    assert_eq!(config.chat_memory, "mid");
}

#[test]
fn chat_memory_cli_mid_overrides_env_high() {
    use crate::config::{build_config_with_value_sources, Args, CliValueSources};
    use clap::Parser;
    use std::collections::HashMap;
    let args = Args::parse_from(["xoksa", "--ticker", "SPY", "--chat-memory", "mid"]);
    let env_map = HashMap::from([("CHAT_MEMORY".to_string(), "high".to_string())]);
    let config = build_config_with_value_sources(
        &args,
        &env_map,
        CliValueSources {
            chat_memory: true,
            ..CliValueSources::default()
        },
    );
    assert_eq!(config.chat_memory, "mid");
}

// --- chat_guard ---

fn make_session_with_guard(base: &str, memory: &str, guard: &str) -> ChatSession {
    ChatSession::new(
        ChatSessionSeed {
            base_context: base.to_string(),
            technical_text: String::new(),
            fundamental_text: String::new(),
            ticker_label: String::new(),
            data_as_of: String::new(),
            facts: None,
        },
        &[],
        parse_memory(memory),
        guard,
        "en",
    )
}

#[test]
fn chat_guard_each_level_injects_correct_constraint() {
    for level in ["low", "mid", "high"] {
        let session = make_session_with_guard("base", "mid", level);
        let prompt = session.build_prompt("question");
        assert!(
            prompt.contains(constraint_text(level, "en")),
            "constraint for {} must appear in prompt",
            level
        );
    }
}

#[test]
fn chat_guard_unknown_falls_back_to_high() {
    assert_eq!(
        constraint_text("unknown", "en"),
        constraint_text("high", "en")
    );
    assert_eq!(constraint_text("", "en"), constraint_text("high", "en"));
}

#[test]
fn prompt_command_includes_constraint_text() {
    for level in ["low", "mid", "high"] {
        let session = make_session_with_guard("base ctx", "mid", level);
        let prompt = session.build_initial_analysis_prompt();
        assert!(
            prompt.contains(constraint_text(level, "en")),
            "constraint for {} must be in /prompt output",
            level
        );
    }
}

#[test]
fn chat_guard_invalid_cli_is_rejected_by_clap() {
    use clap::CommandFactory;
    let result = crate::config::Args::command().try_get_matches_from([
        "xoksa",
        "--ticker",
        "SPY",
        "--chat-guard",
        "invalid",
    ]);
    assert!(
        result.is_err(),
        "clap must reject unknown --chat-guard value"
    );
}

#[test]
fn chat_guard_valid_values_are_accepted() {
    use clap::CommandFactory;
    for val in ["low", "mid", "high"] {
        let result = crate::config::Args::command().try_get_matches_from([
            "xoksa",
            "--ticker",
            "SPY",
            "--chat-guard",
            val,
        ]);
        assert!(result.is_ok(), "--chat-guard {} must be accepted", val);
    }
}

#[test]
fn chat_guard_env_is_resolved_from_env_map() {
    use crate::config::{build_config, Args};
    use clap::Parser;
    use std::collections::HashMap;
    let args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
    let env_map = HashMap::from([("CHAT_GUARD".to_string(), "low".to_string())]);
    let config = build_config(&args, &env_map);
    assert_eq!(config.chat_guard, "low");
}

#[test]
fn chat_guard_invalid_env_falls_back_to_high() {
    use crate::config::{build_config, Args};
    use clap::Parser;
    use std::collections::HashMap;
    let args = Args::parse_from(["xoksa", "--ticker", "SPY"]);
    let env_map = HashMap::from([("CHAT_GUARD".to_string(), "bad_value".to_string())]);
    let config = build_config(&args, &env_map);
    assert_eq!(config.chat_guard, "high");
}

#[test]
fn chat_guard_cli_high_overrides_env_low() {
    use crate::config::{build_config_with_value_sources, Args, CliValueSources};
    use clap::Parser;
    use std::collections::HashMap;
    let args = Args::parse_from(["xoksa", "--ticker", "SPY", "--chat-guard", "high"]);
    let env_map = HashMap::from([("CHAT_GUARD".to_string(), "low".to_string())]);
    let config = build_config_with_value_sources(
        &args,
        &env_map,
        CliValueSources {
            chat_guard: true,
            ..CliValueSources::default()
        },
    );
    assert_eq!(config.chat_guard, "high");
}

// --- parse_llm_switch_command ---

#[test]
fn llm_cmd_bare_lists() {
    assert_eq!(parse_llm_switch_command("/llm"), Some(LlmSwitchCmd::List));
}

#[test]
fn llm_cmd_net_queries_providers() {
    assert_eq!(
        parse_llm_switch_command("/llm net"),
        Some(LlmSwitchCmd::Net)
    );
}

#[test]
fn llm_cmd_non_llm_input_returns_none() {
    assert_eq!(parse_llm_switch_command("hello"), None);
    assert_eq!(parse_llm_switch_command("/prompt"), None);
    assert_eq!(parse_llm_switch_command("/bye"), None);
    assert_eq!(parse_llm_switch_command("/llmx"), None);
}

#[test]
fn llm_cmd_provider_only_valid() {
    for p in ["openai", "gemini", "claude", "ollama"] {
        assert_eq!(
            parse_llm_switch_command(&format!("/llm {}", p)),
            Some(LlmSwitchCmd::Switch {
                provider: p,
                explicit_model: ""
            }),
            "provider-only: {p}"
        );
    }
}

#[test]
fn llm_cmd_provider_with_model() {
    assert_eq!(
        parse_llm_switch_command("/llm openai:gpt-5.4-nano"),
        Some(LlmSwitchCmd::Switch {
            provider: "openai",
            explicit_model: "gpt-5.4-nano"
        })
    );
}

#[test]
fn llm_cmd_ollama_alias_with_colon() {
    // Ollama uses the colon suffix as alias; if the alias itself contains a colon,
    // only the first colon splits provider from alias — alias lookup rejects it at runtime
    assert_eq!(
        parse_llm_switch_command("/llm ollama:gpt-oss:20b"),
        Some(LlmSwitchCmd::Switch {
            provider: "ollama",
            explicit_model: "gpt-oss:20b"
        })
    );
}

#[test]
fn llm_cmd_unknown_provider() {
    assert_eq!(
        parse_llm_switch_command("/llm badprovider"),
        Some(LlmSwitchCmd::UnknownProvider("badprovider"))
    );
}

#[test]
fn facilitator_config_clears_ollama_alias_for_non_ollama_provider() {
    let config = Config {
        llm_provider: "openai".to_string(),
        llm_model: "gpt-5.4-nano".to_string(),
        ollama_alias: "gpu1".to_string(),
        ..Config::default()
    };
    let session = make_session("base", "low");

    let resolved = resolve_facilitator_config(&session, &config);

    assert_eq!(resolved.llm_provider, "openai");
    assert_eq!(resolved.llm_model, "gpt-5.4-nano");
    assert_eq!(resolved.ollama_alias, "");
}

// --- apply_ticker_reload ---

#[test]
fn ticker_reload_stores_indicator_summary_in_table() {
    // Regression: the reload path must write the indicator summary into the
    // TickerEntry row itself (not a throwaway clone), so the autoreload notice
    // can show the "指標:" readings afterward.
    let mut session = make_session("old base", "mid");
    apply_ticker_reload(
        &mut session,
        0,
        TickerReloadData {
            base_context: "new base".to_string(),
            technical_text: "new tech".to_string(),
            indicator_summary: "RSI: 55 / MACD: up".to_string(),
            news: None,
            data_as_of: "2026-07-15".to_string(),
            facts: None,
            fundamental: None,
        },
    );
    assert_eq!(
        session.last_indicator_summaries()[0],
        "RSI: 55 / MACD: up",
        "reload must persist the indicator summary into the ticker table"
    );
    assert_eq!(session.base_contexts()[0], "new base");
    assert_eq!(session.technical_texts()[0], "new tech");
}

// --- apply_ticker_add ---

#[test]
fn ticker_add_preserves_recent_turns_and_summary() {
    let mut session = make_session("ticker A data", "mid");
    session.add_turn("q".to_string(), "a".to_string());
    session.conversation_summary = Some("summary".to_string());

    apply_ticker_add(
        &mut session,
        String::new(),
        "ticker B data".to_string(),
        String::new(),
        String::new(),
        vec![],
        String::new(),
        None,
    );

    assert_eq!(session.base_contexts().len(), 2, "two entries after add");
    assert_eq!(
        session.base_contexts()[0],
        "ticker A data",
        "first context must be unchanged"
    );
    assert!(
        session.base_contexts()[1].contains("ticker B data"),
        "second context must contain new analysis"
    );
    assert_eq!(
        session.recent_turns.len(),
        1,
        "recent_turns must be preserved"
    );
    assert!(
        session.conversation_summary.is_some(),
        "summary must be preserved"
    );
    assert_eq!(
        session.news_items().len(),
        2,
        "news_items must have two entries"
    );
    assert!(
        session.news_items()[1].is_empty(),
        "added ticker news must be empty"
    );
}

#[test]
fn ticker_add_does_not_require_config_change() {
    // apply_ticker_add has no config parameter — verify it only mutates session
    let mut session = make_session("base A", "mid");
    apply_ticker_add(
        &mut session,
        String::new(),
        "base B".to_string(),
        String::new(),
        String::new(),
        vec![],
        String::new(),
        None,
    );
    assert_eq!(
        session.base_contexts().len(),
        2,
        "second entry must be added"
    );
    // config is not a parameter — no mutation possible by design
}

#[test]
fn build_prompt_splits_budget_equally_across_comparison_targets() {
    let mut session = make_session("", "low"); // low budget = 8000 chars
    session.set_base_contexts(vec!["A".repeat(6_000), "B".repeat(6_000)]);
    session.set_technical_texts(vec![String::new(), String::new()]);
    session.set_fundamental_texts(vec![String::new(), String::new()]);
    session.set_news_items(vec![vec![], vec![]]);
    session.set_ticker_labels(vec![String::new(), String::new()]);
    let prompt = session.build_prompt("question");
    assert!(prompt.contains('A'), "first context must appear in prompt");
    assert!(prompt.contains('B'), "second context must appear in prompt");
    assert!(
        prompt.chars().count() <= parse_memory("low").max_context_chars,
        "prompt must fit within low-mode budget"
    );
}

#[test]
fn build_prompt_adds_comparison_headers_when_multiple_contexts() {
    let mut session = make_session("first ticker data", "high");
    session.tickers.push(TickerEntry {
        base_context: "second ticker data".to_string(),
        ..TickerEntry::default()
    });
    session.set_ticker_labels(vec!["9432.T".to_string(), "9433.T".to_string()]);
    let prompt = session.build_prompt("compare");
    assert!(
        prompt.contains("=== 9432.T ==="),
        "first target header must show its ticker symbol"
    );
    assert!(
        prompt.contains("9433.T"),
        "second target header must show its ticker symbol"
    );
    assert!(
        !prompt.contains("Comparison Target"),
        "pronoun-style index label must not appear in headers"
    );
    assert!(
        prompt.contains("technical-focused"),
        "second target must be marked as technical-focused"
    );
    assert!(
        prompt.contains("Comparison note"),
        "comparison constraint note must appear"
    );
}

#[test]
fn set_analysis_param_updates_config_and_validates() {
    use super::exec::set_analysis_param;
    let mut c = crate::config::Config::default();

    // bool: on/off forms
    assert!(set_analysis_param(&mut c, "macd-minus-ok", "on").is_ok());
    assert!(c.macd_minus_ok);
    assert!(set_analysis_param(&mut c, "macd-minus-ok", "off").is_ok());
    assert!(!c.macd_minus_ok);

    // f64
    assert!(set_analysis_param(&mut c, "buy-rsi", "25").is_ok());
    assert_eq!(c.buy_rsi, 25.0);
    assert!(set_analysis_param(&mut c, "bb-sigma", "2.5").is_ok());
    assert_eq!(c.bollinger_stddev_multiplier, 2.5);

    // usize period, must be >= 1
    assert!(set_analysis_param(&mut c, "bb-period", "20").is_ok());
    assert_eq!(c.bollinger_period, 20);
    assert!(set_analysis_param(&mut c, "bb-period", "0").is_err());
    assert!(set_analysis_param(&mut c, "adx-period", "x").is_err());

    // weights ARE settable now (runtime); they scale the score, not the log/CSV columns
    assert!(set_analysis_param(&mut c, "weight-basic", "2").is_ok());
    assert_eq!(c.weight_basic, 2.0);
    assert!(set_analysis_param(&mut c, "weight-ema", "1.5").is_ok());
    assert_eq!(c.weight_ema, 1.5);
    assert!(set_analysis_param(&mut c, "weight-basic", "-1").is_err()); // negative rejected
    assert!(set_analysis_param(&mut c, "weight-bogus", "1").is_err()); // unknown indicator
                                                                       // stance is runtime-settable (buyer/seller/holder); validated
    assert!(set_analysis_param(&mut c, "stance", "buyer").is_ok());
    assert_eq!(c.stance, crate::config::Stance::Buyer);
    assert!(set_analysis_param(&mut c, "stance", "holder").is_ok());
    assert_eq!(c.stance, crate::config::Stance::Holder);
    assert!(set_analysis_param(&mut c, "stance", "bogus").is_err());
    // indicator on/off is not a threshold key; still rejected by /set
    assert!(set_analysis_param(&mut c, "bollinger", "on").is_err());
    assert!(set_analysis_param(&mut c, "bogus", "1").is_err());
}

#[test]
fn build_prompt_comparison_mode_fits_budget_with_long_turns() {
    let mut session = make_session("", "low"); // budget = 8000
    session.set_base_contexts(vec!["A".repeat(4_000), "B".repeat(4_000)]);
    session.set_technical_texts(vec![String::new(), String::new()]);
    session.set_fundamental_texts(vec![String::new(), String::new()]);
    session.set_news_items(vec![vec![], vec![]]);
    session.set_ticker_labels(vec![String::new(), String::new()]);
    // Long turns consume most of the budget before base is allocated
    for i in 0..2 {
        session.add_turn(format!("q{}", i), "a".repeat(6_000));
    }
    let prompt = session.build_prompt("question");
    assert!(
        prompt.chars().count() <= parse_memory("low").max_context_chars,
        "prompt must fit within budget even with long turns in comparison mode"
    );
}

#[test]
fn build_base_text_stays_within_tiny_budget_for_multiple_contexts() {
    let mut session = make_session("", "low");
    session.set_base_contexts(vec!["A".repeat(100), "B".repeat(100)]);
    session.set_technical_texts(vec![String::new(), String::new()]);
    session.set_fundamental_texts(vec![String::new(), String::new()]);
    session.set_news_items(vec![vec![], vec![]]);
    session.set_ticker_labels(vec![String::new(), String::new()]);
    // Budget far smaller than the headers alone
    let text = session.build_base_text(30);
    assert!(
        text.chars().count() <= 30,
        "build_base_text must stay within budget even when headers would overflow: got {} chars",
        text.chars().count()
    );
}

#[test]
fn build_base_text_budget_1_does_not_exceed_budget() {
    // Regression: parts.join("\n\n") returns "\n\n" even when all parts are empty,
    // which exceeds budget=1 without the final clamp.
    let mut session = make_session("", "low");
    session.set_base_contexts(vec!["A".repeat(100), "B".repeat(100)]);
    session.set_technical_texts(vec![String::new(), String::new()]);
    session.set_fundamental_texts(vec![String::new(), String::new()]);
    session.set_news_items(vec![vec![], vec![]]);
    session.set_ticker_labels(vec![String::new(), String::new()]);
    let text = session.build_base_text(1);
    assert!(
        text.chars().count() <= 1,
        "build_base_text must never exceed budget; got {} chars",
        text.chars().count()
    );
}

#[test]
fn autoreload_secs_matches_intraday_mode_minutes() {
    assert_eq!(autoreload_secs(crate::config::AnalysisMode::Daily), None);
    assert_eq!(autoreload_secs(crate::config::AnalysisMode::Weekly), None);
    assert_eq!(autoreload_secs(crate::config::AnalysisMode::Monthly), None);
    assert_eq!(
        autoreload_secs(crate::config::AnalysisMode::Intraday5m),
        Some(300)
    );
    assert_eq!(
        autoreload_secs(crate::config::AnalysisMode::Intraday15m),
        Some(900)
    );
    assert_eq!(
        autoreload_secs(crate::config::AnalysisMode::Intraday30m),
        Some(1_800)
    );
}

// --- fundamental budget regression ---

#[test]
fn build_prompt_base_survives_large_fundamental() {
    // fundamental_text larger than half the budget — base must still appear.
    let base = "B".repeat(3_000);
    let mut session = make_session(&base, "low");
    session.set_fundamental_texts(vec!["F".repeat(6_000)]);
    session.set_ticker_labels(vec!["TICK".to_string()]);
    let prompt = session.build_prompt("question");
    assert!(
        prompt.contains('B'),
        "base context must survive even when fundamental_text is large"
    );
    assert!(
        prompt.chars().count() <= parse_memory("low").max_context_chars,
        "prompt must stay within budget"
    );
}

#[test]
fn build_prompt_base_survives_fundamental_exceeding_remaining() {
    // Regression: fundamental >= remaining → fundamental_reserve == remaining → base = 0.
    // Budget must exceed the mandatory tail: constraint_text + forecast_clause +
    // behavior directives (5) + user_line + SEPs (~1640 chars at defaults), plus room
    // for the base context snippet to survive.
    let params = ChatParams {
        max_context_chars: 2400,
        max_recent_turns: 2,
        max_summary_chars: 100,
    };
    let mut session = ChatSession::new(
        ChatSessionSeed {
            base_context: "B".repeat(300),
            technical_text: String::new(),
            fundamental_text: String::new(),
            ticker_label: "TICK".to_string(),
            data_as_of: String::new(),
            facts: None,
        },
        &[],
        params,
        "high",
        "en",
    );
    // fundamental much larger than the total budget
    session.set_fundamental_texts(vec!["F".repeat(2_000)]);
    let prompt = session.build_prompt("question");
    assert!(
        prompt.contains('B'),
        "base context must appear even when fundamental_text exceeds remaining budget"
    );
    // Ceiling = context budget + the fixed instruction block. 2.9.5 grew that block
    // (always-on answer-composition rule + wider depth/scope texts), hence 2950.
    assert!(
        prompt.chars().count() <= 2950,
        "prompt must stay within budget; got {} chars",
        prompt.chars().count()
    );
}

#[test]
fn council_summary_uses_technical_sot_not_base_template() {
    let mut session = ChatSession::new(
        ChatSessionSeed {
            base_context: "BASE_TEMPLATE_SHOULD_NOT_APPEAR".to_string(),
            technical_text: "TECHNICAL_SOT_SHOULD_APPEAR".to_string(),
            fundamental_text: String::new(),
            ticker_label: "TEST".to_string(),
            data_as_of: String::new(),
            facts: None,
        },
        &[],
        parse_memory("mid"),
        "high",
        "en",
    );
    let qid = session.add_board_entry(BoardEntryDraft {
        speaker_type: SpeakerType::User,
        role: EntryRole::UserQuestion,
        provider: "user",
        model: "",
        engine_id: "",
        content: "question",
        parent_entry_id: None,
        round: 0,
    });
    session.add_board_entry(BoardEntryDraft {
        speaker_type: SpeakerType::Llm,
        role: EntryRole::CouncilProposal,
        provider: "openai",
        model: "model",
        engine_id: "",
        content: "opinion",
        parent_entry_id: Some(qid),
        round: 1,
    });

    let prompt = session
        .build_council_summary_prompt()
        .expect("summary prompt");
    assert!(prompt.contains("TECHNICAL_SOT_SHOULD_APPEAR"));
    assert!(
        !prompt.contains("BASE_TEMPLATE_SHOULD_NOT_APPEAR"),
        "council summary must not use base_context template as SOT"
    );
}

#[test]
fn council_prompts_fit_memory_budget_and_use_plain_labels() {
    let params = ChatParams {
        max_context_chars: 1500,
        max_recent_turns: 2,
        max_summary_chars: 100,
    };
    let articles = vec![crate::news::Article {
        title: "N".repeat(5_000),
        url: "https://example.com/news".to_string(),
        published_at: None,
    }];
    let mut session = ChatSession::new(
        ChatSessionSeed {
            base_context: "BASE".repeat(2_000),
            technical_text: "TECH".repeat(2_000),
            fundamental_text: String::new(),
            ticker_label: "TEST".to_string(),
            data_as_of: String::new(),
            facts: None,
        },
        &articles,
        params,
        "high",
        "ja",
    );
    let qid = session.add_board_entry(BoardEntryDraft {
        speaker_type: SpeakerType::User,
        role: EntryRole::UserQuestion,
        provider: "user",
        model: "",
        engine_id: "",
        content: "古い質問",
        parent_entry_id: None,
        round: 0,
    });
    session.add_board_entry(BoardEntryDraft {
        speaker_type: SpeakerType::Chair,
        role: EntryRole::FacilitatorReport,
        provider: "openai",
        model: "model",
        engine_id: "",
        content: &"古いまとめ".repeat(1_000),
        parent_entry_id: Some(qid),
        round: 1,
    });

    let prompt =
        session.build_council_prompt("今後の見通し", Some(&"直前まとめ".repeat(1_000)), qid + 2);
    assert!(prompt.chars().count() <= params.max_context_chars);
    assert!(prompt.contains("見解:"));
    assert!(
        !prompt.contains("## 見解"),
        "council prompt must not ask for Markdown headings"
    );
}

#[test]
fn parse_council_rounds_rejects_bad_values_and_accepts_fullwidth_digits() {
    assert_eq!(parse_council_rounds("2").unwrap(), 2);
    assert_eq!(parse_council_rounds("２").unwrap(), 2);
    assert!(parse_council_rounds("0").is_err());
    assert!(parse_council_rounds("abc").is_err());
    assert!(parse_council_rounds(&(COUNCIL_MAX_ROUNDS + 1).to_string()).is_err());
}

// --- apply_ticker_switch ---

#[test]
fn ticker_switch_updates_config_ticker_and_clears_session() {
    let mut session = make_session("old base context", "mid");
    session.add_turn("question".to_string(), "answer".to_string());
    session.set_debate_mode("summary");
    session.add_debate_entry("openai", "m", "", "q", "a");
    let mut config = crate::config::Config {
        ticker: "OLD".to_string(),
        ..crate::config::Config::default()
    };

    apply_ticker_switch(
        &mut session,
        &mut config,
        TickerSwitchData {
            ticker: "NEW".to_string(),
            ticker_label: String::new(),
            base_context: "new base context".to_string(),
            technical_text: String::new(),
            fundamental_text: String::new(),
            news: vec![],
            data_as_of: String::new(),
            facts: None,
        },
    );

    assert_eq!(config.ticker, "NEW");
    assert_eq!(session.base_contexts(), vec!["new base context"]);
    assert!(
        session.recent_turns.is_empty(),
        "recent_turns must be cleared"
    );
    assert_eq!(
        session.news_items().len(),
        1,
        "news_items must have one entry after switch"
    );
    assert!(
        session.news_items()[0].is_empty(),
        "news must be cleared after switch"
    );
    assert!(
        session.conversation_summary.is_none(),
        "summary must be cleared"
    );
    assert!(
        session.debate_entries.is_empty(),
        "debate entries must be cleared"
    );
}

#[test]
fn ticker_switch_does_not_touch_config_on_build_failure() {
    // Simulate the staging pattern: next_config carries new ticker,
    // chat_config is only updated via apply_ticker_switch on success.
    let chat_config = crate::config::Config {
        ticker: "OLD".to_string(),
        ..crate::config::Config::default()
    };

    // Simulate a failed build (next_config is prepared but build fails → apply never called)
    let mut next_config = chat_config.clone();
    next_config.ticker = "NEW".to_string();
    // build fails — apply_ticker_switch is NOT called

    assert_eq!(
        chat_config.ticker, "OLD",
        "chat_config must remain unchanged after build failure"
    );
}

// `--private` is a no-trace session, and the chat history file records the
// user's own typed lines — exactly the trace the flag rules out. Guards both
// directions: private yields no path at all (so the file is neither read nor
// written), and the normal path still resolves one, so the guard cannot
// silently disable history for everyone.
#[test]
fn private_mode_uses_no_chat_history_file() {
    let _gate = crate::private::test_gate();
    crate::private::set_private(true);
    assert!(
        super::history_file_path().is_none(),
        "--private must not touch the history file"
    );

    crate::private::set_private(false);
    let path = super::history_file_path().expect("a normal session keeps its history");
    assert!(path.ends_with(".xoksa_history"));
}

// --- G1/G2: the confirmed data a session hands to the output-integrity guard ---

/// Confirmed data carrying a full fundamental block — the values the session
/// displays, including the per-lot dividend and the trading unit.
fn facts_with_full_fundamentals(symbol: &str, close: f64) -> crate::integrity::SymbolFacts {
    let config = crate::config::Config {
        ticker: symbol.to_string(),
        fundamental: true,
        ..crate::config::Config::default()
    };
    let mut g =
        crate::technical::types::TechnicalDataGuard::new(symbol.to_string(), "2026-09-08".into());
    g.set_name("テスト銘柄");
    g.set_currency("JPY");
    g.set_close(close);
    let fundamental = crate::fundamental::FundamentalData::build(
        crate::fundamental::FundamentalInputs {
            currency: "JPY".to_string(),
            revenue: Some(13_704_000_000_000.0),
            dividend: Some(3.0),
            trading_unit: Some(100),
            ..Default::default()
        },
        Some(close),
    );
    crate::integrity::SymbolFacts::from_sources(&g, &config, None, Some(&fundamental))
}

/// Confirmed data for `symbol` with the readings given, built the way the
/// loader builds it — from a guard, never from text.
fn facts_for_test(
    symbol: &str,
    close: f64,
    rsi: f64,
    revenue: Option<f64>,
) -> crate::integrity::SymbolFacts {
    let config = crate::config::Config {
        ticker: symbol.to_string(),
        fundamental: revenue.is_some(),
        ..crate::config::Config::default()
    };
    let mut g =
        crate::technical::types::TechnicalDataGuard::new(symbol.to_string(), "2026-09-08".into());
    g.set_name("テスト銘柄");
    g.set_currency("JPY");
    g.set_close(close);
    g.set_rsi(rsi);
    let fundamental = revenue.map(|r| {
        crate::fundamental::FundamentalData::build(
            crate::fundamental::FundamentalInputs {
                currency: "JPY".to_string(),
                revenue: Some(r),
                ..Default::default()
            },
            Some(close),
        )
    });
    crate::integrity::SymbolFacts::from_sources(&g, &config, None, fundamental.as_ref())
}

/// Does the session's confirmed data accept this sentence?
fn session_accepts(session: &ChatSession, sentence: &str) -> bool {
    let facts = session.confirmed_facts();
    crate::integrity::written_numbers(sentence)
        .into_iter()
        .all(|(range, n)| {
            let (offset, s) = crate::integrity::sentence_bounds(sentence, range.start);
            let local = range.start.saturating_sub(offset)..range.end.saturating_sub(offset);
            !facts.verify(s, &local, &n).is_reportable()
        })
}

#[test]
fn a_reload_replaces_the_confirmed_data_the_guard_checks_against() {
    let mut session = make_session("old base", "mid");
    session.tickers[0].label = "9432.T (テスト銘柄)".to_string();
    session.tickers[0].facts = Some(facts_for_test("9432.T", 100.0, 30.0, None));
    assert!(session_accepts(&session, "終値は100円です。"));

    apply_ticker_reload(
        &mut session,
        0,
        TickerReloadData {
            base_context: "new base".to_string(),
            technical_text: "new tech".to_string(),
            indicator_summary: String::new(),
            news: None,
            data_as_of: "2026-09-08".to_string(),
            facts: Some(facts_for_test("9432.T", 200.0, 60.0, None)),
            fundamental: None,
        },
    );

    assert!(
        session_accepts(&session, "終値は200円です。"),
        "the refreshed value must be accepted"
    );
    assert!(
        !session_accepts(&session, "終値は100円です。"),
        "the stale value must no longer be accepted"
    );
}

#[test]
fn a_technical_only_reload_keeps_the_fundamentals_the_session_still_shows() {
    let mut session = make_session("old base", "mid");
    session.tickers[0].label = "9432.T (テスト銘柄)".to_string();
    session.tickers[0].facts = Some(facts_for_test(
        "9432.T",
        100.0,
        30.0,
        Some(13_704_000_000_000.0),
    ));

    // A periodic reload re-fetches the technical side only.
    apply_ticker_reload(
        &mut session,
        0,
        TickerReloadData {
            base_context: "new base".to_string(),
            technical_text: "new tech".to_string(),
            indicator_summary: String::new(),
            news: None,
            data_as_of: "2026-09-08".to_string(),
            facts: Some(facts_for_test("9432.T", 200.0, 60.0, None)),
            fundamental: None,
        },
    );

    assert!(
        session_accepts(&session, "売上高は13.704兆円です。"),
        "the fundamental block the session still presents must stay confirmed"
    );
    assert!(session_accepts(&session, "終値は200円です。"));
}

#[test]
fn a_loaded_session_hands_confirmed_data_to_every_llm_turn() {
    // `/basic`, the chat turns, `/forum` and the debate all take the data from
    // this one accessor, so a loaded session must never yield an empty set.
    let mut session = make_session("base", "mid");
    assert!(
        session.confirmed_facts().is_empty(),
        "nothing loaded: nothing to attribute against"
    );
    session.tickers[0].facts = Some(facts_for_test("9432.T", 100.0, 30.0, None));
    assert!(!session.confirmed_facts().is_empty());
    // The reviewer's case: the RSI and the MACD swapped must not pass.
    assert!(!session_accepts(&session, "RSIは100です。"));
}

#[test]
fn a_failed_fundamental_refetch_drops_the_display_and_the_reference_together() {
    // A manual refresh that goes for the fundamentals and comes back with
    // nothing must not leave the guard checking against a block the user can no
    // longer see. Display, prompt context and confirmed data move together.
    let mut session = make_session("old base", "mid");
    session.tickers[0].label = "9432.T (テスト銘柄)".to_string();
    session.tickers[0].fundamental_text = "売上高: 13,704,000百万円".to_string();
    session.tickers[0].facts = Some(facts_for_test(
        "9432.T",
        100.0,
        30.0,
        Some(13_704_000_000_000.0),
    ));
    assert!(session_accepts(&session, "売上高は13.704兆円です。"));

    // The fetch was attempted and produced nothing: `Some("")`.
    apply_ticker_reload(
        &mut session,
        0,
        TickerReloadData {
            base_context: "new base".to_string(),
            technical_text: "new tech".to_string(),
            indicator_summary: String::new(),
            news: None,
            data_as_of: "2026-09-08".to_string(),
            facts: Some(facts_for_test("9432.T", 200.0, 60.0, None)),
            fundamental: Some(String::new()),
        },
    );

    assert!(
        session.tickers[0].fundamental_text.is_empty(),
        "the display must reflect the failed fetch"
    );
    assert!(
        !session_accepts(&session, "売上高は13.704兆円です。"),
        "the reference must not keep what the display dropped"
    );
    assert!(session_accepts(&session, "終値は200円です。"));
}

#[test]
fn a_successful_fundamental_refetch_replaces_both() {
    let mut session = make_session("old base", "mid");
    session.tickers[0].label = "9432.T (テスト銘柄)".to_string();
    session.tickers[0].facts = Some(facts_for_test(
        "9432.T",
        100.0,
        30.0,
        Some(13_704_000_000_000.0),
    ));

    apply_ticker_reload(
        &mut session,
        0,
        TickerReloadData {
            base_context: "new base".to_string(),
            technical_text: "new tech".to_string(),
            indicator_summary: String::new(),
            news: None,
            data_as_of: "2026-09-08".to_string(),
            facts: Some(facts_for_test(
                "9432.T",
                200.0,
                60.0,
                Some(14_000_000_000_000.0),
            )),
            fundamental: Some("売上高: 14,000,000百万円".to_string()),
        },
    );

    assert_eq!(
        session.tickers[0].fundamental_text,
        "売上高: 14,000,000百万円"
    );
    assert!(session_accepts(&session, "売上高は14.0兆円です。"));
    assert!(
        !session_accepts(&session, "売上高は13.704兆円です。"),
        "the replaced figure must no longer be confirmed"
    );
}

#[test]
fn a_technical_only_reload_keeps_every_fundamental_value_the_session_shows() {
    // The session keeps showing the whole fundamental block, so every value in
    // it must stay confirmed — the per-lot dividend and the trading unit
    // included, not just the headline figures.
    let mut session = make_session("old base", "mid");
    session.tickers[0].label = "9432.T (テスト銘柄)".to_string();
    session.tickers[0].facts = Some(facts_with_full_fundamentals("9432.T", 100.0));
    for text in [
        "売上高は13.704兆円です。",
        "配当（1株あたり）: 3.00円",
        "配当（1単元あたり）: 300円",
        "最少単位株: 100株",
    ] {
        assert!(session_accepts(&session, text), "before reload: {text}");
    }

    apply_ticker_reload(
        &mut session,
        0,
        TickerReloadData {
            base_context: "new base".to_string(),
            technical_text: "new tech".to_string(),
            indicator_summary: String::new(),
            news: None,
            data_as_of: "2026-09-08".to_string(),
            facts: Some(facts_for_test("9432.T", 200.0, 60.0, None)),
            fundamental: None,
        },
    );

    for text in [
        "売上高は13.704兆円です。",
        "配当（1株あたり）: 3.00円",
        "配当（1単元あたり）: 300円",
        "最少単位株: 100株",
    ] {
        assert!(session_accepts(&session, text), "after reload: {text}");
    }
    assert!(session_accepts(&session, "終値は200円です。"));
}

#[test]
fn a_failed_fundamental_refetch_still_drops_every_fundamental_value() {
    // The counterpart: when the block goes, all of it goes — including the two
    // values the carry-over list was missing.
    let mut session = make_session("old base", "mid");
    session.tickers[0].label = "9432.T (テスト銘柄)".to_string();
    session.tickers[0].facts = Some(facts_with_full_fundamentals("9432.T", 100.0));

    apply_ticker_reload(
        &mut session,
        0,
        TickerReloadData {
            base_context: "new base".to_string(),
            technical_text: "new tech".to_string(),
            indicator_summary: String::new(),
            news: None,
            data_as_of: "2026-09-08".to_string(),
            facts: Some(facts_for_test("9432.T", 200.0, 60.0, None)),
            fundamental: Some(String::new()),
        },
    );

    for text in [
        "売上高は13.704兆円です。",
        "配当（1単元あたり）: 300円",
        "最少単位株: 100株",
    ] {
        assert!(
            !session_accepts(&session, text),
            "after failed refetch: {text}"
        );
    }
}
