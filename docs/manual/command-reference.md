# xoksa Command-Line Reference v2.9.10

[日本語ドキュメントはこちら。](#ja)

> **Language sync:** English and Japanese descriptions are both authoritative. If a requirement, restriction, exception, command behavior, or operational rule appears in only one language, treat it as applying to both; the missing side is a documentation defect to be updated.

This is xoksa's **command-line reference**. xoksa is a browser-first, conversational stock-analysis tool (dashboard + AI chat), but the **same engine also runs headless from the command line** — for one-shot analysis, scripting, and automation. This page documents each CLI option: its function, type, default value, and corresponding environment variable.

## 1. Basic Usage

### Examples

- **Standard execution (technical + news + LLM)**
  ```bash
  xoksa --ticker 7203.T
  ```
- **Technical analysis only (skip news and LLM)**
  ```bash
  xoksa --ticker AAPL --no-news --no-llm
  ```
- **Output CSV header (for batch processing preparation, etc.)**
  ```bash
  xoksa --show-log-header
  ```

---

## 2. Basic Options

| Option | Short | Description | Type | Default | Env Var |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `--ticker` | `-t` | Ticker symbol to analyze (e.g., 7203.T, AAPL) | String | (required※) | - |
| `--analysis-mode` | - | Analysis bar mode (`daily`, `intraday`, `short`, `60m`, `1h`, `hourly`, `30m`, `15m`, `5m`, `1m`, `1min`, `weekly`, `monthly`). `daily` is the default daily bars; minute-bar modes are short-term analysis; weekly/monthly are higher-timeframe analysis | String | `daily` | `ANALYSIS_MODE` |
| `--no-env-indicators` | `-I` | Disable loading indicator settings from `xoksa.env` | bool | false | - |
| `--stance` | - | Analysis perspective (`buyer`, `seller`, `holder`) | String | `holder` | `STANCE` |
| `--show-log-header` | - | Display log CSV header and exit | bool | false | - |
| `--fundamental` | - | Retrieve and display fundamental supplementary data (Japanese stocks: J-Quants API, US stocks: SEC EDGAR) | bool | false | `FUNDAMENTAL` |
| `--out` | - | Also write the analysis report to a file. Format by extension: `.html` / `.md` / otherwise plain text. Open the HTML in a browser → **Print → Save as PDF**. Single analysis only (not `--chat`); `--no-llm` yields a data-only report | String (FILE) | - | - |
| `--env-file` | - | Use this `xoksa.env` instead of the canonical location (`%APPDATA%\xoksa` / `~/Library/Application Support/xoksa` / `~/.config/xoksa`). Read once before subcommand dispatch, so the CLI, `serve` and `apply-config` all resolve the same file | String | - |
| `--lang` | - | Display language (`en`, `ja`). Can also be specified via `LANG` in `xoksa.env` (system environment variable is not referenced) | String | `en` | `LANG` (xoksa.env) |
| `--init` | - | Generate a new `xoksa.env` via interactive wizard. Backs up any existing file to `xoksa.env.bak_NNN` immediately before writing | bool | false | - |
| `--update-key` | - | Interactively update a single API key stored in the OS Keychain. Covers the managed keys (OpenAI / Gemini / Claude / Brave / J-Quants) and the secret (`NOTIFY_<n>_SECRET`) of each notification channel defined in `xoksa.env` | bool | false | - |
| `--check-keys` | - | Print presence and storage source (xoksa.env or os-keyring) for all 9 managed API keys without displaying values | bool | false | - |
| `--doctor` | - | Diagnose `xoksa.env` configuration and display a report | bool | false | - |

※ `--ticker` is required except when `--show-log-header` / `--init` / `--doctor` / `--update-key` / `--check-keys` / `--chat` is specified. When displaying the header without specifying a ticker, `SPY` is used internally.

### Analysis Bar Mode

- The default `daily` is the conventional daily bar analysis with `interval=1d`, `range=3mo`.
- `--analysis-mode 30m` (or `intraday`, `short`) is 30-minute short-term analysis with `interval=30m`, `range=1mo`.
- `--analysis-mode 60m` (aliases: `1h`, `hourly`) is 60-minute analysis with `interval=60m`, `range=2mo`.
- `--analysis-mode 15m` is 15-minute short-term analysis with `interval=15m`, `range=1mo`.
- `--analysis-mode 5m` is 5-minute short-term analysis with `interval=5m`, `range=5d`.
- `--analysis-mode weekly` (aliases: `week`, `1wk`) is weekly-bar higher-timeframe analysis with `interval=1wk`, `range=2y`.
- `--analysis-mode monthly` (aliases: `month`, `1mo`) is monthly-bar higher-timeframe analysis with `interval=1mo`, `range=10y`.
- Indicator formulas, thresholds, weights, and buy/sell judgment logic are not changed by mode.

> **Market data source & fallback.** Bars are fetched from Yahoo (with an automatic alternate-host retry that returns identical data). If Yahoo is fully unavailable, XOKSA falls back to **Stooq** for the **daily** timeframe only — intraday/weekly/monthly return no data during a Yahoo outage rather than substituting daily bars. Fallback data may differ slightly and is clearly labelled in the output. A snapshot always comes whole from one source. Details → [security-assessment.md](../dev-prog/security-assessment.md).

> **The dashboard is self-contained.** `xoksa serve --ui` from a released binary needs no external files — the Web UI is embedded. `serve` has its own flags (`--web-dir` is a developer option); see the [Web UI Guide](usage-guide.md).

---

## 3. Technical Analysis Settings

> **How these are set (three ways).**
> - **CLI flag** (e.g. `--macd-minus-ok`, `--buy-rsi 25`): applies to a single one-shot CLI run. **Not accepted after `serve`** — the server subcommand has its own options.
> - **`xoksa.env`** (e.g. `MACD_MINUS_OK=true`, `BUY_RSI=25`, `EMA=True`, `WEIGHT_EMA=2.0`): the persistent **default** baseline for both CLI and `serve`. Edit it when you want to *change a default*. Which indicators are `True` here decides what is **computed and stored** (the DB/log column set) — that stays env-controlled so accumulated data keeps a stable schema.
> - **`/set` in chat** (e.g. `/set buy-rsi 25`, `/set weight-ema 1.5`, `/set indicator vwap off`): a **temporary, session-only** override — it does **not** rewrite `xoksa.env`. Works in the CLI REPL and the Web UI / `serve`, so you can experiment per-use without editing the env file. Tunable now: thresholds & calc params (Bollinger period/σ/squeeze, ADX/ROC/Stochastics/VWAP periods, Fibonacci ε, `macd-minus-ok`), **indicator weights** (`weight-<indicator>`), and **which indicators are active for the analysis** (`indicator <name> on/off`). Note: `indicator off` only drops it from this session's **score / display / LLM** — the indicator is still computed and stored (DB columns unchanged). Chat tuning is session-only and is **not** written to the DB.

### Threshold Settings

Specifies the numerical criteria for each indicator judgment.

| Option | Description | Type | Default | Env Var |
| :--- | :--- | :--- | :--- | :--- |
| `--buy-rsi` | RSI "oversold" threshold | f64 | 30.0 | `BUY_RSI` |
| `--sell-rsi` | RSI "overbought" threshold | f64 | 70.0 | `SELL_RSI` |
| `--macd-diff-low` | MACD vs Signal divergence "small" threshold | f64 | 2.0 | `MACD_DIFF_LOW` |
| `--macd-diff-mid` | MACD vs Signal divergence "medium" threshold | f64 | 10.0 | `MACD_DIFF_MID` |
| `--macd-diff-extreme` | MACD extreme divergence threshold with overbought RSI | f64 | 100.0 | `MACD_DIFF_EXTREME` |
| `--macd-minus-ok` (`-m`) | Allow buy judgment when MACD is in negative territory | bool | false | `MACD_MINUS_OK` |
| `--bb-bandwidth-squeeze-pct` | Bollinger squeeze judgment threshold (%) | f64 | 8.0 | `BB_BANDWIDTH_SQUEEZE_PCT` |

### Enabling Extended Indicators

Enabled by specifying the flag or setting the environment variable to `true`.

| Option | Indicator | Env Var |
| :--- | :--- | :--- |
| `--ema` | Exponential Moving Average (EMA) | `EMA` |
| `--sma` | Simple Moving Average (SMA) | `SMA` |
| `--roc` | Rate of Change (ROC) | `ROC` |
| `--adx` | Trend Strength (ADX) | `ADX` |
| `--stochastics` | Stochastics (%K, %D) | `STOCHASTICS` |
| `--bollinger` | Bollinger Bands | `BOLLINGER` |
| `--fibonacci` | Fibonacci Retracement | `FIBONACCI` |
| `--vwap` | VWAP (Volume Weighted Average Price. Daily: `--vwap-period` bars; Intraday: all bars in the same session as the last bar used for indicator calculation) | `VWAP` |
| `--ichimoku` | Ichimoku Cloud (Conversion Line, Base Line) | `ICHIMOKU` |

### Indicator Calculation Parameters

Calculation periods and judgment thresholds for each indicator can be customized via CLI options or their corresponding environment variables.
`-I` (`--no-env-indicators`) zeroes out all environment variable influence (recommended for recipe verification).

| Option | Description | Type | Default | Env Var |
| :--- | :--- | :--- | :--- | :--- |
| `--ema-short-period` | EMA short period | usize | 5 | `EMA_SHORT_PERIOD` |
| `--ema-long-period` | EMA long period | usize | 20 | `EMA_LONG_PERIOD` |
| `--sma-short-period` | SMA short period | usize | 5 | `SMA_SHORT_PERIOD` |
| `--sma-long-period` | SMA long period | usize | 20 | `SMA_LONG_PERIOD` |
| `--roc-period` | ROC lookback bars | usize | 10 | `ROC_PERIOD` |
| `--adx-period` | ADX averaging period | usize | 14 | `ADX_PERIOD` |
| `--stochastics-period` | Stochastics %K period | usize | 14 | `STOCHASTICS_PERIOD` |
| `--bollinger-period` | Bollinger Bands period | usize | 20 | `BOLLINGER_PERIOD` |
| `--bollinger-stddev-multiplier` | Bollinger Bands standard deviation multiplier | f64 | 2.0 | `BOLLINGER_STDDEV_MULTIPLIER` |
| `--vwap-period` | VWAP calculation bars (valid in non-intraday modes: daily/weekly/monthly; intraday mode uses all bars in the same-day session as the last bar) | usize | 14 | `VWAP_PERIOD` |
| `--ichimoku-tenkan-period` | Ichimoku Conversion Line period | usize | 9 | `ICHIMOKU_TENKAN_PERIOD` |
| `--ichimoku-kijun-period` | Ichimoku Base Line period | usize | 26 | `ICHIMOKU_KIJUN_PERIOD` |
| `--fibonacci-neutral-ratio` | Fibonacci neutral band around the 50% level, as a share of the 50%→38.2% distance (accepted 0.0–0.5; the cap keeps the ±1 bands reachable) | f64 | 0.05 | `FIBONACCI_NEUTRAL_RATIO` |

> [!NOTE]
> Setting short and long to the same value or reversing them will result in an error (e.g., `--ema-short-period >= --ema-long-period`).
> Similarly, `--ichimoku-tenkan-period >= --ichimoku-kijun-period` will also result in an error.

**Configuration example (EMA/SMA comparison with custom periods)**

```bash
xoksa -t NVDA -I --ema --sma --ema-short-period 8 --ema-long-period 30 \
      --sma-short-period 10 --sma-long-period 40
```

**Setting via environment variable (xoksa.env)**

```env
EMA_SHORT_PERIOD=8
EMA_LONG_PERIOD=30
VWAP_PERIOD=21
BOLLINGER_STDDEV_MULTIPLIER=2.5
```

### Weighting

Specifies the multiplier (0.5–3.0 recommended) applied to the score for each category.

| Option | Target Category | Default | Env Var |
| :--- | :--- | :--- | :--- |
| `--weight-basic` | Basic score (RSI, MACD) | 1.0 | `WEIGHT_BASIC` |
| `--weight-ema` | EMA score | 1.0 | `WEIGHT_EMA` |
| `--weight-sma` | SMA score | 1.0 | `WEIGHT_SMA` |
| `--weight-bollinger` | Bollinger Bands score | 1.0 | `WEIGHT_BOLLINGER` |
| `--weight-roc` | ROC score | 1.0 | `WEIGHT_ROC` |
| `--weight-adx` | ADX score | 1.0 | `WEIGHT_ADX` |
| `--weight-stochastics` | Stochastics score | 1.0 | `WEIGHT_STOCHASTICS` |
| `--weight-fibonacci` | Fibonacci score | 1.0 | `WEIGHT_FIBONACCI` |
| `--weight-vwap` | VWAP score | 1.0 | `WEIGHT_VWAP` |
| `--weight-ichimoku` | Ichimoku score | 1.0 | `WEIGHT_ICHIMOKU` |

---

## 4. News / LLM Settings

### External Service Integration

| Option | Short | Description | Type | Default | Env Var |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `--no-llm` | `-O` | Skip LLM access | bool | false | `NO_LLM` |
| `--no-news` | `-n` | Skip news search | bool | false | `NO_NEWS` |
| `--llm-provider` | - | LLM provider (`openai`\|`gemini`\|`claude`\|`ollama`) | String | `openai` | `llm_provider` |
| `--llm-model` | - | LLM model name (takes precedence over provider-specific settings) | String | - | `llm_model` |
| `--openai-model` | `-M` | OpenAI model name (backward compatible; `--llm-model` takes precedence) | String | `gpt-5.6-terra` | `openai_model` |
| `--extra-note` | `-x` | Append extra note to all LLM prompts (common to all providers) | String | - | `EXTRA_NOTE` |
| `--chat` | - | Enter interactive mode after analysis is complete. Cannot be used with `--init`/`--doctor`/`--ollama-bench` | bool | false | - |
| `--chat-mode` | - | Analysis bar timeframe used by chat mode (`daily`\|`60m`\|`30m`\|`15m`\|`5m`\|`1m`\|`weekly`\|`monthly`). When omitted it falls back to `CHAT_ANALYSIS_MODE`, then to whatever `--analysis-mode` / `ANALYSIS_MODE` resolved to | String | `daily` | `CHAT_ANALYSIS_MODE` |
| `--chat-memory` | - | Amount of context retained within the same execution (`low`\|`mid`\|`high`). Not persistent memory | String | `mid` | `CHAT_MEMORY` |
| `--chat-guard` | - | Buy/sell suggestion prevention level in chat (`high`\|`mid`\|`low`) | String | `high` | `CHAT_GUARD` |
| `--debate` | - | Debate buffer mode in chat (`off`\|`summary`\|`claims`). Keeps bounded excerpts of other LLM outputs as non-SOT review material | String | `summary` | `DEBATE` |
| `--autoreload` | - | Enable chat-mode auto-reload at the current intraday bar interval. Disabled for non-intraday modes | bool | false | `AUTORELOAD` |
| `--autoreload-notify` | - | Print a notification when chat-mode auto-reload fires. Default is silent | bool | false | `AUTORELOAD_NOTIFY` |

### News API Settings (xoksa.env)

| Env Var | Description | Default |
| :--- | :--- | :--- |
| `BRAVE_API_KEY` | Brave Search API key. Stored in OS Keychain via `--init` / `--update-key`; `xoksa.env` value takes priority if present | - |

### Cloud LLM Settings (xoksa.env)

| Env Var | Description | Default |
| :--- | :--- | :--- |
| `llm_provider` | Provider to use (`openai`\|`gemini`\|`claude`\|`ollama`) | `openai` |
| `llm_model` | Common model name for all providers (if unset, uses provider-specific setting) | - |
| `llm_timeout_seconds` | API timeout in seconds | `180` |
| `llm_temperature` | Generation temperature (0.0–2.0) | `0.2` |
| `llm_top_p` | top-p sampling | `0.9` |
| `llm_max_output_tokens` | Maximum output tokens | `16384` |
| `OPENAI_API_KEY` | OpenAI API key (stored in OS Keychain; `xoksa.env` takes priority when present) | - |
| `openai_model` | OpenAI model name | `gpt-5.6-terra` |
| `GEMINI_API_KEY` | Gemini API key (stored in OS Keychain; `xoksa.env` takes priority when present) | - |
| `gemini_model` | Gemini model name | `gemini-3.5-flash` |
| `CLAUDE_API_KEY` | Claude API key (stored in OS Keychain; `xoksa.env` takes priority when present) | - |
| `claude_model` | Claude model name | `claude-sonnet-5` |
| `claude_max_tokens` | Claude-specific max tokens (uses `llm_max_output_tokens` if unset) | `16384` |

### Local LLM Settings (Ollama) (xoksa.env)

Used only when `llm_provider=ollama`. No API key required.

| Env Var | Description | Default |
| :--- | :--- | :--- |
| `OLLAMA_<n>_HOST` | Host of Ollama server `n` (`n` = 1–16). Only a number whose `OLLAMA_<n>_ALIAS` is set counts as a server | `127.0.0.1` |
| `OLLAMA_<n>_PORT` | Port of Ollama server `n` | `11434` |
| `ollama_model` | Ollama model name (fallback when `llm_model` is not set) | - |
| `OLLAMA_TIMEOUT_SECONDS` | Ollama-specific timeout in seconds (takes precedence over `llm_timeout_seconds` when set) | - |
| `OLLAMA_TEMPERATURE` | Generation temperature (uses Ollama default if unset) | - |
| `OLLAMA_TOP_P` | top-p sampling (uses Ollama default if unset) | - |
| `OLLAMA_TOP_K` | top-k sampling (uses Ollama default if unset) | - |
| `OLLAMA_REPEAT_PENALTY` | Repetition penalty (uses Ollama default if unset) | - |
| `OLLAMA_NUM_CTX` | Context window size | `32768` |
| `OLLAMA_NUM_PREDICT` | Maximum predicted tokens | `8192` |
| `OLLAMA_THINK` | Ollama thinking mode (`false`\|`true`\|`low`\|`medium`\|`high`). Sent to Ollama only when set | - |
| `OLLAMA_SEED` | Generation random seed (suppresses variation under identical conditions) | `42` |
| `OLLAMA_DEBUG` | Display Ollama send options and response metadata | `false` |
| `OLLAMA_NO_GUARD` | Disable output consistency check and display model output as-is (`done_reason=length` suppression is maintained) | `false` |
| `OLLAMA_BENCH_MODELS` | Comma-separated Ollama models to benchmark; supplying it enables benchmark mode | - |
| `OLLAMA_BENCH_FORMAT` | Benchmark output format (`table`\|`csv`\|`json`) | `table` |
| `OLLAMA_KEEP_ALIVE` | Model memory retention time (e.g., `5m`, `1h`, `-1`) | - |

**CLI Options (Ollama-specific)**

| Option | Description | Type | Default | Env Var |
| :--- | :--- | :--- | :--- | :--- |
| `--debug-ollama` | Display send options and response metadata (does not output prompt body) | bool | false | `OLLAMA_DEBUG` |
| `--no-ollama-guard` | Disable output consistency check and display model output as-is (`done_reason=length` suppression is maintained) | bool | false | `OLLAMA_NO_GUARD` |
| `--ollama-think` | Thinking mode (`false`\|`true`\|`low`\|`medium`\|`high`). Sent only when specified | String | - | `OLLAMA_THINK` |
| `--ollama-bench` | Comma-separated Ollama models to benchmark. Supplying it enables benchmark mode | String | - | - |
| `--ollama-bench-format` | Benchmark output format (`table`\|`csv`\|`json`) | String | `table` | - |

> [!WARNING]
> Changing `OLLAMA_<n>_HOST` to `0.0.0.0` or a LAN IP address makes the Ollama API accessible over the network. For local use, `127.0.0.1` is recommended to avoid unintended external exposure.

> [!NOTE]
> A XOKSA-specific system message is attached to Ollama to suppress supplementing ticker facts from trained or external knowledge. Only news, partnerships, earnings, values, dates, and proper nouns explicitly stated in the input are used. Direct quotation of input values is permitted, while unit conversion, trillion/hundred-million/ten-thousand conversion, rounding, approximation, recalculation, and independent price target derivation are avoided.
> Every number in the response is checked against the confirmed values XOKSA computed — the same instrument, indicator, bar, unit, currency, sign and period — so a figure taken from another reading, a reading of an indicator that was never computed, a bare price range (e.g. `2600〜2700`), an independently derived buy/sell level, a comparison against something the input never carried (an industry average, a yield) and a reversed VWAP reading are all detected. XOKSA shows what it found, excludes only the offending sentences/lines, and keeps the rest of the answer. **The checks are not per language:** each one holds its Japanese and English wording together, so a `--lang en` answer is checked exactly as a Japanese one is.
> If `--debug-ollama` shows `done_reason=length`, the response was cut off at the generation limit. When the cut-off came with thinking output (a reasoning model spending the budget on hidden thinking), XOKSA automatically steps the reasoning effort down and retries — `…→ low → false` — so the answer fits the same budget; it only suppresses the body if the answer is still incomplete after thinking is off. If the answer is genuinely longer than the budget (no thinking involved), raise `OLLAMA_NUM_PREDICT`. The decision is driven by the runtime response, not the model name. Separately, if Ollama returns a 400 response indicating "thinking not supported," it retries once without sending `think`, regardless of the model name.
> `--ollama-bench` is a diagnostic mode for comparing local LLMs. It does not display the model body; it outputs `status`, `done_reason`, `prompt_ctx_usage`, `eval_count`, `eval_tokens_per_second`, `response_chars`, `thinking_chars`, the integrity-check issue count, and the removed character count with the share of the response it represents. `status` reports **completion only** — `ok` (ran to the end), `length` (truncated at the generation limit), `error` (the request failed) — and never carries a quality verdict: an integrity detection is not a defect (see [security-design.md](../dev-prog/security-design.md) §1). Read the detected share together with `response_chars`; on its own it is minimised by an answer that states nothing. XOKSA's own computed values are the SOT, and the mode is used to compare speed, completion, and how much of a model's output reaches the screen.

**Notes:**

- Model names are specified exactly as each vendor accepts them. No custom conversion is performed on the XOKSA side.
- API keys are stored in the OS Keychain via `--init` or `--update-key`. If a key is also present in `xoksa.env`, the `xoksa.env` value takes priority. Key configuration via command-line arguments is not supported (designed to prevent exposure in command history and process lists).
- The LLM's role is only to explain indicators, scores, and news pre-calculated by the program. It does not perform numerical calculations.

### Fundamental API Settings (xoksa.env)

Used only when `--fundamental` is specified, or when `FUNDAMENTAL=true` is set in `xoksa.env` (enables fundamental data retrieval without the CLI flag; useful when running in chat mode repeatedly).

**Japanese Stocks (J-Quants API)**

One setting is sufficient, in order of highest priority.

| Env Var | Description |
| :--- | :--- |
| `JQUANTS_API_KEY` | J-Quants API key (v2). The only supported J-Quants credential — V1 token auth was retired 2026-06-01 |

**US Stocks (SEC EDGAR)**

No API key is required, but setting the User-Agent is mandatory per SEC's terms of use.

| Env Var | Description | Example |
| :--- | :--- | :--- |
| `SEC_USER_AGENT` | User-Agent string attached to SEC EDGAR requests | `MyApp/1.0 contact@example.com` |

---

### Proxy Settings (xoksa.env)

For environments requiring a proxy, such as corporate networks. Supports CONNECT tunnel-type (non-MITM) proxies.

| Env Var | Description | Default |
| :--- | :--- | :--- |
| `HTTPS_PROXY` | Proxy URL for HTTPS communications | - |
| `HTTP_PROXY` | Proxy URL for HTTP communications | - |
| `NO_PROXY` | Proxy exclusion hosts (comma-separated) | - |

**Configuration examples (xoksa.env):**

```ini
HTTPS_PROXY=http://proxy.corp.example:8080
HTTPS_PROXY=socks5://127.0.0.1:1080
NO_PROXY=127.0.0.1,localhost
```

> **Note:** Authenticated proxy URLs (`user:password@...`) are not supported. Configure proxy authentication at the OS level.

**Notes:**

- Ollama (local LLM) always has its proxy disabled on the code side, so `NO_PROXY` configuration is unnecessary.
- SSL inspection (MITM-type) proxies are not supported. Use a CONNECT tunnel-type proxy.

### Chat Notification (Alerts) Settings (xoksa.env)

While `xoksa serve` is running, a rule watches its condition and pushes a notification to a chat platform (Slack / Discord / Google Chat / LINE). It runs **only while the server is up** (no background residency). See `/alert` above. What it evaluates is the **latest fetched bar** — on Japanese intraday timeframes that is the **still-forming bar carrying the real-time quote**. It does not wait for the bar to close, so a crossing is caught within the bar (once per ticker / condition / bar, on the rising edge). Notification hosts are a **fixed per-platform allowlist** (`hooks.slack.com` / `discord.com` / `chat.googleapis.com` / `api.line.me`), `https`-only; the message carries the confirmed (SOT) value + threshold, never an invented trade call — see [security-design.md §4](../dev-prog/security-design.md).

**Channels** (numbered; the secret lives in the OS keychain, not here):

| Env Var | Description |
| :--- | :--- |
| `NOTIFY_<n>_KIND` | Platform: `slack` / `discord` / `gchat` / `line` |
| `NOTIFY_<n>_NAME` | Nickname referenced by a rule's `NOTIFY` |
| `NOTIFY_<n>_TO` | Destination id (LINE only; webhook types don't need it) |
| `NOTIFY_<n>_SECRET` | **Keychain only** — the webhook token/path, or the LINE bot token (Class A). Register via `--update-key` or the desktop settings app, never as a CLI argument |

**Watch rules** (numbered):

| Env Var | Description |
| :--- | :--- |
| `ALERT_<n>_TICKER` | Ticker to watch (e.g. `NVDA`) |
| `ALERT_<n>_MODE` | Timeframe (e.g. `5m`) |
| `ALERT_<n>_WHEN` | Condition on a computed value (e.g. `rsi<=30`, `score>=4`) |
| `ALERT_<n>_NOTIFY` | Channel `NAME` to send to |
| `ALERT_<n>_EXPLAIN` | `off` / `on` — attach a short guard-checked LLM note (default `off`) |

### Search / Prompt Adjustments

- `--custom-news-query` (`-q`): Specify a custom search query.
- `--news-filter`: Narrow queries with financial terms (earnings, etc.).
- `--news-count`: Number of results to retrieve (1–50). Default is 20 when filter is ON, 50 when OFF.
- `--news-freshness`: Search period (`pd`: 1 day, `pw`: 1 week, `pm`: 1 month, `py`: 1 year, `all`).
- `--show-news`: Display the news list in the terminal.

News items are handled as titles, publication times when available, and URLs. XOKSA does not fetch or send article bodies to the LLM in the standard news flow. LLM news triage is therefore a confirmation-priority sort, not proof that the article body was read.

### Interactive Mode (Chat)

> **v2.1.0 command redesign (BREAKING).** Chat commands were shortened to ≤6-char words with a uniform `cmd [sub] [args]` grammar, and `/help` is now grouped by category. The **authoritative, always-current command list is `/help` in-app** (single source: `chat::help::command_catalog`, shared by the CLI and the Web UI). Rename map (old name → new): `memory`→`/mem`, `ticker`→`/sym` (`sym add` / `sym del`), `autoreload`→`/auto`, `prompt`→`/basic` (alias: `/run`), `news-extra`→`/news` (`news find` / `news use`; the interim `/nx` name is also mapped); the response-style commands are flat top-level (`/depth`, `/scope`, `/shape`, `/cast`; `/tune` and the tone/hypo/sens axes were removed in v2.2.4); the multi-LLM family (`council`, `debate`, `board`, `criticize`) → `/forum` (`set` / `ask [rN] <q>` / `sum` / `log` / `chair <p>` / `clear`; `crit` and `keep` are now top-level `/crit` and `/keep`). New: `/llm` lists usable models, `/llm net` queries provider model lists. Typing an old name prints a one-time hint pointing at the new one.

Chat mode (`--chat`) enters an interactive follow-up session. It can run after ticker analysis or without a ticker at all:

```bash
# After analysis
xoksa --ticker AAPL --chat

# Ticker-less start — add tickers interactively with /sym add
xoksa --chat
```

When started with a ticker, the analysis pipeline runs first (market data and technical indicators always; fundamentals if `--fundamental` or `FUNDAMENTAL=true`; news unless `--no-news`) and its data is passed to the LLM as `base_context` throughout the chat. The initial LLM output is skipped; use `/basic` to run the initial analysis on demand.

| Command | Action |
| :--- | :--- |
| `/sym` | Lists currently loaded tickers with numbers. Example: `1: AAPL (Apple Inc.)` |
| `/sym add <symbol>` | Fetches market data and technical indicators for an additional ticker (news unless `--no-news`; fundamentals if `--fundamental` or `FUNDAMENTAL=true`) and appends its analysis to the existing context. Up to 5 tickers can be loaded simultaneously. Conversation history is preserved. With 4–5 tickers, context per ticker is significantly compressed — using `/mem low` is recommended. See [usage-guide.md](usage-guide.md) for budget details |
| `/sym del <number>` | Removes the specified ticker by its list number. Conversation history is cleared. If the last ticker is removed, the session returns to ticker-less state |
| `/reload t` | Re-fetches market data and rebuilds technical indicators for all loaded tickers. News and fundamental data are unchanged. Conversation history is preserved |
| `/reload n` | Re-fetches market data, technical indicators, and news for all loaded tickers. Fundamental data is unchanged. Conversation history is preserved |
| `/auto [on\|off\|notice]` | Toggles automatic technical reload at the current intraday bar interval. Non-intraday modes are unsupported. Auto-reload does not re-fetch fundamentals. `notice` enables auto-reload and prints a price-vs-prev-bar line per ticker on each reload |
| `/mode [daily\|60m\|30m\|15m\|5m\|1m\|weekly\|monthly]` | Shows the current analysis bar mode, or switches to the specified mode (`daily`=daily bars, `60m`/`30m`/`15m`/`5m`/`1m`=intraday, `weekly`/`monthly`=higher timeframe). Data is automatically re-fetched after switching. Works in ticker-less sessions — the mode persists for subsequent `/sym add` calls |
| `/tech [tf] [1-5]` | Prints technical data for the specified ticker. `tf` is an optional bar timeframe (`daily\|1m\|5m\|15m\|30m\|60m\|weekly\|monthly`); if omitted, the current `/mode` setting is used. Specifying a different timeframe fetches data on the fly without modifying the session. Index is required when multiple tickers are loaded |
| `/funda [1-5]` | Prints fundamental data for the specified ticker. Index forms: `1` / `1,3` / `1-3`. The number is required when multiple tickers are loaded; omit it when only one ticker is loaded |
| `/set` | Shows the current analysis parameters, weights, and the active-vs-enabled indicator sets (session-only; `xoksa.env` is unchanged) |
| `/set <field> <value>` | Changes an analysis parameter (e.g. `set buy-rsi 25`, `set macd-minus-ok on`, `set bb-period 20`). Fields: `macd-minus-ok` / `buy-rsi` / `sell-rsi` / `macd-diff-low\|mid\|extreme` / `bb-period\|sigma\|squeeze` / `adx-period` / `roc-period` / `stoch-period` / `vwap-period` / `fib-ratio` |
| `/set weight-<indicator> <n>` | Sets an indicator's score weight for the session (`basic`/`ema`/`sma`/`bollinger`/`roc`/`adx`/`stochastics`/`fibonacci`/`vwap`/`ichimoku`) |
| `/set indicator <name> <on\|off>` | Activates/deactivates an indicator for **this session's analysis** (score/display/LLM). Only env-enabled indicators can be toggled; the indicator is still computed and stored either way (DB columns unchanged) |
| `/set stance <buyer\|holder\|seller>` | Sets the interpretation stance for the session (affects the score gauge's orientation and the LLM's reading lean; does not change the composite score) |
| `/set reset` | Restores analysis parameters, weights, the active indicator set, and the stance to their `xoksa.env` defaults |
| `/news [1-5]` | Prints the news titles and URLs held for the specified ticker. Index forms: `1` / `1,3` / `1-3` |
| `/news nf [1-5]` | Asks the LLM to triage the title+URL candidates by confirmation priority (the session is left unchanged). Index optional |
| `/news find <keyword>` | Fetches up to 5 news articles for the given keyword via Brave Search and stores them in an extra-news slot (X01–X16). Keyword max 256 characters. Requires `BRAVE_API_KEY` |
| `/news list` | Shows all filled extra-news slots with titles and URLs |
| `/news use <number>\|all [on\|off]` | Schedules injection into the next message. `use all` schedules all slots; `use all on` / `use all off` toggles always-inject; `use <N>` schedules slot N |
| `/news del <number>` | Deletes the specified slot |
| `/news clear` | Empties the extra-news buffer |
| `/mem low\|mid\|high` | Changes the context retention budget for this session. Example: `/mem low` to reduce token usage mid-session |
| `/status` | Shows the current chat-session state in one view: loaded tickers, bar mode, LLM provider/model, chat guard, response style settings (depth/scope/shape/cast), memory budget, Debate Buffer, cumulative tokens, prompt candidate size, extra-news slot count, data-fetch settings, and auto-reload state |
| `/date` | Shows the current date and time — local (with weekday and UTC offset) plus UTC. Example: `📅 Now: 2026-07-09 10:45:12 +09:00 (Thu)` / `🌐 UTC: 2026-07-09 01:45:12 UTC` |
| `/token [detail]` | Displays loaded context size, regular-chat candidate size, memory budget, and cumulative input/output/total token counts. `detail` adds a section-by-section character breakdown for technical, fundamental, news, extra-news slots, recent turns, summary, Debate Buffer, guard, and `/basic` full candidate |
| `/basic` | Sends the base context with the selected chat guard constraints (`--chat-guard`) to the LLM and displays analysis results (basic analysis; alias: `/run`). Sends full context without the `--chat-memory` budget limit, so token consumption is high |
| `/llm <provider>[:<alias_or_model>]` | Switches the LLM provider and model for this session. Examples: `/llm openai`, `/llm gemini`, `/llm openai:gpt-5.6-terra`, `/llm ollama:gpu1`. With no argument, lists usable LLMs (provider / model / API-key presence). For ollama, an alias configured in `xoksa.env` is required; specifying a raw model name that is not a registered alias will be rejected |
| `/llm net` | Queries each provider's API for its current model names (to discover candidates to write into `xoksa.env`) |
| `/depth [shallow\|mid\|deep]` | **Interpretation depth** — how far the reply reads INTO the given data (default `mid`; no new numbers introduced, no outside knowledge). `shallow`: indicator values and their standard meaning only. `mid`: for each indicator cited, explains the market-participant behavior its value implies and its price impact, grounded in the provided values. `deep`: additionally states a directional conclusion where the data supports it |
| `/scope [narrow\|mid\|wide]` | **Knowledge scope** — how far the reply reaches OUTSIDE the input for general knowledge (default `mid`; whatever the setting, the model is told to take confirmed values, stock facts and news from the input only, and the integrity guard removes figures that do not match the computed values). `narrow`: build the answer from the input only. `mid`: may use general finance / market / technical concepts and reasoning. `wide`: additionally uses general industry and macro knowledge to answer the question directly |
| _Difference_ | `depth` = how DEEP into the given data; `scope` = how far OUTSIDE the input. Current values show in `/status` |
| `/shape [talk\|points\|scenario]` | Sets the answer form (default `talk`). `talk`: answer in ordinary prose (conversational), weaving in only the indicators relevant to the question. `points`: structured list of checkpoints and key items. `scenario`: structured as view / conditions for validity / conditions for invalidation / confirmation checkpoints |
| `/cast [off\|soft\|bold]` | Sets the forecasting strength (default `soft`). The confirmed computed values are never rewritten in any mode. `off`: limit to confirmed data; no forecasting. `soft`: near-term directional outlook (up/down/range) and conditions, grounded on the values and trend; avoids asserting specific future figures. `bold`: concrete projections of indicator levels or price direction for horizons such as one hour, tomorrow, or next week, labeled as predictions |
| `/forum` | Show current forum participants, the Chair (synthesizer) setting, and usage |
| `/forum set <p,...>` | Set forum participants. Comma-separated list of `provider[:model]` entries (e.g. `forum set openai:gpt-4o,claude,gemini`). For ollama, alias is required (e.g. `ollama:gpu1`). Calling with no argument clears all participants |
| `/forum ask [rN] <question>` | Multi-LLM deliberation. `rN` sets the number of rounds (default 1, max 8; e.g. `forum ask r2 …`). 1 round = all participants reply in parallel → the Chair synthesizes. The final round's Chair issues a closing conclusion. API calls = N × (participants + 1). Questions and Chair reports are stored in the Board for continuity across asks |
| `/forum sum` | Ask the Chair to synthesize the full Board / Debate Buffer contents into a structured summary |
| `/forum log` | Show the Board (meeting minutes): questions, participant opinions, and Chair reports in chronological order |
| `/forum chair <provider>[:<model>]\|reset` | Set or show the Chair (synthesizer). When not set, the active LLM acts as Chair (instructed to synthesize neutrally even if it is also a participant). `reset` returns the Chair to the active LLM |
| `/forum clear` | Clears the Debate Buffer and the Board |
| `/crit` | Has the **currently selected** LLM critically review the latest opinion in the Debate Buffer against XOKSA's computed/retrieved data (SOT). Requires `/keep summary\|claims` (the default is `summary`) and at least one buffered answer from a **different** LLM. Reports only statements that need attention — contradictions in value or direction, claims not confirmable in the data, news-body assertions (bodies are never fetched), and comparisons whose data timestamps may differ |
| `/keep [off\|summary\|claims]` | Sets the Debate Buffer retention mode for other LLMs' views (default `summary`). With no argument, shows the buffer contents |
| `/alert` | List alert rules, monitoring state, and registered notification channels |
| `/alert on <n>` / `/alert off <n>` | Enable/disable rule n for the session (the monitor runs only while `serve` is up) |
| `/alert add <ticker> <mode> <condition> <channel> [explain]` | Add a watch rule (e.g. `/alert add NVDA 5m rsi<=30 team explain`). On a rising-edge crossing of the condition on a **computed value**, a one-line notification (the confirmed value + threshold) is pushed to the channel; `explain` attaches a short guard-checked LLM note |
| `/alert del <n>` | Remove alert rule n |
| `/alert test <channel>` | Send a test message to verify a channel works. The message is sent in the language configured by `LANG` in `xoksa.env` — the same language real alert notifications use |
| `/clear` | Wipes the chat history and the Debate Buffer. The confirmed indicator data is kept, so analysis continues from the same numbers. In the Web UI the 🗑 button sends this same command |
| `/help` | Shows the in-app command list, grouped by category. This list is the authoritative, always-current set |
| `/bye` | Exits the chat |

**Context Retention Mode (`--chat-memory`)**

| Mode | Max Context Chars | Retained Turns | Max Summary Chars | Use Case |
| :--- | :---: | :---: | :---: | :--- |
| `low` | 8,000 | 2 | 800 | Low cost, simple questions |
| `mid` | 16,000 | 4 | 2,000 | Standard (recommended, default) |
| `high` | 32,000 | 8 | 3,000 | Detailed context, high token |

> [!NOTE]
> `--chat-memory` controls only the amount of context retained within a single execution. It is not persistent memory after the session ends.
> When the budget is tight, the latest question and guard constraints are retained with highest priority, followed by the most recent conversation turns and analysis context (base). Debate Buffer, the news list, and conversation notes are included only when the remaining budget allows.
> Can also be set via environment variable `CHAT_MEMORY` (`low`\|`mid`\|`high`). CLI specification takes precedence.

**How to Read `/token detail`**

`/token detail` is a cost and prompt-size diagnostic for chat mode. The section values are **pre-send character counts**, not billable tokens. Actual billing and token counts depend on each provider's tokenizer and the usage returned by the API. Use it to identify which prompt sections are making the next LLM call heavier.

Example:

```text
=== Token / Context Detail ===
Note: breakdown values are pre-send character counts. Billing and actual tokens depend on the provider tokenizer and returned API usage.
Retention: budget=16000 chars / recent_turns=4 / summary_cap=2000 chars
Loaded tickers: 1

Regular chat candidate context:
  technical context : 1980 chars
  fundamental       : 329 chars
  news              : 10668 chars
  recent turns      : 10085 chars (4 turn(s))
  summary           : 0 chars
  debate buffer     : 0 chars (mode=off, entries=0)
  guard             : 159 chars
  candidate total   : 23221 chars

Special commands:
  /basic full candidate  : 25990 chars
  loaded base context    : 14556 chars
```

| Field | Meaning | What to Do If Large |
| :--- | :--- | :--- |
| `budget` | Maximum character budget used by normal chat turns for the current `/mem` mode | Use `/mem low` to reduce cost, or `/mem high` only when detail is worth the cost |
| `recent_turns` | Number of recent user/assistant turns kept before older turns are rolled into summary or dropped | Use `/mem low`, `/sym del`, or restart the session if old conversation is no longer useful |
| `summary_cap` | Maximum size of rolled conversation summary | Usually small; if it grows, old context is being retained through summary |
| `Loaded tickers` | Number of tickers currently loaded in chat | More tickers split the fixed budget; remove unnecessary tickers with `/sym del <number>` |
| `technical context` | Technical indicator display context used for normal questions | Usually the core SOT. If it is too large, use fewer tickers or lower memory |
| `fundamental` | Loaded fundamental display context | Disable `--fundamental` in future sessions if it is not needed |
| `news` | Loaded news title+URL context | This can become the largest section. Use `--no-news`, lower `--news-count`, narrow `-q`, or use `/reload t` instead of `/reload n` when news refresh is unnecessary |
| `recent turns` | Current conversation history kept in prompt | Ask shorter follow-ups, use `/mem low`, or start a fresh session after a long discussion |
| `summary` | Rolled older conversation summary | Helps continuity, but still consumes prompt space |
| `debate buffer` | Stored non-SOT excerpts from other LLM responses | Keep `/keep off` for low cost; use `/keep summary` or `claims` only when cross-model critique is needed |
| `guard` | Mandatory safety / instruction text injected each turn | Small and intentionally kept even when budgets are tight |
| `candidate total` | Approximate total pre-truncation size of sections that may be used by a normal chat turn | If this exceeds `budget`, lower-priority sections are truncated or omitted before sending |
| `/basic full candidate` | Size of the full initial analysis prompt used by `/basic` | `/basic` ignores the chat-memory budget and can be expensive; use it deliberately |
| `loaded base context` | Full analysis context captured when tickers were loaded | This is the source pool from which normal chat context is built |

In the example above, `candidate total` (23221 chars) exceeds `budget` (16000 chars), so a normal chat turn will not send everything. The largest causes are `news` and `recent turns`; reducing news count or switching to `/mem low` / starting a fresh session will have the biggest cost impact. `/basic full candidate` is larger than normal chat because `/basic` sends the full base context without the normal memory-budget trimming.

**Buy/Sell Suggestion Prevention Level (`--chat-guard`)**

Controls the strength of guard constraints injected each turn. Default is `high`.

| Level | Constraint Content |
| :--- | :--- |
| `high` (default) | Prohibits expressions equivalent to entry prices, take-profit levels, stop-loss lines, and investment solicitation. Only objective explanations of technical and fundamental levels are permitted |
| `mid` | Allows presentation as reference levels. Requires LLM to explicitly state that investment decisions are made by the user |
| `low` | No suggestion prevention. Only numeric fabrication prohibition and currency unit error prohibition are maintained |

> [!NOTE]
> Can also be set via environment variable `CHAT_GUARD` (`high`\|`mid`\|`low`). CLI specification takes precedence.
> At all levels, "prohibition of numeric fabrication outside of input data" and "prohibition of currency unit errors" are maintained.
> **For every LLM provider** (not only Ollama), a consistency guard independent of `--chat-guard` always runs; it can be turned off with `--no-ollama-guard` / `OLLAMA_NO_GUARD`, and setting `low` does not disable it. It checks each number the model writes against the confirmed values the engine computed — same instrument, same indicator, same bar, same unit and currency, same sign, same period — so a value moved from another reading, a reading of an indicator that was never computed, an entry/stop-loss level, an incorrect unit conversion and a reversed comparison are all caught. Only the offending sentences are removed; the rest of the answer is kept. The detection rules are normative and live in [security-design.md](../dev-prog/security-design.md).

**Debate Buffer Mode (`--debate`)**

`--debate` defaults to `summary`. XOKSA stores bounded excerpts of LLM responses during the current chat session so another LLM can review them later. This is intended for critical thinking across providers, not for adding facts.

| Mode | Behavior | Token Impact |
| :--- | :--- | :--- |
| `summary` (default) | Keep up to 2 short excerpts and send a compact Debate Buffer with later turns | low |
| `claims` | Keep up to 3 longer excerpts and ask the next LLM to examine claims, evidence, risks, and unverified points | medium |
| `off` | Do not store or send other LLM opinions | none |

> [!NOTE]
> Debate Buffer entries are LLM opinions, not facts. XOKSA's program-computed indicators, retrieved market/fundamental data, and loaded news remain the source of truth. `/crit` uses the current provider/model to review only the latest stored opinion material with a dedicated SOT Coverage manifest. Claims outside that coverage are treated as unverified rather than false or factual. The `/crit` response itself is not re-added to Debate Buffer, preventing self-referential debate loops.

**Examples**

```bash
# Standard chat (mid context, high guard)
xoksa -t 7203.T --chat

# Ticker-less start (add tickers interactively)
xoksa --chat

# Low-cost mode chat
xoksa -t AAPL --chat --chat-memory low

# High-context chat including fundamental data
xoksa -t NVDA --fundamental --chat --chat-memory high

# Chat with relaxed suggestion prevention (at your own risk)
xoksa -t TSLA --chat --chat-guard mid
```

---

## 5. Log / Output Settings

| Option | Description | Type | Default | Env Var |
| :--- | :--- | :--- | :--- | :--- |
| `--save-technical-log` | Save technical analysis log | bool | false | `SAVE_TECHNICAL_LOG` |
| `--log-format` | Log format (`csv` or `json`) | String | `csv` | `LOG_FORMAT` |
| `--log-dir` | Log save directory | String | `log` | `LOG_DIR` |
| `--data-append` | Append to existing CSV file | bool | false | `CSV_APPEND` |
| `--log-flat` | Do not create per-ticker subdirectories | bool | false | `LOG_FLAT` |
| `--stdout-log` | Write log to standard output | bool | false | - |
| `--silent` | Suppress standard output (display errors only) | bool | false | - |

### Report Output Adjustment (LLM)

| Option | Description | Type | Default |
| :--- | :--- | :--- | :--- |
| `--max-note-length` | Maximum characters for "Key Points" | usize | 300 |
| `--max-shortterm-length` | Maximum characters for report 2nd perspective (daily: "1-week short-term view"; intraday: minute-bar short-term view; weekly/monthly: higher-timeframe view) | usize | 150 |
| `--max-midterm-length` | Maximum characters for report 3rd perspective (daily: "1-month mid-term view"; intraday: several-day view; weekly/monthly: longer-horizon view) | usize | 150 |
| `--max-news-length` | Maximum characters for "News Highlights" | usize | 600 |
| `--max-review-length` | Maximum characters for "Overall Review" | usize | 1000 |

### Debug / Development

| Option | Short | Description |
| :--- | :--- | :--- |
| `--no-alias` | `-a` | Skip company name alias expansion |
| `--alias-csv` | - | CSV path for company name aliases (stock list) |
| `--debug-prompt` | `-d` | Write the LLM prompt to a file (`debug_prompt_<timestamp>.txt`); pair with `--no-llm` to skip the LLM call (common to all providers) |
| `--debug-args` | - | Display parsed command-line arguments (for debugging) |

---

## 6. Error Messages

All user-facing error messages are catalogued here: first the chat-command errors the engine can emit (each prefixed with ❌ on screen), then the desktop connection-screen errors in the last group. Placeholders in braces are filled in at runtime.

**Tickers and data**

| Message | Cause / fix |
| :--- | :--- |
| `No tickers loaded.` / `No ticker loaded. Use /sym add <symbol> first.` | The command needs a loaded ticker. Add one with `/sym add <symbol>` |
| `{} is already loaded.` | That ticker is already in the list |
| `Comparison is limited to 5 tickers. Use /sym del <number> to remove one first, then /sym add <symbol>.` | The five-ticker ceiling was reached |
| `/sym add requires a symbol. Example: /sym add 9433.T` | `add` was given no symbol |
| `Specify ticker number (1–{}): /sym del <number>` | `del` was given no number, or one out of range |
| `Unknown /sym subcommand. Use /sym del <number> or /sym add <symbol>.` | Only `add` and `del` exist |
| `Specify ticker number: /funda [1-{}]` / `Specify ticker number: {}` | Several tickers are loaded, so the index is required |
| `Invalid index (valid: 1–{}, e.g. 1,3 / 1-{})` | The index form is wrong or out of range |
| `Specify /reload t or /reload n.` | `/reload` needs `t` (technical) or `n` (technical + news) |
| `Auto-reload is available only in intraday modes.` | `/auto` works on 1m–60m bars only |
| `/auto notice requires at least one ticker to be loaded first.` | `notice` prints a per-ticker line, so a ticker is needed |
| `/auto accepts: on, off, notice, or no argument (toggle).` | Unknown argument |

**News**

| Message | Cause / fix |
| :--- | :--- |
| `News fetch is disabled (NO_NEWS=true).` | News is off for this run |
| `BRAVE_API_KEY is not set.` / `BRAVE_API_KEY keyring access error: {e}` | `/news find` needs the Brave key; the second form means the keyring could not be read |
| `Provide a keyword after /news find.` | No keyword given |
| `Keyword must be 256 characters or fewer.` | The keyword is too long |
| `Specify a slot number after del (e.g. del 3).` | `del` was given no slot |
| `Slot number must be between 1 and 16.` | Extra-news slots are X01–X16 |
| `Unknown use option: {}. Use use <n> or use all.` | `use` takes a slot number or `all` |
| `Unknown /news subcommand.` | Valid: `nf` / `find` / `list` / `use` / `del` / `clear`, or an index |

**Analysis parameters**

| Message | Cause / fix |
| :--- | :--- |
| `Missing value: /set {key} <value> (list with /set)` | The key was given without a value |
| `Unknown value: {}. Valid: {}` | The value is outside the accepted set |

**LLM and forum**

| Message | Cause / fix |
| :--- | :--- |
| `ollama requires an alias. Example: /llm ollama:gpu1` | An ollama instance must be named by its `xoksa.env` alias, never a raw model name. The same applies to `/forum chair ollama:gpu1` |
| `No Ollama instances configured. Set OLLAMA_N_ALIAS / OLLAMA_N_HOST / OLLAMA_N_PORT / OLLAMA_N_MODEL in xoksa.env.` | No ollama alias is defined |
| `/forum set requires a list of provider[:model] entries.` | `set` was given no participants |
| `/forum ask requires a question.` | `ask` was given no topic |
| `No participants. Use /forum set first.` | The forum has no participants yet |
| `/forum: set / ask / sum / log / chair / clear` | Unknown `/forum` subcommand |
| `Debate Buffer is empty. /forum sum requires opinion data.` | Nothing has been buffered to summarize |
| `crit requires /keep summary or claims.` | Retention is `off`, so nothing is stored to critique |
| `Debate Buffer is empty.` | `/crit` has nothing to review yet |
| `No entries from a different LLM ({}/{}) in the Debate Buffer.` | Everything buffered came from the LLM that is selected now — switch AI and get one answer first |
| `/keep accepts off, summary, or claims.` | Unknown retention mode |

**Alerts**

| Message | Cause / fix |
| :--- | :--- |
| `Usage: /alert on <rule#>` / `Usage: /alert del <rule#>` / `Usage: /alert test <channel>` | The subcommand needs its argument |
| `Unknown subcommand. Usage: /alert [list \| on <n> \| off <n> \| add <ticker> <mode> <cond> <channel> [explain] \| del <n> \| test <channel>]` | Unknown `/alert` subcommand |
| `mode must be one of 1m\|5m\|15m\|30m\|60m.` | Alerts watch intraday bars only |
| `Cannot parse the condition. Form: <indicator><op><number>, op = <= >= < > (e.g. rsi<=30)` | The condition must contain no spaces |
| `No notification channel named "…" (check NOTIFY_<n>_NAME in xoksa.env).` | The channel name does not match any `NOTIFY_<n>_NAME` |
| `Notification channel #{n} is not defined.` | The channel number does not exist |
| `NOTIFY_{n}_SECRET is not set (register it in the settings form or --update-key).` | The channel secret is missing from the OS keychain |
| `The channel secret is invalid: {s}` | The webhook path or bot token was rejected |
| `Keychain read failed: {s}` | The OS keychain could not be read |
| `Send failed: {s}` | The chat platform rejected the send, or the network failed |

**Other**

| Message | Cause / fix |
| :--- | :--- |
| `/mem requires low, mid, or high.` | Unknown context-retention mode |
| `/token accepts only detail.` | `/token` takes `detail` or nothing |
| `/status accepts no arguments.` | `/status` takes no argument |
| `⚠️ Cancelled. Type /bye or Ctrl+C to exit.` | Ctrl+C during a turn cancels it; whatever had already streamed is kept |

**Desktop connection screen**

Shown on the desktop app's connection screen when "Start a local engine automatically" fails, in the UI language.

| Message | Cause / fix |
| :--- | :--- |
| `Failed to start the engine: {0}` | The engine binary could not be launched. `xoksa` must sit beside the desktop executable (reinstalling restores it) |
| `The engine exited right after startup ({0}).` | The spawned engine died immediately (its exit status is shown). Run `xoksa serve` in a terminal to see its error output |
| `Port {0} is in use by another xoksa engine that requires a token.` | LAN access on only: the fixed port is held by a token-gated engine. Stop it, or change the port |
| `Port {0} is in use by another xoksa engine (v{1}).` | LAN access on only: the fixed port is held by another xoksa engine — the app never adopts an engine it did not start. Stop it, or change the port |
| `Port {0} is in use by another process.` | LAN access on only: something other than xoksa holds the port. Change the port |
| `No free local port could be allocated.` | The OS could not assign an ephemeral port (extremely rare). Check firewall / TCP settings |
| `The engine requires an access token (none set or mismatched).` | The engine answered 401. Enter the token issued by its Settings app |
| `The local engine did not become ready in time (last state: {0}).` | 10-second timeout; the last observed state is shown |

---

## 7. Appendix: Priority Definition

Each setting value is determined according to the following priority.

1. **Command-line arguments (CLI)**: If specified, these take the highest priority.
2. **Environment variables (xoksa.env / shell)**: Adopted when no CLI specification is given but an environment variable is defined.
3. **Hardcoded default values**: Applied when neither of the above is specified.

> [!IMPORTANT]
> `xoksa.env` is read from the **canonical per-user path** (`<config_dir>/xoksa/xoksa.env`), not the current directory. `xoksa --init` writes it there; `--env-file <path>` points at a different file. A `./xoksa.env` in the folder you first run from is imported into the canonical location once.

---

## Disclaimer

The information described in this tool and this reference is for informational purposes only and is not intended as investment solicitation. Investment decisions using the analysis results of this tool are made at the user's own risk. The developer assumes no responsibility for any losses arising from the use of this tool.

---

<a id="ja"></a>

# xoksa コマンドラインリファレンス v2.9.10

> **言語同期:** 英語と日本語の記載はいずれも有効です。要件・制約・例外・コマンド挙動・運用ルールが片方の言語にだけ存在する場合でも、両方に適用されます。欠落している側は文書不備として更新対象です。

本ページは xoksa の**コマンドラインリファレンス**です。xoksa はブラウザ会話型の株式分析ツール（ダッシュボード＋AIチャットが主）ですが、**同じエンジンをCLIでヘッドレス実行**もできます（単発分析・スクリプト・自動化）。ここでは各オプションの機能・型・既定値・対応する環境変数を記述します。

## 1. 基本操作

### 実行例

- **標準的な実行（テクニカル + ニュース + LLM）**
  ```bash
  xoksa --ticker 7203.T
  ```
- **テクニカル分析のみ（ニュース・LLMスキップ）**
  ```bash
  xoksa --ticker AAPL --no-news --no-llm
  ```
- **CSVヘッダーの出力（バッチ処理の準備等）**
  ```bash
  xoksa --show-log-header
  ```

---

## 2. 基本オプション

| オプション | 短縮 | 説明 | 型 | 既定値 | 環境変数 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `--ticker` | `-t` | 分析対象のティッカー記号（例: 7203.T, AAPL） | String | (必須※) | - |
| `--analysis-mode` | - | 分析足モード（`daily`, `intraday`, `short`, `60m`, `1h`, `hourly`, `30m`, `15m`, `5m`, `1m`, `1min`, `weekly`, `monthly`）。`daily` は既定の日足、分足指定は短期分析、週足/月足は上位足分析 | String | `daily` | `ANALYSIS_MODE` |
| `--no-env-indicators` | `-I` | `xoksa.env` からの指標設定読み込みを無効化 | bool | false | - |
| `--stance` | - | 分析の視点（`buyer`, `seller`, `holder`） | String | `holder` | `STANCE` |
| `--show-log-header` | - | ログのCSVヘッダーを表示して終了 | bool | false | - |
| `--fundamental` | - | ファンダメンタル補助情報を取得・表示（日本株: J-Quants API、米国株: SEC EDGAR） | bool | false | `FUNDAMENTAL` |
| `--out` | - | 分析レポートをファイルにも書き出す。拡張子で形式判定：`.html` / `.md` / それ以外はテキスト。HTMLはブラウザで開き **印刷→PDFで保存**。単発分析のみ（`--chat` 不可）。`--no-llm` ならデータのみのレポート | String（FILE） | - | - |
| `--env-file` | - | 正準の場所（`%APPDATA%\xoksa` / `~/Library/Application Support/xoksa` / `~/.config/xoksa`）ではなく、指定した `xoksa.env` を使う。サブコマンド振り分けの前に一度だけ読むので、CLI・`serve`・`apply-config` が同じファイルを解決する | String | - |
| `--lang` | - | 表示言語の選択（`en`, `ja`）。`xoksa.env` の `LANG` でも指定可能（システム環境変数は参照しません） | String | `en` | `LANG`（xoksa.env） |
| `--init` | - | 対話型ウィザードで `xoksa.env` を新規生成。既存ファイルがある場合は書き込み直前に `xoksa.env.bak_NNN` へバックアップ | bool | false | - |
| `--update-key` | - | OSキーチェーンに保管された個別APIキーをインタラクティブに更新。管理対象キー（OpenAI / Gemini / Claude / Brave / J-Quants）に加え、`xoksa.env` に定義した各通知チャンネルの secret（`NOTIFY_<n>_SECRET`）にも対応 | bool | false | - |
| `--check-keys` | - | 管理対象9つのAPIキーの保管状況（xoksa.env または os-keyring）を値を表示せずに確認 | bool | false | - |
| `--doctor` | - | `xoksa.env` の設定状況を診断してレポート表示 | bool | false | - |

※ `--show-log-header` / `--init` / `--doctor` / `--update-key` / `--check-keys` / `--chat` 指定時を除き、`--ticker` は必須です。未指定でヘッダー表示を行う場合は内部的に `SPY` が使用されます。

### 分析足モード

- 既定の `daily` は従来通り `interval=1d`, `range=3mo` の日足分析です。
- `--analysis-mode 30m`（または `intraday`, `short`）は `interval=30m`, `range=1mo` の30分足短期分析です。
- `--analysis-mode 60m`（別名: `1h`, `hourly`）は `interval=60m`, `range=2mo` の1時間足分析です。
- `--analysis-mode 15m` は `interval=15m`, `range=1mo` の15分足短期分析です。
- `--analysis-mode 5m` は `interval=5m`, `range=5d` の5分足短期分析です。
- `--analysis-mode weekly`（別名: `week`, `1wk`）は `interval=1wk`, `range=2y` の週足上位足分析です。
- `--analysis-mode monthly`（別名: `month`, `1mo`）は `interval=1mo`, `range=10y` の月足長期分析です。
- 指標計算式、しきい値、重み、売買判定ロジックはモードによって変更されません。

> **市場データ取得元とフォールバック。** 足は Yahoo から取得します（同一データを返す代替ホストへの自動再試行つき）。Yahoo が完全に使えない場合、XOKSA は**日足**に限り **Stooq** へフォールバックします——Yahoo 障害中の分足/週足/月足は日足で代用せず「データなし」になります。代替データはわずかに異なり得るため、出力に明確にラベル表示されます。スナップショットは常に単一ソースから丸ごと来ます。詳細 → [security-assessment.md](../dev-prog/security-assessment.md)。

> **ダッシュボードは自己完結型。** リリースバイナリの `xoksa serve --ui` は外部ファイル不要です——Web UI が埋め込まれています。`serve` は独自のフラグを持ちます（`--web-dir` は開発者向けオプション）。[Web UI ガイド](usage-guide.md)を参照。

---

## 3. テクニカル分析設定

> **設定方法（3通り）。**
> - **CLIフラグ**（例 `--macd-minus-ok`, `--buy-rsi 25`）：ワンショットのCLI実行に適用。**`serve` の後には指定不可**（serve サブコマンドは独自オプションのみ）。
> - **`xoksa.env`**（例 `MACD_MINUS_OK=true`, `BUY_RSI=25`, `EMA=True`, `WEIGHT_EMA=2.0`）：CLI/`serve` 共通の永続**デフォルト**ベースライン。**デフォルトを変えたいとき**に編集する。ここで `True` の指標が「**計算・保存される**」対象（DB/ログの列集合）を決める＝この部分はenv管理のままで、蓄積データの列を安定に保つ。
> - **チャットの `/set`**（例 `/set buy-rsi 25`, `/set weight-ema 1.5`, `/set indicator vwap off`）：**一時的・session限定**の上書きで、`xoksa.env` は書き換えない。CLI REPL と Web UI / `serve` の両方で動作し、envを編集せず都度試せる。対象：閾値・計算パラメータ（ボリンジャー period/σ/squeeze、ADX/ROC/ストキャス/VWAP の period、フィボ ε、`macd-minus-ok`）、**指標の重み**（`weight-<指標>`）、**解析で有効にする指標**（`indicator <名> on/off`）。※`indicator off` はそのセッションの**スコア/表示/LLM**から外すだけ。指標自体は計算・保存され続ける（DB列は不変）。`/set` の変更はDBには保存されない。

### 閾値設定

各指標の判定基準となる数値を指定します。

| オプション | 説明 | 型 | 既定値 | 環境変数 |
| :--- | :--- | :--- | :--- | :--- |
| `--buy-rsi` | RSIの「売られすぎ」閾値 | f64 | 30.0 | `BUY_RSI` |
| `--sell-rsi` | RSIの「買われすぎ」閾値 | f64 | 70.0 | `SELL_RSI` |
| `--macd-diff-low` | MACDとSignalの乖離「小」の閾値 | f64 | 2.0 | `MACD_DIFF_LOW` |
| `--macd-diff-mid` | MACDとSignalの乖離「中」の閾値 | f64 | 10.0 | `MACD_DIFF_MID` |
| `--macd-diff-extreme` | RSI買われすぎ時のMACD極端乖離閾値 | f64 | 100.0 | `MACD_DIFF_EXTREME` |
| `--macd-minus-ok` (`-m`) | MACDマイナス圏での買い判定を許可 | bool | false | `MACD_MINUS_OK` |
| `--bb-bandwidth-squeeze-pct` | ボリンジャースクイーズ判定のしきい値(%) | f64 | 8.0 | `BB_BANDWIDTH_SQUEEZE_PCT` |

### 拡張指標の有効化

フラグを指定するか、環境変数を `true` に設定することで有効になります。

| オプション | 指標 | 環境変数 |
| :--- | :--- | :--- |
| `--ema` | 指数平滑移動平均 (EMA) | `EMA` |
| `--sma` | 単純移動平均 (SMA) | `SMA` |
| `--roc` | 変化率 (ROC) | `ROC` |
| `--adx` | トレンド強度 (ADX) | `ADX` |
| `--stochastics` | ストキャスティクス (%K, %D) | `STOCHASTICS` |
| `--bollinger` | ボリンジャーバンド | `BOLLINGER` |
| `--fibonacci` | フィボナッチ・リトレースメント | `FIBONACCI` |
| `--vwap` | VWAP（出来高加重平均価格。日足: `--vwap-period` 本、分足: 指標計算最終足と同日セッション全足） | `VWAP` |
| `--ichimoku` | 一目均衡表（転換線・基準線） | `ICHIMOKU` |

### 指標計算パラメータ

各指標の計算期間や判定閾値をカスタマイズできます。CLI オプションまたは対応する環境変数で上書き可能です。  
`-I` (`--no-env-indicators`) を付与すると環境変数の影響をゼロにできます（レシピ検証に推奨）。

| オプション | 説明 | 型 | 既定値 | 環境変数 |
| :--- | :--- | :--- | :--- | :--- |
| `--ema-short-period` | EMA 短期期間 | usize | 5 | `EMA_SHORT_PERIOD` |
| `--ema-long-period` | EMA 長期期間 | usize | 20 | `EMA_LONG_PERIOD` |
| `--sma-short-period` | SMA 短期期間 | usize | 5 | `SMA_SHORT_PERIOD` |
| `--sma-long-period` | SMA 長期期間 | usize | 20 | `SMA_LONG_PERIOD` |
| `--roc-period` | ROC ルックバック本数 | usize | 10 | `ROC_PERIOD` |
| `--adx-period` | ADX 平均期間 | usize | 14 | `ADX_PERIOD` |
| `--stochastics-period` | ストキャスティクス %K 期間 | usize | 14 | `STOCHASTICS_PERIOD` |
| `--bollinger-period` | ボリンジャーバンド 期間 | usize | 20 | `BOLLINGER_PERIOD` |
| `--bollinger-stddev-multiplier` | ボリンジャーバンド 標準偏差倍率 | f64 | 2.0 | `BOLLINGER_STDDEV_MULTIPLIER` |
| `--vwap-period` | VWAP 計算本数（非分足モード: 日足/週足/月足で有効。分足モードは指標計算最終足と同日セッション全足を使用） | usize | 14 | `VWAP_PERIOD` |
| `--ichimoku-tenkan-period` | 一目均衡表 転換線期間 | usize | 9 | `ICHIMOKU_TENKAN_PERIOD` |
| `--ichimoku-kijun-period` | 一目均衡表 基準線期間 | usize | 26 | `ICHIMOKU_KIJUN_PERIOD` |
| `--fibonacci-neutral-ratio` | フィボナッチ 50% 水準まわりの中立帯。50%→38.2% 距離に対する割合（受け付ける範囲 0.0〜0.5。上限が `±1` の到達可能性を保つ） | f64 | 0.05 | `FIBONACCI_NEUTRAL_RATIO` |

> [!NOTE]
> 短期・長期を同じ値や逆転した値に設定するとエラーになります（例: `--ema-short-period >= --ema-long-period`）。  
> 同様に `--ichimoku-tenkan-period >= --ichimoku-kijun-period` もエラーになります。

**設定例（カスタム期間での EMA/SMA 比較）**

```bash
xoksa -t NVDA -I --ema --sma --ema-short-period 8 --ema-long-period 30 \
      --sma-short-period 10 --sma-long-period 40
```

**環境変数での設定（xoksa.env）**

```env
EMA_SHORT_PERIOD=8
EMA_LONG_PERIOD=30
VWAP_PERIOD=21
BOLLINGER_STDDEV_MULTIPLIER=2.5
```

### 重み付け (Weight)

各カテゴリのスコアに対する倍率（0.5～3.0推奨）を指定します。

| オプション | 対象カテゴリ | 既定値 | 環境変数 |
| :--- | :--- | :--- | :--- |
| `--weight-basic` | 基本スコア (RSI, MACD) | 1.0 | `WEIGHT_BASIC` |
| `--weight-ema` | EMA スコア | 1.0 | `WEIGHT_EMA` |
| `--weight-sma` | SMA スコア | 1.0 | `WEIGHT_SMA` |
| `--weight-bollinger`| ボリンジャーバンド スコア | 1.0 | `WEIGHT_BOLLINGER` |
| `--weight-roc` | ROC スコア | 1.0 | `WEIGHT_ROC` |
| `--weight-adx` | ADX スコア | 1.0 | `WEIGHT_ADX` |
| `--weight-stochastics`| ストキャスティクス スコア | 1.0 | `WEIGHT_STOCHASTICS` |
| `--weight-fibonacci`| フィボナッチ スコア | 1.0 | `WEIGHT_FIBONACCI` |
| `--weight-vwap` | VWAP スコア | 1.0 | `WEIGHT_VWAP` |
| `--weight-ichimoku`| 一目均衡表 スコア | 1.0 | `WEIGHT_ICHIMOKU` |

---

## 4. ニュース・LLM設定

### 外部サービス連携

| オプション | 短縮 | 説明 | 型 | 既定値 | 環境変数 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `--no-llm` | `-O` | LLM アクセスをスキップ | bool | false | `NO_LLM` |
| `--no-news` | `-n` | ニュース検索をスキップ | bool | false | `NO_NEWS` |
| `--llm-provider` | - | LLM プロバイダー (`openai`\|`gemini`\|`claude`\|`ollama`) | String | `openai` | `llm_provider` |
| `--llm-model` | - | LLM モデル名（プロバイダー固有設定より優先） | String | - | `llm_model` |
| `--openai-model` | `-M` | OpenAI モデル名（後方互換。`--llm-model` 優先） | String | `gpt-5.6-terra` | `openai_model` |
| `--extra-note` | `-x` | 全 LLM プロンプトに追加メモを付与（全プロバイダー共通） | String | - | `EXTRA_NOTE` |
| `--chat` | - | 分析完了後に対話モードへ入る。`--init`/`--doctor`/`--ollama-bench` との併用不可 | bool | false | - |
| `--chat-mode` | - | チャットモードで使う分析足（`daily`\|`60m`\|`30m`\|`15m`\|`5m`\|`1m`\|`weekly`\|`monthly`）。省略時は `CHAT_ANALYSIS_MODE`、次に `--analysis-mode` / `ANALYSIS_MODE` の解決結果にフォールバック | String | `daily` | `CHAT_ANALYSIS_MODE` |
| `--chat-memory` | - | 同一実行中の文脈保持量（`low`\|`mid`\|`high`）。永続記憶ではない | String | `mid` | `CHAT_MEMORY` |
| `--chat-guard` | - | チャット内の売買示唆防止レベル（`high`\|`mid`\|`low`） | String | `high` | `CHAT_GUARD` |
| `--debate` | - | チャット内の Debate Buffer モード（`off`\|`summary`\|`claims`）。他LLM出力の限定抜粋をSOTではない検討材料として保持 | String | `summary` | `DEBATE` |
| `--autoreload` | - | チャットモードで現在の分足間隔に合わせた自動リロードを有効化。分足以外では無効 | bool | false | `AUTORELOAD` |
| `--autoreload-notify` | - | チャットモードの自動リロード実行時に通知を表示。既定はサイレント | bool | false | `AUTORELOAD_NOTIFY` |

### ニュース API 設定（xoksa.env）

| 環境変数 | 説明 | 既定値 |
| :--- | :--- | :--- |
| `BRAVE_API_KEY` | Brave Search API キー。`--init` / `--update-key` でOSキーチェーンに保管。`xoksa.env` に記述がある場合はそちらが優先 | - |

### クラウド LLM 設定（xoksa.env）

| 環境変数 | 説明 | 既定値 |
| :--- | :--- | :--- |
| `llm_provider` | 使用するプロバイダー（`openai`\|`gemini`\|`claude`\|`ollama`） | `openai` |
| `llm_model` | 全プロバイダー共通モデル名（未設定時はプロバイダー別設定を使用） | - |
| `llm_timeout_seconds` | API タイムアウト秒数 | `180` |
| `llm_temperature` | 生成温度 (0.0–2.0) | `0.2` |
| `llm_top_p` | top-p サンプリング | `0.9` |
| `llm_max_output_tokens` | 最大出力トークン数 | `16384` |
| `OPENAI_API_KEY` | OpenAI API キー（OSキーチェーン保管；xoksa.env 記載値が優先） | - |
| `openai_model` | OpenAI モデル名 | `gpt-5.6-terra` |
| `GEMINI_API_KEY` | Gemini API キー（OSキーチェーン保管；xoksa.env 記載値が優先） | - |
| `gemini_model` | Gemini モデル名 | `gemini-3.5-flash` |
| `CLAUDE_API_KEY` | Claude API キー（OSキーチェーン保管；xoksa.env 記載値が優先） | - |
| `claude_model` | Claude モデル名 | `claude-sonnet-5` |
| `claude_max_tokens` | Claude 専用最大トークン数（未設定時は `llm_max_output_tokens`） | `16384` |

### ローカル LLM 設定（Ollama）（xoksa.env）

`llm_provider=ollama` の場合のみ使用する。API キー不要。

| 環境変数 | 説明 | 既定値 |
| :--- | :--- | :--- |
| `OLLAMA_<n>_HOST` | Ollama サーバ `n` の接続先ホスト（`n` は 1〜16）。`OLLAMA_<n>_ALIAS` が設定された番号だけがサーバとして有効 | `127.0.0.1` |
| `OLLAMA_<n>_PORT` | Ollama サーバ `n` の接続先ポート | `11434` |
| `ollama_model` | Ollama モデル名（`llm_model` 未指定時のフォールバック） | - |
| `OLLAMA_TIMEOUT_SECONDS` | Ollama 専用タイムアウト秒数（設定時は `llm_timeout_seconds` より優先） | - |
| `OLLAMA_TEMPERATURE` | 生成温度（未設定時は Ollama 既定値を使用） | - |
| `OLLAMA_TOP_P` | top-p サンプリング（未設定時は Ollama 既定値を使用） | - |
| `OLLAMA_TOP_K` | top-k サンプリング（未設定時は Ollama 既定値を使用） | - |
| `OLLAMA_REPEAT_PENALTY` | 繰り返しペナルティ（未設定時は Ollama 既定値を使用） | - |
| `OLLAMA_NUM_CTX` | コンテキストウィンドウサイズ | `32768` |
| `OLLAMA_NUM_PREDICT` | 最大予測トークン数 | `8192` |
| `OLLAMA_THINK` | Ollama thinking モード（`false`\|`true`\|`low`\|`medium`\|`high`）。設定時のみ Ollama に送信 | - |
| `OLLAMA_SEED` | 生成の乱数シード（同一条件での揺れを抑える） | `42` |
| `OLLAMA_DEBUG` | Ollama の送信オプションと応答メタ情報を表示する | `false` |
| `OLLAMA_NO_GUARD` | 出力整合性チェックを無効化してモデル出力をそのまま表示する（`done_reason=length` 抑止は維持） | `false` |
| `OLLAMA_BENCH_MODELS` | ベンチマーク対象の Ollama モデル（カンマ区切り）。指定するとベンチマークモードが有効になる | - |
| `OLLAMA_BENCH_FORMAT` | ベンチマークの出力形式（`table`\|`csv`\|`json`） | `table` |
| `OLLAMA_KEEP_ALIVE` | モデルのメモリ保持時間（例: `5m`, `1h`, `-1`） | - |

**CLI オプション（Ollama 専用）**

| オプション | 説明 | 型 | 既定値 | 環境変数 |
| :--- | :--- | :--- | :--- | :--- |
| `--debug-ollama` | 送信オプションと応答メタ情報を表示する（プロンプト本文は出力しない） | bool | false | `OLLAMA_DEBUG` |
| `--no-ollama-guard` | 出力整合性チェックを無効化してモデル出力をそのまま表示する（`done_reason=length` 抑止は維持） | bool | false | `OLLAMA_NO_GUARD` |
| `--ollama-think` | thinking モード（`false`\|`true`\|`low`\|`medium`\|`high`）。指定時のみ送信 | String | - | `OLLAMA_THINK` |
| `--ollama-bench` | ベンチマーク対象の Ollama モデルをカンマ区切りで指定。指定するとベンチマークモードになる | String | - | - |
| `--ollama-bench-format` | ベンチマークの出力形式（`table`\|`csv`\|`json`） | String | `table` | - |

> [!WARNING]
> `OLLAMA_<n>_HOST` を `0.0.0.0` や LAN IP アドレスに変更すると、Ollama API がネットワーク越しにアクセス可能になります。意図しない外部公開を避けるため、ローカル利用では `127.0.0.1` を推奨します。

> [!NOTE]
> Ollama には XOKSA 専用の system メッセージを付与し、学習済み知識や外部知識による銘柄事実の補完を抑制します。ニュース・提携・決算・数値・日付・固有名詞は入力に明記されたものだけを使います。入力された数値のそのまま引用は許容し、単位変換・兆円換算・億円換算・丸め・概算化・再計算・独自の価格目標算出は避ける方針です。
> 回答中のすべての数値を、XOKSA が算出した確定値と照合します——同じ銘柄・指標・足・単位・通貨・符号・時点——ので、別の値からの転用、未算出の指標の読み取り値、価格文脈での裸の未提示レンジ（例: `2600〜2700`）、独自に導いた売買水準、入力にない比較（業界平均・配当利回り等）、逆方向のVWAP説明がいずれも検出されます。XOKSA は検出内容を表示し、問題のある文・行だけを除外して、残りの回答は保持します。**検査は言語ごとに分かれていません。** 各検査が日本語表現と英語表現を同居させているため、`--lang en` の回答も日本語の回答とまったく同じように検査されます。
> `--debug-ollama` で `done_reason=length` が出た場合は、回答が生成上限で打ち切られています。打ち切りが thinking を伴う場合（推論モデルが隠れた思考に予算を使った場合）、XOKSA は自動で推論の強さを段階的に下げて再試行します（`…→ low → false`）。同じ予算で本文が収まるようにするためで、思考を切っても本文が不完全なままの時だけ抑止します。思考が絡まず本文自体が予算より長い場合は `OLLAMA_NUM_PREDICT` を増やしてください。判断はモデル名ではなく実行時の応答に基づきます。なお、Ollama が「thinking 非対応」を400応答で返した場合は、モデル名に依存せず `think` を送信しない形で一度だけ再試行します。
> `--ollama-bench` はローカルLLM比較用の診断モードです。モデル本文は表示せず、`status`、`done_reason`、`prompt_ctx_usage`、`eval_count`、`eval_tokens_per_second`、`response_chars`、`thinking_chars`、整合性チェックの指摘件数、除去文字数とそれが回答に占める割合を出力します。`status` は**完了性のみ**を表します——`ok`（最後まで出力）、`length`（生成上限で打ち切り）、`error`（リクエスト失敗）——品質の判定は含みません。整合性の検知は欠陥ではありません（[security-design.md](../dev-prog/security-design.md) §1）。検知の割合は `response_chars` と併せて読んでください。割合だけでは、何も述べない回答が最良になります。XOKSA本体の計算結果をSOTとし、速度・完走性・モデル出力のうち画面に届いた量を比較するために使います。

**注意事項:**

- モデル名は各ベンダーが受け付ける名前をそのまま指定する。XOKSA 側での独自変換は行わない。
- APIキーは `--init` または `--update-key` でOSキーチェーンに保管する。`xoksa.env` に同名のキーが記述されている場合はそちらが優先される。コマンドライン引数によるキー設定はサポートしない（コマンド履歴・プロセスリストへの露出を防ぐ設計）。
- LLM はプログラムで計算済みの指標・スコア・ニュースを説明する役割のみを担う。数値計算は行わない。

### ファンダメンタル API 設定（xoksa.env）

`--fundamental` 指定時のみ使用する。または `xoksa.env` に `FUNDAMENTAL=true` を設定することで CLI フラグなしにファンダメンタルデータを常時取得できます（チャットモードで毎回フラグを入力したくない場合に便利）。

**日本株（J-Quants API）**

認証情報は優先順位の高い順に 1 つ設定すれば十分です。

| 環境変数 | 説明 |
| :--- | :--- |
| `JQUANTS_API_KEY` | J-Quants APIキー（v2）。唯一の対応認証情報 — V1 のトークン認証は 2026-06-01 に終了 |

**米国株（SEC EDGAR）**

API キーは不要ですが、SEC の利用規約に基づき User-Agent の設定が必須です。

| 環境変数 | 説明 | 例 |
| :--- | :--- | :--- |
| `SEC_USER_AGENT` | SEC EDGAR へのリクエストに付与する User-Agent 文字列 | `MyApp/1.0 contact@example.com` |

---

### プロキシ設定（xoksa.env）

社内ネットワーク等でプロキシが必要な環境向け。CONNECT トンネル方式（非 MITM）のプロキシに対応。

| 環境変数 | 説明 | 既定値 |
| :--- | :--- | :--- |
| `HTTPS_PROXY` | HTTPS 通信に使用するプロキシ URL | - |
| `HTTP_PROXY` | HTTP 通信に使用するプロキシ URL | - |
| `NO_PROXY` | プロキシ除外ホスト（カンマ区切り） | - |

**設定例（xoksa.env）:**

```ini
HTTPS_PROXY=http://proxy.corp.example:8080
HTTPS_PROXY=socks5://127.0.0.1:1080
NO_PROXY=127.0.0.1,localhost
```

**注意事項:**

- 認証付きプロキシURL（`user:password@...`）は非対応。プロキシ認証はOSレベルで設定すること。
- Ollama（ローカル LLM）はコード側でプロキシを常時無効化しているため、`NO_PROXY` の設定は不要。
- SSL インスペクション（MITM 型）プロキシには対応していない。CONNECT トンネル方式のプロキシを使用すること。

### チャット通知（アラート）設定（xoksa.env）

`xoksa serve` の起動中、ルールが条件を監視し、成立時にチャット基盤（Slack / Discord / Google Chat / LINE）へ通知をプッシュします。**サーバ起動中のみ動作**（バックグラウンド常駐なし）。上記 `/alert` 参照。判定に使うのは**取得した最新の足**で、日本株の分足では**リアルタイム気配を含む形成中の足**になります。足の確定を待つ仕組みではないため、足の途中でクロスを捉えます（同一の銘柄・条件・足につき、偽→真に変わった瞬間の1回だけ）。

監視はサーバ内の単一のバックグラウンドタスクで、**ブラウザやダッシュボードとは無関係**です。ルールに書いた銘柄をルール自身の足周期で取得・再計算するため、その銘柄を画面に開いておく必要はなく、ブラウザを閉じていても監視は続きます。止まるのは `serve` を終了したときだけです。

**ルールは `ALERT_1_*` 〜 `ALERT_16_*` の16件まで**です。17番以降を書いても読み込まれず、起動時に警告を出して無視します。ルールは起動時に一度だけ読み込むため、**`xoksa.env` を直接編集した場合は再起動するまで反映されません**（`/alert add` と `/alert del`、およびダッシュボードからの追加・削除は、その場で反映したうえでファイルにも書きます）。通知先ホストは**プラットフォーム別の固定許可リスト**（`hooks.slack.com`／`discord.com`／`chat.googleapis.com`／`api.line.me`）で `https` 限定。メッセージは確定値（SOT）＋閾値のみで、創作した売買文言は載せません（[security-design.md §4](../dev-prog/security-design.md) 参照）。

**チャンネル**（番号付き。秘密は OS キーチェーンに保管し、ここには書かない）：

| 環境変数 | 説明 |
| :--- | :--- |
| `NOTIFY_<n>_KIND` | 基盤：`slack` / `discord` / `gchat` / `line` |
| `NOTIFY_<n>_NAME` | ルールの `NOTIFY` から参照する呼び名 |
| `NOTIFY_<n>_TO` | 宛先ID（LINE のみ。Webhook 型は不要） |
| `NOTIFY_<n>_SECRET` | **キーチェーンのみ** — Webhook トークン/パス、または LINE Bot トークン（クラスA）。`--update-key` またはデスクトップの設定アプリで登録し、CLI 引数には出さない |

**監視ルール**（番号付き）：

| 環境変数 | 説明 |
| :--- | :--- |
| `ALERT_<n>_TICKER` | 監視する銘柄（例 `NVDA`） |
| `ALERT_<n>_MODE` | 足種（例 `5m`） |
| `ALERT_<n>_WHEN` | 計算済み値の条件（例 `rsi<=30`・`score>=4`） |
| `ALERT_<n>_NOTIFY` | 送信先チャンネルの `NAME` |
| `ALERT_<n>_EXPLAIN` | `off` / `on` — ガード適用の短いLLM解説を添付（既定 `off`） |

### 検索・プロンプト調整

- `--custom-news-query` (`-q`): 独自の検索クエリを指定します。
- `--news-filter`: クエリを財務用語（財務、決算等）で絞り込みます。
- `--news-count`: 取得件数（1～50）。既定はフィルタON時20、OFF時50。
- `--news-freshness`: 検索期間 (`pd`:1日, `pw`:1週, `pm`:1月, `py`:1年, `all`)。
- `--show-news`: ニュース一覧をターミナルに表示します。

ニュースは、タイトル、取得できた公開時刻、URLを確認候補として扱います。標準のニュースフローでは記事本文を取得・LLM送信しません。LLMのニュース仕訳は本文を読んだ証拠ではなく、確認優先度の整理です。CLIではLLM本文をそのまま表示するため、ニュース仕訳はMarkdown表ではなくベタ打ちの箇条書きを前提にします。URLは前後に空行を置いて表示します。

### 対話モード（チャット）

> **v2.1.0 コマンド再設計（破壊的変更）。** チャットコマンドは語を ≤6 文字に短縮し、文法を `cmd [sub] [args]` に統一、`/help` はカテゴリ別になりました。**最新かつ正となるコマンド一覧はアプリ内の `/help`** です（単一ソース `chat::help::command_catalog`。CLI と Web UI が共有）。改名対応（旧名 → 新名）：`memory`→`/mem`、`ticker`→`/sym`（`sym add` / `sym del`）、`autoreload`→`/auto`、`prompt`→`/basic`（別名: `/run`）、`news-extra`→`/news`（`news find` / `news use`。過渡期の `/nx` も案内対象）。応答スタイルはフラットな直コマンド（`/depth`・`/scope`・`/shape`・`/cast`。`/tune` と tone/hypo/sens 軸は v2.2.4 で廃止）。マルチLLM系（`council`・`debate`・`board`・`criticize`） → `/forum`（`set` / `ask [rN] <質問>` / `sum` / `log` / `chair <p>` / `clear`。`crit` と `keep` は直コマンドの `/crit`・`/keep` に移行）。新規：`/llm` で使えるモデル一覧、`/llm net` で各社のモデル名取得。旧名を打つと新名を案内するヒントを1回表示。

チャットモード（`--chat`）は対話形式のフォローアップセッションです。銘柄分析後に入ることも、銘柄なしで起動することもできます。

```bash
# 銘柄を指定して起動
xoksa --ticker 7203.T --chat

# 銘柄なしで起動（セッション内で /sym add を使って追加）
xoksa --chat
```

銘柄を指定した場合は、分析パイプラインを実行したうえで対話モードへ入ります（市場データ・テクニカル指標は常時取得。ファンダメンタルは `--fundamental` または `FUNDAMENTAL=true` の場合のみ。ニュースは `--no-news` でなければ取得）。初回のLLM出力はスキップされ、収集した全調査データは `base_context` としてチャット全体を通じてLLMへ引き渡されます。`/basic` で初回分析をオンデマンドで実行できます。

| コマンド | 動作 |
| :--- | :--- |
| `/sym` | 現在ロードされている銘柄を番号付きで一覧表示します。例: `1: 9433.T (KDDI)` |
| `/sym add <銘柄>` | 追加銘柄の市場データ・テクニカル指標を取得し、既存の context に追記します（ニュースは `--no-news` でなければ取得。ファンダメンタルは `--fundamental` または `FUNDAMENTAL=true` の場合のみ）。最大5銘柄まで同時ロード可能。会話履歴は維持されます。4〜5銘柄では1銘柄あたりの文脈が大幅に圧縮されるため `/mem low` を推奨します。詳細は [usage-guide.md](usage-guide.md) を参照 |
| `/sym del <番号>` | 指定番号の銘柄を削除します。会話履歴はリセット。最後の銘柄を削除すると銘柄なし状態に戻ります |
| `/reload t` | ロード中の全銘柄について市場データを再取得してテクニカル指標を再構築します。ニュース・ファンダメンタルは変更されません。会話履歴は維持されます |
| `/reload n` | ロード中の全銘柄についてテクニカル指標とニュースを再取得します。ファンダメンタルは変更されません。会話履歴は維持されます |
| `/auto [on\|off\|notice]` | 現在の分足間隔に合わせたテクニカル自動リロードを切り替えます。分足以外では使用できません。ファンダメンタルは再取得しません。`notice` は有効化と同時にリロードごとの銘柄価格変動を1行表示します |
| `/mode [daily\|60m\|30m\|15m\|5m\|1m\|weekly\|monthly]` | 現在の分析足を表示、または切り替えます（`daily`=日足、`60m`/`30m`/`15m`/`5m`/`1m`=分足、`weekly`/`monthly`=上位足）。切り替え後はデータが自動再取得されます。銘柄なしセッションでも設定でき、その後の `/sym add` に引き継がれます |
| `/tech [足] [1-5]` | 指定銘柄のテクニカルデータを表示します。`足` は省略可能な足種（`daily\|1m\|5m\|15m\|30m\|60m\|weekly\|monthly`）で、省略すると現在の `/mode` を使用。現在の足と異なる場合はオンデマンドで取得しセッションは変更しません。複数銘柄ロード時は番号が必須 |
| `/funda [1-5]` | 指定番号の銘柄のファンダメンタルデータを表示します。番号の書き方は `1` / `1,3` / `1-3`。複数銘柄ロード時は番号が必須。1銘柄時は省略可 |
| `/set` | 現在の解析パラメータ・重み・「有効(active)／計算対象(enabled)」の指標セットを表示します（session限定・`xoksa.env` は不変） |
| `/set <項目> <値>` | 解析パラメータを変更します（例: `set buy-rsi 25`、`set macd-minus-ok on`、`set bb-period 20`）。項目: `macd-minus-ok` / `buy-rsi` / `sell-rsi` / `macd-diff-low\|mid\|extreme` / `bb-period\|sigma\|squeeze` / `adx-period` / `roc-period` / `stoch-period` / `vwap-period` / `fib-ratio` |
| `/set weight-<指標> <n>` | 指標のスコア重みをセッション限定で変更（`basic`/`ema`/`sma`/`bollinger`/`roc`/`adx`/`stochastics`/`fibonacci`/`vwap`/`ichimoku`） |
| `/set indicator <名> <on\|off>` | **そのセッションの解析**（スコア/表示/LLM）で指標を有効化/無効化。env で有効な指標のみ切替可。どちらでも指標は計算・保存され続ける（DB列は不変） |
| `/set stance <buyer\|holder\|seller>` | 解釈スタンスをセッション限定で設定（スコアゲージの向きとLLMの読みの傾きに作用。合成スコア自体は変えない） |
| `/set reset` | 解析パラメータ・重み・有効指標セット・スタンスを `xoksa.env` の既定に戻します |
| `/news [1-5]` | 指定銘柄が保持しているニュースのタイトルとURLを表示します。番号の書き方は `1` / `1,3` / `1-3` |
| `/news nf [1-5]` | タイトル+URLの候補をLLMで確認優先度順に仕分けます（セッション変更なし）。番号は省略可。Markdown表ではなくベタ打ちの箇条書きを前提にします |
| `/news find <検索ワード>` | 指定キーワードのニュースをBrave Searchで最大5件取得し、追加ニューススロット（X01〜X16）に保存します。検索ワードは最大256文字。`BRAVE_API_KEY` が必要 |
| `/news list` | 追加ニュースバッファの内容（各スロットのタイトル・URL）を表示します |
| `/news use <番号>\|all [on\|off]` | 次のメッセージへの注入を予約します。`use all` は全スロット、`use all on` / `use all off` は常時注入の切替、`use <N>` はスロットNを予約 |
| `/news del <番号>` | 指定スロットを削除します |
| `/news clear` | 追加ニュースバッファをクリアします |
| `/mem low\|mid\|high` | セッション中の文脈保持予算を変更します。例: `/mem low` でトークン消費を抑制 |
| `/status` | 現在のチャット状態を一覧表示します。ロード中の銘柄、分析足、LLMプロバイダー/モデル、チャットガード、回答スタイル設定（depth/scope/shape/cast）、メモリ上限、Debate Buffer、累計トークン、通常チャット候補サイズ、追加ニューススロット数、データ取得設定、自動リロード状態を確認できます |
| `/date` | 現在の日付と時刻を表示します。ローカル時刻（曜日・UTCオフセット付）と UTC の両方。例: `📅 現在日時: 2026-07-09 10:45:12 +09:00 (木)` / `🌐 UTC: 2026-07-09 01:45:12 UTC` |
| `/token [detail]` | ロード済みコンテキスト、通常チャット候補サイズ、メモリ上限、セッション累計の入力・出力・合計トークン数を表示します。`detail` を付けると、テクニカル、ファンダメンタル、ニュース、追加ニューススロット、直近ターン、要約、Debate Buffer、ガード、`/basic` フル候補の文字数内訳を表示します |
| `/basic` | base context に選択中のチャットガード制約（`--chat-guard`）を付与してLLMへ送信し、分析結果を表示します（基本分析。別名: `/run`）。`--chat-memory` の予算制限を経ずにフル文脈を送信するため、トークン消費が大きくなります |
| `/llm <provider>[:<alias_or_model>]` | セッション中のLLMプロバイダーとモデルを切り替えます。例: `/llm openai`、`/llm gemini`、`/llm openai:gpt-5.6-terra`、`/llm ollama:gpu1`。引数なしで使えるLLM一覧（プロバイダ / モデル / APIキー有無）を表示します。ollama は `xoksa.env` に登録済みのエイリアス指定が必須。エイリアスとして未登録の名前は拒否されます |
| `/llm net` | 各社APIから現行モデル名を取得します（`xoksa.env` に書く候補の発見用） |
| `/depth [shallow\|mid\|deep]` | **解釈の深さ** — 与えられたデータをどこまで読み込むか（既定 `mid`。新たな数値は持ち込まず、データの外の知識は使いません）。`shallow`: 指標値と一般的な意味の提示のみ。`mid`: 引用する指標について、その数値が示す市場参加者の行動と価格への影響まで確定値を根拠に説明する。`deep`: さらに、データが支持する場合は方向性を明言する |
| `/scope [narrow\|mid\|wide]` | **知識の活用** — 入力の外の一般知識をどこまで使うか（既定 `mid`。どの設定でも、確定値・銘柄事実・ニュースは入力にあるものだけを使うようモデルに指示し、確定値と合わない数値は整合性ガードが取り除きます）。`narrow`: 回答を入力データの範囲で組み立てる。`mid`: 一般的な金融・市場・テクニカルの概念や推論を用いてよい。`wide`: さらに業界・マクロの一般知識も用いて質問に直接答える |
| _違い_ | `depth`＝与えられたデータを「中へ深く」／ `scope`＝入力の「外へ広く」。現在値は `/status` で表示 |
| `/shape [talk\|points\|scenario]` | 回答の形を設定します（既定 `talk`）。`talk`: 結論・見解を通常の文章（会話体）で述べ、質問に関係する指標だけを文中に織り込む。`points`: 確認ポイントや重要項目を整理した箇条書きで返す。`scenario`:「見立て」「成立条件」「崩れる条件」「確認ライン」を分けて返す |
| `/cast [off\|soft\|bold]` | 将来予測の強さを設定します（既定 `soft`）。いずれのモードでも計算済み確定値の書き換えは行いません。`off`: 確定データの説明のみで予測しない。`soft`: 確定値とトレンドを根拠に、近い将来の方向性（上昇・下落・レンジ）と条件を示す。具体的な将来数値の断定は控える。`bold`: 1時間後・明日・来週などの指標水準・価格方向を「予測」と明示して具体的に提示する |
| `/forum` | forum参加者とChair（まとめ役）設定・使い方を表示 |
| `/forum set <p,...>` | forum参加者を設定。`provider[:model]` のカンマ区切りリスト（例: `forum set openai:gpt-4o,claude,gemini`）。ollama はエイリアス必須（例: `ollama:gpu1`）。引数なしで全参加者をクリア |
| `/forum ask [rN] <質問>` | 複数LLMで合議。`rN` でラウンド数を指定（既定1・最大8。例: `forum ask r2 …`）。1ラウンド = 全参加者並列回答 → Chair が整理。最終ラウンドのChairが結論を出す。API呼び出し数 = N × (参加者数 + 1)。質問とChairレポートはBoardに記録され、次のaskにも引き継がれる |
| `/forum sum` | Chair に Board / Debate Buffer 全体の構造化サマリーを生成させる |
| `/forum log` | Board（議事録）を表示。質問・参加者見解・Chairレポートを時系列で表示 |
| `/forum chair <provider>[:<model>]\|reset` | Chair（まとめ役）を設定または表示。未設定時はアクティブLLMが代行（参加者と同じLLMでも中立整理に徹するよう指示が入る）。`reset` でアクティブLLMに戻す |
| `/forum clear` | Debate Buffer と Board をクリアします |
| `/crit` | **いま選択中の**LLMに、Debate Buffer の直近の見解を XOKSA の計算・取得データ（SOT）と突き合わせて批判的に検討させます。`/keep summary\|claims`（既定は `summary`）と、**別の**LLMの回答がバッファにあることが必要です。報告するのは問題のある記述だけ — 数値や方向が食い違う記述、データで確認できない根拠にもとづく主張、ニュース本文を読んだかのような記述（本文は取得しません）、データ時点がずれている可能性のある比較 |
| `/keep [off\|summary\|claims]` | 他LLM見解の Debate Buffer 保持モードを設定します（既定 `summary`）。引数なしでBuffer内容を表示 |
| `/alert` | アラートルール（使用数／上限16）・監視状態・登録済み通知チャンネルを一覧表示 |
| `/alert on <n>` / `/alert off <n>` | ルール n を有効化/無効化（監視は `serve` 起動中のみ動作）。**この切り替えは保存されません** — 再起動すると有効に戻ります |
| `/alert add <銘柄> <足> <条件> <宛先> [explain]` | 監視ルールを追加（例：`/alert add NVDA 5m rsi<=30 team explain`）。**計算済み値**の条件が成立の立ち上がりでクロスすると、確定値＋閾値の1行通知を宛先へプッシュ。`explain` でガード適用の短いLLM解説を添付 |
| `/alert del <n>` | アラートルール n を削除。`xoksa.env` からも消すので、再起動後も復活しません |
| `/alert test <宛先>` | 疎通確認のテスト送信。メッセージの言語は `xoksa.env` の `LANG` に従います（実際のアラート通知と同じ言語） |
| `/clear` | チャット履歴と Debate Buffer を消去します。確定した指標データは保持されるため、分析は同じ数値のまま続きます。Web UI の 🗑 ボタンも同じコマンドを送ります |
| `/help` | アプリ内のコマンド一覧をカテゴリ別に表示します。この一覧が最新かつ正となる集合です |
| `/bye` | チャットを終了します |

**文脈保持モード（`--chat-memory`）**

| モード | 最大文脈文字数 | 保持ターン数 | 要約最大文字数 | 用途 |
| :--- | :---: | :---: | :---: | :--- |
| `low` | 8,000 | 2 | 800 | 低コスト・簡易な質問 |
| `mid` | 16,000 | 4 | 2,000 | 標準（推奨・既定値） |
| `high` | 32,000 | 8 | 3,000 | 詳細な文脈・高トークン |

> [!NOTE]
> `--chat-memory` は同一実行中の文脈保持量のみを制御します。セッション終了後の永続記憶ではありません。
> 予算が逼迫した場合、最新の質問とガード制約を最優先で保持し、次に直近の会話ターン・分析文脈（base）を確保します。Debate Buffer、ニュース一覧、会話メモは余剰予算がある場合のみ含まれます。
> 環境変数 `CHAT_MEMORY` でも設定可能です（`low`\|`mid`\|`high`）。CLIでの指定が優先されます。

**`/token detail` の見方**

`/token detail` は、チャットモードのコストとプロンプト肥大化を確認するための診断表示です。内訳は**送信前の文字数**であり、課金対象トークンそのものではありません。実際の課金・実トークン数は、各プロバイダーの tokenizer と API 返却値に依存します。次のLLM呼び出しを重くしている原因を探すために使います。

表示例:

```text
=== Token / Context Detail ===
注記: 内訳は送信前の文字数です。実課金・実トークンはプロバイダー側の tokenizer とAPI返却値に依存します。
現在の保持設定: budget=16000 chars / recent_turns=4 / summary_cap=2000 chars
ロード銘柄数: 1

通常チャット候補（/chat の通常質問で主に使う文脈）:
  technical context : 1980 chars
  fundamental       : 329 chars
  news              : 10668 chars
  recent turns      : 10085 chars (4 turn(s))
  summary           : 0 chars
  debate buffer     : 0 chars (mode=off, entries=0)
  guard             : 159 chars
  candidate total   : 23221 chars

特別コマンド:
  /basic full candidate  : 25990 chars
  loaded base context    : 14556 chars
```

| 項目 | 意味 | 大きい場合の対応 |
| :--- | :--- | :--- |
| `budget` | 現在の `/mem` モードで通常チャット送信に使う最大文字数 | コストを抑えるなら `/mem low`。詳細重視時だけ `/mem high` を使う |
| `recent_turns` | 直近会話として保持する user/assistant ターン数 | 古い会話が不要なら `/mem low`、`/sym del`、または新しいセッションで始め直す |
| `summary_cap` | 古い会話を要約へ丸める最大文字数 | 大きい場合は、過去会話の継続性がプロンプトを消費している |
| `ロード銘柄数` | チャットにロード中の銘柄数 | 銘柄が多いほど固定バジェットを分割する。不要な銘柄は `/sym del <番号>` で削除 |
| `technical context` | 通常質問で使うテクニカル指標文脈 | SOTの中核。大きい場合は銘柄数を減らす、またはメモリを下げる |
| `fundamental` | 読み込まれたファンダメンタル表示文脈 | 不要なセッションでは `--fundamental` を使わない |
| `news` | ニュースのタイトル+URL文脈 | 大きくなりやすい。`--no-news`、`--news-count` の削減、`-q` の絞り込み、ニュース不要時は `/reload n` ではなく `/reload t` を使う |
| `recent turns` | 現在プロンプトに保持される直近会話 | 長い会話ほど増える。短い追加質問にする、`/mem low` にする、またはセッションを切り直す |
| `summary` | 古い会話を丸めた要約 | 会話継続性には効くが、プロンプト枠を消費する |
| `debate buffer` | 他LLM回答の非SOT抜粋 | 低コスト重視なら `/keep off`。モデル間批評が必要な時だけ `/keep summary` / `claims` を使う |
| `guard` | 毎ターン注入する必須制約 | 小さいが、安全性・SOT維持のため削らない |
| `candidate total` | 通常チャット候補の切り詰め前合計文字数 | `budget` を超える場合、低優先度セクションは送信前に切り詰めまたは省略される |
| `/basic full candidate` | `/basic` が送るフル初回分析候補サイズ | `/basic` は通常チャットのメモリ上限を使わないため高コスト。必要な時だけ使う |
| `loaded base context` | 銘柄ロード時に保持された分析文脈全体 | 通常チャット文脈を作る元データ。直接すべて送るとは限らない |

上の例では、`candidate total`（23221 chars）が `budget`（16000 chars）を超えています。この場合、通常チャットでは全量は送られず、低優先度セクションが切り詰めまたは省略されます。肥大化の主因は `news` と `recent turns` なので、ニュース件数を減らす、`/mem low` にする、長い会話を切り直す、などが効きます。`/basic full candidate` は通常チャットより大きくなりやすいため、コストを意識して使ってください。

**売買示唆防止レベル（`--chat-guard`）**

毎ターン注入するガード制約の強さを制御します。既定値は `high` です。

| レベル | 制約内容 |
| :--- | :--- |
| `high`（既定） | エントリー価格・利確水準・損切りライン・投資勧誘に相当する表現を禁止。テクニカル・ファンダメンタル水準の客観的な説明のみ許可 |
| `mid` | 参考水準としての提示を許可。投資判断はユーザー自身が行う旨を明記させる |
| `low` | 示唆防止なし。数値創作禁止・通貨単位誤記禁止のみ |

> [!NOTE]
> 環境変数 `CHAT_GUARD` でも設定可能です（`high`\|`mid`\|`low`）。CLIでの指定が優先されます。
> いずれのレベルでも「入力済みデータの外での数値創作禁止」「通貨単位誤記禁止」は維持されます。
> **すべての LLM プロバイダで**（Ollama に限らず）、`--chat-guard` とは独立した整合性ガードが常時実行され、`--no-ollama-guard` / `OLLAMA_NO_GUARD` で無効化できます（`low` を指定しても無効になりません）。モデルが書いた数値を、エンジンが算出した確定値と照合します——同じ銘柄・同じ指標・同じ足・同じ単位と通貨・同じ符号・同じ時点——ので、別の値からの転用、未算出の指標の読み取り値、エントリー/損切り価格、誤った単位変換、逆方向の比較がいずれも検出されます。除外されるのは該当する文だけで、残りの回答は保持されます。検出規則の正典は [security-design.md](../dev-prog/security-design.md) です。

**Debate Buffer モード（`--debate`）**

`--debate` の既定値は `summary` です。現在のチャットセッション中に得たLLM回答の限定抜粋を保持し、別のLLMに批判的検討をさせるための材料として使えます。目的はプロバイダー間のクリティカルシンキングであり、事実の追加ではありません。

| モード | 動作 | トークン影響 |
| :--- | :--- | :--- |
| `summary`（既定） | 最大2件の短い抜粋を保持し、後続ターンへ小さな Debate Buffer として渡す | 低 |
| `claims` | 最大3件のやや長い抜粋を保持し、主張・根拠・リスク・未検証事項の検討を促す | 中 |
| `off` | 他LLM見解を保存・送信しない | なし |

> [!NOTE]
> Debate Buffer の内容はLLMの見解であり、事実ではありません。XOKSA のプログラムで計算した指標、取得済み市場・ファンダメンタルデータ、ロード済みニュースがSOTです。`/crit` は専用の SOT Coverage マニフェスト付きで、保存された直近1件の見解材料だけを現在のプロバイダー/モデルに検討させます。Coverage外の主張は誤りと断定せず、未確認として扱います。`/crit` の回答自体は Debate Buffer に再追加せず、自己参照ループを防ぎます。

**実行例**

```bash
# 標準的なチャット（mid 文脈、high ガード）
xoksa -t 7203.T --chat

# 銘柄なしで起動（セッション内で /sym add で追加）
xoksa --chat

# 低コストモードでチャット
xoksa -t AAPL --chat --chat-memory low

# ファンダメンタル情報も含めた高文脈チャット
xoksa -t NVDA --fundamental --chat --chat-memory high

# 示唆防止を緩和してチャット（自己責任）
xoksa -t TSLA --chat --chat-guard mid
```

---

## 5. ログ・出力設定

| オプション | 説明 | 型 | 既定値 | 環境変数 |
| :--- | :--- | :--- | :--- | :--- |
| `--save-technical-log` | テクニカル分析ログを保存 | bool | false | `SAVE_TECHNICAL_LOG` |
| `--log-format` | ログ形式（`csv` または `json`） | String | `csv` | `LOG_FORMAT` |
| `--log-dir` | ログ保存先ディレクトリ | String | `log` | `LOG_DIR` |
| `--data-append` | 既存のCSVファイルに追記する | bool | false | `CSV_APPEND` |
| `--log-flat` | ティッカー別のサブディレクトリを作成しない | bool | false | `LOG_FLAT` |
| `--stdout-log` | ログを標準出力に書き出す | bool | false | - |
| `--silent` | 標準出力を抑制（エラーのみ表示） | bool | false | - |

### レポート出力調整（LLM）

| オプション | 説明 | 型 | 既定値 |
| :--- | :--- | :--- | :--- |
| `--max-note-length` | 「注意ポイント」の最大文字数 | usize | 300 |
| `--max-shortterm-length` | レポート第2視点の最大文字数（日足では「1週間短期目線」、分足では分足ベースの短期目線、週足/月足では上位足目線） | usize | 150 |
| `--max-midterm-length` | レポート第3視点の最大文字数（日足では「1ヶ月中期目線」、分足では数営業日目線、週足/月足では長めの時間軸） | usize | 150 |
| `--max-news-length` | 「ニュースハイライト」の最大文字数 | usize | 600 |
| `--max-review-length` | 「総評」の最大文字数 | usize | 1000 |

### デバッグ・開発用

| オプション | 短縮 | 説明 |
| :--- | :--- | :--- |
| `--no-alias` | `-a` | 会社名エイリアスの展開をスキップする |
| `--alias-csv` | - | 会社名エイリアス（銘柄リスト）のCSVパス |
| `--debug-prompt` | `-d` | LLM プロンプトをファイル（`debug_prompt_<timestamp>.txt`）に書き出す（`--no-llm` 併用で送信しない・全プロバイダー共通） |
| `--debug-args` | - | 解析済みコマンドライン引数を表示する（デバッグ用） |

---

## 6. エラーメッセージ

XOKSA が表示するエラーメッセージをここに集約します。前半は入力を受け付けなかったチャットコマンドへのエラー（画面では先頭に ❌ が付きます）、最後のグループはデスクトップ接続画面のエラーです。波括弧の部分は実行時に埋まります。

**銘柄とデータ**

| メッセージ | 原因・対処 |
| :--- | :--- |
| `銘柄がロードされていません。` / `銘柄がロードされていません。先に /sym add <銘柄> を実行してください。` | 銘柄が必要なコマンド。`/sym add <銘柄>` で追加する |
| `{} は既にロードされています。` | その銘柄は既に一覧にある |
| `比較銘柄は5件までです。/sym del <番号> で削除してから /sym add <銘柄> を実行してください。` | 5銘柄の上限に達している |
| `/sym add には銘柄コードを指定してください。例: /sym add 9433.T` | `add` に銘柄が指定されていない |
| `銘柄番号を指定してください（1〜{}）: /sym del <番号>` | `del` に番号がない、または範囲外 |
| `/sym のサブコマンドが不明です。/sym del <番号> または /sym add <銘柄> を使用してください。` | `add` と `del` のみ |
| `銘柄番号を指定してください: /funda [1-{}]` / `銘柄番号を指定してください: {}` | 複数銘柄がロードされているため番号が必須 |
| `番号が不正です（有効: 1〜{}。例: 1,3 / 1-{}）` | 番号の書き方が誤り、または範囲外 |
| `/reload t または /reload n を指定してください。` | `t`（テクニカル）か `n`（テクニカル＋ニュース）が必要 |
| `自動リロードは分足モードでのみ利用できます。` | `/auto` は 1m〜60m のみ |
| `/auto notice は銘柄をロードしてから使用してください。` | `notice` は銘柄ごとに1行出すため銘柄が必要 |
| `/auto には on / off / notice またはなし（トグル）を指定してください。` | 引数が不明 |

**ニュース**

| メッセージ | 原因・対処 |
| :--- | :--- |
| `ニュース取得は無効です（NO_NEWS=true）。` | この実行ではニュースが無効 |
| `BRAVE_API_KEY が設定されていません。` / `BRAVE_API_KEY のキーチェーン取得エラー: {e}` | `/news find` には Brave のキーが必要。後者はキーチェーンを読めなかった場合 |
| `/news find の後に検索ワードを指定してください。` | 検索ワードがない |
| `検索ワードは256文字以内にしてください。` | 検索ワードが長すぎる |
| `del の後にスロット番号を指定してください（例: del 3）。` | `del` にスロット番号がない |
| `スロット番号は 1〜16 で指定してください。` | 追加ニューススロットは X01〜X16 |
| `use のオプションが不明です: {}。use <番号> または use all を使用してください。` | `use` はスロット番号か `all` |
| `/news のサブコマンドが不明です。` | 有効なのは `nf` / `find` / `list` / `use` / `del` / `clear`、または番号 |

**分析パラメータ**

| メッセージ | 原因・対処 |
| :--- | :--- |
| `値がありません: /set {key} <値>（一覧は /set）` | キーだけ指定して値がない |
| `値が不正です: {}。有効な値: {}` | 受け付ける値の範囲外 |

**LLM とフォーラム**

| メッセージ | 原因・対処 |
| :--- | :--- |
| `ollama はエイリアスの指定が必須です。例: /llm ollama:gpu1` | ollama は `xoksa.env` のエイリアスで指定する。生のモデル名は不可。`/forum chair ollama:gpu1` も同様 |
| `Ollama インスタンスが設定されていません。xoksa.env に OLLAMA_N_ALIAS / OLLAMA_N_HOST / OLLAMA_N_PORT / OLLAMA_N_MODEL を設定してください。` | エイリアスが未定義 |
| `/forum set には provider[:model] のリストを指定してください。` | `set` に参加者がない |
| `/forum ask には質問を指定してください。` | `ask` に議題がない |
| `参加者が未設定です。先に /forum set を実行してください。` | 参加者が未設定 |
| `/forum: set / ask / sum / log / chair / clear` | `/forum` のサブコマンドが不明 |
| `Debate Buffer が空です。/forum sum には見解データが必要です。` | 総括する材料がない |
| `crit には /keep summary または claims が必要です。` | 保持モードが `off` のため材料が残らない |
| `Debate Buffer が空です。` | `/crit` の対象がまだない |
| `Debate Buffer に別LLM（{}/{} 以外）の見解がありません。` | いま選択中の LLM の見解しかない。AI を切り替えて1回answerを得てから実行する |
| `/keep には off / summary / claims を指定してください。` | 保持モードが不明 |

**アラート**

| メッセージ | 原因・対処 |
| :--- | :--- |
| `使い方: /alert on <ルール番号>` / `使い方: /alert del <ルール番号>` / `使い方: /alert test <チャンネル名>` | サブコマンドに引数が必要 |
| `サブコマンドが不明です。使い方: /alert [list \| on <n> \| off <n> \| add <銘柄> <足> <条件> <チャンネル名> [explain] \| del <n> \| test <チャンネル名>]` | `/alert` のサブコマンドが不明 |
| `足種は 1m\|5m\|15m\|30m\|60m のいずれかです。` | アラートは分足のみを監視する |
| `条件を解釈できません。形式: <指標><比較><数値>、比較は <= >= < >（例 rsi<=30）` | 条件に空白を入れない |
| `「…」という通知チャンネルがありません（xoksa.env の NOTIFY_<n>_NAME を確認してください）。` | チャンネル名が `NOTIFY_<n>_NAME` と一致しない |
| `通知チャンネル #{n} が定義されていません。` | その番号のチャンネルがない |
| `NOTIFY_{n}_SECRET が未設定です（設定フォームまたは --update-key で登録してください）。` | チャンネルの秘密が OS キーチェーンにない |
| `チャンネルの secret が不正です: {s}` | Webhook のパスまたは Bot トークンが拒否された |
| `キーチェーンの読み取りに失敗しました: {s}` | OS キーチェーンを読めなかった |
| `送信に失敗しました: {s}` | チャット基盤が拒否した、またはネットワーク障害 |

**その他**

| メッセージ | 原因・対処 |
| :--- | :--- |
| `/mem には low / mid / high を指定してください。` | 保持量の指定が不明 |
| `/token には detail のみ指定できます。` | `/token` は `detail` か引数なし |
| `/status に引数はありません。` | `/status` は引数を取らない |
| `⚠️ キャンセルしました。終了する場合は /bye または Ctrl+C` | ターン中の Ctrl+C で中断。それまでに届いた分は保持される |

**デスクトップ接続画面**

デスクトップアプリの接続画面で「ローカルエンジンを自動起動する」が失敗したときに、UI の言語で表示されます。

| メッセージ | 原因・対処 |
| :--- | :--- |
| `エンジンを起動できませんでした: {0}` | エンジンバイナリを起動できない。`xoksa` はデスクトップ実行ファイルの隣に必要（再インストールで復旧） |
| `エンジンが起動直後に終了しました（{0}）。` | 起動した子エンジンが即終了（終了ステータスを表示）。ターミナルで `xoksa serve` を実行するとエラー内容を確認できる |
| `ポート {0} は認証が必要な別の xoksa エンジンが使用中です。` | LAN アクセス有効時のみ：固定ポートをトークン必須のエンジンが占有。停止するか、ポートを変更 |
| `ポート {0} は別の xoksa エンジン（v{1}）が使用中です。` | LAN アクセス有効時のみ：固定ポートを別の xoksa エンジンが占有。アプリは自分が起動していないエンジンには接続しない。停止するか、ポートを変更 |
| `ポート {0} は別のプロセスが使用中です。` | LAN アクセス有効時のみ：xoksa 以外のプロセスが占有。ポートを変更 |
| `空きローカルポートを確保できませんでした。` | OS がエフェメラルポートを割り当てられない（極めて稀）。ファイアウォール／TCP 設定を確認 |
| `エンジンがアクセストークンを要求しています（未設定または不一致）。` | エンジンが 401 を返した。相手の設定アプリで発行したトークンを入力 |
| `ローカルエンジンが時間内に準備できませんでした（最終状態: {0}）。` | 10秒のタイムアウト。最後に観測した状態を表示 |

---

## 7. 付録：優先順位の定義

各設定値は、以下の優先順位に従って決定されます。

1. **コマンドライン引数 (CLI)**: 指定がある場合、最優先されます。
2. **環境変数 (xoksa.env / shell)**: CLIでの指定がなく、環境変数が定義されている場合に採用されます。
3. **ハードコードされた既定値**: 上記のいずれも指定がない場合に適用されます。

> [!IMPORTANT]
> `xoksa.env` は**正準のユーザーパス**（`<config_dir>/xoksa/xoksa.env`）から読み込まれ、カレントディレクトリからではありません。`xoksa --init` がそこへ書き込み、`--env-file <path>` で別ファイルを指定できます。初回に実行フォルダの `./xoksa.env` があれば正準の場所へ一度だけ取り込まれます。

---

## 免責事項

本ツールおよび本リファレンスに記載された情報は、情報提供のみを目的としており、投資勧誘を意図したものではありません。本ツールの分析結果を用いた投資判断は、利用者自身の責任において行ってください。本ツールの利用により生じたいかなる損失についても、開発者は一切の責任を負いません。
