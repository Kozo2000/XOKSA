//! Interactive follow-up chat after XOKSA analysis

mod council;
mod debate;
pub(crate) mod exec; // unified slash-command dispatch (ChatSession::execute) — shared by CLI + Web
pub(crate) mod guard; // SOT constraint / forecast text — reused by the Web UI server
pub(crate) mod help; // command catalog — reused by the Web UI server's /api/chat/commands
pub(crate) mod llm; // LLM provider/model resolution + chat turn — active_llm_model reused by the Web UI server
mod news_extra;
mod prompt;
mod report;
mod run;
pub(crate) mod ticker; // load/reload pipeline + canonical compact indicator line (reused by the Web UI server)
use debate::clean_debate_excerpt;
use guard::{constraint_text, criticize_empty_debate_text, criticize_task_text, forecast_clause};
use news_extra::{build_news_extra_inject_text, NewsExtraEntry};
pub use run::run_chat_loop;

/// Number of /news-extra buffer slots.
const NEWS_EXTRA_SLOTS: usize = 16;
/// Maximum keyword length for /news-extra search.
const NEWS_EXTRA_MAX_KEYWORD_CHARS: usize = 256;
/// SOT context character budget per participant/facilitator prompt in Council mode.
const COUNCIL_SOT_CHARS: usize = 3_000;
/// Upper bound for /council ask --rounds. Council calls multiply API cost.
const COUNCIL_MAX_ROUNDS: u32 = 8;

use crate::config::Config;
use crate::llm::ChatTokenUsage;
use crate::news::Article;
use crate::traits::ArticleFetcher;
use anyhow::Result;
use chrono::Local;
use rustyline::{error::ReadlineError, DefaultEditor, ExternalPrinter};
use std::collections::HashMap;
use std::time::Duration;

// Maximum characters allowed for a single user input
const CHAT_MAX_INPUT_CHARS: usize = 2_000;

/// Output sink for the unified command dispatch. The same `execute()` code drives
/// both front-ends: the CLI prints immediately to the terminal, while the Web UI
/// streams each line over an SSE channel. One implementation, identical results
/// on CLI and GUI (SOT).
pub(crate) enum ChatOut {
    /// CLI: write straight to the terminal.
    Stdout,
    /// Web (streaming): push each line immediately to an SSE channel, so the
    /// browser shows output the moment it is produced (e.g. each forum round).
    Channel(tokio::sync::mpsc::UnboundedSender<String>),
}

impl ChatOut {
    /// A normal output line.
    pub(crate) fn line(&mut self, s: impl Into<String>) {
        match self {
            ChatOut::Stdout => println!("{}", s.into()),
            ChatOut::Channel(tx) => {
                let _ = tx.send(s.into());
            }
        }
    }

    /// An error/notice line (CLI → stderr; Web → same sink).
    pub(crate) fn err(&mut self, s: impl Into<String>) {
        match self {
            ChatOut::Stdout => eprintln!("{}", s.into()),
            ChatOut::Channel(tx) => {
                let _ = tx.send(s.into());
            }
        }
    }
}

/// Where the chat history lives — `~/.xoksa_history`, or the working directory
/// when the home directory is unknown.
///
/// `None` in a no-trace session (`--private`). The history file records the
/// user's own typed lines (Class B), so writing one is exactly the trace the
/// flag rules out; a private session neither reads nor writes it.
fn history_file_path() -> Option<String> {
    if crate::private::is_private() {
        return None;
    }
    Some(match crate::private::home_dir() {
        Some(h) => format!("{h}/.xoksa_history"),
        None => ".xoksa_history".to_string(),
    })
}

