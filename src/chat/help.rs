//! Chat-mode command catalog — the single source of truth for chat commands.
//!
//! Both the CLI `/help` listing (`print_help_ja` / `print_help_en`) and the
//! Web UI Help window (`server::api::chat_commands` via `command_catalog`) render
//! from this one catalog, so the command reference never drifts between the two.
//!
//! Grammar (uniform): `cmd [sub] [args]`. Every command word is ≤6 chars. Section
//! headers use the sentinel command `"#"`; continuation notes use `""`.

use super::COUNCIL_MAX_ROUNDS;

/// One line of the command reference.
/// - `command == "#"` → a section header (`desc` is the title).
/// - `command == ""`  → a continuation note for the preceding command.
/// - otherwise        → a real command row.
pub(crate) struct HelpEntry {
    pub command: &'static str,
    pub desc: String,
}

fn e(command: &'static str, desc: impl Into<String>) -> HelpEntry {
    HelpEntry {
        command,
        desc: desc.into(),
    }
}

fn hd(title: &'static str) -> HelpEntry {
    HelpEntry {
        command: "#",
        desc: title.to_string(),
    }
}

/// The single command catalog, localized. `lang == "ja"` → Japanese, else English.
pub(crate) fn command_catalog(lang: &str) -> Vec<HelpEntry> {
    if lang == "ja" {
        catalog_ja()
    } else {
        catalog_en()
    }
}

fn catalog_ja() -> Vec<HelpEntry> {
    vec![
        hd("銘柄・データ"),
        e("sym", "ロード中の銘柄を一覧"),
        e("sym add <銘柄>", "比較銘柄を追加（最大5銘柄）"),
        e("sym del <番号>", "指定番号の銘柄を削除"),
        e("tech [足] [1-5]", "テクニカル表示。足省略時は現在の mode（daily|1m|5m|15m|30m|60m|weekly|monthly）"),
        e("funda [1-5]", "ファンダメンタル表示。番号: 1 / 1,3 / 1-3"),
        e("mode [足]", "分析足の表示・切替（銘柄ロード済みなら自動再取得）"),
        e("reload t|n", "全銘柄を再取得（t=テクニカル / n=テクニカル+ニュース）"),
        e("auto on|off|notice", "足の長さに合わせた自動リロード（分足のみ。notice=価格変動を表示）"),
        hd("ニュース"),
        e("news [1-5]", "ニュース表示。番号: 1 / 1,3 / 1-3"),
        e("news nf [1-5]", "ニュースをLLMで仕訳（番号指定可）"),
        e("news find <検索ワード>", "Brave Newsで検索しバッファに追加（最大256文字）"),
        e("news list", "追加ニュースバッファの内容を表示"),
        e("news use <番号>|all [on|off]", "注入（all on=常時注入ON / all off=解除）"),
        e("news del <番号>", "指定スロットを削除"),
        e("news clear", "追加ニュースバッファをクリア"),
        hd("分析"),
        e("basic", "基本分析：基本コンテキストをLLMへ送信して分析（旧 /run。/run も当面可）"),
        e("set", "現在の解析パラメータ一覧（session限定・envは不変）"),
        e("set <項目> <値>", "解析パラメータを変更（例: set buy-rsi 25 / set macd-minus-ok on / set bb-period 20）"),
        e("", "項目: macd-minus-ok / buy-rsi / sell-rsi / macd-diff-low|mid|extreme / bb-period|sigma|squeeze / adx-period / roc-period / stoch-period / vwap-period / fib-ratio"),
        e("set reset", "解析パラメータを env 既定に戻す"),
        e("", "※指標の有無・重みは env 専用（DB整合のため /set 対象外）"),
        hd("応答スタイル（現在値は /status）"),
        e(
            "depth shallow|mid|deep",
            "回答の詳しさ：数値を簡潔に ↔ その数値が価格に何を意味するかまで詳しく（既定 mid）",
        ),
        e(
            "scope narrow|mid|wide",
            "話の広さ：この銘柄のデータだけで答える ↔ 業界や市場の一般知識も交えて答える（確定値は捏造しない・既定 mid）",
        ),
        e("shape talk|points|scenario", "回答の形：会話体で答える ↔ 要点を箇条書き ↔ 場合分け（既定 talk）"),
        e("cast off|soft|bold", "将来予測の強さ（既定 soft）"),
        hd("LLM"),
        e("llm", "使えるLLM一覧（プロバイダ/モデル/APIキー有無）"),
        e("llm net", "各社APIから現行モデル名を取得（env に書く候補の発見）"),
        e("llm <provider>[:model]", ":model省略でenv/初期値、明示でモデル指定（例: llm openai / llm openai:gpt-5.6-terra）"),
        e("llm <別名>", "ollama インスタンスに切替（例: llm gpu1）"),
        hd("マルチLLM合議 (forum)"),
        e("forum", "参加者・Chair（まとめ役）・使い方を表示"),
        e("forum set <p,...>", "参加者を設定（例: forum set openai,gemini,claude）"),
        e(
            "forum ask [rN] <質問>",
            format!("複数LLMで合議（rN=ラウンド数 既定1・最大{COUNCIL_MAX_ROUNDS}。例: forum ask r2 …）"),
        ),
        e("", "1ラウンド=全参加者並列回答→Chairが整理。最終でChairが結論"),
        e("forum chair <p>|reset", "まとめ役を設定/確認・初期化"),
        e("forum sum", "Chair が Board / Debate Buffer を総括"),
        e("forum log", "Board（議事録）を表示"),
        e("forum clear", "Debate Buffer と Board をクリア"),
        hd("他LLM検証"),
        e("crit", "直近の見解を批判的に検討（別LLMの回答が必要）"),
        e("keep off|summary|claims", "他LLM見解の保持モード（引数なしでBuffer表示）"),
        hd("アラート通知 (alert・Web専用)"),
        e("alert", "ルールと通知チャンネルの一覧（= alert list）"),
        e("alert add <銘柄> <足> <条件> <チャンネル名> [explain]", "ルール追加（足=分足・条件は空白なし・例 alert add NVDA 5m rsi<=30 team explain）"),
        e("alert on|off <番号>", "ルールの有効化/無効化（session限定）"),
        e("alert del <番号>", "ルールを削除"),
        e("alert test <チャンネル名>", "指定チャンネルへテスト通知を送信"),
        e("", "※チャンネルと secret は xoksa.env / --update-key で設定。/alert では作成しない"),
        hd("コンテキスト"),
        e("mem low|mid|high", "コンテキスト保持量（low:8K/2 mid:16K/4 high:32K/8）"),
        e("token [d]", "コンテキストサイズ・累計トークン（d=詳細）"),
        hd("基本"),
        e("status", "チャット状態（銘柄・LLM・メモリ・forum・トークン）"),
        e("date", "現在の日付と時刻（ローカル＋UTC）を表示"),
        e("help", "このヘルプを表示"),
        e("bye", "チャットを終了"),
    ]
}

