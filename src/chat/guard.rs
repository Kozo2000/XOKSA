//! Prompt guard text: investment-advice constraint, forecast policy, and the
//! `/criticize` task instructions. Pure constants and selectors — no session state.

// Injected in every turn; never truncated. Selected by constraint_text().
const CHAT_CONSTRAINT_HIGH: &str = "【役割】ユーザーの質問に、確定値を根拠にまっすぐ答えること。『今は仕込みどきか』のようにタイミングや是非を問う質問にも、データが傾く方向と、その見立てが変わる具体的な水準・条件を示して答えること。これがあなたの仕事であり、禁止された『推奨』ではない（保証ではなく、最終判断はユーザー）。守る一線は次のみ: 確定値（価格・スコア・各指標値・出来高）は書き換えず入力表記のまま使う。銘柄事実・ニュースを捏造しない。自分から売買を勧めたり、具体的な注文価格（エントリー・利確・損切り）を指示しない。ニュースに言及する際はURLを併記する。Markdownや見出し・箇条書き記号を使わずプレーンテキストで答える。用語や金融概念の一般的な説明は入力に依存せず答えてよい。";
const CHAT_CONSTRAINT_MID: &str = "【制約】提供された計算済みの確定値（価格・スコア・各指標値・出来高）を書き換え・誤記しないこと。銘柄事実・ニュースを捏造しないこと。通貨単位は入力データのまま使用し変換・誤記しないこと。テクニカル・ファンダメンタルの参考水準として提示し、投資判断はユーザー自身が行うこと。ニュースに言及する際は必ずそのURLを明記すること。Markdownや見出し・箇条書き記号を使わず、プレーンテキストで回答すること。テクニカル・ファンダメンタル用語や金融概念の一般的な定義・説明は、入力データに依存せず回答してよい。";
const CHAT_CONSTRAINT_LOW: &str = "【制約】提供された計算済みの確定値（価格・スコア・各指標値・出来高）を書き換え・誤記しないこと。銘柄事実・ニュースを捏造しないこと。通貨単位は入力データのまま使用し変換・誤記しないこと。ニュースに言及する際は必ずそのURLを明記すること。Markdownや見出し・箇条書き記号を使わず、プレーンテキストで回答すること。テクニカル・ファンダメンタル用語や金融概念の一般的な定義・説明は、入力データに依存せず回答してよい。";

const CHAT_CONSTRAINT_HIGH_EN: &str = "[Role] Answer the user's question directly, grounded in the confirmed values. For questions of timing or whether to act — such as \"is now a good time to buy in?\" — answer with the direction the data leans and the concrete level or condition that would change that read. This is your job, not the forbidden \"recommendation\" (it is not a guarantee, and the final decision rests with the user). The only lines to hold: keep the confirmed values (prices, scores, indicators, volume) exactly as given, never rewritten; do not fabricate ticker facts or news; do not volunteer a buy/sell call or prescribe specific order prices (entry, profit-taking, stop-loss). Include a URL when referencing news. Answer in plain text (no Markdown, headers, or bullet symbols). General definitions of terms and financial concepts may be answered without relying on the input.";
const CHAT_CONSTRAINT_MID_EN: &str = "[Constraint] Do not rewrite or misstate the provided computed values (prices, scores, indicator values, volume). Do not fabricate ticker facts or news. Use currency units exactly as given. Present as technical/fundamental reference only; investment decisions rest with the user. When referencing a news item, always include its URL. Respond in plain text; do not use Markdown, headers, bullet points, or other formatting. General definitions and explanations of technical or fundamental terms and financial concepts may be answered without relying on the input data.";
const CHAT_CONSTRAINT_LOW_EN: &str = "[Constraint] Do not rewrite or misstate the provided computed values (prices, scores, indicator values, volume). Do not fabricate ticker facts or news. Use currency units exactly as given. When referencing a news item, always include its URL. Respond in plain text; do not use Markdown, headers, bullet points, or other formatting. General definitions and explanations of technical or fundamental terms and financial concepts may be answered without relying on the input data.";

pub(crate) fn constraint_text(level: &str, lang: &str) -> &'static str {
    match (level.trim(), lang) {
        ("low", "ja") => CHAT_CONSTRAINT_LOW,
        ("low", _) => CHAT_CONSTRAINT_LOW_EN,
        ("mid", "ja") => CHAT_CONSTRAINT_MID,
        ("mid", _) => CHAT_CONSTRAINT_MID_EN,
        (_, "ja") => CHAT_CONSTRAINT_HIGH, // "high" and unknown → high
        (_, _) => CHAT_CONSTRAINT_HIGH_EN,
    }
}

// Forecast policy, injected at guard priority right after constraint_text().
// The confirmed computed values are never rewritten regardless of mode; only
// whether forward projection (grounded on those values) is permitted varies.
const CHAT_FORECAST_OFF: &str =
    "【将来予測】確定データの説明にとどめ、将来の数値・方向を予測しないこと。";