fn ensure_history_permissions(path: &str) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(path).is_ok() {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

const DEBATE_SUMMARY_MAX_ENTRIES: usize = 2;
const DEBATE_CLAIMS_MAX_ENTRIES: usize = 3;
const DEBATE_SUMMARY_ENTRY_CHARS: usize = 500;
const DEBATE_CLAIMS_ENTRY_CHARS: usize = 900;
const DEBATE_SUMMARY_CONTEXT_CHARS: usize = 1_200;
const DEBATE_CLAIMS_CONTEXT_CHARS: usize = 2_400;
const DEBATE_STORED_RESPONSE_CHARS: usize = 1_200;
const DEBATE_STORED_USER_CHARS: usize = 300;

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

// Extra chars reserved for "\n\n" separators between prompt sections (up to 4 sections → 3 separators + tail sep = 8, plus safety margin)
const SEP_OVERHEAD: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ChatParams {
    max_context_chars: usize,
    max_recent_turns: usize,
    max_summary_chars: usize,
}

#[derive(Debug, Clone, Copy)]
enum TruncateMode {
    Head,
    Tail,
}

struct PromptSection {
    text: String,
    mode: TruncateMode,
}

fn parse_memory(memory: &str) -> ChatParams {
    match memory.trim() {
        "low" => ChatParams {
            max_context_chars: 8_000,
            max_recent_turns: 2,
            max_summary_chars: 800,
        },
        "high" => ChatParams {
            max_context_chars: 32_000,
            max_recent_turns: 8,
            max_summary_chars: 3_000,
        },
        _ => ChatParams {
            // "mid" and unknown values → mid (CLI validated by clap; env warns before calling this)
            max_context_chars: 16_000,
            max_recent_turns: 4,
            max_summary_chars: 2_000,
        },
    }
}

fn chat_memory_label(params: ChatParams) -> &'static str {
    match params {
        p if p == parse_memory("low") => "low",
        p if p == parse_memory("high") => "high",
        p if p == parse_memory("mid") => "mid",
        _ => "custom",
    }
}

#[derive(Clone)]
struct NewsItem {
    id: String,
    title: String,
    url: String,
}

impl NewsItem {
    fn prompt_line(&self) -> String {
        format!("[{}] {}\n\n    URL: {}\n", self.id, self.title, self.url)
    }

    fn write(&self, out: &mut ChatOut) {
        out.line(format!("[{}] {}", self.id, self.title));
        out.line("");
        out.line(format!("    {}", self.url));
        out.line("");
    }
}

struct ChatTurn {
    user: String,
    assistant: String,
}