fn catalog_en() -> Vec<HelpEntry> {
    vec![
        hd("Tickers & data"),
        e("sym", "List loaded tickers"),
        e("sym add <symbol>", "Add a comparison ticker (max 5)"),
        e("sym del <number>", "Remove the specified ticker"),
        e("tech [tf] [1-5]", "Technical. tf defaults to current mode (daily|1m|5m|15m|30m|60m|weekly|monthly)"),
        e("funda [1-5]", "Fundamental display. Index: 1 / 1,3 / 1-3"),
        e("mode [tf]", "Show or set bar mode (auto-reloads if tickers loaded)"),
        e("reload t|n", "Reload all tickers (t=technical / n=technical+news)"),
        e("auto on|off|notice", "Auto-reload at bar interval (intraday only; notice=show price change)"),
        hd("News"),
        e("news [1-5]", "News display. Index: 1 / 1,3 / 1-3"),
        e("news nf [1-5]", "News triaged by LLM (index optional)"),
        e("news find <kw>", "Search Brave News and add to buffer (max 256 chars)"),
        e("news list", "Show extra-news buffer contents"),
        e("news use <n>|all [on|off]", "Inject (all on=always-inject ON / all off=disable)"),
        e("news del <n>", "Delete the specified slot"),
        e("news clear", "Clear the extra-news buffer"),
        hd("Analysis"),
        e("basic", "Basic analysis: send base context to the LLM (was /run; /run still works)"),
        e("set", "List current analysis params (session only; env unchanged)"),
        e("set <k> <v>", "Change an analysis param (e.g. set buy-rsi 25 / set macd-minus-ok on / set bb-period 20)"),
        e("", "keys: macd-minus-ok / buy-rsi / sell-rsi / macd-diff-low|mid|extreme / bb-period|sigma|squeeze / adx-period / roc-period / stoch-period / vwap-period / fib-ratio"),
        e("set reset", "Reset analysis params to env defaults"),
        e("", "Indicator on/off and weights are env-only (kept out of /set for DB consistency)"),
        hd("Response style (current values in /status)"),
        e(
            "depth shallow|mid|deep",
            "How detailed the reply is: just the numbers vs what they mean for the price (default mid)",
        ),
        e(
            "scope narrow|mid|wide",
            "How broad the reply is: this stock's data only vs also general market/industry context (confirmed values never fabricated; default mid)",
        ),
        e("shape talk|points|scenario", "Answer form: conversational prose ↔ key-point list ↔ scenarios (default talk)"),
        e("cast off|soft|bold", "Forecasting strength (default soft)"),
        hd("LLM"),
        e("llm", "List usable LLMs (provider / model / API key)"),
        e("llm net", "Query each provider's current model names (to fill env)"),
        e("llm <provider>[:model]", "Omit :model for env/default, or pin one (e.g. llm openai / llm openai:gpt-5.6-terra)"),
        e("llm <alias>", "Switch to an ollama instance (e.g. llm gpu1)"),
        hd("Multi-LLM forum"),
        e("forum", "Show participants, chair, and usage"),
        e("forum set <p,...>", "Set participants (e.g. forum set openai,gemini,claude)"),
        e(
            "forum ask [rN] <q>",
            format!("Multi-LLM deliberation (rN=rounds, default 1, max {COUNCIL_MAX_ROUNDS}; e.g. forum ask r2 …)"),
        ),
        e("", "1 round = all participants in parallel → chair synthesis; final = conclusion"),
        e("forum chair <p>|reset", "Set/show/reset the synthesizer"),
        e("forum sum", "Chair synthesizes Board / Debate Buffer"),
        e("forum log", "Show the Board (meeting minutes)"),
        e("forum clear", "Clear Debate Buffer and Board"),
        hd("Cross-LLM verification"),
        e("crit", "Critically review the latest opinion (needs another LLM's answer)"),
        e("keep off|summary|claims", "Other-LLM opinion retention (no arg: show buffer)"),
        hd("Alerts (alert; Web UI only)"),
        e("alert", "List rules and notification channels (= alert list)"),
        e("alert add <symbol> <mode> <cond> <channel> [explain]", "Add a rule (mode=intraday bar, cond has no spaces, e.g. alert add NVDA 5m rsi<=30 team explain)"),
        e("alert on|off <n>", "Enable/disable a rule (session only)"),
        e("alert del <n>", "Remove a rule"),
        e("alert test <channel>", "Send a test notification to the channel"),
        e("", "Channels & secrets are set in xoksa.env / --update-key; /alert never creates one"),
        hd("Context"),
        e("mem low|mid|high", "Context retention (low:8K/2 mid:16K/4 high:32K/8)"),
        e("token [d]", "Context size and cumulative tokens (d=detail)"),
        hd("Basics"),
        e("status", "Chat status (tickers, LLM, memory, forum, tokens)"),
        e("date", "Show the current date and time (local + UTC)"),
        e("help", "Show this help"),
        e("bye", "Exit chat"),
    ]
}

/// Render the catalog as the `/help` listing into any output sink (single
/// renderer for the CLI startup banner and the unified `execute()` dispatch).
pub(crate) fn write_help(out: &mut super::ChatOut, lang: &str) {
    for entry in command_catalog(lang) {
        match entry.command {
            "#" => {
                out.line("");
                out.line(format!("── {} ──", entry.desc));
            }
            "" => out.line(format!("       {}", entry.desc)),
            cmd => out.line(format!("  /{:<27} {}", cmd, entry.desc)),
        }
    }
}

pub(super) fn print_help_ja() {
    write_help(&mut super::ChatOut::Stdout, "ja");
}

pub(super) fn print_help_en() {
    write_help(&mut super::ChatOut::Stdout, "en");
}
