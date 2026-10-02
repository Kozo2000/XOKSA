# xoksa Source Code Structure Map

[日本語ドキュメントはこちら。](#ja)

> **Language sync:** English and Japanese descriptions are both authoritative. If a requirement, restriction, exception, command behavior, or operational rule appears in only one language, treat it as applying to both; the missing side is a documentation defect to be updated.

Target: All `.rs` files under `src/` (the `xoksa` engine / CLI crate). The desktop crate `xoksa-desktop/` and the settings-app crate `xoksa-setup/` are mapped in their own sections below.

**What this document is, and what it is not.** Every file has a section — that part is complete, and a file missing from here is a defect. The per-file tables are **not** a complete index of public items: they list what a reader needs in order to see the boundary a file owns and how it connects to the rest. Many public functions are deliberately absent. **Do not read "not listed here" as "does not exist."** For the complete, always-current item list, read the code or `cargo doc`; this document exists for the part the code cannot state in one place — what each file is responsible for, and the flow that crosses module lines.

**Which side is authoritative.** The **doc comment in the code** is. It sits next to what it describes, so it cannot silently drift from it, and `cargo doc` renders it for every item rather than the selection made here. Doc comments are written **bilingually** — English, a blank line, then Japanese — so that choosing the code as the source does not cost the Japanese reader anything. **Where a table row and a doc comment disagree, the doc comment is right and the row is a defect.** Update the row, or delete it and let `cargo doc` carry the detail. An entry here is a summary and a pointer, not a second specification.

---

## Entry Point

### `src/main.rs`

The entry point for the executable binary. Performs only orchestration of processing; contains no logic.

| Function | Summary |
|------|------|
| `main()` | Async entry point. Two code paths: (1) `--chat` with no ticker → skip market data pipeline entirely and enter chat loop directly; (2) normal analysis flow → initialization → market data → technical analysis → output → news → optional LLM → optional chat loop |

**Processing Flow (call order)**

```
bootstrap::initialize_environment_and_config()     → (Config, Option<String>, ticker_name_map, chat_default_extra_tickers)

[Branch A: --chat with no ticker]
chat::run_chat_loop(&config, "", ...)              ← enters chat loop without market data
return

[Branch B: normal analysis]
YahooFinancePriceFetcher::fetch_snapshot()         ← via PriceFetcher trait
technical::build_basic_technical_entry()
technical::evaluate_all_selected_extensions()
fundamental::fetch_fundamental_data()              ← only when fundamental=true
app::run_output_pipeline(&fundamental_data)        ← passes fundamental_data
news::news_flow_controller(&fetcher)               ← injects BraveArticleFetcher; skipped when no_news=true
prompt::build_analysis_prompt()                    ← the analysis prompt (also the chat base context)
llm::save_prompt_to_file()                          ← only when debug_prompt
llm::LlmDispatchSender.send_prompt()                ← only when !no_llm && !silent && !chat
chat::run_chat_loop(...)                           ← only when config.chat (ticker present)
```

---

## Library Root

### `src/lib.rs`

Public API definition for the crate. Aggregates `pub mod` declarations and re-exports for all modules. `AnalysisEngine` has been removed (dead code).

---

## Module List

### `src/bootstrap.rs`

Responsible for initialization processing at startup. Reads `xoksa.env` as `env_map` (HashMap), parses CLI arguments, and reads the alias CSV, all in one place.

| Function | Public | Summary |
|------|------|------|
| `initialize_environment_and_config()` | pub | Reads `xoksa.env` as `env_map`, parses CLI arguments, and builds `Config`. API keys are not included in env_map; each send function retrieves them on demand. Also reads the alias CSV. Return value is `(Config, Option<String>, ticker_name_map, chat_default_extra_tickers)` — a 4-element tuple; `chat_default_extra_tickers` holds extra ticker symbols passed via `--chat` with multiple tickers. Ticker is `None` when `--chat` is used without a ticker argument |
| `load_env_map()` | pub | Reads `xoksa.env` into a `HashMap<String, String>` (the `env_map`); used by the server (`build_server_config`) to build a dashboard `Config` without the full CLI init |
| `config_debug_string()` | private | Returns `Config` as a formatted string when `--debug-args` is active. Masks `sec_user_agent` and `https_proxy` (which may contain credentials) with `***` |
| `normalize_ticker_input()` | pub | Expands the one nickname that names the instrument it resolves to (`全米` / `トータルマーケット` → `VTI`); anything else is returned exactly as typed (the trim / upper-case of the value finally used is `sanitize_ticker`'s). **An index name is never answered with a fund**: the `S&P500` → `SPY`, `NASDAQ100` → `QQQ`, `DOW` → `DIA`, `日経平均` → `1321.T`, `TOPIX` → `1306.T` and `FANG+` → `FNGU` (a 3× leveraged ETN) substitutions were removed in 2.9.9 |
| `sanitize_ticker()` | pub | Validates ticker string (permitted character type and length check). Returns `Err` if invalid |
| `sanitize_news_query()` | pub | Validates news query string (length and control character check) |
| `sanitize_llm_note()` | pub | Validates LLM additional note (length and control character check) |
| `normalize_ticker()` | pub | Normalizes ticker notation, including `.T` suffix normalization |
| `jp_code_from_ticker()` | pub | Returns the securities code string if the ticker is a Japanese stock (`XXXX.T` or 4-digit number); otherwise returns `None` |
| `load_alias_csv()` | pub | Reads the CSV at the specified path and returns a `HashMap<String, String>` of `code -> stock name` |

**Note**: `initialize_environment_and_config()` holds `xoksa.env` as `env_map` (HashMap) and uses it to resolve general settings. API keys (`OPENAI_API_KEY` / `BRAVE_API_KEY` / `GEMINI_API_KEY` / `CLAUDE_API_KEY` / `JQUANTS_API_KEY`) are not placed in env_map; each retrieval function obtains them on demand via `resolve_api_key()` (checks `xoksa.env` first, then falls back to the OS Keychain).

---

### `src/config.rs`

Contains CLI argument definitions, the `Config` struct definition, and the merge logic for CLI/environment variables/defaults via `build_config()` / `build_config_from_arg_matches()`.

| Type / Function | Public | Summary |
|----------|------|------|
| `AnalysisMode` enum | pub | Analysis bar mode. `Daily` is `1d/3mo`, `Intraday5m` is `5m/5d`, `Intraday15m` is `15m/1mo`, `Intraday30m` is `30m/1mo`, `Weekly` is `1wk/2y`, `Monthly` is `1mo/10y` |
| `ExtensionIndicator` enum | pub | Enum of extended technical indicators (`Ema`, `Sma`, `Bollinger`, `Roc`, `Adx`, `Stochastics`, `Fibonacci`, `Vwap`, `Ichimoku`) |
| `Stance` enum | pub | Investment stance (`Buyer`, `Seller`, `Holder`) |
| `Args` struct | pub | CLI argument definition using clap. Defines approximately 50 options |
| `Config` struct | pub | The single source of truth for runtime settings. All fields are finalized in `build_config()` |
| `Config::default()` | pub | Default values for testing. Not used in production (`build_config()` is used instead) |
| `CliValueSources` struct | pub | Auxiliary struct holding flags explicitly specified via CLI. Has 9 fields: `analysis_mode`, `log_format`, `llm_provider`, `ollama_bench_format`, `chat_memory`, `chat_guard`, `debate`, `chat_mode`, `lang`. Constructed from clap `ValueSource::CommandLine` via `from_arg_matches()` |
| `build_config()` | pub | Compatible API that receives `Args` and returns `Config`. Use `build_config_from_arg_matches()` when CLI explicit specification detection is needed |
| `build_config_from_arg_matches()` | pub | Constructs `CliValueSources` based on clap `ArgMatches` `ValueSource` and delegates to `build_config_with_value_sources()` |
| `build_config_with_value_sources()` | pub | The central implementation that references `CliValueSources` to accurately merge CLI explicit values, env, and defaults to build `Config` |
| `resolve_threshold_f64()` | private | Helper that generalizes "CLI > env > default" resolution for `buy_rsi` / `sell_rsi` / `macd_diff_*` |
| `get_bool_env()` | private | Reads the specified key's environment variable as `"true"` / `"false"` |
| `get_f64_from_args_or_env()` | private | Reads from environment variable if CLI argument equals the default value. Includes range validation of 0.5–3.0 (dedicated to weight settings) |
| `get_usize_from_args_or_env()` | private | Reads from environment variable if CLI argument equals the default value (for period settings) |
| `get_usize_env()` | private | Gets `usize` from environment variable. Returns default value on failure |
| `sanitize_percent()` | private | Range clamp of 0.0–100.0 (dedicated to `bb_bandwidth_squeeze_pct`) |
| `parse_stance()` | private | Converts `"buyer"` / `"seller"` / other to `Stance` enum |
| `OllamaInstance` / `NotifyChannel` / `AlertRule` structs | pub | Config-side models for multiple Ollama endpoints, chat-notification channels (`NOTIFY_<n>_*`), and saved alert rules (`ALERT_<n>_*`) |
| `scan_ollama_instances()` / `scan_notify_channels()` / `scan_alert_rules()` | pub | Parse the `OLLAMA_<n>_*` / `NOTIFY_<n>_*` / `ALERT_<n>_*` groups out of `env_map` into the structs above |

**General settings priority**: CLI explicit value > xoksa.env (env_map) > system environment variable > default value. API keys are not included in env_map; each send function retrieves them on demand via `resolve_api_key()` (xoksa.env first, then OS Keychain).

---

### `src/market.rs`

Retrieves stock price data from market data APIs.

| Type / Function | Public | Summary |
|----------|------|------|
| `MarketData` struct | pub | OHLCV data for one bar. Has `date`, `datetime`, `timestamp`, `timezone`, `high`, `low`, `close`, `volume`, `name`. Confirmed bars are never rewritten; on Japanese **intraday** timeframes the bar the live observation falls in has its close (and high/low bounds) updated by `apply_realtime_intraday_observation()` |
| `MarketLatestObservation` struct | pub | Holds only the latest retrieved price from the data source (`price: f64`); time and timezone fields live in `MarketDataSnapshot` |
| `MarketDataSnapshot` struct | pub | Combined retrieval result of `bars`, `latest_observation` (price only), `timezone`, `market_data_latest_time`, `market_data_latest_timestamp`, `analyzed_at`. Time and timezone are always held regardless of whether price exists |
| `YahooFinancePriceFetcher` struct | pub | Production implementation of `PriceFetcher` trait (primary provider). Fetches from `query2.finance.yahoo.com`; if that host's retries are exhausted it retries the alternate host `query1.finance.yahoo.com` (same data — pure resilience). Delegates to `fetch_market_data_snapshot_for_mode()` |
| `StooqPriceFetcher` struct | pub | Fallback provider (genuinely different vendor). Daily-only free feed: serves the daily timeframe; returns an error for intraday/weekly/monthly rather than substituting daily bars. Numbers may differ slightly from Yahoo, so an adopted snapshot is labelled |
| `PriceFetcherKind::Failover` | pub | `PriceFetcherKind` variant composing Yahoo (primary) with Stooq (fallback): a successful Yahoo fetch always wins; Stooq is used only on genuine Yahoo failure. `build_price_fetcher()` constructs it. A snapshot is never spliced across providers (SOT). When **both** fail the primary's reason leads the error and the fallback's outcome follows as context — the primary is the one that knows an unknown ticker, and discarding it reported the fallback's CSV complaint for a mistyped symbol |
| `stooq_symbol()` | private | Maps a ticker to Stooq's symbol convention (e.g. JP/US suffix handling) |
| `parse_stooq_csv()` | private | Parses Stooq's daily CSV response into bars, filtering non-finite values at the boundary. An HTML response (Stooq's anti-bot challenge on some networks) is reported as a blocked fallback rather than as a malformed CSV, and the page is never quoted into the message |
| `bounded_provider_text()` | private | Caps untrusted provider text and turns control characters into spaces before it reaches a message or a log, so a response cannot rewrite the line it is printed on or run away with the message |
| `yahoo_chart_error_reason()` | private | The reason Yahoo gives in the JSON body it sends **with** a non-2xx status (`chart.error.code` / `.description`), so an unknown symbol is reported as such instead of a bare "request failed". `None` when the body is not that shape |
| `fetch_stooq_snapshot()` | private | Async. Fetches and builds a `MarketDataSnapshot` from Stooq (daily only) |
| `fetch_market_data_for_mode()` | pub | Async. Retrieves daily, intraday short-term, weekly, or monthly data according to `AnalysisMode` |
| `fetch_market_data_snapshot_for_mode()` | pub | Async. Separately retrieves analysis bar data and latest retrieved price metadata. For Japanese tickers it then overlays the real-time quote (below) onto the latest price / latest data time — best-effort, so any failure keeps the chart values |
| `trim_trailing_empty_bars()` | private | Drops trailing bars with no traded volume. Runs **after** a same-bucket live observation has been merged, so what remains is a genuine post-close snapshot bucket (e.g. a 15:30 bar after the real 15:15 close), not the live tick |
| `parse_yahoo_japan_realtime_quote()` | private | Extracts the real-time TSE price and HH:MM from Yahoo! JAPAN's server-rendered quote page. The international chart API marks Japanese quotes as delayed; this page is current. Matches only stable component prefixes because class suffixes are build hashes |
| `fetch_yahoo_japan_realtime_quote()` | private | Async. Fetches that quote page and delegates to the parser. Any page / network / parse failure returns `None`, preserving the chart value |
| `apply_realtime_intraday_observation()` | private | Applies a real-time trade to the current intraday bucket: updates the existing bar's close and widens high/low only if the trade sits outside them, or starts a new forming bar at H=L=C=price. **Volume is never invented** (a new bar carries none). Returns immediately for non-intraday modes, so daily/weekly/monthly series stay exactly as the source returned them |

**Retrieval conditions (daily)**: `interval=1d`, `range=3mo`
**Retrieval conditions (5-minute)**: `interval=5m`, `range=5d`
**Retrieval conditions (15-minute)**: `interval=15m`, `range=1mo`
**Retrieval conditions (30-minute)**: `interval=30m`, `range=1mo`
**Retrieval conditions (weekly)**: `interval=1wk`, `range=2y`
**Retrieval conditions (monthly)**: `interval=1mo`, `range=10y`

---

### `src/technical/mod.rs`

Public API re-export definition for the `technical` submodule. Contains no logic.

Re-export targets:
- `composite::calculate_final_score_snapshot`
- `composite::calculate_score_gauge`
- `indicators::build_basic_technical_entry`
- `indicators::evaluate_all_selected_extensions`
- `indicators::get_extension_evaluator`
- `indicators::evaluate_all_selected_extensions_with_report`
- `indicators::ExtensionEvaluationFailure`, `ExtensionEvaluationReport`
- `types::AnalysisResult`, `TechnicalDataEntry`, `TechnicalDataGuard`

---

### `src/technical/types.rs`

Definition of all data structures used in technical analysis. Contains no logic.

| Type | Public | Summary |
|----|------|------|
| `AnalysisResult` struct | pub | Analysis result for one indicator. Has `indicator_name`, `description` (Vec of display strings), `score` (f64) |
| `FinalScoreSnapshot` struct | pub | Snapshot of the aggregated score for all indicators. Has `total_score`, `total_weight`, `score_ratio` (-1 to +1) |
| `TechnicalDataEntry` struct | pub | Struct holding all technical data in a flat structure. About 50 fields including ticker, OHLCV, each indicator value, and each score |
| `TechnicalDataGuard` struct | pub | Safe wrapper for `TechnicalDataEntry`. The wrapped entry is **private**; writes are only permitted through dedicated methods and no API hands out a mutable reference to it |
| `TechnicalDataGuard::new()` | pub | Initializes the guarded struct. Specifies ticker and date |
| `TechnicalDataGuard::get_*()` / `set_*()` | pub | Accessors for each field. Many methods including `get_ema_score()`, `set_rsi()`, etc. `set_currency()` records the quoted currency reported by the market data |
| `TechnicalDataGuard::is_computed()` / `computed_indicator()` | pub | Whether an indicator **produced a value**, and that value if so. Covers the indicators stored as a plain `f64` (`rsi`, `macd`, `macd_signal`, `ema_s/l`, `sma_s/l`, `bb_u/l`, `pct_b`), where `0.0` would otherwise be indistinguishable from "no value": a setter marks its key only when it runs with a finite value, and a later failed recomputation clears both the value and the flag. A value that was never produced — never computed, or a computation that failed — is therefore **absent**, while a legitimately computed `0.0` is present. Indicators already stored as `Option` (ADX, %K, VWAP, ROC, Fibonacci, Ichimoku, volume) carry that distinction in the type and are read through their own getters |

---

### `src/technical/indicators.rs`

Responsible for calculation logic of all technical indicators and storage to `TechnicalDataGuard`.

| Function | Public | Summary |
|------|------|------|
| `build_basic_technical_entry()` | pub | Calculates MACD, RSI, and basic score from a `MarketData` array, then builds and returns `TechnicalDataGuard`. Alias name resolution (references `config.no_alias`) also occurs here |
| `evaluate_all_selected_extensions()` | pub | Evaluates the indicators in `config.enabled_extensions` in order and stores scores in `TechnicalDataGuard` |
| `get_extension_evaluator()` | pub | Returns the corresponding `ExtensionEvaluator` (function pointer) from an `ExtensionIndicator` enum value |
| `calculate_period_vwap()` | pub | Calculates VWAP from the most recent `period` bars of OHLCV data. Returns `Err` if any data has `volume=None` |
| `HardcodedInfo` struct | private | Holds one hardcoded formal name (`formal_name`) — nothing else |
| `resolve_hardcoded_info()` | private | The fund's own formal name for QQQ / SPY / ACWI, used only after the CSV alias **and** the provider's reported name, so it never overrides a source. The index each fund tracks is no longer appended in brackets (it read as though the fund were the index), and the unreachable `FANG+` entry is gone |
| `evaluate_and_store_ema()` | private | EMA calculation → score judgment → storage to guard |
| `evaluate_and_store_sma()` | private | SMA calculation → score judgment → storage to guard |
| `evaluate_and_store_adx()` | private | ADX calculation → score judgment → storage to guard |
| `evaluate_and_store_roc()` | private | ROC calculation → score judgment → storage to guard |
| `evaluate_and_store_stochastics()` | private | Stochastics %K/%D calculation → score judgment → storage to guard |
| `evaluate_and_store_bollinger()` | private | Bollinger Bands, %B, Bandwidth calculation → score judgment → storage to guard |
| `evaluate_and_store_fibonacci()` | private | Fibonacci level calculation from high/low → score judgment → storage to guard |
| `evaluate_and_store_vwap()` | private | Intraday mode: session VWAP calculation using all bars in the same session as the last bar used for indicator calculation. Daily mode: rolling VWAP calculation for the most recent N bars via `calculate_period_vwap()` → score judgment → storage to guard |
| `evaluate_and_store_ichimoku()` | private | Ichimoku (Conversion Line, Base Line) calculation → score judgment → storage to guard |

**Scope of `no_alias` effect**:
When resolving `alias_name_opt` inside `build_basic_technical_entry()`, if `config.no_alias=true`, `ticker_name_map` is not referenced and `None` is returned. This causes the stock name header to use the ticker symbol instead of the alias.

---

### `src/technical/composite.rs`

Aggregates all indicator scores to generate the final score and visual gauge.

| Function | Public | Summary |
|------|------|------|
| `calculate_final_score_snapshot()` | pub | References weights from `Config` to compute the weighted aggregate of all indicator scores and returns `FinalScoreSnapshot` |
| `calculate_final_score()` | private | Internal implementation returning the weighted aggregate score (f64) |
| `calculate_score_gauge()` | pub | Generates a text-format gauge string from the score ratio |
| `get_extension_evaluator()` | pub | Adapter: parses an indicator name string (e.g. `"ema"`) to `ExtensionIndicator`, then delegates to `indicators::get_extension_evaluator()`. Returns `None` for unknown strings |

**Score formula**:
$$\text{score\_ratio} = \frac{\sum(\text{score}_i \times \text{weight}_i)}{2 \times \sum(\text{weight}_i)}$$

---

### `src/news.rs`

Responsible for news retrieval and query generation via the Brave Search API.

| Type / Function | Public | Summary |
|----------|------|------|
| `Article` struct | pub | One news article. Has `title`, `url`, `published_at` |
| `BraveArticleFetcher` struct | pub | Production implementation of `ArticleFetcher` trait. Delegates to `fetch_articles_from_brave()` |
| `news_flow_controller()` | pub | Async. Receives `fetcher: &impl ArticleFetcher`. Reads API key on demand → `run_news_once()` → `compose_news_lines()` |
| `news_flow_controller_with_key()` | pub(crate) | Async. Inner implementation accepting an injected API key. Used by tests to avoid depending on a local `xoksa.env` file |
| `compose_news_lines()` | pub | Generates display strings from retrieved article title, URL, and publication time metadata |
| `news_filter_criteria()` | pub | Returns the canonical title-level news confirmation criteria string for LLM prompts. Shared between non-chat analysis prompt and chat-mode `/show nf` |
| `run_news_once()` | private | Async. Receives `fetcher: &impl ArticleFetcher`. Identifies JP/US, constructs query, and calls fetcher. References `config.no_alias` to switch query string |
| `build_news_query_line_for_log()` | private | Generates query string for log recording. References `config.no_alias` |
| `build_news_query_jp()` | private | Constructs JP query string (stock name OR securities code OR ticker) |
| `build_news_query_us()` | private | Constructs US query string. Ticker only when `company_name=None` |
| `news_locale_for_ticker()` | private | Returns `("JP", "jp", "ja-JP")` or `("US", "en", "en-US")` based on whether ticker ends with `.T` |
| `fetch_articles_from_brave()` | private | Async. Sends HTTPS request to Brave Search API and returns article list. API key managed with `Zeroizing` |
| `normalize_url()` | private | Normalizes retrieved URLs (removes query parameters, normalizes trailing slashes) |
| `print_lines_to_terminal()` | private | Outputs string Vec to standard output one line at a time |

**Scope of `no_alias` effect**:
- JP: `no_alias=true` → Uses `guard.get_ticker()` instead of `guard.get_name()` as the first argument of the query
- US: `no_alias=true` → Sets `company_name=None` to construct query with ticker only

---

### `src/render.rs`

Responsible for terminal display logic of technical analysis results. Display functions per indicator have been separated into `render_indicators.rs`.

| Type / Function | Public | Summary |
|----------|------|------|
| `render_ranked()` | pub | Returns the basic indicator plus every enabled extension as a flat `Vec<AnalysisResult>`, ranked by configured weight (highest first; equal weights ordered randomly). No category headers. Feeds both the terminal display and the LLM prompt (SOT) — the single source of the presentation order |
| `Renderer` trait | pub | Abstract interface for renderers. Defines `render_to_terminal()` and `compose_final_score_lines()` |
| `TerminalRenderer` struct | pub | Standard implementation of `Renderer`. Forwards all methods to delegating functions |
| `technical_render_to_terminal()` | pub | Executes the full display of basic analysis + extended analysis + final score |
| `render_basic()` | pub | Calculates basic score based on MACD/RSI and returns `AnalysisResult` |
| `compose_final_score_lines_stance()` | pub | Generates final score string Vec and gauge according to stance (buyer/seller/holder) |
| `collect_main_info_lines()` | pub | Returns the header info lines (stock name, times, latest retrieved price, indicator bar…) as `Vec<String>` — the collect-only counterpart of `display_main_info`, used when building the chat/context text |
| `render_final_score()` | private | Outputs aggregate score and gauge to terminal |
| `display_main_info()` | private | Outputs header information to terminal: stock name, analysis time, market data latest time, bar containing latest retrieved price, last bar used for indicator calculation, latest retrieved price, etc. |
| `display_analysis_result()` | private | Outputs the `description` lines of `AnalysisResult` to terminal |
| `render_unipolar_gauge_rtl()` | private | Generates unipolar gauge (right-to-left direction) string |
| `render_bipolar_gauge_lr()` | private | Generates bipolar gauge (left-right symmetric) string |
| `stance_caption()` | private | Converts stance enum to gauge caption string |

### `src/render_indicators.rs`

Display logic for 9 extended technical indicators. Called as `pub(crate)` from `render.rs`.

| Type / Function | Summary |
|----------|------|
| `render_ema()` | Generates display line Vec for EMA score |
| `render_sma()` | Generates display line Vec for SMA score |
| `render_adx()` | Generates display line Vec for ADX score |
| `render_roc()` | Generates display line Vec for ROC score |
| `render_stochastics()` | Generates display line Vec for Stochastics score |
| `render_bollinger()` | Generates display line Vec for Bollinger Bands score |
| `render_fibonacci()` | Generates display line Vec for Fibonacci score |
| `render_vwap()` | Generates display line Vec for VWAP score |
| `render_ichimoku()` | Generates display line Vec for Ichimoku score |

---

### `src/fundamental.rs`

Responsible for retrieval and formatting of fundamental supplementary information (financial metrics, dividends, share count, etc.). Called only when the `--fundamental` flag is specified.

| Type / Function | Public | Summary |
|----------|------|------|
| `FundamentalData` struct | pub | Confirmed fundamental information. **All fields are private**; they are read through getters, and every path that writes one validates it — there is no way to place an unchecked figure in the struct. Carries `market`, `data_source`, `currency`, `amount_unit` (unit for large values; v2→`"JPY"` / SEC→`"USD"`), `fiscal_period`, `reported_date`, `revenue`, `operating_income`, `net_income`, `eps`, `bps`, `equity`, `shares_outstanding`, `per`, `pbr`, `roe`, `dividend`, `dividend_is_forecast`, `trading_unit`, `next_fiscal_year_end`, plus the price the ratios were derived from |
| `FundamentalInputs` struct | pub | The in-flight values a provider path collects, before validation. It carries no derived ratio — those are computed inside the confirmed data |
| `FundamentalData::build()` | pub | Where the initial construction from a provider's result is concentrated: every provider path goes through it. (`Default` yields an empty value carrying no figures; `set_bps` / `recompute_derived` are the validated public methods for a later BPS or price update.) Takes `FundamentalInputs` + `latest_price`, refuses non-finite figures (keeping a legitimate zero and a meaningful negative), and derives PER/PBR/ROE **once** from validated inputs rather than separately in each provider path. Division by zero and overflow leave a ratio undefined rather than producing infinity |
| `FundamentalData::recompute_derived()` / `set_bps()` | pub | Keep the derived ratios consistent with their inputs and with the price they came from, so a ratio cannot survive beside inputs that have moved on |
| `FundamentalData::operating_margin_pct()` / `roe_pct()` / `lot_dividend()` | pub | Derived readings for display and for the confirmed data. They return `None` rather than a non-finite value, so an overflowing ratio never becomes `inf%` |
| `FundamentalMarket` enum | pub | Market classification of a ticker (`Japan` / `Us` / `Unsupported`), used to route fundamental retrieval |
| `detect_market()` | pub | Returns the `FundamentalMarket` for a ticker (`.T`/4-digit → `Japan`, else `Us`) |
| `fetch_fundamental_data()` | pub | Async. Calls `fetch_jquants()` if ticker is a Japanese stock (`.T`/4-digit), otherwise calls `fetch_sec_edgar()`. `latest_price` is used for PER/PBR calculation |
| `render_fundamental_display()` | pub | Receives `FundamentalData` and `lang: &str` and formats into `Vec<String>` for terminal display. Values have comma-separated formatting and currency prefix. Dividend forecast appends EN: `(forecast)` / JA: `（予想）` |
| `format_fundamental_for_llm()` | pub | Formats `FundamentalData` into a text block for LLM prompt |
| `fetch_jquants()` | private | Async. Retrieves fundamental information from J-Quants API. Two-pass architecture of v2 (JQUANTS_API_KEY) and v1 (token chain). v2 uses `/v2/fins/summary` and `/v2/equities/master` |
| `jquants_bearer_token()` | private | Async. Resolves `JQUANTS_API_KEY` (v2 only). Managed with `Zeroizing<String>` and immediately discarded after use |
| `fetch_sec_edgar()` | private | Async. Retrieves fundamental information from SEC EDGAR companyfacts API. CIK resolved via ticker → EDGAR company_tickers.json. For class shares like BRK.B, uses ticker transformation (`.` → `-` / deletion) for fallback search |
| `pick_best_xbrl()` | private | Targets entries where `form` is 10-K/10-Q. When `annual_only=true`, prioritizes `fp=="FY"`, and returns 1 item in descending order of `end` → `filed` date. Ensures stability of SEC data selection |
| `fmt_currency_auto()` | private | Formats `Option<f64>` into a comma-separated string with currency prefix (None → "N/A") |

**J-Quants (v2 only)**:
- `JQUANTS_API_KEY` → `x-api-key` header → `/v2/fins/summary` → `json["data"]` → `Sales/OP/NP/EPS/BPS/Eq/DivAnn/FDivAnn/NxFDivAnn/DiscDate/CurPerType/CurFYEn/ShOutFY/NxtFYEn`
- Dividend fallback: `DivAnn` → `FDivAnn` → `NxFDivAnn` (when forecast, `dividend_is_forecast=true`)
- Minimum trading unit: `/v2/equities/master` → `json["data"][0]["TradingUnit"]`

**SEC EDGAR field mapping**:
- Revenue: `Revenues` / `RevenueFromContractWithCustomerExcludingAssessedTax` / `SalesRevenueNet` (`annual_only=true`)
- OperatingIncome: `OperatingIncomeLoss` (`annual_only=true`)
- NetIncome: `NetIncomeLoss` (`annual_only=true`)
- EPS: `EarningsPerShareBasic` / `EarningsPerShareDiluted` (`annual_only=true`)
- Equity: `StockholdersEquity` (`annual_only=false`)
- Shares: `CommonStockSharesOutstanding` (latest end date)
- Dividend: `CommonStockDividendsPerShareDeclared` / `...Paid` (`annual_only=false`)

---

### `src/app.rs`

Pipeline that reads extended indicator scores from `TechnicalDataGuard`, repacks them into `AnalysisResult`, and calls terminal display and log saving. Receives fundamental information and displays it sequentially to the terminal.

| Function | Public | Summary |
|------|------|------|
| `build_analyzed_guard()` | pub | Async. Runs the analysis pipeline (market → technical → score) and returns the populated `TechnicalDataGuard` — the single SOT entry point reused by the CLI (`main.rs`) and the server (`server/api.rs`, `server/monitor.rs`) |
| `run_output_pipeline()` | pub | Scans `config.enabled_extensions` to accumulate each score into `Vec<AnalysisResult>`, then calls `render::technical_render_to_terminal()` and `output::save_technical_log()`. Receives `fundamental_data: Option<&FundamentalData>`; if Some, displays the output of `fundamental::render_fundamental_display()` to the terminal |
| `collect_display_lines()` | pub | Returns all display lines for the current analysis as `Vec<String>` (same content as terminal display, without actually printing). Used when building the chat base context |

---

### `src/output.rs`

Responsible for CSV/JSON output of technical logs. Abstracted via traits.

| Type / Function | Public | Summary |
|----------|------|------|
| `TechnicalLogFormatter` trait | pub | Formatter abstraction defining 3 methods: `csv_header()`, `csv_row()`, `json_row()` |
| `TechnicalLogWriter` trait | pub | Writer abstraction defining the `write()` method |
| `DefaultTechnicalLogFormatter` struct | pub | Standard implementation of `TechnicalLogFormatter`. Dynamically generates CSV header and rows according to enabled extended indicators |
| `FileOrStdoutTechnicalLogWriter` struct | pub | Standard implementation of `TechnicalLogWriter`. Switches output destination with the `--stdout-log` flag |
| `generate_csv_header()` | pub | When the `--show-log-header` flag is active, outputs the CSV header based on current settings to standard output and exits |
| `save_technical_log()` | pub | Entry point for log saving. Assembles formatter and writer, then calls `write()` |
| `technical_json_value()` | pub | Builds the technical block as a `serde_json::Value` from the guard + score snapshot — the single source the JSON `json_row` and the server's JSON API share |

### `src/prompt.rs`

Responsible for assembling the prompt string sent to the LLM and the send flow.

| Type / Function | Public | Summary |
|----------|------|------|
| `PromptRenderer` trait | pub | Prompt builder abstraction defining the `build_prompt()` method |
| `DefaultPromptRenderer` struct | pub | Standard implementation of `PromptRenderer`. Contains `Renderer` and passes basic analysis, extended analysis, and score lines to `llm::compose_llm_prompt_lines()` to assemble the prompt |
| `build_analysis_prompt()` | pub | Convenience wrapper that constructs a `DefaultPromptRenderer` and calls `build_prompt()`; takes the config, guard, news and optional fundamental data and returns the assembled prompt string. Used by `main.rs` for both the analysis prompt and the chat base context. (The send itself is inline in `main.rs`: `llm::save_prompt_to_file` when `debug_prompt`, then `LlmDispatchSender.send_prompt` when `!no_llm && !silent`.) |

---

### `src/llm.rs`

Responsible for sending to LLM APIs (OpenAI / Gemini / Claude) and assembling prompt strings.

| Type / Function | Public | Summary |
|----------|------|------|
| `ChatTokenUsage` struct | pub | Holds `input` and `output` token counts returned by an LLM API call |
| `LlmDispatchSender` struct | pub | Production implementation of `PromptSender` trait. Dispatches to `send_openai_prompt` / `send_gemini_prompt` / `send_claude_prompt` / `send_ollama_prompt` according to `config.llm_provider` |
| `OpenAiPromptSender` struct | pub | Compatible implementation of `PromptSender`. Delegates directly to `send_openai_prompt()` (production uses `LlmDispatchSender`) |
| `news_triage_directive()` | pub | Returns the Tier A/B/C title+URL news confirmation-priority instruction string. Shared between `compose_llm_prompt_lines()` (non-chat) and chat-mode `/show nf`; requires plain bullet output instead of Markdown tables |
| `compose_llm_prompt_lines()` | pub | Integrates stance, configuration flags, basic analysis lines, extended analysis lines, score lines, and news article list to generate prompt line Vec |
| `save_prompt_to_file()` | pub | Writes prompt to `debug_prompt_<nanosec>.txt` (when `--debug-prompt`; uses `create_new` to avoid collisions) |
| `send_openai_prompt()` | pub | Async. Sends prompt to OpenAI Chat Completions API (`/v1/chat/completions`) and stream-outputs the response. API key managed with `Zeroizing` |
| `final_language_directive()` | pub | Returns the fixed output-language instruction (`en`/`ja`) appended to every prompt so the LLM answers in the configured language |
| `alert_explain_note()` | pub | Async. Produces the optional one-line explanation attached to a chat-notification alert; its output still passes the §1 output-integrity guard |
| `send_chat_turn()` | pub | Async. Sends a single chat-mode prompt to the configured provider and returns the response string. Used by chat-mode slash commands (`/show`, `/compare`, etc.) |
| `send_chat_turn_with_usage()` | pub | Async. Same as `send_chat_turn()` but also returns `ChatTokenUsage` for token tracking |
| `send_gemini_prompt()` | private | Async. Sends prompt to Gemini API (`generativelanguage.googleapis.com/v1beta`). Retries on 429/503 using `Retry-After` header (up to 3 times). Extracts response body by excluding `"thought": true` parts from thinking models. API key managed with `Zeroizing` |
| `send_claude_prompt()` | private | Async. Sends prompt to Anthropic Messages API (`api.anthropic.com/v1/messages`). Retries on 429/529/503. Has a dynamic fallback that resends without `temperature` when a 400 response contains "temperature". API key managed with `Zeroizing` |
| `send_ollama_prompt()` / `run_ollama_benchmark()` | private | Async. Sends to Ollama `/api/chat` endpoint (`OLLAMA_HOST:OLLAMA_PORT`) with stream=false. No API key required. reqwest client has `.no_proxy()` to always disable proxy. temperature/top_p/top_k/repeat_penalty/keep_alive are only sent when configured. num_ctx defaults to `32768`, num_predict defaults to `8192`, seed defaults to `42` when unset. Thinking mode is only sent when `OLLAMA_THINK` / `--ollama-think` is specified; no auto-switching by model name. If Ollama returns a 400 response indicating thinking is unsupported, retries once without sending `think` based on API response, not model name. The SOT system message — suppressing ticker facts supplemented from trained knowledge, news/values/dates/proper nouns not in the input, and unit conversion/rounding/recalculation/independent price target derivation — is **the same for every provider** (`sot_system_prompt`, sent by the openai / gemini / claude / ollama builders alike), so no provider is held to a different standard. Every number in the response body is verified against the confirmed values passed alongside the prompt (`integrity::ConfirmedFactSet`) — same instrument, bar, indicator, unit, currency, sign and period — so a value moved from another reading, a reading of an indicator never computed, a bare unprovided price range, an independently derived buy/sell level, a comparison the input never carried, or a reversed VWAP reading are detected; the detection content is displayed and only the problematic sentences/lines are excluded from consideration (can be disabled with `--no-ollama-guard` / `OLLAMA_NO_GUARD=true`). The checks are not per language: each holds its Japanese and English wording together. When generation is cut off by `done_reason=length`, the LLM body is suppressed to avoid incorrect display (maintained even with `--no-ollama-guard`). `--debug-ollama` / `OLLAMA_DEBUG=true` displays only send options and response metadata without outputting the prompt body. `--llm-benchmark` sends the same prompt to multiple Ollama models and outputs `done_reason`, ctx usage, tokens/sec, thinking volume, consistency check counts, etc. in table/csv/json without displaying the body. Connection failures, timeouts, and missing models are individually displayed as errors |

---

### `src/integrity.rs`

The confirmed-data reference the LLM output-integrity guard verifies against. Built **only** from the structures that own confirmed values (`TechnicalDataGuard`, `FundamentalData`) and handed to the checker as data — no string is ever read as a source, because a prompt also carries the conversation and other models' words. The detection rules themselves are normative and live in [security-design.md §1](./security-design.md).

| Type / Function | Public | Summary |
|----------|------|------|
| `SymbolFacts` struct | pub | One instrument's confirmed values, with its symbol, name, currency, bar year and (multi-timeframe only) the bar it was computed on |
| `SymbolFacts::from_sources()` | pub | Builds the above from a guard, a `Config` and optional `FundamentalData`. Only indicators the guard records as having **produced a value** become facts, so a computation that failed is absent rather than confirmed as `0.0`. Each value carries what distinguishes it: the window it was computed over where the indicator has legs (EMA/SMA/bands), and its own fiscal year for a fundamental one |
| `SymbolFacts::with_timeframe()` / `carry_over_fundamentals()` | pub | Names the bar these values belong to; carries **every** fundamental value of a previous load when a reload did not re-fetch them — the per-lot dividend and the trading unit included, so the confirmed data covers the whole block the session still shows |
| `ConfirmedFactSet` struct | pub | The confirmed data of a request — one entry per instrument, and per timeframe where several are loaded. `facts_for()` / `facts_from_parts()` build it |
| `ConfirmedFactSet::verify()` / `verify_with()` | pub | Verifies one written number against the fact its own **claim** attributes it to: instrument, bar, indicator, unit, currency, magnitude, sign and period. `verify_with` takes a `Scan` the caller already built for that sentence |
| `ConfirmedFactSet::compare()` / `single_value()` | pub | Reads a direction (e.g. VWAP vs close) from the confirmed values, so the prompt's wording cannot change the verdict |
| `ClaimVerdict` enum | pub | Why a claim failed — wrong indicator, wrong instrument, uncomputed indicator, unit/currency, period, or no confirmed counterpart. `is_reportable()` decides what is surfaced |
| `Scan` struct | pub | Where the numbers, dates, years, period specifications, timeframes, tickers and stated indicator windows of a sentence are (each window with the span of the indicator name it was written on). Built **once per sentence** and shared by the claim splitting and every binding, so notation (a thousands separator, a date spelling, a window written in brackets or in front of the name) cannot change a verdict |
| `written_numbers()` | pub | Reads every number with its sign, magnitude, unit, currency and role (observation / period parameter / date). A ticker's digits are not readings; an unknown suffix is never dropped |
| `sentence_bounds()` / `sentence_end()` | pub | Sentence splitting shared with the guard: an English `.` ends a sentence only when whitespace or the end follows, so a decimal point and a ticker's dot do not split one. The sanitizer uses `sentence_end`, so in prose what is judged and what is removed are the same spans; a table row, a bullet or a single-sentence line is removed whole instead |

---

### `src/chat/`

Interactive chat loop and slash-command processing, organized as a module directory. Called from `main.rs` when `--chat` is specified.

| Submodule | Summary |
|------|------|
| `chat/mod.rs` | `ChatSession` definition, shared chat types, and small free helpers |
| `chat/exec.rs` | Unified slash-command dispatch — `ChatSession::execute`, shared by CLI and Web — plus Web-session helpers (`run_web_command`) and the per-client LLM selection store (`apply_client_llm_selection` / `store_client_llm_selection` / `client_llm_label`) |
| `chat/prompt.rs` | `impl ChatSession` prompt/context assembly (chat, `/forum crit`, forum deliberation/Chair, `/basic`) |
| `chat/run.rs` | `run_chat_loop`: the interactive REPL and slash-command dispatch |
| `chat/guard.rs` | Investment-advice constraint text, forecast policy, and `/forum crit` task text (pure consts + selectors) |
| `chat/help.rs` | `/help` listings (pure `println!`) |
| `chat/council.rs` | Council/Board multi-LLM deliberation (`handle_council_command`, `send_council_ask`, facilitator resolution) |
| `chat/debate.rs` | Debate Buffer & `/forum crit` helpers (`send_criticize_last_to_llm`, excerpt cleaning) |
| `chat/report.rs` | Token/context-size reporting and chat-status display (`/token`, `/status`) |
| `chat/llm.rs` | `/llm` provider switching, model resolution, and chat-turn dispatch |
| `chat/ticker.rs` | Ticker loading/switching/reloading and the autoreload notice |
| `chat/news_extra.rs` | `/nx` buffer: fetch and inject extra news slots |
| `chat/tests.rs` | `#[cfg(test)]` unit tests for the chat module |

| Type / Function | Public | Summary |
|----------|------|------|
| `run_chat_loop()` | pub | Async. Main entry point for chat mode. Accepts base analysis context, technical/fundamental display text, ticker label, news articles, and ticker name map. Manages the REPL loop, memory, Debate Buffer, `/forum crit` dedicated SOT-coverage prompt, `/status` chat-state summary, `/token detail` context-size breakdown, auto-reload timer, and slash-command dispatch |

**Slash commands handled internally**:
`/help`, `/show` (t/f/n/nf), `/basic`, `/mem`, `/set`, `/status`, `/forum`, `/reload`, `/auto`, `/sym`, `/llm`, `/mode`, `/nx`, `/depth, /scope`, `/token [detail]`, `/bye`

---

### `src/keystore.rs`

OS Keychain access layer. All Class A credentials (API keys, tokens) are stored per-key under the service name `"xoksa"` via the `keyring` / `keyring-core` crates. Acts as the single gateway for secure key retrieval and storage; callers receive `Zeroizing<String>` to ensure memory is zeroed on drop.

| Function | Public | Summary |
|------|------|------|
| `get_key()` | pub | Retrieve a key from the OS Keychain by name. Returns `Ok(Some(key))` if found, `Ok(None)` if not set, `Err` on keyring access failure. Return type is `Result<Option<Zeroizing<String>>>` |
| `delete_key()` | pub | Delete a key from the OS Keychain by name. Returns `Ok(())` if deleted or not present, `Err` on access failure |
| `set_key()` | pub | Store a key in the OS Keychain by name. Returns `Err` on keyring access failure |
| `resolve_key_presence()` | pub | Check whether a key is present in `xoksa.env` or OS Keychain. Returns `KeyPresence` (`Found(source)` / `NotFound` / `KeyringError`) to distinguish access failures from absent keys |

---

### `src/setup.rs`

Interactive wizard (`--init`) and environment health check (`--doctor`). Called from `bootstrap.rs` before normal analysis starts.

| Function | Public | Summary |
|------|------|------|
| `run_init()` | pub | Interactive wizard that generates a new `xoksa.env` file. Asks about each feature (LLM, news, fundamental data, proxy) independently. Backs up any existing `xoksa.env` to `xoksa.env.bak_NNN` immediately before writing the new file. Receives optional `lang_override` for early message localisation |
| `run_update_key()` | pub | Interactive prompt to update a single stored API key in the OS Keychain. Returns `true` on success, `false` if the user cancelled with Ctrl-C |
| `run_check_keys()` | pub | Reports presence/absence of all 9 managed API keys (checking both `xoksa.env` and OS Keychain) without displaying values |
| `run_doctor()` | pub | Checks API key presence (via `resolve_key_presence`, covering both `xoksa.env` and OS Keychain) and proxy/connection settings, and reports results to the terminal. Does not exit; returns after printing |
| `run_apply_config_cli()` | pub | Headless apply-setup for the `apply-config` subcommand: reads a JSON config from stdin (incl. Class A keys) and writes the env file + OS keychain, reusing the same logic as `--init`. Driven by the settings app (`xoksa-setup`) over the child process's stdin |
| `run_ollama_models_cli()` | pub | Backs the `ollama-models` subcommand: queries a local Ollama server (host/port args) and prints its model list as JSON for the settings app |

---

### `src/server/`

Local HTTP server for the Web UI (`xoksa serve --ui`). Transport/UI layer only — analysis, indicators, fundamentals, LLM, and backtests stay native and are reached only through the HTTP API. Intercepted in `main()` before the analysis CLI parser, so the existing CLI/chat are untouched.

| Submodule / Function | Public | Summary |
|------|------|------|
| `server/mod.rs` | pub | `ServeArgs` (clap; `--ui`/`--port`/`--host`/`--web-dir`/`--private`/`--log-format`), `run_serve_cli()` / `run_server()` (axum router, bind, URL print, security-header middleware), and in-house static-asset serving with a path-traversal guard (`read_asset`, unit-tested) + `.wasm` MIME + cache-control (no-store index.html). `read_asset` prefers an on-disk `--web-dir` (defaults to the Leptos build output `webui-leptos/dist`), then **falls back to assets embedded in the binary** — built in via the `embedded-ui` cargo feature (`include_dir!` on `webui-leptos/dist`), so a release binary serves the dashboard with no external `dist/`. The Rust/WASM frontend source lives in `webui-leptos/` (Leptos, standalone crate, built with `trunk`; not part of the native workspace/CI) |
| `server/api.rs` | pub(crate) | HTTP handlers + JSON models. Status: `health` / `config`. Per-symbol: `symbol_summary` (real pipeline via `app::build_analyzed_guard` + `collect_display_lines`), `symbol_news`, `symbol_chart`. Analysis: `analysis_multi_timeframe`. Backtest: `backtest` / `list_rules` / `save_rule` (rules stored in a JSON file `~/.xoksa.strategies.json`). Alerts: `alerts_list` / `alerts_add` / `alerts_delete` / `alerts_toggle` / `alerts_test`. LLM: `llm_options` / `llm_select`. Settings: `set_lang` (writes `LANG`; loopback-only, refuses under `--private`). Chat: `chat_stream` (SSE, real LLM grounded in the analysis, unified `ChatSession::execute` dispatch) / `chat_commands`. `build_server_config` builds a dashboard Config from `bootstrap::load_env_map` |
| `server/monitor.rs` | pub(crate) | Alert monitor: one background task for the whole `serve` process, independent of any browser session. `spawn_alert_monitor` polls the intraday bars at their own cadence and pushes a chat notification when a computed-value condition crosses (rising-edge; once per ticker/condition/indicator bar). Rule/channel CRUD backing the `alerts_*` endpoints (`add` / `remove` / `set_active` / `send_test`, `RuleView` / `ChannelView`) and `--private`-honoring `persist_add_to_env` / `persist_remove_from_env` |

---

### `src/strategies.rs`

File-backed saved backtest strategies — the rule editor's persistence. Loads and saves `~/.xoksa.strategies.json` (a small JSON list of `{name, spec_json}`); backs the `GET`/`POST /api/backtest/rules` endpoints. Honors private mode (skips writing when `--private` is active). No database.

---

### `src/private.rs`

Process-wide private-mode flag and the home-directory helper. `set_private` / `is_private` manage the no-trace flag (set by `--private`); when private, nothing is written to disk. `home_dir` resolves the user home directory (for the strategies / history dotfiles). Shared by the logger and the saved-strategies store.

---

### `src/context.rs`

Builds a multi-timeframe context for the LLM: `ContextSpec` lists `{type, timeframe, indicator_name}` items; each timeframe is analyzed once through the shared engine and summarized on the Rust side into `context_pack_text` + structured `context_pack_json` (never raw bulk data — SOT + token control). `ContextPack.facts` carries the same content as confirmed data, one entry per analyzed timeframe, each naming its bar, so the output-integrity guard verifies a figure against the bar it was stated for.

---

### `src/backtest.rs`

Rule-based backtest over a contiguous provider history. `BacktestSpec` / `StrategyRules` / `Condition`; per-bar indicators recomputed by the same engine (look-ahead-free); realistic money model (cash in the instrument's currency, whole minimum lots, start fraction + per-signal sizing, FIFO tranches); returns metrics + Buy & Hold benchmark + trade ledger + warnings. Saved rules persist via `strategies.rs` to a JSON file (`~/.xoksa.strategies.json`).

---

### `src/logging.rs`

Dependency-free coded logger: `warn` / `error` / `info(code, msg)` with a stable error code, deduplicated console output, and an append-only local file (`logs/xoksa-error.log` under the working directory, 0600, `logs/` auto-created; generational rotation `.1`/`.2` past 5 MB; system fallback if the cwd is unwritable — `%PROGRAMDATA%\xoksa\logs` on Windows, `/tmp/xoksa` on Unix, never the user home; skipped in private mode). `current_log_path` exposes the absolute path for a startup notice. `set_format(Text|Json)` selects human text or NDJSON (`xoksa serve --log-format json`).

---

### `src/notify.rs`

Chat-notification layer. `Notifier` trait + `HttpNotifier` push a one-line message to Slack / Discord / Google Chat (incoming webhook) or LINE (Messaging API). `NotifierKind` is the fixed per-platform host allowlist (`https`-only, validated in code — anti-SSRF); `build_notifier` selects the platform from a channel config; `run_test_notify_cli` backs the `test-notify` subcommand (channel secret arrives on stdin, never argv). The `NOTIFY_<n>_SECRET` is Class A (OS keyring, `Zeroizing`); data POST only, never a shell.

---

### `src/report.rs`

Analysis-report export for `--out <file>`: `write_report` renders the computed analysis to HTML / Markdown / plain text (format inferred from the file extension). Presentation only — the values come from the SOT-guarded analysis, never re-derived here.

---

### `src/traits.rs`

Abstraction trait definitions for HTTP dependencies. Enables dependency injection (mock substitution) during testing. Concrete implementations are placed in each module (`market.rs`, `news.rs`, `llm.rs`).

| trait | Summary |
|-------|------|
| `PriceFetcher` | Defines `fetch_snapshot(ticker, mode)`. `YahooFinancePriceFetcher` is the production implementation |
| `ArticleFetcher` | Defines `fetch_articles(query, api_key, ...)`. `BraveArticleFetcher` is the production implementation |
| `PromptSender` | Defines `send_prompt(config, prompt, facts)`. `facts` carries the confirmed values as structures so the output-integrity guard verifies attribution rather than the prompt text. `LlmDispatchSender` is the production implementation (`OpenAiPromptSender` is for compatibility) |

**Testing method**: By defining a concrete mock struct such as `MockArticleFetcher { articles: Vec<Article> }` and implementing `ArticleFetcher` for it, flow logic can be tested without HTTP calls.

---

### `src/utils.rs`

Cross-cutting utility functions.

| Function | Public | Summary |
|------|------|------|
| `build_proxy()` | pub | Builds a `reqwest::Proxy` from a proxy URL string and optional no-proxy list |
| `parse_env_value()` | pub | Strips inline comments (`#`) and trims whitespace from a raw `xoksa.env` value string |
| `read_key_from_env_file()` | pub | Reads a single key value from `xoksa.env` on demand. Returns `None` if the file or key is absent |
| `resolve_api_key()` | pub | Resolves an API key: checks `xoksa.env` first, falls back to OS Keychain. Returns `Result<Option<Zeroizing<String>>>` — `Ok(Some)` if found, `Ok(None)` if not set, `Err` on keyring access failure. Used for all API key retrieval |
| `key_present_in_env_file()` | pub | Returns `true` if the given key exists and is non-empty in `xoksa.env`. Used for `--doctor` health checks |
| `sanitize_ascii_file_lines()` | pub | Reads a file line by line, performing BOM removal, line length check (error when exceeding 500 characters), NULL byte detection, and control character detection. Used when reading `xoksa.env` and `alias_csv` |
| `classify_score()` | pub | Converts score ratio (-1 to +1) to a label string (e.g., "🟢 強い買い"). Lang-aware |
| `get_color_for_score()` | pub | Returns ANSI color code string from score ratio |
| `get_score_description()` | pub | Returns a description string from the combination of indicator and integer score. Uses static table constants per indicator (`SMA_SCORE_TABLE`, etc.) and common lookup helper `score_table_lookup()`. Lang-aware |
| `display_price_for_diff()` | pub(crate) | Returns the price used for diff display (`get_latest_observed_price`, or `get_close` as fallback) |
| `displayed_price_diff()` | pub(crate) | Returns `(diff_value, diff_pct)` tuple for price diff display |
| `format_analysis_time()` | pub(crate) | Returns the analysis time string formatted for terminal display. Lang-aware |
| `format_time_with_timezone()` | pub(crate) | Appends the given timezone label to the time string and returns a formatted string. Lang-aware |
| `market_data_latest_time()` | pub(crate) | Returns the latest market data timestamp from the guard (intraday: latest bar time, daily: latest date) |
| `floor_time_to_interval()` | pub(crate) | Floors a datetime string to a given minute interval (e.g., 5m, 15m, 30m) |
| `format_bar_time_for_mode()` | pub(crate) | Returns the bar time label string formatted according to analysis mode and lang |
| `format_latest_observed_price_bar()` | pub(crate) | Returns the "latest observed price bar" line string for display/prompt use |
| `format_indicator_latest_bar()` | pub(crate) | Returns the "indicator latest bar" line string for display/prompt use |
| `format_market_data_latest_time()` | pub(crate) | Returns the market data latest time line string for display/prompt use. Lang-aware |
| `macd_minus_policy_label()` | pub(crate) | Returns the MACD-minus-allow policy label string. Lang-aware. Shared by `render.rs` and `llm.rs` |

---

## Desktop crate — `xoksa-desktop/`

A separate Cargo workspace: a **Tauri** connection shell. It links **no** engine code (no `xoksa` dependency); it launches the `xoksa` engine binary shipped beside it as a child process (`xoksa serve`), points the WebView at that engine's dashboard, and opens the standalone settings app (`xoksa-setup`). The CLI / engine crate gains **no** Tauri/WebView dependency. See [design-philosophy.md](./design-philosophy.md) and [security-design.md §6](./security-design.md).

### `xoksa-desktop/src/main.rs`

| Item | Kind | Responsibility |
| :--- | :--- | :--- |
| `engine_bin_path()` / `engine_command()` | fn | Resolve the `xoksa` engine binary beside the app; `engine_command` sets `CREATE_NO_WINDOW` on Windows. Dev falls back to PATH. |
| `setup_bin_path()` / `launch_setup_app()` | fn | Resolve and launch the standalone settings app (`xoksa-setup`) shipped beside the desktop, as its own process. |
| `DesktopConn` / `desktop_conn_path()` / `load_desktop_conn()` / `save_desktop_conn()` | struct / fn | The non-secret connection target (host / port / auto_start) in a desktop-owned `desktop.json` (**not** `xoksa.env`). |
| `ConnToken` / `read_conn_token()` / `write_conn_token()` | struct / fn | The access token the desktop presents to a token-gated engine; stored in the keychain via `xoksa conn-token`. |
| `spawn_local_engine()` / `probe_engine()` / `pick_local_port()` | fn / enum `Probe` | Start a local engine (`xoksa serve --ui`, bind per `DESKTOP_LAN_ACCESS`); health-probe distinguishes Ready / OtherXoksa / AuthRequired (401) / Foreign / Down, with Ready requiring an exact version match (matched pair); `pick_local_port` takes an OS-assigned ephemeral port for loopback auto-start (v2.6.4 restored). |
| `load_connection` | `#[command]` | Prefill for the connection screen: host / port / auto_start + whether a token is stored. |
| `connect` | `#[command]` | Save the target, start a local engine if requested, and return the dashboard URL; a token-gated engine is authenticated by the page-load hook. |
| `open_settings_app` | `#[command]` | Launch the settings app from the connection screen. |
| `open_popup_window` / `open_external` | `#[command]` | Open a dashboard popup (chart / help) as a native window / open an http(s) URL in the OS browser. |
| `bind_host()` / `engine_lang_is_ja()` | fn | Loopback vs `0.0.0.0` per `DESKTOP_LAN_ACCESS`; menu-language lookup via `xoksa config-json`. |
| `main()` / `setup()` | fn | Set cwd to the per-user app-data dir; hold the (initially none) engine child + connection token (killed on `RunEvent::Exit`); native menu (Settings → launch settings app / Restart); on the `/login` page, auto-submit the stored token to `/auth`. |

### `xoksa-desktop/frontend/index.html`

The **connection screen**: which engine to talk to — an **engine address** (host:port), an **access token** (blank for a local engine), and a **start-a-local-engine-automatically** toggle — plus a **Settings** button that opens the standalone settings app. `connect` returns the dashboard URL and the WebView navigates to it. No API keys or configuration here — those live in `xoksa-setup`. Bilingual (en/ja).

---

## Settings-app crate — `xoksa-setup/`

A separate Cargo workspace (like `xoksa-desktop`): a **Tauri** settings app. It links **no** engine code; it drives the `xoksa` engine binary as a child process for all configuration — `config-json` / `apply-config` / `settings-password` / `conn-token` / `serve --show-token`·`--rotate-serve-token` / `ollama-models` / `test-notify` — so it writes `xoksa.env` + the OS keychain without a running server. Password-gated; opened from the desktop's **Settings**. See [security-design.md §6](./security-design.md).

### `xoksa-setup/src/main.rs`

| Item | Kind | Responsibility |
| :--- | :--- | :--- |
| `verify_password` / `change_password` | `#[command]` | Gate the app behind a password (default `XOKSA_password`) via `xoksa settings-password` (keychain; value on stdin, never argv). |
| `load_config` / `save_config` | `#[command]` | Recall the current config (`xoksa config-json`; non-secret + key-presence) and write it (`xoksa apply-config` over stdin; keys never over argv/HTTP). |
| `get_token` / `generate_token` | `#[command]` | Show / rotate the serve access token via `xoksa serve --show-token` / `--rotate-serve-token`. |
| `list_ollama_models` / `test_notify` | `#[command]` | Query installed Ollama models (`xoksa ollama-models`); send one test notification (`xoksa test-notify`, secret on stdin). |
| `restart_engine` | `#[command]` | One action: stop the child this session started (if any) and start a fresh engine (`xoksa serve --ui`) — the only way a new port / access token takes effect. Pre-checks the port and names an occupant it did not start (never kills it); waits for the engine to listen before returning the serve URL. |
| `set_unsaved` / `quit_app` | `#[command]` | Mirror the form's unsaved-changes flag into Rust so the native close button can warn; quit the app (the engine it started keeps running — §6). |
| `engine_bin_path()` / `engine_command()` / `serve_host()` / `serve_port()` | fn | Resolve the engine binary (no-window on Windows); read the bind host / port from `xoksa.env`. |
| `main()` / `setup()` | fn | Set cwd to the shared canonical config dir (SOT — `xoksa-paths`); native window; the frontend gates on the password, then shows the config form. |

### `xoksa-setup/frontend/index.html`

The settings form: a **password gate**, then the full configuration (language, LLM / API keys + cloud models, Ollama, news / fundamentals, chat-notification channels, and — under *Advanced* — indicators), an **access-token** section (generate / copy), and engine **start / restart**. Bilingual (en/ja, English-priority, OS-locale default). API keys reach the engine over the Tauri IPC boundary → the child's stdin (never HTTP).

---

## Dependency Graph (primary call directions)

```
main.rs
  └─ bootstrap.rs             (initialization, Config construction)
  │    └─ setup.rs            (run_init on --init; run_doctor on --doctor)
  └─ market.rs                (YahooFinancePriceFetcher → PriceFetcher trait)
  └─ technical/
  │    ├─ indicators.rs       (indicator calculation, Guard generation)
  │    ├─ composite.rs        (score aggregation)
  │    └─ types.rs            (data structures)
  └─ fundamental.rs           (fundamental retrieval and formatting; only when --fundamental)
  │    ├─ J-Quants API v2/v1  (Japanese stocks: /v2/fins/summary, /v2/equities/master)
  │    └─ SEC EDGAR API       (US stocks: companyfacts, company_tickers.json)
  └─ app.rs                   (output pipeline)
  │    ├─ render.rs           (terminal display)
  │    │    └─ render_indicators.rs  (display logic for each indicator)
  │    ├─ output.rs           (file/stdout log)
  │    └─ fundamental.rs      (render_fundamental_display → terminal display)
  └─ news.rs                  (BraveArticleFetcher → ArticleFetcher trait)
  └─ prompt.rs                (prompt assembly)
  │    ├─ llm.rs              (LlmDispatchSender → PromptSender trait)
  │    └─ fundamental.rs      (format_fundamental_for_llm → appended to end of prompt)
  └─ chat/                    (run.rs: run_chat_loop → REPL; slash commands use llm::send_chat_turn; submodules: prompt/run/guard/help/council/debate/report/llm/ticker/news_extra; tests in tests.rs)

traits.rs    ← defines PriceFetcher / ArticleFetcher / PromptSender
config.rs    ← referenced from all modules
utils.rs     ← referenced from render.rs, render_indicators.rs, output.rs, bootstrap.rs, llm.rs, market.rs, news.rs, fundamental.rs, setup.rs
```

---

## Type Flow

```
MarketData[]
  → indicators::build_basic_technical_entry()
  → TechnicalDataGuard  ─────────────────────────────┐
  → indicators::evaluate_all_selected_extensions()    │
                                                       ▼
                                          render.rs    (terminal display)
                                          output.rs    (log saving)
                                          prompt.rs    (LLM prompt)
                                          composite.rs (score aggregation)

FundamentalData (only when --fundamental)
  → fundamental::fetch_fundamental_data()
       ├─ fetch_jquants()     (Japanese stocks: J-Quants v2/v1)
       └─ fetch_sec_edgar()   (US stocks: SEC EDGAR companyfacts)
  → FundamentalData ─────────────────────────────────┐
                                                       ▼
                              fundamental::render_fundamental_display() → app.rs (terminal)
                              fundamental::format_fundamental_for_llm() → prompt.rs (LLM)
```

---

<a id="ja"></a>

# xoksa ソースコード構成マップ

> **言語同期:** 英語と日本語の記載はいずれも有効です。要件・制約・例外・コマンド挙動・運用ルールが片方の言語にだけ存在する場合でも、両方に適用されます。欠落している側は文書不備として更新対象です。

対象: `src/` 配下の全 `.rs` ファイル（`xoksa` エンジン / CLI クレート）。デスクトップクレート `xoksa-desktop/` と設定アプリクレート `xoksa-setup/` は末尾の専用節に記載。

**この文書が何であり、何でないか。** ファイルは全数に節がある——ここは網羅であり、抜けていれば不備である。一方、ファイルごとの表は**公開項目の完全な索引ではない**。そのファイルが持つ境界と、他とのつながりを読み取るのに要るものを挙げている。公開関数の多くは意図的に載せていない。**「ここに無い」を「存在しない」と読まないこと。** 完全で常に最新の項目一覧が要るならコードか `cargo doc` を読む。この文書があるのは、コードが 1 箇所では述べられない部分——各ファイルの責務と、モジュールをまたぐ流れ——のためである。

**どちらが正か。** **コード側の doc コメント**が正である。記述対象のすぐ隣にあるため黙ってずれることがなく、`cargo doc` はここで選んだ項目ではなく全項目について描画する。doc コメントは**日英併記**で書く——英語、空行、日本語の順。コードを正に据えることで日本語の読者が何かを失うことがないようにするためである。**表の行と doc コメントが食い違う場合、doc コメントが正であり、行のほうが不備である。** 行を直すか、削除して詳細を `cargo doc` に委ねる。ここの記載は要約と道しるべであって、第 2 の仕様書ではない。

---

## エントリポイント

### `src/main.rs`

実行バイナリのエントリポイント。処理のオーケストレーションのみを行い、ロジックは持たない。

| 関数 | 概要 |
|------|------|
| `main()` | 非同期エントリポイント。2つのコードパス: (1) ティッカーなし `--chat` → 市場データパイプラインを丸ごとスキップしてチャットループへ直行; (2) 通常分析フロー → 初期化 → 市場データ → テクニカル分析 → 出力 → ニュース → LLM（任意） → チャットループ（任意） |

**処理フロー（呼び出し順）**

```
bootstrap::initialize_environment_and_config()     → (Config, Option<String>, ticker_name_map, chat_default_extra_tickers)

[分岐A: ティッカーなし --chat]
chat::run_chat_loop(&config, "", ...)              ← 市場データなしでチャットループへ直行
return

[分岐B: 通常分析]
YahooFinancePriceFetcher::fetch_snapshot()         ← PriceFetcher trait 経由
technical::build_basic_technical_entry()
technical::evaluate_all_selected_extensions()
fundamental::fetch_fundamental_data()              ← fundamental=true の場合のみ
app::run_output_pipeline(&fundamental_data)        ← fundamental_data を渡す
news::news_flow_controller(&fetcher)               ← BraveArticleFetcher を注入。no_news=true 時はスキップ
prompt::build_analysis_prompt()                    ← 分析プロンプト（チャット用ベースコンテキストも兼ねる）
llm::save_prompt_to_file()                          ← debug_prompt 時のみ
llm::LlmDispatchSender.send_prompt()                ← !no_llm && !silent && !chat の場合のみ
chat::run_chat_loop(...)                           ← config.chat 時のみ（ティッカーあり）
```

---

## ライブラリルート

### `src/lib.rs`

クレートの公開 API 定義。全モジュールの `pub mod` 宣言と re-export を集約する。`AnalysisEngine` は削除済み（デッドコード）。

---

## モジュール一覧

### `src/bootstrap.rs`

起動時の初期化処理を担当。`xoksa.env` を `env_map`（HashMap）として読み込み・CLI引数パース・エイリアスCSV読み込みをまとめて行う。

| 関数 | 公開 | 概要 |
|------|------|------|
| `initialize_environment_and_config()` | pub | `xoksa.env` を `env_map` として読み込み、CLI引数をパースして `Config` を構築する。APIキーは env_map に含めず各送信関数がオンデマンド取得する。エイリアスCSVも読み込む。戻り値は `(Config, Option<String>, ticker_name_map, chat_default_extra_tickers)` の4要素タプル。`chat_default_extra_tickers` は `--chat` に複数ティッカーを渡した場合の追加ティッカー一覧。ティッカーなし `--chat` 起動では `None` になる |
| `load_env_map()` | pub | `xoksa.env` を `HashMap<String, String>`（`env_map`）に読み込む。サーバ（`build_server_config`）が完全な CLI init を経ずにダッシュボード用 `Config` を構築する際に使う |
| `config_debug_string()` | private | `--debug-args` 時に `Config` を整形文字列で返す。`sec_user_agent` および `https_proxy`（認証情報を含む可能性）を `***` でマスクして出力する |
| `normalize_ticker_input()` | pub | 行き先そのものを指す通称だけを展開する（`全米`・`トータルマーケット` → `VTI`）。該当しなければ打たれたまま返す（最終的に使う値の trim・大文字化は `sanitize_ticker` 側）。**指数名に黙ってファンドを返すことはしない**——`S&P500` → `SPY`、`NASDAQ100` → `QQQ`、`DOW` → `DIA`、`日経平均` → `1321.T`、`TOPIX` → `1306.T`、`FANG+` → `FNGU`（3 倍レバレッジ ETN）のすり替えは 2.9.9 で削除した |
| `sanitize_ticker()` | pub | ティッカー文字列のバリデーション（許可文字種・長さチェック）。不正な場合は `Err` を返す |
| `sanitize_news_query()` | pub | ニュースクエリ文字列のバリデーション（長さ・制御文字チェック） |
| `sanitize_llm_note()` | pub | LLM追加ノートのバリデーション（長さ・制御文字チェック） |
| `normalize_ticker()` | pub | `.T` サフィックスの正規化など、ティッカー表記の統一化 |
| `jp_code_from_ticker()` | pub | ティッカーが日本株（`XXXX.T` または4桁数字）の場合に証券コード文字列を返す。そうでなければ `None` |
| `load_alias_csv()` | pub | 指定パスのCSVを読み込み `code -> 銘柄名` の `HashMap<String, String>` を返す |

**注意**: `initialize_environment_and_config()` は `xoksa.env` を `env_map`（HashMap）として保持し、一般設定の解決に使用する。APIキー（`OPENAI_API_KEY` / `BRAVE_API_KEY` / `GEMINI_API_KEY` / `CLAUDE_API_KEY` / `JQUANTS_API_KEY`）は env_map に入れず、各取得関数が `resolve_api_key()` でオンデマンド取得する（`xoksa.env` を優先し、未設定の場合はOSキーチェーンにフォールバック）。

---

### `src/config.rs`

CLI引数定義・`Config` 構造体定義・`build_config()` / `build_config_from_arg_matches()` によるCLI/環境変数/デフォルトのマージロジックを持つ。

| 型・関数 | 公開 | 概要 |
|----------|------|------|
| `AnalysisMode` enum | pub | 分析足モード。`Daily` は `1d/3mo`、`Intraday5m` は `5m/5d`、`Intraday15m` は `15m/1mo`、`Intraday30m` は `30m/1mo`、`Weekly` は `1wk/2y`、`Monthly` は `1mo/10y` |
| `ExtensionIndicator` enum | pub | 拡張テクニカル指標の列挙型（`Ema`, `Sma`, `Bollinger`, `Roc`, `Adx`, `Stochastics`, `Fibonacci`, `Vwap`, `Ichimoku`） |
| `Stance` enum | pub | 投資スタンス（`Buyer`, `Seller`, `Holder`） |
| `Args` struct | pub | clap による CLI引数定義。全オプション約50個を定義 |
| `Config` struct | pub | 実行時設定の唯一の真実。全フィールドは `build_config()` で確定される |
| `Config::default()` | pub | テスト用デフォルト値。本番では使用しない（`build_config()` を使う） |
| `CliValueSources` struct | pub | CLI で明示指定されたフラグを保持する補助構造体。`analysis_mode`, `log_format`, `llm_provider`, `ollama_bench_format`, `chat_memory`, `chat_guard`, `debate`, `chat_mode`, `lang` の9フィールドを持つ。`from_arg_matches()` で clap `ValueSource::CommandLine` から構築 |
| `build_config()` | pub | `Args` を受け取り `Config` を返す互換API。CLIの明示指定判定が必要な場合は `build_config_from_arg_matches()` を使う |
| `build_config_from_arg_matches()` | pub | clap `ArgMatches` の `ValueSource` に基づき `CliValueSources` を構築して `build_config_with_value_sources()` に委譲する |
| `build_config_with_value_sources()` | pub | `CliValueSources` を参照して CLI 明示値・env・デフォルトを正確にマージし `Config` を構築する中心実装 |
| `resolve_threshold_f64()` | private | `buy_rsi` / `sell_rsi` / `macd_diff_*` の「CLI > env > デフォルト」解決を共通化したヘルパー |
| `get_bool_env()` | private | 指定キーの環境変数を `"true"` / `"false"` として読み込む |
| `get_f64_from_args_or_env()` | private | CLI引数がデフォルト値と同じであれば環境変数から読み込む。0.5〜3.0の範囲バリデーション付き（weight系専用） |
| `get_usize_from_args_or_env()` | private | CLI引数がデフォルト値と同じであれば環境変数から読み込む（period系） |
| `get_usize_env()` | private | 環境変数から `usize` を取得する。失敗時はデフォルト値を返す |
| `sanitize_percent()` | private | 0.0〜100.0 の範囲クランプ（`bb_bandwidth_squeeze_pct` 専用） |
| `parse_stance()` | private | `"buyer"` / `"seller"` / その他 を `Stance` enum に変換 |
| `OllamaInstance` / `NotifyChannel` / `AlertRule` structs | pub | 複数Ollamaエンドポイント・チャット通知チャンネル（`NOTIFY_<n>_*`）・保存アラートルール（`ALERT_<n>_*`）の config 側モデル |
| `scan_ollama_instances()` / `scan_notify_channels()` / `scan_alert_rules()` | pub | `env_map` から `OLLAMA_<n>_*` / `NOTIFY_<n>_*` / `ALERT_<n>_*` グループを上記構造体へパースする |

**一般設定の優先順位**: CLI明示値 > xoksa.env（env_map） > システム環境変数 > デフォルト値。APIキーは env_map に含めず、各送信関数が `resolve_api_key()` でオンデマンド取得する（xoksa.env 優先、未設定時はOSキーチェーンにフォールバック）。

---

### `src/market.rs`

市場データAPIから株価データを取得する。

| 型・関数 | 公開 | 概要 |
|----------|------|------|
| `MarketData` struct | pub | 1本足のOHLCVデータ。`date`, `datetime`, `timestamp`, `timezone`, `high`, `low`, `close`, `volume`, `name` を持つ。確定足は書き換えない。日本株の**分足**では、リアルタイム観測値が属する足の終値（および必要なら高値・安値の範囲）を `apply_realtime_intraday_observation()` が更新する |
| `MarketLatestObservation` struct | pub | データソース由来の最新取得価格のみを保持する（`price: f64`）。時刻・タイムゾーン系フィールドは `MarketDataSnapshot` にある |
| `MarketDataSnapshot` struct | pub | `bars`、`latest_observation`（価格のみ）、`timezone`、`market_data_latest_time`、`market_data_latest_timestamp`、`analyzed_at` をまとめた取得結果。時刻・タイムゾーンは価格の有無に関わらず常に保持する |
| `YahooFinancePriceFetcher` struct | pub | `PriceFetcher` trait の本番実装（主プロバイダ）。`query2.finance.yahoo.com` から取得し、そのホストの再試行を使い切ると代替ホスト `query1.finance.yahoo.com` を再試行する（同一データ＝純粋な堅牢化）。`fetch_market_data_snapshot_for_mode()` に委譲 |
| `StooqPriceFetcher` struct | pub | フォールバックプロバイダ（明確に別ベンダー）。日足のみの無料フィード：日足は提供し、分足/週足/月足は日足で代用せずエラーを返す。数値が Yahoo とわずかに異なり得るため、採用したスナップショットはラベル表示される |
| `PriceFetcherKind::Failover` | pub | Yahoo（主）と Stooq（代替）を合成する `PriceFetcherKind` バリアント：Yahoo 取得が成功すれば常にそちらが勝ち、Stooq は Yahoo が本当に失敗した場合のみ使う。`build_price_fetcher()` が構築する。スナップショットをプロバイダ間で継ぎ接ぎしない（SOT）。**両方**失敗した場合は主プロバイダの理由を見出しにし、代替の結果を後ろに添える——未知のティッカーを知っているのは主プロバイダの側であり、これを捨てていたために打ち間違いに対して代替の CSV の不満が報告されていた |
| `stooq_symbol()` | private | ティッカーを Stooq のシンボル規約に変換する（JP/US サフィックス処理など） |
| `parse_stooq_csv()` | private | Stooq の日足 CSV レスポンスをバーにパースする。非有限値は境界で除外。HTML が返った場合（一部ネットワークでの anti-bot チャレンジ）は、壊れた CSV ではなく「代替プロバイダが遮断された」として報告し、そのページをメッセージに引用しない |
| `bounded_provider_text()` | private | プロバイダ由来の非信頼テキストを、メッセージやログに載る前に長さを切り、制御文字を空白に置き換える。応答が、自分が印字される行を書き換えたり、メッセージを占有したりできないようにする |
| `yahoo_chart_error_reason()` | private | Yahoo が非 2xx ステータスと**一緒に**返す JSON 本文（`chart.error.code`／`.description`）から理由を取り出す。未知のシンボルを、素の「request failed」ではなくそのように報告するため。その形でない本文なら `None` |
| `fetch_stooq_snapshot()` | private | 非同期。Stooq から `MarketDataSnapshot` を取得・構築する（日足のみ） |
| `fetch_market_data_for_mode()` | pub | 非同期。`AnalysisMode` に応じて日足・分足短期・週足・月足データを取得する |
| `fetch_market_data_snapshot_for_mode()` | pub | 非同期。分析足データと最新取得価格メタ情報を分離して取得する。日本株ではその後、リアルタイム気配（下記）を最新価格・データ最新時刻に重ねる。ベストエフォートで、失敗時はチャートAPIの値を保持する |
| `trim_trailing_empty_bars()` | private | 末尾の出来高ゼロ足を落とす。同一バケットのリアルタイム観測を統合した**後**に走るため、残るのは引け後のスナップショット足（例：15:15 の実引け後に付く 15:30 足）であり、場中のティックではない |
| `parse_yahoo_japan_realtime_quote()` | private | Yahoo!ファイナンス日本版のサーバーレンダリング済み気配ページから、東証のリアルタイム価格と HH:MM を抽出する。国際版チャートAPIは日本株を遅延扱いにするが、このページは現在値。クラス名の接尾辞はビルドハッシュのため、安定した接頭辞だけで照合する |
| `fetch_yahoo_japan_realtime_quote()` | private | 非同期。上記ページを取得してパーサに委譲する。ページ・ネットワーク・パースのいずれかが失敗すれば `None` を返し、チャートAPIの値を保持する |
| `apply_realtime_intraday_observation()` | private | リアルタイムの約定を現在の分足バケットに反映する。既存足があれば終値を更新し、範囲外のときだけ高値・安値を広げる。無ければ H=L=C=価格 で形成中の足を追加する。**出来高は創作しない**（新規足は出来高を持たない）。分足以外では即座に return するため、日足・週足・月足の系列はデータソースが返したまま |

**取得条件（日足）**: `interval=1d`, `range=3mo`
**取得条件（5分足）**: `interval=5m`, `range=5d`
**取得条件（15分足）**: `interval=15m`, `range=1mo`
**取得条件（30分足）**: `interval=30m`, `range=1mo`
**取得条件（週足）**: `interval=1wk`, `range=2y`
**取得条件（月足）**: `interval=1mo`, `range=10y`

---

### `src/technical/mod.rs`

`technical` サブモジュールの公開 API 再エクスポート定義。ロジックは持たない。

再エクスポート対象:
- `composite::calculate_final_score_snapshot`
- `composite::calculate_score_gauge`
- `indicators::build_basic_technical_entry`
- `indicators::evaluate_all_selected_extensions`
- `indicators::get_extension_evaluator`
- `indicators::evaluate_all_selected_extensions_with_report`
- `indicators::ExtensionEvaluationFailure`, `ExtensionEvaluationReport`
- `types::AnalysisResult`, `TechnicalDataEntry`, `TechnicalDataGuard`

---

### `src/technical/types.rs`

テクニカル分析で使われる全データ構造の定義。ロジックは持たない。

| 型 | 公開 | 概要 |
|----|------|------|
| `AnalysisResult` struct | pub | 1指標の分析結果。`indicator_name`, `description`（表示用文字列Vec）, `score`（f64）を持つ |
| `FinalScoreSnapshot` struct | pub | 全指標の合算スコアスナップショット。`total_score`, `total_weight`, `score_ratio`（-1〜+1）を持つ |
| `TechnicalDataEntry` struct | pub | 全テクニカルデータを平坦に保持する構造体。ティッカー・OHLCV・各指標値・各スコアを含む約50フィールド |
| `TechnicalDataGuard` struct | pub | `TechnicalDataEntry` の安全なラッパー。包んでいるエントリは**非公開**で、書き込みは専用メソッド経由のみ許可。内部への可変参照を返す API も無い |
| `TechnicalDataGuard::new()` | pub | ガード付き構造体を初期化。ティッカーと日付を指定 |
| `TechnicalDataGuard::get_*()` / `set_*()` | pub | 各フィールドへのアクセサ。`get_ema_score()`, `set_rsi()` 等多数。`set_currency()` は市場データが報告した建値通貨を記録する |
| `TechnicalDataGuard::is_computed()` / `computed_indicator()` | pub | その指標が**値を生成したか**と、生成していればその値。対象は素の `f64` で保持する指標（`rsi`・`macd`・`macd_signal`・`ema_s/l`・`sma_s/l`・`bb_u/l`・`pct_b`）で、これらは `0.0` と「値なし」が区別できないため。セッターは有限値で呼ばれたときだけキーを記録し、後の再計算に失敗したときは値と記録の両方を消す。したがって生成されなかった値——未計算・計算失敗のいずれも——は**欠損**となり、正常に計算された `0.0` は存在する。既に `Option` で保持している指標（ADX・%K・VWAP・ROC・フィボナッチ・一目・出来高）は型で区別を持ち、それぞれの getter で読む |

---

### `src/technical/indicators.rs`

全テクニカル指標の計算ロジックと `TechnicalDataGuard` への格納を担当する。

| 関数 | 公開 | 概要 |
|------|------|------|
| `build_basic_technical_entry()` | pub | `MarketData` の配列から MACD・RSI・基本スコアを計算して `TechnicalDataGuard` を構築して返す。エイリアス名の解決（`config.no_alias` 参照）もここで行う |
| `evaluate_all_selected_extensions()` | pub | `config.enabled_extensions` の指標を順に評価し `TechnicalDataGuard` にスコアを格納する |
| `get_extension_evaluator()` | pub | `ExtensionIndicator` 列挙値から対応する `ExtensionEvaluator`（関数ポインタ）を返す |
| `calculate_period_vwap()` | pub | 直近 `period` 本のOHLCVデータから VWAP を計算する。`volume=None` のデータがあれば `Err` を返す |
| `HardcodedInfo` struct | private | ハードコードされた正式名（`formal_name`）1 つだけを保持する |
| `resolve_hardcoded_info()` | private | QQQ / SPY / ACWI について、そのファンド自身の正式名を返す。CSV エイリアス **と** プロバイダ報告名の後にしか使わないので、情報源が述べた名前を上書きしない。連動先の指数を括弧で添える書き方（ファンドが指数そのものだと読めた）は廃止し、到達しない `FANG+` の項目も削除した |
| `evaluate_and_store_ema()` | private | EMA計算 → スコア判定 → ガードへ格納 |
| `evaluate_and_store_sma()` | private | SMA計算 → スコア判定 → ガードへ格納 |
| `evaluate_and_store_adx()` | private | ADX計算 → スコア判定 → ガードへ格納 |
| `evaluate_and_store_roc()` | private | ROC計算 → スコア判定 → ガードへ格納 |
| `evaluate_and_store_stochastics()` | private | ストキャスティクス %K/%D 計算 → スコア判定 → ガードへ格納 |
| `evaluate_and_store_bollinger()` | private | ボリンジャーバンド・%B・Bandwidth 計算 → スコア判定 → ガードへ格納 |
| `evaluate_and_store_fibonacci()` | private | 高値・安値からフィボナッチ水準を計算 → スコア判定 → ガードへ格納 |
| `evaluate_and_store_vwap()` | private | 分足モード: 指標計算最終足と同日セッション全足でセッションVWAP計算。日足モード: `calculate_period_vwap()` で直近N本ローリングVWAP計算 → スコア判定 → ガードへ格納 |
| `evaluate_and_store_ichimoku()` | private | 一目均衡表（転換線・基準線）計算 → スコア判定 → ガードへ格納 |

**`no_alias` の影響範囲**:  
`build_basic_technical_entry()` 内で `alias_name_opt` を解決する際、`config.no_alias=true` の場合は `ticker_name_map` を参照せず `None` を返す。これにより銘柄名ヘッダーがエイリアスではなくティッカーシンボルになる。

---

### `src/technical/composite.rs`

全指標スコアを合算して最終スコアと視覚ゲージを生成する。

| 関数 | 公開 | 概要 |
|------|------|------|
| `calculate_final_score_snapshot()` | pub | `Config` の重みを参照して全指標スコアを加重合算し `FinalScoreSnapshot` を返す |
| `calculate_final_score()` | private | 重み付き合算スコア（f64）を返す内部実装 |
| `calculate_score_gauge()` | pub | スコア比率からテキスト形式のゲージ文字列を生成する |
| `get_extension_evaluator()` | pub | アダプタ: 指標名文字列（例: `"ema"`）を `ExtensionIndicator` にパースし `indicators::get_extension_evaluator()` に委譲する。未知の文字列は `None` を返す |

**スコア計算式**:
$$\text{score\_ratio} = \frac{\sum(\text{score}_i \times \text{weight}_i)}{2 \times \sum(\text{weight}_i)}$$

---

### `src/news.rs`

Brave Search API によるニュース取得とクエリ生成を担当する。

| 型・関数 | 公開 | 概要 |
|----------|------|------|
| `Article` struct | pub | ニュース記事1件。`title`, `url`, `published_at` を持つ |
| `BraveArticleFetcher` struct | pub | `ArticleFetcher` trait の本番実装。`fetch_articles_from_brave()` に委譲 |
| `news_flow_controller()` | pub | 非同期。`fetcher: &impl ArticleFetcher` を受け取る。APIキーをオンデマンド取得 → `run_news_once()` → `compose_news_lines()` を統括する |
| `news_flow_controller_with_key()` | pub(crate) | 非同期。APIキーを注入できる内部実装。テストで `xoksa.env` に依存しないために使用 |
| `compose_news_lines()` | pub | 取得した記事のタイトル・URL・公開時刻メタ情報から表示用文字列 `Vec<String>` を生成する |
| `news_filter_criteria()` | pub | LLM プロンプト用のタイトル単位ニュース確認条件文字列を返す。非チャット分析プロンプトとチャットモード `/show nf` で共用 |
| `run_news_once()` | private | 非同期。`fetcher: &impl ArticleFetcher` を受け取る。JP/US を判別してクエリを構築しfetcherを呼ぶ。`config.no_alias` を参照してクエリ文字列を切り替える |
| `build_news_query_line_for_log()` | private | ログ記録用のクエリ文字列を生成する。`config.no_alias` を参照 |
| `build_news_query_jp()` | private | JP向けクエリ文字列を構築（銘柄名 OR 証券コード OR ティッカー） |
| `build_news_query_us()` | private | US向けクエリ文字列を構築。`company_name=None` の場合はティッカーのみ |
| `news_locale_for_ticker()` | private | ティッカーが `.T` で終わるか否かで `("JP", "jp", "ja-JP")` または `("US", "en", "en-US")` を返す |
| `fetch_articles_from_brave()` | private | 非同期。Brave Search API に HTTPS リクエストを送り記事リストを返す。APIキーは `Zeroizing` で管理 |
| `normalize_url()` | private | 取得URLの正規化（クエリパラメータ除去・末尾スラッシュ正規化） |
| `print_lines_to_terminal()` | private | 文字列 Vec を標準出力に1行ずつ出力する |

**`no_alias` の影響範囲**:
- JP: `no_alias=true` → `guard.get_name()` の代わりに `guard.get_ticker()` をクエリの第1引数にする
- US: `no_alias=true` → `company_name=None` としてティッカーのみでクエリを構築する

---

### `src/render.rs`

テクニカル分析結果のターミナル表示ロジックを担当する。指標ごとの表示関数は `render_indicators.rs` に分離済み。

| 型・関数 | 公開 | 概要 |
|----------|------|------|
| `render_ranked()` | pub | 基本指標と有効な全拡張指標を、設定した重み順（高い順・同値はランダム）でフラットな `Vec<AnalysisResult>` にして返す。カテゴリヘッダーなし。ターミナル表示と LLM prompt の双方に供給する表示順の単一ソース |
| `Renderer` trait | pub | レンダラーの抽象インターフェース。`render_to_terminal()`, `compose_final_score_lines()` を定義 |
| `TerminalRenderer` struct | pub | `Renderer` の標準実装。全メソッドを委譲関数に転送する |
| `technical_render_to_terminal()` | pub | 基本分析 + 拡張分析 + 最終スコアの全表示を実行する |
| `render_basic()` | pub | MACD/RSI に基づく基本スコアを計算し `AnalysisResult` を返す |
| `compose_final_score_lines_stance()` | pub | スタンス（buyer/seller/holder）に応じた最終スコア文字列 Vec とゲージを生成する |
| `collect_main_info_lines()` | pub | ヘッダー情報行（銘柄名・各時刻・最新取得価格・指標を計算した足…）を `Vec<String>` で返す — `display_main_info` の収集専用の対応物で、チャット/文脈テキスト構築時に使う |
| `render_final_score()` | private | 合算スコアとゲージをターミナルに出力する |
| `display_main_info()` | private | 銘柄名・分析時刻・データの最新時刻・最新価格が入る足・指標を計算した足・最新取得価格などのヘッダー情報をターミナルに出力する。4つの時刻ラベルは `utils::freshness_labels()` が単一ソース（CLI表示とLLM文脈で共通） |
| `display_analysis_result()` | private | `AnalysisResult` の `description` 行をターミナルに出力する |
| `render_unipolar_gauge_rtl()` | private | 単極ゲージ（右→左方向）の文字列を生成する |
| `render_bipolar_gauge_lr()` | private | 双極ゲージ（左右対称）の文字列を生成する |
| `stance_caption()` | private | スタンス enum をゲージキャプション文字列に変換する |
---

### `src/render_indicators.rs`

9つの拡張テクニカル指標の表示ロジック。`render.rs` から `pub(crate)` で呼ばれる。

| 型・関数 | 概要 |
|----------|------|
| `render_ema()` | EMA スコアの表示行 Vec を生成 |
| `render_sma()` | SMA スコアの表示行 Vec を生成 |
| `render_adx()` | ADX スコアの表示行 Vec を生成 |
| `render_roc()` | ROC スコアの表示行 Vec を生成 |
| `render_stochastics()` | ストキャスティクス スコアの表示行 Vec を生成 |
| `render_bollinger()` | ボリンジャーバンド スコアの表示行 Vec を生成 |
| `render_fibonacci()` | フィボナッチ スコアの表示行 Vec を生成 |
| `render_vwap()` | VWAP スコアの表示行 Vec を生成 |
| `render_ichimoku()` | 一目均衡表 スコアの表示行 Vec を生成 |

---

### `src/fundamental.rs`

ファンダメンタル補助情報（財務指標・配当・株数等）の取得・整形を担当する。`--fundamental` フラグ指定時のみ呼ばれる。

| 型・関数 | 公開 | 概要 |
|----------|------|------|
| `FundamentalData` struct | pub | 確定したファンダメンタル情報。**全フィールドが非公開**で、読み出しは getter 経由。書き込む経路はいずれも値を検証するため、未検査の数値をこの構造体に置く手段は無い。`market`, `data_source`, `currency`, `amount_unit`（大数値の単位。v2→`"JPY"` / SEC→`"USD"`）, `fiscal_period`, `reported_date`, `revenue`, `operating_income`, `net_income`, `eps`, `bps`, `equity`, `shares_outstanding`, `per`, `pbr`, `roe`, `dividend`, `dividend_is_forecast`, `trading_unit`, `next_fiscal_year_end` と、比率の元になった価格を保持する |
| `FundamentalInputs` struct | pub | プロバイダ経路が集める検証前の取得途中の値。派生比率は持たない（比率は確定データの内側で算出する） |
| `FundamentalData::build()` | pub | プロバイダの取得結果からの初期構築を集約する経路。プロバイダ経路はすべてここを通る（`Default` は数値を持たない空の値を作るだけ。BPS と価格の後からの更新は、検証付きの公開メソッド `set_bps` / `recompute_derived` を通る）。`FundamentalInputs` と `latest_price` を受け取り、非有限値を排除し（正常なゼロと意味のある負値は保持）、PER・PBR・ROE を検証済み入力から**一度だけ**導出する（プロバイダ経路ごとには計算しない）。ゼロ除算とオーバーフローは無限大を出さず未定義とする |
| `FundamentalData::recompute_derived()` / `set_bps()` | pub | 派生比率を入力および元になった価格と整合させ、比率だけが古いまま残らないようにする |
| `FundamentalData::operating_margin_pct()` / `roe_pct()` / `lot_dividend()` | pub | 表示と確定データのための派生値。非有限値ではなく `None` を返すので、桁あふれした比率が `inf%` になることはない |
| `FundamentalMarket` enum | pub | ティッカーの市場分類（`Japan` / `Us` / `Unsupported`）。ファンダメンタル取得の振り分けに使う |
| `detect_market()` | pub | ティッカーの `FundamentalMarket` を返す（`.T`/4桁 → `Japan`、それ以外 → `Us`） |
| `fetch_fundamental_data()` | pub | 非同期。ティッカーが日本株（`.T`/4桁）なら `fetch_jquants()`、それ以外なら `fetch_sec_edgar()` を呼ぶ。`latest_price` は PER/PBR 計算に使用 |
| `render_fundamental_display()` | pub | `FundamentalData` と `lang: &str` を受け取りターミナル表示用 `Vec<String>` に整形する。数値はカンマ区切り・通貨プレフィックス付き。配当予想は EN: `(forecast)` / JA: `（予想）` を付与 |
| `format_fundamental_for_llm()` | pub | `FundamentalData` を LLM プロンプト用テキストブロックに整形する |
| `fetch_jquants()` | private | 非同期。J-Quants API からファンダメンタル情報を取得する。v2（JQUANTS_API_KEY）と v1（トークンチェーン）の2パスアーキテクチャ。v2 は `/v2/fins/summary` と `/v2/equities/master` を使用 |
| `jquants_bearer_token()` | private | 非同期。`JQUANTS_API_KEY`（v2のみ）を解決。認証情報は `Zeroizing<String>` で管理し使用後即廃棄 |
| `fetch_sec_edgar()` | private | 非同期。SEC EDGAR companyfacts API からファンダメンタル情報を取得する。CIK は ticker → EDGAR company_tickers.json で解決。BRK.B 等のクラス株はティッカー変形（`.` → `-` / 削除）でフォールバック検索 |
| `pick_best_xbrl()` | private | `form` が 10-K/10-Q のエントリを対象に、`annual_only=true` なら `fp=="FY"` を優先し、`end` → `filed` 日付の降順で最新を1件返す。SEC データ選択の安定性を担保する |
| `fmt_currency_auto()` | private | `Option<f64>` を通貨プレフィックス付き・カンマ区切り文字列に整形する（None → "N/A"） |

**J-Quants（v2のみ）**:
- `JQUANTS_API_KEY` → `x-api-key` ヘッダー → `/v2/fins/summary` → `json["data"]` → `Sales/OP/NP/EPS/BPS/Eq/DivAnn/FDivAnn/NxFDivAnn/DiscDate/CurPerType/CurFYEn/ShOutFY/NxtFYEn`
- 配当フォールバック: `DivAnn` → `FDivAnn` → `NxFDivAnn`（予想値の場合 `dividend_is_forecast=true`）
- 最低売買単位: `/v2/equities/master` → `json["data"][0]["TradingUnit"]`

**SEC EDGAR フィールドマッピング**:
- Revenue: `Revenues` / `RevenueFromContractWithCustomerExcludingAssessedTax` / `SalesRevenueNet` (`annual_only=true`)
- OperatingIncome: `OperatingIncomeLoss` (`annual_only=true`)
- NetIncome: `NetIncomeLoss` (`annual_only=true`)
- EPS: `EarningsPerShareBasic` / `EarningsPerShareDiluted` (`annual_only=true`)
- Equity: `StockholdersEquity` (`annual_only=false`)
- Shares: `CommonStockSharesOutstanding` (最新 end 日付)
- Dividend: `CommonStockDividendsPerShareDeclared` / `...Paid` (`annual_only=false`)

---

### `src/app.rs`

拡張指標スコアを `TechnicalDataGuard` から読み出して `AnalysisResult` に詰め直し、ターミナル表示・ログ保存を呼び出すパイプライン。ファンダメンタル情報を受け取りターミナルに続けて表示する。

| 関数 | 公開 | 概要 |
|------|------|------|
| `build_analyzed_guard()` | pub | 非同期。分析パイプライン（market → technical → score）を実行し、値を詰めた `TechnicalDataGuard` を返す — CLI（`main.rs`）とサーバ（`server/api.rs`・`server/monitor.rs`）が共用する唯一の SOT エントリポイント |
| `run_output_pipeline()` | pub | `config.enabled_extensions` を走査して各スコアを `Vec<AnalysisResult>` に積み、`render::technical_render_to_terminal()` と `output::save_technical_log()` を呼ぶ。`fundamental_data: Option<&FundamentalData>` を受け取り、Some の場合は `fundamental::render_fundamental_display()` の出力をターミナルに表示する |
| `collect_display_lines()` | pub | 現在の分析結果の全表示行を `Vec<String>` で返す（ターミナル表示と同内容、実際には出力しない）。チャットベースコンテキスト構築時に使用 |

---

### `src/output.rs`

テクニカルログのCSV/JSON出力を担当する。trait ベースで抽象化されている。

| 型・関数 | 公開 | 概要 |
|----------|------|------|
| `TechnicalLogFormatter` trait | pub | `csv_header()`, `csv_row()`, `json_row()` の3メソッドを定義するフォーマッタ抽象 |
| `TechnicalLogWriter` trait | pub | `write()` メソッドを定義するライタ抽象 |
| `DefaultTechnicalLogFormatter` struct | pub | `TechnicalLogFormatter` の標準実装。有効な拡張指標に応じてCSVヘッダー・行を動的生成 |
| `FileOrStdoutTechnicalLogWriter` struct | pub | `TechnicalLogWriter` の標準実装。`--stdout-log` フラグで出力先を切り替える |
| `generate_csv_header()` | pub | `--show-log-header` フラグ時に現在の設定に基づくCSVヘッダーを標準出力に出力して終了する |
| `save_technical_log()` | pub | ログ保存のエントリポイント。フォーマッタとライタを組み立てて `write()` を呼ぶ |
| `technical_json_value()` | pub | guard ＋スコアスナップショットからテクニカルブロックを `serde_json::Value` として構築する — JSON の `json_row` とサーバの JSON API が共有する単一ソース |
---

### `src/prompt.rs`

LLMに送るプロンプト文字列の組み立てと送信フローを担当する。

| 型・関数 | 公開 | 概要 |
|----------|------|------|
| `PromptRenderer` trait | pub | `build_prompt()` メソッドを定義するプロンプトビルダ抽象 |
| `DefaultPromptRenderer` struct | pub | `PromptRenderer` の標準実装。`Renderer` を内包し、基本分析・拡張分析・スコア行を `llm::compose_llm_prompt_lines()` に渡してプロンプトを組み立てる |
| `build_analysis_prompt()` | pub | `DefaultPromptRenderer` を構築して `build_prompt()` を呼ぶ便利ラッパー。config・guard・ニュース・任意のファンダメンタルデータを受け取り、組み立て済みプロンプト文字列を返す。`main.rs` が分析プロンプトとチャットベースコンテキストの両方に使用する。（送信自体は `main.rs` にインライン：`debug_prompt` 時に `llm::save_prompt_to_file`、`!no_llm && !silent` 時に `LlmDispatchSender.send_prompt`。） |

---

### `src/llm.rs`

LLM API（OpenAI / Gemini / Claude）への送信とプロンプト文字列の組み立てを担当する。

| 型・関数 | 公開 | 概要 |
|----------|------|------|
| `ChatTokenUsage` struct | pub | LLM API呼び出しで返される `input`・`output` トークン数を保持する |
| `LlmDispatchSender` struct | pub | `PromptSender` trait の本番実装。`config.llm_provider` に応じて `send_openai_prompt` / `send_gemini_prompt` / `send_claude_prompt` / `send_ollama_prompt` に振り分ける |
| `OpenAiPromptSender` struct | pub | `PromptSender` の互換実装。`send_openai_prompt()` に直接委譲（本番では `LlmDispatchSender` を使用） |
| `news_triage_directive()` | pub | Tier A/B/C のタイトル+URLニュース確認優先度仕訳指示文字列を返す。`compose_llm_prompt_lines()`（非チャット）とチャットモード `/show nf` で共用し、Markdown表ではなくベタ打ちの箇条書きを要求する |
| `compose_llm_prompt_lines()` | pub | スタンス・設定フラグ・基本分析行・拡張分析行・スコア行・ニュース記事リストを統合してプロンプト行 Vec を生成する |
| `save_prompt_to_file()` | pub | `debug_prompt_<nanosec>.txt` にプロンプトを書き出す（`--debug-prompt` 時、`create_new` で衝突回避） |
| `send_openai_prompt()` | pub | 非同期。OpenAI Chat Completions API (`/v1/chat/completions`) にプロンプトを送信してレスポンスをストリーム出力する。APIキーは `Zeroizing` で管理 |
| `final_language_directive()` | pub | 各プロンプト末尾に付ける出力言語の固定指示（`en`/`ja`）を返し、LLM が設定言語で回答するようにする |
| `alert_explain_note()` | pub | 非同期。チャット通知アラートに付ける任意の1行解説を生成する。出力は §1 の出力整合ガードを通す |
| `send_chat_turn()` | pub | 非同期。設定されたプロバイダーへ1回分のチャット用プロンプトを送信してレスポンス文字列を返す。チャットモードのスラッシュコマンド（`/show`・`/compare` 等）が使用 |
| `send_chat_turn_with_usage()` | pub | 非同期。`send_chat_turn()` と同じだが `ChatTokenUsage` もあわせて返す |
| `send_gemini_prompt()` | private | 非同期。Gemini API (`generativelanguage.googleapis.com/v1beta`) にプロンプトを送信。429/503 に対して `Retry-After` ヘッダーを使ったリトライ（最大3回）。thinking モデルの `"thought": true` parts を除外してレスポンス本文を抽出。APIキーは `Zeroizing` で管理 |
| `send_claude_prompt()` | private | 非同期。Anthropic Messages API (`api.anthropic.com/v1/messages`) にプロンプトを送信。429/529/503 にリトライ。400 レスポンスに "temperature" が含まれる場合は `temperature` なしで再送する動的フォールバックを持つ。APIキーは `Zeroizing` で管理 |
| `send_ollama_prompt()` / `run_ollama_benchmark()` | private | 非同期。Ollama `/api/chat` エンドポイント（`OLLAMA_HOST:OLLAMA_PORT`）に stream=false で送信。APIキー不要。reqwest クライアントに `.no_proxy()` を付与しプロキシを常時無効化。temperature/top_p/top_k/repeat_penalty/keep_alive は設定されている場合のみ送出。num_ctx は未設定時 `32768`、num_predict は未設定時 `8192`、seed は未設定時 `42` を送出する。thinking モードは `OLLAMA_THINK` / `--ollama-think` が指定された場合のみ送出し、モデル名による自動切り替えは行わない。Ollama が thinking 非対応を400応答で返した場合は、モデル名ではなくAPI応答に基づき `think` を送信せず一度だけ再試行する。SOT の system メッセージ——学習済み知識による銘柄事実の補完、入力にないニュース・数値・日付・固有名詞の追加、単位変換・丸め・再計算・独自価格目標算出の抑制——は**全プロバイダで共通**（`sot_system_prompt` を openai / gemini / claude / ollama の各ビルダが同じく送出する）。プロバイダによって基準が変わることはない。応答本文のすべての数値を、プロンプトと共に渡された確定データ（`integrity::ConfirmedFactSet`）と照合する——同じ銘柄・足・指標・単位・通貨・符号・時点——ので、別の値からの転用、未算出の指標の読み取り値、価格文脈での裸の未提示レンジ、独自に導いた売買水準、入力にない比較、逆方向のVWAP説明を検出する。検出内容を表示し、問題のある文・行だけを検討材料から除外する（`--no-ollama-guard` / `OLLAMA_NO_GUARD=true` で無効化可能）。検査は言語ごとに分かれておらず、各検査が日本語表現と英語表現を同居させている。`done_reason=length` で生成が打ち切られた場合は誤表示を避けるため LLM 本文を抑止する（`--no-ollama-guard` でも維持）。`--debug-ollama` / `OLLAMA_DEBUG=true` ではプロンプト本文を出さず、送信オプションと応答メタ情報のみ表示する。`--llm-benchmark` では同一プロンプトを複数Ollamaモデルに送信し、本文を表示せず `done_reason`、ctx使用率、tokens/sec、thinking量、整合性チェック件数などを table/csv/json で出力する。接続失敗・タイムアウト・モデル未存在を個別にエラー表示 |

---

### `src/integrity.rs`

LLM 出力整合性ガードが照合する確定データの基準。確定値を保持する構造体（`TechnicalDataGuard`・`FundamentalData`）**だけ**から構築し、データとして検査側へ渡す——プロンプトには会話や他モデルの発言も載るため、いかなる文字列も出所として読まない。検出規則そのものの正典は [security-design.md §1](./security-design.md)。

| 型・関数 | 公開 | 概要 |
|----------|------|------|
| `SymbolFacts` struct | pub | 1銘柄の確定値。銘柄コード・銘柄名・通貨・対象足の年、および（複数足を読み込んだ場合のみ）算出した足を伴う |
| `SymbolFacts::from_sources()` | pub | 上記を guard・`Config`・任意の `FundamentalData` から構築する。guard が**値を生成したと記録した**指標だけが確定値になるので、計算に失敗した指標は `0.0` として確定するのではなく欠損する。各値は自分を区別するものを伴う——脚を持つ指標（EMA・SMA・バンド）は計算期間、ファンダメンタルの値は自身の会計年度 |
| `SymbolFacts::with_timeframe()` / `carry_over_fundamentals()` | pub | その値が属する足を名乗らせる／再取得でファンダメンタルを取り直さなかった場合に前回の値を**すべて**引き継ぐ（単元配当・単元株数を含む）。セッションが表示し続けるブロック全体が確定データで覆われる |
| `ConfirmedFactSet` struct | pub | 1リクエストの確定データ。銘柄ごとに1件（複数足を読み込んだ場合は足ごとに1件）。`facts_for()` / `facts_from_parts()` で構築する |
| `ConfirmedFactSet::verify()` / `verify_with()` | pub | 書かれた数値1つを、その**主張**が帰属させている確定値と照合する（銘柄・足・指標・単位・通貨・桁・符号・時点）。`verify_with` は呼び出し側がその文について作った `Scan` を受け取る |
| `ConfirmedFactSet::compare()` / `single_value()` | pub | 向き（例：VWAP と終値）を確定値から読む。プロンプトの表記で判定が変わらないようにするため |
| `ClaimVerdict` enum | pub | 主張が通らなかった理由——別指標・別銘柄・未算出の指標・単位/通貨・時点・確定データに対応が無い。`is_reportable()` が報告対象を決める |
| `Scan` struct | pub | 文中の数値・日付・年・期間指定・足・ティッカー、および文が述べた指標の計算期間（各期間は、それが書かれた指標名の範囲つき）がどこにあるか。**1文につき1回**構築し、主張の分割と全帰属判定が共有するので、表記（桁区切り・日付の綴り・期間を括弧に書くか名前の前に置くか）で判定が変わらない |
| `written_numbers()` | pub | すべての数値を、符号・倍率・単位・通貨・役割（観測値／期間パラメータ／日付）とともに読む。ティッカーの数字は読み取り値ではなく、理解できない接尾辞は読み捨てない |
| `sentence_bounds()` / `sentence_end()` | pub | ガードと共有する文分割。英語の `.` は空白か文末が続くときだけ文を終えるので、小数点もティッカーのドットも文を切らない。サニタイザも `sentence_end` を使うため、散文では判定する範囲と除去する範囲が一致する。表の行・箇条書き・1 文しかない行は、代わりに行ごと除去する |

---

### `src/chat/`

インタラクティブなチャットループとスラッシュコマンド処理（モジュールディレクトリとして構成）。`--chat` 指定時に `main.rs` から呼ばれる。

| サブモジュール | 概要 |
|------|------|
| `chat/mod.rs` | `ChatSession` 定義・チャット共通の型・小さな自由関数ヘルパ |
| `chat/exec.rs` | 統一スラッシュコマンドディスパッチ — `ChatSession::execute`（CLI と Web で共有）— ＋ Web セッション補助（`run_web_command`）と、クライアント単位の LLM 選択ストア（`apply_client_llm_selection`・`store_client_llm_selection`・`client_llm_label`） |
| `chat/prompt.rs` | `impl ChatSession` のプロンプト/コンテキスト構築（チャット・`/forum crit`・forum合議/Chair・`/basic`） |
| `chat/run.rs` | `run_chat_loop`：対話REPLとスラッシュコマンド振り分け |
| `chat/guard.rs` | 投資助言制約テキスト・将来予測ポリシー・`/forum crit` タスク文（純粋const＋セレクタ） |
| `chat/help.rs` | `/help` 表示（純粋 `println!`） |
| `chat/council.rs` | Council/Board のマルチLLM合議（`handle_council_command`・`send_council_ask`・Facilitator解決） |
| `chat/debate.rs` | Debate Buffer・`/forum crit` 補助（`send_criticize_last_to_llm`・抜粋クリーニング） |
| `chat/report.rs` | token/context サイズのレポートとチャット状態表示（`/token`・`/status`） |
| `chat/llm.rs` | `/llm` プロバイダ切替・モデル解決・チャットターン送信 |
| `chat/ticker.rs` | 銘柄のロード/切替/リロードと自動リロード通知 |
| `chat/news_extra.rs` | `/nx` バッファ：追加ニュースの取得とインジェクト |
| `chat/tests.rs` | chat モジュールの `#[cfg(test)]` ユニットテスト |

| 型・関数 | 公開 | 概要 |
|----------|------|------|
| `run_chat_loop()` | pub | 非同期。チャットモードのメインエントリポイント。分析ベースコンテキスト・テクニカル/ファンダメンタル表示テキスト・ティッカーラベル・ニュース記事・ティッカー名前マップを受け取る。REPLループ・メモリ・Debate Buffer・`/forum crit` 専用SOT Coverageプロンプト・`/status` のチャット状態サマリ・`/token detail` の文脈サイズ内訳・自動リロードタイマー・スラッシュコマンド振り分けを管理する |

**内部で処理するスラッシュコマンド**:
`/help`, `/show`（t/f/n/nf）, `/basic`, `/mem`, `/set`, `/status`, `/forum`, `/reload`, `/auto`, `/sym`, `/llm`, `/mode`, `/nx`, `/depth, /scope`, `/token [detail]`, `/bye`

---

### `src/keystore.rs`

OSキーチェーンアクセス層。すべてのクラスA認証情報（APIキー・トークン）は `keyring` / `keyring-core` クレートを通じてサービス名 `"xoksa"` 配下に1キー1エントリで保管する。セキュアなキー取得・保存の唯一のゲートウェイとして機能し、呼び出し元は `Zeroizing<String>` で受け取ることでドロップ時のメモリゼロ化を保証する。

| 関数 | 公開 | 概要 |
|------|------|------|
| `get_key()` | pub | 名前でOSキーチェーンからキーを取得する。見つかった場合は `Ok(Some(key))`、未設定は `Ok(None)`、アクセス失敗は `Err` を返す。戻り値は `Result<Option<Zeroizing<String>>>` |
| `delete_key()` | pub | 名前でOSキーチェーンのキーを削除する。削除済みまたは存在しない場合は `Ok(())`、アクセス失敗時は `Err` を返す |
| `set_key()` | pub | 名前でOSキーチェーンにキーを保存する。キーリングアクセス失敗時は `Err` を返す |
| `resolve_key_presence()` | pub | キーが `xoksa.env` またはOSキーチェーンに存在するか確認する。`KeyPresence`（`Found(source)` / `NotFound` / `KeyringError`）を返し、アクセス障害と未登録を区別する |

---

### `src/setup.rs`

インタラクティブウィザード（`--init`）と環境ヘルスチェック（`--doctor`）。通常分析開始前に `bootstrap.rs` から呼ばれる。

| 関数 | 公開 | 概要 |
|------|------|------|
| `run_init()` | pub | 新しい `xoksa.env` ファイルを生成するインタラクティブウィザード。LLM・ニュース・ファンダメンタル・プロキシを独立して確認する。既存の `xoksa.env` がある場合は新ファイル書き込み直前に `xoksa.env.bak_NNN` へバックアップする。初期メッセージのローカライズ用に `lang_override` を受け取る |
| `run_update_key()` | pub | OSキーチェーンに保存された単一のAPIキーを更新するインタラクティブプロンプト。成功時は `true`、Ctrl-Cでキャンセル時は `false` を返す |
| `run_check_keys()` | pub | 管理対象9種のAPIキーの存在有無を（`xoksa.env` と OSキーチェーンの両方を確認して）値を表示せずに報告する |
| `run_doctor()` | pub | `resolve_key_presence`（`xoksa.env` と OSキーチェーンの両方を参照）でAPIキーの存在確認と、プロキシ・接続設定の確認結果をターミナルに表示する。終了せずに表示後に返る |
| `run_apply_config_cli()` | pub | `apply-config` サブコマンドの headless apply-setup：stdin から JSON 設定（クラスAキー含む）を読み、`--init` と同じロジックで env ファイル＋OSキーチェーンに書き込む。設定アプリ（`xoksa-setup`）が子プロセスの stdin 経由で駆動する |
| `run_ollama_models_cli()` | pub | `ollama-models` サブコマンドを支える：ローカル Ollama サーバ（host/port 引数）へ問い合わせ、モデル一覧を JSON で出力して設定アプリに渡す |

---

### `src/server/`

`xoksa serve --ui`（Web UI）のローカル HTTP サーバ。トランスポート／UI 層のみ — 分析・指標・ファンダメンタル・LLM・バックテストはネイティブに残り HTTP API 経由でのみ到達する。`main()` で分析 CLI パーサより前に先回り判定するため、既存 CLI／チャットは無変更。

| サブモジュール / 関数 | 公開 | 概要 |
|------|------|------|
| `server/mod.rs` | pub | `ServeArgs`（clap；`--ui`/`--port`/`--host`/`--web-dir`/`--private`/`--log-format`）・`run_serve_cli()` / `run_server()`（axum ルータ・bind・URL 表示・セキュリティヘッダ middleware）・自前静的配信（`read_asset`、パストラバーサル防御・ユニットテスト済・`.wasm` MIME・index.html は no-store）。`read_asset` はディスク上の `--web-dir`（既定は Leptos ビルド成果物 `webui-leptos/dist`）を優先し、無ければ**バイナリ埋め込みアセットにフォールバック**する——`embedded-ui` cargo フィーチャ（`webui-leptos/dist` を `include_dir!`）で組み込まれ、リリースバイナリは外部の `dist/` なしでダッシュボードを配信する。Rust/WASM フロントエンドのソースは `webui-leptos/`（Leptos、独立クレート、`trunk` でビルド。ネイティブ workspace／CI には含めない） |
| `server/api.rs` | pub(crate) | HTTP ハンドラ＋JSON モデル。状態：`health` / `config`。銘柄別：`symbol_summary`（`app::build_analyzed_guard`＋`collect_display_lines` で実パイプライン）・`symbol_news`・`symbol_chart`。分析：`analysis_multi_timeframe`。バックテスト：`backtest` / `list_rules` / `save_rule`（ルールはJSONファイル `~/.xoksa.strategies.json` に保存）。アラート：`alerts_list` / `alerts_add` / `alerts_delete` / `alerts_toggle` / `alerts_test`。LLM：`llm_options` / `llm_select`。設定：`set_lang`（`LANG` を書く。ループバック限定・`--private` では拒否）。チャット：`chat_stream`（SSE・確定分析に基づく実LLM・統一 `ChatSession::execute` ディスパッチ）/ `chat_commands`。`build_server_config` が `bootstrap::load_env_map` からダッシュボード用 Config を構築 |
| `server/monitor.rs` | pub(crate) | アラート監視。`serve` プロセス全体で1つのバックグラウンドタスクで、ブラウザのセッションとは無関係。`spawn_alert_monitor` が分足をその周期で監視し、計算済み値の条件がクロスしたらチャット通知をプッシュ（rising-edge・同一の銘柄/条件/指標足につき1回）。`alerts_*` エンドポイントを支えるルール/チャンネルCRUD（`add` / `remove` / `set_active` / `send_test`・`RuleView` / `ChannelView`）と、`--private` を尊重する `persist_add_to_env` / `persist_remove_from_env` |

---

### `src/strategies.rs`

ファイルベースの保存済みバックテスト戦略 — ルールエディタの永続化。`~/.xoksa.strategies.json`（`{name, spec_json}` の小さなJSONリスト）を読み書きし、`GET`/`POST /api/backtest/rules` エンドポイントを支える。プライベートモードを尊重する（`--private` 時は書き込みをスキップ）。データベースは使わない。

---

### `src/private.rs`

プロセス全体のプライベートモードフラグとホームディレクトリヘルパー。`set_private` / `is_private` が痕跡なしフラグ（`--private` で設定）を管理する。プライベート時はディスクに何も書かない。`home_dir` はユーザーホームディレクトリを解決する（戦略/履歴ドットファイル用）。ロガーと保存戦略ストアで共用する。

---

### `src/context.rs`

LLM向けのマルチタイムフレーム文脈を生成：`ContextSpec` が `{type, timeframe, indicator_name}` を列挙し、各足を共有エンジンで1回分析→Rust側で要約した `context_pack_text` ＋構造化 `context_pack_json` を返す（生データは渡さない＝SOT・トークン管理）。`ContextPack.facts` は同じ内容を確定データとしても運ぶ（分析した足ごとに1件、各件が自分の足を名乗る）ので、出力整合性ガードは「その足に対して述べられた数値か」を検証できる。

---

### `src/backtest.rs`

連続したプロバイダ履歴に対するルールベースのバックテスト。`BacktestSpec` / `StrategyRules` / `Condition`。各足の指標は同一エンジンで再計算（先読みなし）。現実的な資金モデル（銘柄通貨の現金・最小単元・開始割合＋シグナルごとのサイジング・FIFOトランシェ）。指標＋Buy&Hold併記＋売買明細＋警告を返す。保存したルールは `strategies.rs` 経由でJSONファイル（`~/.xoksa.strategies.json`）に永続化する。

---

### `src/logging.rs`

外部依存なしのコード付きロガー：安定エラーコード付きの `warn` / `error` / `info(code, msg)`、コンソールは重複圧縮、ローカルファイル追記（作業ディレクトリ直下の `logs/xoksa-error.log`・0600・`logs/` は自動作成・5MB超で世代ローテート `.1`/`.2`・cwd が書込不可ならシステムへフォールバック（Windows は `%PROGRAMDATA%\xoksa\logs`、Unix は `/tmp/xoksa`・ホームには書かない）・プライベートモードは書かない）。`current_log_path` が起動通知用に絶対パスを公開。`set_format(Text|Json)` で人間可読テキストかNDJSONを選択（`xoksa serve --log-format json`）。

---

### `src/notify.rs`

チャット通知層。`Notifier` トレイト＋`HttpNotifier` が Slack / Discord / Google Chat（Incoming Webhook）または LINE（Messaging API）へ1行メッセージをプッシュする。`NotifierKind` はプラットフォーム別の固定ホスト許可リスト（`https` 限定・コードで検証＝anti-SSRF）。`build_notifier` がチャンネル設定からプラットフォームを選択し、`run_test_notify_cli` が `test-notify` サブコマンドを支える（チャンネル秘密は stdin で受け取り argv は通さない）。`NOTIFY_<n>_SECRET` はクラスA（OSキーチェーン・`Zeroizing`）。データPOSTのみでシェルは介さない。

---

### `src/report.rs`

`--out <file>` 用の分析レポート出力：`write_report` が計算済み分析を HTML / Markdown / プレーンテキストへ書き出す（形式はファイル拡張子から判定）。表示専用——値は SOT ガード済みの分析に由来し、ここで再計算はしない。

---

### `src/traits.rs`

HTTP 依存の抽象化トレイト定義。テスト時の依存注入（モック差し替え）を可能にする。具体実装は各モジュール（`market.rs`, `news.rs`, `llm.rs`）に置く。

| trait | 概要 |
|-------|------|
| `PriceFetcher` | `fetch_snapshot(ticker, mode)` を定義。`YahooFinancePriceFetcher` が本番実装 |
| `ArticleFetcher` | `fetch_articles(query, api_key, ...)` を定義。`BraveArticleFetcher` が本番実装 |
| `PromptSender` | `send_prompt(config, prompt, facts)` を定義。`facts` は確定値を構造体のまま運び、出力整合性ガードがプロンプト本文ではなく帰属を検証できるようにする。`LlmDispatchSender` が本番実装（`OpenAiPromptSender` は互換用） |

**テスト方法**: `MockArticleFetcher { articles: Vec<Article> }` のように具体的なモック struct を定義して `impl ArticleFetcher` するだけで、HTTP 呼び出しなしにフローロジックのテストが可能。

---

### `src/utils.rs`

クロスカット的なユーティリティ関数群。

| 関数 | 公開 | 概要 |
|------|------|------|
| `build_proxy()` | pub | プロキシURL文字列とオプションのno-proxyリストから `reqwest::Proxy` を構築する |
| `parse_env_value()` | pub | `xoksa.env` の生バリュー文字列からインラインコメント（`#`）を除去してトリムする |
| `read_key_from_env_file()` | pub | `xoksa.env` から指定キーの値をオンデマンドで読み込む。ファイルまたはキーが存在しない場合は `None` を返す |
| `resolve_api_key()` | pub | APIキーを解決する。`xoksa.env` を優先し、未設定の場合はOSキーチェーンにフォールバック。`Result<Option<Zeroizing<String>>>` を返す（`Ok(Some)` = 取得成功、`Ok(None)` = 未設定、`Err` = キーリングアクセス障害）。全APIキー取得で使用 |
| `key_present_in_env_file()` | pub | 指定キーが `xoksa.env` に存在し非空であれば `true` を返す。`--doctor` ヘルスチェックで使用 |
| `sanitize_ascii_file_lines()` | pub | ファイルを行単位で読み込み、BOM除去・行長チェック（500文字超でエラー）・NULLバイト検出・制御文字検出を行う。`xoksa.env` と `alias_csv` の読み込みで使用 |
| `classify_score()` | pub | スコア比率（-1〜+1）をラベル文字列（「🟢 強い買い」等）に変換する。lang対応 |
| `get_color_for_score()` | pub | スコア比率から ANSI カラーコード文字列を返す |
| `get_score_description()` | pub | 指標と整数スコアの組み合わせから説明文字列を返す。指標ごとの静的テーブル定数（`SMA_SCORE_TABLE` 等）と共通ルックアップヘルパー `score_table_lookup()` を使用。lang対応 |
| `display_price_for_diff()` | pub(crate) | 差分表示に使う価格を返す（`get_latest_observed_price`、なければ `get_close` にフォールバック） |
| `displayed_price_diff()` | pub(crate) | 価格差分表示用の `(diff_value, diff_pct)` タプルを返す |
| `format_analysis_time()` | pub(crate) | ターミナル表示向けの分析時刻文字列を返す。lang対応 |
| `format_time_with_timezone()` | pub(crate) | 時刻文字列に timezone ラベルを付与して整形文字列を返す。lang対応 |
| `market_data_latest_time()` | pub(crate) | guardから最新マーケットデータのタイムスタンプを返す（分足: 最新バー時刻、日足: 最新日付） |
| `floor_time_to_interval()` | pub(crate) | 日時文字列を指定分単位（5m・15m・30m等）に切り捨てる |
| `format_bar_time_for_mode()` | pub(crate) | 分析モードとlangに応じたバー時刻ラベル文字列を返す |
| `format_latest_observed_price_bar()` | pub(crate) | 表示・プロンプト共用の「最新観測価格バー」行文字列を返す |
| `format_indicator_latest_bar()` | pub(crate) | 表示・プロンプト共用の「指標最新バー」行文字列を返す |
| `format_market_data_latest_time()` | pub(crate) | 表示・プロンプト共用のマーケットデータ最新時刻行文字列を返す。lang対応 |
| `macd_minus_policy_label()` | pub(crate) | MACDマイナス許容ポリシーのラベル文字列を返す。lang対応。`render.rs` と `llm.rs` で共用 |

---

## デスクトップクレート — `xoksa-desktop/`

別の Cargo workspace。**Tauri** の接続シェル。エンジンのコードを**一切リンクしない**（`xoksa` 依存なし）。隣に置かれた `xoksa` エンジンバイナリを子プロセス（`xoksa serve`）として起動し、WebView をそのエンジンのダッシュボードへ向け、独立した設定アプリ（`xoksa-setup`）を開く。CLI / エンジン側は Tauri/WebView 依存を**持たない**。[design-philosophy.md](./design-philosophy.md)・[security-design.md §6](./security-design.md) 参照。

### `xoksa-desktop/src/main.rs`

| 項目 | Kind | 役割 |
| :--- | :--- | :--- |
| `engine_bin_path()` / `engine_command()` | fn | エンジンバイナリを解決（Windows は無窓）。 |
| `setup_bin_path()` / `launch_setup_app()` | fn | 設定アプリを起動。 |
| `DesktopConn` / `desktop_conn_path()` / `load_desktop_conn()` / `save_desktop_conn()` | struct / fn | 接続先（非秘密）を desktop.json に保存。 |
| `ConnToken` / `read_conn_token()` / `write_conn_token()` | struct / fn | 接続トークンを keychain 経由で管理。 |
| `spawn_local_engine()` / `probe_engine()` / `pick_local_port()` | fn / enum `Probe` | ローカルエンジン起動＋原因識別つき疎通確認（Ready は版数完全一致）＋ループバック用エフェメラルポート取得。 |
| `load_connection` | `#[command]` | 接続画面の事前入力。 |
| `connect` | `#[command]` | 接続してダッシュボード URL を返す。 |
| `open_settings_app` | `#[command]` | 設定アプリを開く。 |
| `open_popup_window` / `open_external` | `#[command]` | ポップアップ窓／外部ブラウザ。 |
| `bind_host()` / `engine_lang_is_ja()` | fn | bind 先／メニュー言語。 |
| `main()` / `setup()` | fn | 起動時処理・メニュー・/login 自動認証。 |

### `xoksa-desktop/frontend/index.html`

**接続画面**：どのエンジンに接続するか——**エンジンアドレス**（host:port）・**アクセストークン**（ローカルエンジンなら空欄）・**ローカルエンジンの自動起動**トグル——と、独立した設定アプリを開く **設定** ボタン。`connect` がダッシュボード URL を返し、WebView がそこへ遷移する。API キーや設定はここには無く `xoksa-setup` にある。日英 i18n。

---

## 設定アプリクレート — `xoksa-setup/`

別の Cargo workspace（`xoksa-desktop` と同様）。**Tauri** の設定アプリ。エンジンのコードを**一切リンクしない**。全設定をエンジンの子プロセス（`config-json`／`apply-config`／`settings-password`／`conn-token`／`serve --show-token`・`--rotate-serve-token`／`ollama-models`／`test-notify`）で行い、サーバ稼働なしに `xoksa.env`＋OS キーチェーンへ書き込む。パスワードで保護し、デスクトップの **設定** から開く。[security-design.md §6](./security-design.md) 参照。

### `xoksa-setup/src/main.rs`

| 項目 | Kind | 役割 |
| :--- | :--- | :--- |
| `verify_password` / `change_password` | `#[command]` | パスワードゲート。 |
| `load_config` / `save_config` | `#[command]` | 設定の読み書き。 |
| `get_token` / `generate_token` | `#[command]` | アクセストークンの表示／再生成。 |
| `list_ollama_models` / `test_notify` | `#[command]` | Ollama モデル一覧／通知テスト。 |
| `restart_engine` | `#[command]` | エンジン再起動（起動も兼ねる）。ポート占有は原因を返す。 |
| `set_unsaved` / `quit_app` | `#[command]` | 未保存フラグの同期／終了。 |
| `engine_bin_path()` / `engine_command()` / `serve_host()` / `serve_port()` | fn | エンジン解決・bind 先。 |
| `main()` / `setup()` | fn | 起動時処理。 |

### `xoksa-setup/frontend/index.html`

設定フォーム：**パスワードゲート**の後、全設定（言語・LLM／API キー＋クラウドモデル・Ollama・ニュース/ファンダ・チャット通知チャンネル・（詳細設定で）指標）、**アクセストークン**節（生成／コピー）、エンジンの**起動／再起動**。日英 i18n（英語優先・OS ロケール既定）。API キーは Tauri IPC 境界→子プロセスの stdin のみを通る（HTTP 不経由）。

---

## 依存関係グラフ（主要な呼び出し方向）

```
main.rs
  └─ bootstrap.rs             (初期化・Config構築)
  │    └─ setup.rs            (--init 時に run_init; --doctor 時に run_doctor)
  └─ market.rs                (YahooFinancePriceFetcher → PriceFetcher trait)
  └─ technical/
  │    ├─ indicators.rs       (指標計算・Guard生成)
  │    ├─ composite.rs        (スコア合算)
  │    └─ types.rs            (データ構造)
  └─ fundamental.rs           (ファンダメンタル取得・整形。--fundamental 時のみ)
  │    ├─ J-Quants API v2/v1  (日本株: /v2/fins/summary, /v2/equities/master)
  │    └─ SEC EDGAR API       (米国株: companyfacts, company_tickers.json)
  └─ app.rs                   (出力パイプライン)
  │    ├─ render.rs           (ターミナル表示)
  │    │    └─ render_indicators.rs  (各指標の表示ロジック)
  │    ├─ output.rs           (ファイル/標準出力ログ)
  │    └─ fundamental.rs      (render_fundamental_display → ターミナル表示)
  └─ news.rs                  (BraveArticleFetcher → ArticleFetcher trait)
  └─ prompt.rs                (プロンプト組み立て)
  │    ├─ llm.rs              (LlmDispatchSender → PromptSender trait)
  │    └─ fundamental.rs      (format_fundamental_for_llm → プロンプト末尾に追加)
  └─ chat/                    (run.rs: run_chat_loop → REPL。スラッシュコマンドは llm::send_chat_turn を使用。サブモジュール: prompt/run/guard/help/council/debate/report/llm/ticker/news_extra、テストは tests.rs)

traits.rs    ← PriceFetcher / ArticleFetcher / PromptSender の定義
config.rs    ← 全モジュールから参照
utils.rs     ← render.rs, render_indicators.rs, output.rs, bootstrap.rs, llm.rs, market.rs, news.rs, fundamental.rs, setup.rs から参照
```

---

## 型の流れ

```
MarketData[]
  → indicators::build_basic_technical_entry()
  → TechnicalDataGuard  ─────────────────────────────┐
  → indicators::evaluate_all_selected_extensions()    │
                                                       ▼
                                          render.rs    (ターミナル表示)
                                          output.rs    (ログ保存)
                                          prompt.rs    (LLMプロンプト)
                                          composite.rs (スコア合算)

FundamentalData (--fundamental 時のみ)
  → fundamental::fetch_fundamental_data()
       ├─ fetch_jquants()     (日本株: J-Quants v2/v1)
       └─ fetch_sec_edgar()   (米国株: SEC EDGAR companyfacts)
  → FundamentalData ─────────────────────────────────┐
                                                       ▼
                              fundamental::render_fundamental_display() → app.rs (ターミナル)
                              fundamental::format_fundamental_for_llm() → prompt.rs (LLM)
```