struct DebateEntry {
    provider: String,
    model: String,
    engine_id: String, // ollama alias when provider=ollama; empty for other providers
    user: String,
    response_excerpt: String,
    data_as_of: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SpeakerType {
    User,
    Llm,
    Chair,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EntryRole {
    UserQuestion,
    Proposal,          // from regular debate (stored via add_debate_entry)
    CouncilProposal,   // from /council ask participant round
    FacilitatorReport, // from /council ask facilitator round
    Critique,
    Summary,
}

struct BoardEntry {
    id: u64,
    speaker_type: SpeakerType,
    role: EntryRole,
    provider: String,
    model: String,
    engine_id: String, // ollama alias when provider=ollama; empty for other providers
    content: String,
    parent_entry_id: Option<u64>,
    round: u32,
}

struct BoardEntryDraft<'a> {
    speaker_type: SpeakerType,
    role: EntryRole,
    provider: &'a str,
    model: &'a str,
    engine_id: &'a str,
    content: &'a str,
    parent_entry_id: Option<u64>,
    round: u32,
}

#[derive(Clone)]
struct CouncilParticipant {
    provider: String,
    model: String,
    alias: String, // ollama alias when provider=ollama; empty for other providers
    // Non-empty only when provider=ollama and alias was resolved at /council set time
    ollama_host: String,
    ollama_port: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ContextSizeBreakdown {
    prompt_budget_chars: usize,
    max_recent_turns: usize,
    summary_budget_chars: usize,
    base_context_chars: usize,
    technical_context_chars: usize,
    fundamental_chars: usize,
    news_chars: usize,
    recent_turns_chars: usize,
    summary_chars: usize,
    debate_chars: usize,
    guard_chars: usize,
    regular_chat_candidate_chars: usize,
    initial_prompt_candidate_chars: usize,
    ticker_count: usize,
    recent_turn_count: usize,
    debate_mode: String,
    debate_entry_count: usize,
    extra_news_chars: usize,
    extra_news_entry_count: usize,
    extra_news_auto_inject: bool,
}

/// One loaded ticker as a single, self-contained record. All of a ticker's
/// rendered data lives together here (never in parallel arrays), so a command can
/// never scope to a subset, get the fields out of sync, or drop a ticker: the table
/// is the single source and every command iterates all of it.
#[derive(Default)]
struct TickerEntry {
    label: String,
    base_context: String,
    technical_text: String,
    /// Compact indicator readings (enabled indicators only) for the autoreload notice.
    indicator_summary: String,
    fundamental_text: String,
    news: Vec<NewsItem>,
    data_as_of: String,
    /// Confirmed values for this instrument, taken from the guard and the
    /// fundamental data at load time. The output-integrity guard verifies the
    /// model's numbers against these — never against the prompt text, which also
    /// carries the conversation and other models' words.
    facts: Option<crate::integrity::SymbolFacts>,
}

struct ChatSession {
    /// The loaded tickers as a table (one `TickerEntry` per ticker). Every command
    /// targets all rows — the handling does not change with the number of tickers.
    tickers: Vec<TickerEntry>,
    conversation_summary: Option<String>,
    recent_turns: Vec<ChatTurn>,
    debate_entries: Vec<DebateEntry>,
    debate_mode: String,
    params: ChatParams,
    constraint_level: String,
    lang: String,
    news_extra: Vec<Option<NewsExtraEntry>>,
    news_extra_slot: usize,
    news_extra_pending_text: Option<String>,
    news_extra_auto_inject: bool,
    read_depth: String,
    knowledge_scope: String,
    response_shape: String,
    forecast_mode: String,
    board_entries: Vec<BoardEntry>,
    board_entry_counter: u64,
    council_participants: Vec<CouncilParticipant>,
    council_facilitator: Option<CouncilParticipant>,
}

impl ChatSession {
    /// The confirmed data of every loaded instrument, for the output-integrity
    /// guard. Empty when nothing has been loaded, in which case the guard falls
    /// back to its presence test and does not claim attribution was verified.
    fn confirmed_facts(&self) -> crate::integrity::ConfirmedFactSet {
        crate::integrity::facts_from_parts(self.tickers.iter().filter_map(|t| t.facts.clone()))
    }
}

struct ChatSessionSeed {
    base_context: String,
    technical_text: String,
    fundamental_text: String,
    ticker_label: String,
    data_as_of: String,
    /// Confirmed values of the instrument, carried from the loader.
    facts: Option<crate::integrity::SymbolFacts>,
}

pub struct ChatLoopInput<'a> {
    pub config: &'a Config,
    pub base_context: &'a str,
    pub technical_display: &'a str,
    pub fundamental_display: &'a str,
    pub ticker_label: &'a str,
    pub articles: &'a [Article],
    pub ticker_name_map: &'a HashMap<String, String>,
    pub initial_data_as_of: &'a str,
    pub default_extra_tickers: &'a [String],
    /// Confirmed values of the primary instrument, built by the caller from the
    /// guard and the fundamental data. `None` for a session opened with no ticker.
    pub facts: Option<crate::integrity::SymbolFacts>,
}

impl ChatSession {
    fn new(
        seed: ChatSessionSeed,
        articles: &[Article],
        params: ChatParams,
        constraint_level: &str,
        lang: &str,
    ) -> Self {
        let news: Vec<NewsItem> = articles
            .iter()
            .enumerate()
            .map(|(i, a)| NewsItem {
                id: format!("N{:02}", i + 1),
                title: a.title.clone(),
                url: a.url.clone(),
            })
            .collect();
        Self {
            tickers: vec![TickerEntry {
                label: seed.ticker_label,
                base_context: seed.base_context,
                technical_text: seed.technical_text,
                // Populated on the first reload; the autoreload notice only fires after a reload.
                indicator_summary: String::new(),
                fundamental_text: seed.fundamental_text,
                news,
                data_as_of: seed.data_as_of,
                facts: seed.facts,
            }],
            conversation_summary: None,
            recent_turns: Vec::new(),
            debate_entries: Vec::new(),
            debate_mode: "off".to_string(),
            params,
            constraint_level: constraint_level.to_string(),
            lang: lang.to_string(),
            news_extra: vec![None; NEWS_EXTRA_SLOTS],
            news_extra_slot: 0,
            news_extra_pending_text: None,
            news_extra_auto_inject: false,
            read_depth: "mid".to_string(),
            knowledge_scope: "mid".to_string(),
            response_shape: "talk".to_string(),
            forecast_mode: "soft".to_string(),
            board_entries: Vec::new(),
            board_entry_counter: 0,
            council_participants: Vec::new(),
            council_facilitator: None,
        }
    }

