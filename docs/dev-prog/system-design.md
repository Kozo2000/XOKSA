# xoksa Architecture Overview

[日本語ドキュメントはこちら。](#ja)


```
┌─────────────────┐                                          ┌─────────────────┐
│  DATA SOURCES   │                                          │  LLM PROVIDERS  │
│                 │                                          │                 │
│ ┌─────────────┐ │         ┌───────────────────────┐        │ ┌─────────────┐ │
│ │Yahoo Finance│ │──────▶  │                       │  ◀──── │ │   OpenAI    │ │
│ │     API     │ │         │     xoksa engine      │        │ │gpt-5.6-terra│ │
│ │Price OHLCV  │ │         │                       │        │ └─────────────┘ │
│ │Market Data  │ │         │ ┌───────────────────┐ │        │                 │
│ └─────────────┘ │         │ │ TechnicalDataGuard│ │        │ ┌─────────────┐ │
│                 │         │ │   SOT / Integrity │ │  ◀──── │ │   Gemini    │ │
│ ┌─────────────┐ │         │ └───────────────────┘ │        │ │3.5-flash    │ │
│ │Brave Search │ │──────▶  │ ┌───────────────────┐ │        │ └─────────────┘ │
│ │     API     │ │         │ │    LLM Engine     │ │        │                 │
│ │Latest News  │ │         │ │OpenAI/Gemini/     │ │  ◀──── │ ┌─────────────┐ │
│ │Title + URL  │ │         │ │Claude/Ollama      │ │        │ │   Claude    │ │
│ └─────────────┘ │         │ └───────────────────┘ │        │ │sonnet-5     │ │
│                 │         │ ┌───────────────────┐ │        │ └─────────────┘ │
│ ┌─────────────┐ │         │ │   Chat & Forum    │ │        │                 │
│ │  J-Quants   │ │──────▶  │ │Chat / Parallel    │ │  ◀──── │ ┌─────────────┐ │
│ │  SEC EDGAR  │ │         │ │Forum/Facilitator  │ │        │ │   Ollama    │ │
│ │  JP/US      │ │         │ └───────────────────┘ │        │ │Up to 16     │ │
│ │  Fundamental│ │         │ ┌───────────────────┐ │        │ │instances    │ │
│ └─────────────┘ │         │ │   News Triage     │ │        │ │Local LLM    │ │
└─────────────────┘         │ │Title + URL only   │ │        │ └─────────────┘ │
                            │ └───────────────────┘ │        └─────────────────┘
                            │ ┌───────────────────┐ │
                            │ │ Config & Security │ │
                            │ │OS Keychain/zeroize│ │
                            │ └───────────────────┘ │
                            │ ┌───────────────────┐ │
                            │ │  Backtest / MTF   │ │
                            │ └───────────────────┘ │
                            │ ┌───────────────────┐ │        ┌─────────────────┐
                            │ │  Alert & Notify   │ │──────▶ │ Slack / Discord │
                            │ │ SOT threshold →   │ │        │ Google Chat /   │
                            │ │ https POST only   │ │        │ LINE (fixed     │
                            │ └───────────────────┘ │        │ host allowlist) │
                            │                       │        └─────────────────┘
                            │ Rust 100% · native engine + Rust/WASM web UI
                            └───────────────────────┘
                                       │
              ┌────────────────────────┼──────────────────────────┐
              ▼                        ▼                           ▼
        ┌───────────┐         ┌──────────────┐          ┌────────────────────────┐
        │ Terminal  │         │  CSV / JSON  │          │   Browser dashboard     │
        │  Display  │         │     Log      │          │   Rust/WASM (Leptos)    │
        │   (CLI)   │         │              │          │   via built-in HTTP     │
        └───────────┘         └──────────────┘          │   server + JSON API     │
                                                         │   (no Apache/nginx)     │
                                                         └────────────────────────┘
```

## Components

The diagram is high-level; the engine's main subsystems (all Rust, native side):

- **SOT — `TechnicalDataGuard` / `FundamentalData`** — indicators, scores and fundamentals are computed in Rust and reached only through a validated boundary, so every surface reads identical values. The two boundaries have different shapes for the same reason: the technical guard records **which indicators produced a value**, so an uncomputed one is absent rather than a zero; the fundamental struct keeps every field private and **validates on every write path**, deriving PER / PBR / ROE once inside the confirmed data.
- **LLM engine** — OpenAI / Gemini / Claude / Ollama (local, multi-instance), switchable per session. The same source-of-truth reinforcement (no fabrication, quote numbers verbatim, news titles+URLs only) is prepended **identically for every provider** — there is no per-provider prompt.
- **Chat & Forum** — a single unified dispatch (`ChatSession::execute`) shared by the CLI and the Web UI; `/forum` runs several LLMs in parallel with a Chair synthesis.
- **News triage** — Brave Search titles + URLs only (never article bodies), filtered for relevance and sorted by recency.
- **Multi-timeframe context** — `build_context_pack` analyzes several timeframes once through the shared engine and hands the LLM a Rust-built summary (`ContextPack`, never raw bulk data).
- **Backtest** — a rule engine over a contiguous provider history; realistic sizing (start fraction + per-signal, minimum lots, instrument currency) with an always-on Buy & Hold benchmark. Saved backtest rules (the rule editor's save/reuse) persist to a small local JSON file (`~/.xoksa.strategies.json`) — the only thing written to disk besides the log.
- **Alert & notify** — an in-session monitor inside `serve` watches the bars at their own cadence and, when a user-defined condition on a **computed** value is met, https-POSTs a one-line message to a chat platform (Slack / Discord / Google Chat webhooks; LINE Messaging API). The destination host is a hardcoded per-platform allowlist, the message carries the SOT fact (never an LLM-authored trade call), and it fires once per (ticker, condition, indicator bar) on a rising edge. See [security-design.md §4](./security-design.md).
- **Config & Security** — one `xoksa.env` for CLI and Web; API keys in the OS keychain, zeroized after use.
- **Structured logging** — dependency-free coded logger (`logs/xoksa-error.log` under the working directory; absolute path printed at startup); `xoksa serve --log-format json` emits NDJSON.

Surfaces (presentation only): the **CLI** terminal display, the **local log file**, the **browser dashboard** (Rust/WASM Leptos served by the built-in HTTP server over a JSON API), and the **desktop app** (a Tauri connection shell that launches the engine as a child process and hosts its dashboard in an OS WebView; configuration is a companion settings app, `xoksa-setup`; see [security-design.md §6](./security-design.md)). Analysis, indicators, the LLM call, and backtests never move to the browser/WASM layer.

- **Self-contained CLI binary** — the `xoksa` CLI / `serve` release binary is built with the `embedded-ui` cargo feature, which embeds the built Web UI (`webui-leptos/dist`, via `include_dir!`) into the executable. A downloaded release therefore serves the dashboard with **no external `dist/` files**. `server::mod`'s asset serving prefers an on-disk `--web-dir` (dev workflow), then falls back to the embedded assets; developers building from source still use `trunk build` + `dist/`.

## Distribution — shipped artifacts

A release produces **two artifacts over one engine**; users pick by how they want to run xoksa.

| Artifact | What it is | Package (per OS) | For |
| :--- | :--- | :--- | :--- |
| **`xoksa` (CLI)** | Single self-contained binary — the engine plus the embedded Web UI (`embedded-ui`); also serves the dashboard via `serve --ui`. | zip of the executable (Windows / macOS); build from source (Linux) | automation, batch / pipelines, scripting, integration with other tools, servers |
| **XOKSA (desktop)** | A **Tauri** connection shell that launches the *same* engine as a **child process** (`xoksa serve`) and shows its dashboard; the package also carries the standalone settings app (`xoksa-setup`) for configuration. | macOS `.app` / `.dmg`; Windows `.msi` (NSIS was measured and rejected — its container structure trips AV heuristics). Linux is buildable from source but **not shipped**. | beginners / non-terminal users |

- **One engine, no fork.** The desktop app and the settings app are **separate workspaces** that link **no engine code** (no `xoksa` dependency); the desktop launches the `xoksa` engine binary as a child process (`xoksa serve`), and the settings app drives the engine's config subcommands (`xoksa apply-config` / `config-json` / …). No analysis logic is duplicated, and the CLI binary itself gains no Tauri/WebView dependency. Both artifacts yield identical results (SOT). See [design-philosophy.md](./design-philosophy.md) ("One Engine, Multiple Front-Ends") and [security-design.md §6](./security-design.md).
- **Signing.** The macOS artifacts are **Developer ID–signed, notarized and stapled** (no Gatekeeper prompt on first run). **Windows currently ships unsigned** — see [security-design.md §6](./security-design.md).
- **Integrity.** Each artifact ships with its own SHA-256 checksum, and a CycloneDX SBOM is published **per target** — an SBOM lists what is inside *that* binary, and the platform crates differ (`schannel` / `windows-native-keyring-store` vs `security-framework` / `apple-native-keyring-store`), so `xoksa-native-windows-x86_64.cdx.json` and `xoksa-native-macos-arm64.cdx.json` are separate files, alongside the host-independent WASM UI SBOM (`xoksa-webui-leptos.cdx.json`).

## Module responsibilities

Full per-file detail lives in [source-map.md](./source-map.md); this is the responsibility map.

| Module | Responsibility | Central items |
|---|---|---|
| `bootstrap` | env/CLI loading, ticker sanitization | `load_env_map`, `sanitize_ticker` |
| `config` | one config vocabulary for CLI + Web | `Config`, `Args`, `AnalysisMode` |
| `market` | provider price fetch (Yahoo primary, alt-host + Stooq daily fallback) → OHLCV | `fetch_market_data_snapshot_for_mode`, `MarketDataSnapshot`, `StooqPriceFetcher`, `PriceFetcherKind::Failover` |
| `technical` | indicators + composite score (SOT) | `build_basic_technical_entry`, `TechnicalDataGuard`, `calculate_final_score_snapshot` |
| `app` | the shared market→analysis pipeline | `build_analyzed_guard` |
| `fundamental` / `news` | fundamentals (J-Quants/EDGAR) — confirmed behind a validated write boundary (SOT) / Brave titles+URLs | `FundamentalData`, `FundamentalData::build`, `render_fundamental_display`, `BraveArticleFetcher` |
| `prompt` / `chat` | prompt assembly; unified chat dispatch | `ChatSession::execute`, `/forum`, `/depth, /scope` |
| `context` | multi-timeframe context pack for the LLM | `ContextSpec`, `build_context_pack`, `ContextPack` |
| `backtest` | rule engine over provider history | `run_backtest`, `StrategyRules`, `eval_rules` |
| `llm` | provider dispatch + SOT reinforcement + retry | `LlmDispatchSender`, `send_chat_turn`, `sot_system_prompt` |
| `integrity` | the confirmed-data reference the output guard verifies against — built from `TechnicalDataGuard` / `FundamentalData`, never from text | `SymbolFacts::from_sources`, `ConfirmedFactSet::verify`, `Scan` |
| `strategies` | file-backed saved backtest strategies | `~/.xoksa.strategies.json` load/save |
| `private` | private-mode flag + home-directory helper | `is_private`, `set_private`, `home_dir` |
| `server::{mod,api}` | HTTP transport + JSON handlers | `ServeArgs`, `run_server`, `serve_static`, `symbol_summary`, `chat_stream` |
| `traits` | DI seams for external deps | `PriceFetcher`, `ArticleFetcher`, `PromptSender` |
| `logging` / `render` / `output` | coded logger; terminal formatting | `logging::{warn,error,info}`, `set_format` |
| `setup` | interactive `--init` wizard; **headless apply-setup** reused by the `apply-config` subcommand; `ollama-models` | `run_init`, `apply_setup`, `run_apply_config_cli`, `run_ollama_models_cli` |
| desktop + settings apps (Tauri) | connection shell + config app: launch `xoksa` as a child process | separate workspaces; **no `xoksa` dependency**; the desktop drives `serve`, the settings app (`xoksa-setup`) drives the config subcommands |

## Request flow

1. **Input** — CLI `Args`/env, or an HTTP request to `/api/*`.
2. **Config** — `bootstrap::load_env_map` → `Config` (single config shared by CLI and Web); the ticker is sanitized.
3. **Market data** — `market::fetch_market_data_snapshot_for_mode` (via the `PriceFetcher` trait: Yahoo primary, with an alt-host retry and a labelled daily-only Stooq fallback) → `MarketDataSnapshot` (OHLCV for the timeframe, whole from one source).
4. **Analysis** — `app::build_analyzed_guard` computes indicators into a `TechnicalDataGuard` (SOT) and the composite score; optional fundamentals are built into a `FundamentalData` (the second SOT boundary — validated on every write) and, with news, folded into the same context.
5. **LLM context** — `prompt`/`chat` build the SOT-grounded prompt; `LlmDispatchSender` prepends the identical SOT reinforcement and dispatches to the selected provider.
6. **Output** — CLI → `render`/`output` (terminal); Web → JSON (`server::api`) or an SSE stream (chat).

Branches off the same core: **chat** (`ChatSession::execute`, identical for CLI/Web; `/forum` runs several LLMs in parallel), **multi-timeframe** (`context::build_context_pack` → Rust summary → LLM), **backtest** (`backtest::run_backtest` over a contiguous provider history).

## Boundaries & abstractions

- **Trait seams** (`traits.rs`): `PriceFetcher` (Yahoo primary + Stooq fallback via `PriceFetcherKind::Failover`), `ArticleFetcher` (Brave), `PromptSender` (`LlmDispatchSender`) — allow mock injection in tests and new providers without touching call sites.
- **LLM provider abstraction**: one dispatch (`LlmDispatchSender` / `send_chat_turn`) → per-provider request builders (OpenAI/Gemini/Claude/Ollama). The prompt and the SOT reinforcement are **identical for every provider**.
- **HTTP boundary**: `server::api` is the *only* path from the browser to the engine. Analysis, indicators, the LLM call, and backtests stay native; the WASM UI is presentation only. The **desktop app** drives the engine as a child process: the dashboard is the engine's own HTTP server, and configuration — including key entry — goes through the **settings app** to `xoksa apply-config` over the child's stdin (→ keyring, never over HTTP). Neither the desktop nor the settings app carries any analysis. See [security-design.md §6](./security-design.md).
- **Config boundary**: a single `xoksa.env` / `Config` for the CLI, `serve`, the desktop, and the settings app — no GUI-specific settings file (the settings app writes the same `xoksa.env` via the shared apply-setup logic). `xoksa.env` resolves to **one canonical, user-scoped path** (`<config_dir>/xoksa/xoksa.env`, via the shared `xoksa-paths` crate); `--env-file` overrides it and there is **no** implicit `./xoksa.env` fallback, so the config cannot split by launch directory (see [security-design.md §6](./security-design.md)).

## Failure handling & operations

- **Retry** — LLM: up to `MAX_RETRIES` (3) on HTTP 429/503, honoring `Retry-After`, otherwise `FALLBACK_WAIT_SECS` (60s). Market data: retried on 429 / 5xx / timeout / connect errors.
- **Timeout** — LLM: `llm_timeout_secs` (config). Market fetch: 10s per request.
- **Fallback** — Market data: Yahoo's alt host (`query1` after `query2`, identical numbers), then a labelled daily-only Stooq fallback (intraday/weekly/monthly get no data rather than substituted daily bars); a successful Yahoo fetch always wins and a snapshot is never spliced across providers. OpenAI request-shape fallback (`max_completion_tokens` ↔ `max_tokens`, sampling params on/off); truncated Ollama responses are suppressed with guidance; a missing API key / news result is a *note*, never fabricated data.
- **Resource caps (Web)** — per-message length cap (`MAX_CHAT_MSG_CHARS`), same-origin (CSRF) check on chat, and an LRU-bounded `WEB_SESSIONS`. Security headers on every response; `index.html` is served `no-store` so a rebuilt UI is never stale. **Not yet enabled:** app-level rate limiting and authentication — deliberate ship-gates for any *exposed* deployment (the default is loopback).
- **Stale data** — no look-ahead: nothing beyond the latest bar is ever read. On Japanese intraday timeframes that latest bar is the still-forming one, carrying the real-time quote; the live chart always refetches from the provider.
- **No-trace mode** — `--private` disables all disk writes: the log file, the saved-strategies file, the alert rules the dashboard would append to `xoksa.env`, the chat history file, and the language setting all honor it (the language endpoint answers `409` rather than pretending to save).
- **Logging & secrets** — the coded logger dedups console warnings and appends `logs/xoksa-error.log` under the working directory (`--log-format json` for NDJSON). API keys live in the OS keychain, are zeroized after use, and are never placed in prompts or logs.

---

---

<a id="ja"></a>

# xoksa アーキテクチャ概要


```
┌─────────────────┐                                          ┌─────────────────┐
│  DATA SOURCES   │                                          │  LLM PROVIDERS  │
│                 │                                          │                 │
│ ┌─────────────┐ │         ┌───────────────────────┐        │ ┌─────────────┐ │
│ │Yahoo Finance│ │──────▶  │                       │  ◀──── │ │   OpenAI    │ │
│ │     API     │ │         │     xoksa engine      │        │ │gpt-5.6-terra│ │
│ │Price OHLCV  │ │         │                       │        │ └─────────────┘ │
│ │Market Data  │ │         │ ┌───────────────────┐ │        │                 │
│ └─────────────┘ │         │ │ TechnicalDataGuard│ │        │ ┌─────────────┐ │
│                 │         │ │   SOT / Integrity │ │  ◀──── │ │   Gemini    │ │
│ ┌─────────────┐ │         │ └───────────────────┘ │        │ │3.5-flash    │ │
│ │Brave Search │ │──────▶  │ ┌───────────────────┐ │        │ └─────────────┘ │
│ │     API     │ │         │ │    LLM Engine     │ │        │                 │
│ │Latest News  │ │         │ │OpenAI/Gemini/     │ │  ◀──── │ ┌─────────────┐ │
│ │Title + URL  │ │         │ │Claude/Ollama      │ │        │ │   Claude    │ │
│ └─────────────┘ │         │ └───────────────────┘ │        │ │sonnet-5     │ │
│                 │         │ ┌───────────────────┐ │        │ └─────────────┘ │
│ ┌─────────────┐ │         │ │   Chat & Forum    │ │        │                 │
│ │  J-Quants   │ │──────▶  │ │Chat / Parallel    │ │  ◀──── │ ┌─────────────┐ │
│ │  SEC EDGAR  │ │         │ │Forum/Facilitator  │ │        │ │   Ollama    │ │
│ │  JP/US      │ │         │ └───────────────────┘ │        │ │Up to 16     │ │
│ │  Fundamental│ │         │ ┌───────────────────┐ │        │ │instances    │ │
│ └─────────────┘ │         │ │   News Triage     │ │        │ │Local LLM    │ │
└─────────────────┘         │ │Title + URL only   │ │        │ └─────────────┘ │
                            │ └───────────────────┘ │        └─────────────────┘
                            │ ┌───────────────────┐ │
                            │ │ Config & Security │ │
                            │ │OS Keychain/zeroize│ │
                            │ └───────────────────┘ │
                            │ ┌───────────────────┐ │
                            │ │  Backtest / MTF   │ │
                            │ └───────────────────┘ │
                            │ ┌───────────────────┐ │        ┌─────────────────┐
                            │ │  Alert & Notify   │ │──────▶ │ Slack / Discord │
                            │ │ SOT threshold →   │ │        │ Google Chat /   │
                            │ │ https POST only   │ │        │ LINE (fixed     │
                            │ └───────────────────┘ │        │ host allowlist) │
                            │                       │        └─────────────────┘
                            │ Rust 100% · native engine + Rust/WASM web UI
                            └───────────────────────┘
                                       │
              ┌────────────────────────┼──────────────────────────┐
              ▼                        ▼                           ▼
        ┌───────────┐         ┌──────────────┐          ┌────────────────────────┐
        │ Terminal  │         │  CSV / JSON  │          │   Browser dashboard     │
        │  Display  │         │     Log      │          │   Rust/WASM (Leptos)    │
        │   (CLI)   │         │              │          │   via built-in HTTP     │
        └───────────┘         └──────────────┘          │   server + JSON API     │
                                                         │   (no Apache/nginx)     │
                                                         └────────────────────────┘
```

## コンポーネント

上図は概略です。エンジンの主要サブシステム（すべて Rust・ネイティブ側）：

- **SOT — `TechnicalDataGuard` / `FundamentalData`** — 指標・スコア・ファンダは Rust で算出し、検証を経た境界からしか到達できない。全表示面が同一値を読む。2 つの境界は形が違うが理由は同じ。テクニカル側は**どの指標が値を生成したか**を記録するので未計算の指標はゼロではなく欠損になり、ファンダ側は全フィールドを非公開にして**書き込む経路がいずれも検証する**（PER・PBR・ROE は確定データの内側で一度だけ導出）。
- **LLM エンジン** — OpenAI / Gemini / Claude / Ollama（ローカル・複数インスタンス）をセッション単位で切替。確定データ順守の念押し（創作禁止・数値は入力どおり・ニュースはタイトル＋URLのみ）は**全プロバイダに同一前置**（プロバイダ別プロンプトは持たない）。
- **Chat & Forum** — CLI と Web UI で共有する単一ディスパッチ（`ChatSession::execute`）。`/forum` は複数 LLM を並列実行し Chair が整理。
- **News triage** — Brave Search のタイトル＋URLのみ（本文は取得しない）を関連度でフィルタし新しさ順に並べる。
- **マルチタイムフレーム文脈** — `build_context_pack` が複数足を共有エンジンで一度分析し、Rust 側で要約（`ContextPack`）して LLM に渡す（生データは渡さない）。
- **バックテスト** — プロバイダの連続履歴に対するルールエンジン。現実的なサイジング（開始割合＋シグナルごと・最小単元・銘柄通貨）と Buy&Hold 併記。保存したバックテストのルール（ルールエディタの保存/呼び出し）は小さなローカルJSONファイル（`~/.xoksa.strategies.json`）に永続化する — ログを除けばディスクに書く唯一のもの。
- **アラートと通知** — `serve` 内の起動中のみ動く監視が足をその周期で見て、利用者が定めた**計算済み値**の条件が成立したらチャット基盤へ1行を https POST する（Slack / Discord / Google Chat は Incoming Webhook、LINE は Messaging API）。送信先ホストはプラットフォーム別の固定許可リスト、メッセージが運ぶのは確定値（SOT）の事実で LLM が書く売買文言は載せない。同一（銘柄・条件・指標足）につき、条件が偽→真に変わった瞬間だけ1回発火。[security-design.md §4](./security-design.md) 参照。
- **Config & Security** — CLI と Web で単一の `xoksa.env`。APIキーは OS キーチェーンに保管し使用後ゼロ化。
- **構造化ログ** — 外部依存なしのコード付きロガー（作業ディレクトリ直下の `logs/xoksa-error.log`・絶対パスは起動時に表示）。`xoksa serve --log-format json` で NDJSON 出力。

表示面（プレゼンのみ）：**CLI** 端末表示・**ローカルログ**・**ブラウザ・ダッシュボード**（Rust/WASM Leptos を内蔵 HTTP サーバが JSON API で配信）、そして **デスクトップアプリ**（エンジンを子プロセスとして起動し、そのダッシュボードを OS の WebView でホストする Tauri 接続シェル。設定は付属の設定アプリ `xoksa-setup`。[security-design.md §6](./security-design.md) 参照）。分析・指標・LLM 呼び出し・バックテストはブラウザ／WASM 層へ移さない。

- **自己完結型の CLI バイナリ** — `xoksa` CLI / `serve` のリリースバイナリは `embedded-ui` cargo フィーチャでビルドされ、ビルド済み Web UI（`webui-leptos/dist` を `include_dir!` で）を実行ファイルに埋め込む。よってダウンロードしたリリースは**外部の `dist/` ファイルなし**でダッシュボードを配信する。`server::mod` のアセット配信はディスク上の `--web-dir`（開発ワークフロー）を優先し、無ければ埋め込みアセットにフォールバックする。ソースからビルドする開発者は従来どおり `trunk build` ＋ `dist/` を使う。

## 配布 — 出荷成果物

1リリースで**一つのエンジン上に二つの成果物**を生成し、利用者は使い方で選ぶ。

| 成果物 | 中身 | パッケージ（OS別） | 対象 |
| :--- | :--- | :--- | :--- |
| **`xoksa`（CLI）** | 単一自己完結バイナリ＝エンジン＋埋め込み Web UI（`embedded-ui`）。`serve --ui` でダッシュボードも配信。 | 実行ファイルの zip（Windows / macOS）／ソースビルド（Linux） | 自動化・バッチ/パイプライン・スクリプト・他ツール連携・サーバ |
| **XOKSA（デスクトップ）** | *同一*エンジンを**子プロセス**（`xoksa serve`）として起動し、そのダッシュボードを表示する **Tauri** 接続シェル。設定用に独立した設定アプリ（`xoksa-setup`）も同梱。 | macOS `.app`／`.dmg`、Windows `.msi`（NSIS は実測のうえ不採用＝コンテナ構造が AV ヒューリスティックに引っかかる）。Linux はソースからビルド可能だが**配布していない**。 | 初心者・非ターミナル |

- **一つのエンジン・フォークなし。** デスクトップと設定アプリは**別 workspace** で、**エンジンのコードを一切リンクしない**（`xoksa` 依存なし）。デスクトップは `xoksa` エンジンバイナリを子プロセス（`xoksa serve`）として起動し、設定アプリがエンジンの設定サブコマンド（`xoksa apply-config`／`config-json`／…）を駆動する。分析ロジックの重複はなく、CLI 本体は Tauri/WebView 依存を持たない。両成果物は同一結果（SOT）。[design-philosophy.md](./design-philosophy.md)（単一エンジン・複数フロントエンド）・[security-design.md §6](./security-design.md) 参照。
- **署名。** macOS の成果物は **Developer ID 署名＋公証＋staple 済み**（初回起動で Gatekeeper の警告が出ない）。**Windows は現在未署名で出荷**（[security-design.md §6](./security-design.md) 参照）。
- **完全性。** 各成果物に SHA-256 を添え、CycloneDX SBOM は**ターゲットごとに**公開する — SBOM は*そのバイナリ*に入っているものを列挙し、プラットフォーム crate は入れ替わるため（`schannel`・`windows-native-keyring-store` ↔ `security-framework`・`apple-native-keyring-store`）、`xoksa-native-windows-x86_64.cdx.json` と `xoksa-native-macos-arm64.cdx.json` は別ファイルとする。加えてホスト非依存の WASM UI SBOM（`xoksa-webui-leptos.cdx.json`）。

---

## モジュール責務

ファイル単位の詳細は [source-map.md](./source-map.md)。ここは責務の地図。

| モジュール | 責務 | 中心要素 |
|---|---|---|
| `bootstrap` | env/CLI 読込・ティッカー sanitize | `load_env_map`・`sanitize_ticker` |
| `config` | CLI/Web 共通の設定語彙 | `Config`・`Args`・`AnalysisMode` |
| `market` | プロバイダ価格取得（Yahoo 主・代替ホスト＋Stooq 日足フォールバック）→OHLCV | `fetch_market_data_snapshot_for_mode`・`MarketDataSnapshot`・`StooqPriceFetcher`・`PriceFetcherKind::Failover` |
| `technical` | 指標＋総合スコア（SOT） | `build_basic_technical_entry`・`TechnicalDataGuard`・`calculate_final_score_snapshot` |
| `app` | 市場→分析の共有パイプライン | `build_analyzed_guard` |
| `fundamental` / `news` | ファンダ（J-Quants/EDGAR）。検証付きの書き込み境界の内側に確定値を持つ（SOT）/ Brave タイトル+URL | `FundamentalData`・`FundamentalData::build`・`render_fundamental_display`・`BraveArticleFetcher` |
| `prompt` / `chat` | プロンプト構築・統一チャットディスパッチ | `ChatSession::execute`・`/forum`・`/depth, /scope` |
| `context` | LLM 向けマルチタイムフレーム文脈 | `ContextSpec`・`build_context_pack`・`ContextPack` |
| `backtest` | プロバイダ履歴に対するルールエンジン | `run_backtest`・`StrategyRules`・`eval_rules` |
| `llm` | プロバイダ振り分け＋SOT念押し＋リトライ | `LlmDispatchSender`・`send_chat_turn`・`sot_system_prompt` |
| `integrity` | 出力ガードが照合する確定データの基準。`TechnicalDataGuard`／`FundamentalData` から構築し、テキストからは作らない | `SymbolFacts::from_sources`・`ConfirmedFactSet::verify`・`Scan` |
| `strategies` | ファイルベースの保存済みバックテスト戦略 | `~/.xoksa.strategies.json` の読み書き |
| `private` | プライベートモードフラグ＋ホームディレクトリヘルパー | `is_private`・`set_private`・`home_dir` |
| `server::{mod,api}` | HTTP トランスポート＋JSON ハンドラ | `ServeArgs`・`run_server`・`serve_static`・`symbol_summary`・`chat_stream` |
| `traits` | 外部依存のDI継ぎ目 | `PriceFetcher`・`ArticleFetcher`・`PromptSender` |
| `logging` / `render` / `output` | コード付きロガー・端末整形 | `logging::{warn,error,info}`・`set_format` |
| `setup` | 対話的 `--init` ウィザード・**headless apply-setup**（`apply-config` サブコマンドが再利用）・`ollama-models` | `run_init`・`apply_setup`・`run_apply_config_cli`・`run_ollama_models_cli` |
| デスクトップ＋設定アプリ（Tauri） | 接続シェル＋設定アプリ：`xoksa` を子プロセス起動 | 別 workspace。**`xoksa` 依存なし**。デスクトップは `serve`、設定アプリ（`xoksa-setup`）は設定サブコマンドを駆動 |

## リクエスト実行フロー

1. **入力** — CLI の `Args`/env、または `/api/*` への HTTP リクエスト。
2. **設定** — `bootstrap::load_env_map` → `Config`（CLI/Web 共有の単一設定）。ティッカーは sanitize。
3. **市場データ** — `market::fetch_market_data_snapshot_for_mode`（`PriceFetcher` 経由：Yahoo 主、代替ホスト再試行＋ラベル付き日足のみ Stooq フォールバック）→ `MarketDataSnapshot`（足の OHLCV、単一ソースから丸ごと）。
4. **分析** — `app::build_analyzed_guard` が指標を `TechnicalDataGuard`（SOT）と総合スコアに算出。任意のファンダは `FundamentalData`（もう一方の SOT 境界。書き込む経路がいずれも検証する）に構築し、ニュースとともに同一文脈へ統合。
5. **LLM 文脈** — `prompt`/`chat` が SOT 準拠プロンプトを構築、`LlmDispatchSender` が**全プロバイダ同一の念押し**を前置して振り分け。
6. **出力** — CLI → `render`/`output`（端末）／Web → JSON（`server::api`）または SSE ストリーム（チャット）。

同一コアからの分岐：**チャット**（`ChatSession::execute`＝CLI/Web 同一、`/forum` は複数 LLM 並列）、**マルチタイムフレーム**（`context::build_context_pack`→Rust要約→LLM）、**バックテスト**（`backtest::run_backtest`＝連続プロバイダ履歴）。

## 境界と抽象化

- **トレイト継ぎ目**（`traits.rs`）：`PriceFetcher`（Yahoo 主＋`PriceFetcherKind::Failover` 経由の Stooq フォールバック）・`ArticleFetcher`（Brave）・`PromptSender`（`LlmDispatchSender`）。呼び出し側を変えずにモック注入・新規プロバイダ追加が可能。
- **LLM プロバイダ抽象化**：単一ディスパッチ（`LlmDispatchSender`/`send_chat_turn`）→ プロバイダ別リクエストビルダ。プロンプトと SOT 念押しは**全プロバイダ同一**。
- **HTTP 境界**：`server::api` がブラウザ→エンジンの唯一の経路。分析・指標・LLM・バックテストはネイティブに残す。WASM UI は表示専用。**デスクトップアプリ**はエンジンを子プロセスとして駆動する：ダッシュボードはエンジン自身の HTTP サーバで、設定（キー入力含む）は**設定アプリ**を通じて子プロセスの stdin 経由で `xoksa apply-config` へ渡す（→ keyring・HTTP を通さない）。デスクトップも設定アプリも分析を運ばない。[security-design.md §6](./security-design.md) 参照。
- **設定境界**：CLI・`serve`・デスクトップ・設定アプリで単一の `xoksa.env`/`Config`。GUI 専用設定ファイルは持たない（設定アプリも共有の apply-setup ロジックで同じ `xoksa.env` を書く）。`xoksa.env` は**唯一の正準・ユーザースコープのパス**（`<config_dir>/xoksa/xoksa.env`・共有 `xoksa-paths` クレート経由）に解決し、`--env-file` で上書き、**暗黙の `./xoksa.env` フォールバックは無い**ため、起動ディレクトリで設定が分裂しない（[security-design.md §6](./security-design.md) 参照）。

## 失敗時の設計・運用

- **リトライ** — LLM：HTTP 429/503 で最大 `MAX_RETRIES`（3）、`Retry-After` を尊重、無ければ `FALLBACK_WAIT_SECS`（60秒）。市場データ：429 / 5xx / タイムアウト / 接続失敗でリトライ。
- **タイムアウト** — LLM：`llm_timeout_secs`（設定）。市場取得：1リクエスト 10 秒。
- **フォールバック** — 市場データ：Yahoo の代替ホスト（`query2` の次に `query1`、数値は同一）、次にラベル付き日足のみの Stooq フォールバック（分足/週足/月足は日足で代用せず「データなし」）。Yahoo 取得が成功すれば常にそちらが勝ち、スナップショットをプロバイダ間で継ぎ接ぎしない。OpenAI のリクエスト形フォールバック（`max_completion_tokens` ↔ `max_tokens`、サンプリング有無）。Ollama の途中切れ応答はガイド付きで抑制。APIキー欠如やニュース失敗は**注記**であり、偽データは作らない。
- **リソース上限（Web）** — メッセージ長上限（`MAX_CHAT_MSG_CHARS`）、チャットの同一オリジン（CSRF）チェック、LRU 上限の `WEB_SESSIONS`。全レスポンスにセキュリティヘッダ、`index.html` は `no-store`（再ビルドUIが古いまま出ない）。**未実装：** アプリ層のレート制限と認証＝**公開運用時の出荷ゲート**（既定はループバック）。
- **ステールデータ** — 先読みなし：最新足より先は一切読まない。日本株の分足ではその最新足が形成中の足であり、リアルタイム気配を含む。ライブチャートは常にプロバイダから再取得。
- **痕跡なしモード** — `--private` で全ディスク書込を無効化：ログファイル・保存戦略ファイル・ダッシュボードが `xoksa.env` に追記するアラートルール・チャット履歴ファイル・言語設定が従う（言語のエンドポイントは保存したふりをせず `409` を返す）。
- **ログと機密** — コード付きロガーはコンソール警告をデデュープし作業ディレクトリ直下の `logs/xoksa-error.log` に追記（`--log-format json` で NDJSON）。APIキーは OS キーチェーン保管・使用後ゼロ化・プロンプト/ログに載せない。
