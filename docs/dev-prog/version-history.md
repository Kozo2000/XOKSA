# xoksa Version History

[日本語ドキュメントはこちら。](#ja)

> **Language sync:** English and Japanese descriptions are both authoritative. If a requirement, restriction, exception, command behavior, or operational rule appears in only one language, treat it as applying to both; the missing side is a documentation defect to be updated.

This document contains free-form notes from the development team — background context, design rationale, and feature explanations worth communicating to users. Unlike [CHANGELOG.md](../../CHANGELOG.md), which follows a strict format and records the facts of each change, this document captures the *why* and *what it means in practice*. For the overall design principles, see [design-philosophy.md](./design-philosophy.md).

> **Update policy:** This document records **minor-version milestones only** (the `Y` in `vX.Y.z`). Patch releases (`vX.Y.z`) are recorded in [CHANGELOG.md](../../CHANGELOG.md) only, keeping a clear division of roles between the two documents. Each minor section groups the work delivered across that line's patch releases.

---

## v2 — Web UI / Local HTTP Server, Rust/WASM Frontend, Backtest

### v2.9 — Real-Time Japanese Prices, Observation-Based Refresh, and an Audit of the Shipped Binary

Japanese intraday analysis stopped being wrong. The TSE price Yahoo's Chart API returns is delayed ~15 minutes, and it was being treated as the latest price — so every intraday indicator for a Japanese stock was computed on stale input. The real-time price and time now come from the public Yahoo! Finance Japan page and are folded into the forming bar, with the Chart API kept as the fallback so an analysis never fails outright. The dashboard's refresh also stopped guessing: instead of a fixed clock it follows **the source's own data time**, polling at that granularity and serving a cached build inside the window rather than re-fetching and re-analyzing.

The desktop line was repaired twice over. Its **local auto-start had lost the v2.6.4 hardening** — it had gone back to a fixed port, so a leftover `xoksa serve` could be silently adopted or block startup with nothing but a ten-second timeout. The auto-started engine now takes an OS-assigned ephemeral port again, the identity check is an exact version match, and every failure names its cause (spawn failure, immediate exit with status, port occupied and by what, 401 distinguished from unreachable).

The line closed with an **audit of the shipped binary against the canon**, prompted by a simple question: why does editing a manual change the binary's hash? Because the manuals were compiled into it and served over HTTP at `/manual/:slug` — a surface **no UI ever linked to and no CLI flag exposed**. It was removed along with the embedded screenshots, the markdown renderer, an unused analysis endpoint, and a desktop command with zero callers (~1.4 MB smaller). The same audit found `--private` writing the user's typed chat lines to `~/.xoksa_history` in a "no-trace" session, and a settings app that reported restarts it had not performed — it tracked only the child it spawned in the current session, so after reopening the app "Restart" killed nothing, spawned an engine that died on the occupied port, and still claimed success, leaving the old access token in force. Restart is now one honest action: it pre-checks the port, names an occupant it did not start (and never kills it), and waits for the new engine to listen before reporting success. The port itself became a visible setting instead of an env-file-only value.

The line closed by turning three promises the documents made into things the code actually does. An indicator that **could not be computed** had been reaching rule evaluation as a valid `0.0`, so `close > long EMA` could hold on a value that was never produced; the guard now records which indicators were genuinely computed, a failed recomputation clears the previous reading, and a condition on a missing indicator is simply false — the backtest and the live alert monitor share that one map, so they cannot disagree. The **output-integrity guard** stopped being a Japanese-language feature and stopped taking the prompt as its reference: each check now holds its Japanese and English wording together, and the reference is built from `TechnicalDataGuard` and `FundamentalData` and passed alongside the request. That distinction matters because a prompt also carries the conversation and other models' opinions — a number a user typed used to be indistinguishable from one the engine wrote. Verification became per claim rather than per answer: a figure must match the fact its own clause attributes it to, in instrument, bar, indicator, unit, currency, sign and period, so "the daily close is X and the hourly close is Y" is checked as two claims and the pair with the values exchanged is refused. **Fundamental data** gained a validated access boundary — private fields with every write path checking what it writes, and the initial construction from a provider's result concentrated in one place (see [design-philosophy.md §4.5](./design-philosophy.md)) — which also removed the duplicated PER/PBR/ROE formulas each provider path had been carrying.

### v2.8 — A Standalone Settings App, and `serve` That Can Be Reached Safely From Another Machine

Configuration moved out of the desktop into **`xoksa-setup`**, a separate password-gated app that owns every setting — API keys, LLM, indicators, news, notifications, the access token. It links no engine code either: it drives the engine's own subcommands (`config-json` / `apply-config` / `settings-password` / `conn-token`), so secrets travel over a child process's stdin to the OS keychain, never over argv and never over HTTP. The desktop became purely a **connection shell**: it asks *which engine to talk to* — one it starts locally, or a remote one by address and token — and points the WebView at it.

That split is what made remote use worth supporting, and remote use is what made authentication mandatory. A **non-loopback bind is fail-closed**: if no access token exists the engine generates a 256-bit one from the OS CSPRNG and stores it like any other credential (keychain, `Zeroizing`, never a process argument or a log line), so a non-loopback server can never run unauthenticated. Because the operator never chose that token, it is the one Class A secret the engine will show on demand (`serve --show-token`, rotatable with `--rotate-serve-token`); a browser exchanges it once for an `HttpOnly` / `SameSite=Strict` session cookie. Only private-range and loopback source addresses are accepted, keeping the server inside a trusted network rather than the open internet.

### v2.7 — One Canonical Config Location, and the Desktop as Presentation Only

Two single-source-of-truth defects were fixed in the same line. The desktop had been parsing `xoksa.env` with **its own working-directory-relative reader** — a second source of truth — so re-saving from that divergent view could overwrite the real configuration: re-configuring the LLM appeared to forget the cloud API keys and drop the local-model setup. The desktop now asks the engine (`xoksa config-json`) for the current configuration and is presentation-only, so a re-save preserves everything.

The config file itself gained **one canonical, user-scoped location** (`%APPDATA%\xoksa` / `~/Library/Application Support/xoksa` / `~/.config/xoksa`), shared by the CLI, `serve`, the desktop, and the settings app, with a one-time non-destructive migration on startup. There is no implicit `./xoksa.env` fallback any more, so configuration can no longer split by launch directory. Japanese stock names also became far easier to set up: the JPX `data_j.xls` is read directly, with no conversion to CSV.

### v2.6 — Chat Push Notifications, Manual Consolidation, Shipping the Desktop App

(There is no v2.5 line; v2.4 was followed directly by v2.6.)

XOKSA learned to **tell you when something happens**. While `xoksa serve` runs, an in-session monitor watches the confirmed intraday bars and, when a condition on a **computed value** is met (e.g. `RSI ≤ 30`), pushes a one-line message to Slack / Discord / Google Chat (incoming webhook) or LINE (Messaging API). What it sends is the confirmed fact — ticker, bar time, the value that crossed, the threshold — never a trade call; outbound hosts are a fixed per-platform allowlist in code, `https`-only, and the channel secret is Class A (OS keychain, never the env file). Rules are a first-class product feature: an **alerts panel** in the dashboard lists rules and channels and can add / delete / enable / disable / test them (persisted to `xoksa.env`), and the chat `/alert` command does the same live. The desktop settings form can **test a channel before saving it** — the secret travels over the child process's stdin, never argv. J-Quants also became **v2-only** here (the V1 API was discontinued 2026-06-01).

The manuals were **consolidated from nine documents to four**: `usage-guide` (dashboard, chat & commands, local LLM), `analysis-guide` (reading the analysis, indicators & the score, strategies), `setup` (setup + integration), and `command-reference`. Overlapping guides had made the same topic answerable in three places, each drifting separately.

The rest of the line is robustness and shipping. The desktop shell starts the engine on an **ephemeral port** and attaches the WebView only after `/api/health` reports a matching version, so a stale `serve` can never be mistaken for the app's engine; engine bundling moved to Tauri's **`externalBin` sidecar** so Windows and macOS package it the same way; an Ollama answer truncated by a reasoning model's thinking budget now **steps the effort down and retries** instead of failing; and `--debug-prompt` and `--no-llm` became **independent** options. Two supply-chain findings surfaced from the shipping inspection itself: `keyring` 4.0's default features had linked an **embedded SQL database (turso)** into the binary — contradicting the No-Database design — which was removed in favour of the platform-native credential stores (`Cargo.lock` −3000 lines), and a notification-setup prompt could write a pasted webhook secret to `xoksa.env` in plaintext. The line also produced the **anti-malware false-positive case study**: the desktop binary's Defender/Trapmine detections were traced not to code but to the **icon's encoding** in the PE resource section, and Tauri's 3-byte MSI bundle-type marker alone flipped a clean binary to `Trojan:Win32/Wacatac.B!ml` — hence the MSI is built with the unpatched payload and a hash check ([av-false-positive-case-study.md](./av-false-positive-case-study.md)). macOS shipped **Developer ID–signed, notarized and stapled** (DMG, desktop and engine all VirusTotal-clean); Windows signing remains under consideration.

### v2.4 — Desktop UI-Only Shell

The desktop app became a **UI shell that owns no engine code**. It no longer embeds the engine; instead it launches the `xoksa` engine binary shipped beside it as a **child process** (`xoksa serve`) and shows its dashboard in the WebView. The `xoksa` dependency was removed from the desktop crate, so it structurally cannot compile any analysis logic — there is exactly one engine, driven identically by the CLI and the desktop (SOT). Two new non-interactive engine subcommands let the shell hand work to the engine instead of linking it: `xoksa apply-config` (writes `xoksa.env` + the OS keychain from a JSON payload on stdin — keys never touch argv or HTTP) and `xoksa ollama-models` (lists installed Ollama models as JSON). The dashboard, tunings, and results are unchanged; the desktop binary drops from ~22.8 MB to ~6.6 MB. See [design-philosophy.md](./design-philosophy.md) and [security-design.md §6](./security-design.md).

### v2.3 — Desktop App (Tauri)

XOKSA gained a **desktop app** so the tool reaches people who won't open a terminal, without giving up the lean single-binary CLI. It is a thin **Tauri** wrapper that starts the *same* engine in-process (a library call, not a child process) and shows the same dashboard in an OS-native WebView — a separate `xoksa-desktop` workspace, so the CLI keeps zero Tauri/WebView dependency. The two are one tool over one engine (identical results, SOT); they differ only in how you drive them — the CLI for automation, batch, and pipelines, the desktop app for interactive use. First launch shows a **GUI onboarding form** (language; LLM provider + API keys; cloud model; Ollama; news / fundamentals; and, under Advanced, indicator settings); keys cross into Rust over the Tauri IPC boundary and are written to the OS keychain, never over HTTP. The form doubles as an in-app **Settings** screen (⌘,) that pre-fills the current config and can **Save & Restart**, and manuals open in-app (`📖`) at the reader's language. The cloud-LLM **model is now selectable** (previously only Ollama was), and the default cloud models were refreshed to current balanced-tier releases. The macOS build is Developer ID–signed; Windows signing moves from deferred to a distribution prerequisite for this build. Why a separate binary and why Tauri: [design-philosophy.md](./design-philosophy.md); the security model: [security-design.md §6](./security-design.md).

### v2.2 — Multi-Timeframe, Backtest

An opt-in local database (bundled SQLite via `sqlx`) with a **📊 History** view was explored on this line but **removed before release (v2.2.2)** — nothing read the stored data back into any computation (analysis, backtest, and chart are always computed fresh from the provider), so it was pure risk for little value. Saved **backtest rules** now persist to a small JSON file (`~/.xoksa.strategies.json`) instead of a DB; `--private` remains a no-trace flag. **Multi-timeframe analysis** builds a Rust-side context pack (e.g. monthly MACD + daily Bollinger + 5m price) and has the LLM interpret it (**📐 MTF**). A usable **backtest** (**🧪**): a rule editor with templates and save/reuse, the *actual* rule shown in plain language, an always-on **Buy & Hold** benchmark, scale-aware period selection, and a realistic money model — cash in the instrument's own currency, whole minimum lots, a start fraction bought up front plus a per-signal amount. The market-chart popup scales to the window and supports **drag-select of an interval** attached as chat reference (the confirmed values, never an image). Structured NDJSON logs via `xoksa serve --log-format json`. The personal-data / portfolio (Class D) feature explored early in this line was **removed before release** — most sensitive yet never fed to the analysis, so highest risk for least unique value; a brokerage covers it better.

### v2.1 — Rust/WASM Frontend (Leptos), News Panel, Real Web Chat

The Web UI became a client-side **Leptos (Rust/WASM)** app, replacing the dependency-free JS UI: dark/light theme, S/M/L font size, reactive auto-refresh, resizable panels. Added a **News** panel and a **Help** screen, and a **real SSE-streamed chat** that is a true CLI superset — every slash command runs through one unified `ChatSession::execute` dispatch, so a command can never differ between CLI and Web. Added 1-minute bar mode (`1m`) and a dedicated Web UI manual. The legacy JS UI (`webui/`) was retired.

### v2.0 — Web UI / Local HTTP Server (first cut)

Introduced `xoksa serve --ui`: a local-first browser dashboard plus a JSON API (axum). `/summary` runs the **real native pipeline** and returns the exact CLI analysis text (SOT) — never mock data. Panels are independently resizable and persisted. The market-data → analysis pipeline was extracted into `app::build_analyzed_guard()` so the CLI and the server share one code path. The architecture boundary was set here: analysis, indicators, the LLM call, the DB, and backtests stay native; the browser is presentation only, reaching the engine solely over HTTP.

---

## v1.6 — Council/Board Mode, Design-Philosophy Refactor, Higher-Timeframe Modes

### v1.6.1 — Council/Board Mode

Chat mode gains structured multi-LLM debate capability. Previously, users could switch LLMs manually within a session using `/llm`, but each model answered in isolation with no shared context. The new Council mode lets multiple LLMs debate the same question in parallel rounds, with a Facilitator synthesizing results after each round — mirroring how a real investment committee would operate.

**What Council adds:**

- **Parallel participant rounds**: All registered LLMs are queried simultaneously and their responses are displayed in round order.
- **Facilitator synthesis**: After each participant round, the designated Facilitator LLM produces a structured summary — each participant's position, points of agreement, points of divergence, and focal questions for the next round.
- **Session continuity**: Each `/council ask` stores its question and all Facilitator reports to the Board. The next `/council ask` injects prior Facilitator reports as `[Session History]`, so the Council carries institutional memory across asks without the user having to repeat context.
- **Automatic closing round**: When `--rounds N` ends on a participant round (N is odd), a final Facilitator round is automatically appended. The discussion always ends with a structured synthesis.
- **Cost formula**: `ceil(N/2) × (P+1)` — calculate before running. With 3 participants and `--rounds 3`: 8 API calls. See the cost calculator in [usage-guide.md](../manual/usage-guide.md).
- **Debate Buffer integration**: Manual `/llm` switching with `/debate` continues to work as before — Council is the higher-cost option when you want structured parallel debate rather than sequential switching.

The Facilitator is prompted with a strict neutrality mandate even when the same LLM model acts as both a participant and the Facilitator. The role separation is enforced at the prompt level.

**Also in this version:**

- Gemini API error messages are now surfaced from the JSON response body (`error.message`). Previously, Gemini errors showed only a status code; the actionable message was silently discarded.
- `build_facilitator_prompt` bug fixed: on the 2nd+ `/council ask`, CouncilProposal entries were not scoped to the current ask — proposals from previous asks were incorrectly included. Fixed by filtering on `entry.id > user_question_id`.

### v1.6.2 — Codebase-wide refactoring to address design-philosophy violations

The codebase audit in v1.6.1 identified a gap between the stated principles in [design-philosophy.md](./design-philosophy.md) and the actual implementation. v1.6.2 closes that gap systematically.

The core issue was not that the product was broken — the SOT mechanism, Rust calculations, and LLM boundary were intact. The issue was that internal code violated the project's own readability and redundancy rules, making the codebase progressively harder to maintain and audit. For a product whose credibility rests on transparency and verifiability, that gap is not acceptable.

**What was fixed and why it mattered:**

- **Score float literals → `SignalStrength` enum**: `render.rs` matched against `2.0`, `1.0`, `0.0`, `-1.0`, `-2.0` as raw float literals. If `indicators.rs` ever returned `1.9999` due to floating-point arithmetic, the description would silently fall through to "unknown score." The coupling between the two files was implicit and undocumented. `SignalStrength` makes the contract type-enforced.

- **Buyer/Seller 4× duplication → single helper**: The same threshold logic (90/61/40/20) appeared in four separate match arms. A threshold change required four edits; one missed edit would cause Japanese and English to diverge silently. `verdict_mark_and_text()` centralizes this.

- **185-line function → split**: `compose_final_score_lines_stance` handled Buyer, Seller, and Holder logic in one function with four nested match levels. The three stances have fundamentally different rendering paths; they now live in separate functions.

- **Multi-state boolean → `RemovalState` enum**: `sanitize_ollama_line` returned `(Option<String>, bool)` where `true` meant "removed," "partially filtered," or "unsafe content detected" depending on context. Callers could not distinguish the cases. `RemovalState` names each state explicitly.

- **4-tuple 11-arm match → named predicates**: The signal score match in `indicators.rs` used an unlabeled 4-element tuple with 11 arms. The intent of each arm was invisible without tracing through the logic manually. Named boolean predicates (`macd_up_rsi_high_extreme`, `macd_up_rsi_low`, etc.) make each condition self-documenting.

- **Council round logic → cycle-based loop**: The previous implementation used odd/even parity (`% 2`) to alternate between participant and Facilitator rounds, plus a post-loop block to auto-append a closing Facilitator when rounds was odd. This was logic invented to paper over a poor round-numbering design. The replacement is a simple cycle loop: `for round in 1..=rounds` where each iteration runs all participants then the Facilitator.

### v1.6.3 — Weekly and Monthly Analysis Modes

This version adds higher-timeframe analysis modes while keeping the existing daily and intraday behavior intact.

The motivation is practical: when the user asks about the next one-week movement, daily data can be useful but may not always show the broader structure clearly, while 5-minute / 15-minute / 30-minute data is often too short-term. Weekly bars give xoksa a cleaner higher-timeframe view without changing the technical indicator formulas. Monthly bars are also supported for long-term context, although they are expected to be used less frequently than weekly bars.

**What changed:**

- **`--analysis-mode weekly`**: Adds weekly analysis mode. Aliases: `week`, `1wk`. Market data uses `interval=1wk` and `range=2y`.
- **`--analysis-mode monthly`**: Adds monthly analysis mode. Aliases: `month`, `1mo`. Market data uses `interval=1mo` and `range=10y`.
- **Default unchanged**: Omitting `--analysis-mode` still uses daily analysis.
- **Intraday unchanged**: `5m`, `15m`, and `30m` remain short-term modes.
- **No scoring model change**: Indicator formulas, thresholds, weights, and buy/sell scoring logic are unchanged. The same calculations run on a different bar timeframe.
- **Chat support**: `/mode weekly`, `/mode monthly`, `/show t weekly`, and `/show t monthly` are available in chat mode.
- **Autoreload scope unchanged**: `/autoreload` remains intraday-focused and does not run for daily, weekly, or monthly modes.
- **VWAP behavior clarified**: Intraday modes keep session VWAP behavior. Daily, weekly, and monthly modes use rolling VWAP based on the configured `--vwap-period` number of bars.
- **Prompt context**: LLM prompts and terminal labels now identify weekly/monthly analysis as higher-timeframe analysis, avoiding daily or intraday wording.

**Maintenance note:** This version also cleans up existing Clippy warning patterns so that `cargo clippy --all-targets -- -D warnings` passes.

---

## v1.5 — Multilingual Support, Runtime Switching, Multi-Instance Ollama, Secure Key Storage

### v1.5 — Multilingual Support (i18n)

Full English/Japanese bilingual support added to all user-facing output.

- **`--lang en/ja` CLI option**: Switches all terminal display, LLM prompts, and fundamental output between English and Japanese. Default is `"en"`.
- **`LANG` in `xoksa.env`**: Language can be set persistently. Intentionally reads from `env_map` (not the OS `LANG` variable) to avoid locale collisions.
- **Bilingual terminal output**: All user-facing strings in `render.rs`, `render_indicators.rs`, `news.rs`, `output.rs`, `app.rs`, `utils.rs`, `market.rs`, `bootstrap.rs`, and `config.rs` branch on `config.lang`.
- **Bilingual LLM prompts**: `llm.rs` generates English or Japanese prompt bodies including headings, time expressions, language directives, and system prompts (Ollama). API error messages and integrity guard wrapper strings are unconditionally in English.
- **Bilingual fundamental display**: `fundamental.rs` accepts a `lang` parameter; all labels, units, and market/currency identifiers branch on language.
- **`AnalysisMode` lang-aware methods**: `label(lang)`, `context(lang)`, `period_context(lang, periods)`, `bar_label(lang)`, `previous_close_label_l(lang)`, `price_diff_label_l(lang)` replace the previous `_ja`-suffix methods.
- **Score description i18n**: `utils::classify_score(ratio, lang)` and `utils::get_score_description(indicator, score, lang)` select between `SCORE_LEVELS_EN` / `SCORE_LEVELS` tables.
- **Known limitation at the time**: Ollama integrity guard detection keyword patterns were Japanese-only, as they matched Japanese LLM output. *(Resolved in v2.9.7: each check now holds its Japanese and English wording together, so it cannot exist in one language only.)*

### v1.5.1 — Runtime LLM/Model Switching

- **`/llm` command in chat mode**: Switch LLM provider and model at runtime without restarting the session.

### v1.5.3 — Multi-Instance Ollama Support

Running multiple local LLM servers is a natural setup when you want to compare a fast draft model against a larger reasoning model, or when different machines host models tuned for different tasks. Prior to this version, xoksa could only point to one Ollama server at a time, making that comparison require a config edit and a restart.

`OLLAMA_INSTANCES` solves this by letting you define named instances in `xoksa.env` once and switch between them with `/llm ollama:<alias>` during a chat session. The alias is whatever short name you choose — `fast`, `think`, `gpu2` — with no enforced length limit. The first instance in the list serves as the default, eliminating the need to separately specify `OLLAMA_HOST`, `OLLAMA_PORT`, and `ollama_model`.

The `--init` wizard adapts to your configuration: if you register a single server with the default alias, it writes the familiar `OLLAMA_HOST/PORT/MODEL` format for backward compatibility. If you assign a custom name or add more servers, it switches to `OLLAMA_INSTANCES=` automatically.

Five chat-mode quality improvements also ship in this version.

**`/mode` now auto-reloads data.** Previously, switching bar modes with `/mode 5m` only updated the internal state — the data being analyzed remained the daily bars from the initial load, and the user had to remember to run `/reload t` manually. The displayed technical snapshot would silently describe the wrong timeframe until that step was taken. The command now triggers an immediate data re-fetch, so the analysis context always matches the active bar mode.

**News URLs are always shown in LLM responses.** The LLM receives the URL of every news item alongside its title. Previously it would omit URLs in conversational responses, summarizing news by content alone. This is a deliberate policy choice: omitting source links when referencing news content is unfriendly to the sites being referenced. The chat constraint now makes URL attribution mandatory whenever the LLM mentions a news item.

**`/criticize` now detects data time differences.** Each Debate Buffer entry records the market data snapshot time (`data_as_of`) at the moment the LLM produced that response. When `/criticize` runs, the prompt includes both the session's current data time and the opinion's referenced data time. If they differ, the LLM classifies discrepancies as `possible discrepancy due to data time difference` rather than `contradicts SOT` — separating genuine analytical errors from differences caused by market data updates between LLM calls. The classification scheme was also restructured into six explicit categories: `confirmed in SOT`, `contradicts SOT`, `possible discrepancy due to data time difference`, `general technical interpretation`, `SOT-unverified`, and `article body not fetched — body-based claim unverifiable`.

**`/show t` now accepts an optional bar mode argument.** Previously `/show t [1-5]` always displayed technical data for the session's active bar mode. It now accepts an optional timeframe token (`daily`, `5m`, `15m`, `30m`) before the index — `/show t 5m 2` fetches and displays 5-minute bar data for ticker 2 without changing the session's active mode. Omitting the mode token preserves the prior behavior.

**`CHAT_DEFAULT_TICKER` now supports multiple tickers.** The `xoksa.env` setting now accepts a comma-separated list of up to 5 tickers (e.g. `AAPL,MSFT,NVDA`). On startup, the first ticker goes through the full analysis pipeline as before, and tickers 2–5 are automatically loaded via the same on-demand fetch used by `/ticker add`. If more than 5 are specified, a warning is shown and only the first 5 are used. Invalid tickers are skipped with a per-ticker warning. CLI `--ticker` continues to take precedence and suppresses the setting entirely.

### v1.5.4 — Secure Key Storage

Prior to this version, API keys were loaded from `xoksa.env` as plain-text entries. Keys were excluded from `env_map` and retrieved on demand via `resolve_api_key()`, but the source file itself was protected only by filesystem permissions. Any process with read access to the file could read all keys at once.

v1.5.4 moves Class A credentials (API keys, tokens) to the OS Keychain — macOS Keychain, Linux Secret Service, or Windows Credential Manager — so each key occupies its own isolated entry. The file-based path (`xoksa.env`) remains as an override: if a key is listed in the file, it takes priority over the keychain entry, preserving backward compatibility.

`--init` now prompts for keys and stores them directly in the OS Keychain. The `xoksa.env` file receives only commented-out placeholders, making it safe to inspect without exposing live credentials. `--update-key` provides a menu-driven way to overwrite any individual key without re-running `--init`. `--check-keys` reports which keys are set and from which source (file or keyring), without showing values.

The security hardening applied alongside the keychain feature:

- **Diagnostic reads stay `Zeroizing`**: `resolve_key_presence()` now delegates to `get_key()`, so credential material is wrapped in `Zeroizing<T>` even during `--doctor` / `--check-keys` presence checks. The retrieved value is dropped immediately after the presence test.
- **Rollback safety in `--init`**: Old keychain values are snapshotted before any write. If the snapshot read fails, the wizard aborts without touching the keychain or the file. If a write partially succeeds and a subsequent step fails, rollback failures are now reported to the user instead of being silently discarded.
- **Keyring access errors propagated**: `resolve_api_key()` now returns `Result<Option<Zeroizing<String>>>`. A keyring access failure is distinguishable from a key that is simply not set, and callers propagate the error rather than silently treating it as absent.
- **`/news-extra` SOT compliance**: Articles are stored as `(title, url)` pairs rather than a single concatenated string. URLs are never truncated. A boundary note ("Article bodies are not fetched") is injected into every prompt that includes extra news, matching the handling of the standard news channel.
- **`--doctor` J-Quants priority aligned to runtime**: The display order now matches the actual resolution order (`API_KEY` → `ID_TOKEN` → `REFRESH_TOKEN` → `EMAIL+PASSWORD`) so the displayed status reflects which credential will actually be used.

---

## v1.4 — Interactive Mode (Chat) and Fundamental Data

- **`--chat` mode**: After technical analysis, enters an interactive LLM session. Up to 5 tickers can be loaded simultaneously for comparison analysis. The normal analysis pipeline (data retrieval, indicator calculation, fundamental retrieval, news collection) completes in full before the chat loop starts.
- **`/prompt` command**: Sends `base_context` directly to the LLM at any point in the session — equivalent to executing the skipped initial LLM analysis on demand.
- **`--fundamental` option**: Fetches supplementary fundamental data (J-Quants for JP tickers, SEC EDGAR for US tickers) and incorporates it into the LLM prompt.
- **Context budget management (`ChatParams`)**: Each turn's LLM prompt is constructed to fit within `max_context_chars` specified by `--chat-memory`. Priority order: guard constraints + user question → recent turns → analysis context → news list / summary.
- **`CHAT_CONSTRAINT` mandatory injection**: Guard constraints are injected every turn to structurally suppress fact creation outside provided data.
- **Chat mode reference**: See [usage-guide.md](../manual/usage-guide.md).

---

## v1.3 — Module Splitting, Multi-Provider LLM, Intraday Mode

- **Module splitting completed**: Resolved the `main.rs` single-file structure. Modules: `config`, `market`, `technical`, `llm`, `news`, `render`, `render_indicators`, `app`, `output`, `prompt`, `bootstrap`, `traits`, `utils`.
- **Multi-provider LLM support**: Gemini, Claude, and Ollama (local LLM) added alongside OpenAI. `LlmDispatchSender` handles provider routing centrally.
- **Local LLM (Ollama) support**: Local LLMs operating without API keys. Connection via `OLLAMA_HOST` and `OLLAMA_PORT`. Technical indicator and score calculations remain on the Rust side; the LLM handles explanation only.
- **Intraday analysis mode**: `--analysis-mode 5m/15m/30m` for short-term intraday bar analysis. Prompt headings and time-axis expressions switch automatically by bar type.
- **Proxy support**: CONNECT-tunnel proxy support (`HTTPS_PROXY`, `HTTP_PROXY`, `NO_PROXY` in `xoksa.env`). Ollama always bypasses proxy via `.no_proxy()`.
- **LLM prompt quality**: Rules added prohibiting preambles, greetings, and meta-descriptions; strict date/tense usage rules (prohibiting uncertain expressions such as "next business day").
- **Claude dynamic parameter fallback**: Detects 400 error for `temperature` not supported and automatically resends without it. No model name hardcoding.
- **Gemini retry**: Up to 3 retries referencing `Retry-After` header for 429/503. Excludes `"thought": true` parts from thinking model responses.

---

## v1.2 — Indicator Parameter Customization

- **Extension indicator dispatch consolidation (`get_extension_evaluator`)**: Selection logic for indicator evaluation functions consolidated into a single function.
- **Indicator calculation parameter customization**: EMA/SMA periods, ADX period, Bollinger standard deviation multiplier, VWAP period, Ichimoku Conversion/Base line periods, ROC period, Stochastics period, and Fibonacci neutral band are configurable via CLI options and environment variables. All 13 parameters follow the CLI-first → environment variable → default value chain.

---

## v1.1 — Market Hours, API Key Security, Log Enhancement

- **Calibration function (`-I` option)**: Ignores `.env` indicator settings and runs with tool default values.
- **Full market hours support**: Analysis is based on the trading hours of the target market, not the system's local time.
- **Enhanced log output**: CSV/JSON integration adds analysis execution timestamps, improving usability as time-series data.
- **API key security**: Discontinued loading at process startup. Enforced on-demand retrieval immediately before use and complete in-memory erasure via `zeroize`.
- **Internal processing**: Discontinued intermediate file generation. In-memory processing and stream output established.
- **LLM prompt optimization**: Context combination of news and technical indicators restructured more naturally.

---

<a id="ja"></a>

# xoksa バージョン履歴

> **言語同期:** 英語と日本語の記載はいずれも有効です。要件・制約・例外・コマンド挙動・運用ルールが片方の言語にだけ存在する場合でも、両方に適用されます。欠落している側は文書不備として更新対象です。

開発側が履歴に残したい内容・ユーザーに伝えたい内容を自由記載で記述したドキュメントです。変更事実を定められたフォーマットで記録する [CHANGELOG.md](../../CHANGELOG.md) とは異なり、各機能の背景・設計の意図・実際の使い方といった文脈を自由な形式で補足します。設計原則の全体像については [design-philosophy.md](./design-philosophy.md) を参照してください。

> **更新方針:** 本ドキュメントは**マイナー版のマイルストーンのみ**（`vX.Y.z` の `Y`）を記録します。パッチ版（`vX.Y.z`）は [CHANGELOG.md](../../CHANGELOG.md) にのみ記録し、両ドキュメントの役割を明確に分けます。各マイナー節は、その系列のパッチ版で提供された変更をまとめています。

---

## v2 — Web UI／ローカルHTTPサーバ・Rust/WASMフロントエンド・バックテスト

### v2.9 — 日本株のリアルタイム価格・観測ベースの更新・出荷バイナリの監査

日本株の分足分析が「間違っている」状態を脱した。Yahoo の Chart API が返す東証価格は約15分遅延しており、それを最新値として扱っていたため、日本株の分足指標はすべて古い入力で計算されていた。現在はリアルタイムの価格と時刻を Yahoo! ファイナンス（日本）の公開ページから補い、形成中の足に織り込む。取得に失敗した場合は Chart API の値を使うので、分析全体が止まることはない。ダッシュボードの更新も推測をやめ、固定周期ではなく**データ元自身のデータ時刻**に追随し、その粒度でポーリングして、窓の内側では再取得・再分析せずキャッシュ済みの結果を返す。

デスクトップ系は二重に手当てした。**ローカル自動起動から v2.6.4 の堅牢化が失われていた**——固定ポートに戻っていたため、居残りの `xoksa serve` を黙って掴む、あるいは10秒の素のタイムアウトだけを残して起動に失敗する状態だった。自動起動のエンジンは OS 割り当てのエフェメラルポートに回帰し、同一性確認はバージョン完全一致となり、失敗はすべて原因を名乗る（起動失敗・終了ステータス付きの即時終了・ポート占有と占有者・401 と到達不能の区別）。

系列の締めくくりは、**出荷バイナリを正典と突き合わせた監査**である。きっかけは単純な疑問だった——マニュアルを直すとなぜバイナリのハッシュが変わるのか。マニュアルがバイナリにコンパイルされ、`/manual/:slug` として HTTP で配信されていたからで、しかもその面は**どの UI からもリンクされておらず、CLI にも出口が無かった**。埋め込みスクリーンショット・Markdown レンダラ・未使用の分析エンドポイント・呼び出し 0 件のデスクトップコマンドとともに削除した（約 1.4 MB 減）。同じ監査で、`--private`（無痕跡セッション）がユーザーの打った行を `~/.xoksa_history` に書いていたこと、設定アプリが実行していない再起動を成功と報告していたことが判明した——現在のセッションで自分が起動した子しか把握していないため、アプリを開き直したあとの「再起動」は何も停止せず、埋まったポートで即死するエンジンを起こし、それでも成功と表示して古いアクセストークンを生かし続けていた。再起動は正直な1操作になった：事前にポートを確認し、自分が起動していない占有者は名指しするだけで kill せず、新しいエンジンが待ち受けを始めてから成功を返す。ポート自体も、env ファイルだけの値から可視の設定項目になった。

系列の最後に、文書が掲げていた 3 つの保証を、実際にコードが行うことへ変えた。**計算できなかった指標**が有効な `0.0` として判定に入っていたため、生成されていない値で「終値 > 長期EMA」が成立し得た。現在は guard が「実際に計算された指標」を記録し、再計算に失敗すれば前回の値も消し、欠損指標を参照する条件は単に false になる——バックテストとアラート監視はその 1 つの表を共有するので、両者が食い違うことはない。**出力整合性ガード**は日本語限定の機能であることをやめ、プロンプトを照合の基準にすることもやめた。各検査が日本語表現と英語表現を同居させ、基準は `TechnicalDataGuard` と `FundamentalData` から構築してリクエストと共に渡す。この区別が重要なのは、プロンプトには会話や他モデルの見解も載っているからで、以前は利用者が打った数字とエンジンが書いた数字を区別できなかった。検証は回答単位ではなく**主張単位**になった。数値は、その節が帰属させている確定値と、銘柄・足・指標・単位・通貨・符号・時点で一致しなければならない。したがって「日足の終値は X、1時間足の終値は Y」は 2 つの主張として検査され、値を入れ替えた対は拒否される。**ファンダメンタルデータ**は検証済みのアクセス境界を持った——フィールドは非公開で、書き込む経路はいずれも書き込む値を検証し、プロバイダの取得結果からの初期構築は 1 か所に集約した（[design-philosophy.md §4.5](./design-philosophy.md)）。あわせて、各プロバイダ経路が抱えていた PER・PBR・ROE の重複した式も消えた。

### v2.8 — 独立した設定アプリと、別マシンから安全に届く `serve`

設定はデスクトップから外に出て、**`xoksa-setup`** という独立したパスワード保護アプリが全設定（API キー・LLM・指標・ニュース・通知・アクセストークン）を持つようになった。このアプリもエンジンのコードをリンクしない：エンジン自身のサブコマンド（`config-json`／`apply-config`／`settings-password`／`conn-token`）を駆動し、秘密は子プロセスの stdin を通って OS キーチェーンへ入る——argv も HTTP も通らない。デスクトップは純粋な**接続シェル**になった：*どのエンジンに繋ぐか*（自分で起動するローカル、またはアドレスとトークンで指定するリモート）を尋ね、WebView をそこへ向ける。

この分離こそがリモート利用を意味あるものにし、リモート利用が認証を必須にした。**非ループバックの bind は fail-closed** である：アクセストークンが無ければエンジンが OS の CSPRNG から 256bit を生成し、他の認証情報と同じ扱いで保管する（キーチェーン・`Zeroizing`・プロセス引数にもログにも出さない）。よって非ループバックのサーバが無認証で動くことはない。操作者自身が選んだ値ではないため、これは要求に応じて値を表示する唯一のクラス A 秘密であり（`serve --show-token`、`--rotate-serve-token` で再生成可）、ブラウザは初回にそれを `HttpOnly`／`SameSite=Strict` のセッション cookie と交換する。接続元はプライベート範囲とループバックのみを受け付け、公開インターネットではなく信頼できるネットワークの内側に留める。

### v2.7 — 設定ファイルの正準化と、デスクトップの表示専用化

同じ系列で SOT（単一の情報源）に関わる欠陥を2件直した。デスクトップは `xoksa.env` を**自分の作業ディレクトリ基準のリーダー**で読んでいた——第2の情報源である——ため、そのずれた見え方から保存し直すと本来の設定を上書きし得た：LLM を設定し直すと、クラウドの API キーを忘れ、ローカルモデルの設定が消えたように見えた。現在はエンジン（`xoksa config-json`）に現在の設定を尋ねる表示専用となり、保存し直しても全設定が保たれる。

設定ファイル自体も**唯一の正準・ユーザースコープの場所**（`%APPDATA%\xoksa`／`~/Library/Application Support/xoksa`／`~/.config/xoksa`）を得て、CLI・`serve`・デスクトップ・設定アプリで共有し、起動時に一度だけ非破壊で移行する。暗黙の `./xoksa.env` フォールバックは廃止したので、起動ディレクトリによって設定が分裂することはもう無い。日本株の銘柄名設定も容易になった：JPX の `data_j.xls` を直接読むため、CSV への変換が要らない。

### v2.6 — チャット・プッシュ通知／マニュアル統合／デスクトップアプリの出荷

（v2.5 系は存在しない。v2.4 の次が v2.6。）

XOKSA が**何かが起きたときに知らせて**くれるようになった。`xoksa serve` の起動中、セッション内の監視が確定した分足を見て、**計算済み値**の条件（例：`RSI ≤ 30`）が成立したら Slack / Discord / Google Chat（Incoming Webhook）または LINE（Messaging API）へ 1 行をプッシュする。送るのは確定した事実——銘柄・確定足の時刻・クロスした値・閾値——であって売買文言ではない。送信先ホストはコード固定のプラットフォーム別許可リストで `https` 限定、チャンネルの秘密はクラスA（OS キーチェーン。env ファイルには置かない）。ルールは製品機能として一級市民になった：ダッシュボードの**アラートパネル**がルールとチャンネルを一覧し、追加／削除／有効化／無効化／テストができ（`xoksa.env` に永続化）、チャットの `/alert` でも同じことができる。デスクトップの設定フォームは**保存前にチャンネルをテスト**でき、秘密は子プロセスの stdin を通り argv には載らない。J-Quants はこの系で **v2 専用**になった（V1 API は 2026-06-01 に提供終了）。

マニュアルは **9 本から 4 本へ統合**した：`usage-guide`（ダッシュボード・チャットとコマンド・ローカルLLM）、`analysis-guide`（分析の読み方・指標とスコア・戦略）、`setup`（導入＋連携）、`command-reference`。重複したガイドは同じ話題に 3 か所で答えられる状態を作り、それぞれが別々に古びていた。

残りは堅牢化と出荷である。デスクトップシェルはエンジンを**エフェメラルポート**で起動し、`/api/health` の版数一致を確認してから WebView を接続するようになり、居残った `serve` をアプリのエンジンと取り違えることがなくなった。エンジンの同梱は Tauri の **`externalBin` sidecar** に移り、Windows と macOS が同じ方式で梱包する。推論モデルの thinking 予算で切り詰められた Ollama の回答は、**努力度を段階的に下げて再試行**するようになった。`--debug-prompt` と `--no-llm` は**独立**したオプションになった。出荷検査そのものから 2 件のサプライチェーン所見が出た：`keyring` 4.0 の既定機能が**組込 SQL データベース（turso）**をバイナリに引き込んでおり——No-Database 設計に反する——プラットフォーム別のネイティブ資格情報ストアへ置き換えて除去した（`Cargo.lock` −3000 行）。もう 1 件は、貼り付けた webhook 秘密を平文で `xoksa.env` に書き得た通知設定のプロンプト。この系はさらに**アンチウイルス誤検知のケーススタディ**を生んだ：デスクトップバイナリの Defender / Trapmine 検知はコードではなく **PE リソースセクションのアイコン符号化**に起因し、Tauri の 3 バイトの MSI マーカーだけでクリーンなバイナリが `Trojan:Win32/Wacatac.B!ml` に転じた——ゆえに MSI はパッチ前の payload とハッシュ照合付きで作る（[av-false-positive-case-study.md](./av-false-positive-case-study.md)）。macOS は **Developer ID 署名＋公証＋staple 済み**で出荷（DMG・デスクトップ・エンジンの 3 本とも VirusTotal クリーン）、Windows の署名は検討中。

### v2.4 — デスクトップの UI シェル化

デスクトップアプリを、**エンジンのコードを一切持たない UI シェル**にしました。エンジンを内蔵せず、隣に置かれた `xoksa` エンジンバイナリを**子プロセス**（`xoksa serve`）として起動し、そのダッシュボードを WebView に表示します。デスクトップクレートから `xoksa` 依存を外したため、構造上いかなる分析ロジックもコンパイルできません——エンジンは一つだけで、CLI とデスクトップが同一に駆動します（SOT）。シェルがエンジンをリンクせずに処理を渡せるよう、非対話のサブコマンドを2つ追加：`xoksa apply-config`（stdin の JSON から `xoksa.env`＋OS キーチェーンを書く——鍵は argv も HTTP も通らない）と `xoksa ollama-models`（インストール済み Ollama モデルを JSON で列挙）。ダッシュボード・チューニング・結果は不変で、デスクトップバイナリは約22.8MBから約6.6MBに縮小。[design-philosophy.md](./design-philosophy.md)・[security-design.md §6](./security-design.md) 参照。

### v2.3 — デスクトップアプリ（Tauri）

XOKSA に**デスクトップアプリ**を追加し、ターミナルを開かない人にもツールを届けられるようにしました——軽量な単一バイナリの CLI はそのままです。同じエンジンをインプロセスで起動（ライブラリ呼び出し・子プロセスではない）し、OS ネイティブの WebView で同じダッシュボードを表示する薄い **Tauri** ラッパーで、独自の `xoksa-desktop` workspace のため CLI は Tauri/WebView 依存をゼロに保ちます。両者は一つのエンジン上の一つのツール（結果は同一・SOT）で、違いは「どう動かすか」だけ——CLI は自動化・バッチ・パイプライン、デスクトップは対話利用。初回起動で **GUI onboarding フォーム**（言語／LLM プロバイダ＋APIキー／クラウドモデル／Ollama／ニュース・ファンダ／詳細設定で指標）を表示します。キーは Tauri IPC 境界を通って Rust へ渡り、OS キーチェーンに書き込まれます（HTTP を通らない）。フォームはアプリ内**設定**画面（⌘,）も兼ね、現在値を事前入力して**保存して再起動**でき、マニュアルはアプリ内（`📖`）で読者の言語で開きます。クラウド LLM の**モデルを選択可能**に（従来は Ollama のみ）、既定クラウドモデルも現行の中位ティアへ更新。macOS ビルドは Developer ID 署名済み。Windows 署名は本ビルドでは「先送り」から配布の前提条件へ。別バイナリと Tauri を選んだ理由：[design-philosophy.md](./design-philosophy.md)。セキュリティモデル：[security-design.md §6](./security-design.md)。

### v2.2 — マルチタイムフレーム・バックテスト

本系列ではオプトインのローカルDB（bundled SQLite ＋ `sqlx`）と**📊 履歴**ビューを検討しましたが、**リリース前（v2.2.2）に撤去**しました——保存したデータを計算に読み戻す経路が一つも無く（分析・バックテスト・チャートは常にプロバイダから都度取得）、リスクだけで価値が薄かったためです。保存した**バックテストのルール**は DB ではなく小さな JSON ファイル（`~/.xoksa.strategies.json`）に永続化します。`--private` は従来どおり痕跡なしフラグです。**マルチタイムフレーム分析**はRust側でcontext pack（例：月足MACD＋日足ボリンジャー＋5分足価格）を組み、LLMに解釈させます（**📐 MTF**）。実用的な**バックテスト**（**🧪**）：テンプレと保存/呼び出しを備えたルールエディタ、*実際の*ルールを平易な言葉で表示、常時併記の**Buy&Hold**、足種連動の期間選択、そして現実的な資金モデル——銘柄自身の通貨での現金、最小単元単位、開始時に一定割合を買い＋シグナルごとの増減。市況チャートのポップアップはウィンドウに追従し、**区間のドラッグ選択**をチャットの参照として添付（画像ではなく確定値）。`xoksa serve --log-format json` で構造化NDJSONログ。本系列の初期に検討した個人データ／ポートフォリオ（クラスD）機能は**リリース前に撤去**——最も機密なのに分析へ渡さず、リスク最大・独自価値最小で、証券会社の方が上。

### v2.1 — Rust/WASMフロントエンド（Leptos）・Newsパネル・実チャット

Web UI を依存ゼロの JS UI から**Leptos（Rust/WASM）**のクライアントサイドアプリへ刷新：ダーク/ライトテーマ、フォント大中小、リアクティブ自動更新、リサイズ可能なパネル。**News**パネルと**Help**画面、そして**SSEストリーミングの実チャット**（真のCLI上位互換——全スラッシュコマンドが単一の `ChatSession::execute` を通るので、CLIとWebで結果が食い違わない）を追加。1分足（`1m`）とWeb UI専用マニュアルも追加。旧 JS UI（`webui/`）は廃止。

### v2.0 — Web UI／ローカルHTTPサーバ（初版）

`xoksa serve --ui` を導入：ローカルファーストのブラウザ用ダッシュボード＋JSON API（axum）。`/summary` は**実際のネイティブパイプライン**を実行し、CLIと同一の分析テキスト（SOT）を返します（モックではない）。パネルは独立リサイズ・保存。市場データ→分析のパイプラインを `app::build_analyzed_guard()` に抽出し、CLIとサーバで単一コード共有。ここでアーキテクチャ境界を確立：分析・指標・LLM・DB・バックテストはネイティブに残し、ブラウザは表示専用でエンジンへはHTTP経由のみ。

---

## v1.6 — Council/Boardモード・設計思想リファクタ・上位足モード

### v1.6.1 — Council/Boardモード

チャットモードに構造化されたマルチLLM議論機能を追加した。従来は `/llm` で手動切り替えをしながら各LLMに個別に質問する方式だったが、LLMどうしは互いの回答を知らず、文脈の積み上げもなかった。Councilモードでは、複数のLLMが同一の質問に並列で回答し、Facilitatorが各ラウンドを整理する。現実の投資委員会に近い構造を、チャットセッションの中で再現するものだ。

**Councilが加えるもの：**

- **並列参加者ラウンド**: 登録したすべてのLLMに同時に問い合わせ、回答をラウンド順に表示する。
- **Facilitatorによる整理**: 各参加者ラウンドの後、指定したFacilitator LLMが構造化サマリーを生成する — 各参加者の立場・合意点・対立点・次ラウンドへの問い。
- **セッション継続性**: `/council ask` のたびにその質問と全FacilitatorレポートがBoardに記録される。次の `/council ask` では過去のFacilitatorレポートが `[会議履歴]` として注入され、ユーザーが文脈を繰り返さなくてもCouncilが記憶を引き継ぐ。
- **自動最終ラウンド**: `--rounds N` でNが奇数（最終ラウンドが参加者ラウンド）の場合、Facilitatorラウンドが自動追加される。議論は必ず構造化されたまとめで終わる。
- **コスト計算式**: `ceil(N/2) × (P+1)` — 実行前に必ず計算する。参加者3名 + `--rounds 3` の場合は8回のAPI呼び出し。詳細は [usage-guide.md](../manual/usage-guide.md) を参照。
- **Debate Bufferとの併用**: `/llm` 手動切り替え + `/debate` による逐次比較は従来どおり動作する。CouncilはDebate Bufferより高コストだが、並列構造化議論が必要な場面での選択肢になる。

FacilitatorはプロンプトレベルでLLMに中立性を徹底させる。同一モデルが参加者とFacilitatorを兼任する場合も、プロンプトの役割分離によって中立性を維持するよう指示している。

**同バージョンの修正：**

- GeminiのAPIエラーメッセージをレスポンスボディの `error.message` から読み取り表示するようにした。従来はステータスコードのみで、原因となるメッセージはサイレント破棄されていた。
- `build_facilitator_prompt` のバグを修正：2回目以降の `/council ask` で、前のaskの参加者回答（CouncilProposal）が誤って混入していた。`entry.id > user_question_id` のフィルタを追加して修正。

### v1.6.2 — Codebase-wide refactoring to address design-philosophy violations

v1.6.1 のコードベース監査で、[design-philosophy.md](./design-philosophy.md) に記載された原則と実装の乖離が明らかになった。v1.6.2 はその乖離を体系的に解消する。

プロダクトが壊れていたわけではない。SOTの仕組み・Rust計算・LLMの境界は設計通りに機能していた。問題は、内部コードがプロジェクト自身の視認性・冗長化禁止ルールに違反しており、保守性と監査可能性が損なわれていたことだ。透明性と検証可能性を売りにするプロダクトにとって、その乖離は許容できない。

**修正内容とその理由:**

- **スコアfloatリテラル → `SignalStrength` enum**: `render.rs` が `2.0`・`1.0`・`0.0`・`-1.0`・`-2.0` をfloatリテラルで直接マッチしていた。`indicators.rs` が浮動小数点演算の結果 `1.9999` を返した場合、説明が「不明スコア」にサイレントフォールスルーする。2ファイル間の結合が暗黙的だった。`SignalStrength` により型レベルで保証される。

- **Buyer/Seller 4重複 → helper関数**: 同一の閾値ロジック（90/61/40/20）が4つのmatch armに重複していた。閾値変更時に4箇所を修正する必要があり、1箇所でも漏れると日英で挙動が乖離する。`verdict_mark_and_text()` に集約した。

- **185行関数 → 分割**: `compose_final_score_lines_stance` がBuyer・Seller・Holderの3スタンスを4重ネストmatchで処理していた。3スタンスは本質的に異なる描画パスを持つため、個別関数に分割した。

- **多値boolean → `RemovalState` enum**: `sanitize_ollama_line` が `(Option<String>, bool)` を返していたが、`true` の意味が「削除した」「一部削除した」「unsafeを検出した」の3通りに混在していた。`RemovalState` で各状態を明示的に命名した。

- **4タプル11armマッチ → 名前付き述語**: `indicators.rs` のシグナルスコア計算がラベルなし4要素タプルで11armを判定していた。各armの意図がコードから読み取れなかった。`macd_up_rsi_high_extreme`・`macd_up_rsi_low` 等の名前付き述語で各条件が自己説明的になった。

- **Council奇偶ロジック → サイクルループ**: 旧実装は奇偶判定（`% 2`）で参加者ラウンドとFacilitatorラウンドを交互判定し、ループ後のブロックで奇数ラウンド時に自動追加していた。これは不適切なラウンド番号設計のほころびを後付けで塞いだものだった。`for round in 1..=rounds` のサイクルループ（各サイクル = 全参加者 → Facilitator）に置き換えた。

### v1.6.3 — 週足・月足分析モード

既存の日足・分足の挙動を維持したまま、上位足の分析モードを追加した。

背景は実用上の要請である。ユーザーが「今後1週間の動き」を聞く場合、日足だけでは大きな構造が見えにくいことがあり、一方で5分足・15分足・30分足は短期に寄りすぎる。週足は、テクニカル指標の計算式を変えずに、より大きな時間軸の見方を提供できる。月足も長期確認用として対応するが、利用頻度は週足より低い想定である。

**変更内容:**

- **`--analysis-mode weekly`**: 週足分析モードを追加。別名は `week`, `1wk`。市場データは `interval=1wk`, `range=2y` を使用する。
- **`--analysis-mode monthly`**: 月足分析モードを追加。別名は `month`, `1mo`。市場データは `interval=1mo`, `range=10y` を使用する。
- **デフォルトは不変**: `--analysis-mode` を指定しない場合は、従来どおり日足分析を使う。
- **分足は不変**: `5m`, `15m`, `30m` は短期分析モードとして維持する。
- **スコアモデルは不変**: 指標計算式・しきい値・重み・売買スコア判定ロジックは変更しない。同じ計算を別の足種に対して実行する。
- **チャット対応**: チャットモードで `/mode weekly`, `/mode monthly`, `/show t weekly`, `/show t monthly` を利用できる。
- **自動リロード範囲は不変**: `/autoreload` は分足向けの機能として維持し、日足・週足・月足では動作させない。
- **VWAPの扱いを明確化**: 分足モードはセッションVWAPの扱いを維持する。日足・週足・月足では、設定された `--vwap-period` 本数に基づくローリングVWAPを使う。
- **プロンプト文脈**: LLMプロンプトと端末表示では、週足・月足を上位足分析として明示し、日足や分足の文言と混同しないようにした。

**メンテナンス補足:** 既存のClippy警告パターンも整理し、`cargo clippy --all-targets -- -D warnings` が通過する状態にした。

---

## v1.5 — 多言語対応・実行時切り替え・Ollamaマルチインスタンス・セキュアキー保管

### v1.5 — 多言語対応（i18n）

ユーザー向けすべての出力に英語／日本語のバイリンガル対応を追加。

- **`--lang en/ja` CLIオプション**: ターミナル表示・LLMプロンプト・ファンダメンタル出力の言語を切り替え。デフォルトは `"en"`。
- **`xoksa.env` の `LANG` 設定**: 言語を恒久的に設定可能。システムのロケール変数（`ja_JP.UTF-8` 等）との衝突を避けるため、意図的に `env_map` からのみ読み取る。
- **ターミナル出力の二言語化**: `render.rs`・`render_indicators.rs`・`news.rs`・`output.rs`・`app.rs`・`utils.rs`・`market.rs`・`bootstrap.rs`・`config.rs` のユーザー向け文字列がすべて `config.lang` で分岐。
- **LLMプロンプトの二言語化**: `llm.rs` が英語・日本語のプロンプト本文を生成。APIエラーメッセージと整合性ガードのラッパー文字列は無条件に英語。
- **ファンダメンタル表示の二言語化**: `fundamental.rs` に `lang` パラメータを追加。すべてのラベル・単位・market/currency識別子が言語で分岐。
- **`AnalysisMode` の言語対応メソッド**: `label(lang)`・`context(lang)`・`period_context(lang, periods)`・`bar_label(lang)`・`previous_close_label_l(lang)`・`price_diff_label_l(lang)` が旧来の `_ja` サフィックスメソッドを置き換え。
- **スコア説明の i18n**: `utils::classify_score(ratio, lang)` と `utils::get_score_description(indicator, score, lang)` が言語に応じてテーブルを選択。
- **当時の既知の制限**: Ollama整合性ガードの検出キーワードパターンは日本語LLM出力を対象とするため日本語のままだった。*（v2.9.7 で解消。各検査が日本語表現と英語表現を同居させる形になり、片方の言語にしか無い検査は存在しない。）*

### v1.5.1 — 実行時LLM/モデル切り替え

- **チャットモードの `/llm` コマンド**: セッションを再起動せず、実行時にLLMプロバイダーとモデルを切り替え可能。

### v1.5.3 — Ollamaマルチインスタンス対応

- **`OLLAMA_INSTANCES=alias:model@host:port,...`**: `xoksa.env` で複数のOllamaサーバーを名前付きで定義できる。高速モデルと大規模推論モデルを別々のマシンで動かして比較するなど、複数台構成のユースケースに対応。
- **チャットモードの `/llm ollama:<alias>` 切り替え**: セッションを再起動せず、エイリアス指定で即座にサーバーを切り替え可能。エイリアス名は `fast`・`think`・`gpu2` など自由に設定でき、文字数制限はない。
- **先頭エントリの自動デフォルト適用**: `OLLAMA_INSTANCES` の先頭エントリが `OLLAMA_HOST`・`OLLAMA_PORT`・`ollama_model` に自動適用されるため、重複設定は不要。
- **`--init` ウィザードの自動判定**: デフォルトエイリアスで1台登録の場合は `OLLAMA_HOST/PORT/MODEL` 形式、名前付きまたは複数台の場合は `OLLAMA_INSTANCES=` 形式を自動生成。

同バージョンでチャットモードの品質改善も5点行っている。

**`/mode` での自動データ再取得。** これまで `/mode 5m` で足を切り替えても内部状態が変わるだけで、分析対象のデータは初回ロード時の日足のままだった。手動で `/reload t` を実行するまで、表示されるテクニカル情報は指定した足とは別のものを示し続けていた。コマンド実行と同時にデータを再取得するよう改め、分析コンテキストが常に選択中の足と一致するようにした。

**チャット応答でのニュースURLの常時表示。** LLMは各ニュースのURLをタイトルとともに受け取っている。これまでは会話形式の返答でURLを省略し、内容だけを要約することがあった。ニュース記事を参照しながら出典リンクを示さないのは参照先サイトへの配慮を欠く。チャット制約にURL明記を必須ルールとして追加し、ニュースに言及するたびに必ずURLを表示するようにした。

**`/criticize` によるデータ時点差の検出。** Debate Buffer の各エントリに、LLMが回答を生成した時点の市場データスナップショット時刻（`data_as_of`）を記録するようにした。`/criticize` 実行時には、現在のセッションのデータ時刻と当該見解が参照したデータ時刻の両方がプロンプトに提示される。時刻が異なる場合、LLMは差異を `xoksaデータと矛盾` ではなく `データ時点差による不一致の可能性` に分類するよう指示され、真の分析誤りとLLM呼び出し間のデータ更新によって生じた見かけの差異が区別される。分類体系も6カテゴリに整理された：`xoksaデータで確認可能`・`xoksaデータと矛盾`・`データ時点差による不一致の可能性`・`一般的なテクニカル解釈`・`xoksaデータでは確認不能`・`ニュース本文未取得のため本文根拠なし`。

**`/show t` への足種引数の追加。** これまで `/show t [1-5]` は常にセッションのアクティブな足のデータを表示していた。オプションで足種トークン（`daily`・`5m`・`15m`・`30m`）をインデックスの前に指定できるようになった。例えば `/show t 5m 2` は銘柄2の5分足データをオンデマンドで取得・表示し、セッションのアクティブな足は変更しない。足種省略時は従来どおりの動作を保持する。

**`CHAT_DEFAULT_TICKER` の複数銘柄対応。** `xoksa.env` のこの設定がカンマ区切りで最大5件の銘柄リストを受け付けるようになった（例：`AAPL,MSFT,NVDA`）。起動時、先頭銘柄は従来どおり全分析パイプラインを経由し、2〜5番目の銘柄は `/ticker add` と同じオンデマンド取得で自動ロードされる。6件以上を指定した場合は警告を表示して先頭5件のみ使用する。無効な銘柄は個別に警告を表示してスキップする。CLIの `--ticker` は引き続き優先され、`CHAT_DEFAULT_TICKER` は完全に無視される。

### v1.5.4 — セキュアキー保管

このバージョン以前は、APIキーは `xoksa.env` のプレーンテキストとして保管されていた。`env_map` には格納せずオンデマンド取得していたものの、ソースファイル自体はファイルシステムパーミッションのみで保護されており、読み取り権限があればすべてのキーを一度に参照できる状態だった。

v1.5.4 では、クラスA認証情報（APIキー・トークン）をOSキーチェーン（macOS Keychain / Linux Secret Service / Windows Credential Manager）に移行し、各キーが独立したエントリに格納されるようにした。`xoksa.env` へのファイルベースのパスはオーバーライドとして残しており、ファイルにキーが記載されていれば常に優先されるため、後方互換性は保たれている。

`--init` はキー入力を促してOSキーチェーンに直接保存するよう変更した。`xoksa.env` にはコメントアウトされたプレースホルダーのみが記載され、ライブの認証情報を露出させることなくファイルを安全に確認できる。`--update-key` は `--init` を再実行せずに個別キーを上書きするメニュー方式の手段を提供する。`--check-keys` は各キーの有無と保管元（ファイルまたはキーリング）を値を表示せずに報告する。

キーチェーン機能と並行して適用したセキュリティ強化：

- **診断パスの `Zeroizing` 維持**: `resolve_key_presence()` が `get_key()` に委譲するようになり、`--doctor` / `--check-keys` の存在確認でも認証情報が `Zeroizing<T>` で保持される。取得した値は存在確認の直後にドロップされる。
- **`--init` のロールバック安全性**: 書き込み前に既存のキーチェーン値をスナップショット取得する。スナップショット読み取りに失敗した場合は、キーチェーンもファイルも変更せずに処理を中断する。書き込みの一部が成功した後に後続ステップが失敗した場合、ロールバックの失敗がユーザーに報告されるようになった（以前はサイレントに破棄していた）。
- **キーリングアクセスエラーの伝播**: `resolve_api_key()` が `Result<Option<Zeroizing<String>>>` を返すようになった。キーリングアクセス失敗は単純に未設定とは区別され、呼び出し元がエラーを伝播する。
- **`/news-extra` のSOT準拠**: 記事を連結文字列ではなく `(title, url)` ペアで保管するよう変更。URLが途中で切り捨てられることがなくなった。追加ニュースを含むすべてのプロンプトに境界テキスト（「記事本文は取得しない」）を注入し、標準ニュースチャネルと同等の扱いにした。
- **`--doctor` のJ-Quants優先順位を実行時と一致**: 表示順序が実際の解決順序（`API_KEY` → `ID_TOKEN` → `REFRESH_TOKEN` → `EMAIL+PASSWORD`）と一致するようになり、実際に使われる認証情報が正確に反映される。

---

## v1.4 — 対話モード（チャット）・ファンダメンタルデータ

- **`--chat` モード**: テクニカル分析後にLLMとの対話セッションに入る。最大5銘柄を同時にロードして比較分析が可能。通常の分析パイプライン（データ取得・指標演算・ファンダメンタル取得・ニュース収集）はすべて完了した状態でチャットループへ入る。
- **`/prompt` コマンド**: `base_context` をそのままLLMへ送信。`--chat` でスキップされた初回LLM分析を任意のタイミングで実行することに相当。
- **`--fundamental` オプション**: 補助的なファンダメンタルデータ（JP銘柄はJ-Quants、US銘柄はSEC EDGAR）を取得してLLMプロンプトに組み込む。
- **文脈予算管理（`ChatParams`）**: 各ターンのLLMプロンプトは `--chat-memory` で指定した `max_context_chars` に収まるよう構成。優先順位：ガード制約＋ユーザー質問 → 直近ターン → 分析コンテキスト → ニュース一覧/要約。
- **`CHAT_CONSTRAINT` 必須注入**: 毎ターンガード制約を注入し、入力データ外での事実創作を構造的に抑止。
- **チャットモードリファレンス**: [usage-guide.md](../manual/usage-guide.md) を参照。

---

## v1.3 — モジュール分割・マルチプロバイダーLLM・分足モード

- **モジュール分割の完了**: `main.rs` 単一ファイル構造を解消。モジュール：`config`・`market`・`technical`・`llm`・`news`・`render`・`render_indicators`・`app`・`output`・`prompt`・`bootstrap`・`traits`・`utils`。
- **マルチプロバイダーLLM対応**: OpenAIに加えGemini・Claude・Ollama（ローカルLLM）をサポート。`LlmDispatchSender` がプロバイダーへのルーティングを統一的に担う。
- **ローカルLLM（Ollama）対応**: APIキー不要でローカルに動作するLLMを利用可能。`OLLAMA_HOST`・`OLLAMA_PORT` で接続先を指定。テクニカル指標・スコア計算はRust側、LLMは説明のみ。
- **分足分析モード追加**: `--analysis-mode 5m/15m/30m` による日中足の短期分析をサポート。足の種別に応じたプロンプト見出し・時間軸表現の自動切り替え。
- **プロキシ対応**: CONNECTトンネル方式プロキシをサポート（`HTTPS_PROXY`・`HTTP_PROXY`・`NO_PROXY`）。Ollamaはローカルのため `.no_proxy()` でプロキシを常時無効化。
- **LLMプロンプト品質強化**: 前置き・挨拶・メタ説明の出力禁止ルール、日付・時制の厳密化ルールをプロンプトに追加。
- **Claude 動的パラメータフォールバック**: `temperature` 非対応時の400エラーを検出して自動的に再送。モデル名をハードコードしない。
- **Gemini リトライ強化**: 429/503 に対して `Retry-After` ヘッダーを参照したリトライ（最大3回）。thinkingモデルの `"thought": true` パーツを除外してレスポンス本文を正しく抽出。

---

## v1.2 — 指標計算パラメータのカスタマイズ

- **拡張指標ディスパッチの集約（`get_extension_evaluator`）**: 指標評価関数の選択ロジックを単一の関数に集約。
- **指標計算パラメータのカスタマイズ**: EMA/SMA 期間、ADX 期間、Bollinger 標準偏差倍率、VWAP 期間、一目均衡表の転換線/基準線期間、ROC 期間、Stochastics 期間、Fibonacci 中立幅など、主要な指標計算パラメータをCLIオプションおよび環境変数から設定可能。全13パラメータがCLI最優先・環境変数・デフォルト値のチェーンで動作。

---

## v1.1 — 市場時間対応・APIキーセキュリティ・ログ強化

- **キャリブレーション機能（`-I` オプション）**: `.env` のインジケーター設定を無視し、ツール既定値で動かす機能。
- **市場時間への完全対応**: システムのローカル時刻ではなく、対象市場の取引時間に準拠した正確な時間枠での解析を実現。
- **ログ出力の強化**: CSV/JSON連携において解析実行時のタイムスタンプ項目を追加。
- **APIキー・セキュリティの極大化**: プロセス起動時の読み込みを廃止。使用直前のオンデマンド取得と `zeroize` によるメモリ完全消去を徹底。
- **内部処理の軽量化**: 不要な中間ファイルの生成を廃止し、メモリ内処理とストリーム出力を確立。
- **LLMプロンプトの最適化**: ニュースとテクニカル指標のコンテキスト結合をより自然に再構成。