    fn current_data_as_of(&self) -> String {
        self.tickers
            .iter()
            .map(|t| t.data_as_of.as_str())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    }

    // Read views over the ticker table, in table order. Owned so call sites keep the
    // former parallel-array ergonomics ([i], .iter(), .len()); the table is the
    // single source and every read spans all loaded tickers.
    fn ticker_labels(&self) -> Vec<String> {
        self.tickers.iter().map(|t| t.label.clone()).collect()
    }
    fn base_contexts(&self) -> Vec<String> {
        self.tickers
            .iter()
            .map(|t| t.base_context.clone())
            .collect()
    }
    fn technical_texts(&self) -> Vec<String> {
        self.tickers
            .iter()
            .map(|t| t.technical_text.clone())
            .collect()
    }
    fn fundamental_texts(&self) -> Vec<String> {
        self.tickers
            .iter()
            .map(|t| t.fundamental_text.clone())
            .collect()
    }
    fn news_items(&self) -> Vec<Vec<NewsItem>> {
        self.tickers.iter().map(|t| t.news.clone()).collect()
    }
    fn last_indicator_summaries(&self) -> Vec<String> {
        self.tickers
            .iter()
            .map(|t| t.indicator_summary.clone())
            .collect()
    }

    // Test-only column setters: resize the ticker table to the given column and
    // write that one field per row, so tests can seed the table the way they used
    // to seed the old parallel arrays. Production code mutates whole `TickerEntry`
    // rows via the ticker pipeline, never a single column.
    #[cfg(test)]
    fn tickers_resize(&mut self, n: usize) {
        self.tickers.resize_with(n, TickerEntry::default);
    }
    #[cfg(test)]
    fn set_ticker_labels(&mut self, v: Vec<String>) {
        self.tickers_resize(v.len());
        for (t, x) in self.tickers.iter_mut().zip(v) {
            t.label = x;
        }
    }
    #[cfg(test)]
    fn set_base_contexts(&mut self, v: Vec<String>) {
        self.tickers_resize(v.len());
        for (t, x) in self.tickers.iter_mut().zip(v) {
            t.base_context = x;
        }
    }
    #[cfg(test)]
    fn set_technical_texts(&mut self, v: Vec<String>) {
        self.tickers_resize(v.len());
        for (t, x) in self.tickers.iter_mut().zip(v) {
            t.technical_text = x;
        }
    }
    #[cfg(test)]
    fn set_fundamental_texts(&mut self, v: Vec<String>) {
        self.tickers_resize(v.len());
        for (t, x) in self.tickers.iter_mut().zip(v) {
            t.fundamental_text = x;
        }
    }
    #[cfg(test)]
    fn set_news_items(&mut self, v: Vec<Vec<NewsItem>>) {
        self.tickers_resize(v.len());
        for (t, x) in self.tickers.iter_mut().zip(v) {
            t.news = x;
        }
    }

    fn debate_context_budget(&self) -> Option<usize> {
        match self.debate_mode.as_str() {
            "summary" => Some(DEBATE_SUMMARY_CONTEXT_CHARS),
            "claims" => Some(DEBATE_CLAIMS_CONTEXT_CHARS),
            _ => None,
        }
    }

    fn debate_limits(&self) -> Option<(usize, usize)> {
        match self.debate_mode.as_str() {
            "summary" => Some((DEBATE_SUMMARY_MAX_ENTRIES, DEBATE_SUMMARY_ENTRY_CHARS)),
            "claims" => Some((DEBATE_CLAIMS_MAX_ENTRIES, DEBATE_CLAIMS_ENTRY_CHARS)),
            _ => None,
        }
    }

