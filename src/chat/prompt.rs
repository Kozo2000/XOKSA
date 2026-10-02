//! `ChatSession` prompt & context assembly: regular chat prompts,
//! `/criticize`, Council/Facilitator prompts, and `/prompt` base builders.
//! An `impl ChatSession` block in a descendant module (parent-private access).

use super::*;

impl ChatSession {
    // Generate behavior instruction text from the 4 user-settable style fields.
    // This is appended to the constraint block in regular chat prompts.
    // Guards (investment_advice, news_body, hallucination) are in constraint_text and are NOT affected.
    pub(super) fn behavior_instruction_text(&self) -> String {
        let parts: [&str; 3] = match self.lang.as_str() {
            "ja" => [
                // depth — 与えられたデータをどこまで読み込むか
                match self.read_depth.as_str() {
                    "shallow" => "解釈の深さ: 指標値と一般的な意味の提示にとどめること。",
                    "deep" => "解釈の深さ: 引用する指標は、その数値が示す市場参加者の行動（買い戻し・利確・需給など）と価格への影響まで、与えられた数値を根拠に具体的に説明すること。方向性を明言し、強弱の主因を特定し、見立てが変わる分岐条件と、反対方向のシナリオが成立する条件まで整理すること。",
                    _ => "解釈の深さ: 引用する指標は、その数値が示す市場参加者の行動（買い戻し・利確・需給など）と価格への影響まで、与えられた数値を根拠に具体的に説明すること。あわせて、見立てが変わる分岐条件（どの水準を割れば・超えれば判断が変わるか）を示すこと。データが明確に支持する場合は方向性を述べてよい。",
                },
                // scope — 入力の外の一般知識をどこまで使うか（肯定形）
                match self.knowledge_scope.as_str() {
                    "narrow" => "知識の活用: 回答は入力データの範囲で組み立てること。",
                    "wide" => "知識の活用: 関連する業界・マクロ・季節性・地政学などの文脈（例: 夏枯れ、供給網の混乱）は、ユーザーが挙げていなくても一般的な知識で自発的に補い、それがこの銘柄の見立てにどう関わるかまで述べること。安定した企業構造（資本関係・親子会社・事業内容）などの確立した事実は一般知識で補ってよい（確定データとは区別）。数値・速報的な個別事実・ニュースの具体的内容は入力にあるものを使う。",
                    _ => "知識の活用: 一般的な金融・市場・テクニカルの概念や用語を用いて説明・推論してよい。安定した企業構造などの確立した事実は一般知識で補ってよい（確定データとは区別）。数値・速報的な個別事実・ニュースの具体的内容は入力にあるものを使う。",
                },
                // shape — 回答形式（既定 talk＝会話体）
                match self.response_shape.as_str() {
                    "points" => "回答形式: 確認ポイントや重要項目を整理した箇条書きで返すこと。",
                    "scenario" => "回答形式:「見立て」「成立条件」「崩れる条件」「確認ライン」を分けて返すこと。",
                    _ => "回答形式: 結論・見解を通常の文章（会話体）で述べ、質問に関係する指標だけを文中に織り込むこと。",
                },
            ],
            _ => [
                match self.read_depth.as_str() {
                    "shallow" => "Interpretation depth: state indicator values and their standard meaning only.",
                    "deep" => "Interpretation depth: for each indicator you cite, explain the market-participant behavior its value implies (short-covering, profit-taking, supply/demand) and its price impact, grounded in the provided values. State a directional conclusion, identify the main driver of strength or weakness, and lay out the levels at which the view changes and the conditions under which the opposite scenario takes hold.",
                    _ => "Interpretation depth: for each indicator you cite, explain the market-participant behavior its value implies (short-covering, profit-taking, supply/demand) and its price impact, grounded in the provided values. Also state the pivot conditions at which the view changes (which level, broken or reclaimed, flips the read). Where the data clearly supports it, you may state a direction.",
                },
                match self.knowledge_scope.as_str() {
                    "narrow" => "Knowledge scope: build the answer from the provided input.",
                    "wide" => "Knowledge scope: bring in relevant industry, macro, seasonal, or geopolitical context (e.g. a summer lull, a supply-chain disruption) from general knowledge on your own initiative — even when the user has not raised it — and say how each bears on the read for this stock. You may fill in well-established facts such as a company's ownership structure from general knowledge (kept distinct from confirmed data). Take specific figures, high-velocity ticker facts, and concrete news content only from the input.",
                    _ => "Knowledge scope: you may use general finance, market, and technical concepts and reasoning, and well-established facts such as a company's ownership structure from general knowledge (kept distinct from confirmed data); take specific figures, high-velocity ticker facts, and concrete news content only from the input.",
                },
                match self.response_shape.as_str() {
                    "points" => "Response format: organize checkpoints and key items as a structured list.",
                    "scenario" => "Response format: structure your response as: view / conditions for validity / conditions for invalidation / confirmation checkpoints.",
                    _ => "Response format: answer in ordinary prose (conversational), weaving in only the indicators relevant to the question.",
                },
            ],
        };
        // Always-on default (not a knob): answer the user's actual question first.
        let lead = if self.lang == "ja" {
            "上記のデータは答えの根拠として参照する情報です。まず最初の一文でユーザーの問いへの結論（方向・見立て）をそのまま述べ、続けてその結論を支える主因の数値だけを簡潔に添えること。会話として、聞かれたことにまっすぐ答えること。"
        } else {
            "The data above is reference to ground your answer. Open with a one-sentence direct answer to the user's exact question — the conclusion or lean — then add only the few figures that support it. Answer the thing that was asked, as a conversation."
        };
        // Always-on default (not a knob): balance the three input elements. The knobs
        // tune HOW deep/wide the reading goes; the composition below applies regardless.
        let balance = if self.lang == "ja" {
            "回答の基本構成: 入力にテクニカル・ファンダメンタル・ニュースが存在する場合、質問がどれか一方に偏っていても、3要素をできる限りバランスよく織り込んで全体像を示すこと。ニュースは見出し（確定入力）が示す話題とセンチメントとして扱い、本文未確認である旨を明示し、記事に触れるときはURLを併記すること。ファンダメンタルはスコア外の客観情報としてテクニカルと突き合わせること。入力に存在しない要素は創作せず、無い旨のみ述べること。"
        } else {
            "Answer composition: when the input contains technicals, fundamentals, and news, weave all three into the answer in reasonable balance — even when the question leans on only one of them. Treat news as the topics and sentiment its headlines (confirmed input) show, state that article bodies are unverified, and include the URL when citing an article. Treat fundamentals as objective context outside the score, checked against the technicals. Never fabricate an element absent from the input — say it is absent instead."
        };
        [lead, balance]
            .into_iter()
            .chain(parts.iter().filter(|s| !s.is_empty()).copied())
            .collect::<Vec<_>>()
            .join(" ")
    }

