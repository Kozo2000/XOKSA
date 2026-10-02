# XOKSA Error / Log Codes

[日本語は下部。](#ja)

XOKSA emits diagnostics through the structured logger (`src/logging.rs`), not raw
`eprintln!`. Every entry carries a **stable code** so it is searchable and
documentable, and the full record — with a timestamp column — is appended to a
local log file for later review.

## Format & location

- **Log file:** `logs/xoksa-error.log` under the current working directory (the `logs/` dir is created on first write; append-only; unix permissions `0600`). If the working directory isn't writable (a backend launched from a locked-down or system dir), it falls back to a machine-wide **system** location — Windows `%PROGRAMDATA%\xoksa\logs`, Unix `/tmp/xoksa` — **never the user's home**. The absolute path is printed at startup (`Diagnostics log: …`).
- **Rotation:** past 5 MB it rolls generationally — `xoksa-error.log` → `.1` → `.2` (oldest dropped) — keeping the two most recent full files plus the current one.
- **Line (text):** `<ts> <LEVEL> [<CODE>] <message>` — e.g. `2026-07-14T04:18:28Z INFO [XK-FUND-MISSING] fundamental data unavailable for SOXL: ...`. `<ts>` is the ISO-8601 UTC timestamp column.
- **Line (json):** one NDJSON object per line — `{"ts":…,"level":…,"code":…,"msg":…}` (select with `serve --log-format json`).
- **Levels:** `ERROR` / `WARN` print to the console **once** per unique `(code, msg)` (repeats are collapsed) **and** go to the file; `INFO` is **file-only** (no console output).
- **`--private`:** no-trace session — nothing is written to the file.
- **Security:** never a credential (Class A — API keys) or chat/user-input content in `code`/`msg`. See [security-design.md §2](../dev-prog/security-design.md).

## Code scheme

`XK-<CATEGORY>-<NAME>` — a stable, descriptive identifier (no volatile numeric index). One code = one condition.

| Code | Level | Meaning |
| :--- | :--- | :--- |
| `XK-FUND-MISSING` | INFO | Fundamental data is unavailable for the ticker. Expected for tickers without SEC/J-Quants filings (e.g. **ETFs**); fundamentals are supplementary, so this is **not** a user-facing error — recorded to the file only. |
| `XK-NEWS-FETCH` | INFO | A news fetch failed. News is supplementary and non-fatal; recorded to the file only. |
| `XK-IND-EVAL` | WARN | One or more extended technical-indicator evaluations failed; the affected indicators are omitted while the rest proceed. Emitted on the user-facing analysis (`app.rs`) and on a chat ticker load — not on the background per-bar and context builds, which run `silent` so one analysis does not repeat the same line hundreds of times. |
| `XK-BT-IND` | WARN | **Backtest only** (emitted by the per-bar `bar_indicators`): a simulated bar could not compute one or more indicators, so any rule condition referencing them evaluates to false for that bar (never as a valid `0.0`). Records which indicators were missing, so a rule that never trades can be explained. The alert monitor evaluates the same map but does not pass through this function, so it emits no code for a per-bar miss — a condition on an indicator with no value is simply false, and only a failed recompute or an unusable rule is recorded, under `XK-ALERT`. |
| `XK-LLM-INTEGRITY` | INFO | The output-integrity guard (every provider, not only Ollama) excluded one or more sentences/lines whose numbers did not belong to the confirmed data they claimed — a value moved to another indicator, symbol, bar, unit or period, a reading of an indicator that was never computed, or a figure with no confirmed counterpart. The exclusion is already shown to the user in the chat panel; this is a file-only record. |
| `XK-TICKER-INVALID` | WARN | A ticker symbol failed format validation (allowed: alphanumeric, `.`, `-`). The request rejecting it already surfaces the error to the caller; this records it once. |
| `XK-MARKET-TZ` | INFO | The exchange timezone from the market feed could not be parsed; the analysis falls back to UTC. Recoverable; file-only. |
| `XK-ALERT` | INFO / WARN | The in-session alert monitor (`serve`): the code for the monitor starting, notifications sent, and work that could not run. An alert's optional EXPLAIN note goes through the output-integrity guard like any other LLM answer, so a detection there is recorded under `XK-LLM-INTEGRITY` (the note is then dropped, never sent). **INFO** records the monitor starting and each notification sent. **WARN** records a rule that could not run or a message that was not delivered — a rule whose `MODE` is not an intraday bar (`1m｜5m｜15m｜30m｜60m`) is skipped, a recompute for the rule's ticker failed, or `NOTIFY=` names a channel that is not configured. Never contains the webhook secret (Class A). |

New conditions add a new `XK-<CATEGORY>-<NAME>` row here when they are wired to the logger.

---

<a id="ja"></a>

# XOKSA エラー / ログコード

XOKSA の診断出力は、生の `eprintln!` ではなく構造化ロガー（`src/logging.rs`）を通します。各エントリは**安定したコード**を持ち（検索・文書化が可能）、**日時列つき**の全記録がローカルログファイルに追記されます。

## 形式と場所

- **ログファイル:** カレント作業ディレクトリ直下の `logs/xoksa-error.log`（`logs/` は初回書込時に作成・追記専用・unix パーミッション `0600`）。作業ディレクトリが書き込み不可の場合（ロックダウンやシステムディレクトリから起動されたバックエンド）は、マシン共通の**システム**の場所へフォールバックする — Windows は `%PROGRAMDATA%\xoksa\logs`、Unix は `/tmp/xoksa` — **ユーザーホームには一切書かない**。絶対パスは起動時に表示（`Diagnostics log: …`）。
- **ローテーション:** 5 MB を超えると世代ローテート — `xoksa-error.log` → `.1` → `.2`（最古を破棄）— 直近2世代＋現行を保持。
- **行（text）:** `<日時> <LEVEL> [<CODE>] <メッセージ>`（例: `2026-07-14T04:18:28Z INFO [XK-FUND-MISSING] fundamental data unavailable for SOXL: ...`）。`<日時>` は ISO-8601 UTC のタイムスタンプ列。
- **行（json）:** 1行1 NDJSON オブジェクト（`serve --log-format json` で選択）。
- **レベル:** `ERROR` / `WARN` は同一 `(code, msg)` につき**一度だけ**コンソールに出し（重複は抑制）、**かつ**ファイルへ。`INFO` は**ファイルのみ**（コンソールに出さない）。
- **`--private`:** 無痕跡セッション。ファイルには一切書かない。
- **セキュリティ:** `code`/`msg` に認証情報（クラスA＝APIキー）やチャット/ユーザー入力を**絶対に含めない**。[security-design.md §2](../dev-prog/security-design.md) 参照。

## コード体系

`XK-<カテゴリ>-<名前>` — 安定した記述的な識別子（揺れる連番は使わない）。1コード＝1条件。

| コード | レベル | 意味 |
| :--- | :--- | :--- |
| `XK-FUND-MISSING` | INFO | 銘柄のファンダメンタルデータが取得できない。SEC/J-Quants の開示が無い銘柄（例: **ETF**）では想定内。ファンダは補助情報なので**ユーザー向けエラーではなく**、ファイルにのみ記録。 |
| `XK-NEWS-FETCH` | INFO | ニュース取得に失敗。ニュースは補助情報で非致命。ファイルにのみ記録。 |
| `XK-IND-EVAL` | WARN | 拡張テクニカル指標の評価が一部失敗。該当指標は省き、残りは継続。ユーザー向けの分析（`app.rs`）とチャットの銘柄ロードで発行する。背景の足単位・文脈構築は `silent` で走るため出さない（1回の分析で同じ行を何百回も繰り返さないため）。 |
| `XK-BT-IND` | WARN | **バックテスト専用**（各足の `bar_indicators` が発行）。シミュレーション中の足で指標が計算できず、その足ではその指標を参照する条件が false になる（有効な `0.0` としては扱わない）。どの指標が欠けたかを記録するので、売買が起きないルールの理由を追える。アラート監視は同じ表を使うがこの関数を通らないため、足単位の欠損ではコードを出さない——値の無い指標を参照する条件は単に false であり、記録されるのは再計算の失敗や実行できないルールだけで、それは `XK-ALERT` に出る。 |
| `XK-LLM-INTEGRITY` | INFO | 出力整合性ガード（Ollama に限らず全プロバイダ）が、確定データに帰属しない数値を含む文・行を除外。別指標・別銘柄・別足・別単位・別時点への付け替え、未算出の指標の読み取り値、確定データに対応が無い数値が対象。除外はチャット画面で既にユーザーに提示済みのため、ここではファイルのみ記録。 |
| `XK-TICKER-INVALID` | WARN | ティッカー記号が形式検証に失敗（許可: 英数字・`.`・`-`）。拒否したリクエストが呼び出し元にエラーを返すので、ここでは1回記録するだけ。 |
| `XK-MARKET-TZ` | INFO | 市場フィードの取引所タイムゾーンを解析できず、UTC にフォールバック。回復可能・ファイルのみ。 |
| `XK-ALERT` | INFO / WARN | `serve` 内のアラート監視。監視の開始・通知の送信・実行できなかった処理を記録するコード。アラートの任意の EXPLAIN 注記は他の LLM 回答と同じく出力整合性ガードを通るため、そこでの検知は `XK-LLM-INTEGRITY` に記録される（注記自体は破棄され、送信しない）。**INFO** は監視の開始と通知の送信を記録。**WARN** は動かせなかったルール・届かなかった通知を記録する — `MODE` が分足（`1m｜5m｜15m｜30m｜60m`）でないルールはスキップ、対象銘柄の再計算に失敗、`NOTIFY=` が未設定のチャンネルを指している、の各場合。Webhook の秘密（クラスA）は決して含めない。 |

新しい条件をロガーに接続したら、`XK-<カテゴリ>-<名前>` の行をここに追加する。