    fn build_debate_text(&self) -> Option<String> {
        let (max_entries, entry_chars) = self.debate_limits()?;
        if self.debate_entries.is_empty() {
            return None;
        }

        let lang = self.lang.as_str();
        let header = match (lang, self.debate_mode.as_str()) {
            ("ja", "claims") => "【Debate Buffer: 他LLMの見解（事実ではない）】\n以下は他LLM出力の抜粋です。XOKSA本体の計算・取得データを基準として、主張・根拠・懸念・未検証事項を批判的に検討してください。",
            ("ja", _) => "【Debate Buffer: 他LLMの見解（事実ではない）】\n以下は他LLM出力の短い抜粋です。XOKSA本体の計算・取得データを基準として、矛盾・過剰推論・不足観点の検討材料として扱ってください。",
            (_, "claims") => "[Debate Buffer: Other LLM Opinions, Not Facts]\nThe following are excerpts from other LLM outputs. Treat XOKSA-computed/retrieved data as SOT and critically review claims, evidence, risks, and unverified points.",
            _ => "[Debate Buffer: Other LLM Opinions, Not Facts]\nThe following are short excerpts from other LLM outputs. Treat XOKSA-computed/retrieved data as SOT and use them only to review contradictions, overreach, and missing perspectives.",
        };

        let entries: Vec<String> = self
            .debate_entries
            .iter()
            .rev()
            .take(max_entries)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .enumerate()
            .map(|(i, entry)| {
                let excerpt = chars_truncate(&entry.response_excerpt, entry_chars);
                match lang {
                    "ja" => format!(
                        "--- 見解 {}: {} / {} ---\nUser: {}\nExcerpt:\n{}",
                        i + 1,
                        entry.provider,
                        entry.model,
                        entry.user,
                        excerpt
                    ),
                    _ => format!(
                        "--- Opinion {}: {} / {} ---\nUser: {}\nExcerpt:\n{}",
                        i + 1,
                        entry.provider,
                        entry.model,
                        entry.user,
                        excerpt
                    ),
                }
            })
            .collect();

        Some(format!("{}\n{}", header, entries.join("\n\n")))
    }

    fn build_latest_debate_text(
        &self,
        current_provider: &str,
        current_model: &str,
        current_engine_id: &str,
    ) -> Option<String> {
        let entry = self.debate_entries.iter().rev().find(|e| {
            e.provider != current_provider
                || e.model != current_model
                || e.engine_id != current_engine_id
        })?;
        let lang = self.lang.as_str();
        let header = match lang {
            "ja" => "【Critique Target: 直近の他LLM見解（事実ではない）】",
            _ => "[Critique Target: Latest Other-LLM Opinion, Not Facts]",
        };
        let excerpt = chars_truncate(&entry.response_excerpt, DEBATE_STORED_RESPONSE_CHARS);
        let data_as_of_line = match &entry.data_as_of {
            Some(s) if !s.is_empty() => match lang {
                "ja" => format!("\nOpinion referenced data as of: {}", s),
                _ => format!("\nOpinion referenced data as of: {}", s),
            },
            _ => match lang {
                "ja" => "\nOpinion referenced data as of: unknown".to_string(),
                _ => "\nOpinion referenced data as of: unknown".to_string(),
            },
        };
        let body = format!(
            "{}\nProvider: {}\nModel: {}\nUser: {}{}\nExcerpt:\n{}",
            header, entry.provider, entry.model, entry.user, data_as_of_line, excerpt
        );
        Some(body)
    }

    fn add_debate_entry(
        &mut self,
        provider: &str,
        model: &str,
        engine_id: &str,
        user: &str,
        response: &str,
    ) {
        if self.debate_mode == "off" {
            return;
        }
        let cleaned = clean_debate_excerpt(response);
        let cleaned_trimmed = cleaned.trim();
        if cleaned_trimmed.is_empty() {
            return;
        }
        let max_entries = match self.debate_mode.as_str() {
            "claims" => DEBATE_CLAIMS_MAX_ENTRIES,
            "summary" => DEBATE_SUMMARY_MAX_ENTRIES,
            _ => return,
        };
        let data_as_of = self.current_data_as_of();
        self.debate_entries.push(DebateEntry {
            provider: provider.to_string(),
            model: model.to_string(),
            engine_id: engine_id.to_string(),
            user: chars_truncate(user, DEBATE_STORED_USER_CHARS),
            response_excerpt: chars_truncate(cleaned_trimmed, DEBATE_STORED_RESPONSE_CHARS),
            data_as_of: if data_as_of.is_empty() {
                None
            } else {
                Some(data_as_of)
            },
        });
        let overflow = self.debate_entries.len().saturating_sub(max_entries);
        if overflow > 0 {
            self.debate_entries.drain(..overflow);
        }
        self.add_board_entry(BoardEntryDraft {
            speaker_type: SpeakerType::Llm,
            role: EntryRole::Proposal,
            provider,
            model,
            engine_id,
            content: cleaned_trimmed,
            parent_entry_id: None,
            round: 0,
        });
    }