    // Build the prompt for the current turn.
    //
    // BUDGET PRIORITY (highest → lowest — what survives when budget is tight):
    //   1. constraint_text + behavior + User: {user_input}  ← mandatory tail, always present
    //   2. recent_turns  — reserved first from body_budget; oldest skipped if tight
    //   3. base_context  — technical indicator snapshot; gets full remaining budget, trimmed from end
    //   4. fundamental   — supplementary; capped at remaining/2 so base always retains half
    //   5. debate buffer — included only when enabled and space remains
    //   6. conversation_summary — all-or-nothing; included only if it fits
    //   7. news_items    — truncated to whatever budget remains; if fully dropped for
    //                      budget, a compact "news_titles exist" hint takes the news slot
    //   8. news_extra inject — see note below
    //
    // DISPLAY ORDER (LLM reads this sequence):
    //   base → fundamental → debate → news → summary → turns → [inject] → constraint → user
    //
    // Note on news_extra inject: when auto-inject is on, inject text is placed before the
    // mandatory tail. It is capped at max/2 so it cannot crowd out SOT base context.
    pub(super) fn build_prompt(&self, user_input: &str) -> String {
        const SEP: &str = "\n\n";

        let max = self.params.max_context_chars;

        // Mandatory tail — always at the very end.
        // news_extra inject is prepended here but capped at max/2 so it cannot crowd out
        // SOT base context (which lives in the body budget computed below).
        let user_line = format!("User: {}", user_input);
        let inject_prefix = if self.news_extra_auto_inject {
            build_news_extra_inject_text(&self.news_extra, &self.lang)
                .map(|s| {
                    let capped = chars_truncate(&s, max / 2);
                    format!("{}{}", capped, SEP)
                })
                .unwrap_or_default()
        } else {
            self.news_extra_pending_text
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| format!("{}{}", s, SEP))
                .unwrap_or_default()
        };
        let behavior = self.behavior_instruction_text();
        let tail = format!(
            "{}{}{}{}{}{}{}",
            inject_prefix,
            constraint_text(&self.constraint_level, &self.lang),
            SEP,
            forecast_clause(&self.forecast_mode, &self.lang),
            SEP,
            if behavior.is_empty() {
                String::new()
            } else {
                format!("{}{}", behavior, SEP)
            },
            user_line
        );
        let tail_chars = tail.chars().count();

        // If max is somehow smaller than the mandatory tail, return just the tail
        if tail_chars + SEP_OVERHEAD >= max {
            return tail;
        }

        // Budget for the body that appears before "\n\n{tail}"
        let body_budget = max.saturating_sub(tail_chars).saturating_sub(SEP_OVERHEAD);

        // Pack recent turns newest-first within body_budget; oldest dropped if they don't fit
        let turns_text = self.build_turns_text_within_budget(body_budget);
        let turns_chars = turns_text
            .as_ref()
            .map(|s| s.chars().count() + 2)
            .unwrap_or(0);

        // Remaining budget after turns for base, fundamental, summary, news
        let remaining = body_budget.saturating_sub(turns_chars);

        let news_text = self.build_news_text();
        let summary_text = self.build_summary_text();
        let fundamental_text = self.build_fundamental_text();
        let debate_text = self.build_debate_text();

        // Fundamental is supplementary — cap its reserve at half of `remaining`
        // so base/technical context always retains at least half the budget.
        let fundamental_max = remaining / 2;
        let fundamental_reserve = fundamental_text
            .as_ref()
            .map(|s| (s.chars().count() + 2).min(fundamental_max))
            .unwrap_or(0);

        // Truncate fundamental_text to its reserved budget before assembly.
        let fundamental_text =
            fundamental_text.map(|s| chars_truncate(&s, fundamental_reserve.saturating_sub(2)));

        // Base gets remaining budget minus fundamental reservation.
        // Use technical_texts (indicator data without format instructions) so that
        // regular chat turns receive raw data context, not the /prompt analysis template.
        let base_budget = remaining.saturating_sub(fundamental_reserve);
        let base = self.build_technical_context_text(base_budget);
        let base_chars = base.chars().count();

        // Space left after base + fundamental for debate, summary and news
        let after_base = remaining
            .saturating_sub(if base_chars > 0 { base_chars + 2 } else { 0 })
            .saturating_sub(fundamental_reserve);

