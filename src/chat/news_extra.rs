//! `/news` buffer ops (find/list/use/del/clear): fetch and inject extra news slots.
//! Operates on `ChatSession` via descendant-module access.

use super::*;

#[derive(Clone)]
pub(super) struct NewsExtraEntry {
    keyword: String,
    articles: Vec<(String, String)>, // (title, url)
}

pub(super) async fn show_news_filtered_llm(
    news: &[NewsItem],
    ticker_label: &str,
    config: &crate::config::Config,
    lang: &str,
    out: &mut ChatOut,
) {
    if news.is_empty() {
        out.line(match lang {
            "ja" => "ニュースは読み込まれていません。",
            _ => "No news loaded.",
        });
        return;
    }
    let items_text = news
        .iter()
        .map(NewsItem::prompt_line)
        .collect::<Vec<_>>()
        .join("\n");
    let criteria = crate::news::news_filter_criteria(lang);
    let triage = crate::llm::news_triage_directive(lang);
    let prompt = match lang {
        "ja" => format!(
            "ニュースハイライト（{}字以内、{}。{}）\n注意: 入力はタイトルとURLのみです。記事本文は取得・読解していません。本文内容を読んだ前提の評価は禁止です。Markdown表や罫線ではなく、ベタ打ちの箇条書きで回答してください。\n\n{}",
            config.max_news_length, criteria, triage, items_text
        ),
        _ => format!(
            "News highlights (within {} chars, {}. {})\nNote: Inputs are titles and URLs only. Article bodies are not fetched or read. Do not imply body-content review. Do not use Markdown tables or ruled layouts; answer in plain bullet lines.\n\n{}",
            config.max_news_length, criteria, triage, items_text
        ),
    };
    if !ticker_label.is_empty() {
        out.line(format!("[ {} ]", short_display_label(ticker_label)));
    }
    out.line(match lang {
        "ja" => "⏳ LLMで仕訳中...",
        _ => "⏳ Classifying via LLM...",
    });
    // News triage is not a market-data claim: the model is asked to sort the
    // titles it was given, and those titles — not the computed values — are what
    // its numbers must come from. So this path deliberately passes no confirmed
    // data, and the guard checks the answer against the input titles (its
    // presence test) instead of attributing numbers to indicators. Attribution is
    // not claimed here; see security-design §1 "Limitations".
    match crate::llm::send_chat_turn(config, &prompt).await {
        Ok(response) => {
            out.line(response.trim());
        }
        Err(e) => out.err(match lang {
            "ja" => format!("⚠️ LLM仕訳に失敗しました: {}", e),
            _ => format!("⚠️ LLM triage failed: {}", e),
        }),
    }
}

pub(super) fn format_extra_entry(slot_label: &str, entry: &NewsExtraEntry) -> String {
    let articles = entry
        .articles
        .iter()
        .enumerate()
        .map(|(j, (title, url))| format!("[{}] {}\n\n    URL: {}", j + 1, title, url))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{} {}\n{}", slot_label, entry.keyword, articles)
}

pub(super) fn build_news_extra_inject_text(
    entries: &[Option<NewsExtraEntry>],
    lang: &str,
) -> Option<String> {
    let filled: Vec<(usize, &NewsExtraEntry)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| e.as_ref().map(|e| (i, e)))
        .collect();
    if filled.is_empty() {
        return None;
    }
    let header = match lang {
        "ja" => "[追加ニュース (Extra News)]",
        _ => "[Extra News]",
    };
    let boundary = match lang {
        "ja" => "注意: ニュースはタイトルとURLのみです。記事本文は取得・読解していません。本文内容の推測や価格影響断定は禁止。",
        _ => "Note: News items are titles and URLs only. Article bodies are not fetched or read. Do not infer body content or assert price impact.",
    };
    let blocks: Vec<String> = filled
        .iter()
        .map(|(i, e)| format_extra_entry(&format!("[X{:02}]", i + 1), e))
        .collect();
    Some(format!("{}\n{}\n{}", header, boundary, blocks.join("\n\n")))
}