    fn set_debate_mode(&mut self, mode: &str) {
        self.debate_mode = match mode {
            "summary" | "claims" => mode.to_string(),
            _ => "off".to_string(),
        };
    }

    fn add_board_entry(&mut self, draft: BoardEntryDraft<'_>) -> u64 {
        self.board_entry_counter += 1;
        let id = self.board_entry_counter;
        self.board_entries.push(BoardEntry {
            id,
            speaker_type: draft.speaker_type,
            role: draft.role,
            provider: draft.provider.to_string(),
            model: draft.model.to_string(),
            engine_id: draft.engine_id.to_string(),
            content: draft.content.to_string(),
            parent_entry_id: draft.parent_entry_id,
            round: draft.round,
        });
        id
    }

    fn show_board(&self, out: &mut ChatOut, lang: &str) {
        out.line(match lang {
            "ja" => format!(
                "Board: {} エントリー (Debate Buffer: mode={}, entries={})",
                self.board_entries.len(),
                self.debate_mode,
                self.debate_entries.len()
            ),
            _ => format!(
                "Board: {} entries (Debate Buffer: mode={}, entries={})",
                self.board_entries.len(),
                self.debate_mode,
                self.debate_entries.len()
            ),
        });
        if self.board_entries.is_empty() {
            out.line(match lang {
                "ja" => "Board は空です。",
                _ => "Board is empty.",
            });
            return;
        }
        for entry in &self.board_entries {
            let role_label = match entry.role {
                EntryRole::UserQuestion => "user-question",
                EntryRole::Proposal => "proposal",
                EntryRole::CouncilProposal => "council-proposal",
                EntryRole::FacilitatorReport => "facilitator-report",
                EntryRole::Critique => "critique",
                EntryRole::Summary => "summary",
            };
            let speaker_label = match entry.speaker_type {
                SpeakerType::User => "user",
                SpeakerType::Llm => "llm",
                SpeakerType::Chair => "chair",
            };
            let parent_label = match entry.parent_entry_id {
                Some(p) => format!(" (re:#{p})"),
                None => String::new(),
            };
            let round_label = if entry.round > 0 {
                format!(" R{}", entry.round)
            } else {
                String::new()
            };
            let model_display = if !entry.engine_id.is_empty() {
                format!("ollama:{}/{}", entry.engine_id, entry.model)
            } else if entry.model.is_empty() {
                entry.provider.clone()
            } else {
                format!("{}/{}", entry.provider, entry.model)
            };
            out.line(format!(
                "--- #{} [{}|{}]{} {}{}  ---",
                entry.id, speaker_label, role_label, round_label, model_display, parent_label
            ));
            out.line(entry.content.clone());
        }
    }

    fn show_debate(&self, out: &mut ChatOut, _lang: &str) {
        out.line(format!(
            "Debate Buffer: mode={} / entries={}",
            self.debate_mode,
            self.debate_entries.len()
        ));
        if self.debate_entries.is_empty() {
            out.line(match _lang {
                "ja" => "Debate Buffer は空です。",
                _ => "Debate Buffer is empty.",
            });
            return;
        }
        for (i, entry) in self.debate_entries.iter().enumerate() {
            let engine_label = if entry.engine_id.is_empty() {
                String::new()
            } else {
                format!(" [{}]", entry.engine_id)
            };
            out.line(format!(
                "--- {}: {} / {}{} ---",
                i + 1,
                entry.provider,
                entry.model,
                engine_label
            ));
            out.line(format!("User: {}", entry.user));
            out.line(entry.response_excerpt.clone());
        }
    }