        let debate_budget = self
            .debate_context_budget()
            .map(|max| max.min(after_base))
            .unwrap_or(0);
        let debate_included = debate_text.and_then(|s| {
            if debate_budget == 0 {
                None
            } else {
                Some(chars_truncate(&s, debate_budget))
            }
        });
        let debate_used = debate_included
            .as_ref()
            .map(|s| s.chars().count() + 2)
            .unwrap_or(0);
        let after_debate = after_base.saturating_sub(debate_used);

        // Include summary if it fits (all-or-nothing: summary is a compressed digest)
        let summary_included = summary_text.filter(|s| s.chars().count() <= after_debate);
        let summary_used = summary_included
            .as_ref()
            .map(|s| s.chars().count() + 2)
            .unwrap_or(0);

        // Include news truncated to whatever budget remains after summary
        let news_budget = after_debate.saturating_sub(summary_used);
        let news_included = news_text.and_then(|s| {
            if news_budget == 0 {
                None
            } else {
                Some(chars_truncate(&s, news_budget))
            }
        });
        // When the confirmed news section is fully dropped for budget, keep a compact
        // availability hint so the model does not answer "no news in the input" while the
        // news panel actually holds titles. Titles are omitted; fabrication is forbidden.
        let news_hint = if news_included.is_none() {
            self.build_news_availability_hint()
        } else {
            None
        };

        // Assemble body in order: base → fundamental → debate → news → summary → turns
        let mut parts: Vec<String> = Vec::new();
        if !base.is_empty() {
            parts.push(base);
        }
        if let Some(s) = fundamental_text {
            parts.push(s);
        }
        if let Some(s) = debate_included {
            parts.push(s);
        }
        if let Some(s) = news_included {
            parts.push(s);
        } else if let Some(s) = news_hint {
            parts.push(s);
        }
        if let Some(s) = summary_included {
            parts.push(s);
        }
        if let Some(s) = turns_text {
            parts.push(s);
        }

