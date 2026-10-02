//! Token / context-size reporting and chat-status display (`/status`, `/token`).
//! Read-only views over `ChatSession`; reached via descendant-module access.

use super::llm::active_llm_model;
use super::*;

pub(super) fn write_token_summary(
    out: &mut super::ChatOut,
    session: &ChatSession,
    cumulative_tokens: &ChatTokenUsage,
    lang: &str,
    detail: bool,
) {
    let b = session.context_size_breakdown();
    let total_tokens = cumulative_tokens
        .input
        .saturating_add(cumulative_tokens.output);
    match lang {
        "ja" => {
            out.line(format!(
                "ロード済みコンテキスト: {} 文字 | 通常チャット候補: {} 文字 | メモリ上限: {} 文字",
                b.base_context_chars, b.regular_chat_candidate_chars, b.prompt_budget_chars
            ));
            out.line(format!(
                "セッション累計 — 入力: {} / 出力: {} / 合計: {}",
                cumulative_tokens.input, cumulative_tokens.output, total_tokens
            ));
            if !detail {
                out.line("内訳: /token detail");
                return;
            }
            out.line("");
            out.line("=== Token / Context Detail ===");
            out.line("注記: 内訳は送信前の文字数です。実課金・実トークンはプロバイダー側の tokenizer とAPI返却値に依存します。");
            out.line(format!(
                "現在の保持設定: budget={} chars / recent_turns={} / summary_cap={} chars",
                b.prompt_budget_chars, b.max_recent_turns, b.summary_budget_chars
            ));
            out.line(format!("ロード銘柄数: {}", b.ticker_count));
            out.line("");
            out.line("通常チャット候補（/chat の通常質問で主に使う文脈）:");
            out.line(format!(
                "  technical context : {} chars",
                b.technical_context_chars
            ));
            out.line(format!(
                "  fundamental       : {} chars",
                b.fundamental_chars
            ));
            out.line(format!("  news              : {} chars", b.news_chars));
            out.line(format!(
                "  recent turns      : {} chars ({} turn(s))",
                b.recent_turns_chars, b.recent_turn_count
            ));
            out.line(format!("  summary           : {} chars", b.summary_chars));
            out.line(format!(
                "  debate buffer     : {} chars (mode={}, entries={})",
                b.debate_chars, b.debate_mode, b.debate_entry_count
            ));
            out.line(format!("  guard             : {} chars", b.guard_chars));
            out.line(format!(
                "  candidate total   : {} chars",
                b.regular_chat_candidate_chars
            ));
            out.line(format!(
                "追加ニュース (mandatory tail): {} chars (entries={}/16, auto={})",
                b.extra_news_chars,
                b.extra_news_entry_count,
                on_off(b.extra_news_auto_inject)
            ));
            out.line("");
            out.line("特別コマンド:");
            out.line(format!(
                "  /run full candidate    : {} chars",
                b.initial_prompt_candidate_chars
            ));
            out.line(format!(
                "  loaded base context    : {} chars",
                b.base_context_chars
            ));
            out.line("実際の通常送信では、メモリ上限を超える低優先度セクションは切り詰めまたは省略されます。");
        }
        _ => {
            out.line(format!(
                "Loaded context: {} chars | Regular-chat candidate: {} chars | Memory budget: {} chars",
                b.base_context_chars, b.regular_chat_candidate_chars, b.prompt_budget_chars
            ));
            out.line(format!(
                "Session total — input: {} / output: {} / total: {}",
                cumulative_tokens.input, cumulative_tokens.output, total_tokens
            ));
            if !detail {
                out.line("Breakdown: /token detail");
                return;
            }
            out.line("");
            out.line("=== Token / Context Detail ===");
            out.line("Note: breakdown values are pre-send character counts. Billing and actual tokens depend on the provider tokenizer and returned API usage.");
            out.line(format!(
                "Retention: budget={} chars / recent_turns={} / summary_cap={} chars",
                b.prompt_budget_chars, b.max_recent_turns, b.summary_budget_chars
            ));
            out.line(format!("Loaded tickers: {}", b.ticker_count));
            out.line("");
            out.line("Regular chat candidate context:");
            out.line(format!(
                "  technical context : {} chars",
                b.technical_context_chars
            ));
            out.line(format!(
                "  fundamental       : {} chars",
                b.fundamental_chars
            ));
            out.line(format!("  news              : {} chars", b.news_chars));
            out.line(format!(
                "  recent turns      : {} chars ({} turn(s))",
                b.recent_turns_chars, b.recent_turn_count
            ));
            out.line(format!("  summary           : {} chars", b.summary_chars));
            out.line(format!(
                "  debate buffer     : {} chars (mode={}, entries={})",
                b.debate_chars, b.debate_mode, b.debate_entry_count
            ));
            out.line(format!("  guard             : {} chars", b.guard_chars));
            out.line(format!(
                "  candidate total   : {} chars",
                b.regular_chat_candidate_chars
            ));
            out.line(format!(
                "Extra news (mandatory tail): {} chars (entries={}/16, auto={})",
                b.extra_news_chars,
                b.extra_news_entry_count,
                on_off(b.extra_news_auto_inject)
            ));
            out.line("");
            out.line("Special commands:");
            out.line(format!(
                "  /run full candidate    : {} chars",
                b.initial_prompt_candidate_chars
            ));
            out.line(format!(
                "  loaded base context    : {} chars",
                b.base_context_chars
            ));
            out.line("In normal chat sends, lower-priority sections are truncated or omitted when the memory budget is exceeded.");
        }
    }
}