    fn add_turn(&mut self, user: String, assistant: String) {
        self.recent_turns.push(ChatTurn { user, assistant });
        if self.recent_turns.len() > self.params.max_recent_turns {
            self.roll_summary();
        }
    }

    fn roll_summary(&mut self) {
        let overflow = self
            .recent_turns
            .len()
            .saturating_sub(self.params.max_recent_turns);
        if overflow == 0 {
            return;
        }
        let drained: Vec<_> = self.recent_turns.drain(..overflow).collect();
        let mut parts = Vec::new();
        if let Some(ref existing) = self.conversation_summary {
            parts.push(existing.clone());
        }
        for turn in drained {
            parts.push(format!("Q: {}", turn.user));
            parts.push(format!("A: {}", turn.assistant));
        }
        let combined = parts.join("\n");
        // Drop from the head so newly rolled turns survive when over budget
        self.conversation_summary = Some(chars_truncate_tail(
            &combined,
            self.params.max_summary_chars,
        ));
    }

    fn show_technical(&self, idx: usize, out: &mut ChatOut) {
        for line in self.technical_texts()[idx].lines() {
            out.line(line);
        }
    }

    fn show_fundamental(&self, idx: usize, out: &mut ChatOut, lang: &str) {
        if self.fundamental_texts()[idx].is_empty() {
            out.line(match lang {
                "ja" => "ファンダメンタルデータは読み込まれていません。",
                _ => "Fundamental data not loaded.",
            });
        } else {
            let label = &self.ticker_labels()[idx];
            if !label.is_empty() {
                out.line(format!("[ {} ]", short_display_label(label)));
            }
            for line in self.fundamental_texts()[idx].lines() {
                out.line(line);
            }
        }
    }

    fn show_news(&self, idx: usize, out: &mut ChatOut, lang: &str) {
        let news = &self.news_items()[idx];
        if news.is_empty() {
            out.line(match lang {
                "ja" => "ニュースは読み込まれていません。",
                _ => "No news loaded.",
            });
        } else {
            let label = &self.ticker_labels()[idx];
            if !label.is_empty() {
                out.line(format!("[ {} ]", short_display_label(label)));
            }
            for item in news {
                item.write(out);
            }
        }
    }