const CHAT_FORECAST_SOFT: &str = "【将来予測】確定値とトレンドを根拠に、近い将来の方向性（上昇・下落・レンジ）を明言すること。この方向性を回答の冒頭で述べ、続けてその方向を支える主因と、見立てが変わる具体的な水準・条件を示すこと。具体的な将来数値の断定は控え、確定値そのものは書き換えないこと。";
const CHAT_FORECAST_BOLD: &str = "【将来予測】確定値とトレンドを根拠に、1時間後・明日・来週などの指標水準や価格の方向を具体的な予測として明言すること。この予測を回答の冒頭で述べ、続けてその方向を支える主因と、見立てが変わる具体的な水準・条件を示すこと。予測は「予測」「推定」と明示し、確定値そのものは書き換えないこと。";

const CHAT_FORECAST_OFF_EN: &str =
    "[Forecast] Limit responses to confirmed data; do not predict future values or direction.";
const CHAT_FORECAST_SOFT_EN: &str = "[Forecast] Based on the confirmed values and trend, state a clear near-term direction (up/down/range). Lead with this direction, then give the main factor behind it and the concrete level or condition that would change the read. Avoid asserting specific future figures, and do not rewrite the confirmed values themselves.";
const CHAT_FORECAST_BOLD_EN: &str = "[Forecast] Based on the confirmed values and trend, state concrete projections of indicator levels or price direction for horizons such as one hour, tomorrow, or next week. Lead with this projection, then give the main factor behind it and the concrete level or condition that would change the read. Label projections as predictions/estimates, and do not rewrite the confirmed values themselves.";

pub(crate) fn forecast_clause(mode: &str, lang: &str) -> &'static str {
    match (mode.trim(), lang) {
        ("off", "ja") => CHAT_FORECAST_OFF,
        ("off", _) => CHAT_FORECAST_OFF_EN,
        ("bold", "ja") => CHAT_FORECAST_BOLD,
        ("bold", _) => CHAT_FORECAST_BOLD_EN,
        (_, "ja") => CHAT_FORECAST_SOFT, // "soft" and unknown → soft (default)
        (_, _) => CHAT_FORECAST_SOFT_EN,
    }
}

pub(super) fn criticize_task_text(lang: &str) -> &'static str {
    match lang {
        "ja" => {
            "【/criticize 専用タスク】\n\
直近のDebate Buffer内の他LLM見解を、xoksaデータ（計算結果・取得データ）で照合してください。\n\
\n\
最初の行に「対象: [Provider] ([Model]) — 質問: [Userの質問の冒頭]」を記載してください。\n\
\n\
報告対象は「問題のある記述」だけにしてください。\n\
xoksaデータで確認できた正確な記述・一般的なテクニカル解釈の列挙は不要です。\n\
\n\
以下に該当する記述が見つかった場合のみ報告してください。\n\
- xoksaデータと数値または方向が矛盾する記述\n\
- xoksaデータでは確認できない根拠に基づく主張\n\
- ニュース本文を読んだかのような記述（本文は未取得）\n\
- データ時点が異なる可能性がある数値の比較\n\
- SOT未提供の指標・銘柄についての断定的な記述\n\
\n\
見つかった場合は箇条書きで簡潔に示してください。\n\
問題が見当たらない場合は「SOTとの照合で特記すべき問題点なし」と一言で終えてください。\n\
\n\
照合ルール: ニュースはタイトルとURLのみ（本文未取得）。news_titles=0 の銘柄へのニュース由来の主張は「xoksa内では未確認」。データ時点差がある場合は矛盾と断定しない。他LLM見解・会話履歴はSOTに昇格させない。"
        }
        _ => {
            "[/criticize task]\n\
Review the latest other-LLM opinion in the Debate Buffer against XOKSA data (computed results and fetched data).\n\
\n\
Begin your response with: \"Target: [Provider] ([Model]) — Question: [brief user question]\"\n\
\n\
Report only findings that require attention. Do not enumerate statements that are confirmed by SOT or reflect standard technical interpretation.\n\
\n\
Report only when you find:\n\
- A claim that contradicts XOKSA data in value or direction\n\
- A claim based on information not present in XOKSA data\n\
- A statement implying article body was read (bodies are not fetched)\n\
- A numeric comparison where data timestamps may differ\n\
- A definitive claim about an indicator or ticker not covered in SOT\n\
\n\
Present findings as concise bullet points.\n\
If nothing warrants flagging, end with: \"No issues found against SOT.\"\n\
\n\
Verification rules: News items are titles and URLs only (bodies not fetched). For tickers with news_titles=0, treat news-derived claims as SOT-unverified. When data timestamps differ, classify as possible discrepancy — do not assert contradiction. Do not promote other-LLM opinions or conversation history to SOT."
        }
    }
}

pub(super) fn criticize_empty_debate_text(lang: &str) -> &'static str {
    match lang {
        "ja" => "【Critique Target】Debate Buffer は空です。",
        _ => "[Critique Target] Debate Buffer is empty.",
    }
}