// "on"/"off" is identical in both languages, so this takes no `lang` (the previous
// `localized_on_off` had a dead `lang` parameter — SOT §4.2 cleanup).
fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn localized_yes_no(lang: &str, value: bool) -> &'static str {
    match (lang, value) {
        ("ja", true) => "あり",
        ("ja", false) => "なし",
        (_, true) => "yes",
        (_, false) => "no",
    }
}

fn autoreload_status_text(
    enabled: bool,
    next_reload_at: Option<&tokio::time::Instant>,
    lang: &str,
) -> String {
    if !enabled {
        return match lang {
            "ja" => "off".to_string(),
            _ => "off".to_string(),
        };
    }
    match next_reload_at {
        Some(deadline) => {
            let secs = deadline
                .saturating_duration_since(tokio::time::Instant::now())
                .as_secs();
            match lang {
                "ja" => format!("on（次回まで約{}秒）", secs),
                _ => format!("on (next in ~{}s)", secs),
            }
        }
        None => match lang {
            "ja" => "on（次回時刻未設定）".to_string(),
            _ => "on (next time not scheduled)".to_string(),
        },
    }
}

pub(super) fn write_chat_status(
    out: &mut super::ChatOut,
    session: &ChatSession,
    config: &Config,
    cumulative_tokens: &ChatTokenUsage,
    autoreload_enabled: bool,
    next_reload_at: Option<&tokio::time::Instant>,
    lang: &str,
) {
    let b = session.context_size_breakdown();
    let total_tokens = cumulative_tokens
        .input
        .saturating_add(cumulative_tokens.output);
    let memory = chat_memory_label(session.params);
    let model = active_llm_model(config);
    let mode_label = config.analysis_mode.label(lang);
    let interval = config.analysis_mode.data_interval();
    let autoreload = autoreload_status_text(autoreload_enabled, next_reload_at, lang);

    match lang {
        "ja" => {
            out.line("=== Chat Status ===");
            out.line(format!("ロード銘柄数: {}", b.ticker_count));
            if b.ticker_count == 0 {
                out.line("  銘柄はロードされていません。");
            } else {
                for (idx, label) in session.ticker_labels().iter().enumerate() {
                    if label.is_empty() {
                        continue;
                    }
                    let technical_chars = session
                        .technical_texts()
                        .get(idx)
                        .map(|s| s.chars().count())
                        .unwrap_or(0);
                    let has_fundamental = session
                        .fundamental_texts()
                        .get(idx)
                        .map(|s| !s.is_empty())
                        .unwrap_or(false);
                    let news_count = session.news_items().get(idx).map(Vec::len).unwrap_or(0);
                    out.line(format!(
                        "  {}. {} | technical={} chars / fundamental={} / news={}",
                        idx + 1,
                        short_display_label(label),
                        technical_chars,
                        localized_yes_no(lang, has_fundamental),
                        news_count
                    ));
                }
            }
            out.line(format!("分析足: {} ({})", mode_label, interval));
            out.line(format!(
                "LLM: provider={} / model={} / guard={}",
                config.llm_provider, model, session.constraint_level
            ));
            out.line(format!(
                "回答スタイル: depth={} / scope={} / shape={} / cast={}",
                session.read_depth,
                session.knowledge_scope,
                session.response_shape,
                session.forecast_mode
            ));
            out.line(format!(
                "メモリ: {} (budget={} chars / recent_turns={}/{} / summary_cap={} chars)",
                memory,
                b.prompt_budget_chars,
                b.recent_turn_count,
                b.max_recent_turns,
                b.summary_budget_chars
            ));
            out.line(format!("会話要約: {} chars", b.summary_chars));
            out.line(format!(
                "Debate Buffer: mode={} / entries={} / chars={}",
                b.debate_mode, b.debate_entry_count, b.debate_chars
            ));
            out.line(format!(
                "追加ニュース: entries={}/16 / chars={} / auto={}",
                b.extra_news_entry_count,
                b.extra_news_chars,
                on_off(b.extra_news_auto_inject)
            ));
            out.line(format!(
                "トークン累計: input={} / output={} / total={}",
                cumulative_tokens.input, cumulative_tokens.output, total_tokens
            ));
            out.line(format!(
                "通常チャット候補: {} chars / budget={} chars",
                b.regular_chat_candidate_chars, b.prompt_budget_chars
            ));
            out.line(format!(
                "データ取得設定: fundamental={} / news={}",
                on_off(config.fundamental),
                on_off(!config.no_news)
            ));
            out.line(format!("自動リロード: {}", autoreload));
            out.line("詳細: /token detail");
        }
        _ => {
            out.line("=== Chat Status ===");
            out.line(format!("Loaded tickers: {}", b.ticker_count));
            if b.ticker_count == 0 {
                out.line("  No ticker loaded.");
            } else {
                for (idx, label) in session.ticker_labels().iter().enumerate() {
                    if label.is_empty() {
                        continue;
                    }
                    let technical_chars = session
                        .technical_texts()
                        .get(idx)
                        .map(|s| s.chars().count())
                        .unwrap_or(0);
                    let has_fundamental = session
                        .fundamental_texts()
                        .get(idx)
                        .map(|s| !s.is_empty())
                        .unwrap_or(false);
                    let news_count = session.news_items().get(idx).map(Vec::len).unwrap_or(0);
                    out.line(format!(
                        "  {}. {} | technical={} chars / fundamental={} / news={}",
                        idx + 1,
                        short_display_label(label),
                        technical_chars,
                        localized_yes_no(lang, has_fundamental),
                        news_count
                    ));
                }
            }
            out.line(format!("Bar mode: {} ({})", mode_label, interval));
            out.line(format!(
                "LLM: provider={} / model={} / guard={}",
                config.llm_provider, model, session.constraint_level
            ));
            out.line(format!(
                "Response style: depth={} / scope={} / shape={} / cast={}",
                session.read_depth,
                session.knowledge_scope,
                session.response_shape,
                session.forecast_mode
            ));
            out.line(format!(
                "Memory: {} (budget={} chars / recent_turns={}/{} / summary_cap={} chars)",
                memory,
                b.prompt_budget_chars,
                b.recent_turn_count,
                b.max_recent_turns,
                b.summary_budget_chars
            ));
            out.line(format!("Summary: {} chars", b.summary_chars));
            out.line(format!(
                "Debate Buffer: mode={} / entries={} / chars={}",
                b.debate_mode, b.debate_entry_count, b.debate_chars
            ));
            out.line(format!(
                "Extra news: entries={}/16 / chars={} / auto={}",
                b.extra_news_entry_count,
                b.extra_news_chars,
                on_off(b.extra_news_auto_inject)
            ));
            out.line(format!(
                "Token total: input={} / output={} / total={}",
                cumulative_tokens.input, cumulative_tokens.output, total_tokens
            ));
            out.line(format!(
                "Regular-chat candidate: {} chars / budget={} chars",
                b.regular_chat_candidate_chars, b.prompt_budget_chars
            ));
            out.line(format!(
                "Fetch settings: fundamental={} / news={}",
                on_off(config.fundamental),
                on_off(!config.no_news)
            ));
            out.line(format!("Auto-reload: {}", autoreload));
            out.line("Details: /token detail");
        }
    }
}