    fn context_size_breakdown(&self) -> ContextSizeBreakdown {
        let base_context_chars = self.build_base_text(usize::MAX).chars().count();
        let technical_context_chars = self
            .build_technical_context_text(usize::MAX)
            .chars()
            .count();
        let fundamental_chars = self
            .build_fundamental_text()
            .map(|s| s.chars().count())
            .unwrap_or(0);
        let news_chars = self
            .build_news_text()
            .map(|s| s.chars().count())
            .unwrap_or(0);
        let recent_turns_chars = self
            .build_turns_text_within_budget(usize::MAX)
            .map(|s| s.chars().count())
            .unwrap_or(0);
        let summary_chars = self
            .build_summary_text()
            .map(|s| s.chars().count())
            .unwrap_or(0);
        let debate_chars = self
            .build_debate_text()
            .map(|s| s.chars().count())
            .unwrap_or(0);
        let behavior_chars = self.behavior_instruction_text().chars().count();
        let guard_chars = constraint_text(&self.constraint_level, &self.lang)
            .chars()
            .count()
            + if behavior_chars > 0 {
                behavior_chars + 2 // +2 for SEP overhead
            } else {
                0
            };
        let regular_chat_candidate_chars = technical_context_chars
            + fundamental_chars
            + news_chars
            + recent_turns_chars
            + summary_chars
            + debate_chars
            + guard_chars;
        let initial_prompt_candidate_chars = self.build_initial_analysis_prompt().chars().count();
        let extra_news_chars = build_news_extra_inject_text(&self.news_extra, &self.lang)
            .map(|s| s.chars().count())
            .unwrap_or(0);
        let extra_news_entry_count = self.news_extra.iter().filter(|e| e.is_some()).count();

        ContextSizeBreakdown {
            prompt_budget_chars: self.params.max_context_chars,
            max_recent_turns: self.params.max_recent_turns,
            summary_budget_chars: self.params.max_summary_chars,
            base_context_chars,
            technical_context_chars,
            fundamental_chars,
            news_chars,
            recent_turns_chars,
            summary_chars,
            debate_chars,
            guard_chars,
            regular_chat_candidate_chars,
            initial_prompt_candidate_chars,
            ticker_count: self
                .ticker_labels()
                .iter()
                .filter(|s| !s.is_empty())
                .count(),
            recent_turn_count: self.recent_turns.len(),
            debate_mode: self.debate_mode.clone(),
            debate_entry_count: self.debate_entries.len(),
            extra_news_chars,
            extra_news_entry_count,
            extra_news_auto_inject: self.news_extra_auto_inject,
        }
    }
}

// Parse a number spec like "1", "1-3", "1,3,5", "1-3,5" into sorted 0-based indices.
// Returns Err if any token is out of range (1..=n) or malformed.
fn parse_show_indices(s: &str, n: usize) -> Result<Vec<usize>, ()> {
    let mut set = std::collections::BTreeSet::new();
    for seg in s.split(',') {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        if let Some((lo, hi)) = seg.split_once('-') {
            let lo = lo.trim().parse::<usize>().map_err(|_| ())?;
            let hi = hi.trim().parse::<usize>().map_err(|_| ())?;
            if lo < 1 || hi > n || lo > hi {
                return Err(());
            }
            for i in lo..=hi {
                set.insert(i - 1);
            }
        } else {
            let x = seg.parse::<usize>().map_err(|_| ())?;
            if x < 1 || x > n {
                return Err(());
            }
            set.insert(x - 1);
        }
    }
    if set.is_empty() {
        Err(())
    } else {
        Ok(set.into_iter().collect())
    }
}

fn short_display_label(label: &str) -> &str {
    let ticker = match label.find(" (") {
        Some(pos) => &label[..pos],
        None => label,
    };
    let is_jp =
        ticker.ends_with(".T") || (ticker.len() == 4 && ticker.chars().all(|c| c.is_ascii_digit()));
    if is_jp {
        match label.find(" (") {
            Some(pos) => {
                let name = &label[pos + 2..];
                name.strip_suffix(')').unwrap_or(name)
            }
            None => ticker,
        }
    } else {
        ticker
    }
}

// Returns the usage notes + task instruction to append when fundamental data is present
// in /prompt. The base context was built with fundamental_data=None, so the numbered
// task list omits it; this addendum restores the missing instruction.
fn fundamental_task_addendum(lang: &str) -> &'static str {
    match lang {
        "ja" => "【ファンダメンタル補助情報の扱い】\
\n- ファンダメンタル情報は四半期〜年次更新であり、短期テクニカルスコアを上書きしない。\
\n- PER/PBR/EPS/ROE/売上高等はRust側で計算済みの値のみ使用し、N/Aの項目を推測・補完しない。\
\n- テクニカル分析（短期売買タイミング）とファンダメンタル分析（業績・割高割安）の時間軸の違いを明示する。\
\n\n【追加タスク：ファンダメンタル補助情報（800字以内）】業績水準・収益性・割高割安の補助判断・短期テクニカルとの関係を記述すること。数値は提供されたもののみ使用し、N/Aの項目は推測しない。",
        _ => "[Fundamental Data Usage Notes]\
\n- Fundamental data is updated quarterly to annually and does not override short-term technical scores.\
\n- Use only Rust-computed values for P/E, P/B, EPS, ROE, revenue, etc.; do not infer or supplement N/A items.\
\n- Clearly distinguish the time horizons of technical analysis (short-term timing) and fundamental analysis (earnings/valuation).\
\n\n[Additional task: Fundamental supplementary information (within 800 chars)] Describe earnings level, profitability, valuation assessment, and relationship to short-term technical analysis. Use only provided values; do not infer N/A items.",
    }
}

fn chars_truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else if max == 0 {
        String::new()
    } else {
        s.chars().take(max).collect()
    }
}

// Keeps the tail of `s`, dropping from the head when over budget.
fn chars_truncate_tail(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let total = s.chars().count();
    if total <= max {
        s.to_string()
    } else {
        s.chars().skip(total - max).collect()
    }
}

#[cfg(test)]
mod tests;