pub(super) async fn handle_news_extra_command(
    raw_input: &str,
    session: &mut ChatSession,
    chat_config: &Config,
    lang: &str,
    out: &mut ChatOut,
) {
    let arg = raw_input.strip_prefix("/news ").unwrap_or("").trim();
    match arg {
        "list" => {
            let filled: Vec<(usize, &NewsExtraEntry)> = session
                .news_extra
                .iter()
                .enumerate()
                .filter_map(|(i, e)| e.as_ref().map(|e| (i, e)))
                .collect();
            if filled.is_empty() {
                out.line(match lang {
                    "ja" => "（追加ニュースバッファは空です）",
                    _ => "(Extra news buffer is empty)",
                });
            } else {
                out.line(match lang {
                    "ja" => "── 追加ニュースバッファ ──",
                    _ => "── Extra News Buffer ──",
                });
                for (i, e) in &filled {
                    out.line(format!("[X{:02}] {}", i + 1, e.keyword));
                    for (j, (title, url)) in e.articles.iter().enumerate() {
                        out.line(format!("  [{}] {}", j + 1, title));
                        out.line(format!("      {}", url));
                    }
                }
            }
        }
        "clear" => {
            session.news_extra = vec![None; 16];
            session.news_extra_slot = 0;
            out.line(match lang {
                "ja" => "✅ 追加ニュースバッファをクリアしました。",
                _ => "✅ Extra news buffer cleared.",
            });
        }
        // del <n>
        s if s.starts_with("del ") => {
            let n_str = s.strip_prefix("del ").unwrap().trim();
            match n_str.parse::<usize>() {
                Ok(n) if (1..=NEWS_EXTRA_SLOTS).contains(&n) => {
                    let idx = n - 1;
                    if session.news_extra[idx].is_some() {
                        session.news_extra[idx] = None;
                        out.line(match lang {
                            "ja" => format!("✅ [X{:02}] を削除しました。", n),
                            _ => format!("✅ [X{:02}] deleted.", n),
                        });
                    } else {
                        out.line(match lang {
                            "ja" => format!("（[X{:02}] は空です）", n),
                            _ => format!("([X{:02}] is empty)", n),
                        });
                    }
                }
                Ok(_) => out.err(match lang {
                    "ja" => "❌ 番号は1〜16で指定してください。",
                    _ => "❌ Slot number must be between 1 and 16.",
                }),
                Err(_) => out.err(match lang {
                    "ja" => "❌ del の後に番号を指定してください（例: del 3）。",
                    _ => "❌ Specify a slot number after del (e.g. del 3).",
                }),
            }
        }
        // use <n>  or  use all
        s if s == "use" || s.starts_with("use ") => {
            let sub = s.strip_prefix("use").unwrap().trim();
            match sub {
                "" => {
                    // bare /news use → show usage
                    match lang {
                        "ja" => {
                            out.line("使い方:");
                            out.line("  /news use <番号>    指定スロットのみ次のメッセージに注入（例: use 2 → X02）");
                            out.line("  /news use all       全スロットをワンショット注入（トークン増加注意）");
                            out.line("  /news use all on    常時注入モードON（以降の全メッセージに自動注入）");
                            out.line("  /news use all off   常時注入モードOFF（自動注入を解除）");
                            out.line("  /news list          スロット内容を確認してから番号を指定");
                        }
                        _ => {
                            out.line("Usage:");
                            out.line("  /news use <n>        Inject the specified slot into the next message (e.g. use 2 → X02)");
                            out.line("  /news use all        One-shot inject of all filled slots (note: increases token count)");
                            out.line("  /news use all on     Always-inject mode ON (auto-inject all slots into every message)");
                            out.line("  /news use all off    Always-inject mode OFF (disable auto-inject)");
                            out.line("  /news list           Check buffer contents before choosing a slot number");
                        }
                    }
                }
                "all on" => {
                    session.news_extra_auto_inject = true;
                    let count = session.news_extra.iter().filter(|e| e.is_some()).count();
                    out.line(match lang {
                        "ja" => format!("✅ 常時注入モードON。現在{}件。以降の全メッセージに自動注入されます。", count),
                        _ => format!("✅ Always-inject ON. {} slot(s) currently filled. All slots will be injected into every message.", count),
                    });
                }
                "all off" => {
                    session.news_extra_auto_inject = false;
                    session.news_extra_pending_text = None;
                    out.line(match lang {
                        "ja" => "✅ 常時注入モードOFF。自動注入を解除しました。",
                        _ => "✅ Always-inject OFF. Auto-inject disabled.",
                    });
                }
                "all" => match build_news_extra_inject_text(&session.news_extra, lang) {
                    Some(text) => {
                        let count = session.news_extra.iter().filter(|e| e.is_some()).count();
                        session.news_extra_pending_text = Some(text);
                        out.line(match lang {
                            "ja" => format!("✅ {}件のスロットを次のメッセージに注入します。", count),
                            _ => {
                                format!("✅ {} slot(s) will be injected into the next message.", count)
                            }
                        });
                    }
                    None => out.line(match lang {
                        "ja" => "（追加ニュースバッファは空です）",
                        _ => "(Extra news buffer is empty)",
                    }),
                },
                n_str => match n_str.parse::<usize>() {
                    Ok(n) if (1..=NEWS_EXTRA_SLOTS).contains(&n) => {
                        let idx = n - 1;
                        match &session.news_extra[idx] {
                            Some(entry) => {
                                let header = match lang {
                                    "ja" => "[追加ニュース (Extra News)]",
                                    _ => "[Extra News]",
                                };
                                let boundary = match lang {
                                    "ja" => "注意: ニュースはタイトルとURLのみです。記事本文は取得・読解していません。本文内容の推測や価格影響断定は禁止。",
                                    _ => "Note: News items are titles and URLs only. Article bodies are not fetched or read. Do not infer body content or assert price impact.",
                                };
                                let body = format_extra_entry(&format!("[X{:02}]", n), entry);
                                let text = format!("{}\n{}\n{}", header, boundary, body);
                                session.news_extra_pending_text = Some(text);
                                out.line(match lang {
                                    "ja" => format!("✅ [X{:02}] を次のメッセージに注入します。", n),
                                    _ => {
                                        format!("✅ [X{:02}] will be injected into the next message.", n)
                                    }
                                });
                            }
                            None => out.line(match lang {
                                "ja" => format!("（[X{:02}] は空です）", n),
                                _ => format!("([X{:02}] is empty)", n),
                            }),
                        }
                    }
                    Ok(_) => out.err(match lang {
                        "ja" => "❌ 番号は1〜16で指定してください。",
                        _ => "❌ Slot number must be between 1 and 16.",
                    }),
                    Err(_) => out.err(match lang {
                        "ja" => format!("❌ 不明なオプション: use {}。use <番号> または use all を使ってください。", n_str),
                        _ => format!("❌ Unknown use option: {}. Use use <n> or use all.", n_str),
                    }),
                },
            }
        }
        "" => match lang {
            "ja" => {
                out.line("使い方:");
                out.line("  /news find <検索ワード> キーワードでBrave Newsを検索してバッファに追加（最大256文字）");
                out.line("  /news list             バッファ内容を表示（番号を確認してからuse）");
                out.line(
                    "  /news use <番号>        指定スロットのみ次のメッセージに注入（例: use 2）",
                );
                out.line(
                    "  /news use all           全スロットをワンショット注入（トークン増加注意）",
                );
                out.line(
                    "  /news use all on        常時注入モードON（以降の全メッセージに自動注入）",
                );
                out.line("  /news use all off       常時注入モードOFF（自動注入を解除）");
                out.line("  /news del <番号>        指定スロットを削除（例: del 3）");
                out.line("  /news clear            全スロットをクリア");
            }
            _ => {
                out.line("Usage:");
                out.line(
                    "  /news find <keyword>   Search Brave News and add to buffer (max 256 chars)",
                );
                out.line("  /news list             Show buffer contents (check slot numbers before injecting)");
                out.line("  /news use <n>           Inject the specified slot into the next message (e.g. use 2)");
                out.line("  /news use all           One-shot inject of all filled slots (note: increases token count)");
                out.line("  /news use all on        Always-inject mode ON (auto-inject all slots into every message)");
                out.line("  /news use all off       Always-inject mode OFF (disable auto-inject)");
                out.line("  /news del <n>           Delete the specified slot (e.g. del 3)");
                out.line("  /news clear            Clear all slots");
            }
        },
        s if s.starts_with("find ") => {
            let keyword = s.strip_prefix("find ").unwrap_or("").trim();
            if keyword.is_empty() {
                out.err(match lang {
                    "ja" => "❌ /news find の後に検索ワードを指定してください。",
                    _ => "❌ Provide a keyword after /news find.",
                });
                return;
            }
            if keyword.chars().count() > NEWS_EXTRA_MAX_KEYWORD_CHARS {
                out.err(match lang {
                    "ja" => "❌ 検索ワードは256文字以内にしてください。",
                    _ => "❌ Keyword must be 256 characters or fewer.",
                });
                return;
            }
            if chat_config.no_news {
                out.err(match lang {
                    "ja" => "❌ ニュース取得は無効です（NO_NEWS=true）。",
                    _ => "❌ News fetch is disabled (NO_NEWS=true).",
                });
                return;
            }
            let api_key = match crate::utils::resolve_api_key("BRAVE_API_KEY") {
                Ok(Some(k)) => k,
                Ok(None) => {
                    out.err(match lang {
                        "ja" => "❌ BRAVE_API_KEY が設定されていません。",
                        _ => "❌ BRAVE_API_KEY is not set.",
                    });
                    return;
                }
                Err(e) => {
                    out.err(match lang {
                        "ja" => {
                            format!("❌ BRAVE_API_KEY のキーチェーンアクセスに失敗しました: {e}")
                        }
                        _ => format!("❌ BRAVE_API_KEY keyring access error: {e}"),
                    });
                    return;
                }
            };
            let (country, search_lang, ui_lang) = if lang == "ja" {
                ("JP", "jp", "ja-JP")
            } else {
                ("US", "en", "en-US")
            };
            let fetcher = crate::news::BraveArticleFetcher {
                proxy_url: chat_config.https_proxy.clone(),
                no_proxy: chat_config.no_proxy.clone(),
            };
            match fetcher
                .fetch_articles(keyword, &api_key, country, search_lang, ui_lang, 5, None)
                .await
            {
                Ok(articles) if !articles.is_empty() => {
                    let pairs: Vec<(String, String)> = articles
                        .iter()
                        .map(|a| (a.title.clone(), a.url.clone()))
                        .collect();
                    let slot =
                        if let Some(empty) = session.news_extra.iter().position(|s| s.is_none()) {
                            empty
                        } else {
                            let s = session.news_extra_slot;
                            session.news_extra_slot = (s + 1) % NEWS_EXTRA_SLOTS;
                            s
                        };
                    session.news_extra[slot] = Some(NewsExtraEntry {
                        keyword: keyword.to_string(),
                        articles: pairs,
                    });
                    out.line(match lang {
                        "ja" => format!("✅ [X{:02}] に保存しました。", slot + 1),
                        _ => format!("✅ Saved to [X{:02}].", slot + 1),
                    });
                }
                Ok(_) => out.line(match lang {
                    "ja" => "（該当するニュースが見つかりませんでした）",
                    _ => "(No news articles found for this keyword)",
                }),
                Err(e) => out.err(format!("⚠️ News fetch failed: {e}")),
            }
            drop(api_key);
        }
        _ => out.err(match lang {
            "ja" => "❌ 不明な /news サブコマンドです。",
            _ => "❌ Unknown /news subcommand.",
        }),
    }
}