        if parts.is_empty() {
            tail
        } else {
            format!("{}{}{}", parts.join(SEP), SEP, tail)
        }
    }

    pub(super) fn build_criticize_prompt(
        &self,
        current_provider: &str,
        current_model: &str,
        current_engine_id: &str,
    ) -> String {
        const SEP: &str = "\n\n";

        let mandatory = [
            constraint_text(&self.constraint_level, &self.lang).to_string(),
            criticize_task_text(&self.lang).to_string(),
            self.build_sot_coverage_text(),
            self.build_latest_debate_text(current_provider, current_model, current_engine_id)
                .unwrap_or_else(|| criticize_empty_debate_text(&self.lang).to_string()),
        ];
        let mandatory_text = mandatory.join(SEP);
        let max = self.params.max_context_chars;
        if mandatory_text.chars().count() + SEP_OVERHEAD >= max {
            return chars_truncate(&mandatory_text, max);
        }

        let remaining = max
            .saturating_sub(mandatory_text.chars().count())
            .saturating_sub(SEP_OVERHEAD);
        let mut optional_sections: Vec<String> = Vec::new();
        let technical = self.build_technical_context_text(remaining);
        if !technical.is_empty() {
            optional_sections.push(technical);
        }
        if let Some(fundamental) = self.build_fundamental_text() {
            optional_sections.push(fundamental);
        }
        if let Some(news) = self.build_news_text() {
            optional_sections.push(news);
        }

        if optional_sections.is_empty() || remaining == 0 {
            return mandatory_text;
        }

        let per_section = (remaining / optional_sections.len()).max(1);
        let optional_text = optional_sections
            .into_iter()
            .map(|section| chars_truncate(&section, per_section))
            .collect::<Vec<_>>()
            .join(SEP);

        format!("{}{}{}", mandatory_text, SEP, optional_text)
    }

    // Pack recent turns from newest to oldest, stopping when adding a turn would exceed budget.
    // Each turn is "User: {q}\nAI: {a}". Turns are joined by "\n".
    pub(super) fn build_turns_text_within_budget(&self, budget: usize) -> Option<String> {
        if self.recent_turns.is_empty() || budget == 0 {
            return None;
        }
        let mut collected: Vec<&ChatTurn> = Vec::new();
        let mut used = 0usize;
        for turn in self.recent_turns.iter().rev() {
            // "User: " = 6, "\nAI: " = 5 → fixed overhead = 11 per turn
            let turn_chars = 11 + turn.user.chars().count() + turn.assistant.chars().count();
            // Inter-turn "\n" separator (not needed for the first collected turn)
            let sep = if collected.is_empty() { 0 } else { 1 };
            if used + turn_chars + sep > budget {
                break;
            }
            collected.push(turn);
            used += turn_chars + sep;
        }
        if collected.is_empty() {
            return None;
        }
        // Reverse to chronological order
        collected.reverse();
        let lines: Vec<String> = collected
            .iter()
            .flat_map(|t| vec![format!("User: {}", t.user), format!("AI: {}", t.assistant)])
            .collect();
        Some(lines.join("\n"))
    }

    pub(super) fn build_summary_text(&self) -> Option<String> {
        self.conversation_summary
            .as_ref()
            .map(|s| format!("[Conversation Summary]\n{}", s))
    }

    pub(super) fn build_news_text(&self) -> Option<String> {
        let empty_session = self.ticker_labels().len() == 1 && self.ticker_labels()[0].is_empty();
        if empty_session {
            return None;
        }
        let n = self.ticker_labels().len();
        let mut parts: Vec<String> = Vec::new();
        for (i, news) in self.news_items().iter().enumerate() {
            if news.is_empty() {
                continue;
            }
            let lines: Vec<String> = news.iter().map(NewsItem::prompt_line).collect();
            let header = if n > 1 {
                let label = &self.ticker_labels()[i];
                let disp = short_display_label(label);
                format!("[News Index: {}]\n", disp)
            } else {
                "[News Index]\n".to_string()
            };
            let boundary = match self.lang.as_str() {
                "ja" => "注意: ニュースはタイトルとURLのみです。記事本文は取得・読解していません。本文内容の推測や価格影響の断定は禁止。ただし見出しそのものは確定入力であり、積極的に使うこと: ニュースがある場合は、質問がテクニカル中心でも、見出しが示す注目テーマとセンチメント（前向き・中立・警戒）を材料環境として必ず織り込む。個別記事に触れるときは本文未確認である旨を明示し、URLを併記する。",
                _ => "Note: News items are titles and URLs only. Article bodies are not fetched or read. Do not infer body content or assert price impact. The headlines themselves, however, are confirmed input — use them proactively: whenever news is present, weave the themes and sentiment the headlines show (positive / neutral / cautionary) into the analysis as the surrounding material environment, even for technical-centric questions. When citing an article, state that the body is unverified and include its URL.",
            };
            parts.push(format!("{}{}\n{}", header, boundary, lines.join("\n")));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n\n"))
        }
    }

    // Compact fallback for when the full news section is dropped from a chat prompt for
    // budget: report only that confirmed news titles exist (per ticker count), so the model
    // does not answer "no news in the input" while the news panel holds articles. The titles
    // themselves are omitted; the model must not fabricate them.
    pub(super) fn build_news_availability_hint(&self) -> Option<String> {
        let empty_session = self.ticker_labels().len() == 1 && self.ticker_labels()[0].is_empty();
        if empty_session {
            return None;
        }
        let counts: Vec<String> = self
            .ticker_labels()
            .iter()
            .enumerate()
            .filter(|(_, label)| !label.is_empty())
            .filter_map(|(i, label)| {
                let n = self.news_items().get(i).map_or(0, Vec::len);
                (n > 0).then(|| format!("{}={}", short_display_label(label), n))
            })
            .collect();
        if counts.is_empty() {
            return None;
        }
        let detail = counts.join(", ");
        Some(match self.lang.as_str() {
            "ja" => format!(
                "[ニュース索引: 一覧省略] この会話には確定ニュース見出しが存在します（news_titles: {}）。ただし今ターンは文脈予算の都合で見出し一覧を省略しています。ニュースを問われても「入力にニュースが無い」とは答えず、今回は一覧が省略された旨のみ伝え、単一銘柄に絞るか改めて尋ねるよう促すこと。見出しやURLを創作しないこと。",
                detail
            ),
            _ => format!(
                "[News Index: list omitted] Confirmed news titles exist in this session (news_titles: {}), but the title list was omitted this turn due to context budget. If asked about news, do not answer that there is no news in the input; state only that the list was omitted this turn and suggest narrowing to a single ticker or asking again. Never fabricate headlines or URLs.",
                detail
            ),
        })
    }

    pub(super) fn build_sot_coverage_text(&self) -> String {
        let lang = self.lang.as_str();
        let data_as_of = self.current_data_as_of();
        let data_time_line = if data_as_of.is_empty() {
            match lang {
                "ja" => "session_data_as_of=unknown".to_string(),
                _ => "session_data_as_of=unknown".to_string(),
            }
        } else {
            match lang {
                "ja" => format!("session_data_as_of={}", data_as_of),
                _ => format!("session_data_as_of={}", data_as_of),
            }
        };
        let mut lines = match lang {
            "ja" => vec![
                "【xoksa確認範囲】".to_string(),
                "news_body=not_fetched_or_read（URLは確認先であり、本文を読んだ証拠ではない）".to_string(),
                data_time_line,
            ],
            _ => vec![
                "[SOT Coverage]".to_string(),
                "news_body=not_fetched_or_read (URLs are source pointers, not evidence that bodies were read)".to_string(),
                data_time_line,
            ],
        };

        for (i, label) in self.ticker_labels().iter().enumerate() {
            if label.is_empty() {
                continue;
            }
            let technical = self
                .technical_texts()
                .get(i)
                .is_some_and(|text| !text.trim().is_empty());
            let fundamental = self
                .fundamental_texts()
                .get(i)
                .is_some_and(|text| !text.trim().is_empty());
            let news_count = self.news_items().get(i).map_or(0, Vec::len);
            let display = short_display_label(label);
            lines.push(format!(
                "{}: technical={} fundamental={} news_titles={}",
                display,
                yes_no(technical),
                yes_no(fundamental),
                news_count
            ));
        }

        if lines.len() == 2 {
            lines.push(match lang {
                "ja" => "loaded_tickers=0".to_string(),
                _ => "loaded_tickers=0".to_string(),
            });
        }

        lines.join("\n")
    }

    pub(super) fn build_fundamental_text(&self) -> Option<String> {
        let labels = self.ticker_labels();
        let empty_session = labels.len() == 1 && labels[0].is_empty();
        if empty_session {
            return None;
        }
        let n = labels.len();
        let fundamental_texts = self.fundamental_texts();
        let entries: Vec<(usize, &str)> = fundamental_texts
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.is_empty())
            .map(|(i, t)| (i, t.as_str()))
            .collect();
        if entries.is_empty() {
            return None;
        }
        let lang = self.lang.as_str();
        let parts: Vec<String> = entries
            .into_iter()
            .map(|(i, text)| {
                if n > 1 {
                    let label = &labels[i];
                    let disp = short_display_label(label);
                    let header = match lang {
                        "ja" => format!("--- ファンダメンタル補助 {} {} ---", i + 1, disp),
                        _ => format!("--- Fundamental Data {} {} ---", i + 1, disp),
                    };
                    format!("{}\n{}", header, text)
                } else {
                    text.to_string()
                }
            })
            .collect();
        Some(parts.join("\n\n"))
    }

    pub(super) fn build_session_history(&self, before_id: u64) -> Option<String> {
        let mut groups: Vec<(String, String)> = Vec::new();
        let mut current_q: Option<String> = None;
        let mut last_report: Option<String> = None;
        for e in self.board_entries.iter().filter(|e| e.id < before_id) {
            match e.role {
                EntryRole::UserQuestion => {
                    if let Some(q) = current_q.take() {
                        if let Some(r) = last_report.take() {
                            groups.push((q, r));
                        }
                    }
                    current_q = Some(e.content.clone());
                    last_report = None;
                }
                EntryRole::FacilitatorReport => {
                    last_report = Some(e.content.clone());
                }
                _ => {}
            }
        }
        if let Some(q) = current_q {
            if let Some(r) = last_report {
                groups.push((q, r));
            } else {
                // Facilitator was skipped (all participants failed); record the question as unanswered.
                let no_answer = match self.lang.as_str() {
                    "ja" => "(全参加者がAPIエラーにより回答できなかったため、Facilitatorのまとめはありません)".to_string(),
                    _ => "(All participants failed due to API errors; no Facilitator summary available)".to_string(),
                };
                groups.push((q, no_answer));
            }
        }
        if groups.is_empty() {
            return None;
        }
        let header = match self.lang.as_str() {
            "ja" => "[会議履歴]",
            _ => "[Session History]",
        };
        let parts: Vec<String> = groups
            .iter()
            .enumerate()
            .map(|(i, (q, r))| {
                let (q_label, r_label) = match self.lang.as_str() {
                    "ja" => ("質問", "ファシリテータまとめ"),
                    _ => ("Question", "Facilitator Summary"),
                };
                format!("Q{}: {} {}\n{}: {}", i + 1, q_label, q, r_label, r)
            })
            .collect();
        Some(format!("{}\n{}", header, parts.join("\n\n")))
    }

    pub(super) fn truncate_section(section: &PromptSection, max: usize) -> String {
        match section.mode {
            TruncateMode::Head => chars_truncate(&section.text, max),
            TruncateMode::Tail => chars_truncate_tail(&section.text, max),
        }
    }

    pub(super) fn build_budgeted_prompt(
        &self,
        optional_sections: Vec<PromptSection>,
        mandatory_sections: Vec<String>,
    ) -> String {
        const SEP: &str = "\n\n";
        let max = self.params.max_context_chars;
        let mandatory = mandatory_sections
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>()
            .join(SEP);
        let mandatory_chars = mandatory.chars().count();

        if mandatory_chars >= max {
            return chars_truncate_tail(&mandatory, max);
        }

        let sep_chars = SEP.chars().count();
        let mut remaining = max.saturating_sub(mandatory_chars);
        let mut parts: Vec<String> = Vec::new();

        for section in optional_sections {
            if section.text.trim().is_empty() || remaining <= sep_chars {
                continue;
            }
            let section_budget = remaining - sep_chars;
            let truncated = Self::truncate_section(&section, section_budget);
            if truncated.is_empty() {
                continue;
            }
            let used = truncated.chars().count() + sep_chars;
            parts.push(truncated);
            remaining = remaining.saturating_sub(used);
        }

        parts.push(mandatory);
        parts.join(SEP)
    }

    pub(super) fn council_history_section(&self, user_question_id: u64) -> Option<PromptSection> {
        self.build_session_history(user_question_id)
            .map(|text| PromptSection {
                text,
                mode: TruncateMode::Tail,
            })
    }

    pub(super) fn council_sot_section(&self) -> Option<PromptSection> {
        self.build_council_sot_text().map(|text| PromptSection {
            text,
            mode: TruncateMode::Head,
        })
    }

    pub(super) fn council_news_section(&self) -> Option<PromptSection> {
        self.build_news_text().map(|text| PromptSection {
            text,
            mode: TruncateMode::Head,
        })
    }

    pub(super) fn build_council_prompt(
        &self,
        question: &str,
        prior_facilitator: Option<&str>,
        user_question_id: u64,
    ) -> String {
        let mut optional_sections: Vec<PromptSection> = Vec::new();
        if let Some(sot) = self.council_sot_section() {
            optional_sections.push(sot);
        }
        if let Some(fc) = prior_facilitator {
            let fc_block = match self.lang.as_str() {
                "ja" => format!("[ファシリテータレポート]\n{}", fc),
                _ => format!("[Facilitator Report]\n{}", fc),
            };
            optional_sections.push(PromptSection {
                text: fc_block,
                mode: TruncateMode::Tail,
            });
        }
        if let Some(h) = self.council_history_section(user_question_id) {
            optional_sections.push(h);
        }
        if let Some(news) = self.council_news_section() {
            optional_sections.push(news);
        }
        let role_instruction = match self.lang.as_str() {
            "ja" => "あなたはこの会議の参加者です。以下の議題に対し、専門的見解を述べてください。\n出力形式はプレーンテキストで、次のラベルを使ってください。\n見解:\n<主張>\n根拠:\n<根拠・データへの言及>\n懸念点:\n<リスク・反論>".to_string(),
            _ => "You are a participant in this council. State your expert opinion on the topic below.\nUse plain text with these labels:\nPosition:\n<your stance>\nReasoning:\n<evidence and data references>\nConcerns:\n<risks or counterarguments>".to_string(),
        };
        self.build_budgeted_prompt(
            optional_sections,
            vec![
                constraint_text(&self.constraint_level, &self.lang).to_string(),
                role_instruction,
                question.to_string(),
            ],
        )
    }

    pub(super) fn build_facilitator_prompt(
        &self,
        question: &str,
        participant_round: u32,
        user_question_id: u64,
        is_final: bool,
        failed_count: usize,
    ) -> String {
        let mut optional_sections: Vec<PromptSection> = Vec::new();
        let opinions: Vec<String> = self
            .board_entries
            .iter()
            .filter(|e| {
                e.role == EntryRole::CouncilProposal
                    && e.round == participant_round
                    && e.id > user_question_id
            })
            .map(|e| {
                let display = if e.provider == "ollama" && !e.engine_id.is_empty() {
                    format!("ollama:{}/{}", e.engine_id, e.model)
                } else if e.model.is_empty() {
                    e.provider.clone()
                } else {
                    format!("{}/{}", e.provider, e.model)
                };
                format!("--- {} ---\n{}", display, e.content)
            })
            .collect();
        let opinions_text = opinions.join("\n\n");
        let question_block = match self.lang.as_str() {
            "ja" => format!("[原質問]\n{}", question),
            _ => format!("[Original Question]\n{}", question),
        };
        let opinions_block = match self.lang.as_str() {
            "ja" => format!(
                "[Round {} 参加者の見解]\n{}",
                participant_round, opinions_text
            ),
            _ => format!(
                "[Round {} Participant Opinions]\n{}",
                participant_round, opinions_text
            ),
        };
        if !opinions_block.trim().is_empty() {
            optional_sections.push(PromptSection {
                text: opinions_block,
                mode: TruncateMode::Tail,
            });
        }
        if let Some(sot) = self.council_sot_section() {
            optional_sections.push(sot);
        }
        if let Some(h) = self.council_history_section(user_question_id) {
            optional_sections.push(h);
        }
        if let Some(news) = self.council_news_section() {
            optional_sections.push(news);
        }
        let mut mandatory_sections = vec![
            constraint_text(&self.constraint_level, &self.lang).to_string(),
            question_block,
        ];
        if failed_count > 0 {
            let note = match self.lang.as_str() {
                "ja" => format!(
                    "[注意] このラウンドで {}名の参加者がAPIエラーにより回答できませんでした。上記の見解は一部の参加者のみを反映しています。",
                    failed_count
                ),
                _ => format!(
                    "[Note] {} participant(s) could not respond due to API errors this round. The opinions above reflect only the participants who succeeded.",
                    failed_count
                ),
            };
            mandatory_sections.push(note);
        }
        let task = if is_final {
            match self.lang.as_str() {
                "ja" => "あなたはこの会議のファシリテータです。これは最終ラウンドです。自分の意見を述べず、中立的に整理することだけに徹してください（同一モデルが参加者を兼ねている場合も同様）。\n参加者は、意見の前に付いている表示名（例: openai/gpt-5.6-terra、ollama:local2/gpt-oss:20b）でそのまま呼んでください。「参加者1」のような番号や匿名の呼称は使わないこと。\n出力形式はプレーンテキストで、次のラベルを使ってください。\n各参加者の立場:\n<参加者ごとの主張を要約>\n合意点:\n<共通して認識されている点>\n対立点・緊張点:\n<相違または緊張している点>\n結論・総合判断:\n<各参加者の見解を踏まえた会議全体のまとめ。次ラウンドへの問いは含めない>".to_string(),
                _ => "You are the facilitator of this council. This is the final round. Do not state your own opinions — focus solely on neutral synthesis (even if you are also a participant in this session).\nRefer to each participant by the display label shown above their opinion (e.g. openai/gpt-5.6-terra, ollama:local2/gpt-oss:20b). Do not use numbers or anonymous labels such as \"Participant 1\".\nUse plain text with these labels:\nEach Participant's Position:\n<summarize each participant's stance>\nPoints of Agreement:\n<what participants commonly recognize>\nPoints of Disagreement / Tension:\n<where positions diverge or conflict>\nConclusion:\n<overall synthesis of the council's discussion. Do not include questions for further rounds>".to_string(),
            }
        } else {
            match self.lang.as_str() {
                "ja" => "あなたはこの会議のファシリテータです。自分の意見を述べず、中立的に整理することだけに徹してください（同一モデルが参加者を兼ねている場合も同様）。\n参加者は、意見の前に付いている表示名（例: openai/gpt-5.6-terra、ollama:local2/gpt-oss:20b）でそのまま呼んでください。「参加者1」のような番号や匿名の呼称は使わないこと。\n出力形式はプレーンテキストで、次のラベルを使ってください。\n各参加者の立場:\n<参加者ごとの主張を要約>\n合意点:\n<共通して認識されている点>\n対立点・緊張点:\n<相違または緊張している点>\n次ラウンドへの問い:\n<次のラウンドで参加者が検討すべき焦点>".to_string(),
                _ => "You are the facilitator of this council. Do not state your own opinions — focus solely on neutral synthesis (even if you are also a participant in this session).\nRefer to each participant by the display label shown above their opinion (e.g. openai/gpt-5.6-terra, ollama:local2/gpt-oss:20b). Do not use numbers or anonymous labels such as \"Participant 1\".\nUse plain text with these labels:\nEach Participant's Position:\n<summarize each participant's stance>\nPoints of Agreement:\n<what participants commonly recognize>\nPoints of Disagreement / Tension:\n<where positions diverge or conflict>\nQuestions for the Next Round:\n<focal questions for participants to address next>".to_string(),
            }
        };
        mandatory_sections.push(task);
        self.build_budgeted_prompt(optional_sections, mandatory_sections)
    }

    pub(super) fn build_council_sot_text(&self) -> Option<String> {
        let labels = self.ticker_labels();
        let technical_texts = self.technical_texts();
        let entries: Vec<(usize, &str)> = technical_texts
            .iter()
            .enumerate()
            .filter(|(_, text)| !text.trim().is_empty())
            .map(|(i, text)| (i, text.as_str()))
            .collect();
        if entries.is_empty() {
            return None;
        }

        let total_budget = COUNCIL_SOT_CHARS.saturating_mul(entries.len().max(1));
        let sep_overhead = entries.len().saturating_sub(1).saturating_mul(2);
        let per_budget = total_budget.saturating_sub(sep_overhead) / entries.len();
        let parts: Vec<String> = entries
            .iter()
            .map(|(i, text)| {
                let label = labels
                    .get(*i)
                    .map(|s| short_display_label(s).to_string())
                    .unwrap_or_else(|| (i + 1).to_string());
                let header = format!("[{}]\n", label);
                let header_len = header.chars().count();
                if header_len >= per_budget {
                    chars_truncate(text, per_budget)
                } else {
                    format!(
                        "{}{}",
                        header,
                        chars_truncate(text, per_budget - header_len)
                    )
                }
            })
            .collect();
        Some(parts.join("\n\n"))
    }

    pub(super) fn build_council_summary_prompt(&self) -> Option<String> {
        // Group Board entries by UserQuestion so the summary knows which answer belongs to which question
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        let mut current_q: Option<String> = None;
        let mut current_ops: Vec<String> = Vec::new();
        for entry in &self.board_entries {
            match entry.role {
                EntryRole::UserQuestion => {
                    if let Some(q) = current_q.take() {
                        if !current_ops.is_empty() {
                            groups.push((q, std::mem::take(&mut current_ops)));
                        }
                    }
                    current_q = Some(entry.content.clone());
                }
                EntryRole::CouncilProposal | EntryRole::FacilitatorReport
                    if current_q.is_some() =>
                {
                    let display = if entry.provider == "ollama" && !entry.engine_id.is_empty() {
                        format!("ollama:{}/{}", entry.engine_id, entry.model)
                    } else if entry.model.is_empty() {
                        entry.provider.clone()
                    } else {
                        format!("{}/{}", entry.provider, entry.model)
                    };
                    let label = match entry.role {
                        EntryRole::FacilitatorReport => match self.lang.as_str() {
                            "ja" => format!("[ファシリテータ R{}] {}", entry.round, display),
                            _ => format!("[Facilitator R{}] {}", entry.round, display),
                        },
                        _ => format!("[R{}] {}", entry.round, display),
                    };
                    current_ops.push(format!("--- {} ---\n{}", label, entry.content));
                }
                _ => {}
            }
        }
        if let Some(q) = current_q.take() {
            if !current_ops.is_empty() {
                groups.push((q, current_ops));
            }
        }

        let opinions_text = if !groups.is_empty() {
            let g_texts: Vec<String> = groups
                .iter()
                .enumerate()
                .map(|(i, (q, ops))| {
                    let q_header = match self.lang.as_str() {
                        "ja" => format!("【質問{}】{}", i + 1, q),
                        _ => format!("[Q{}] {}", i + 1, q),
                    };
                    format!("{}\n\n{}", q_header, ops.join("\n\n"))
                })
                .collect();
            g_texts.join("\n\n===\n\n")
        } else {
            // Fall back to debate_entries when board has no council data
            let debate_ops: Vec<String> = self
                .debate_entries
                .iter()
                .map(|e| {
                    format!(
                        "Provider: {} / Model: {}\nUser Q: {}\nExcerpt: {}",
                        e.provider, e.model, e.user, e.response_excerpt
                    )
                })
                .collect();
            if debate_ops.is_empty() {
                return None;
            }
            debate_ops.join("\n\n---\n\n")
        };

        let task = match self.lang.as_str() {
            "ja" => "上記の質問と各LLMの見解を客観的に整理してください。質問ごとの合意点・相違点・確認が必要な点を簡潔にまとめること。SOTデータ（Rustで算出済み）の数値を根拠として利用すること。意見の優劣を判断せず、整理に徹すること。",
            _ => "Organize the above questions and LLM opinions objectively. For each question, summarize points of agreement, divergence, and items requiring verification. Use the SOT data (Rust-computed) as the factual basis. Do not judge which opinion is better — focus on neutral synthesis.",
        };
        let board_header = match self.lang.as_str() {
            "ja" => "[Council Session Board]",
            _ => "[Council Session Board]",
        };
        let mut optional_sections: Vec<PromptSection> = Vec::new();
        optional_sections.push(PromptSection {
            text: format!("{}\n{}", board_header, opinions_text),
            mode: TruncateMode::Tail,
        });
        if let Some(sot) = self.council_sot_section() {
            optional_sections.push(sot);
        }
        if let Some(news) = self.council_news_section() {
            optional_sections.push(news);
        }

        Some(self.build_budgeted_prompt(
            optional_sections,
            vec![
                constraint_text(&self.constraint_level, &self.lang).to_string(),
                task.to_string(),
            ],
        ))
    }

    // Build the prompt for the /prompt command: full base_contexts + fundamental + news + constraint.
    // Does not go through --chat-memory budget; sends full context to LLM.
    pub(super) fn build_initial_analysis_prompt(&self) -> String {
        let base = self.build_base_text(usize::MAX);
        let constraint = constraint_text(&self.constraint_level, &self.lang);
        let news = self.build_news_text();
        let mut parts: Vec<String> = vec![base];
        if let Some(fundamental) = self.build_fundamental_text() {
            let addendum = fundamental_task_addendum(self.lang.as_str());
            parts.push(fundamental);
            parts.push(addendum.to_string());
        }
        if let Some(n) = news {
            parts.push(n);
        }
        parts.push(constraint.to_string());
        // Final line before generation: forceful output-language directive.
        parts.push(crate::llm::final_language_directive(&self.lang).to_string());
        parts.join("\n\n")
    }

    // Assemble base_contexts into a single string within `budget` chars.
    // Single context: plain truncation (no header). Multiple contexts: prepend a
    // comparison constraint note, split the remaining budget equally across contexts,
    // and wrap each with a numbered header. Second+ contexts are marked
    // "technical-focused" to surface the data asymmetry.
    pub(super) fn build_base_text(&self, budget: usize) -> String {
        let n = self.base_contexts().len();
        if n == 0 || budget == 0 {
            return String::new();
        }
        if n == 1 {
            return chars_truncate(&self.base_contexts()[0], budget);
        }
        let lang = self.lang.as_str();
        // Comparison constraint note prepended before context entries
        let note = match lang {
            "ja" => "【比較注意】テクニカル指標を比較の主軸とすること。ニュース・ファンダメンタルは一部銘柄にのみ存在するため補足情報として扱うこと。各銘柄はティッカーそのままで呼称し、順番や代名詞で銘柄を置き換えないこと。",
            _ => "[Comparison note: Focus on technical indicators common to all tickers. News and fundamental data are available for some tickers only; treat them as supplementary. Refer to each ticker by its symbol as-is; never replace a ticker with an ordinal or pronoun-like label.]",
        };
        let note_chars = note.chars().count();
        // Reserve budget for note + "\n\n" separator; skip note when budget is too tight
        let (has_note, body_budget) = if note_chars + 2 < budget {
            (true, budget.saturating_sub(note_chars + 2))
        } else {
            (false, budget)
        };
        // Distribute body_budget equally across n contexts; account for "\n\n" separators
        let sep_overhead = (n - 1) * 2;
        let per_budget = body_budget.saturating_sub(sep_overhead) / n;
        let parts: Vec<String> = self
            .base_contexts()
            .iter()
            .enumerate()
            .map(|(i, ctx)| {
                let label = &self.ticker_labels()[i];
                let sym = label.split_once(" (").map_or(label.as_str(), |(t, _)| t);
                let header = match (lang, i == 0) {
                    ("ja", true) => format!("=== {} ===\n", sym),
                    ("ja", false) => format!("=== {}（テクニカル主軸） ===\n", sym),
                    (_, true) => format!("=== {} ===\n", sym),
                    (_, false) => format!("=== {} (technical-focused) ===\n", sym),
                };
                let header_len = header.chars().count();
                // When header alone exceeds per-entry budget, skip it to stay within budget
                if header_len >= per_budget {
                    chars_truncate(ctx, per_budget)
                } else {
                    let ctx_budget = per_budget - header_len;
                    format!("{}{}", header, chars_truncate(ctx, ctx_budget))
                }
            })
            .collect();
        let body = parts.join("\n\n");
        let result = if has_note {
            format!("{}\n\n{}", note, body)
        } else {
            body
        };
        chars_truncate(&result, budget)
    }

    // Assemble technical_texts into a single string within `budget` chars.
    // Used by build_prompt() for regular chat turns so that format/structure
    // instructions in base_contexts do not leak into conversational responses.
    pub(super) fn build_technical_context_text(&self, budget: usize) -> String {
        let n = self.technical_texts().len();
        if n == 0 || budget == 0 {
            return String::new();
        }
        if self
            .technical_texts()
            .iter()
            .all(|text| text.trim().is_empty())
        {
            return self.build_base_text(budget);
        }
        if n == 1 {
            return chars_truncate(&self.technical_texts()[0], budget);
        }
        let lang = self.lang.as_str();
        let note = match lang {
            "ja" => "【比較注意】テクニカル指標を比較の主軸とすること。ニュース・ファンダメンタルは一部銘柄にのみ存在するため補足情報として扱うこと。各銘柄はティッカーそのままで呼称し、順番や代名詞で銘柄を置き換えないこと。",
            _ => "[Comparison note: Focus on technical indicators common to all tickers. News and fundamental data are available for some tickers only; treat them as supplementary. Refer to each ticker by its symbol as-is; never replace a ticker with an ordinal or pronoun-like label.]",
        };
        let note_chars = note.chars().count();
        let (has_note, body_budget) = if note_chars + 2 < budget {
            (true, budget.saturating_sub(note_chars + 2))
        } else {
            (false, budget)
        };
        let sep_overhead = (n - 1) * 2;
        let per_budget = body_budget.saturating_sub(sep_overhead) / n;
        let parts: Vec<String> = self
            .technical_texts()
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let label = &self.ticker_labels()[i];
                let disp = if label.is_empty() {
                    (i + 1).to_string()
                } else {
                    format!("{} {}", i + 1, short_display_label(label))
                };
                let header = match lang {
                    "ja" => format!("=== 銘柄 {} ===\n", disp),
                    _ => format!("=== Ticker {} ===\n", disp),
                };
                let header_len = header.chars().count();
                if header_len >= per_budget {
                    chars_truncate(text, per_budget)
                } else {
                    let text_budget = per_budget - header_len;
                    format!("{}{}", header, chars_truncate(text, text_budget))
                }
            })
            .collect();
        let body = parts.join("\n\n");
        let result = if has_note {
            format!("{}\n\n{}", note, body)
        } else {
            body
        };
        chars_truncate(&result, budget)
    }
}
