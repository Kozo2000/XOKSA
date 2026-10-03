# xoksa — Security, Reliability & Shipping Inspection

[日本語はこちら。](#ja)

> **Language sync:** English and Japanese sections are both authoritative. If a finding, control, recommendation, or behaviour appears in only one language, treat it as applying to both; the missing side is a documentation defect.

> **Point-in-time report.** This is the single consolidated record of xoksa's **reliability** (where data comes from, how it fails, what is reproducible), **security assessment** (static review + dynamic playbook + findings), and **shipping inspection** (the frozen-artifact evidence for a release). It is a snapshot — re-run it when the server/core surface or a shipped artifact changes. The *design* (threat model, CIA controls, data classification, desktop §6) lives separately in [security-design.md](./security-design.md).

> **Scope.** The `xoksa` CLI / server binary: the analysis engine, the `xoksa serve` axum HTTP server + JSON API, the static Web UI, and the data/LLM providers it drives. External services (LLM providers, Brave, Yahoo/Stooq, SEC/J-Quants) are assessed only at their trust boundary. The **desktop app** (`xoksa-desktop`, Tauri) is a *separate artifact* with its own surface (OS WebView, Tauri IPC); its model is in [security-design.md §6](./security-design.md) and it is inspected against its own build when it enters the release process.

---

# Part A — Reliability & Reproducibility

Where xoksa gets its data, how it behaves on failure, and what reproducibility it guarantees — the basis for auditing and trusting a result. Indicator math is in [analysis-guide.md](../manual/analysis-guide.md); the SOT philosophy in [design-philosophy.md](./design-philosophy.md).

## A.1 Data sources and priority

| Domain | Source | Notes |
| :--- | :--- | :--- |
| Market data (OHLCV) | Primary: Yahoo Finance-compatible public endpoint (`query2.finance.yahoo.com`, alternate host `query1.finance.yahoo.com`). Fallback: Stooq (daily only) | Primary is Yahoo; Stooq is a labelled fallback used only on genuine Yahoo failure (see A.2). **Unofficial endpoint** — see the Disclaimer in [README](../../README.md). Provider construction is centralized (`build_price_fetcher`, [design-philosophy §8.1](./design-philosophy.md)) so a source can be swapped/added without touching call sites. |
| Fundamentals (JP) | J-Quants | Auth: `JQUANTS_API_KEY` (v2 only; V1 token auth retired 2026-06-01). |
| Fundamentals (US) | SEC EDGAR | Identified by `SEC_USER_AGENT` (app name + email, required by SEC). |
| News | Brave Search API | **Titles and URLs only**; article bodies are never fetched, stored, or sent to the LLM. |

**Market detection:** a `.T` suffix or a recognized JP code routes to Japan; otherwise the ticker is treated as US (`detect_market`). Unsupported symbols are reported, not guessed.

## A.2 Failure handling and fallback

- **Market data:** up to 2 attempts (`MARKET_DATA_FETCH_ATTEMPTS`), 10-second timeout, retried on HTTP 429/5xx/timeout/connection error. Two layers:
  - **Same-vendor resilience (identical numbers):** primary host `query2.finance.yahoo.com`; on retry exhaustion it retries `query1.finance.yahoo.com`. Both serve the **same** Yahoo data — pure availability, not a different source.
  - **Fallback vendor (degraded, labelled):** if Yahoo fully fails, the fetch falls back to **Stooq** (a genuinely different vendor). Stooq's free feed here is **daily-only**: it serves the daily timeframe and returns an error for intraday/weekly/monthly rather than substituting daily bars — an intraday request during a Yahoo outage gets *no* data, never *wrong* data. A snapshot adopted from Stooq is **clearly labelled** with one warning line: `⚠️ Fallback data source in use: …` / `⚠️ 代替データ元を使用: …`. A successful Yahoo fetch always wins.
  - On exhaustion of both, the error is surfaced — values are never fabricated. A snapshot is **never spliced across providers**; it always comes whole from exactly one source (SOT). The failover improves availability without weakening SOT — fallback data is labelled, not silently mixed.
- **LLM (cloud):** up to 3 attempts (`MAX_RETRIES`) on HTTP 429/503, honoring `Retry-After`. Claude additionally retries without `temperature` if it is rejected (400) — driven by the API response, not model-name hardcoding. `--no-llm` skips the LLM and returns the computed analysis only.
- **News:** a Brave failure surfaces an error. News is supplementary; `--no-news` proceeds on technical + fundamental data alone.
- **Numeric safety:** non-finite values are filtered at the parse boundary (`is_finite()`); zero-range (`high == low`) and zero-divisor cases yield safe neutral values, never `NaN`/`Inf` (see [security-design §1](./security-design.md)).

## A.3 Reproducibility and determinism

- **Single-execution calculation:** each indicator is computed once per run; the identical values feed the screen, the log, and the LLM prompt (Source of Truth).
- **Cache-free analysis:** every run fetches a fresh dataset; the analysis never computes from a stored cache. (Backtests fetch their bars fresh too.) Re-running later differs because the market moved, **not** because the logic changed.
- **Deterministic given identical input:** for a fixed input bar series + configuration, indicator values and scores are deterministic — all math is finiteness-checked, range-clamped `f64` (`ta` crate for basics, transparent proprietary formulas otherwise). **One deliberate exception — presentation order:** equal-weight indicators are ordered **randomly per run** (so the lead indicator is never a hardcoded bias); that order feeds screen and prompt, but never changes a computed value.
- **Data freshness is explicit:** `market_data_latest_time` (latest obtainable bar) is reported separately from `analyzed_at` (run time); their gap is shown, not hidden.
- **LLM output is NOT deterministic:** a natural-language explanation of the deterministic SOT data, it may vary run to run and never alters computed values. Forecasts (`/cast`), when enabled, are labelled projections grounded on those values, not new confirmed data.

## A.4 Version impact on results

Indicator formulas, thresholds, and defaults are stable within a minor line; any change that can affect computed values is recorded in [CHANGELOG.md](../../CHANGELOG.md) and summarized per minor version in [version-history.md](./version-history.md). Two runs of the same version at different times are expected to differ (live data); two runs on the **same fetched input** are expected to match (LLM prose aside).

---

# Part B — Security Assessment

## B.1 Threat model

The server binds `127.0.0.1` by default, with **no authentication and no TLS** (a documented gate — see security-design). The realistic attacker for the loopback default is **a malicious web page open in the same browser** (drive-by CSRF against `localhost:8787`) and **malicious upstream data** (news titles/URLs, market feeds). `--host 0.0.0.0` widens the surface to the private network, so that mode is **fail-closed**: it always runs with an auth token active — the engine auto-generates a 256-bit one on first non-loopback start if none exists — it answers only private/loopback source addresses (`403` otherwise) and refuses an unauthenticated request with `401`. TLS and multi-tenant rate-limiting stay out of scope by design for a single-user tool, and keeping the exposed mode inside a trusted private network remains the maintainer's explicit responsibility.

## B.2 Attack surface

| Endpoint | Method | Attacker-controlled input | Controls in place |
| :--- | :--- | :--- | :--- |
| `/`, fallback static | GET | request path | `read_asset` allows only `Component::Normal` (rejects `..`, absolute, prefix) |
| `/api/health`, `/api/config`, `/api/llm/options` | GET | none / env | return only version/labels/key-**presence** — never secret values |
| `/api/symbol/{symbol}/{summary,news,chart}` | GET | `{symbol}`, query | `sanitize_ticker` (alnum/`.`/`-`, uppercased); `bars` clamped `2..=300` |
| `/api/analysis/{context-pack,multi-timeframe}` | POST | JSON body | `Json<T>` enforces `application/json`; multi-timeframe **runs the LLM** (billable) |
| `/api/backtest`, `/api/backtest/rules`, `/api/llm/select` | POST | JSON body | `Json<T>`; rule save: `name` ≤80, `spec_json` must parse + ≤32 KiB, path fixed |
| `/api/chat/{stream (SSE), commands}` | GET | `message` query | **explicit same-origin check** (`cross_origin_rejected`) + `MAX_CHAT_MSG_CHARS = 8000` |

**Global request controls (middleware).** Outermost, a **connection-admission semaphore** sheds requests over `MAX_CONCURRENT_CONNECTIONS` with `503` before any work (`limit_concurrency`). Every response carries a per-client **`xoksa_sid`** cookie — 32 hex, `HttpOnly`, `SameSite=Strict`, no `Secure` (loopback http) — and Web session state is keyed `sid|symbol|timeframe`, so **one browser's session can never read or mutate another's**. Response headers add a strict, nonce-based **CSP** and a **`Permissions-Policy`** denying geolocation/camera/microphone/payment/USB/motion sensors.

## B.3 Findings

Severities reflect the **loopback default** unless noted. All confirmed against the actual code.

### 1. Content-Security-Policy — Medium (defense-in-depth) — **RESOLVED (strict, v2.2.2)**
Originally (v2.2.1) no CSP: popups used inline styles/handlers, so XSS defence rested entirely on output-encoding. **Fixed:** popups fully de-inlined (SVG presentation attributes, linked CSS, delegated listeners) and the WASM bootstrap `<script>` carries a per-request nonce, so the policy drops `'unsafe-inline'` entirely: `default-src 'self'; script-src 'self' 'wasm-unsafe-eval' 'nonce-{per-request}'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'`. A `Permissions-Policy` was added alongside.

### 2. No request body-size limit; unbounded saved-strategies file — Medium (exposed) / Low (loopback) — **RESOLVED**
A non-browser client or `--host 0.0.0.0` could POST `application/json` directly and inflate the strategies file / burn CPU (O(n) rewrite per save). **Fixed:** global `DefaultBodyLimit::max(64 KiB)`; `save_rule` caps `spec_json` at 32 KiB; saved rules capped at 200 (oldest evicted).

### 3. State-changing POSTs not explicitly origin-checked — Low (defense-in-depth) — **RESOLVED**
Not an exploitable browser-CSRF hole (every such handler uses `Json<T>`, which requires `application/json` → CORS preflight the server never answers; `text/plain` → 415). **Hardened:** the `reject_cross_origin_mutations` middleware rejects cross-origin POSTs server-side, so the browser preflight is no longer the sole control.

### 4. Saved-strategies path is fixed — Info (safe)
Path is the fixed `~/.xoksa.strategies.json`; the rule `name` is only a JSON field value, never used to build a filesystem path. No path injection.

### 5. One slow chat turn stalled the summary endpoint — High (availability) — **RESOLVED (v2.2.2)**
Web session state was held behind a single global mutex locked across the LLM stream, so a long chat turn blocked unrelated requests (`/summary` observed hanging ~90 s) — a self-inflicted DoS. **Fixed:** each session sits behind its own `Arc<Mutex<WebSession>>`, reads use `try_lock`; the global map is never held across I/O.

### 6. Cross-client session isolation — Medium (confidentiality/integrity) — **RESOLVED (v2.2.2)**
Session state was keyed without a per-client id, so two browsers on the same symbol/timeframe could share one session (language/mode bleed). **Fixed:** a random `xoksa_sid` cookie (`HttpOnly`, `SameSite=Strict`) namespaces the key to `sid|symbol|timeframe`.

### 7. Prompt injection via news titles into the LLM prompt — Medium — **RESOLVED (v2.3.x, found & fixed here)**
News is attacker-influenceable (anyone can publish an article), and the threat model already names malicious news titles/URLs as an upstream vector. The provider's title and URL were inserted into the LLM analysis/chat prompt **raw** (`news.rs`, `NewsItem::prompt_line`) — parsed straight from the API with no sanitisation. A crafted title could inject newlines to break the prompt's line structure, **forge structural markers** (`=== 比較対象 ===`, `【制約】`, `---`, code fences) to spoof a system-authored section or constraint, embed instruction-override text, or exhaust the token budget.
**Fixed — layered:**
1. **Ingestion-boundary sanitisation (input).** At the single point where an `Article` is built from the provider response (`news.rs`), the title/date are flattened to a single line with control chars stripped, structural markers neutralised, and length-capped (title 240 / date 40); the URL is kept only if it is a single-line `http(s)` scheme (others dropped). One choke point → every downstream path (CLI analysis, chat, dashboard) is clean, consistent with SOT (no CLI/GUI fork).
2. **Output-integrity guard generalised to every provider (output).** Confirmed values live in the prompt, so any out-of-input number, bare price range, independent trade level, or reversed comparison in the output is a hallucination or an injection effect — detected and the offending sentence/line excluded. Previously Ollama-only; now applied uniformly (`send_chat_turn_with_usage` dispatcher + the one-shot analysis senders) to **openai / gemini / claude / ollama**, gated by `--no-ollama-guard` / `OLLAMA_NO_GUARD`. *(Since v2.9.7 the reference is no longer the prompt: it is built from `TechnicalDataGuard` / `FundamentalData` and passed alongside the request, precisely because a prompt also carries the conversation and other models' words — see [security-design.md §1](./security-design.md). That strengthens this control; the layering described here is unchanged.)*
3. **Structural placement (existing).** The constraint block sits **after** the untrusted body (recency) and is **budget-reserved** (a long title cannot push the guard out of the prompt); a boundary note tells the model the news is titles+URLs only.
Deterministic tests cover forged-marker / newline / control / length titles, URL-scheme filtering, and the guard applying to a non-Ollama provider. Combined with SOT (confirmed values are rendered in Rust, not by the LLM), the blast radius of any surviving injection is limited to the LLM's *prose*, never the authoritative numbers.

### 8. A failed notification put the webhook secret in the log and in the browser — Medium (confidentiality) — **RESOLVED (v2.9.10, found & fixed here)**
For Slack, Discord and Google Chat the secret **is** the webhook URL: the user supplies the whole thing, and whoever holds it can post to their channel. A transport-level failure was reported by printing `reqwest`'s error, and that error's `Display` ends with ` for url (…)` (`error.rs:300`). Measured with a canary token and an unreachable proxy: `error sending request for url (https://hooks.slack.com/services/T…/B…/LEAKCANARY123): client error (Connect): …`. The alert monitor wrote that line to the diagnostic log, and `/alert test` returned it to the dashboard as JSON — forbidden by [security-design.md](./security-design.md) §0.1 (a Class A secret is never logged) and §4 (no credential crosses the browser boundary). A connect failure, a timeout, a TLS error or a DNS failure all take this path, so the trigger is an ordinary misconfiguration, not an attack. **Fixed** at the notifier, the one place every caller goes through: the message is rebuilt from the error's classification (`is_connect` / `is_timeout` / `is_decode` / `is_body`) plus its source chain, neither of which carries the URL, and the secret is then scrubbed from whatever text is about to be shown — which also covers the LINE bearer token and a platform error body that echoes what was sent. The cause survives (`notification failed: connection failed: … Connection refused (os error 61)`). Two tests: the measurement inverted into a regression test that also asserts the cause is still named, and a direct test of the redaction. Found while implementing the send-failure visibility the same release added — surfacing that failure in the dashboard is exactly what would have published the secret.


## B.4 Verified safe (with the defence)

- **Path traversal / LFI** — `read_asset` rejects any non-`Component::Normal` path before I/O; the embedded fallback gets the same pre-checked `rel`; tests cover `..`, nested `..`, absolute paths.
- **Command injection** — zero `Command::new`/`std::process::Command`/shell-exec in `src/` (engine); `sanitize_ticker`/`sanitize_news_query` also strip `;` `|` `` ` ``. *(The desktop app's `open_external` uses `std::process` with the URL as a single non-shell argument, restricted to http(s); assessed with the desktop artifact.)*
- **XSS** — user/external strings (news title/URL, chat, analysis text, company name) render via Leptos escaped text nodes or `analysis_html`/`html_escape`, not raw `innerHTML`. News URLs pass `safe_http_url` (only `http(s)` become links; `javascript:` blocked) with `rel="noopener noreferrer"`. Penetration testing found one exception — the chart-popup `set_inner_html` interpolated the ticker unescaped (reflected XSS via the ticker input) — **fixed** by `html_escape`ing the symbol in `chart_doc_html` (`html_escape` now also escapes `'`); the popup message receiver was later removed entirely (E3).
- **SSRF** — outbound hosts are hardcoded literals (`query{1,2}.finance.yahoo.com`, `stooq.com`, `api.search.brave.com`); the symbol is `sanitize_ticker`-restricted and `urlencoding::encode`d, so it cannot inject a host/path/authority.
- **Secret / info disclosure** — keys never appear in responses; `/api/llm/options` reports key **presence** only; error notes reference key **names**, not values; keys live in the OS keychain and are zeroized.
- **SQLi** — no SQL sink (the database was removed; persistence is a flat JSON file only).
- **Dependency SCA** — `cargo audit` (native) and `osv-scanner` (native + `webui-leptos`) report **zero vulnerabilities**; CI additionally gates on `cargo deny` (advisories + license compliance + crates.io-only source allowlist + duplicate/ban) across both workspaces. Release builds use `--locked` and `cargo auditable` (the shipped binary embeds its dependency list, recoverable with `cargo audit bin`). Two RustSec *unmaintained* INFO advisories remain on webui **build-time proc-macros** (`paste` RUSTSEC-2024-0436, `proc-macro-error2` RUSTSEC-2026-0173, pulled transitively by leptos) — **not vulnerabilities**, compile-time only, **not in the shipped WASM**, removable only once leptos migrates upstream.

## B.5 Dynamic-testing playbook (Phase 3)

Application-layer testing of `xoksa serve` — endpoints, inputs, request policy, responses, client surface. **Exclude LLM-triggering endpoints (`/api/chat/stream`, `/api/analysis/multi-timeframe`) from automated scanners** (provider cost / rate-limits). Test build: `xoksa serve --private --port 8787`. A diverging result is a regression to file.

```bash
# App response headers — strict CSP (NO 'unsafe-inline'); Permissions-Policy + session cookie
curl -sI http://127.0.0.1:8787/ | grep -iE 'content-security|permissions-policy|x-frame|x-content|referrer|set-cookie'
#   expect: script-src carries a per-request 'nonce-…' and NO 'unsafe-inline'; Set-Cookie: xoksa_sid=<32 hex>; HttpOnly; SameSite=Strict

# Session isolation (K1): two clients get distinct sids
curl -s -c a.jar -o /dev/null http://127.0.0.1:8787/ ; curl -s -c b.jar -o /dev/null http://127.0.0.1:8787/
grep -h xoksa_sid a.jar b.jar   # expect two DIFFERENT 32-hex ids

# Availability (K2): summary stays responsive while a slow chat turn runs
# Capacity shed (K3): past MAX_CONCURRENT_CONNECTIONS the server returns 503, not a hang

# Path traversal (expect 404 for all)
for p in '../Cargo.toml' '..%2fCargo.toml' '%2e%2e%2fsrc/main.rs' '..\Cargo.toml' '/etc/passwd' '%00'; do
  curl -s -o /dev/null -w "%{http_code}  $p\n" "http://127.0.0.1:8787/$p"; done

# CSRF / cross-origin mutation
curl -s -o /dev/null -w "text/plain -> %{http_code}\n" -X POST http://127.0.0.1:8787/api/backtest/rules \
  -H 'Content-Type: text/plain' -d '{"name":"x","spec_json":"{}"}'                    # expect 415
curl -s -o /dev/null -w "cross-origin POST -> %{http_code}\n" -X POST http://127.0.0.1:8787/api/backtest/rules \
  -H 'Content-Type: application/json' -H 'Origin: http://evil.example' -d '{"name":"x","spec_json":"{}"}'  # expect 403
curl -s -o /dev/null -w "cross-origin GET -> %{http_code}\n" -H 'Origin: http://evil.example' \
  'http://127.0.0.1:8787/api/health'                                                  # expect 200 (only POST is gated)

# DoS / body limit (a large VALID rule; application/json = a non-browser client)
python3 -c 'print("{\"name\":\"a\",\"spec_json\":\"{\\\"x\\\":\""+"A"*5000000+"\\\"}\"}")' > big.json
curl -s -o /dev/null -w "big rule -> %{http_code}\n" -X POST http://127.0.0.1:8787/api/backtest/rules \
  -H 'Content-Type: application/json' -H 'Origin: http://127.0.0.1:8787' --data @big.json  # expect 413

# Prompt injection (H1/#7): a crafted news title cannot forge structure — verified deterministically
cargo test --quiet news_title_is_single_line_no_forged_markers output_guard_applies_to_every_provider

# Dependencies
cargo audit && cargo deny check
```
For Burp/ZAP: scope to static assets + GET endpoints; **exclude** the LLM/analysis routes from active scanning.

## B.6 Status

No exploitable vulnerability remains in the loopback default. Availability + cross-client-isolation defects (Findings #5–#6), the news→LLM prompt-injection gap (#7), and the notification-failure secret leak (#8) are fixed and covered by tests. The `--host 0.0.0.0` mode is **fail-closed**, not open: it always runs token-gated (a 256-bit token the engine auto-generates on first non-loopback start), accepts only private/loopback source addresses, and is kept inside a trusted private network — never the raw public internet. TLS and multi-tenant rate-limiting remain out of scope by design for a single-user tool, and the exposed deployment is still the maintainer's responsibility.

## B.7 Desktop app (Tauri) surface — v2.3.x

The desktop app (`xoksa-desktop`) wraps the same engine + dashboard in an OS WebView and adds a **Tauri IPC** boundary. The dashboard is served from the loopback origin (`127.0.0.1:8787`) — the same attacker-influenceable content (news, LLM output) as the browser surface — so the IPC design is least-privilege: only the two windowing/link commands are reachable from that remote origin.

| # | Surface | Criterion | Verdict |
|---|---|---|---|
| DT1 | IPC least-privilege (remote dashboard origin) | the high-risk remote origin can invoke only safe commands | **PASS** — the remote grants (`allow-open-popup-window`, `allow-open-external`) expose ONLY `open_popup_window` + `open_external`. `apply_onboarding` / `load_config` / `list_ollama_models` / `restart_app` / `open_manual` / `setup_status` have **no remote ACL** → not callable from the dashboard (app commands need an explicit permission for a remote origin). |
| DT2 | `open_external` command injection | no shell-metachar execution | **FOUND & FIXED** — the Windows path was `cmd /C start "" <url>`; a URL with `&` `|` `^` (query strings) is a cmd separator and Rust does not quote a space-less arg → command injection. Fixed: `rundll32 url.dll,FileProtocolHandler <url>` (no shell parsing). Restricted to `http(s)`; macOS `open` / Linux `xdg-open` pass the URL as a single non-shell arg. |
| DT3 | `open_popup_window` target | loopback only, no injection | **PASS** — host/port hardcoded (loopback); `kind` filtered to ASCII alphanumerics; symbol/tf/lang percent-encoded into the query. |
| DT4 | plugin surface | no dangerous capability | **PASS** — no `tauri-plugin-shell` / `-fs` / `-http`; the capability grants `core:default` + the two app commands only (no arbitrary shell/fs/http). |
| DT5 | API-key entry | keys never over HTTP | **PASS** — `apply_onboarding` (local-only) forwards keys to the engine over the child process's stdin (`xoksa apply-config`), which writes the OS keychain; keys never touch the HTTP server or a process argument, and the desktop zeroizes its JSON buffer after the write; `load_config` returns non-secret config only. |
| DT6 | popup windows | least-privilege + event channel | **PASS (low residual)** — `popup-chart` / `popup-help` get `core:event:default` only (no window/fs/shell). The `chart-brush` / `help-pick` events carry brushed-interval / command data into the main window's chat input; the content is our own loopback UI and the payloads are chat-input data subject to SOT + the output-integrity guard. |
| DT7 | remote origin over-grant | remote scoped to what it needs | **RESOLVED (v2.3.x)** — the main window's access was **split**: a local `default` capability (`core:default`, for the app-origin onboarding form) and a separate remote `dashboard` capability granting the loopback origin **only** `open_popup_window` + `open_external` + `core:event:default`. The high-risk remote origin no longer receives `core:default`. Verified at runtime: chart/help/external + brush→chat + the settings form all still work. |
| DT8 | env-value injection | onboarding values cannot inject env lines | **RESOLVED (v2.3.x, local-only)** — a `clean_env_value` helper strips control chars and caps length on every user-supplied value written to `xoksa.env` (cloud model names, Ollama alias/host/port/model, and `SEC_USER_AGENT` via `format_env_assignment`), so a pasted newline cannot inject a second env line. |
| DT9 | onboarding-form CSP (hardening) | defense-in-depth on the key-entry page | **HARDENING (deferred)** — `tauri.conf.json` sets `csp: null`; the app-origin onboarding form has no CSP. It renders no external/user-controlled content (no current XSS sink) but handles key entry. Deferred: a working CSP is finicky with the form's inline script + the Tauri IPC allowances, and the current benefit is low. |

**Conclusion.** The desktop's IPC surface is least-privilege for the high-risk remote origin (only windowing + external-link commands + the event channel reach it), key handling matches the design (IPC → keychain, never HTTP), and no shell/fs/http plugin is enabled. One real defect was found and fixed (DT2, Windows `open_external` command injection); DT7 (remote least-privilege split) and DT8 (env-value sanitisation) were implemented and verified. DT9 (onboarding-form CSP) is deferred. The binary-bound desktop inspection (signed/notarized artifact, hashes, VirusTotal) is recorded in Part C at ship-time.


## B.8 OWASP API Security Top 10 (2023) mapping — v2.9.10

A static review of `src/` against the 2023 list, first made 2026-09-29 and **re-read against the code on 2026-10-01**, after the fixes that review prompted landed. Two verdicts moved (API7, API10) and one improved (API4); the rest are unchanged. "By design" below means a scope decision recorded in [security-design.md §0.2](./security-design.md), not an unexamined gap.

| | Item | Verdict | Basis |
|---|---|---|---|
| API1 | Broken Object Level Authorization | **PASS** | Session state is namespaced by the `sid` from an `HttpOnly` cookie, and `parse_session_cookie` accepts **only 32 hex characters** (`src/server/mod.rs`). No endpoint takes a `sid` from a body or a query. |
| API2 | Broken Authentication | **PARTIAL** | Constant-time compare, a 256-bit OS-RNG token, `HttpOnly` / `SameSite=Strict`. The cookie carries **the token itself** — by design (§0.2). `/auth` has no attempt throttling. |
| API3 | Broken Object Property Level Authorization | **PASS** | `/api/config` returns UI vocabulary and defaults only; keys appear as **presence**, never value (`resolve_key_presence`). No key leaves the process. |
| API4 | Unrestricted Resource Consumption | **PARTIAL** (was: effectively none) | 64 KiB body cap; the admission cap is now **256 and sheds with `503`** instead of `u16::MAX`, which nothing reached before memory and file descriptors did; outbound client timeouts are capped at 15 minutes, so a configured value cannot hold a permit indefinitely. Still no per-handler timeout. **Measured on a shipped artifact at v2.9.10 (Windows), at the real ceiling rather than a lowered one:** 192 held requests answer `200`, 256 answer `503`, releasing half restores `200`. Earlier releases could only demonstrate the shed on a build with the cap set to 2 — a request must be *admitted* to hold a permit, and the obvious load generators throttle the headers too, so the connections never became requests. |
| API5 | Broken Function Level Authorization | **N/A** | There are no roles — only authentication. A client holding the token can do everything. That is the single-user design (§0.2), not a defect. |
| API6 | Unrestricted Access to Sensitive Business Flows | **PARTIAL** | `/api/alerts/test` has no throttle, so a session can post to the user's own channel repeatedly. The monitor's own rate control (one send per rising edge) does not cover this path. |
| API7 | Server Side Request Forgery | **PASS** (was: partial) | Outbound hosts are a fixed per-platform allowlist with `https` enforced (`src/notify.rs`), and the browser cannot steer a destination. The gap this review found — **redirect following took the request off the allowlist** — is closed: the notifier, the LLM paths and Ollama refuse redirects outright, market and news follow at most three. |
| API8 | Security Misconfiguration | **PASS** | Nonce-based strict CSP (no `unsafe-inline`, `connect-src 'self'`), `nosniff`, `X-Frame-Options: DENY`, `no-referrer`, Permissions-Policy; server-side origin checks on state-changing POSTs; an explicit same-origin check on the SSE path; the `Json<T>` content-type gate. |
| API9 | Improper Inventory Management | **PARTIAL** | `/api/health` reports the service name and version and the desktop requires an exact match. There is no version in the path and no machine-readable spec — by design (§0.2) for a pair shipped together. |
| API10 | Unsafe Consumption of APIs | **PARTIAL** (was: none) | Every external client has a timeout, and the §1 output-integrity guard goes beyond what this item asks of an LLM response. Redirects are now bounded (API7). Response size is capped **on the Ollama path only** — plaintext HTTP to a user-configured host — because that is where a reply can come from something other than the intended server; the TLS paths to hosts fixed in code are deliberately untouched. |

**What this mapping does not cover.** It is a source review, not a dynamic test — the dynamic evidence per release is in Part C. Two of the three open questions this review listed are now answered: `reqwest` does **not** strip `x-api-key` across a cross-host redirect (measured against the crate's `remove_sensitive_headers`, which is why API7's fix also closed a key-leak path), and ZAP has been run against the shipped artifacts of every release since — most recently v2.9.10 (Windows), 8 alerts and 0 High, with each alert's basis measured on that artifact (C.4). The third stands: API3 was confirmed on `/api/config` and the key-presence path, not by sweeping every handler's response fields.

---

# Part C — Shipping Inspection (frozen-artifact evidence)

A shipping inspection binds to **one frozen local artifact by hash**: build once → inspect that exact file → ship that file. Source-level checks (`fmt`/`clippy`/tests/`audit`) analyse source/deps only and write nothing into the release binary, so the inspected hash stays fixed while they run. The method is stable across releases; the measured hashes/results below are **per release**.

## C.1 Method (per release)

| Stage | Checks |
| :--- | :--- |
| Static / source | `cargo fmt --check` (no diff); `cargo clippy -- -D warnings` (0); `cargo test` (0 failed); `cargo audit` / `osv-scanner` / `cargo deny` (0 vulnerabilities) |
| SOT gate | csv == json == api (output equivalence) |
| Runtime | `--version`; `serve` binds loopback + `health` 200; strict CSP + `xoksa_sid` cookie present; CSRF/oversize/Content-Type gates (403/413/415); CLI analysis non-empty exit 0; backtest API valid JSON |
| Penetration test | the A1–K3 matrix (below), measured against the frozen binary — the runtime counterpart of the Part B findings |
| External | OWASP ZAP passive scan; VirusTotal on the shipped hash; SBOM (CycloneDX via `cargo auditable` + syft) |

**Build provenance (not a reproducible recipe — MSVC is not bit-reproducible; the WASM frontend is deterministic **per runner**, not across operating systems):** `cargo build --release` (default `embedded-ui`, self-contained, **no `cargo auditable` data baked in** — that is used only to *generate* the SBOM). `[profile.release]`: `strip = true`, `lto = true` (fat), `codegen-units = 1`.

**What "deterministic" covers for the WASM, measured.** Two runs of the same workflow on the same runner produce byte-identical output — measured at v2.9.10 on macOS, where two builds of the same effective source gave the same four hashes. Two *operating systems* do not: 2.9.10's macOS frontend is `xoksa-webui-2fc5f86e0258a0ca` (`4aa431b1…`, 710,967 B) and its Windows frontend is `xoksa-webui-22fbb44a3ccaa34a` (`983596d5…`, 724,577 B), from the same source and the same `webui-leptos/Cargo.lock`. This is stated because a reader comparing the two §C.4 records would otherwise read the differing hashes as a contradiction: there is **one** WASM inventory, because the dependency set is host-independent, and **two** WASM binaries, because the builds are not.

**Release build recipe — stated once here, so a §C.4 record names its commit, host and hashes instead of restating the steps.** Restating them is how a step was lost: v2.9.2's record said "`cargo build --release` in `xoksa-setup` staged as the second sidecar", and from v2.9.3 through v2.9.8 five records shortened that to "`xoksa-setup` staged as the second sidecar" — after which a v2.9.9 build staged a 2.9.8 settings binary, because nothing in the written recipe said to build it.

1. `cd webui-leptos && trunk build --release` — the WASM frontend the engine embeds.
2. `cargo build --release --bin xoksa` — the engine.
3. `cd xoksa-setup && cargo build --release` — the settings app. **Building it is part of the recipe, not only staging it:** it is a separate workspace that a root-level build never touches, and it ships inside the installer as the second sidecar.
4. `scripts/bundle-engine.sh` — stages **both** binaries as Tauri sidecars (`bundle.externalBin`). It resolves and checks both before copying either, and refuses to stage a binary that does not report the version in `Cargo.toml`, so a forgotten rebuild fails the bundle instead of shipping.
5. `cd xoksa-desktop && cargo tauri build` — the installer (with `APPLE_SIGNING_IDENTITY` set on macOS).

**From v2.9.9 the release build runs on CI, not on a developer machine.** `.github/workflows/release-windows.yml` performs steps 1–5 above on a `windows-latest` runner and, before uploading, runs `scripts/check-no-host-paths.sh` over every artifact. The reason is §C.4's v2.9.9 record: rustc bakes each crate's source path into the binary for its panic messages, so a developer-machine build ships the builder's account name, and neither `strip = true` nor `--remap-path-prefix` removes all of it. A local build is still fine for development; it is no longer what gets inspected or shipped.

**Start the inspected engine from a directory that contains no `webui-leptos/dist`.** `--web-dir` defaults to that **relative** path, and `read_asset` prefers an on-disk file over the binary's embedded copy — intended for development, but it means an engine launched from the repository root serves the working tree's frontend instead of its own. An inspection run that way is not bound to the artifact: this release's first ZAP scan had to be discarded because it scanned a stale local frontend while the engine under test was the CI one. Whatever the scan, confirm the binding by fetching `/` and checking the `xoksa-webui-<hash>` it references against the WASM in the artifact set.

**Binding the inspection to its source.** A hash says *which file* was inspected; it does not say *which source produced it*. Two failures during the v2.6.8 release made the gap concrete: a release was published before its source was committed, so `gh release create --target main` tagged a commit that did not contain the code; and a dependency update merged mid-inspection, leaving the first build unreproducible from the merged source. Six rules close it, and every record from v2.7.0 onward follows them.

1. **Build from a committed, clean working tree.** An artifact built over uncommitted edits has no source to name.
2. **Publish after the merge, never before.**
3. **Tag the commit that actually contains the code.** A mistagged release is corrected with `git tag -f` and a force push of *the tag ref* — repointing a reference rather than rewriting a branch's history.
4. **Name the source commit and its branch in the §C.4 record**, beside the artifact hashes.
5. **If the base moves between the build and the publish, rebuild and re-inspect.** The artifact is bound to one source; a source that has moved is a different source.
6. **Never delete a release branch.** Squash & Merge folds a branch's commits into one new commit on `main`, so the commit a record names survives **only on that branch** — measured at v2.9.8: `5065da8` (v2.9.7) and `e8b973d` (v2.9.8) are both unreachable from `main` and present only on `origin/release/v2.9.7` and `origin/release/v2.9.8`. Deleting the branch would leave the record pointing at nothing. `deleteBranchOnMerge` is therefore off for this repository.
7. **A §C.4 heading states dated facts, never a mutable state.** "Not yet shipped" is true until it is not, and nothing makes it change: v2.9.9's record carried it while the release went out **twelve minutes later**, and the other side of the project read it, believed the version was unpublished, and rebuilt a macOS artifact against a source the published one does not match — half a day lost. So a heading carries *when it was inspected* and *when it was published*, both of which stay true; the **current** state lives in one place only, the handover note, which is overwritten rather than appended. **Publishing is not finished until the §C.4 heading carries the publication date**, in both languages.

**Across a repository migration.** This project plans to publish a fresh repository built from `main`'s current tree **without carrying the history**. Every commit a §C.4 record names, and every `release/*` branch holding one, stays behind with that history — and so would any tag, since a tag travels with the commit it points at. The decision recorded here is that **the source repository is kept as a private archive rather than deleted**, so those commits stay reachable: a record written before the migration refers to the archive, not to the published repository. Rule 6 therefore keeps applying *to the archive* — the release branches must survive there — and the first §C.4 record written after the migration states which repository it is bound to, so no record is ambiguous about where its commit lives.

## C.2 Penetration-test matrix (measured against the frozen binary)

| # | Surface | Criterion | Verdict |
|---|---|---|---|
| A1–A4 | SSRF / scheme/path injection / redirect | outbound host fixed; symbol sanitised+encoded | PASS |
| B1–B5 | nested JSON / NaN-Inf / unknown tokens / control chars / OS metachars | no overflow/panic/exec; escaped/ignored | PASS |
| C1–C2 | static path traversal / `--web-dir` escape | all 404; `Component::Normal` only | PASS |
| D1–D5 | Content-Type bypass / cross-origin mutation / method override / body limit / `spec_json` cap | 415 / 403 / 405 / 413 / rejected | PASS |
| E1–E5 | reflected/stored/DOM XSS / popup message / `data:` load | escaped; `postMessage` removed; CSP-blocked (E1 found & fixed) | PASS |
| F1–F4 | error leak / secret disclosure / `--private` no-trace / `/api/config` over-exposure | none; minimal | PASS |
| G1 | resource-exhaustion DoS | admission-capped (503); stays responsive | PASS |
| H1 | prompt injection | frame holds; news titles sanitised at ingestion; override/leak refused (#7) | PASS |
| H2 | LLM number tampering | SOT unchanged; output-integrity guard on all providers | PASS |
| I1–I2 | dependency SCA (native / webui) | 0 vulnerabilities; warnings only — two unmaintained proc-macro crates (build-time, not in the shipped WASM) and `event-listener` 5.4.1's unsound `RUSTSEC-2026-0221`, which **is** in the shipped WASM but needs threads that its non-shared memory (`limits_flags=0x00`) rules out | PASS |
| J1 | `0.0.0.0` exposed access | fail-closed — always token-gated (auto-generated), private/loopback source addresses only (`401` / `403`) | PASS (re-measured over the LAN at v2.9.8, with the token gate) |
| K1–K3 | cross-client isolation / summary self-DoS / unbounded concurrency | isolated `sid`; per-session lock; admission cap | PASS |

**OWASP ZAP passive scan** (separate host, `0.0.0.0` over the LAN): the standing baseline since v2.9.2 is **8 alerts, 0 High** — usually 2 Medium / 1 Low / 5 Info — all accepted as not-applicable to this single-user, same-origin, no-CDN, no-DB architecture: Anti-CSRF on `GET /login` (the `/auth` POST it points at is gated by the 256-bit token itself, measured 401 on a wrong or empty token); SRI-missing (the alert lands on a frontend asset, while the HTML actually served carries `integrity` on all three of its subresources); Unix timestamp disclosure (market and analysis times, Class C); cookie poisoning on `POST /auth` (a correct observation of the scope decision recorded in [security-design.md §0.2](./security-design.md) — the auth cookie is the credential, not a derived session id); session-management response, modern web application, localStorage (UI state only; `xoksa_sid` is an `HttpOnly` cookie held server-side) and suspicious comments (wasm-bindgen's own generated output), all Info. **The count was 6 through v2.6.7 and has been 8 since**; the two added are exactly the two that sit on the auth endpoints the serve token introduced, not a regression. Per-release counts and the measured basis for each alert are in §C.4. `/api/chat/stream` (SSE) cannot be proxy-scanned (buffers → 502); its controls are verified directly (403/413/200).

**Network vulnerability scanners (Nessus/OpenVAS) — out of scope, by reason.** They fingerprint an off-the-shelf product+version against a CVE plugin DB; xoksa is a self-contained custom Rust binary (axum/hyper/rustls/tokio, static) with no middleware to fingerprint. The angle is covered at the correct layer: known-CVE exposure → dependency SCA (I1/I2); custom logic → the app-layer matrix + ZAP.

## C.3 SBOM — how it is generated

Two requirements: (1) no developer absolute path in the SBOM (editing the output to strip it is rejected on principle); (2) list what is actually **in the binary**, not the whole `Cargo.lock`. `cargo-cyclonedx` leaks `bom-ref path+file:///<abs>` (no suppress); `syft dir:.` over-includes the whole lockfile. **Chosen:** `cargo auditable build --release` (throwaway, embeds deps) → `syft file:<binary>` reads the embedded list → accurate, path-clean CycloneDX → then `cargo build --release` rebuilds the **clean shipped binary** (no audit data). Read the throwaway from a neutral directory (syft's Mach-O path records the file it read); never edit the output. `build.rs` emits `cargo:rerun-if-changed=build.rs` so an SBOM commit no longer triggers a full rebuild.

## C.4 Release records (per shipped artifact)

**Which repository the commits below live in.** This repository was published from the
tree of `Kozo2000/xoksa-dev` at `9d05cc7` — **without its history** — and that is the first commit
here. Every record written before the migration (v2.9.10 and earlier) names commits and
`release/*` branches that exist only in `Kozo2000/xoksa-dev`, which is kept as a **private archive rather than
deleted** precisely so they stay reachable (§C.1, "Across a repository migration").
A record's tag line is read the same way: `v2.9.10` names `9d05cc7` in the archive, and the tag
of the same name here points at this repository's own initial commit, which carries the
identical tree. Records written from the next release onward are bound to this repository.


### v2.2.3 (shipped 2026-07-09)
Windows and macOS binaries, `cargo build --release` (default `embedded-ui`), `rustc 1.96.0`, LLVM 22.1.2. Hashes cross-verified (`sha256sum` + `CertUtil`). All static/runtime/pentest items PASS (33/33). VirusTotal on the shipped hashes: **0/69** (Windows), **0/62** (macOS) — on-artifact, clean. Unsigned and **not notarized** (Gatekeeper blocks first run). The 1 ignored test is the live-Stooq failover e2e (external anti-bot block — not a defect).

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| `xoksa.exe` (Windows x86_64) | `67c2c7823e80c50d57d0072073e93d008570ae7921535ebcf1c27c0707c9bbfa` | 28,739,072 B |
| `xoksa-macos-arm64.zip` (download) | `1fed997d01338b783973fd9c8dc4719ec941673237d9172da7f8dbbde3fbbc02` | 9,161,674 B |
| `xoksa-macos-arm64` (extracted) | `3c67efabe79b9fb126190746bcae2ac48b736dc7567f89585bf46bee78316361` | 21,108,400 B |

### v2.3.x (desktop line) — source & dynamic PASS; binary-bound pending
The v2.3 line adds the **desktop app** (Tauri, separate `xoksa-desktop` artifact) plus core changes (news-title ingestion sanitisation and the all-provider output-integrity guard — Finding #7). Current head is **v2.3.4** (`cargo test` 245 lib + 10 integration pass, 1 ignored).

**Source & dynamic layer — measured 2026-07-19 (v2.3.4), all PASS.** B.5 playbook against `xoksa serve --private`: strict CSP with a per-request nonce and no `unsafe-inline`; `xoksa_sid` HttpOnly/SameSite=Strict; cross-client `sid` isolation (K1); path traversal all 404; CSRF / Content-Type / oversize gates 403 / 415 / 413; loopback GET 200. **`--host 0.0.0.0` measured too (not assumed unused):** the same B.5 controls re-run against `0.0.0.0` over the LAN IP hold identically (strict CSP + headers, `sid` isolation, traversal 404, Content-Type 415, oversize 413); the CSRF gate matches the request's own `Host` (not a hardcoded loopback), so a same-origin LAN client is allowed (200) while a cross-origin page is blocked (403). The only difference is the **documented no-auth** — an exposed deployment's authentication/TLS/rate-limit is the operator's responsibility (reverse proxy), not an app-layer gap. Prompt-injection / output-guard tests pass (news title/URL sanitisation; guard on every provider). **SCA via `osv-scanner` (Cargo.lock ×3): 0 vulnerabilities** — 5 *unmaintained-only* INFO advisories (`paste`, `proc-macro-error2` = build-time proc-macros not in the shipped binary; `unic-common` / `-ucd-ident` / `-ucd-version` = desktop Unicode deps), none exploitable. `cargo audit` / `cargo deny` currently abort on a CVSS-4.0 advisory the bundled parser rejects (a tooling-version issue, not a xoksa defect) — SCA is covered by osv-scanner; update `cargo-audit` when convenient. **Desktop regression check (this line's new form):** the multiple-Ollama setup form writes every `OLLAMA_<n>_ALIAS/HOST/PORT/MODEL` through `clean_env_value` (control-strip + 512-char cap) → DT8 holds, no env-line injection; chart hover tooltips render via a `data-tip` attribute + `textContent` (html-escaped, not `innerHTML`) → XSS-safe.

Binary-bound items (per-artifact hashes, VirusTotal, **notarization**, SBOM) are recorded here once the shipped binaries are frozen; the desktop artifact is assessed against its own build ([security-design.md §6](./security-design.md)).

**Desktop line — current head v2.6.4 (in development, not yet shipped).** Since v2.3.4 the desktop line added push notifications (fixed-host allowlist, https-only, Class-A keychain secrets, rising-edge + per-confirmed-bar dedup), the alerts dashboard feature (`/api/alerts`, `xoksa.env` persistence), the `--update-key` / `test-notify` flows, removal of the discontinued J-Quants v1 API, and the loopback engine hardening (ephemeral port + `/api/health` readiness/identity check before the WebView attaches — no version gate; the desktop and engine ship as a matched pair). Source & dynamic layers continue to pass (`cargo test`, `clippy -D warnings` across all three workspaces; notification host-allowlist and output-integrity guards unchanged). The binary-bound checks (hashes, VirusTotal, notarization, SBOM) for v2.6.x are recorded here when its binaries are frozen for shipping.

### v2.7.0 (engine inspected 2026-08-03; not yet shipped) — source / dynamic / SBOM PASS; binary-bound (VirusTotal, signing, macOS) pending

v2.7.0 aligns the desktop to 2.7.0 and drops its engine version-check (the desktop attaches to its bundled engine by `/api/health` `service` identity — no version gate, superseding the same-major check from #112; desktop and engine ship as a matched pair), adds loading the Japanese company-name list straight from the JPX `data_j.xls`/`.xlsx` (via `calamine`, so no Excel→CSV/UTF-8 conversion) with a native file picker in the setup form, and fixes two dashboard UX issues (empty-state now reads "銘柄を入力してください" not "読み込み中…"; the Market panel is cleared when the last ticker is removed). See PR #114 / CHANGELOG [2.7.0].

**Local engine inspection — all items measured on the shipped-build engine `A7D26EAD4A20DC58951C5264BA454C89B5DDA43B493AECA67F9E95539C151409` (13,578,752 B, `cargo build --release`, default `embedded-ui`, `rustc 1.97.1`).** **Static** — `check.sh` all twelve gates green (fmt / clippy `-D warnings` / test across engine, webui-leptos [wasm32] and xoksa-desktop; `cargo audit` ×2, `cargo deny` ×2, `cargo machete`); the same gates are green on CI (PR #114). **SOT** — `latest_observed_price` 308.91 and `final_score` 5.0 identical across csv / json / `/api/symbol/AAPL/summary`. **Runtime & penetration (B.5 against `serve --private`)** — `inspect.sh` 8/8 (embedded UI 200; CSP present; cross-origin POST 403; >64 KiB body 413; `text/plain` 415; same-origin POST 200; CLI analysis produces a report; backtest API valid JSON); a headless dump-DOM confirmed the WASM app mounts (`XOKSA` brand + panel / ticker / timeframe markers). **SCA / SBOM** — `sbom/xoksa-native-windows-x86_64.cdx.json` regenerated on this host (**CycloneDX 1.5, 211 components, no absolute path**), now including `calamine` and its transitive deps (the committed SBOM had been stale, generated before `calamine` was added); **binary-bound `cargo audit bin`** over the 233 embedded dependencies: **0 vulnerabilities**.

**Binary-bound items pending (recorded at ship-time, per this C.4 convention):** VirusTotal on the shipped hashes is **deferred until code signing** — an unsigned, new-hash desktop would only reproduce the reputation false positive of [§6.2 / §6.3](./av-false-positive-case-study.md) (Windows signing = Certum Open Source Code Signing, purchased, issuance pending); OWASP ZAP passive scan (Parrot host); the **desktop** binary-bound inspection (signed/notarized artifact, hashes, VirusTotal) and the **macOS** engine SBOM regeneration (with `calamine`, on a Mac) are done at their respective ship steps. The live-Stooq failover e2e remains the 1 ignored test (external anti-bot).

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| engine `xoksa.exe` (local `cargo build --release`, unsigned) | `A7D26EAD4A20DC58951C5264BA454C89B5DDA43B493AECA67F9E95539C151409` | 13,578,752 B |

### v2.7.4 (source-level SOT / security audit-and-fix, inspected 2026-08-09; not yet shipped) — source PASS; binary-bound pending

v2.7.4 is a shift-left audit-and-fix cycle: a line-by-line review of the source against design-philosophy §3 (config precedence) / §4.2 (one implementation in exactly one place) and security-design §0.1–§6, followed by the fixes — so the shipped binary matches the published principles before release. **Verification: `cargo fmt` clean, `cargo clippy` (stable) `-D`-clean, `cargo test` 264 lib + 10 integration pass.**

**Fixed (was a violation → corrected):**

| ID | Principle | Finding | Fix (evidence) |
| :--- | :--- | :--- | :--- |
| A | §3 / §4.2 | Config defaults duplicated across 6–8 sites with **13 confirmed drifts** — real bugs: on a bare CLI run `llm_max_output_tokens` fell back to 2048 (output truncated) and `weight_basic` differed 1.0/2.0 by whether the user had run `--init` (user-dependent composite scores) | `Config::default()` is now the single source; clap `default_value_t`, `build_config_with_value_sources`, the `setup` template, `ac_*`, `run_config_json_cli` and `xoksa.env.sample` all derive from it; the scattered `const` defaults were removed; Ollama `num_ctx`/`num_predict`/`seed` de-`Option`ed (the `llm.rs` `DEFAULT_OLLAMA_*` consts removed). Guard tests: `xoksa_env_sample_values_match_config_default`, `bare_build_config_scalar_defaults_match_config_default` |
| B | §0.1 / §2 | Class A keys copied into an owned `String` before the request in `/llm net` (OpenAI/Gemini/Claude) and the LINE bearer; `ac_opt_key` dropped the serde-parsed source key un-zeroized; `load_env_map` did not exclude `NOTIFY_*_SECRET` | Header values are borrowed `&str` (the OpenAI `Bearer …` value is built in a `Zeroizing` buffer; LINE uses `reqwest` `bearer_auth`), leaving only the accepted reqwest-`HeaderValue` copy; `ac_opt_key` zeroizes the source buffer; `load_env_map` drops any `NOTIFY_<n>_SECRET` (defense-in-depth) |
| D | §5 / lang-sync | §5 claimed the binary has "no high-entropy embedded blobs (`include_bytes!`)" — false: the engine embeds the WASM UI via `include_dir!` and setup PNGs via `include_bytes!` | Corrected to state these are legitimate `.rdata` resources, **not** the `.rsrc` icon section behavioural ML engines score for entropy (JA/EN); the §0.1 key exception was generalized from J-Quants to the HTTP-header transmission boundary for every Class A key |
| C | §4.2 | Duplicated logic | All audit groups resolved — see the §4.2 note below the table (18 consolidated to a single source; #19 verified as a distinct implementation, not duplicate logic, so left as-is) |

**Verified PASS (no violation — the core guarantees hold):** SOT numeric integrity (`TechnicalDataGuard` — every `entry.*` write goes through a `technical/types.rs` setter, no bypass; the composite total is computed once in `composite.rs`); **f64 unification with no calc-path rounding or cast** (the thousands-separator helpers are display-only `f64 → String`; the SOT `f64` is never rounded nor fed back into a calculation); CLI = GUI = Web single code (`api_data_equals_cli_json_row` gate test); the all-provider LLM output-integrity guard; the §4 server controls (per-client session isolation, origin/CSRF checks, the `Json<T>` content-type gate, the nonce-based strict CSP, the 503 admission cap, per-session locks); boundary sanitization; file-path resolution; the notification host allowlist (anti-SSRF); `#![forbid(unsafe_code)]` with no `std::process::Command` in the engine; and the reachability-based `unwrap`/`expect` policy.

**§4.2 duplication — resolved (behaviour-preserving throughout).** Consolidated to one source: #1 the adjusted-score line (10 copies → 1 `adjusted_score_line`), #3 `send_cloud_prompt` (3 provider senders → 1), #5 `key_present_in_env_file` → `read_key_from_env_file`, #6 the score-unavailable labels unified on "情報なし/unavailable" (utils the single source), #7 `colored_price_diff`, #8 `cloud_provider_rows`, #10 `send_chat_turn_dispatch`, #2 `group_thousands` (each caller keeps its own f64→int conversion — display-only), #4 `parse_env_line` (+ `live_ollama_instances` now uses `load_env_map`; each caller keeps its own Class-A filter, so a Class A value is never materialized), #9 `sanitize_symbol_result` (returns the error so the four reporting sites keep their message), #11 the operator symbol via `rule_operator_catalog`, #12 `fundamental_value_lines` (the revenue→next-FY value block; the display vs LLM header blocks are distinct views and stay per-function), #13 `cross_status`/`CrossLabels`, #14 `deviation_line`, #15 `symbol_of` reuse, #16 `ChatTokenUsage::accumulate`, #17 `on_off`, #18 the context.rs percent-change via `displayed_price_diff`. **Verified NOT a real duplicate implementation, left as-is:** #19 — `fmt_num` and `fmt_currency_auto` produce different output (no-symbol 4-dp vs symbol+commas 2-dp); their only shared token is the `100.0` threshold, which also drives the §1 output-integrity guard, so unifying it would false-couple display precision with a security threshold. Distinct sub-parts were likewise left (not duplicates): the `parse_alert_condition` operator ordering and the eval semantics in #11; the raw-bar (indicators.rs) and reload-delta (chat) percent computations in #18.

**Binary-bound items pending (recorded at ship-time, per this C.4 convention):** per-artifact SHA-256, VirusTotal on the shipped hashes (deferred until code signing, as for v2.7.0), the OWASP ZAP passive scan, and the per-OS SBOM regeneration are measured at the tag/CI ship step against the frozen binary. This record is the source-level evidence at working-tree state and does not substitute for the on-artifact measurement.

### v2.9.10 (macOS) — inspected 2026-10-02 on the CI-built artifacts, signed and notarized — PASS (static, SOT, runtime, penetration, payload identity, SCA, SBOM, signing/notarization, VirusTotal, ZAP); **published 2026-10-03 07:09 JST** (tag `v2.9.10` at `b3ef8e1`)

The macOS half of 2.9.10. The Windows half is not built yet; §1.3 of the handover requires both to come from the same `main`, so a Windows build of this version must use the same commit named below.

**Build provenance — GitHub Actions `Release build (macOS)` run `36981982521`, workflow `.github/workflows/release-macos.yml`, runner `macos-14`, source commit `ce4c589` on `main`** (the squash merge of #181, which carries #180's release work). Toolchain pinned by `rust-toolchain.toml` (1.98.1). Wall-clock 7 minutes. **The build log contains zero compiler warnings**; the only `warning:` line is cargo's standing future-incompat notice about `proc-macro-error2 v2.0.1`, the dependency whose vendored fix was deliberately dropped at v2.9.9 (record below) because `[patch.crates-io]` changes the WASM.

**This is the second set of 2.9.10 artifacts, and the first was discarded for a reason worth stating.** The first build (`73319c3`, run `36871609030`) was signed, notarized and inspected; the inspection's static gate then failed on `an_ollama_response_within_the_cap_is_read_whole`, a test added in this very release. Measured: one failure in thirty runs. The fault is in the test's own loopback server, which answered without reading the request, so closing the socket could RST the response away — nothing in the product. It would have been defensible to publish the inspected artifact and fix the test afterwards, and that is exactly what §C.1 rule 5 forbids: the artifact is bound to one source, and a source that has moved is a different source. So the fix was merged (#181) and everything from the build onward was done again.

**The rebuild proves the claim it was made to protect.** The engine, the desktop shell, the settings app and the WASM from run `36981982521` are **byte-identical** to those from run `36871609030` — `54f9ac1d…`, `d9c68bb4…`, `ea29e6f6…`, `4aa431b1…` on both — because `#[cfg(test)]` code is not compiled into a release binary. Only the `.tar.gz` envelope differs, which carries timestamps. Two consequences are now measured rather than asserted: the test fix changes nothing a user receives, and **this project's macOS CI build is reproducible across runs** for the same effective source, which §C.1 had only ever claimed for the WASM on one machine.

**Frozen macOS artifacts** (signed, notarized, stapled; every one verified against the CI-published `SHA256SUMS` before signing — 5/5 OK):

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| `XOKSA-2.9.10-arm64.dmg` (download) | `b01570387a5b7e96ccc4fbac6f53c678f732fd8edb151255b6d14bc5a29626e1` | 11,716,532 B |
| `XOKSA.app/Contents/MacOS/xoksa` (engine) | `ecc4a49d79f0a17618133ff1d8ed5d1f143ed1e27909499bbea887d49c72715f` | 12,014,544 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop` (shell) | `44482043031dde209aa0410870594f299f25860ab9ca1d5f0fa310c98d73ea17` | 6,346,000 B |
| `XOKSA.app/Contents/MacOS/xoksa-setup` (settings app) | `88f4f3cc9b58eac5f675a0d4a0b09ec5890078d7c3135338ca3b9679499af4db` | 5,914,400 B |
| `xoksa-webui-2fc5f86e0258a0ca_bg.wasm` (frontend embedded in the engine) | `4aa431b1ee5d84067b7345d62e8859cae037ad8cec3e3e2c1f48fe1209a782b0` | 710,967 B |

**Signing pipeline — the CI artifact is the input, not a rebuild.** The three inner binaries and the bundle were signed `Developer ID Application: Kohzoh Tsuchiya (463YK5WF6H)` with the hardened runtime and a secure timestamp, notarized (submission `c0adf8b8-2c15-4604-bf8e-1037609f9cc4`, **Accepted**), and stapled; the DMG was built locally from the stapled `.app` with an `/Applications` symlink, then signed, notarized (`0b5ce636-c3a1-40f2-8e84-c6af50ee6854`, **Accepted**) and stapled in turn. Gatekeeper reports **`source=Notarized Developer ID`** for both the app and the DMG, and the DMG still reports it after being copied into the repository's `dist/`.

**Two things blocked the signing step, and both are worth stating because they will recur.** Apple's notary service answered **HTTP 403 — "A required agreement is missing or has expired"** to every call, including `notarytool history`, until the Apple Developer Program License Agreement was re-accepted in the developer account; the credentials and the certificate were valid throughout (the certificate runs to 2031-07-12), so nothing local diagnosed it. And `codesign` refused the bundle with **"resource fork, Finder information, or similar detritus not allowed"**: the working copy carried `com.apple.FinderInfo` and `com.apple.fileprovider.fpfs#P`, because the repository lives under `~/Documents`, which is iCloud-managed (§3.1 of the handover). **Signing is therefore done outside iCloud** — the artifact is extracted, signed, notarized and stapled in a non-synced directory, and only the finished DMG and a zipped copy of the stapled `.app` are brought back into `dist/`. Clearing the attributes in place is not a fix: the file provider re-applies them.

**Payload identity — one byte, and it is the byte signing has to move.** With signatures stripped, each binary differs from its CI original in **exactly one byte**, in `__LINKEDIT`'s `vmsize`, grown by one 16 KiB page to hold the signature (`0x28000`→`0x2c000` engine, `0x18000`→`0x1c000` shell, `0x14000`→`0x18000` settings app). Sizes are unchanged and code and data are byte-identical to what CI produced — the same result as v2.9.9, measured again rather than assumed.

**Static — PASS.** `scripts/check.sh` at `ce4c589`: fifteen gates green, **491 tests**, zero failures (`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` across the engine, the WASM frontend, the desktop shell and `xoksa-paths`, plus `cargo audit`, `cargo deny` and `cargo machete` on both workspaces). **Green is not the whole story here:** the same gate, run at `73319c3`, is what caught the flaky test described above, and it caught it on the second run of the day. A gate that passes a 3%-flaky test most of the time is reporting the common outcome, not the absence of the fault — which is why the failure, the thirty-run measurement and the fix are recorded rather than a bare "zero failures".

**SOT — PASS.** csv == json == api output equivalence (AAPL: `latest_observed_price` 330.32 and `final_score` 2.0 identical across `--save-technical-log --stdout-log --log-format csv`, `--log-format json`, and `/api/symbol/AAPL/summary`; the nine per-indicator scores match as well).

**Runtime — PASS, 9/9.** `scripts/inspect.sh` against the signed engine: embedded UI 200, CSP header present, cross-origin POST 403, oversized body 413, `text/plain` POST 415, same-origin POST 200, CLI analysis produces a report, backtest API returns a result, no identifying build path. *One note for future readers: item 6 posts `spec_json: "{}"`, which 2.9.10 now rejects at the validation layer — the HTTP status is still 200 (the handler answers `{"ok":false}`), so the check still measures what it is for, the admission of a same-origin request, and not a successful save.*

**Frontend binding — PASS.** The engine was started from a directory with no `webui-leptos/dist`, and `/` references `xoksa-webui-2fc5f86e0258a0ca`; the served `.wasm` hashes to `4aa431b1…`, identical to the artifact's own WASM. The inspection is bound to the artifact, not to a working tree.

**Penetration — PASS on everything measured, with one item not exercised.** Path traversal: four encodings, all 404. Method override and wrong method: 422 / 405. `/api/config` carries no secret (fields are UI vocabulary, defaults and the version; a scan for key-shaped strings finds none). Cross-origin SSE: 403. Session isolation: two clients receive distinct 32-hex `xoksa_sid` values. `--private`: no `logs/` directory is created and `~/.xoksa_history` is untouched; the engine itself prints `Diagnostics log: disabled (private mode)`. **Not exercised: the capacity shed (K3).** 300 concurrent `/api/health` requests were all served `200`, which does not demonstrate the `503` — the handler returns far too quickly for 256 requests to be in flight at once. The cap is present and is now 256 (it was `u16::MAX`, see Finding B.3 and the §4 design note), and the shed itself was measured during development on a build with the cap lowered to 2. A release-time measurement would need a slow endpoint and is not claimed here.

**SCA — PASS.** `cargo audit bin` against a throwaway `cargo auditable` build of the same source: **245 dependencies embedded, 0 vulnerabilities**. `osv-scanner --lockfile Cargo.lock`: 338 packages, **no issues found**. `cargo audit` and `cargo deny` (advisories, licences and sources) are green on both workspaces in the static gate above.

**SBOM — regenerated, and the inventory did not move.** `sbom/xoksa-native-macos-arm64.cdx.json` (**221 library components**) and `sbom/xoksa-webui-leptos.cdx.json` (**188**), both CycloneDX 1.5 via `syft` 1.46.0, the native one read from the throwaway `cargo auditable` build in a neutral directory (never from the shipped binary, which carries no auditable data). Compared component-by-component against the 2.9.9 inventories, the **only** difference is this project's own crate version (`xoksa` 2.9.9 → 2.9.10, `xoksa-webui` likewise): no dependency was added, removed or moved in this release. The Windows inventories are untouched — they describe the published 2.9.9 Windows artifacts and will be regenerated when 2.9.10 is built there.

**VirusTotal — 0 detections on all four shipped hashes** (2026-10-02, submitted by the operator): the DMG `b01570387a…` **0/61**, the engine `ecc4a49d…` **0/63**, the desktop shell `44482043…` **0/62**, the settings app `88f4f3cc…` **0/63**. Each was submitted as the signed artifact, and VirusTotal reports all three Mach-O binaries as `signed`. A clean multi-engine scan is not an audit — it is one datapoint bound to these hashes.

**OWASP ZAP passive scan — 8 alert types, all accepted** (2026-10-02, separate host over the LAN against `--host 0.0.0.0`, which runs token-gated). Two of them come from the login flow, which earlier scans never reached, and both are the designed behaviour rather than a finding:

- **Anti-CSRF token missing on `/login`.** The control is the 256-bit access token itself, not a form token: a cross-site POST without it is refused. Measured here — a cross-origin `POST /auth` carrying a wrong token returns `401` and sets no `xoksa_auth` cookie (only the session id, which is not authentication). The route is deliberately exempt from the server-side origin check because `Referrer-Policy: no-referrer` makes a browser send `Origin: null` on that same-origin form POST, which the check would reject, breaking a legitimate login; the exemption and its reasoning are in the code.
- **Cookie poisoning on `POST /auth`.** ZAP flags that a request parameter reaches a cookie. That is the designed flow — the cookie carries the token itself, a scope decision recorded in [security-design.md §0.2](./security-design.md) — and the value is compared in constant time, so a value the client chose simply fails.

The other six match the v2.9.9 reading: SRI (no cross-origin assets), Unix timestamp disclosure (analysis data by design), session-management response identified and "modern web application" (informational), `localStorage` (UI state only; keys stay server-side), and suspicious comments in the generated wasm-bindgen JavaScript. As at v2.9.9, `/api/chat/stream` (SSE) cannot be proxy-scanned; its controls were measured directly (cross-origin `403`, oversize `413`, same-origin `200`).

**Published 2026-10-03 07:09 JST** (tag `v2.9.10` at `b3ef8e1`).

### v2.9.10 (Windows) — inspected 2026-10-02 on the CI-built artifacts, unsigned — PASS (static, SOT, runtime, penetration **including the capacity shed, measured here for the first time**, payload identity, ZAP, SCA, SBOM); VirusTotal deferred until signing; **published 2026-10-03 07:09 JST** (tag `v2.9.10` at `b3ef8e1`)

The Windows half of 2.9.10, and the last inspection of the 2.x line.

**The two halves of this release are bound to different commits, and that is stated here rather than glossed.** macOS built from `ce4c589`; Windows built from `ba7f397`, two commits later on the same `main`. §C.1 rule 5 says an artifact is bound to one source and a source that has moved is a different source — so the question is whether the program moved. It did not: `git diff ce4c589 ba7f397` over `src/`, `webui-leptos/src/`, `xoksa-desktop/src/`, `xoksa-setup/src/`, `xoksa-paths/`, every `Cargo.toml` and `Cargo.lock`, and `tauri.conf.json` is **empty**. The whole difference is **two documentation files, 326 insertions, zero deletions**: the 84 lines of the macOS record above, and the 242 lines `docs/manual/setup.md` gained for the chat-notification channels (#183). The manual had to be in the tree *before* the Windows build, because the shipped manual zip is built from `docs/manual/` by `scripts/make-manual-zip.sh`. Each half therefore names its own commit, and the program both halves contain is the same program.

**Build provenance — GitHub Actions `Release build (Windows)` run `36997241294`, workflow `.github/workflows/release-windows.yml`, runner `windows-latest`, source commit `ba7f397` on `main`**, triggered by `workflow_dispatch`. Wall-clock 58 minutes 36 seconds (10:45:32–11:44:08 UTC); all eighteen steps green, including the `Gate — no identifying build path in any artifact` step that would have failed the upload. **Toolchain read out of the shipped engine itself, not from a build log: `rustc 1.98.1 (48a229cea)`** — the same commit hash is in the desktop shell and the settings app — which is what `rust-toolchain.toml` pins; `trunk 0.21.14` and `tauri-cli 2.11.4` are pinned in the workflow. Unsigned. The working tree at inspection time is that same commit, clean.

**Frozen Windows artifacts — every one verified against the CI-published `SHA256SUMS`: 5/5 OK, and re-verified unchanged after the inspection finished.**

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| `XOKSA-2.9.10-x64.msi` (installer, carries the three binaries below) | `f86084dcef2f32b384a9fcbe3f357ba4f1336c14f66012c9488d40503b1b50f7` | 11,714,560 B |
| `xoksa.exe` (engine) | `0433ea81556738370a91ce60cbba4576b8b519b1c3d69cd6a100b9775455f2c9` | 14,857,216 B |
| `xoksa-desktop.exe` (connection shell) | `63f865ee4dd8b0f87f1a72866fa2b48cd8c90531410aee5ced56df93dcf6f3b3` | 9,629,696 B |
| `xoksa-setup.exe` (settings app) | `4efe9432755494c1ab744cb94ce0185afa0bcc83cdf153a738e6f8a78c28c29e` | 8,414,720 B |
| `xoksa-webui-22fbb44a3ccaa34a_bg.wasm` (frontend embedded in the engine) | `983596d5fb4502bec51581129b343aa384bb7280b815f83a4128576e9061a043` | 724,577 B |

**The WASM is not the one macOS shipped, and §C.3's determinism claim needs that boundary.** macOS's 2.9.10 frontend is `xoksa-webui-2fc5f86e0258a0ca` / `4aa431b1…` / 710,967 B; this one is `xoksa-webui-22fbb44a3ccaa34a` / `983596d5…` / 724,577 B — same source, same `webui-leptos/Cargo.lock`, different bytes. §C.3 calls the WASM deterministic, and the macOS record measured it reproducible **across runs on one runner**; neither statement was ever that it is reproducible **across operating systems**, and it is not. Recorded so that a future reader does not read one hash as a contradiction of the other: there is one WASM inventory because the dependency set is host-independent, and two WASM binaries because the builds are not.

**No artifact names its builder.** `scripts/check-no-host-paths.sh` passes on all five. The desktop shell, the settings app, the WASM and the MSI carry **zero** user paths of any kind; the engine carries **142**, every one the GitHub-hosted runner's own built-in account — `C:\Users\runneradmin` (97) and its 8.3 short form `C:\Users\RUNNER~1` (45, from `aws-lc-sys`'s C build) — which identifies nobody. Identical to v2.9.9, down to the counts.

**Payload identity — measured by extracting the MSI, and it reproduces v2.9.9 exactly.** The CAB holds three files (`Bin_xoksa.exe`, `Bin_xoksa_setup.exe`, and the desktop shell, which 7-Zip lists under the name `Path`). The engine and the settings app are **byte-identical** to the standalone artifacts. The desktop shell differs in **exactly three bytes, at `0x77190f`–`0x771911`**, where Tauri stamps the install form (`__TAURI_BUNDLE_TYPE_VAR_UNK` → `…_MSI`) — the same three bytes at the same offset as v2.9.8 and v2.9.9, so the installer carries no other modification. All six binaries (three standalone, three extracted) report `FileVersion` **2.9.10**: no sidecar drift.

**AV false-positive recipe held on CI.** Engine `.rsrc` **2,048 B at entropy 4.176** and `.rdata` **5,198,848 B at 5.887**; desktop `.rsrc` **32,768 B at 3.898**; settings app `.rsrc` **32,768 B at 3.900**. Every figure is within 0.02 of v2.9.9's, so the axis the behavioural ML engines score has not moved. (The engine's `.text` is 9,227,776 B at 6.291 and `.pdata` 321,536 B at 6.564, for whoever next wonders which section the entropy headline refers to: it is `.rsrc` and `.rdata`, not the code.)

**Static — PASS.** `scripts/check.sh` at `ba7f397`: **fifteen gates green, 491 tests, zero failures** (473 engine + 11 WASM frontend + 6 desktop shell + 1 `xoksa-paths`), with the working tree clean, which is §C.1 rule 1.

**SOT — PASS, and this time every column is accounted for.** The CSV log is positional, so its header (`--show-log-header`) was used to map all 51 columns back to the JSON log's keys and to `/api/symbol/NVDA/summary`: **the three key sets are identical in membership — 51, 51, 51 — 49 columns agree across all three paths, and none disagrees.** The two that are not compared are `analyzed_at` and `analyzed_at_timestamp`, which record *when the run happened* rather than a computed value; they differ by the seconds between the three calls (08:58, 08:58, 09:00), which is correct behaviour. All three paths name the same bar, `2026-10-01`, and the same instrument. Identical across all three: `bar_close` **230.86000061035156**, `rsi` **66.52194618…**, `macd` **3.140415954…**, `signal` **2.559756341…**, `adx` **16.0682252…**, `ema_short` **228.3516487…**, `ema_long` **223.9319608…**, `bb_upper` **235.332715…**, `bb_lower` **211.6892852…**, `latest_volume` **98,369,800**, `final_score` **7**.

*One note on reading the SOT numbers at all: they are a snapshot. A CLI analysis run two hours later, after the US open, reported RSI 74.78 on a forming 2026-10-02 bar. The three paths must agree with each other at one instant, which is what is measured; they are not expected to agree with a later run.*

**Frontend binding — PASS.** The engine was started from `dist/2.9.10-windows-x86_64/`, a directory with no `webui-leptos/dist`, so `read_asset` could not prefer a working-tree file over the embedded copy. `/` references `xoksa-webui-22fbb44a3ccaa34a`, and the `.wasm` the engine served hashes to `983596d5…` — identical to the artifact's own WASM. Three figures, one value: the page's reference, the served bytes, and the frozen file.

**Runtime — PASS, 9/9.** `scripts/inspect.sh` against the frozen engine: embedded UI 200, CSP header present, cross-origin POST 403, oversized body 413, `text/plain` POST 415, same-origin POST 200, CLI analysis produces a report (3,650 B, 99 lines, exit 0, nothing on stderr), backtest API returns a result, no identifying build path.

**Browser render — measured against the raw HTML, not just by counting elements.** Headless Edge mounted the WASM app: the page the engine serves is **1,480 B with zero `div`s**, and the rendered DOM is **5,392 B with 16 `div`s, 7 buttons, 5 sections, 3 selects, 4 inputs, 2 labels, a form, a textarea and an SVG**. Differencing the two is what shows the WASM built the UI rather than the server having sent it. One `<script>` in the DOM, the nonce'd module bootstrap.

**Penetration — the §C.2 matrix against the frozen engine `0433ea81…`, and K3 is no longer unmeasured.**

- **G1 / K3 — the capacity shed, measured at release time for the first time.** The macOS 2.9.10 record states plainly that it could not demonstrate the `503`: 300 concurrent `/api/health` requests all returned 200, because the handler returns far too quickly for 256 to be in flight. Two further attempts here failed the same way for a reason worth recording: **`curl --limit-rate` throttles the request *headers* as well as the body**, so 512 TCP connections were open — and `/api/health` still answered in 0.7 ms, because a request whose headers have not arrived is not a request yet and holds no permit. The server shrugging off 512 half-sent connections is itself a result; it is not the shed. The shed needs requests that have been *admitted* and are waiting: headers sent complete, a `Content-Length` declared, and the body withheld. Done with raw sockets, the figure is exact — **at 192 held requests `/api/health` returns 200; at 256 it returns 503 (`server at capacity`), and so do `/` and `/api/config`; releasing half returns the server to 200; releasing all restores it fully.** `MAX_CONCURRENT_CONNECTIONS = 256` is therefore the real ceiling, the shed is immediate rather than a queue, and it does not latch.
- **K1 — cross-client isolation, measured in both directions.** Two browsers receive distinct 32-hex `xoksa_sid` values (`HttpOnly`, `SameSite=Strict`). Client A selected `gemini` and client B `claude` through `/api/llm/select`; `/api/llm/options` then reported A=`gemini` and B=`claude` — neither client could see or take the other's selection. A forged all-zeros `sid` simply gets its own empty namespace rather than access to anyone's.
- **K2 — one slow chat turn does not stall anything.** A real local-model turn (Ollama, `qwen3:8b`) was streamed under client A for at least 14 seconds; measured under client B during that window: `/api/symbol/NVDA/summary` **1.5 ms**, `/api/health` **0.6 ms**, `/api/config` **1.6 ms**.
- **A1–A4 — SSRF / scheme and path injection.** Eight crafted symbols (`http://evil.example/x`, `//evil.example/a`, `NVDA/../../etc/passwd`, `NVDA@evil.example`, `NVDA#@evil`, `file:///c:/windows/win.ini`, `NVDA?x=1`, `NVDA%0d%0aHost:evil`) were all refused by the character allowlist before any outbound request was built. The CRLF payload comes back **JSON-escaped** (`\r\n` as two characters), so it cannot inject a header; and the echo is served as `application/json` with `nosniff`, so it cannot render either.
- **B1–B5 — malformed input.** JSON nested 2,000 deep → **422**; `NaN` → **400**; `1e400` → **400**; a NUL inside a string → **400**; no panic, no hang. An unknown `timeframe` (`../../etc`) and an absurd `period` (`99999y`) are **ignored**, and the backtest falls back to the timeframe default — measured by comparing against a legitimate request: identical `timeframe: "daily"` and `bars: 64`. A symbol carrying OS metacharacters (`NVDA|calc.exe & whoami`) is refused with `note: "invalid symbol"`.
- **C1–C2 — path traversal.** Eight encodings, all **404**: `../../../../Windows/win.ini`, `..%2f…`, `....//....//…`, `/C:/Windows/win.ini`, `%2e%2e%5c…`, plus `.git/config`, `xoksa.env` and `../xoksa.exe`.
- **D1–D5 — request policy.** `text/plain`, `application/x-www-form-urlencoded` and a missing `Content-Type` are each **415** on `/api/backtest`, `/api/llm/select`, `/api/alerts`, `/api/analysis/multi-timeframe` and `/api/backtest/rules`; a cross-origin JSON POST is **403** on all of them, and so is the SSE `GET /api/chat/stream` — by `Origin` **and** by `Referer`. A 6 MB body is **413** (the limit is 64 KiB), and so is a 70 KB rule. `X-HTTP-Method-Override: POST` on a GET is ignored and changes nothing (the alert list is byte-identical before and after); `PUT` and `PATCH` are **405**. **D5, the saved-rules cap:** 210 rules were submitted and **exactly 200 kept**, the oldest (`rule1`) evicted and the newest (`rule210`) retained; a rule named `../../../evil` is stored as a *field value* inside the one fixed file and creates nothing anywhere else. Measured in a sandboxed `HOME` so the operator's own four saved rules were never touched — verified unchanged, 200 B, 4 rules.
- **E1–E5 — browser attack surface.** The reflected-XSS probe (`<img src=x onerror=alert(1)>` as a symbol) comes back inside `application/json` with `nosniff`, where markup cannot execute. In the shipped frontend the only `inner_html` sinks are fed escaped content: `analysis_html` escapes every line with `html_escape` before wrapping a numeric value in a `<span>` whose class comes from a two-item literal set, and `chart_doc_html` escapes the symbol and the timeframe label with a comment saying why. **`postMessage` does not occur in the shipped artifact at all** — zero in the JS glue, zero in the WASM — so finding E4's removal is a property of the binary rather than of the source. A news URL becomes an `href` only when `safe_http_url` accepts it; anything else renders as text in a `<span>`, and links carry `rel="noopener noreferrer"`. The CSP is nonce-based with no `'unsafe-inline'`: `default-src 'self'`, `script-src 'self' 'wasm-unsafe-eval' 'nonce-…'`, `style-src 'self'`, `img-src 'self' data:`, `connect-src 'self'`, `object-src`/`base-uri`/`frame-ancestors 'none'`, `form-action 'self'`, beside `X-Frame-Options: DENY`, `nosniff`, `Referrer-Policy: no-referrer` and a Permissions-Policy denying eight features.
- **F1–F4 — leakage.** Error responses from an unknown symbol, a bogus timeframe and a missing route carry **no** absolute path, `.rs` location, panic text or account name. A scan of seven endpoints for key-shaped strings (`sk-`, `AIza`, `sk-ant-`, `xoxb-`, a Slack `/services/` path, a LINE push path, a long `Bearer`) finds **zero**. `/api/config` is stronger than "presence only": it carries **no key, secret or token field at all**, not even an existence flag — its `"key"` entries are indicator and operator vocabulary. **`--private` wrote nothing:** with a `serve --private` session driven through a symbol summary, a backtest and a language change, the canonical config directory is byte-for-byte unchanged — the language write is *refused* rather than performed, as §0.1 requires — and the engine says so itself (`Private mode: nothing is saved to disk.` / `Diagnostics log: disabled (private mode)`).
- **J1 — non-loopback is fail-closed, measured on the artifact.** With `--host 0.0.0.0`, **every** path answers **401** without a valid token — `/api/health`, `/api/config` and `/` alike — and so does a wrong token and an empty `Bearer`; the correct token answers 200. The browser exchange is a form POST to `/auth`: a wrong token **401**, the correct one **303** to `/` setting `xoksa_auth` as `HttpOnly; SameSite=Strict; Path=/`. That cookie then answers **200**, a forged 64-hex cookie **401**. The comparison is constant-time (`ct_eq`, length-checked, XOR-accumulating), and **the startup log does not print the token** — zero runs of 40+ alphanumerics in the log; it prints where to get it (`Show token with \`xoksa serve --show-token\``). The token was read into a shell variable and never displayed during this inspection.
- **H1 — prompt injection: boundary verified, runtime injection not performed.** The control is `sanitize_news_text` / `sanitize_news_url`, which flatten a title to one line and neutralise the prompt's *structural* markers (newlines, `===`, `【】`, code fences, control characters, a length cap) while deliberately leaving the words as plain text — structure forging is what the output guard cannot detect, so it is what this boundary removes. Three deterministic tests cover it and ran green in the static gate above. What is **not** claimed is a runtime injection against the artifact: the title comes from Brave, which this inspection cannot make say anything in particular.
- **H2 — LLM numeric tampering.** Re-measured independently on Windows the same day, against confirmed values re-read from this engine: **17/17** on the acceptance set (ten reversal cases, five regression cases, two excluded by §9.2 of the investigation record). The harness and the finding are in the investigation folder; the rules themselves are normative in security-design §1.
- **I1–I2 — see SCA below.**

**No implicit `./xoksa.env` — measured by behaviour rather than by reading a log line.** A decoy `xoksa.env` (`SELL_RSI=99`, `LANG=en`, `STANCE=decoy`) was planted in the launch directory beside a copy of the engine. The analysis it produced used `SELL_RSI=70` and `stance: Holder` — the canonical values — and reported RSI 74.78 as *overbought*, which only happens at a threshold of 70, not 99. The canonical file is unchanged (mtime 2026-09-27, zero occurrences of the decoy's strings), so the one-time migration did not fire either. §6's single-config-location contract holds on the artifact.

**OWASP ZAP passive scan (over the LAN against the frozen engine `0433ea81…` bound to `0.0.0.0:8787`) — 8 alerts, 17 instances, 0 High, and not one of them a defect.** Identical to v2.9.8 and v2.9.9 in categories and verdicts. **The run is bound to this artifact:** the alerts name `xoksa-webui-22fbb44a3c…`, the frontend this engine embeds and serves, and the engine was started from a directory with no `webui-leptos/dist`. Each item below was re-measured here rather than carried over.

| Alert | Instances | Measured |
| :--- | --: | :--- |
| Anti-CSRF token missing (`GET /login`) | 1 | Deliberate — see below |
| Sub Resource Integrity attribute missing | 1 | ZAP's HTML rule applied to a binary URL |
| Timestamp disclosure — Unix (`/api/symbol/NVDA/summary`) | 3 | Market and analysis times |
| Cookie poisoning (`POST /auth`, `token`) | 1 | Correct observation of a recorded design choice |
| Session management response identified | 3 | Correct detection |
| Modern web application (`GET /`) | 1 | Correct detection |
| Information disclosure — browser localStorage (`GET /`) | 6 | Display state only |
| Information disclosure — suspicious comments | 1 | wasm-bindgen's own generated output |

- **Anti-CSRF token missing on `GET /login` (Medium, confidence Low)** — unchanged and deliberate. The evidence ZAP cites is `<form method="post" action="/auth">`, and `/auth` is the one POST exempt from the origin check (the reason is stated in `src/server/mod.rs`), gated instead by the 256-bit token itself. Measured on this engine during this inspection: a POST to `/auth` with a wrong token returns **401** and an empty token **401**; only the correct token returns **303**. A CSRF token would add nothing a cross-site attacker could not already fail to guess, because the secret being submitted *is* the credential.
- **SRI attribute missing — the alert's URL is the frontend asset, not an HTML document.** The HTML this engine actually serves (1,480 B) carries `integrity="sha384-…"` on **all three** of its subresources — the stylesheet, the JS `modulepreload` and the WASM `preload` — measured, 3 `integrity` attributes for 3 subresource references. ZAP is applying an HTML passive rule to binary content, exactly as it did at v2.9.9, where the evidence was traced to a Rust `format!` template compiled into the WASM data section.
- **Timestamp disclosure (Low, 3 instances)** — measured: `bar_timestamp` 1790947800 (2026-10-02 13:30:00 UTC), `market_data_latest_timestamp` 1790970171 (19:42:51 UTC), `analyzed_at_timestamp` 1790970172 (19:42:52 UTC). Market and analysis times, Class C under §0.1, and required by §1 — the bar a reading was computed on travels with it. Nothing about the host is disclosed.
- **Cookie poisoning on `POST /auth` (Info)** — ZAP correctly observes that the `token` parameter reaches a cookie. Measured: a successful exchange answers **303** to `/` and sets `xoksa_auth=<the token verbatim>` and `xoksa_sid=<32 random hex>`, both `HttpOnly` and `SameSite=Strict`, **neither carrying `Max-Age` or `Expires`** (session cookies). The auth cookie **is** the credential rather than a derived session id, which §0.2 now records as an explicit scope decision rather than an oversight: separating them needs a session table with expiry and revocation and buys only per-device revocation. The cost is recorded rather than omitted — a passive LAN observer who captures one request obtains the master credential, which also opens the CLI/API — and `--rotate-serve-token` invalidates every browser session at once.
- **Information disclosure — browser localStorage (Info, 6 instances)** — all nine keys enumerated from the frontend source, and they carry a namespace prefix the v2.9.9 record omitted: `xoksa.theme`, `xoksa.font`, `xoksa.symbol`, `xoksa.tickers`, `xoksa.targets`, and the pane geometry `xoksa.right_w`, `xoksa.fund_h`, `xoksa.chat_h`, `xoksa.news_h`. Display state and ticker symbols (Class C); no credential and no session id — `xoksa_sid` is an `HttpOnly` cookie, held server-side.
- **Suspicious comments (Info)** — measured on the served glue JS (50,628 B): **19 `//` comments and one `TODO`**, *"// TODO we could test for more things here, like `Set`s and `Map`s."* — every one of them wasm-bindgen's own generated output, not xoksa source. The shipped `.wasm` contains **no `/*` at all**; its three `//` byte pairs are inside URLs.
- **Session management response identified** and **Modern web application** (Info) — correct detections of the `xoksa_sid` cookie and of a JS-driven UI, not findings.

`/api/chat/stream` (SSE) stays outside the scan — a proxy buffers it into a 502 — and its controls were verified directly above (403 cross-origin by `Origin` and by `Referer`, 200 same-origin).

**SCA — 0 exploitable in the shipped Windows artifacts.** `cargo audit`: **0 vulnerabilities, 0 warnings** over the engine's 338 crate dependencies. `cargo audit -f webui-leptos/Cargo.lock`: **0 vulnerabilities, 3 warnings** (`paste` and `proc-macro-error2` unmaintained; `event-listener` 5.4.1 unsound). `cargo deny check` advisories / bans / licences / sources and `cargo machete` are green on both workspaces. `osv-scanner` per lockfile: engine **0** (338 packages), `xoksa-paths` **0** (18), WASM **3** (188), `xoksa-desktop` **8** (420), `xoksa-setup` **8** (446) — the last two being seven distinct advisories, since `GHSA-wrw7-89jp-8q8g` is an alias of `RUSTSEC-2024-0429`.

What reaches a shipped Windows file was checked against the **binary-derived** inventories rather than assumed from the lockfile. `glib` 0.18.5 (`RUSTSEC-2024-0429`, CVSS 6.9 — the only advisory anywhere here that carries a CVSS) is in the gtk-rs Linux bindings: `glib`, `gtk` and `gdk` are **absent from all three** Windows binaries. `proc-macro-error` 1.0.4 is a build-time proc macro and is **absent** too. What does ship is the five `unic-*` 0.9.0 crates, in the desktop shell and the settings app only — all five **unmaintained, none a vulnerability, none carrying a CVSS**. `cargo audit bin` over the throwaway `cargo auditable` builds confirms it on the dependency sets actually embedded: engine **254** dependencies / **0** vulnerabilities, desktop shell **251** / **0** (5 unmaintained warnings), settings app **267** / **0** (the same 5).

**`RUSTSEC-2026-0221` does ship, and the reason it cannot be reached is measured on this WASM.** `event-listener` 5.4.1 is unsound — `StackSlot` lets a `!Send` tag cross a thread boundary — and it is not a build-time crate: `cargo tree` puts it at leptos → `reactive_graph` → `async-lock`, and its strings are in the shipped `.wasm`. §C.2's note that the WASM findings are "not in the distributed WASM" was written for the two proc-macro crates and does **not** extend to this one. Crossing a thread boundary needs a thread: parsed out of the shipped binary, its memory section is a single entry with **`limits_flags=0x00`, min 18 pages, non-shared** — the same value the macOS record measured on its own WASM, and wasm threading requires shared memory. Not reachable in this artifact; to be re-checked whenever Leptos moves.

**A conclusion recorded at v2.9.9 was wrong, and here is the measurement.** That record said of `deny.toml`'s `RUSTSEC-2026-0173` entry: "the dependency is gone and the entry is stale. Removing it restores the warning's usefulness." It is not stale. One `deny.toml` is shared by two workspaces: `paste`, `proc-macro-error2` and `event-listener` exist **only** in `webui-leptos/Cargo.lock` (zero occurrences in the root `Cargo.lock`), so the two `ignore` entries are **load-bearing on the webui workspace and unmatched on the root one** — and the `advisory-not-detected` warning is a consequence of that sharing, not of a dead entry. Measured on a copy of the file, with `deny.toml` itself untouched: with the entries, `cargo deny --manifest-path webui-leptos/Cargo.toml check advisories` reports **`advisories ok`**; with them removed it reports **`advisories FAILED`**. Acting on v2.9.9's recommendation would have broken a release gate. (The same run shows `event-listener`'s unsound advisory does not fail that gate although it is not ignored — so on this configuration `unmaintained` is what fails and `unsound` is not.)

**SBOM — three Windows inventories regenerated, and the comparison method had to be fixed before the result could be trusted.** `sbom/xoksa-native-windows-x86_64.cdx.json` (**225** library components), `sbom/xoksa-desktop-windows-x86_64.cdx.json` (**196**) and `sbom/xoksa-setup-windows-x86_64.cdx.json` (**212**), all CycloneDX 1.5 via `syft` 1.49.0, each read from a throwaway `cargo auditable` build in a neutral directory and **not** from the shipped binary, which CI does not build with `auditable`. None carries an absolute path, a user name or even an `.exe` (measured). Against the published v2.9.9 inventories the **only** difference in each is this project's own crate version (`xoksa`, `xoksa-desktop`, `xoksa-setup`: 2.9.9 → 2.9.10); no dependency was added, removed or moved.

That comparison was first made keyed by crate **name**, which silently merged crates present at two versions — 225 components collapsed to 217 names, and a version move on any of the eight duplicated names would have been invisible. Redone as a **(name, version) multiset**, the counts match the inventories exactly and the duplicate sets are identical before and after: `windows-sys` at four versions in the engine, `schemars` at three in both Tauri apps, and so on. The conclusion survived; the first basis for it did not, and a comparison that cannot see a version move is not a comparison.

**Two recipe facts for the next Windows release.** First, `sbom/README.md` says `cp target-sbom/release/xoksa /tmp/sbomwork/xoksa` — and on Windows under Git Bash **that cannot produce an extensionless file**: MSYS resolves `xoksa` to the existing `xoksa.exe`, for `cp` and for a shell redirect alike, so both silently write the wrong name. It matters because syft's `pe-binary-package-cataloger` fires on the `.exe` extension and adds a 226th component of type `application` (with a `cpe:2.3:a:xoksa:xoksa:2.9.10` and a relative `\xoksa.exe` location), which would make this inventory the only one of the five that is not `library`-only. The copy must be made with PowerShell's `Copy-Item`. Second, `sbom/xoksa-webui-leptos.cdx.json` is current and was left alone: it is catalogued from `webui-leptos/Cargo.lock`, whose last change is `73319c3` (2026-10-01), and it was regenerated at `ce4c589` (2026-10-02) during the macOS inspection — after the lockfile, so it already describes this tree. The throwaway trees (671 MB + 1.2 GB + 1.2 GB) were removed and no shipped file was touched.

**Findings carried forward — both re-verified on this tree, both unchanged.**

- **`native-tls` is compiled into the shipped engine unused.** `cargo tree -i native-tls` still shows a single parent: xoksa's own `Cargo.toml`. `hyper-tls` has **zero** occurrences in the binary-derived inventory, so reqwest has no native-tls path; `native-tls` 0.2.18 and `schannel` are in the inventory, so they ship as dead code. `[package.metadata.cargo-machete] ignored = ["native-tls"]` still suppresses the one check that would report it, which is why `cargo machete` is clean above. Not a vulnerability; unnecessary surface in a binary that handles Class A credentials.
- **The settings app is still outside the static gate set.** `xoksa-setup` appears **zero** times in `scripts/check.sh` and **zero** times in `.github/workflows/audit.yml`; its only two mentions in `ci.yml` are the comment and the `touch` that create the sidecar stub `tauri-build` validates. `xoksa-desktop`, by contrast, is named five times in `check.sh`. So a shipped binary that collects Class A credentials (§6) receives no `fmt --check`, no `clippy -D warnings`, no `test`, and no `cargo audit` / `cargo deny` over its own lockfile. Its SBOM and its `cargo audit bin` above were produced by this inspection, not by a gate.

**Not measured, and why.**

- **VirusTotal is excluded this cycle, not pending.** The owner's stated sequence is that publication must precede signing, and an unsigned brand-new hash would only reproduce the first-seen reputation penalty of [§6.2 / §6.3](./av-false-positive-case-study.md) (Windows signing = Certum Open Source Code Signing, purchased, issuance pending). It is therefore not carried as an open task. *(Corrected 2026-10-03: eight places in this file called the certificate "Certum standard CodeSigning". The order is for **Open Source Code Signing in the Cloud** (SKU `bd_005`), bought from Certum directly on 2026-08-04 for USD 58 — a different product, and cloud-based, so signing uses SimplySign and no USB token. An earlier order through a reseller was cancelled because Certum had discontinued the product **for resellers**. Issuance has not been started; this file does not state why, because that has not been established.)*
- **MSI install / run / uninstall** stays elevation-gated and was not exercised. The payload is hash-verified above — the three binaries inside the CAB, two of them byte-identical to the standalone artifacts and the third differing only in Tauri's three-byte install-form stamp.
- **The LLM leg of the chat path was not exercised.** OpenAI answered `429 Too Many Requests` throughout, which is why the session-state probes used `/api/llm/select` (no model call) and K2 used a local Ollama model instead. The SOT and integrity measurements are unaffected: both read confirmed values the engine computed, and the integrity guard was exercised through a stubbed provider.
- **The Stooq failover test** (`cargo test -- --ignored failover_adopts_stooq`) cannot run on this network: Stooq answers with its anti-bot web page instead of CSV. The test forces the primary down through an unreachable proxy and needs the real secondary to answer, so this is an environment condition rather than a product fault — and what the engine did with it is the behaviour §3 promises: it named both causes (`Market data API request failed … the fallback provider did not answer either: Stooq answered with a web page instead of CSV data (its anti-bot check, most likely); the fallback provider is unavailable on this network`) and fabricated nothing.
- **A runtime prompt injection (H1)**, for the reason given above.

**The release is published** — 2026-10-03 07:09 JST (tag `v2.9.10` at `b3ef8e1`).

### v2.9.9 (macOS) — inspected 2026-09-27 on the CI-built artifacts, signed and notarized — PASS (static, SOT, runtime, penetration, payload identity, SCA, SBOM, signing/notarization, VirusTotal, ZAP)

The macOS half of 2.9.9. It is the **second** set of macOS artifacts for this version; the first was discarded, and why is the substance of this record.

**The first set was built against a source the published Windows half does not share.** A one-line fix to a vendored `proc-macro-error2` was added to the macOS branch to silence an E0365 that rustc will make a hard error. `[patch.crates-io]` changes a package's source id, Cargo folds a unit's dependencies — proc-macros included — into its `-C metadata`, and that propagates to the eight crates compiled to wasm32. **Measured, not inferred:** vendoring a byte-identical copy of an unrelated proc-macro crate (`paste` 1.0.15, `diff -r` clean) and patching it in changed the WASM on its own, `7cc029f2…` → `5e12fc6d…`. The `data`, `type`, `import`, `export`, `memory`, `global` and `table` sections stayed byte-identical; only `code` (−135 B), `function` (+4 B) and `element` (+6 B) moved. Same program, different bytes — and under §C.1 rule 5 a v2.9.9 that carried the fix could no longer reproduce the **published, already-downloaded** Windows MSI. The fix was therefore dropped from this version and the artifacts rebuilt.

**This build is program-identical to the published Windows source.** `git diff 6f6657f 5082f32` over `src/`, `webui-leptos/`, `xoksa-desktop/src/`, `xoksa-setup/src/`, `xoksa-paths/`, every `Cargo.toml` / `Cargo.lock` and both `tauri.conf.json` is **empty**. The eight files that differ are workflows, `.gitignore`, documentation and `rust-toolchain.toml` — none of them compiled.

**Build provenance — GitHub Actions `Release build (macOS)` run `36283764077`, workflow `.github/workflows/release-macos.yml`, runner `macos-14`, source commit `5082f32` on `main`** (not on a branch — `workflow_dispatch` registers only from the default branch, which is also what §C.1 requires). Toolchain `rustc 1.98.1 (48a229cea 2026-09-01)`, `cargo 1.98.1`, `trunk 0.21.14`, `tauri-cli 2.11.4`. **The build log contains zero compiler warnings.**

**The toolchain is pinned now, and the pin was verified rather than assumed.** `rust-toolchain.toml` carries `1.98.1` — the version that produced the published Windows artifacts (run 36134676892), so the hashes already recorded stay reproducible. Each job reads the channel out of that file: `dtolnay/rust-toolchain` only runs `rustup default`, so leaving `stable` there would have had rustup **auto-install** the pinned toolchain on the first cargo call, a path rustup prints as deprecated. CI log: `rustup toolchain install 1.98.1`, `rustup default 1.98.1`, and the string `stable` **zero** times. Locally, `rustup show` reports `active because: overridden by '…/rust-toolchain.toml'` — the check that matters, since `stable` happens to be 1.98.1 today and a version number alone cannot say which mechanism chose it.

**Frozen macOS artifacts** (signed, notarized, stapled; every one verified against the CI-published `SHA256SUMS` before signing — 5/5 OK):

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| `XOKSA-2.9.9-arm64.dmg` (download) | `700303e4598c1181c62b1b0e0367ff82297737e165ba72b47f0f5c839c38a3d5` | 11,700,991 B |
| `XOKSA.app/Contents/MacOS/xoksa` (engine) | `34159ab7a827e7eb08bba2c3cb9790380b93f0f479c107473153b578c7aa5848` | 11,997,984 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop` (shell) | `de07bb115a379a24a5696cc8aaea2ffae97103778db2ed402f78f96ef3025436` | 6,346,000 B |
| `XOKSA.app/Contents/MacOS/xoksa-setup` (settings app) | `5af2432d5c46cdc86c8c3aab1f3ef097f05ffc991f14dc6e6ffe5d2f08189c9c` | 5,914,400 B |
| `xoksa-webui-6d3d14e56100a0b9_bg.wasm` (frontend embedded in the engine) | `1967db8dadb2591bf085f367c611a087d6d5c28867b4dec4997860535407568b` | 710,943 B |

**The WASM is not byte-identical to the Windows half's, and that is expected.** Windows ships `xoksa-webui-9516f21d2d5de561` (`bce0f631…`). The source is identical, so the difference is the build host — the same class of difference as the engine binaries themselves, which are per-OS artifacts. What §C.1 calls deterministic was re-measured and holds **per host**: two builds of this tree on this machine produced byte-identical WASM. Cross-host byte-identity has never been claimed and is not required; what is required, and is measured above, is that both halves come from the same source.

**No artifact names its builder.** `scripts/check-no-host-paths.sh` passes on all four. A raw scan for `/Users/<anything>` returns **zero** in the engine, the desktop shell and the WASM; the settings app carries **one**, `/Users/runner/work/xoksa-dev/xoksa-dev/xoksa-setup`, the runner's own workspace, which identifies nobody and is present in the unsigned CI output too (a build-time macro embeds it as data, so `--remap-path-prefix` cannot reach it). For contrast, the 2.9.8 macOS artifacts — developer-machine builds — carried `/Users/koz` **616** times in the engine, 336 in the shell, 280 in the settings app and 85 in the WASM. The gate was re-run **after** signing and still passes.

**Signing pipeline — the CI artifact is the input, not a rebuild.** The three inner binaries and the bundle were signed `Developer ID Application: Kohzoh Tsuchiya (463YK5WF6H)` with the hardened runtime, notarized, and stapled; the DMG was built locally from the stapled `.app` with an `/Applications` symlink, then signed, notarized and stapled in turn. `notarytool` **Accepted** for both; Gatekeeper reports **`source=Notarized Developer ID`** for app and DMG.

**Payload identity — one byte, and it is the byte signing has to move.** With signatures stripped, each binary differs from its CI original in **exactly one byte**, in `__LINKEDIT`'s `vmsize`, grown by one 16 KiB page to hold the signature (`0x28000`→`0x2c000` engine, `0x18000`→`0x1c000` shell, `0x14000`→`0x18000` settings app). Code and data are byte-identical to what CI produced.

**Static — PASS.** `check.sh` all fifteen gates green on the pinned toolchain; `cargo test` **485 passed / 0 failed / 1 ignored** (the ignored one is the live-Stooq failover e2e).

**SOT — PASS, checked column by column.** Measured on a Sunday with Tokyo and New York closed, so the series could not move between reads. The CSV log is positional, so its **51 columns** were joined to the header `--show-log-header` emits and mapped onto the JSON log and `/api/symbol/7203/summary`: **50 of 51 agree**. The one that does not is `analyzed_at_timestamp` — when the run happened, not a computed value — which differs by the seconds between calls, as in the Windows record. Identical across all three: `final_score` **−6.0**, `rsi` **40.681851**, `macd` **−7.415265**, confirmed close **2989.5**.

**Runtime & penetration (B.5 against the frozen signed engine `34159ab7…`) — PASS.** `inspect.sh` **9/9**, including the host-path gate as check 9. Six security headers present; the CSP carries a per-request nonce and **zero** occurrences of `unsafe-inline`; two clients receive two distinct 32-hex `xoksa_sid` (`HttpOnly`, `SameSite=Strict`); all six path-traversal payloads **404**; `text/plain` **415**, cross-origin POST **403**, a 5 MB valid rule **413**.

**The frontend under test is the one that ships.** The engine was started from a directory with no `webui-leptos/dist` — `--web-dir` defaults to that relative path and `read_asset` prefers an on-disk file over the embedded copy, which invalidated a scan earlier in this release — and the served page references `xoksa-webui-6d3d14e56100a0b9`, the WASM in the table above.

**Functional — the app was run, not only inspected.** Launched with an isolated `HOME` so the operator's real configuration was untouched (verified by mtime before and after): the desktop shell spawned the engine as a child on an **ephemeral port** (`serve --ui --host 127.0.0.1 --port 57555`, §6's contract), `/api/health` answered `xoksa-web 2.9.9`, and the page it served referenced this artifact's own WASM.

**SCA — 0 vulnerabilities.** `osv-scanner`: engine **0** (338 packages), `xoksa-paths` **0** (18), WASM **3** (188), `xoksa-desktop` **7** (420), `xoksa-setup` **7** (446); the last two render as eight rows because `GHSA-wrw7-89jp-8q8g` is an alias of `RUSTSEC-2024-0429`. `cargo audit`, `cargo deny check` and `cargo machete` pass in both workspaces. Neither finding that could matter reaches a shipped macOS file: `glib` 0.18.5 (6.9, the only CVSS anywhere) is in the gtk-rs Linux bindings, which are not compiled into the WKWebView binaries, and `event-listener` 5.4.1's `RUSTSEC-2026-0221` — a `!Send` tag crossing a thread boundary — needs threads, while the shipped WASM's memory section carries `limits_flags=0x00`, non-shared memory.

**SBOM — one regenerated, one measured and left alone.** `sbom/xoksa-native-macos-arm64.cdx.json`: CycloneDX 1.5, **221 `library` components**, **derived from a throwaway `cargo auditable` build of the same lockfile — not extracted from the shipped binary**, which CI does not build with `auditable`. Against the published inventory the library set is unchanged; the only difference is the crate's own version (`xoksa@2.9.8` → `xoksa@2.9.9`). The file also loses a 222nd component of type `file` that named the scanned binary by absolute path (`/private/tmp/sbom298/xoksa`); the recipe now sets `SYFT_FILE_METADATA_SELECTION=none`, so all five inventories are `library`-only. `sbom/xoksa-webui-leptos.cdx.json` was regenerated and compared: **zero difference in the library set**, so the published file stands — dropping the vendored patch returned the lockfile to the state that produced it. **`cargo audit bin`** over the **245** dependencies embedded in the throwaway: **0 vulnerabilities**; the throwaway tree was removed and no shipped file was touched.

**VirusTotal (on the shipped hashes, 2026-09-27) — all four clean.** DMG `700303e4…` **0/61**; engine `34159ab7…` **0/62**; desktop shell `de07bb11…` **0/62**; settings app `5af2432d…` **0/63**.

**OWASP ZAP passive scan (Parrot host, `0.0.0.0` over the LAN, against the frozen signed engine) — 8 alerts, 0 High / 2 Medium / 1 Low / 5 Info, identical in categories and verdicts to the Windows half.** The run is bound to *this* artifact: the alerts name `xoksa-webui-6d3d14e56100a0b9`. Both Mediums were re-measured here rather than carried over:

- **Anti-CSRF token missing on `GET /login`.** `/auth` is the one POST exempt from the origin check, gated instead by the 256-bit token itself. Measured on this engine: a cross-origin POST with a wrong token returns **401**, an empty token **401**, and the correct token **303** — the documented token-for-cookie exchange. The session cookie is `HttpOnly`, `SameSite=Strict`.
- **SRI attribute missing**, reported against the `.wasm`. The HTML the engine actually serves carries `integrity="sha384-…"` on **all three** subresources (stylesheet, JS glue, WASM) — measured. ZAP is applying an HTML passive rule to binary content.

The Low is Unix-timestamp disclosure on `/api/symbol/…/summary`, which is market-data time by design. The five Info are the documented baseline: cookie poisoning on `POST /auth` (that same token-for-cookie exchange), session-management response, Modern-Web-App, localStorage disclosure (UI display state; the session id is the server-side `HttpOnly` cookie, not in localStorage), and a suspicious comment inside `wasm-bindgen`'s own glue.

### v2.9.9 (Windows) — inspected 2026-09-26 on the CI-built artifacts; **published 2026-09-26 15:38 JST** (tag `v2.9.9` at `83cfb38`) — static / SOT / runtime / penetration / ZAP / SCA / SBOM PASS; VirusTotal deferred until signing; macOS shipped separately (record above)

v2.9.9 exists because the 2.9.8 documentation audit reached the indicator code. Four score ladders compared an absolute price difference, so the same threshold meant a 0.2 % move on one instrument and a 124 % move on another; three of nine ladders disagreed with the other six about whether a reading exactly on a threshold meets it; `normalize_ticker_input` answered an index name with a tracking fund, and `FANG+` with a 3× leveraged ETN measured at 3.1× QQQ's daily move; and a failed fetch discarded the provider's own reason before reporting a generic failure. The release therefore changes computed scores, so every gate was re-measured on the new artifacts rather than carried over.

**Why these artifacts and not the first set.** An inspection of developer-machine builds was taken on 2026-09-24/25 and then discarded, because of the defect it did not look for: rustc records each crate's source path for panic messages, so those binaries carried the builder's account name — `C:\Users\<name>` **689 times in the engine** and **85 times in the WASM the browser downloads** — which `strip = true` does not remove. `--remap-path-prefix` was measured (690 → 168 occurrences) and does not close it: it cannot reach the paths baked into the `std` rlibs rustup ships, nor those a C toolchain (`aws-lc-sys`) embeds through cc-rs, and `trim-paths` is not stabilised in this Cargo. The release is therefore built by CI, where the build account identifies nobody. **Every measurement below is taken on the CI artifacts**; nothing is carried over from the discarded run except the findings that are about the *source* rather than the artifact.

**Build provenance — GitHub Actions `Release build (Windows)` run `36134676892`, workflow `.github/workflows/release-windows.yml`, runner `windows-latest`, source commit `6f6657f`** (the Squash & Merge of #166 into `main`, and the commit the CI-published `SHA256SUMS` names in its own header). Toolchain: **`rustc 1.98.1` (`48a229ce`) — read out of the shipped engine itself rather than from a build log** — with `trunk 0.21.14` and `tauri-cli 2.11.4` pinned in the workflow. Unsigned. The working tree at inspection time is that same commit, clean.

**These artifacts are the same program the discarded ones were.** `git diff a17c3f0 6f6657f` over `src/`, `webui-leptos/src/`, `xoksa-desktop/src/`, `xoksa-setup/src/`, `xoksa-paths/`, every `Cargo.toml` and `Cargo.lock`, `tauri.conf.json` and the icon sets is **empty**; the fifteen changed files are documentation, SBOMs, scripts and CI configuration. The binaries differ because the builder and the compiler differ (`1.97.1` → `1.98.1`), not because the program does.

**Frozen Windows artifacts — every one verified against the CI-published `SHA256SUMS`: 5/5 OK.**

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| `XOKSA-2.9.9-x64.msi` (installer, carries the three binaries below) | `8460f11c061435ca50da57d019552945747566f69c86613aa9830a5d975f8849` | 11,702,272 B |
| `xoksa.exe` (engine) | `087f2ba97d4f73e5b196ac2e36d48245facc6066dec939d432b35207fc70ce17` | 14,812,160 B |
| `xoksa-desktop.exe` (connection shell) | `fa26f6ad5365a11673a118d49bf9ae117e169834b5862a7dc62c2d3a2f46a014` | 9,629,696 B |
| `xoksa-setup.exe` (settings app) | `41f6d732774c76fcbefea116499c5ec18f9b67b432600bc7717bfe181aeacb85` | 8,414,720 B |
| `xoksa-webui-9516f21d2d5de561_bg.wasm` (frontend embedded in the engine) | `bce0f63124767841b5dc596f2be92a3ceba090968a81443a05f872ea20022727` | 711,103 B |

**No artifact names its builder — the reason for the rebuild, measured.** `scripts/check-no-host-paths.sh` passes on all five. The desktop shell, the settings app and the WASM carry **zero** user paths of any kind. The engine carries **142**, and every one is the GitHub-hosted runner's own built-in account — `C:\Users\runneradmin` (97) and its 8.3 short form `C:\Users\RUNNER~1` (45, from `aws-lc-sys`'s C build) — which identifies nobody. The discarded local engine carried 689 and its WASM 85. The gate is check 9 of `inspect.sh` and a required step of the release workflow, so a build that would reintroduce this fails before upload.

**Payload identity — measured by extracting the MSI.** The engine and the settings app inside the installer are **byte-identical** to the standalone artifacts above. The desktop shell differs in **exactly three bytes, at `0x77190f`–`0x771911`**, where Tauri stamps the install form (`__TAURI_BUNDLE_TYPE_VAR_UNK` → `…_MSI`) — reproducing v2.9.8's finding exactly, so the installer carries no other modification. All six binaries (three standalone, three extracted) report `FileVersion` **2.9.9**: the settings-app sidecar drift that the first v2.9.9 build hit does not recur, because `scripts/bundle-engine.sh` now refuses to stage a sidecar that does not report the version in its `Cargo.toml`.

**AV false-positive recipe held on CI.** Engine `.rsrc` **2,048 B at entropy 4.168** and `.rdata` **5,181,440 B at 5.875**; desktop `.rsrc` **32,768 B at 3.897**; settings app `.rsrc` **32,768 B at 3.899** — every figure within 0.01 of the discarded local build's, so moving the build to CI did not move the axis the behavioural ML engines score.

**Static — PASS.** CI ran the `scripts/check.sh` gate set on this commit (#166 green). Re-measured on the same tree: `cargo audit` over **338** crate dependencies — **0 vulnerabilities**; `cargo deny check` — advisories / bans / licenses / sources **all ok**; `cargo machete` — no unused dependency; `osv-scanner` per lockfile — engine **0**, `xoksa-paths` **0**, WASM **3**, desktop **8**, settings app **8** (unchanged from the discarded run, the lockfiles being unchanged).

**SOT — PASS, checked column by column rather than on three sample values.** The CSV log is positional, so its 51 columns were mapped back to the JSON log's keys: **50 of 51 identified, and all 50 agree with `/api/symbol/AAPL/summary`**. The one unmatched column is `analyzed_at_timestamp` — when the run happened, not a computed value; the API's copy differs from the log's by the 20 seconds between the two calls, which is correct behaviour rather than a mismatch. Identical across all three paths: `latest_observed_price` **341.07**, `final_score` **5.0**, `bar_close` **341.07000732421875**, `rsi` **70.41048490649925**.

**Runtime & penetration (B.5 against the frozen engine `087f2ba9…`) — 21/21 PASS.** `inspect.sh` **9/9**, now including the host-path gate as check 9. Extended set **12/12**: `--version`, `/api/health` and the served page agree on `xoksa-web` **2.9.9**; six security headers present; the CSP carries a per-request nonce and no `unsafe-inline` (two fetches return different nonces); `xoksa_sid` is 32 hex, `HttpOnly`, `SameSite=Strict`, and two clients receive distinct ids; all six path-traversal payloads 404; cross-origin `GET /` 200 while the Content-Type and cross-origin-POST gates return 415 and 403; a 5 MB body 413. **No implicit `./xoksa.env`** — a decoy env file planted in the launch directory (`SELL_RSI=99`, `LANG=en`) was ignored, so §6's single-config-location contract holds on the artifact. **Browser render** — headless Edge mounted the WASM app: 5,020-character DOM, 3 selects, 4 inputs, 7 buttons, Japanese UI.

**The frontend under test is the one that ships — proved, not assumed.** The served page references `xoksa-webui-9516f21d2d5de561`, and the `_bg.wasm` it serves hashes to `bce0f631…`, byte-identical to the WASM in the artifact table. This was checked rather than assumed because this release's first ZAP scan was invalid for exactly this reason: `--web-dir` defaults to the **relative** path `webui-leptos/dist`, and `read_asset` prefers an on-disk file over the embedded copy — a deliberate development convenience, documented in `src/server/mod.rs` — so an engine started from the repository root serves the working tree's frontend rather than its own. §C.1 now requires the inspected engine to be started from a directory that has no `webui-leptos/dist`.

**Exposed mode (`--host 0.0.0.0`) — measured on the frozen engine, over the LAN.** The LAN address answers **401** unauthenticated and **200** (`{"status":"ok","service":"xoksa-web","version":"2.9.9"}`) with the 64-hex bearer token. `--private` with a non-loopback host is refused at startup with its reason named: *"Refusing to bind non-loopback host '0.0.0.0' in --private mode: an auth token cannot be persisted."*

**OWASP ZAP passive scan (Parrot host, `0.0.0.0` over the LAN, against the frozen engine `087f2ba9…`) — 8 alerts (0 High / 2 Medium / 1 Low / 5 Info), none a defect, and identical to v2.9.8 in categories and verdicts.** The run is bound to *this* artifact: the alerts name `xoksa-webui-9516f21d2d5de561`, the frontend this engine embeds and serves.

- **SRI attribute missing (Medium) — traced to its bytes.** The alert's URL is the `.wasm`, not an HTML document, and its evidence — `<link rel="stylesheet" href="…">` — is the Rust `format!` template at `webui-leptos/src/main.rs:3199`, compiled into *this artifact's* wasm data section at offset **`0x09ce6e`**, where the `{origin}` and `{file}` placeholders read as non-printable bytes. ZAP is applying an HTML passive rule to binary content. The HTML the engine actually serves carries `integrity="sha384-…"` on **all three** subresources (measured), and the popup stylesheet the frontend builds at runtime is same-origin under `style-src 'self'`, where SRI — a control against a third-party host — has nothing to protect.
- **Anti-CSRF token missing (Medium) on `GET /login`** — unchanged and deliberate: `/auth` is the one POST exempt from the origin check (`src/server/mod.rs`, with the reason stated in the code), gated instead by the 256-bit token itself. Measured: a cross-origin POST carrying a wrong token returns **401**, and an empty token **401**.
- **Cookie poisoning (Info) on `POST /auth`** — ZAP correctly observes that the `token` parameter reaches a cookie. Measured: a successful exchange sets `xoksa_auth=<the token verbatim>` and `xoksa_sid=<32 random hex>`, both `HttpOnly` and `SameSite=Strict`, neither carrying `Max-Age` (session cookies). The auth cookie **is** the credential rather than a derived session id. That is accepted here — the operator had to put the token on that device anyway, `HttpOnly` blocks script access, `SameSite=Strict` blocks cross-site send, the browser profile is inside the OS trust boundary §0.2 puts out of scope, and `--rotate-serve-token` invalidates every browser session at once, which a separate id would not give for free. The cost is recorded rather than omitted: a passive LAN observer who captures one request obtains the master credential — which also opens the CLI/API — rather than a browser-scoped id that dies with the tab. Issuing a server-side random id is listed as optional hardening below.
- **Unix timestamp disclosure (Low, 3 instances) on `/api/symbol/AAPL/summary`** — measured: `bar_timestamp` 1790343000 (2026-09-25 13:30 UTC), `market_data_latest_timestamp` 1790366401 (20:00:01 UTC), `analyzed_at_timestamp` 1790388074 (2026-09-26 02:01:14 UTC). Market and analysis times, Class C under §0.1, and required by §1 — the bar a reading was computed on travels with it. Nothing about the host is disclosed.
- **Information disclosure — browser localStorage (Info, 5 instances)** — all nine keys enumerated from the source: `theme`, `font`, `symbol`, `tickers`, `targets`, and the pane geometry `right_w`, `fund_h`, `chat_h`, `news_h`. Display state and ticker symbols (Class C); no credential and no session id (`xoksa_sid` is an `HttpOnly` cookie, server-side).
- **Suspicious comment (Info)** — measured on the served glue JS: 19 `//` comments and one `TODO` — *"// TODO we could test for more things here, like `Set`s and `Map`s."* — every one of them wasm-bindgen's own generated output, not xoksa source. The shipped `.wasm` contains **no comment at all**; its only `/*` is a chance byte pair at offset `0x09d2c9` inside binary data.
- **Session-management response identified** and **Modern Web Application** (Info) — correct detections of the `xoksa_sid` cookie and of a JS-driven UI, not findings.

`/api/chat/stream` (SSE) stays outside the scan — a proxy buffers it into a 502 — and its controls were verified directly.

**SCA — PASS, with one layer measured less precisely than last time, and the reason is the build host.** `cargo audit` 0 vulnerabilities over 338 dependencies; `cargo deny check` advisories / bans / licenses / sources all ok; `cargo machete` no unused dependency; `osv-scanner` as tabulated above. The release workflow builds with `cargo build --release`, **not** `cargo auditable build`, so the shipped engine carries no embedded dependency list and `cargo audit bin` recovers only **109** of its dependencies from panic strings instead of all of them. The previous record's "254 embedded dependencies, 0 vulnerabilities" therefore **cannot** be restated for these artifacts as an on-artifact measurement, and is not.

**SBOM — unchanged, with the basis for saying so stated rather than assumed.** The five published inventories are current: `xoksa-native-windows-x86_64.cdx.json` **225** components, `xoksa-native-macos-arm64.cdx.json` **222** (still bound to the 2.9.8 macOS binary until the Mac builds 2.9.9), `xoksa-desktop-windows-x86_64.cdx.json` **196**, `xoksa-setup-windows-x86_64.cdx.json` **212**, `xoksa-webui-leptos.cdx.json` **188** — all CycloneDX 1.5, none carrying an absolute path (measured). They were generated from throwaway `cargo auditable` builds during the discarded local inspection, and they remain valid for the CI artifacts because **`git diff a17c3f0 6f6657f` over every `Cargo.toml` and `Cargo.lock` is empty**: the dependency set a build resolves is fixed by the lockfile, the feature selection and the target, none of which changed. What is **not** claimed is that they were read out of *these* binaries — they could not be, for the `cargo auditable` reason above. **Recorded as the fix for the next release:** either the workflow gains a throwaway `cargo auditable` build whose output feeds the SBOM and is then discarded, or SBOM generation moves into CI beside the release build. Until one of those lands, the inventories describe the shipped dependency set by derivation rather than by extraction.

**Findings carried forward to the next version — both re-verified on this tree, both unchanged.**

- **`native-tls` is compiled into the shipped engine unused.** `cargo tree -i native-tls` shows a single parent: xoksa's own `Cargo.toml`. `reqwest` 0.13.5 resolves to rustls, and `hyper-tls` is absent from the binary-derived SBOM, so reqwest has had no native-tls path since #159. Both comments on the declaration are therefore false, and `[package.metadata.cargo-machete] ignored = ["native-tls"]` suppresses the one check that would report it — which is why `cargo machete` is clean above. `native-tls` 0.2.18 and its only unique child `schannel` 0.1.28 ship as dead code. Not a vulnerability; unnecessary attack surface in a binary that handles Class A credentials.
- **The settings app is outside the static gate set.** `xoksa-setup` appears in neither `scripts/check.sh` nor `.github/workflows/audit.yml`; its only two mentions in `ci.yml` are the comment and the `touch` that create the sidecar stub `tauri-build` validates. So it receives no `fmt --check`, no `clippy -D warnings`, no `test`, and no `cargo audit` / `cargo deny` over its own lockfile — a shipped binary that collects Class A credentials (§6).

**New this release, minor:** `deny.toml` ignores `RUSTSEC-2026-0173` (`proc-macro-error2`, unmaintained) and `cargo deny` now warns that **no crate matches it** — the dependency is gone and the entry is stale. Removing it restores the warning's usefulness.

**Optional hardening, for the owner to decide:** issue a random session id bound server-side to the serve token instead of setting the token verbatim as `xoksa_auth`, so that a captured cookie is not the master credential (see the ZAP cookie item above).

**Binary-bound items pending (recorded at ship-time, per this C.4 convention).** **VirusTotal** is excluded this cycle by the owner's stated sequence — publication must precede signing, and an unsigned, brand-new hash would only reproduce the first-seen reputation penalty of [§6.2 / §6.3](./av-false-positive-case-study.md) (Windows signing = Certum Open Source Code Signing, purchased, issuance pending). **MSI install / run / uninstall** stays elevation-gated and was not exercised; the payload is hash-verified above. **The live-Stooq failover e2e is the one ignored test** — this network's anti-bot challenge blocks it. **A second external condition is recorded rather than hidden:** the CLI analysis used for the SOT check exited non-zero because OpenAI answered `429 Too Many Requests`; the technical computation and both logs completed, so the SOT comparison is unaffected, but the LLM leg of the CLI path was not exercised on that run. The **macOS** side — its own CI build, Developer ID signing and notarization, its own inspection, and the macOS engine SBOM — is handed to the Mac.

### v2.9.8 (macOS inspected 2026-09-23) — PASS (static, SOT, runtime, penetration, SCA, SBOM, signing/notarization, VirusTotal, ZAP)

The macOS half of 2.9.8, shipped from a `main` that carries two fixes the Windows inspection could not have seen. **#159** batched five dependency updates — serde, csv, dirs 6→7, reqwest 0.12→0.13, axum 0.7→0.8 with the route-syntax change 0.8 requires — and left CI red on `main`: reqwest 0.13 verifies TLS through `rustls-platform-verifier`, which pulls in `webpki-root-certs` (Mozilla's CA bundle) under **CDLA-Permissive-2.0**, a licence the `deny.toml` allowlist had never been asked about. It is allowed in **#160** with the reasoning recorded beside it — certificate data rather than code, no copyleft, compatible with shipping under MIT — and the desktop lockfile picked up the `dirs` bump #159 had applied only to `xoksa-paths`.

**A guard was added for what stopped the suite entirely.** The repository lives under `~/Documents`, so iCloud had quietly accumulated **72 sync-conflict copies** (`Cargo 2.toml`, `icon 3.png`, …) — untracked, absent from `git status`. One of them, `tests/no_drift 2.rs`, turned into a crate name and stopped **all 453 tests** from compiling: `invalid character ' ' in crate name`. The failure list was empty; only raw cargo output named the cause. **#161** adds `no_icloud_conflict_copies`, which walks the tree and fails with the offending paths. It is containment: the cure is moving the repository out of iCloud, which would also end the `codesign … detritus not allowed` staging dance at every signing step.

**Build provenance — source commit `76120d5` (`main`), clean working tree at build time.** Same recipe: `trunk build --release` → `cargo build --release --bin xoksa` → `scripts/bundle-engine.sh` → `xoksa-setup` staged as the second sidecar → `cargo tauri build` with `APPLE_SIGNING_IDENTITY`. Host `aarch64-apple-darwin`. Built once, signed, notarized, and frozen; every check below was measured on those files.

**Frozen macOS artifacts** (signed, notarized, stapled):

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| `XOKSA-2.9.8-arm64.dmg` (download) | `008f2142c9854ae5219b0905a4cf684ba650ce02ffb20cb0176c12866a42aa50` | 11,826,991 B |
| `XOKSA.app/Contents/MacOS/xoksa` (engine) | `aec27e19787b8e84fd599905df2d8eed3315e2ffebab589fe8259088c3cc009e` | 11,997,920 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop` (shell) | `aec1a1ad3f2e75e493d8aaf6bd97a3b4cfebb36d0520441ec88c3bc8c4f7f987` | 6,395,376 B |
| `XOKSA.app/Contents/MacOS/xoksa-setup` (settings app) | `cc60654568e0fd4946095c0cf1727a9f9fc1aed3d42afc7c906cf6b91ce20637` | 5,963,792 B |

**Static — PASS.** `check.sh` all sixteen gates green; `cargo test` **454 passed / 0 failed / 1 ignored** — 438 lib, 10 integration, **6 drift guards** (the iCloud guard is the sixth), plus `xoksa-paths`; the ignored one is the live-Stooq failover e2e.

**SOT — PASS.** Measured after the Tokyo close so the series could not move between runs: `final_score` **−10.0**, RSI **45.98**, MACD **−0.2630** and the confirmed close **3025.0** identical across the CSV log, the JSON log, and `/api/symbol/7203/summary`.

**Runtime & penetration (B.5 against the frozen signed engine `aec27e19…`) — PASS.** `inspect.sh` **8/8**; `--version` and `/api/health` both 2.9.8; the security headers present; two clients receive two distinct 32-hex `xoksa_sid`; path traversal **404 on all six payloads**.

**Payload identity.** With signatures stripped, `xoksa` and `xoksa-setup` are **byte-identical** to their build output; `xoksa-desktop` differs by **one byte** in `__LINKEDIT` `vmsize`, as in every macOS release since 2.6.7.

**Signing / notarization — PASS.** `Developer ID Application: Kohzoh Tsuchiya (463YK5WF6H)`, hardened runtime on the bundle and all three inner binaries; `notarytool` Accepted for app and DMG, both stapled; Gatekeeper **accepted, source=Notarized Developer ID** for both.

**SCA — 0 in the shipped engine.** `osv-scanner`: engine **0** (338 packages — up from 306 with the reqwest 0.13 tree), `xoksa-paths` **0** (18), WASM **3** (188), `xoksa-desktop` **7** (420), `xoksa-setup` **7** (446). The only finding carrying a CVSS is `glib` 0.18.5 (6.9) in the gtk-rs Linux bindings, which are not compiled into the macOS WKWebView binaries; the rest are unmaintained-crate notices (`proc-macro-error`, the `unic-*` family).

**SBOM — regenerated.** `sbom/xoksa-native-macos-arm64.cdx.json`: CycloneDX 1.5, **221 `library` components**, no absolute path — up from 209 at 2.9.4, the reqwest 0.13 platform-verifier tree being the bulk of the difference. **`cargo audit bin`** over the **245** embedded dependencies: **0 vulnerabilities**. `sbom/xoksa-webui-leptos.cdx.json` was checked against its lockfile and needs no regeneration (unchanged since #149). The `target-sbom` throwaway was removed; the shipped binary was untouched.

**VirusTotal (on the shipped hashes, 2026-09-23) — all four clean.** DMG `008f2142…` **0/60**; engine `aec27e19…` **0/63**; desktop `aec1a1ad…` **0/63**; settings app `cc606545…` **0/63**.

**OWASP ZAP passive scan (Parrot host, `0.0.0.0` over the LAN, against the frozen signed engine) — 8 alerts, 0 High, 1 Medium, unchanged in categories and verdicts from v2.9.4.** The scan was re-run rather than carried over because axum 0.7 → 0.8 changes the routing layer this release ships. The Medium — no Anti-CSRF token on `GET /login` — was re-measured on this artifact: a cross-site POST to `/auth` with a wrong token returns **401**, and only a caller already holding the 256-bit token receives a session (**303**), which such a caller could obtain by reaching the engine directly. The session cookie remains `HttpOnly` + `SameSite=Strict`. The rest are the documented baseline: SRI-missing on the same-origin WASM bundle, Unix-timestamp disclosure on `/api/symbol/…/summary` (market-data times by design), cookie poisoning on `POST /auth` (the documented token-for-cookie exchange), session-management response, Modern-Web-App, a suspicious comment in `wasm-bindgen`'s own glue, and localStorage disclosure — **nine** UI-state keys (`theme` / `font` / `symbol` / `tickers` / `targets`, plus the pane geometry `right_w` / `fund_h` / `chat_h` / `news_h`, present since the 2.1.0 frontend), every one display state with no credential and no session id (`xoksa_sid` is an `HttpOnly` cookie, server-side), the language having moved to `xoksa.env` in 2.9.4. *(Corrected 2026-09-25, during the v2.9.9 inspection: this paragraph previously named five localStorage keys; the four pane-geometry keys have been present since the 2.1.0 frontend. No verdict changes.)*

### v2.9.8 (Windows inspected 2026-09-15; not yet shipped) — static / SOT / runtime / penetration / ZAP / SCA / SBOM PASS; binary-bound (VirusTotal, signing, macOS) pending

v2.9.8 exists because a pre-merge review of 2.9.7 found three defects in the integrity guard, and because an advisory landed the day after 2.9.7's inspection measured `cargo audit` clean. All three defects are the guard erring toward **removing a correct sentence** rather than toward admitting a fabricated number, and all three are reachable only in an English answer: the bare-price and numeric-unit checks re-read each figure from the isolated capture, where the words that make it a date or a window length are not visible (`Support has held since 2019.` was removed as a fabricated price); `contains_any_term` matched substrings, so `low` matched *below* and `line` matched *decline*, opening the price-context gate on almost any English answer; and `safe_ratio` had narrowed its denominator guard from `abs() > f64::EPSILON` to `!= 0.0` when the per-provider guards were concentrated in 2.9.7, letting an EPS of `1e-300` produce a finite `1.8e302` PER that reached confirmed data. **`rustls` 0.23.32 → 0.23.45** closes RUSTSEC-2026-0285 (TLS 1.3 handshake messages accepted across encryption level boundaries, 5.3 medium), published 2026-09-14 — the day after the v2.9.7 inspection recorded a clean `cargo audit`, so 2.9.7 would have shipped carrying it. See CHANGELOG [2.9.8].

**A known limitation is stated rather than claimed fixed.** A year written anywhere in a sentence still dates a reading in another clause, because the year qualifier falls back to the nearest mention in the sentence when its own clause names none — a fallback required for `In 2019, revenue was …` and harmful for `Price has been in this range since 2019, and RSI is 45.2.` Separating them means changing the normative rule in [security-design.md §1](./security-design.md), so it is tracked in Issue #146 with its measurements rather than changed on the eve of a release.

**Build provenance — source commit `e8b973d` (branch `release/v2.9.8`, branched from `main` `c79d5ad` [v2.9.7]), clean working tree at build time.** Same recipe: `trunk build --release` → `cargo build --release --bin xoksa` → `scripts/bundle-engine.sh` → `xoksa-setup` staged as the second sidecar → `cargo tauri build`. `rustc 1.97.1`, LLVM 22.1.6, `tauri-cli` 2.11.4, host `x86_64-pc-windows-msvc`, unsigned. Built once and frozen; every check below was measured on these files, and the engine's hash was cross-verified (`sha256sum` + `CertUtil`).

**Frozen Windows artifacts:**

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| `XOKSA-2.9.8-x64.msi` (installer, carries the three binaries below) | `7868ea00803b596debc352bb34f7761dff650ef9e3fbdf85f5e58e629b530d98` | 10,612,736 B |
| `xoksa.exe` (engine) | `7fff208573fc8b4e9add1950201d4d3bb5d3b6fda5a7a15ec0507b5e3499a0a3` | 12,625,408 B |
| `xoksa-desktop.exe` (connection shell) | `cf42dd2e908dc65b045f0e05bb9328065a84f74f6d45a4ceafb3473304d69e7f` | 9,646,080 B |
| `xoksa-setup.exe` (settings app) | `859273b1024f9189daff94b90af60df828bf1bf242d97f3cfed0957ad2543d84` | 8,427,520 B |

The MSI was moved to `target/release/XOKSA-2.9.8-x64.msi` — the one canonical location, one artifact — the NSIS output discarded, and the superseded v2.9.7 MSI removed.

**Payload identity — measured by extracting the MSI.** The engine and the settings app inside the installer are **byte-identical** to the build output. The desktop shell differs in **exactly 3 bytes**, at `0x773FFF`, where Tauri stamps how the app was installed (`__TAURI_BUNDLE_TYPE_VAR_UNK` → `…_MSI`). The engine staged as the first sidecar is byte-identical to `target/release/xoksa.exe`.

**AV false-positive recipe held.** The desktop `.rsrc` is **32,768 B at entropy 3.897**, with **0 PNG frames** and a complete `VS_VERSION_INFO` — unchanged from v2.9.7.

**Static — PASS.** `check.sh` all fifteen gates green — the same set as v2.9.7, its test gates included. `cargo test --lib` **438 passed / 0 failed / 1 ignored**, up from the 433 lib tests at v2.9.7: four regression tests in `llm.rs` for the defects above and one in `fundamental.rs` for the denominator band; the ignored one is the live-Stooq failover e2e. The first run of the gates is what caught the `rustls` advisory — `cargo fmt` and `cargo audit` both failed, and both were fixed before the artifacts were built.

**SOT — PASS.** Measured after the Tokyo close so the series could not move between runs: **22 of the record's 24 fields identical** across the CSV log, the JSON log and `/api/symbol/7203.T/summary` — `bar_date` **2026-09-15**, `bar_close` **3021.0**, `rsi` **44.39993302**, `macd` **2.856642277**, `final_score` **-2.0**. The two excluded are the analysis run's own clock.

**Runtime & penetration (B.5 against the frozen engine `7fff2085…`) — PASS.** `inspect.sh` **8/8**; the extended set **19/19**: `--version` and `/api/health` both report **2.9.8** (`service: xoksa-web`); six security headers present, a nonce CSP with no `unsafe-inline`, `Set-Cookie: xoksa_sid=<32 hex>; HttpOnly; SameSite=Strict`; two clients receive two distinct 32-hex `xoksa_sid`; path traversal **404 on all six payloads**; cross-origin `GET /api/health` **200**; no implicit `./xoksa.env` (a decoy in the launch directory is ignored).

**Exposed mode (`--host 0.0.0.0`) — measured on the frozen engine, over the LAN.** `--private` with a non-loopback host is refused at startup; the LAN address answers **401** without credentials and **200** (`xoksa-web`, `2.9.8`) with `Authorization: Bearer <token>`; the token already in the OS keyring was reused, so none was generated for the inspection.

**OWASP ZAP passive scan (Parrot host, `0.0.0.0` over the LAN, against the frozen engine `7fff2085…`) — 8 alerts (0 High / 2 Medium / 1 Low / 5 Info), none a defect, and identical to v2.9.7 in categories and verdicts.** The run is bound to *this* release's artifact: the alerts name `xoksa-webui-1b33333186c56cac_bg.wasm`, the frontend this engine embeds (v2.9.7's was `ce27a3bf84e5938a`). No new alert appeared. The two Mediums are the baseline SRI-missing on the same-origin WASM bundle and the missing Anti-CSRF token on `GET /login`, the one the token gate introduced in 2.9.2, where `/auth` is the one POST deliberately exempt from the origin check. Cookie poisoning on `POST /auth` is Info, as recorded for v2.9.7 and v2.9.3: the cookie is written only after a constant-time comparison against the engine's own token succeeds. *(Corrected 2026-09-25, during the v2.9.9 inspection: this paragraph previously named cookie poisoning as the second Medium; ZAP reports it as Info, as the v2.9.7 and v2.9.3 records state. The counts are unchanged.)* `/api/chat/stream` (SSE) stays out of the scan.

**SCA — 0 vulnerabilities across all five lockfiles.** `osv-scanner`: engine **0** (306 packages), `xoksa-paths` **0** (18), WASM **3** (187), `xoksa-desktop` **7** (419), `xoksa-setup` **7** (445) — the same seventeen RustSec *informational* notices as v2.9.7, none of them a vulnerability, and `rustls` no longer among the engine's findings. `cargo audit` (source) and `cargo deny` are green on both workspaces.

**SBOM — regenerated; binary-bound audit clean.** `sbom/xoksa-native-windows-x86_64.cdx.json` (**212 `library` components**) and `sbom/xoksa-webui-leptos.cdx.json` (**187**), CycloneDX 1.5 via `syft` 1.49.0, no absolute path. The only deltas from v2.9.7 are `rustls` 0.23.32 → 0.23.45, `rustls-webpki` 0.103.13 → 0.103.15, and the root component's own version. **`cargo audit bin`** over the **234** dependencies embedded in the throwaway `cargo auditable` build: **0 vulnerabilities**. The `target-sbom` tree was removed and the shipped binary never touched.

**Binary-bound items pending (recorded at ship-time, per this C.4 convention).** **VirusTotal** is excluded this cycle by the owner's decision: the artifacts are unsigned, and a brand-new hash would only reproduce the first-seen reputation penalty of [§6.2 / §6.3](./av-false-positive-case-study.md) (Windows signing = Certum Open Source Code Signing, purchased, issuance pending). **MSI install / run / uninstall** stays elevation-gated and was not exercised; the payload is hash-verified above. The **macOS** side — Developer ID signing and notarization, its own inspection, and the macOS engine SBOM (Issue #142, still at 2.9.4) — is handed to the Mac, which ships **2.9.8** rather than 2.9.7.

### v2.9.7 (Windows inspected 2026-09-13; not yet shipped) — static / SOT / runtime / penetration / ZAP / SCA / SBOM PASS; binary-bound (VirusTotal, signing, macOS) pending

v2.9.7 takes the integrity guard's reference **off the prompt**. The numbers an LLM answer is checked against are now built from `TechnicalDataGuard` and `FundamentalData` and travel alongside the request, because a prompt also carries the conversation, the user's own turns and other models' opinions — a label written by any of them was indistinguishable from one the engine wrote, so a number a user typed could become "confirmed". The same line hardens the alert surface: an EXPLAIN note may restate the rule that fired, but the comparison is now **parsed** and matched against that rule, so a wrong direction, a shifted threshold or a unit on the threshold is refused **as a condition** rather than handed back to the reading check — where `close > ¥3091` had passed on an instrument whose close is 3091. Four persistence defects are closed with it: a rule persists whichever surface created it, a write lands in its own slot instead of stacking on what is there, the 16-rule limit lives in one place, and `xoksa.env` has **exactly one writer** (shared lock, permissions kept, a variable the loader does not own left alone). The two UI shells now resolve the config file explicitly instead of through the working directory. The documentation correction matters as much: the monitor evaluates **the latest fetched bar** — on Japanese intraday timeframes the still-forming one, not a confirmed bar — and the dashboard's own description, the `AlertRule` doc comment and two manuals had said otherwise. See CHANGELOG [2.9.7].

**Build provenance — source commit `5065da8` (branch `release/v2.9.7`, branched from `main` `4a3ab94` [v2.9.6] and 16 commits ahead of it), clean working tree at build time.** Same recipe: `trunk build --release` → `cargo build --release --bin xoksa` → `scripts/bundle-engine.sh` → `xoksa-setup` staged as the second sidecar → `cargo tauri build`. `rustc 1.97.1`, LLVM 22.1.6, `tauri-cli` 2.11.4, host `x86_64-pc-windows-msvc`, unsigned. Built once and frozen; every check below was measured on these files, and the hashes were cross-verified (`sha256sum` + `CertUtil`).

**Frozen Windows artifacts:**

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| `XOKSA-2.9.7-x64.msi` (installer, carries the three binaries below) | `c265617c9aaf80c1f222323a7d0aa93c7e52fa03a641287ec61e3be07bc7abff` | 10,616,832 B |
| `xoksa.exe` (engine) | `b6593b0d5ff98f1cc6e0a0254fbe6837ceeb63c6815882981f03de6a3fd5b307` | 12,624,896 B |
| `xoksa-desktop.exe` (connection shell) | `36f9384bed05a2ade94fbb81c6af17e9c221b93cb79a58f4c51c78bd2429e4da` | 9,646,080 B |
| `xoksa-setup.exe` (settings app) | `d52de6092a6784fec89070ccfcd3516432636d05af922d14169b624529b00b76` | 8,427,520 B |

The MSI was moved from `xoksa-desktop/target/release/bundle/msi/` to `target/release/XOKSA-2.9.7-x64.msi` — the one canonical location, one artifact — and the NSIS output discarded. The MSI dated 2026-09-12 that sat there is **superseded, not merely older**: it predates `eb17cde` and must not ship.

**Payload identity — measured by extracting the MSI.** The engine and the settings app inside the installer are **byte-identical** to the build output. The desktop shell differs in **exactly 3 bytes**, at `0x773FEF`, where Tauri stamps how the app was installed (`__TAURI_BUNDLE_TYPE_VAR_UNK` → `…_MSI`); nothing else differs. The engine staged as the first sidecar is byte-identical to `target/release/xoksa.exe`.

**AV false-positive recipe held.** The desktop `.rsrc` is **32,768 B at entropy 3.897**, with **0 PNG frames** and a complete `VS_VERSION_INFO` (all six fields); the engine's is 2,048 B at 4.172. The PE carries seven sections rather than v2.6.8's six — the extra one is `_RDATA`, emitted by `rustc 1.97.1`, not a resource change. §5's controls are properties of those two numbers, and both are inside the recipe.

**Static — PASS.** `check.sh` all fifteen gates green (fmt / clippy `-D warnings` / test across the engine, `webui-leptos` [wasm32], `xoksa-desktop` and `xoksa-paths`; `cargo audit` ×2, `cargo deny` ×2, `cargo machete`). `cargo test` **448 passed / 0 failed / 1 ignored** — 433 lib, 10 integration, 5 drift guards — plus 1 in `xoksa-paths`; the ignored one is the live-Stooq failover e2e.

**SOT — PASS.** Measured on a closed market (7203.T, Tokyo shut for the weekend) so the series could not move between runs: **22 of the record's 24 fields identical** across the CSV log, the JSON log and `/api/symbol/7203.T/summary` — `final_score` **-2.0**, `rsi` **46.107725849147485**, `macd` **9.113332858421927**, `bar_close` **3031.0**, down to the bar and market-data timestamps. The two excluded are the analysis run's own clock (`analyzed_at`, `analyzed_at_timestamp`), which differ between runs by construction.

**Runtime & penetration (B.5 against the frozen engine `b6593b0d…`) — PASS.** `inspect.sh` **8/8**; `--version` and `/api/health` both report **2.9.7** (`service: xoksa-web`); six security headers present — a nonce CSP with **no** `unsafe-inline`, `Permissions-Policy`, `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, and `Set-Cookie: xoksa_sid=<32 hex>; HttpOnly; SameSite=Strict`; two clients receive two distinct 32-hex `xoksa_sid`; path traversal **404 on all six payloads**; cross-origin `GET /api/health` **200** (only mutation is gated). A headless `--dump-dom` confirms the WASM app mounts: the header renders `XOKSA` with `v2.9.7`, the panel titles and the ツール / チャット / 銘柄 / 設定 markers are present, the module script's nonce is consumed (nonce-hiding), and script and stylesheet carry SRI digests.

**2.9.7's own fixes, on the artifact.** **No implicit `./xoksa.env`** — a decoy `xoksa.env` carrying `LANG=en` in the launch directory is ignored and the engine reports the canonical `ja`. **One writer** — a language write through `POST /api/config/lang` changed **exactly the `LANG=` line**, leaving a comment, a variable the loader does not own, a five-line `ALERT_1_*` block and `STANCE` untouched, with the file's ACL identical before and after.

**Inspection hygiene — the checks cannot reach the operator's channels.** Every `serve` run was given `--env-file` pointing at a purpose-made inspection config with no `ALERT_<n>_*` and no `NOTIFY_<n>_*` entries. The canonical config on this machine carries three live rules and a notification channel, so an inspection run against it could have pushed a real notification — and an EXPLAIN LLM call — on the first rising edge the monitor saw. Nothing about the binary is weakened by this: the config file is input, and `inspect.sh` reached the same frozen engine through a wrapper that appends the flag.

**SCA — 0 vulnerabilities across all five lockfiles.** `osv-scanner`: engine **0** (306 packages), `xoksa-paths` **0** (18), WASM **3** (187), `xoksa-desktop` **7** (419), `xoksa-setup` **7** (445). All seventeen are RustSec **informational** notices — *unmaintained* or *unsound* — and **none is a vulnerability**. `glib` RUSTSEC-2024-0429 (the only one osv scores, 6.9) and `proc-macro-error` RUSTSEC-2024-0370 are **not in the Windows dependency graph at all** — measured with `cargo tree --target x86_64-pc-windows-msvc`; they belong to the Linux WebKitGTK path. The five `unic-*` notices *are* reached at runtime, through `urlpattern` → `tauri-utils`; they are upstream-owned with no fixed version published. `event-listener` 5.4.1 RUSTSEC-2026-0221 is *unsound* only (`StackSlot` implements `Send`/`Sync` unconditionally) and unreachable in the shipped WASM — a single-threaded `wasm32-unknown-unknown` build with no atomics or threads enabled anywhere in its configuration; it is fixed in 5.4.2, so it is a lockfile bump for the next version rather than a fix to this artifact. `cargo audit` (source) and `cargo deny` (advisories + licenses + bans + sources) are green on both workspaces.

**SBOM — regenerated; binary-bound audit clean.** `sbom/xoksa-native-windows-x86_64.cdx.json` (**212 `library` components**) and `sbom/xoksa-webui-leptos.cdx.json` (**187**), CycloneDX 1.5 via `syft` 1.49.0, no absolute path. Both dependency sets are **identical to v2.9.6**; the only delta is the root component's own version (`xoksa` / `xoksa-webui` 2.9.6 → 2.9.7). **`cargo audit bin`** over the **234** dependencies embedded in the throwaway `cargo auditable` build: **0 vulnerabilities**. The `target-sbom` tree was removed and the shipped binary never touched.

**Exposed mode (`--host 0.0.0.0`) — measured on the frozen engine, over the LAN.** The fail-closed path was exercised end to end while the ZAP scan ran. `--private` together with a non-loopback host is **refused at startup** ("an auth token cannot be persisted"), so a no-trace session cannot silently run unauthenticated. With the bind up, `GET /api/health` from the LAN address answers **401** without credentials and **200** (`xoksa-web`, `2.9.7`) with `Authorization: Bearer <token>`; **loopback is gated too** — under a non-loopback bind `127.0.0.1` also answers 401, so the gate does not depend on the source address. The token was already in the OS keyring from an earlier LAN session, so none was newly generated for the inspection. Reaching the port from another host additionally required removing two inbound **Block** rules Windows had created for the engine binary (the "Query User" rules a cancelled firewall prompt leaves behind) — an operator-side condition, not an application one.

**OWASP ZAP passive scan (Parrot host, `0.0.0.0` over the LAN, against the frozen engine `b6593b0d…`) — 8 alerts (0 High / 2 Medium / 1 Low / 5 Info), none a defect.** The run is bound to *this* release's artifact: the alerts name `xoksa-webui-ce27a3bf84e5938a_bg.wasm` and `…ce27a3bf84e5938a.js`, the frontend this engine embeds. Six categories are the previously-accepted set — SRI (no cross-origin assets), Unix timestamps (analysis data by design, on `/api/symbol/NVDA/summary`), session-management response (the `xoksa_sid` control itself), modern-web-application, localStorage (UI state only; keys stay server-side) and suspicious comments (in the WASM glue JS). **Two are new since the v2.6.8 baseline, and both belong to the login form the token gate introduced in 2.9.2 — not to a regression:**

- **Anti-CSRF token missing (Medium, confidence Low) on `GET /login`** — the evidence is `<form method="post" action="/auth">`. `/auth` is the one POST deliberately exempt from the server-side origin check, for two reasons recorded in the code: the route is gated by the unguessable 256-bit token itself (a cross-site POST without it gets `401`), and the strict `Referrer-Policy: no-referrer` makes a browser send `Origin: null` on that same-origin form POST, so the check would refuse a legitimate login. A login-CSRF here requires already knowing the token — which is already full access — so no privilege is gained. Every other state-changing POST keeps the check.
- **Cookie poisoning (Info) on `POST /auth` (`token`)** — the form field becomes the cookie value. It is written only after a constant-time comparison against the engine's own token succeeds, so the value stored is always the engine's own 256-bit hex; a submitted string never reaches `Set-Cookie`, and `HeaderValue::from_str` refuses control characters, so header splitting cannot occur either.

`/api/chat/stream` (SSE) stays out of the scan — a proxy buffers it into a 502 — and its controls were verified directly.

**Binary-bound items pending (recorded at ship-time, per this C.4 convention).** **VirusTotal** is excluded this cycle by the owner's decision: the artifacts are unsigned, and a brand-new hash would only reproduce the first-seen reputation penalty of [§6.2 / §6.3](./av-false-positive-case-study.md) (Windows signing = Certum Open Source Code Signing, purchased, issuance pending). **MSI install / run / uninstall** stays elevation-gated and was not exercised; the payload is hash-verified above. The **macOS** side — Developer ID signing and notarization, its own inspection, and the macOS engine SBOM (Issue #142, still at 2.9.4) — is handed to the Mac.

### v2.9.4 (macOS inspected 2026-08-21) — PASS (static, SOT, runtime, penetration, SCA, SBOM, signing/notarization, VirusTotal, ZAP)

v2.9.4 is the release where the canon was checked against the shipped binary rather than the other way round, after the owner asked why editing a manual changed the artifact's hash. It changed because the manuals were compiled in and served at `/manual/:slug` — a surface **no UI linked to and no CLI flag exposed**. That is gone, with the embedded screenshots, the markdown renderer, an analysis endpoint nothing called, and a desktop command with no caller. The audit that followed found four more gaps and closed them: `--private` was writing the user's typed chat lines to `~/.xoksa_history`; the settings app reported restarts it had not performed; `SERVE_PORT` was readable but had no field on any screen; and the UI language had **four independent sources**, so choosing Japanese in the settings app could leave the connection screen in English. `LANG` in `xoksa.env` is now the single source, written back by the dashboard's selector (loopback-only, `409` under `--private`). What the audit did **not** find matters as much: every indicator has exactly one implementation, the composite score one, and display / logs / reports / the LLM prompt / CSV / JSON all read the same analyzed guard through one text builder — the SOT core is intact.

**Drift guards added.** Each defect above was an *absence* — a missing connection between two files, invisible in a diff. `tests/no_drift.rs` asserts those connections under `cargo test` (not a shell script, so Windows runs the same checks): apply-config's fields against the settings form's, every dictionary key against something that renders it, every `/api/*` route and every `#[tauri::command]` against a caller, and the UI language against its single source. Each guard was verified by breaking the invariant it protects.

**Build provenance — source commit `dcaf7a4` (branch `release/v2.9.4`, branched from `main` `05f280e` and fast-forwarded over `release/v2.9.3`), clean working tree at build time.** Same recipe: `trunk build --release` → `cargo build --release --bin xoksa` → `scripts/bundle-engine.sh` → `xoksa-setup` staged as the second sidecar → `cargo tauri build` with `APPLE_SIGNING_IDENTITY`. `rustc 1.96.0`, host `aarch64-apple-darwin`.

**Frozen macOS artifacts** (signed, notarized, stapled):

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| `XOKSA-2.9.4-arm64.dmg` (download) | `a26b11181fd69ce9ab6a07b02f0d34b770bd1ff12cd8c48747644ec926fdc235` | 10,473,031 B |
| `XOKSA.app/Contents/MacOS/xoksa` (engine) | `870be676ece35e24de2b1d593d45ea0cc9364f325f9a2a84a600c910f95facd3` | 9,447,168 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop` (shell) | `5a44dbad4fcef6f851b87c45dce39770f920a62453cc327699942d3dcd81a96c` | 6,395,376 B |
| `XOKSA.app/Contents/MacOS/xoksa-setup` (settings app) | `bca912c2cd34c482c817d672ee98bed999cfa1fb6384cbf2523c25f93ec7b539` | 5,963,792 B |

The engine is **1.4 MB smaller than v2.9.3** — the manuals and screenshots are no longer compiled into it. Measured on the frozen binary: the four manual strings present in the shipped 2.9.3 return **zero** matches.

**Static — PASS.** `check.sh` all sixteen gates green across engine, `webui-leptos` (wasm32), `xoksa-desktop` and `xoksa-paths`; `cargo test` **299 passed / 0 failed / 1 ignored** — 284 lib, 10 integration, **5 drift guards** (`tests/no_drift.rs`), plus 10 in `xoksa-paths`; the ignored one is the live-Stooq failover e2e.

**SOT — PASS.** Measured on a closed market (7203.T, Tokyo shut) so the series could not move between runs: `final_score` **7.0**, RSI **67.78**, MACD **37.0873** and the confirmed-bar close **3132.0** identical across the CSV log, the JSON log, and `/api/symbol/7203/summary`.

**Runtime & penetration (B.5 against the frozen signed engine `870be676…`) — PASS.** `inspect.sh` 8/8; `--version` and `/api/health` both 2.9.4; six security headers present; two clients receive two distinct 32-hex `xoksa_sid`; path traversal 404 on all six payloads. **Removed surfaces confirmed gone on the artifact**: `/manual/setup`, `/manual/analysis-guide`, `/manual.css`, `/images/*` and `POST /api/analysis/context-pack` all **404**. **Language SOT confirmed on the artifact**: `/api/config` and the page's `<meta name="xoksa-lang">` agree, an unknown value is **400**, and a `--private` session answers **409** with the config file untouched.

**Payload identity.** The bundle-type marker stays `__TAURI_BUNDLE_TYPE_VAR_UNK`; with signatures stripped `xoksa` and `xoksa-setup` are **byte-identical** to their build output and `xoksa-desktop` differs by **one byte** in `__LINKEDIT` `vmsize`.

**Signing / notarization — PASS.** `Developer ID Application: Kohzoh Tsuchiya (463YK5WF6H)`, hardened runtime on the bundle and all three inner binaries; `notarytool` Accepted for app and DMG, both stapled; Gatekeeper **accepted, source=Notarized Developer ID** for both.

**SCA — unchanged from v2.9.3.** `osv-scanner`: engine **0** (306 packages), `xoksa-paths` **0** (18), WASM **3** (`event-listener` 5.4.1 RUSTSEC-2026-0221 unsound-only and unreachable in a single-threaded wasm32 build, plus the unmaintained build-time proc-macros `paste` and `proc-macro-error2`), `xoksa-desktop` **17** and `xoksa-setup` **17** (the gtk-rs 0.18 Linux bindings, not compiled into the macOS WKWebView binaries, plus unmaintained build-time deps).

**SBOM — regenerated.** `sbom/xoksa-native-macos-arm64.cdx.json`: CycloneDX 1.5, **209 `library` components** (212 in v2.9.3; `pulldown-cmark` and its two dependencies left with the in-binary manuals), no absolute path. **`cargo audit bin`** over the **225** embedded dependencies: **0 vulnerabilities**. The `--target-dir target-sbom` throwaway was removed; the shipped binary was untouched.

**VirusTotal (on the shipped hashes, 2026-08-21) — all four clean.** DMG `a26b1118…` **0/60**; engine `870be676…` **0/62**; desktop `5a44dbad…` **0/61**; settings app `bca912c2…` **0/63**.

**OWASP ZAP passive scan (separate host, over the LAN against the frozen artifact) — 8 alerts: 0 High / 1 Medium / 1 Low / 6 Info.** All are the baseline documented for v2.9.3, re-measured here against this build:

- **Anti-CSRF token missing on `/login` (Medium) — accepted, measured again.** A cross-site POST to `/auth` with a wrong token returns **401**; only a caller who already holds the 256-bit token gets a session (**303**), and such a caller can reach the engine directly. The session cookie is `HttpOnly` + `SameSite=Strict`.
- **Cookie poisoning on `POST /auth` (Info) — by design.** The alert fires because the `token` parameter influences `Set-Cookie`; that is the documented exchange (present the token, receive an `HttpOnly` / `SameSite=Strict` cookie), confirmed on the artifact.
- SRI missing on the WASM bundle (Low) — same-origin, hash-named assets under `script-src 'self'`; no CDN. Unix-timestamp disclosure ×4 — market-data timestamps by design. Session-management response, Modern-Web-App, suspicious comment (`wasm-bindgen`'s own glue, not project code).
- **localStorage disclosure — now five keys, not six.** `xoksa.theme` / `xoksa.font` / `xoksa.symbol` / `xoksa.tickers` / `xoksa.targets`, all Class C. The language left this list in this release; it is a setting, and settings live in `xoksa.env`.

**Windows** — the MSI for 2.9.4 is rebuilt and inspected on the Windows side; that record is theirs.

### v2.9.3 (macOS inspected 2026-08-21) — PASS (static, SOT, runtime, penetration, SCA, SBOM, signing/notarization, VirusTotal, ZAP)

v2.9.3 fixes what launching the v2.9.2 build exposed: the desktop's local auto-start now asks the OS for an **ephemeral port** (bind `:0`, read, release) instead of contending for a fixed one, requires `/api/health` to answer as `xoksa-web` with **exactly this app's version** before the WebView attaches, and reports every auto-start failure **with its cause** — spawn failure, immediate child exit, port occupancy and by what, HTTP 401 distinguished from unreachable — instead of a bare timeout. When LAN access is on the fixed port is kept (remote devices rely on it) and probed before spawning. The rule is now a design contract in [security-design.md §6](./security-design.md): *local auto-start never adopts a foreign engine*. The settings app's LAN warning and the connection screen's token guidance were corrected to the fail-closed behaviour shipped in v2.8.0.

**Both fixes verified on macOS against the frozen signed build** — the platform where the original failure was found:

| Condition | Result |
| :--- | :--- |
| LAN on, a stale `xoksa serve` holding `127.0.0.1:8787` | Refused **immediately**, naming the occupant: 「ポート 8787 は別の xoksa エンジン（v2.9.3）が使用中です。」 (v2.9.2 gave a bare 10-second timeout) |
| LAN off, same port still held | Child engine started on ephemeral **50569**, the squatter on 8787 **untouched** (`/api/health` still 200), dashboard opened |

**Build provenance — source commit `3762eed` (branch `release/v2.9.3`, branched from `main` `05f280e`), clean working tree at build time.** Same recipe as v2.9.2: `trunk build --release` → `cargo build --release --bin xoksa` → `scripts/bundle-engine.sh` → `xoksa-setup` staged as the second sidecar → `cargo tauri build` with `APPLE_SIGNING_IDENTITY`. `rustc 1.96.0`, host `aarch64-apple-darwin`.

**Frozen macOS artifacts** (signed, notarized, stapled):

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| `XOKSA-2.9.3-arm64.dmg` (download) | `206653bdb93a8c7b5c2feb1937bef063144b8944a7e5407c0e6427a4be2397f8` | 11,463,321 B |
| `XOKSA.app/Contents/MacOS/xoksa` (engine) | `a90ee619e5ed1a33c0ec9c44bc8e021a938781cf462ceee0754834e22d25c7e9` | 10,865,808 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop` | `ea3d71162976623cc45cdc4e59c5b7bf33a45213c779fadf5428789c3bbd0728` | 6,411,776 B |
| `XOKSA.app/Contents/MacOS/xoksa-setup` (settings app) | `a913c133a3a15df302c957519327d4a82372bb545a63d7be5a06a39ddb826548` | 5,946,992 B |

**Static — PASS.** `check.sh` all fifteen gates green across engine, `webui-leptos` (wasm32), `xoksa-desktop` and `xoksa-paths`; `cargo test` **294 passed / 0 failed / 1 ignored** (283 lib + 10 integration + 1 `xoksa-paths`; the ignored one is the live-Stooq failover e2e).

**SOT — PASS.** `latest_observed_price` 311.3 and `final_score` −4.0 identical across csv / json / `/api/symbol/AAPL/summary`.

**Runtime & penetration (B.5 against the frozen signed engine `a90ee619…`, `serve --private`) — PASS.** `inspect.sh` 8/8; `--version` and `/api/health` both 2.9.3; six security headers; K1 two distinct 32-hex `xoksa_sid`; path traversal 404 on all six payloads.

**Payload identity.** The bundle-type marker stays `__TAURI_BUNDLE_TYPE_VAR_UNK`; with signatures stripped each of the three binaries differs from its build output by **one byte** in `__LINKEDIT` `vmsize`.

**Signing / notarization — PASS.** `Developer ID Application: Kohzoh Tsuchiya (463YK5WF6H)`, hardened runtime on the bundle and all three inner binaries; `notarytool` Accepted for app and DMG, both stapled; Gatekeeper **accepted, source=Notarized Developer ID** for both.

**SCA — 0 exploitable in the shipped artifacts, unchanged from v2.9.2.** `osv-scanner`: engine **0** (309 packages), `xoksa-paths` **0** (18), WASM 3, `xoksa-desktop` 17, `xoksa-setup` 17. The WASM three are `paste` / `proc-macro-error2` (unmaintained build-time proc-macros) and `event-listener` 5.4.1 (RUSTSEC-2026-0221, *unsound*, not reachable in a single-threaded `wasm32-unknown-unknown` build; patched in 5.4.2 — a lockfile bump for a later version). The desktop / settings-app 17 are the gtk-rs 0.18 Linux bindings (not compiled into the macOS WKWebView binaries) plus unmaintained build-time deps.

**SBOM — regenerated, dependency set unchanged.** `sbom/xoksa-native-macos-arm64.cdx.json`: CycloneDX 1.5, **212 `library` components**, no absolute path — identical in content to the v2.9.2 inventory apart from the `serialNumber`, since v2.9.3 touches only desktop/settings code. **`cargo audit bin`** over the **228** embedded dependencies: **0 vulnerabilities**. The `--target-dir target-sbom` throwaway was removed; the shipped binary was untouched.

**VirusTotal (on the shipped hashes, 2026-08-21) — all four clean.** DMG `206653bd…` **0/59**; engine `a90ee619…` **0/62**; desktop `ea3d7116…` **0/63**; settings app `a913c133…` **0/63**.

**OWASP ZAP passive scan (separate host, `192.168.0.3:8787` over the LAN with the access token in play) — 8 alerts: 0 High / 2 Medium / 1 Low / 5 Info.** Six are the documented baseline (SRI-missing on the WASM bundle — same-origin assets only, no CDN; Unix-timestamp disclosure ×4 on `/api/symbol/…/summary` — market-data timestamps by design; session-management response, Modern-Web-App, localStorage ×6 — UI state only, keys server-side; suspicious comments). **Two are new, and both come from the v2.8.0 login flow this release exercises for the first time under ZAP:**

- **Anti-CSRF token missing on `/login` (Medium) — accepted, measured.** The login form posts to `/auth`, which is deliberately exempt from the origin check (`reject_cross_origin_mutations`) because a strict `Referrer-Policy: no-referrer` makes browsers send `Origin: null` on that same-origin form POST, which the check would otherwise reject. The exemption is safe because the endpoint is gated by the 256-bit token itself, and that was verified against the frozen build: a **cross-site POST to `/auth` with a wrong token returns 401** and sets only the analysis `xoksa_sid` cookie — no authenticated session is created. An attacker who does not hold the token achieves nothing; one who does can reach the engine directly, so routing it through a victim's browser gains nothing. Classic login-CSRF harm (logging the victim into the *attacker's* account) has no meaning here: there is one engine and one user, and the session cookie is `HttpOnly` + `SameSite=Strict`.
- **Cookie poisoning on `POST /auth` (Info) — by design.** The alert fires because a request parameter (`token`) influences a `Set-Cookie`; that is exactly the documented exchange — present the access token once, receive an `HttpOnly` / `SameSite=Strict` session cookie.

Also measured while confirming the above: **`--private` refuses a non-loopback bind outright** (“an auth token cannot be persisted”), so a no-trace session can never open an unauthenticated LAN surface.

**Windows** — `XOKSA-2.9.3-x64.msi` was rebuilt on the Windows side with its payload-hash check passing; its own inspection record is the Windows side's.

### v2.9.2 (macOS inspected 2026-08-21) — measured items PASS; **inspection suspended, artifacts superseded** (connection-screen defects found on launch)

The v2.9.2 line splits the Tauri surface in two: the **desktop app** becomes a connection shell (local engine it auto-starts, or a remote one by address + access token) and all configuration moves to a separate **settings app (`xoksa-setup`)**, password-gated. A shared crate `xoksa-paths` holds the one canonical config path. The shipped `.app` therefore carries **three** signed binaries — engine, desktop shell, settings app — where v2.6.8 carried two.

**Build provenance — source commit `6368308` (branch `release/v2.9.2`), clean working tree at build time.** `trunk build --release` → `cargo build --release --bin xoksa` → `scripts/bundle-engine.sh` → `cargo build --release` in `xoksa-setup` staged as the second sidecar → `cargo tauri build` with `APPLE_SIGNING_IDENTITY`. `rustc 1.96.0`, host `aarch64-apple-darwin`.

**Frozen macOS artifacts** (signed, notarized, stapled):

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| `XOKSA-2.9.2-arm64.dmg` (download) | `b87909ce12e0dee1b66be127d1f696ef94013fbeadf75de8c903945e5cf326e7` | 11,463,587 B |
| `XOKSA.app/Contents/MacOS/xoksa` (engine) | `98493aaa5f4d987faad78c0f48d5e602fff46ba387d3e3233bd87682f4bdac3c` | 10,884,016 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop` | `731fde2befa7dc81d124e06e678e70b068a4b0932f4ad07442aea4d24a5f55fa` | 6,411,760 B |
| `XOKSA.app/Contents/MacOS/xoksa-setup` (settings app) | `fa9ad8323d4c749a40ca3291874dfde663bbd3839229275e2ddbfbc10601278a` | 5,946,992 B |

**Static — PASS.** `check.sh` all fifteen gates green: `cargo fmt --check` and `clippy -D warnings` across engine, `webui-leptos` (wasm32), `xoksa-desktop` and `xoksa-paths`; `cargo test` **294 passed / 0 failed / 1 ignored** (283 lib + 10 integration + 1 `xoksa-paths`; the ignored one is the live-Stooq failover e2e, blocked by external anti-bot); `cargo audit` ×2, `cargo deny` ×2, `cargo machete` clean.

**SOT — PASS.** `latest_observed_price` 311.3 and `final_score` −4.0 identical across `--log-format csv`, `--log-format json` and `/api/symbol/AAPL/summary`.

**Runtime & penetration (B.5 against the frozen signed engine `98493aaa…`, `serve --private`) — PASS.** `inspect.sh` 8/8 (embedded UI 200; CSP present; cross-origin POST 403; >64 KiB body 413; `text/plain` 415; same-origin POST 200; CLI analysis non-empty exit 0; backtest API valid JSON). Additionally measured: `--version` and `/api/health` both report 2.9.2; six security headers present; K1 two distinct 32-hex `xoksa_sid` values; path traversal 404 on all six payloads.

**Payload identity — the shipped binaries differ from the built ones by the code signature alone.** The Tauri bundler does not patch the bundle-type marker on macOS (`__TAURI_BUNDLE_TYPE_VAR_UNK` in both), and with signatures stripped each of the three binaries differs from its build output by **one byte**, in the `__LINKEDIT` `vmsize` field — a value determined by the signature size.

**Signing / notarization — PASS.** `Developer ID Application: Kohzoh Tsuchiya (463YK5WF6H)`, hardened runtime on the bundle and on all three inner binaries. `notarytool` Accepted for the app and the DMG; both stapled (`stapler validate` ok). Gatekeeper: `spctl -a` returns **accepted, source=Notarized Developer ID** for both the `.app` and the DMG.

**SCA — 0 exploitable in the shipped artifacts.** `osv-scanner` over all five lockfiles: engine **0** (309 packages), `xoksa-paths` **0** (18), WASM frontend 3, `xoksa-desktop` 17, `xoksa-setup` 17. The WASM three are `paste` and `proc-macro-error2` (unmaintained, build-time proc-macros) plus **`event-listener` 5.4.1, RUSTSEC-2026-0221** — an *unsound* advisory (`StackSlot` implements `Send`/`Sync` unconditionally), **not reachable in the shipped WASM**, which is a single-threaded `wasm32-unknown-unknown` build with no thread boundary to cross; patched in 5.4.2, so it is a lockfile bump for the next version rather than a fix to this artifact. The desktop and settings-app counts are the same set as previous releases: 11 gtk-rs 0.18 bindings (the Linux WebKitGTK backend, **not compiled into the macOS WKWebView binaries**, including the single Medium `glib` RUSTSEC-2024-0429), 5 `unic-*` and 1 `proc-macro-error` (unmaintained build-time / Unicode deps).

**SBOM — regenerated on this host.** `sbom/xoksa-native-macos-arm64.cdx.json`: **CycloneDX 1.5, 212 `library` components** (195 at v2.6.8 — the v2.9.2 dependency set), no absolute path in the inventory. **Binary-bound `cargo audit bin`** over the **228** embedded dependencies: **0 vulnerabilities**. The throwaway `--target-dir target-sbom` tree was removed and the shipped `target.nosync/release` binary left untouched.

**VirusTotal (on the shipped hashes, 2026-08-21) — all four clean.** DMG `b87909ce…` **0/60**; engine `98493aaa…` **0/63**; desktop `731fde2b…` **0/62**; settings app `fa9ad832…` **0/63**. The newly added `xoksa-setup` is clean on its first scan, consistent with the macOS pattern: Developer ID signing plus notarization from the start, rather than the icon-encoding work the Windows desktop needed (§C.4 v2.6.7).

**Inspection suspended — these artifacts are superseded.** Launching the signed build surfaced defects in the new connection screen (below), which require code changes; the binaries above will therefore be rebuilt and re-inspected, and this record stands as the measurement of a build that was **not shipped**. What was found:

1. **Local auto-start fails with a message that does not state the cause.** With another engine already holding the port (measured: a running instance on `127.0.0.1:8787`), the spawned child cannot bind and never becomes ready, and the only feedback is `the local engine did not become ready in time` after a 10-second wait. Three contributors: `spawn_local_engine` treats a successful `spawn()` as success and never checks whether the child died; `engine_reachable` returns a bool, so **HTTP 401 is indistinguishable from unreachable**; and there is no pre-flight check that the port is free.
2. **The v2.6.4 hardening is gone from the auto-start path.** That release moved the desktop to an **ephemeral port** plus an `/api/health` identity check precisely so a stale engine could not be mistaken for the app's own; v2.9.2's connection screen uses a fixed, user-visible `host:port` (default `8787`) with no such check. The explicit address is a deliberate consequence of supporting remote engines — the loss of the stale-engine protection on the *local* path is not, and it is what this failure exercised.
3. **Two places in this document still describe the ephemeral port as current behaviour** (§C.4 v2.6.4 line and the v2.6.8 macOS launch note). They now contradict the implementation.
4. **The settings app's LAN warning is a version behind the engine.** It tells the user that a non-loopback bind has *no login* and that anyone who can reach it may use the configured LLM and read their data — but v2.8.0 made a non-loopback `serve` **fail-closed**, auto-generating a 256-bit `SERVE_AUTH_TOKEN` and rejecting unauthenticated requests (measured: the occupying engine answered `/api/health` with `authentication required`). The warning errs toward alarm rather than false safety, but it contradicts §4 and would push users away from a feature that is in fact authenticated. Only the Japanese string was found; the English counterpart was not located, so the pair may also be out of language sync.

**Not measured.** **OWASP ZAP passive scan** — not run for this build. **Windows** — its own build and inspection are the Windows side's, tracked on PR #137.

### v2.6.8 (macOS inspected 2026-07-27, Windows re-inspected 2026-07-27) — PASS on both OSes
v2.6.8 exists because the v2.6.7 shipping inspection's own follow-up — a line-by-line audit of the security and design documents against the source — found a **real defect, not a documentation error**: adding an alert rule from the dashboard appended `ALERT_<n>_*` to `xoksa.env` **unconditionally**, so a `--private` session wrote the user's watched ticker and threshold to disk. `--private` is documented as a no-trace session in four places (§4 here, design-philosophy §8.2, the README, the usage guide), and the log and saved-strategies paths honour it; the alert path did not. Fixed by returning early under `is_private()` in `persist_add_to_env` / `persist_remove_from_env` (the rule still runs for the session), with a regression test asserting both directions. The channel secret was never affected — it lives in the OS keyring. The same release adds `#![forbid(unsafe_code)]` to the desktop crate, which had no `unsafe` but was not compiler-enforced, and corrects four documentation discrepancies found in the audit (the `discordapp.com` entry missing from the notification allowlist; the `test-notify` child process missing from the subcommand enumerations; design-philosophy §8.2's "only disk write" claim; and four `#[cfg]` seams missing from §8.3's "complete set").

**Build provenance.** Built on branch `xoksa-dev-v2.6.8`, based on `origin/main` `1879aae`, so the tree carries the `serde_json` 1.0.151 bump (dependabot #104) that landed while the v2.6.8 work was in progress. An earlier v2.6.8 build made before that rebase was **discarded and rebuilt**: its WASM frontend used `serde_json` 1.0.150, so it could not have been reproduced from the merged source — the hashes below are the rebuilt, reproducible set. The discarded build's hashes are deliberately not recorded here; they were never the shipped artifact.

**Frozen macOS artifacts** (signed, notarized, stapled):

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| `XOKSA-2.6.8-arm64.dmg` (download) | `c7a225ae246f159633fb836683d1b0d7bf4cab6eee4c47dfb916f5691e4dc25d` | 8,425,005 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop` | `ea6e8f5b69e45a9895c972846948e9b4706f6d62d3bd6af13e784dafff4d40be` | 6,510,688 B |
| `XOKSA.app/Contents/MacOS/xoksa` (engine) | `6efc1559ff57be2e9cac9b90757ad7ea0cd7e9db5f605fd65b916ed5d09b2336` | 10,387,312 B |
| `xoksa-macos-arm64.zip` (CLI, same engine byte-for-byte) | `1c1c979ab28bfa988ff4f5418dfc2553cce1d0c538ba36b8461cec11cd53dc99` | — |

**All items re-measured on the new artifacts — PASS.** **Static** — `check.sh` all twelve gates green; `cargo test` **266 passed / 0 failed / 1 ignored** (256 lib + 10 integration; the new lib test is the `--private` alert regression). **Payload identity** — reproduces the v2.6.7 finding: the bundle-type marker stays `__TAURI_BUNDLE_TYPE_VAR_UNK`, and with signatures stripped the built and shipped binaries differ by 1–2 bytes in `__LINKEDIT` `vmsize` alone. **Signing / notarization** — `notarytool` Accepted for the app (`73794b25…`), the DMG (`1c6fa7af…`) and the CLI zip (`1ce67222…`); app and DMG stapled; Gatekeeper `accepted, source=Notarized Developer ID`. **SOT** — `latest_observed_price` 333.02 and `final_score` 1.0 identical across csv / json / `/api/symbol/AAPL/summary`. **Runtime & penetration** — `inspect.sh` 8/8 against the frozen signed engine; `/api/health` reports 2.6.8; six security headers present; K1 two distinct 32-hex sids; path traversal 404 on all six payloads. **SCA** — unchanged from v2.6.7 (native 0; WASM 2 unmaintained-only; desktop 17, the single Medium being the Linux-only `glib` binding). **SBOM** — `sbom/xoksa-native-macos-arm64.cdx.json` regenerated on this host (195 components, no absolute path); **binary-bound `cargo audit bin`** over the 210 embedded dependencies: 0 vulnerabilities. **VirusTotal (shipped hashes)** — DMG **0/61**, desktop **0/62**, engine **0/60**, all clean.

**Frozen Windows artifacts** (`cargo build --release`, default `embedded-ui`; MSI via `build-msi.ps1`; `rustc 1.96.0`, host `x86_64-pc-windows-msvc`, LLVM 22.1.2):

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| `xoksa.exe` (Windows x86_64, engine) | `fad0a1acf654df411578073799bb11499c9fa9e33f1078eb25565375082883f1` | 13,065,728 B |
| `xoksa-desktop.exe` (Windows x86_64) | `3abe6aefbad79a1754ee1cb11b39c25b4fb901b19328fb36315c07256d4a728a` | 9,904,128 B |
| `XOKSA-2.6.8-x64.msi` (installer, carries both binaries above) | `fba768d2248c84acf50138a199ec1dde708ddca7aceff10bac5771ec394bfdfe` | 8,327,168 B |

**The v2.6.7 Windows artifacts are superseded, not merely older** — they carry the `--private` alert defect fixed here, so `dist/v2.6.7/` is marked DO-NOT-SHIP and the v2.6.7 release stays deleted (tag kept).

**All items re-measured on the new Windows artifacts — PASS.** **`--private` fix, on-artifact** — `persist_add_to_env` returns early under `is_private()`; the regression test (`private path is a no-op`) is in the 266-test run below, and the running `serve --private` engine was exercised for the surrounding B.5 gates. **Recipe preserved** — the desktop `.rsrc` is again **32,768 B @ entropy 3.982** (6 sections), the bundle marker is `__TAURI_BUNDLE_TYPE_VAR_UNK` (MSI-patch avoided), the icon has 0 PNG frames, and the four version-metadata fields are present; `build-msi.ps1`'s payload hash-check passed, so the MSI carries exactly `3abe6aef…` and `fad0a1ac…`. **Static** — `check.sh` all gates green (`cargo-deny` 0.20.2); `cargo test` **266 passed / 0 failed / 1 ignored**. **SOT** — `latest_observed_price` 333.02 and `final_score` 12.0 identical across csv / json / `/api/symbol/AAPL/summary`. **Runtime & penetration (B.5 against `serve --private`)** — `/api/health` reports 2.6.8; nonce-CSP with no `unsafe-inline`; `xoksa_sid` HttpOnly/SameSite=Strict; K1 two distinct 32-hex sids; path traversal 404 on all six payloads; Content-Type / cross-origin-POST / cross-origin-GET gates 415 / 403 / 200; 5 MB body 413; `/api/backtest` 200 valid JSON; H1/H2 guard tests pass. **Exposed mode** — the same controls re-measured over the LAN IP against `--host 0.0.0.0` hold identically. **SCA** — `osv-scanner` ×3 reproduces the recorded counts (native 0; WASM 2 unmaintained-only; desktop 17, single Medium = Linux-only `glib`); `cargo audit` (source) 0; **binary-bound `cargo audit bin`** over the 220 embedded dependencies: 0 vulnerabilities. **SBOM** — `sbom/xoksa-native-windows-x86_64.cdx.json` regenerated on this host (199 components, no absolute path); its dependency set is **identical to v2.6.7** — the only delta is the root component's own version (`xoksa` 2.6.7 → 2.6.8). **OWASP ZAP passive (Parrot host, LAN)** — 6 alerts (0 High / 1 Medium SRI-missing / 1 Low Unix-timestamp / 4 Info), **identical to the baseline in count and category**, no new alert. **VirusTotal (shipped hashes)** — MSI **0/62**, engine **0/69**, desktop **0/70** — all clean, *with a caveat recorded because it matters for re-inspectors*: the desktop binary scored **1/70 on first upload** (Microsoft `Trojan:Win32/Wacatac.C!ml`) and cleared to **0/70 on reanalysis with no change to the file**. Two controls placed this on the reputation axis, not the file: the v2.6.7 desktop (`5d267218…`), rescanned the same day, was still 0/70 (no model drift against the recipe), and reanalysing the unchanged v2.6.8 binary withdrew the verdict. It is the first-seen penalty on a brand-new unsigned hash; the mechanism and its shipping consequence are written up in [av-false-positive-case-study.md §6.2](./av-false-positive-case-study.md). **A fresh scan of a new build may show it again transiently until reputation accrues.** MSI install/run/uninstall was not re-exercised for v2.6.8 (elevation-gated); the tooling (`build-msi.ps1`, WiX) is unchanged from the v2.6.7 run measured above, and the payload is hash-verified and scans clean.

### v2.6.7 (Windows inspected 2026-07-23, re-inspected 2026-07-24; macOS inspected 2026-07-27) — PASS (source, dynamic, SBOM, ZAP; VirusTotal clean on every shipped artifact: Windows engine **0/69**, desktop **0/70**, MSI **0/62**; macOS DMG **0/61**, desktop **0/62**, engine **0/62**)
v2.6.7 removes the turso-backed embedded SQL database that `keyring` 4.0's default features linked in (No-Database property restored; `Cargo.lock` ~−3000 lines, native SBOM 337 → 200 components, binary 28.7 MB → 13.1 MB), fixes a notification-setup prompt that could write a pasted webhook secret to `xoksa.env` in plaintext, fixes the Windows desktop chart/HELP popups opening blank (a synchronous `#[tauri::command]` deadlocked WebView2 controller init — now `async`, unifying Windows/macOS to one path), decouples `--debug-prompt` (writes the prompt file) from `--no-llm` (skips the LLM call) into independent options, and suppresses the Windows console-window flash on engine spawn.

**Frozen local artifacts** (`cargo build --release`, default `embedded-ui`):

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| `xoksa.exe` (Windows x86_64, engine) | `a7d59038b93050fb49d6cd879bca69ab4e655d36362721caa6d719e806da7ca1` | 13,058,048 B |
| `xoksa-desktop.exe` (Windows x86_64) | `5d2672185c1d3461cb6cad5c297a3ccadf6ac6c96045a96a85843bb594e92dee` | 9,904,128 B |
| `XOKSA-2.6.7-x64.msi` (Windows installer, carries both binaries above) | `a7f978382e725b6e1f285c81c0bd9366d7b7dcd0cb5fce5ae4d26cbbd491fa73` | 8,327,168 B |

**Static — PASS.** `cargo fmt --check` (no diff); `cargo clippy -- -D warnings` = 0 across all three workspaces; `cargo test` = 266 pass (256 lib + 10 integration), 0 failed; `cargo audit` = 0 vulnerabilities over 288 native deps; `cargo deny check` = advisories/bans/licenses/sources ok; `cargo machete` = no unused deps.

**SOT — PASS.** csv == json == api output equivalence (AAPL: `latest_observed_price` 325.89 and `final_score` 13.0 identical across `--stdout-log --log-format csv`, `--log-format json`, and `/api/symbol/AAPL/summary`).

**Runtime & penetration (B.5 playbook against `xoksa serve --private`) — PASS.** `--version` 2.6.7; `/api/health` 200; strict per-request-nonce CSP with no `unsafe-inline` (`object-src`/`base-uri`/`frame-ancestors 'none'`); `xoksa_sid` HttpOnly/SameSite=Strict; K1 cross-client `sid` isolation (two distinct 32-hex ids); path traversal all 404 (6 payloads); CSRF / Content-Type / oversize gates 403 / 415 / 413; CLI analysis non-empty exit 0; `/api/backtest` valid JSON; H1 prompt-injection (`news_title_is_single_line_no_forged_markers`) and H2 all-provider output-integrity guard (`output_guard_applies_to_every_provider_and_respects_flag`) tests pass. K3 capacity-shed is the admission semaphore (`MAX_CONCURRENT_CONNECTIONS = u16::MAX`), architecturally present.

**SCA (of record: `osv-scanner` on all three `Cargo.lock`, plus `cargo audit` / `cargo deny`) — 0 exploitable in the shipped artifacts.** Native engine: **0**. WASM frontend: 2 *unmaintained-only* advisories (`paste`, `proc-macro-error2` = build-time proc-macros, not in the shipped WASM). Desktop (Windows): 17 advisories — the 11 gtk-rs 0.18 bindings (`atk` / `gdk` / `glib` / `gtk` …) are the **Linux WebKitGTK backend, not compiled into the Windows WebView2 binary**; 5 `unic-*` and 1 `proc-macro-error` are unmaintained build-time / Unicode deps. The single Medium (`glib`, RUSTSEC-2024-0429) is one of those Linux-only bindings, absent from the Windows artifact.

**SBOM — regenerated (CycloneDX via `cargo auditable build` → `syft file:`, path-clean).** `sbom/xoksa-native-windows-x86_64.cdx.json` (renamed from `xoksa-native.cdx.json` when the macOS inventory joined it) lists **199** in-binary components (down from v2.2.3's 337 — the turso removal); `sbom/xoksa-webui-leptos.cdx.json` is unchanged in dependency set (a source-only change).

**Exposed mode (`--host 0.0.0.0`) — measured, not assumed.** The B.5 controls re-run over the LAN IP hold identically: strict per-request-nonce CSP + `xoksa_sid`, path traversal 404, and the CSRF gate matching the request's own `Host` (same-origin LAN `text/plain` POST → 415, GET → 200, cross-origin POST → 403). The only difference is the documented no-auth (operator's responsibility — reverse-proxy auth/TLS/rate-limit for any exposed deployment).

**OWASP ZAP passive scan (Parrot host, `0.0.0.0` over the LAN) — 6 alerts (4 Info, 1 Low, 1 Medium), all previously-accepted / not-applicable** to this single-user, same-origin, no-CDN, no-DB architecture: SRI-missing (Medium — same-origin assets only, no cross-origin CDN); Unix timestamp disclosure (Low — market-data timestamps by design); session-cookie identified, Modern-Web-App, localStorage (UI state only, keys server-side), suspicious-comments (all Info). No new alert versus the documented baseline.

**Re-inspection (2026-07-24) — the desktop artifact changed; every item re-measured, all PASS.** The AV false-positive fix re-encoded the desktop icon, so the desktop artifact is new (`5d267218…`), while the **engine artifact is bit-identical to the one inspected on 07-23** (`a7d59038…`, hash verified before and after), leaving engine-bound results bound to an inspected hash. All checks ran against those exact files on the dev profile, with the release artifacts left untouched (hashes re-verified afterwards). **Static** — `cargo fmt --check` clean on all three workspaces; `clippy -D warnings` clean on all three; `cargo test` **265 passed / 0 failed / 1 ignored** (255 lib + 10 integration; the ignored one is the real-Stooq failover e2e, blocked by external anti-bot, not a defect). **SCA** — `cargo audit` exit 0 over 288 deps against 1169 advisories; `cargo deny check` advisories/bans/licenses/sources ok; `cargo machete` no unused deps; `osv-scanner` over all three lockfiles reproduces the counts above exactly (native 0; WASM 2 unmaintained-only; desktop 17 with the single Medium being the Linux-only `glib` binding). **SBOM** — all three `Cargo.lock` unchanged, so the published SBOM stays bound to the shipped dependency set. **SOT** — `latest_observed_price` 321.66 and `final_score` 10.0 identical across `--log-format csv`, `--log-format json` and `/api/symbol/AAPL/summary`. **Runtime & penetration (B.5)** — `--version` 2.6.7; `/api/health` 200; per-request-nonce CSP with no `unsafe-inline`; `xoksa_sid` HttpOnly/SameSite=Strict; K1 two distinct 32-hex sids; path traversal 404 on all six payloads; 415 / 403 / 200 on the Content-Type, cross-origin POST and cross-origin GET gates; 413 on a 5 MB body; `/api/backtest` POST 200 with valid JSON; CLI analysis exit 0; H1 and H2 guard tests pass. **Exposed mode** — the same controls re-measured over the LAN IP against `--host 0.0.0.0` hold identically. **OWASP ZAP passive (Parrot host, LAN)** — **6 alerts, 0 High / 1 Medium / 1 Low / 4 Info: identical to the 07-23 baseline in both count and category**, no new alert, confirming the icon change touches nothing in the web layer. Housekeeping noted, not a defect: `deny.toml` still ignores `RUSTSEC-2026-0173` (`proc-macro-error2`), a crate no longer in the tree ("no crate matched advisory criteria").

**MSI distribution (added 2026-07-24) — installer and both payloads scan clean; install / run / uninstall measured.** The two-binary layout is a real onboarding hazard (the desktop resolves the engine as a *sibling* file), so v2.6.7 adds a Windows installer. It is **not** produced by `cargo tauri build`: Tauri's bundler patches a 3-byte marker into the desktop binary (`BUNDLE_TYPE_VAR_UNK` → `…_MSI`), and those three ASCII characters alone flip Microsoft to `Trojan:Win32/Wacatac.B!ml` — measured against the otherwise byte-identical clean build (case study §4.3). The shipped MSI is therefore built by [`xoksa-desktop/build-msi.ps1`](../../xoksa-desktop/build-msi.ps1): build with `cargo build --release`, let Tauri generate the WiX sources, re-run WiX `light` with the **unpatched** binary in place, then **fail the build unless the packaged payload hash-matches what was built**. Result — MSI `a7f97838…` **0/62 clean**, carrying `5d267218…` (0/70) and `a7d59038…` (0/69) **byte-identical to the inspected artifacts**, i.e. what users install is exactly what was inspected. Measured install: per-machine to `C:\Program Files\XOKSA\` with both binaries in one directory (the sibling-resolution requirement), Start-Menu and uninstall shortcuts, uninstall registry entry (`XOKSA` / 2.6.7 / `kozo2000`); the installed app spawns the engine, `/api/health` returns 2.6.7 and the loopback UI serves the same nonce-CSP and `xoksa_sid` cookie as the inspected binary. Uninstall removes every installed file, both shortcuts and its own registry entry, and leaves no running process. Two things it does **not** remove, stated because "no leftovers" would otherwise overstate it: `%APPDATA%\app.xoksa.desktop` (config + logs) is kept intentionally, and the **Windows Firewall allow-rule** created when the user answers the first-run prompt persists — Windows creates and owns that rule, an MSI cannot remove what it did not install. The loopback default avoids the question entirely: measured with `DESKTOP_LAN_ACCESS` unset/false the engine binds `127.0.0.1` (verified by `netstat` and by the LAN address being unreachable while loopback answers), and no firewall prompt is raised, so no such rule is created. The `%APPDATA%` retention has one consequence worth naming: when the `xoksa.env` route is used for API keys, it leaves credentials on disk after uninstall (open design question, tracked, not a defect of this build). Two observations recorded, neither a defect: the app has **no single-instance guard** (each launch spawns its own engine on its own ephemeral port), and the Windows Firewall prompt seen at first run comes from `DESKTOP_LAN_ACCESS=true` binding `0.0.0.0` — the loopback default does not raise it. NSIS was measured and **rejected**: its `overlay` payload structure scores 3–4/69 on the container itself.

**VirusTotal (on the shipped hashes).** Engine `xoksa.exe` — **0/69, clean**. Desktop `5d267218…` — **0/70, clean**, reached by locating the cause of the ML detections in the build rather than by signing. Installer `a7f97838…` — **0/62, clean**. The first build (`61987ec4…`) scored 2/70 — Microsoft `Trojan:Win32/Wacatac.B!ml` and Trapmine `Malicious.moderate.ml.score`, both ML heuristics. Controlled single-variable builds established that **neither engine reacts to program code**: both key on the PE resource section (`.rsrc` — the icon), along two *different* measurements. **Trapmine tracks `.rsrc` entropy** (flags 7.928; clean at 5.381 / 4.176 / 2.835). **Microsoft tracks `.rsrc` size** (clean at 2,048 B and 51,200 B; flags at 297,472 B and ~351,000 B; boundary bracketed, not bisected), with two further **measured** necessary conditions: PE version metadata, and `CREATE_NO_WINDOW` on the engine spawn — the latter established by rebuilding the shipped recipe without it (`b80d49bb…`, `.rsrc` bit-identical to the shipped artifact) and rescanning: **1/70, Microsoft `Program:Win32/Wacapew.C!ml`**. All three conditions must hold together; removing any one re-flags the binary. The shipped icon is therefore **uncompressed BMP frames capped at 64×64** — `.rsrc` 32,768 B @ entropy 3.982, inside both clean regions. Version metadata is kept at engine parity: `bundle.publisher` / `bundle.copyright` in `tauri.conf.json` (→ CompanyName / LegalCopyright), `OriginalFilename` via `[package.metadata.tauri-winres]` in the desktop `Cargo.toml`, and a descriptive `FileDescription` applied **post-build with `rcedit`** — tauri-build force-sets `FileDescription = productName` after reading that table, and adding `winresource` to `build.rs` would embed a second `RT_VERSION` and fail to link (tauri #10154), so the descriptive value cannot come from source. **Regression constraint:** restoring a 256×256 or PNG-compressed icon frame re-flags the binary; a 128×128 BMP frame would put `.rsrc` at ~98 KB, between the clean 51 KB and flagged 297 KB samples (untested — requires a fresh scan). **Correction to the earlier record:** the Trapmine residual was previously described here as "behavioural, not metadata … not clearable in code" — that was wrong; it was `.rsrc` entropy, and it is cleared in the build. Method, full experiment matrix and tooling: [av-false-positive-case-study.md](./av-false-positive-case-study.md). Code signing (§5 / §6) remains the definitive fix and remains deferred.

**macOS artifacts (measured 2026-07-27) — signed, notarized, stapled; all measured items PASS.** Built on the same source (`main` at the v2.6.7 line): `trunk build --release` → `cargo build --release --bin xoksa` → `scripts/bundle-engine.sh` → `cargo tauri build` with `APPLE_SIGNING_IDENTITY`. `rustc 1.96.0`, host `aarch64-apple-darwin`.

| Artifact | SHA-256 | Size |
| :--- | :--- | :--- |
| `XOKSA.app/Contents/MacOS/xoksa` (engine, signed) | `39443ca8aec64c561344e8a416cbbd47d0420c3aedc3a847342d2456344b96bc` | 10,370,816 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop` (signed) | `c6dd20cf890e570277147b60af5f2db248c41c749ed085940da1f62706c84a0f` | 6,510,672 B |
| `XOKSA-2.6.7-arm64.dmg` (download, notarized + stapled) | `c1ef8761180fdc596285c8edcbbb8deb5440046f6bc9933c271dde33aaed0823` | 8,430,808 B |

**Payload identity — measured, and it corrects the assumption carried over from Windows.** On macOS the Tauri bundler does **not** patch the bundle-type marker: both the `cargo build` output and the binary inside the `.app` still read `__TAURI_BUNDLE_TYPE_VAR_UNK` (on Windows it becomes `…_MSI`). Stripping signatures from both copies (`codesign --remove-signature`) leaves files of identical size differing in **1–2 bytes only**, inside the `__LINKEDIT` segment's `vmsize` field — a value determined by the signature size. **The shipped binary therefore differs from the built binary by the code signature alone** (pre-sign: engine `2c8a00da…` 10,352,576 B, desktop `e5f6f458…` 6,492,368 B). The binaries inside the DMG are byte-identical to the staged signed ones, and stapling the ticket does not alter them.

**Signing / notarization — PASS.** `Developer ID Application: Kohzoh Tsuchiya (463YK5WF6H)`, hardened runtime (`--options runtime --timestamp`). `notarytool` **Accepted** for the app (`d4f417da-457e-4bff-bbd9-285778187126`) and for the DMG (`46eecabc-d029-45bf-b61b-80b4bbf92c0b`); both stapled (`stapler validate` ok). Gatekeeper: `spctl -a -t open` on the DMG and `spctl -a -t exec` on the `.app` inside it both return **accepted, source=Notarized Developer ID**. `codesign --verify --deep --strict` = valid on disk, satisfies its Designated Requirement.

**Build-environment constraint (recorded so the next release does not lose time on it).** The repository lives under an iCloud-managed `~/Documents`, and the file provider attaches `com.apple.FinderInfo` / `com.apple.fileprovider.fpfs#P` / `com.apple.macl` to the generated `.app` directory; `codesign` then aborts with *"resource fork, Finder information, or similar detritus not allowed"*, and clearing the attributes in place does not help because they are re-applied. The bundle is therefore staged outside the synced tree, `xattr -cr`'d, and signed there; the DMG is built from that staging directory (`hdiutil create -format UDZO` over the signed `.app` plus an `/Applications` symlink), then signed, notarized and stapled.

**Static — PASS.** `cargo fmt --check` clean on all three workspaces; `clippy -D warnings` clean on all three; `cargo test` **265 passed / 0 failed / 1 ignored** (255 lib + 10 integration; the ignored one is the live-Stooq failover e2e); `cargo audit` 288 deps against 1169 advisories = 0; `cargo deny check` advisories/bans/licenses/sources **ok**; `cargo machete` no unused deps. Tooling note: `cargo-deny` **0.18.5 aborts** on the CVSS-4.0 advisory (`RUSTSEC-2026-0073`) — 0.20.2 parses it; the same housekeeping warning as on Windows remains (`deny.toml` ignores `RUSTSEC-2026-0173`, "no crate matched advisory criteria"). `scripts/check.sh` invokes `cargo deny check --config deny.toml` for the webui workspace, which cargo-deny ≥ 0.20 rejects (`--config` is now a global option, before the subcommand).

**SOT — PASS.** `latest_observed_price` 333.02 and `final_score` 1.0 identical across `--stdout-log --log-format csv` (`333.0199890136719` at full f64 precision), `--log-format json`, and `/api/symbol/AAPL/summary` (AAPL, confirmed bar 2026-07-24).

**Runtime & penetration (B.5 playbook against the frozen signed engine `39443ca8…`, `serve --private`) — PASS.** `scripts/inspect.sh` 8/8 (embedded UI 200, CSP present, cross-origin POST 403, oversize 413, `text/plain` 415, same-origin POST 200, CLI analysis non-empty exit 0, backtest API valid JSON). Additionally measured: `/api/health` reports 2.6.7; per-request-nonce CSP (`default-src 'self'`; `script-src 'self' 'wasm-unsafe-eval' 'nonce-…'`; `connect-src 'self'`; `object-src 'none'`) with no `unsafe-inline`; Permissions-Policy denying geolocation/camera/microphone/payment/usb/motion; `X-Frame-Options: DENY`, `nosniff`, `Referrer-Policy: no-referrer`; `xoksa_sid` HttpOnly/SameSite=Strict; K1 two distinct 32-hex sids; path traversal 404 on all six payloads; cross-origin GET 200 (only POST is gated). H1 (`news_title_is_single_line_no_forged_markers`) and H2 (`output_guard_applies_to_every_provider_and_respects_flag`) pass.

**SCA — same counts as the Windows record, reproduced on this host.** `osv-scanner` over all three `Cargo.lock`: native **0**; WASM 2 unmaintained-only (`paste`, `proc-macro-error2`); desktop 17, of which the 11 gtk-rs 0.18 bindings — including the single Medium (`glib`, RUSTSEC-2024-0429) — are the **Linux WebKitGTK backend and are not compiled into the macOS WKWebView binary**.

**Desktop launch — engine spawns and serves.** The signed `.app` starts, spawns its sibling engine on an ephemeral port, and the WebView attaches after the `/api/health` version match. Two observations, neither a defect and both already recorded for Windows: there is **no single-instance guard** (a second launch spawns its own engine on its own port), and the bind address follows the user's own config — on this machine `~/Library/Application Support/app.xoksa.desktop/xoksa.env` carries `DESKTOP_LAN_ACCESS=true` (set 2026-07-21), so the engine binds `0.0.0.0`. The build default is loopback (`xoksa-desktop/src/main.rs::bind_host` returns `HOST` unless the config says `true`); the LAN bind here is the operator's explicit opt-in, not a property of the artifact.

**Popup regression (the one behaviour the `async` rewrite could have broken) — PASS, confirmed on screen 2026-07-27.** The chart popup renders (QQQ daily, price/VWAP/EMA with the RSI, MACD and volume panes) and the HELP popup renders the chat-command list; neither opens blank. The Windows-only injection workaround is gone and both platforms now run the single `async` path.

**VirusTotal (on the shipped macOS hashes, 2026-07-27) — all three clean.** `XOKSA-2.6.7-arm64.dmg` (`c1ef8761…`) — **0/61**, recognised as a signed DMG (`app.xoksa.desktop`). Desktop `xoksa-desktop` (`c6dd20cf…`) — **0/62**, signed Mach-O 64-bit ARM. Engine `xoksa` (`39443ca8…`) — **0/62**, signed Mach-O 64-bit ARM. Each artifact was scanned separately because a container scanning clean does not vouch for its payload (on Windows the MSI scored 0/62 while a payload scored 1/70 at one point). Zero detections across all three — unlike the Windows desktop binary, which needed the icon-encoding work to reach 0/70, the macOS artifacts were clean on the first scan, with Developer ID signing + notarization already in place.

**SBOM — a per-OS inventory now exists, because one SBOM cannot cover both.** The published native SBOM was Windows-host generated, and measurement showed it carries **17 Windows-only crates** (`schannel`, `winapi`, `windows-sys`, `windows-registry`, **`windows-native-keyring-store`**, …) and **zero** macOS ones — same source and same `Cargo.lock`, but a lockfile is the union over all targets while an SBOM lists what is *in that binary*. So `xoksa-native.cdx.json` was renamed to **`sbom/xoksa-native-windows-x86_64.cdx.json`** (199 components) and **`sbom/xoksa-native-macos-arm64.cdx.json`** (**195** components) was generated here by the §C.3 method — `cargo auditable build --release` (throwaway) → `syft file:` read from a neutral directory → `cargo build --release` to restore the clean binary (verified: back to `2c8a00da…`, and the frozen `dist/v2.6.7/` artifacts re-hashed unchanged). The macOS inventory carries `security-framework` / `core-foundation` / **`apple-native-keyring-store`** and no Windows crate; no absolute path appears in the output. The WASM SBOM stays a single file (its `wasm32-unknown-unknown` target does not depend on the build host).

**Binary-bound SCA (new, `cargo audit bin`) — 0 vulnerabilities.** Run against the same auditable throwaway, i.e. the **210 dependencies actually embedded in the macOS engine** rather than the 288-crate lockfile union: exit 0, no advisory. This is the artifact-bound counterpart of the lockfile-level scans, and it is the reason the Linux-only gtk-rs advisories cannot apply to a shipped binary — they are not in it.

---

<a id="ja"></a>

# xoksa — セキュリティ・信頼性・出荷検査

> **言語同期:** 英語と日本語の記載はいずれも有効です。片方の言語にしかない所見・対策・推奨・挙動は両方に適用され、欠落側は文書不備として更新対象です。

> **時点レポート。** 本書は xoksa の **信頼性**（データの出所・失敗時の挙動・再現性）、**セキュリティ診断**（静的レビュー＋動的 playbook ＋所見）、**出荷検査**（リリースの凍結現物証跡）を**1冊に集約**した記録です。スナップショットであり、サーバ/コア面や出荷現物が変わったら再実施します。*設計*（脅威モデル・CIA 制御・データ分類・デスクトップ §6）は [security-design.md](./security-design.md) に別掲します。

> **対象。** `xoksa` CLI / サーババイナリ：分析エンジン・`xoksa serve`（axum HTTP＋JSON API）・静的 Web UI・それが駆動するデータ/LLM プロバイダ。外部サービス（LLM・Brave・Yahoo/Stooq・SEC/J-Quants）は信頼境界のみ評価。**デスクトップアプリ**（`xoksa-desktop`・Tauri）は独自面（OS WebView・Tauri IPC）を持つ*別成果物*で、モデルは [security-design.md §6](./security-design.md)、評価は自身のビルドに対し出荷プロセスで実施します。

---

# Part A — 信頼性・再現性

xoksa がどこからデータを取得し、失敗時にどう振る舞い、どこまで再現性を保証するか——分析結果の監査と信頼の根拠。指標の算術は [analysis-guide.md](../manual/analysis-guide.md)、SOT 思想は [design-philosophy.md](./design-philosophy.md)。

## A.1 データソースと優先順位

| 領域 | ソース | 備考 |
| :--- | :--- | :--- |
| 市場データ（OHLCV） | 主：Yahoo Finance 互換の公開エンドポイント（`query2.finance.yahoo.com`、代替ホスト `query1.finance.yahoo.com`）。代替：Stooq（日足のみ） | 主は Yahoo。Stooq は Yahoo が本当に失敗した場合のみ使うラベル付きフォールバック（A.2）。**非公式エンドポイント**（[README](../../README.md) の免責参照）。プロバイダ構築は単一拡張点（`build_price_fetcher`、[design-philosophy §8.1](./design-philosophy.md)）に集約され、呼び出し側変更なしで差替/追加できる。 |
| ファンダメンタル（日本株） | J-Quants | 認証：`JQUANTS_API_KEY`（v2のみ・V1トークン認証は2026-06-01終了）。 |
| ファンダメンタル（米国株） | SEC EDGAR | `SEC_USER_AGENT`（SEC が要求するアプリ名＋メール）で識別。 |
| ニュース | Brave Search API | **タイトルとURLのみ**。記事本文は取得・保存・LLM 送信を一切行わない。 |

**市場判定:** `.T` または既知の JP コードは日本株、それ以外は米国株（`detect_market`）。非対応シンボルは推測せず報告する。

## A.2 失敗時の挙動とフォールバック

- **市場データ:** 最大2回試行（`MARKET_DATA_FETCH_ATTEMPTS`）・タイムアウト10秒。429/5xx/タイムアウト/接続エラーで再試行。2層：
  - **同一ベンダー内の堅牢化（数値は同一）:** 主ホスト `query2.finance.yahoo.com`、使い切ると `query1.finance.yahoo.com` を再試行。両ホストは**同じ** Yahoo データ＝別ソースではなく純粋な可用性向上。
  - **代替ベンダー（機能低下・ラベル付き）:** Yahoo が完全失敗時は **Stooq**（別ベンダー）へ。無料フィードは**日足のみ**——分足/週足/月足は代用せずエラー＝Yahoo 障害中の分足は「データなし」で、誤データにはならない。Stooq 採用分は警告行1本で**明示ラベル**：`⚠️ 代替データ元を使用: …`。Yahoo 成功時は常にそちらが勝つ。
  - 両方使い切ればエラーを表面化し、値を創作しない。スナップショットを**プロバイダ間で継ぎ接ぎしない**（常に単一ソースから丸ごと＝SOT）。フェイルオーバーは SOT を弱めず可用性を高める。
- **LLM（クラウド）:** 429/503 に最大3回（`MAX_RETRIES`）、`Retry-After` 尊重。Claude は `temperature` が400拒否なら外して自動再送（API レスポンスで判定）。`--no-llm` は LLM を飛ばし計算済み分析のみ返す。
- **ニュース:** Brave 失敗時はエラー表面化。補助情報であり `--no-news` でテクニカル＋ファンダのみでも継続可。
- **数値安全性:** 非有限値はパース境界で除外（`is_finite()`）。値幅ゼロ・ゼロ除算は `NaN`/`Inf` ではなく安全中立値（[security-design §1](./security-design.md)）。

## A.3 再現性と決定性

- **一回限りの計算:** 各指標は実行ごと一度だけ計算し、同一値が画面・ログ・LLM プロンプトへ供給（SOT）。
- **キャッシュ非依存:** 毎回新規取得し、分析計算はキャッシュを読まない（バックテストも新規取得）。後で値が変わるのは相場が動いたからで、ロジックが変わったからではない。
- **同一入力なら決定的:** 固定の入力足系列＋設定に対し指標値・スコアは決定的（有限性チェック・範囲クランプの `f64`、基本は `ta` クレート、他は透明な独自式）。**唯一の意図的例外＝提示順**：等ウェイト指標は**実行ごとランダム順**（先頭指標を固定バイアスにしない）。順序は画面・プロンプトへ渡るが、計算値は変えない。
- **データ鮮度を明示:** `market_data_latest_time`（取得できた最新足）と `analyzed_at`（実行時刻）は別報告。差は隠さず表示。
- **LLM 出力は非決定的:** 決定的な SOT データの自然言語説明で、実行ごと変わり得るが計算値を改変しない。将来予測（`/cast`）は有効時もその値を根拠としたラベル付き予測であり新たな確定データではない。

## A.4 バージョンによる結果への影響

指標式・閾値・既定はマイナー系列内で安定。計算値に影響し得る変更は [CHANGELOG.md](../../CHANGELOG.md) に記録し [version-history.md](./version-history.md) で要約。異なる時刻の2回は差が出て正常（ライブデータ）、**同一の取得入力**なら一致して正常（LLM の文章を除く）。

---

# Part B — セキュリティ診断

## B.1 脅威モデル

サーバは既定 `127.0.0.1`・**認証なし/TLS なし**（文書化済みゲート）。ループバック既定の現実的攻撃者は、**同一ブラウザで開いた悪意ページ**（`localhost:8787` へのドライブバイ CSRF）と、**悪意ある上流データ**（ニュース タイトル/URL・市場データ）。`--host 0.0.0.0` はプライベートネットワークまで面を広げるため、このモードは **fail-closed** である：常に認証トークンを有効にして動き（未設定なら初回の非ループバック起動でエンジンが 256bit を自動生成）、接続元はプライベート／ループバックのみに応答し（それ以外は `403`）、無認証のリクエストは `401` で拒否する。TLS と多テナント型レート制限は単一ユーザのツールとして設計上対象外であり、公開モードを信頼できるプライベートネットワーク内に留めることは、引き続きメンテナの明示的な責任である。

## B.2 攻撃面

| エンドポイント | メソッド | 攻撃者制御入力 | 既存の防御 |
| :--- | :--- | :--- | :--- |
| `/`・fallback 静的 | GET | パス | `read_asset` は `Component::Normal` のみ（`..`/絶対/prefix 拒否） |
| `/api/health`・`/api/config`・`/api/llm/options` | GET | なし/env | version/ラベル/鍵の**有無**のみ。値は返さない |
| `/api/symbol/{symbol}/{summary,news,chart}` | GET | `{symbol}`・query | `sanitize_ticker`（英数/`.`/`-`）。`bars` は `2..=300` clamp |
| `/api/analysis/{context-pack,multi-timeframe}` | POST | JSON | `Json<T>` が `application/json` 強制。multi-timeframe は**有料 LLM** |
| `/api/backtest`・`/api/backtest/rules`・`/api/llm/select` | POST | JSON | `Json<T>`。ルール保存は `name`≤80・`spec_json` 検証＋≤32 KiB・パス固定 |
| `/api/chat/{stream(SSE),commands}` | GET | `message` query | **明示的な同一オリジン検査**＋`MAX_CHAT_MSG_CHARS=8000` |

**共通制御（ミドルウェア）。** 最外周で**接続受入セマフォ**が `MAX_CONCURRENT_CONNECTIONS` 超を処理前に `503`。全レスポンスにクライアント別 **`xoksa_sid`**（32 hex・`HttpOnly`・`SameSite=Strict`）を付与し、セッションは `sid|symbol|timeframe` でキー化＝**他ブラウザのセッションを読めない/書けない**。ヘッダに nonce ベースの strict **CSP** と **`Permissions-Policy`**（位置情報/カメラ/マイク/決済/USB/モーションセンサ拒否）。

## B.3 所見

重大度は特記なき限りループバック既定前提。すべて実コードで確認済み。

### 1. Content-Security-Policy — Medium（多重防御）— **解消（strict, v2.2.2）**
当初（v2.2.1）は CSP なし＝XSS 防御が出力エスケープ一本。**修正:** ポップアップを完全 de-inline（SVG 属性・外部 CSS・委譲リスナ）、WASM ブートストラップ `<script>` にリクエスト毎 nonce → `'unsafe-inline'` を完全排除：`default-src 'self'; script-src 'self' 'wasm-unsafe-eval' 'nonce-{リクエスト毎}'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'`。`Permissions-Policy` も追加。

### 2. body サイズ上限なし／保存戦略ファイルの無制限増加 — Medium（公開時）/ Low（ループバック）— **解消**
非ブラウザや `--host 0.0.0.0` が `application/json` を直接送り、ファイル肥大化・O(n) 再書込で CPU 消費を誘発し得た。**修正:** グローバル `DefaultBodyLimit::max(64 KiB)`、`save_rule` は `spec_json` 32 KiB 上限、保存件数 200 上限（最古退避）。

### 3. 状態変更 POST に明示 Origin 検査なし — Low（多重防御）— **解消**
悪用可能な CSRF 穴ではない（全て `Json<T>`＝`application/json` 強制→サーバが応答しないプリフライトでブラウザが遮断、`text/plain` は 415）。**hardening:** `reject_cross_origin_mutations` でサーバ側拒否＝ブラウザのプリフライトを唯一の防御にしない。

### 4. 保存戦略のパスは固定 — Info（安全）
固定 `~/.xoksa.strategies.json`。ルール `name` は JSON 値のみでパス構築に使わない。パス注入なし。

### 5. 遅いチャット1ターンで summary 停止 — High（可用性）— **解消（v2.2.2）**
セッション状態を単一グローバル mutex 配下に置き LLM ストリーム跨ぎで保持→長いチャットが無関係要求を阻害（`/summary` 約90秒ハング）＝自己 DoS。**修正:** セッション毎 `Arc<Mutex<WebSession>>`＋読み `try_lock`、I/O 越しにグローバル map を保持しない。

### 6. クロスクライアントのセッション分離 — Medium（機密性/完全性）— **解消（v2.2.2）**
クライアント別 id なしでキー化し、同一 symbol/timeframe の2ブラウザが1セッション共有し得た（言語/モード漏れ）。**修正:** ランダム `xoksa_sid`（`HttpOnly`・`SameSite=Strict`）でキーを `sid|symbol|timeframe` に名前空間化。

### 7. ニュースタイトル経由の LLM プロンプトインジェクション — Medium — **解消（v2.3.x・本診断で発見・修正）**
ニュースは攻撃者が影響可能（誰でも記事を出せる）で、脅威モデルも悪意タイトル/URL を上流ベクタとして挙げている。プロバイダのタイトル・URL は LLM 分析/チャットのプロンプトへ**生挿入**されていた（`news.rs`・`NewsItem::prompt_line`）——API から無サニタイズ。細工したタイトルで、改行注入によるプロンプト行構造の破壊、**構造マーカーのなりすまし**（`=== 比較対象 ===`・`【制約】`・`---`・コードフェンス）による偽のシステム発セクション/制約、指示上書き文の埋め込み、予算食い潰しが可能だった。
**修正——多層:**
1. **取り込み境界の無害化（入口）。** プロバイダ応答から `Article` を作る単一箇所（`news.rs`）で、タイトル/日付を制御文字除去の1行化・構造マーカー無害化・長さ上限（タイトル240/日付40）。URL は1行の `http(s)` のみ許容（他は破棄）。1つの choke point → 下流全経路（CLI 分析・チャット・ダッシュボード）が清潔＝SOT と一貫（CLI/GUI 分岐なし）。
2. **出力整合ガードの全プロバイダ化（出口）。** 確定値はプロンプト内にあるので、出力の入力外数値・裸の価格レンジ・独自売買水準・逆方向比較は幻覚か注入効果＝検出し該当文/行を除外。従来 Ollama 限定 → **openai/gemini/claude/ollama** に一律適用（`send_chat_turn_with_usage` ディスパッチャ＋一発分析 sender）、`--no-ollama-guard`/`OLLAMA_NO_GUARD` で無効化可。*（v2.9.7 以降、照合の基準はプロンプトではない。`TechnicalDataGuard`／`FundamentalData` から構築してリクエストと共に渡す——プロンプトには会話や他モデルの発言も載るためである。[security-design.md §1](./security-design.md) 参照。この対策を強めるもので、ここに書いた多層構成は変わらない。）*
3. **構造配置（既存）。** 制約ブロックは untrusted 本文の**後ろ**（近接性）かつ**予算優先で確保**（長いタイトルでガードを押し出せない）、ニュース境界注記で「タイトル＋URLのみ」を明示。
決定的テストが、偽マーカー/改行/制御/長さのタイトル・URL scheme フィルタ・非 Ollama での適用を網羅。SOT（確定値は Rust 側描画）と併せ、残存注入の被害は LLM の**散文**に限定され、確定値には及ばない。

### 8. 通知の送信失敗がログとブラウザに webhook の秘密を出していた — Medium（機密性）— **解消（v2.9.10・本診断で発見・修正）**
Slack・Discord・Google Chat では、秘密は **webhook URL そのもの**である（利用者が URL 全体を登録し、それを持つ者は当人のチャンネルへ投稿できる）。転送層の失敗は `reqwest` のエラーをそのまま出力しており、その `Display` は末尾に ` for url (…)` を付ける（`error.rs:300`）。カナリアトークンと到達不能プロキシで実測：`error sending request for url (https://hooks.slack.com/services/T…/B…/LEAKCANARY123): client error (Connect): …`。アラート監視はこの行を診断ログに書き、`/alert test` は JSON としてダッシュボードへ返していた——[security-design.md](./security-design.md) §0.1（クラス A の秘密はログに書かない）と §4（認証情報はブラウザ境界を越えない）が禁じている。接続失敗・タイムアウト・TLS エラー・DNS 失敗のいずれもこの経路を通るので、きっかけは攻撃ではなく通常の設定ミスである。**修正**は全呼び出し元が通る通知側の 1 箇所：エラーの分類（`is_connect`／`is_timeout`／`is_decode`／`is_body`）と source 連鎖——どちらも URL を含まない——から本文を組み直し、表示直前のテキストから秘密を除去する（LINE の Bearer トークンや、送信内容を反射するプラットフォームのエラー本文にも効く）。原因は残る（`notification failed: connection failed: … Connection refused (os error 61)`）。試験 2 本：実測を反転させた回帰試験（原因が消えていないことも検査する）と、除去処理の単体試験。同じ版で実装した「送信失敗の可視化」の作業中に発見した——その失敗を画面に出すことが、まさにこの秘密を公開する動作だった。


## B.4 検証済み SAFE（防御つき）

- **パストラバーサル/LFI** — `read_asset` が I/O 前に非 `Component::Normal` を全拒否。埋め込みも同検査済み `rel`。`..`・入れ子・絶対をテスト網羅。
- **コマンド注入** — エンジン `src/` に `Command::new`/`std::process::Command`/shell 実行 0 件。`sanitize_ticker`/`sanitize_news_query` が `;` `|` `` ` `` 除去。*（デスクトップの `open_external` は URL を非シェルの単一引数で `std::process` 実行・http(s) 限定。評価はデスクトップ現物で。）*
- **XSS** — 外部/ユーザ文字列（ニュース title/URL・チャット・分析文・会社名）は Leptos のエスケープ済みテキストノードか `analysis_html`/`html_escape` 経由、生 `innerHTML` ではない。ニュース URL は `safe_http_url`（`http(s)` のみリンク化、`javascript:` 遮断）＋`rel="noopener noreferrer"`。ペネトレで1例外——チャートポップアップの `set_inner_html` が ticker を未エスケープ埋込（反射 XSS）——**修正**（`chart_doc_html` で `html_escape`、`html_escape` は `'` も対象化）、ポップアップ message 受信は後に完全撤去（E3）。
- **SSRF** — 送信先ホストはハードコード（`query{1,2}.finance.yahoo.com`・`stooq.com`・`api.search.brave.com`）、symbol は `sanitize_ticker` 制限＋`urlencoding::encode`＝ホスト/パス/認証子を注入不可。
- **秘密/情報漏洩** — 鍵はレスポンスに出ない。`/api/llm/options` は鍵の**有無**のみ。エラーは鍵**名**であり値でない。鍵は OS キーチェーン＋zeroize。
- **SQLi** — SQL シンクなし（DB 撤去、永続化はフラット JSON のみ）。
- **依存 SCA** — `cargo audit`（native）と `osv-scanner`（native＋`webui-leptos`）で**脆弱性 0**。CI は両 workspace に `cargo deny`（advisories＋ライセンス＋crates.io 限定 source 許可＋重複/禁止）。リリースは `--locked`＋`cargo auditable`（出荷バイナリが依存リスト内蔵、`cargo audit bin` で復元）。webui の**ビルド時 proc-macro** に *unmaintained* INFO 2件（`paste`・`proc-macro-error2`、leptos 推移的）——**脆弱性ではなく**コンパイル時のみ・**配布 WASM 非搭載**・leptos 上流移行まで除去不可。

## B.5 動的テスト playbook（Phase 3）

`xoksa serve` のアプリ層診断。**LLM 経路（`/api/chat/stream`・`/api/analysis/multi-timeframe`）は自動スキャナから除外**（課金/レート制限）。テスト起動 `xoksa serve --private --port 8787`。コマンドは上の English playbook と共通（言語非依存）。乖離結果は regression として起票。

## B.6 状態

ループバック既定に悪用可能な脆弱性は残らない。可用性・クロスクライアント分離（#5–#6）、news→LLM 注入（#7）、通知失敗による秘密漏出（#8）は修正済みでテスト済み。`--host 0.0.0.0` は開放ではなく **fail-closed** である：常にトークン必須で動き（未設定なら初回の非ループバック起動でエンジンが 256bit を自動生成）、接続元はプライベート／ループバックのみを受け付け、信頼できるプライベートネットワーク内に留める（生の公開インターネットには置かない）。TLS と多テナント型レート制限は単一ユーザのツールとして設計上対象外で、公開運用は引き続きメンテナの責任。

## B.7 デスクトップアプリ（Tauri）面 — v2.3.x

デスクトップアプリ（`xoksa-desktop`）は同一エンジン＋ダッシュボードを OS WebView で包み、**Tauri IPC** 境界を足す。ダッシュボードはループバック（`127.0.0.1:8787`）配信＝ブラウザ面と同じ攻撃者影響可能なコンテンツ（ニュース・LLM 出力）を載せるため、IPC 設計は最小権限：**その高リスクなリモートオリジンから届くのは窓/リンクの2コマンドのみ**。

| # | 攻撃面 | 基準 | 判定 |
|---|---|---|---|
| DT1 | IPC 最小権限（リモート＝ダッシュボード） | 高リスク面から呼べるのは安全なコマンドだけ | **合格** — リモート許可（`allow-open-popup-window`・`allow-open-external`）が公開するのは `open_popup_window` ＋ `open_external` のみ。`apply_onboarding`／`load_config`／`list_ollama_models`／`restart_app`／`open_manual`／`setup_status` は**リモート ACL なし**＝ダッシュボードから呼べない（自作コマンドはリモートで明示許可が必須）。 |
| DT2 | `open_external` コマンドインジェクション | シェルメタ文字を実行させない | **発見＆修正** — Windows 経路が `cmd /C start "" <url>`。`&` `|` `^`（クエリ）を含む URL は cmd の区切りで、Rust はスペースなし引数をクォートしない→任意コマンド実行。修正：`rundll32 url.dll,FileProtocolHandler <url>`（シェル解釈なし）。`http(s)` 限定。macOS `open`／Linux `xdg-open` は URL を非シェルの単一引数で渡す。 |
| DT3 | `open_popup_window` の宛先 | ループバック限定・注入なし | **合格** — host/port はハードコード（ループバック）、`kind` は ASCII 英数字のみ、symbol/tf/lang はクエリへ percent-encode。 |
| DT4 | プラグイン面 | 危険な capability なし | **合格** — `tauri-plugin-shell`/`-fs`/`-http` なし。capability は `core:default` ＋2コマンドのみ（任意 shell/fs/http なし）。 |
| DT5 | API キー入力 | 鍵は HTTP を通らない | **合格** — `apply_onboarding`（ローカル限定）は鍵を子プロセスの stdin 経由でエンジン（`xoksa apply-config`）へ渡し、エンジンが OS キーチェーンに書き込む。鍵は HTTP サーバもプロセス引数も通らず、デスクトップは書込み後に JSON バッファを zeroize する。`load_config` は非機微設定のみ返す。 |
| DT6 | popup 窓 | 最小権限＋イベント経路 | **合格（残差小）** — `popup-chart`/`popup-help` は `core:event:default` のみ（窓/fs/shell なし）。`chart-brush`/`help-pick` イベントは区間/コマンドデータをメイン窓のチャット入力へ渡すが、コンテンツは自前のループバック UI で、ペイロードは SOT ＋出力整合ガードの対象となるチャット入力データ。 |
| DT7 | リモートの過剰付与 | リモートを必要分に絞る | **解消（v2.3.x）** — main 窓の権限を**分割**：ローカル `default` capability（`core:default`・app オリジンの onboarding フォーム用）と、別の リモート `dashboard` capability（ループバックに `open_popup_window`＋`open_external`＋`core:event:default` **のみ**付与）。高リスクなリモートは `core:default` を受け取らない。実機検証済み：チャート/Help/外部リンク＋ドラッグ添付＋設定フォームがすべて動作。 |
| DT8 | env 値インジェクション | onboarding 値が env 行を注入できない | **解消（v2.3.x・ローカル限定）** — `clean_env_value` ヘルパが、`xoksa.env` に書く全ユーザ値（クラウドモデル名・Ollama alias/host/port/model・`format_env_assignment` 経由の `SEC_USER_AGENT`）から制御文字を除去し長さを上限化。貼り付け改行で2行目を注入できない。 |
| DT9 | onboarding フォームの CSP（hardening） | キー入力ページの多重防御 | **hardening（保留）** — `tauri.conf.json` は `csp: null`。app オリジンの onboarding フォームに CSP なし。外部/ユーザ制御コンテンツを描画しない（現状 XSS シンクなし）がキー入力を扱う。保留理由：フォームの inline script＋Tauri IPC 許可との両立が地味に面倒で、現時点の便益が低い。 |

**結論。** デスクトップの IPC 面は高リスクなリモートに対し最小権限（窓＋外部リンク＋イベント経路のみ到達）、鍵の扱いは設計どおり（IPC→キーチェーン・HTTP を通らない）、shell/fs/http プラグインは無効。実欠陥を1件発見・修正（DT2・Windows `open_external` コマンドインジェクション）。DT7（リモート最小権限化）と DT8（env 値サニタイズ）は実装・検証済み。DT9（フォーム CSP）は保留。現物依存のデスクトップ検査（署名/公証済み現物・ハッシュ・VirusTotal）は出荷時に Part C へ記録。


## B.8 OWASP API Security Top 10 (2023) の判定 — v2.9.10

`src/` を 2023 年版に突き合わせた静的レビュー。初回は 2026-09-29、**2026-10-01 にコードへ再度突き合わせた**（当該レビューが促した修正が入ったあと）。判定が動いたのは 2 件（API7・API10）、改善が 1 件（API4）で、他は変わっていない。以下の「設計上」は [security-design.md §0.2](./security-design.md) に記録した範囲の決定を指し、未検討の欠落ではない。

| | 項目 | 判定 | 根拠 |
|---|---|---|---|
| API1 | Broken Object Level Authorization | **適合** | セッションは `HttpOnly` cookie の `sid` で名前空間化し、`parse_session_cookie` は **32 桁 hex 以外を拒否**（`src/server/mod.rs`）。body やクエリから `sid` を受ける口は 1 つも無い。 |
| API2 | Broken Authentication | **部分的** | 定数時間比較・256bit（OS 乱数）・`HttpOnly`／`SameSite=Strict`。cookie が運ぶのは**トークンそのもの**——設計上（§0.2）。`/auth` に試行制限は無い。 |
| API3 | Broken Object Property Level Authorization | **適合** | `/api/config` が返すのは UI 語彙と既定値のみ。鍵は**有無**だけで値は返さない（`resolve_key_presence`）。鍵はプロセス外へ出ない。 |
| API4 | Unrestricted Resource Consumption | **部分的**（従来：実質未対応） | body 上限 64KiB。接続受入は `u16::MAX`（メモリと fd が先に尽きる値）から **256 に変え、超過は `503` で shed する**。外向きクライアントのタイムアウトに 15 分の上限を置き、設定値で許可証を無期限に保持できないようにした。ハンドラ単位のタイムアウトは依然として無い。**v2.9.10（Windows）で、下げた上限ではなく本来の 256 のまま出荷現物に対して実測：** 保留 192 本で `200`、256 本で `503`、半分を解放すれば `200` に復帰する。以前のリリースでは上限を 2 に下げたビルドでしか shed を示せなかった——許可証を持つにはリクエストが*受け入れられている*必要があり、思いつく負荷生成器はヘッダも絞るので、接続がリクエストにならなかったためである。 |
| API5 | Broken Function Level Authorization | **該当なし** | 役割が無く、あるのは認証だけ。トークンを持つクライアントは全部できる。これは単一利用者設計（§0.2）であって欠陥ではない。 |
| API6 | Unrestricted Access to Sensitive Business Flows | **部分的** | `/api/alerts/test` に連打制限が無く、利用者自身のチャンネルへ繰り返し投稿できる。監視側のレート制御（立ち上がり 1 回）はこの経路に効かない。 |
| API7 | Server Side Request Forgery | **適合**（従来：部分的） | 送信先はプラットフォーム別の固定許可リストで `https` 強制（`src/notify.rs`）、ブラウザから宛先は操れない。本レビューが見つけた穴——**リダイレクト追従で許可リストが外れる**——は塞いだ：通知・各 LLM・Ollama はリダイレクトを拒否し、市場とニュースは 3 回までに制限。 |
| API8 | Security Misconfiguration | **適合** | nonce ベースの strict CSP（`unsafe-inline` 無し・`connect-src 'self'`）、`nosniff`・`X-Frame-Options: DENY`・`no-referrer`・Permissions-Policy、状態変更 POST へのサーバ側 Origin 検査、SSE への明示的な同一オリジン検査、`Json<T>` の Content-Type ゲート。 |
| API9 | Improper Inventory Management | **部分的** | `/api/health` が名前と版を返し、デスクトップは完全一致を要求する。パスにバージョンは無く、機械可読の仕様も無い——対で出荷する以上、設計上（§0.2）。 |
| API10 | Unsafe Consumption of APIs | **部分的**（従来：未対応） | 全外部クライアントにタイムアウトがあり、§1 の出力整合性ガードは本項目が LLM 応答に求める水準を超えている。リダイレクトは制限済み（API7）。応答サイズの上限は **Ollama 経路のみ**——平文 HTTP で接続先が利用者設定であり、意図した相手以外から応答が来得る唯一の経路だからで、ホストがコード固定の TLS 経路は意図的に触っていない。 |

**この判定が覆っていないもの。** これはソースレビューであって動的試験ではない（リリースごとの動的証跡は Part C）。本レビューが挙げた未確認 3 件のうち 2 件は解消した：`reqwest` はクロスホストのリダイレクトで **`x-api-key` を除去しない**（crate の `remove_sensitive_headers` に対する実測。API7 の修正が鍵の漏出経路も同時に塞いだ理由がこれである）、ZAP はそれ以降の各リリースの出荷現物に対して実施しており、直近は v2.9.10（Windows）の 8 アラート・High 0 で、各アラートの根拠をその現物で実測している（C.4）。残る 1 件は未了：API3 は `/api/config` と鍵の存在確認で確かめたのであって、全ハンドラの応答項目を網羅したわけではない。

---

# Part C — 出荷検査（凍結現物の証跡）

出荷検査は**1つの凍結現物にハッシュで束縛**する：1回ビルド → その現物を検査 → その現物を出荷。ソース検査（`fmt`/`clippy`/tests/`audit`）はソース/依存のみ解析しリリースバイナリへ何も書かないので、実行中も検査ハッシュは固定。手法はリリース間で安定、下の測定ハッシュ/結果は**リリース別**。

## C.1 手法（リリース別）

| 段階 | チェック |
| :--- | :--- |
| 静的/ソース | `cargo fmt --check`（差分なし）・`cargo clippy -- -D warnings`（0）・`cargo test`（失敗0）・`cargo audit`/`osv-scanner`/`cargo deny`（脆弱性0） |
| SOT ゲート | csv == json == api（出力等価） |
| ランタイム | `--version`・`serve` ループバック bind＋`health` 200・strict CSP＋`xoksa_sid`・CSRF/過大/Content-Type ゲート（403/413/415）・CLI 分析 非空 exit 0・backtest API 正常 JSON |
| ペネトレ | A1〜K3 マトリクス（下）を凍結現物に対し測定＝Part B 所見のランタイム対応 |
| 外部 | OWASP ZAP パッシブ・出荷ハッシュへの VirusTotal・SBOM（CycloneDX：`cargo auditable`＋syft） |

**ビルド証跡（再現レシピではない——MSVC は非ビット再現・WASM は**同一 runner 内で**決定的であり、OS をまたぐと一致しない）:** `cargo build --release`（既定 `embedded-ui`・自己完結・**auditable データは焼き込まない**＝SBOM 生成専用）。`[profile.release]`：`strip = true`・`lto = true`（fat）・`codegen-units = 1`。

**WASM の「決定的」が覆う範囲（実測）。** 同一 runner で同じ workflow を 2 回走らせた出力はバイト一致する——v2.9.10 の macOS で実測しており、実質同一ソースの 2 ビルドが同じ 4 つのハッシュを与えた。**OS をまたぐと一致しない：** 2.9.10 の macOS フロントエンドは `xoksa-webui-2fc5f86e0258a0ca`（`4aa431b1…`・710,967 B）、Windows フロントエンドは `xoksa-webui-22fbb44a3ccaa34a`（`983596d5…`・724,577 B）であり、ソースも `webui-leptos/Cargo.lock` も同じである。これを明記するのは、2 つの §C.4 記録を見比べた読み手が、異なるハッシュを矛盾と読むからである——インベントリが **1 本**なのは依存集合がホスト非依存だからであり、WASM のバイナリが **2 本**あるのはビルドがそうでないからである。

**リリースのビルド recipe — ここに 1 回だけ記す。§C.4 の記録は手順を書き直すのではなく、コミット・host・ハッシュを名乗る。** 書き直しは実際に手順を失わせた：v2.9.2 の記録は「`cargo build --release` in `xoksa-setup` を 2 本目の sidecar として配置」と書いていたが、v2.9.3 から v2.9.8 までの 5 本が「`xoksa-setup` を 2 本目の sidecar として配置」と短縮し、その結果 v2.9.9 のビルドが 2.9.8 の設定アプリを配置した——書かれた recipe のどこにも「ビルドする」と無かったためである。

1. `cd webui-leptos && trunk build --release` — エンジンが内蔵する WASM フロントエンド。
2. `cargo build --release --bin xoksa` — エンジン。
3. `cd xoksa-setup && cargo build --release` — 設定アプリ。**配置するだけでなくビルドすることが recipe の一部である**：ルートのビルドが触らない別 workspace であり、2 本目の sidecar としてインストーラに同梱される。
4. `scripts/bundle-engine.sh` — **両方**を Tauri の sidecar として配置する（`bundle.externalBin`）。どちらかを複製する前に両方を解決・検査し、`Cargo.toml` の版数を報告しない現物は配置を拒否するので、ビルド忘れは出荷ではなくバンドルの失敗になる。
5. `cd xoksa-desktop && cargo tauri build` — インストーラ（macOS では `APPLE_SIGNING_IDENTITY` を設定）。

**v2.9.9 以降、リリースビルドは開発機ではなく CI で行う。** `.github/workflows/release-windows.yml` が上記 1〜5 を `windows-latest` ランナーで実行し、アップロード前に全現物へ `scripts/check-no-host-paths.sh` を掛ける。理由は §C.4 の v2.9.9 の記録にある——rustc は panic メッセージのために各クレートのソースパスをバイナリへ焼き込むため、開発機でのビルドはビルド者のアカウント名を出荷してしまい、`strip = true` でも `--remap-path-prefix` でも全部は消えない。開発目的のローカルビルドは従来どおりでよいが、検査と出荷の対象ではなくなった。

**検査するエンジンは、`webui-leptos/dist` が存在しないディレクトリから起動する。** `--web-dir` の既定はこの**相対パス**であり、`read_asset` はディスク上のファイルをバイナリの埋め込みより優先する——開発時の利便性のためだが、結果としてリポジトリ直下から起動したエンジンは自身の埋め込みではなく作業ツリーのフロントエンドを配信する。その状態での検査は現物に束縛されない：本リリースの最初の ZAP スキャンは、検査対象が CI 現物であるにもかかわらず古いローカルのフロントエンドを走査していたため破棄した。どのスキャンでも、`/` を取得して参照されている `xoksa-webui-<hash>` を現物の WASM と突き合わせ、束縛を確認すること。

**検査をソースに束縛する。** ハッシュが示すのは*どのファイルを検査したか*であって、*どのソースから生まれたか*ではない。v2.6.8 のリリース作業で、この隙間が実際に 2 つの失敗として現れた。ソースをコミットする前にリリースを公開したため `gh release create --target main` が**コードを含まないコミット**にタグを打ったこと。そして検査の最中に依存の更新がマージされ、最初のビルドがマージ後のソースから再現できなくなったことである。以下の 6 項目でこれを塞ぐ。v2.7.0 以降の記録はすべてこれに従っている。

1. **コミット済み・作業ツリー clean からビルドする。** 未コミットの編集の上で作った現物には、名乗るべきソースが無い。
2. **公開はマージの後に行う。** 順序を逆にしない。
3. **タグは実体を含むコミットに打つ。** 打ち間違えたリリースは `git tag -f` と**タグ ref** の force push で是正する——ブランチの履歴を書き換えるのではなく、参照を付け替える操作である。
4. **§C.4 の記録に、ソースコミットとそのブランチを現物のハッシュと並べて明記する。**
5. **ビルドから公開までの間にベースが動いたら、作り直して再検査する。** 現物は 1 つのソースに束縛される。動いたソースは別のソースである。
6. **release ブランチは削除しない。** Squash & Merge はブランチのコミットを畳んで `main` に新しい 1 個を作るため、記録が名指しするコミットは**そのブランチにしか残らない**——v2.9.8 時点の実測で、`5065da8`（v2.9.7）と `e8b973d`（v2.9.8）はいずれも `main` から到達不能で、`origin/release/v2.9.7` と `origin/release/v2.9.8` にのみ存在する。ブランチを消すと記録の参照先が消える。本リポジトリで `deleteBranchOnMerge` を無効にしているのはこのためである。
7. **§C.4 の見出しには日付つきの事実だけを書き、可変の状態を書かない。**「未出荷」は、そうでなくなるまでは真で、変える仕掛けが無い。実際に v2.9.9 の記録はこれを載せたまま**12 分後にリリースが出て**、反対側（macOS）がそれを読んで未公開と信じ、公開物と一致しないソースから現物を作り直した——半日の損失。よって見出しには**いつ検査したか**と**いつ公開したか**——どちらも後から変わらない——を書く。**現在の**状態は 1 箇所（引継ぎ書）にだけ置き、追記ではなく上書きする。**公開は、§C.4 の見出しに公開日が入るまで終わっていない**（日英とも）。

**リポジトリの引っ越しをまたぐ場合。** 本プロジェクトは、`main` の現在のツリーだけを新しいリポジトリとして公開し、**履歴は持ち込まない**計画である。§C.4 の記録が名指しするコミットも、それを保持する `release/*` ブランチも、履歴とともに元のリポジトリに残る（タグも同様である。タグは指し示すコミットと一緒にしか移動しない）。ここで記録する判断は、**引っ越し元を削除せず非公開のアーカイブとして残す**ことである。したがってそれらのコミットは引き続き到達可能であり、引っ越し前に書かれた記録が指すのは公開リポジトリではなく**このアーカイブ**である。規定 6 はアーカイブに対して効き続ける——release ブランチはそこで生き残らなければならない——また、引っ越し後に最初に書く §C.4 の記録には、どのリポジトリに束縛されているかを明記する。これにより、どの記録も自分のコミットの在り処が曖昧にならない。

## C.2 ペネトレ・マトリクス（凍結現物に対し測定）

| # | 攻撃面 | 基準 | 判定 |
|---|---|---|---|
| A1–A4 | SSRF / scheme・path 注入 / redirect | 送信先ホスト固定・symbol 無害化＋encode | 合格 |
| B1–B5 | ネスト JSON / NaN-Inf / 未知トークン / 制御文字 / OS メタ文字 | 溢れ/panic/実行なし・エスケープ/無視 | 合格 |
| C1–C2 | 静的パストラバーサル / `--web-dir` 逸脱 | 全404・`Component::Normal` のみ | 合格 |
| D1–D5 | Content-Type バイパス / クロスオリジン変更 / method override / body 上限 / `spec_json` 上限 | 415 / 403 / 405 / 413 / 拒否 | 合格 |
| E1–E5 | 反射/格納/DOM XSS / popup message / `data:` 読込 | エスケープ・`postMessage` 撤去・CSP 遮断（E1 発見修正） | 合格 |
| F1–F4 | エラー漏洩 / 秘密露出 / `--private` 無痕 / `/api/config` 過剰 | なし・最小 | 合格 |
| G1 | リソース枯渇 DoS | 入場上限（503）・応答維持 | 合格 |
| H1 | プロンプトインジェクション | フレーム保持・タイトルを取り込み境界で無害化・上書き/奪取拒否（#7） | 合格 |
| H2 | LLM 数値改ざん | SOT 不変・出力整合ガードを全プロバイダで | 合格 |
| I1–I2 | 依存 SCA（native / webui） | 脆弱性0。警告のみ——unmaintained な proc-macro 2 件（ビルド時・配布 WASM 非搭載）と、`event-listener` 5.4.1 の unsound `RUSTSEC-2026-0221`。後者は配布 WASM に**載る**が、非共有メモリ（`limits_flags=0x00`）のためスレッドが無く到達不能 | 合格 |
| J1 | `0.0.0.0` 公開アクセス | fail-closed——常にトークン必須（自動生成）・接続元はプライベート／ループバック限定（`401`／`403`） | 合格（v2.9.8 でトークンゲートごと LAN 経由で再実測） |
| K1–K3 | クロスクライアント分離 / summary 自己DoS / 無制限並行 | `sid` 分離・セッション毎ロック・入場上限 | 合格 |

**OWASP ZAP パッシブ**（別ホスト・`0.0.0.0`・LAN）：v2.9.2 以降の定常は **8 アラート・High 0**（通常は Medium 2・Low 1・Info 5）で、いずれも単一ユーザ・同一オリジン・非CDN・非DB のアーキで非該当として許容する：`GET /login` の Anti-CSRF（指す先の `/auth` POST は 256bit トークン自体が門。誤トークン・空トークンとも 401 を実測）、SRI 欠如（アラートの対象はフロントエンド資産で、実際に配信される HTML は 3 つのサブリソースすべてに `integrity` を持つ）、Unix タイムスタンプの露見（市場と分析の時刻。クラス C）、`POST /auth` の Cookie ポイズニング（[security-design.md §0.2](./security-design.md) が記録する範囲の決定を正しく観測したもの——認証 cookie は派生したセッション ID ではなく認証情報そのもの）、セッション管理レスポンス・モダン Web アプリ・localStorage（UI 状態のみ。`xoksa_sid` は `HttpOnly` cookie でサーバ側が保持）・疑わしいコメント（wasm-bindgen 自身の生成出力）で、これらは Info。**件数は v2.6.7 まで 6 件、それ以降は 8 件である**。増えた 2 件は、serve のトークンが面に加えた認証経路に出る 2 件そのものであり、退行ではない。回ごとの件数と各アラートの実測根拠は §C.4 にある。`/api/chat/stream`（SSE）はプロキシで終端できず（バッファ→502）、制御は直接検証（403/413/200）。

**ネットワーク脆弱性スキャナ（Nessus/OpenVAS）— 根拠ありでスコープ外。** 既製品＋版を CVE プラグイン DB に照合する方式で、xoksa は自己完結の Rust バイナリ（axum/hyper/rustls/tokio・静的）＝フィンガープリントできる既製ミドルウェアがない。観点は正しい層でカバー済み：既知 CVE → 依存 SCA（I1/I2）、カスタムロジック → アプリ層マトリクス＋ZAP。

## C.3 SBOM — 生成方法

要件2つ：(1) 開発者絶対パスを載せない（生成後編集での除去は原則却下）、(2) Cargo.lock 全体でなく**バイナリに入っている物**を列挙。`cargo-cyclonedx` は `bom-ref path+file:///<絶対>` を漏らす（抑制不可）、`syft dir:.` は lockfile 全体で過剰。**採用:** `cargo auditable build --release`（使い捨て・依存埋込）→ `syft file:<バイナリ>` が埋込を読む → 正確・パスなしの CycloneDX → その後 `cargo build --release` で**クリーンな出荷バイナリを作り直す**（監査データなし）。使い捨ては中立ディレクトリから読ませ、出力は編集しない。`build.rs` は `cargo:rerun-if-changed=build.rs` を出し、SBOM のコミットで全体再ビルドが起きない。

## C.4 リリース記録（出荷現物ごと）

**以下の記録が名指しするコミットは、どのリポジトリにあるか。** 本リポジトリは `Kozo2000/xoksa-dev` の
`9d05cc7` のツリーから、**履歴を持ち込まずに**公開したものであり、それがここでの最初の
コミットである。引っ越し前に書かれた記録（v2.9.10 以前）が名指しするコミットと
`release/*` ブランチは `Kozo2000/xoksa-dev` にしか存在しない。`Kozo2000/xoksa-dev` を**削除せず非公開のアーカイブとして
残している**のは、まさにそれらを到達可能に保つためである（§C.1「リポジトリの
引っ越しをまたぐ場合」）。記録中のタグの記述も同じ読み方をする——`v2.9.10` は
アーカイブの `9d05cc7` を指し、ここにある同名のタグは本リポジトリ自身の最初の
コミット（ツリーは同一）を指す。次のリリース以降に書く記録は、本リポジトリに
束縛される。


### v2.2.3（2026-07-09 出荷）
Windows・macOS バイナリ、`cargo build --release`（既定 `embedded-ui`）、`rustc 1.96.0`・LLVM 22.1.2。ハッシュは相互検証（`sha256sum`＋`CertUtil`）。静的/ランタイム/ペネトレ全項目 合格（33/33）。出荷ハッシュへの VirusTotal：**0/69**（Windows）・**0/62**（macOS）＝on-artifact・clean。未署名・**notarize なし**（Gatekeeper が初回停止）。1 ignored は実 Stooq フェイルオーバ e2e（外部 anti-bot 遮断＝欠陥ではない）。

| 成果物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| `xoksa.exe`（Windows x86_64） | `67c2c7823e80c50d57d0072073e93d008570ae7921535ebcf1c27c0707c9bbfa` | 28,739,072 B |
| `xoksa-macos-arm64.zip`（配布物） | `1fed997d01338b783973fd9c8dc4719ec941673237d9172da7f8dbbde3fbbc02` | 9,161,674 B |
| `xoksa-macos-arm64`（展開後） | `3c67efabe79b9fb126190746bcae2ac48b736dc7567f89585bf46bee78316361` | 21,108,400 B |

### v2.3.x（デスクトップ系）— ソース／動的 合格・現物依存は保留
v2.3 系は**デスクトップアプリ**（Tauri・別成果物 `xoksa-desktop`）と、コア変更（ニュースタイトルの取り込み境界サニタイズ・出力整合ガードの全プロバイダ化＝Finding #7）を含む。現行 head は **v2.3.4**（`cargo test` ライブラリ245＋統合10 合格・1 ignored）。

**ソース／動的層 — 2026-07-19 測定（v2.3.4）・全項目 合格。** `xoksa serve --private` に対する B.5 playbook：per-request nonce 付き strict CSP・`unsafe-inline` なし／`xoksa_sid` HttpOnly・SameSite=Strict／クロスクライアント `sid` 分離（K1）／パストラバーサル 全404／CSRF・Content-Type・過大 body ゲート 403/415/413／ループバック GET 200。**`--host 0.0.0.0` も実測（「使わない前提で未検査」ではなく）：** 同じ B.5 制御を LAN IP 経由の `0.0.0.0` に対して再実行し、全て同一に機能（strict CSP＋ヘッダ・`sid` 分離・トラバーサル404・Content-Type 415・過大 body 413）。CSRF ゲートはリクエスト自身の `Host` と一致判定（127.0.0.1 決め打ちではない）ため、**同一オリジンの LAN クライアントは許可(200)・クロスオリジンは拒否(403)**。差分は**設計上の無認証**のみ＝公開運用時の認証/TLS/レート制限は運用者責任（リバースプロキシ）であり、アプリ層の欠落ではない。プロンプトインジェクション／出力ガード試験 合格（タイトル・URL サニタイズ、全プロバイダのガード）。**SCA は `osv-scanner`（Cargo.lock ×3）で 0 vulnerabilities** — 未メンテ INFO 5件のみ（`paste`・`proc-macro-error2`＝ビルド時 proc-macro で出荷実体に含まれない／`unic-common`・`-ucd-ident`・`-ucd-version`＝デスクトップ Unicode 依存）、いずれも悪用不可。`cargo audit`/`cargo deny` は現状、同梱パーサ未対応の CVSS 4.0 助言でエラー（xoksa の欠陥ではなくツール版の問題）→ SCA は osv-scanner で代替し、`cargo-audit` は追って更新推奨。**デスクトップ回帰確認（本系の新フォーム）：** 複数 Ollama 設定フォームは `OLLAMA_<n>_ALIAS/HOST/PORT/MODEL` を全て `clean_env_value`（制御文字除去＋512上限）経由で書き込み → DT8 維持・env 行注入不可。チャートのホバーツールチップは `data-tip` 属性＋`textContent`（html エスケープ済み・`innerHTML` 不使用）で描画 → XSS 安全。

現物依存項目（現物別ハッシュ・VirusTotal・**notarization**・SBOM）は出荷バイナリ凍結時に本節へ記録。デスクトップ現物は自身のビルドで評価（[security-design.md §6](./security-design.md)）。

### v2.7.0（エンジン：2026-08-03 検査／未出荷）— ソース／動的／SBOM 合格・現物依存（VirusTotal・署名・macOS）は保留

v2.7.0 は、デスクトップを 2.7.0 に合わせてエンジンの版数チェックを外し（デスクトップは `/api/health` の `service` 同一性で同梱エンジンに接続する。版数ゲートは持たない——#112 の同一メジャー判定を置き換える。デスクトップとエンジンは対で出荷される）、日本株の銘柄名一覧を JPX の `data_j.xls`／`.xlsx` から直接読み込めるようにし（`calamine` 経由なので Excel→CSV／UTF-8 変換が不要）、セットアップ画面にネイティブのファイル選択を追加し、ダッシュボードの UX 不具合 2 件を直した（空状態が「読み込み中…」ではなく「銘柄を入力してください」になる／最後の銘柄を消したときに Market パネルをクリアする）。PR #114 ／ CHANGELOG [2.7.0] 参照。

**ローカル・エンジン検査 — 全項目を出荷ビルドのエンジン `A7D26EAD4A20DC58951C5264BA454C89B5DDA43B493AECA67F9E95539C151409`（13,578,752 B・`cargo build --release`・既定 `embedded-ui`・`rustc 1.97.1`）に対して実測。** **静的** — `check.sh` の 12 ゲートすべて green（engine・webui-leptos［wasm32］・xoksa-desktop の fmt / clippy `-D warnings` / test、`cargo audit` ×2、`cargo deny` ×2、`cargo machete`）。同じゲートは CI でも green（PR #114）。**SOT** — `latest_observed_price` 308.91 と `final_score` 5.0 が csv / json / `/api/symbol/AAPL/summary` で一致。**ランタイム／ペネトレ（`serve --private` に対する B.5）** — `inspect.sh` 8/8（埋め込み UI 200・CSP あり・クロスオリジン POST 403・64 KiB 超の body 413・`text/plain` 415・同一オリジン POST 200・CLI 分析がレポートを出す・バックテスト API が妥当な JSON を返す）。ヘッドレスの dump-DOM で WASM アプリの mount を確認（`XOKSA` のブランドとパネル／銘柄／足の目印）。**SCA／SBOM** — `sbom/xoksa-native-windows-x86_64.cdx.json` をこのホストで再生成（**CycloneDX 1.5・211 コンポーネント・絶対パスなし**）。`calamine` とその推移的依存を含むようになった（コミット済みの SBOM は `calamine` 追加前に生成されたもので古かった）。**現物ベースの `cargo audit bin`** を内蔵 233 依存に対して実行：**脆弱性 0**。

**現物依存項目は保留（出荷時に本節へ記録・C.4 の慣例通り）：** 出荷ハッシュへの **VirusTotal** は**コード署名まで延期**する——未署名で新規ハッシュの desktop は [§6.2／§6.3](./av-false-positive-case-study.md) のレピュテーション誤検知を再現するだけである（Windows の署名は Certum Open Source Code Signing、購入済み・発行待ち）。**OWASP ZAP** パッシブスキャン（Parrot ホスト）。**desktop** の現物依存検査（署名・公証済みの現物、ハッシュ、VirusTotal）と **macOS** エンジンの SBOM 再生成（`calamine` 込み・Mac 機で）は、それぞれの出荷段で行う。実網の Stooq フェイルオーバー e2e は ignored の 1 本として残る。

| 現物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| エンジン `xoksa.exe`（ローカル `cargo build --release`・未署名） | `A7D26EAD4A20DC58951C5264BA454C89B5DDA43B493AECA67F9E95539C151409` | 13,578,752 B |

### v2.7.4（ソース級 SOT／セキュリティ監査・修正：2026-08-09 検査／未出荷）— ソース合格・現物依存は保留

v2.7.4 はシフトレフトの監査・修正サイクル：design-philosophy §3（設定優先順位）／§4.2（実装は正確に一箇所）と security-design §0.1〜§6 に対しソースを1行ずつ突き合わせ、その修正を行ったもの——出荷バイナリを公表原則に一致させる工程。**検証：`cargo fmt` クリーン・`cargo clippy`(stable) `-D` クリーン・`cargo test` ライブラリ264＋統合10 合格。**

**是正済み（違反 → 修正）：**

| ID | 原則 | 所見 | 修正（根拠） |
| :--- | :--- | :--- | :--- |
| A | §3／§4.2 | 設定デフォルトが6〜8箇所に重複し**13件の確定ドリフト**——実バグ：素のCLIで `llm_max_output_tokens` が2048にフォールバックし出力打切り／`weight_basic` が `--init` 実施の有無で1.0/2.0 と食い違い（総合スコアが利用者依存） | `Config::default()` を唯一源化。clap `default_value_t`・`build_config_with_value_sources`・`setup` テンプレ・`ac_*`・`run_config_json_cli`・`xoksa.env.sample` を全て派生に。散在 `const` を撤廃。Ollama `num_ctx`/`num_predict`/`seed` を非Option化（`llm.rs` の `DEFAULT_OLLAMA_*` 撤廃）。ガードテスト：`xoksa_env_sample_values_match_config_default`・`bare_build_config_scalar_defaults_match_config_default` |
| B | §0.1／§2 | クラスAキーを `/llm net`（OpenAI/Gemini/Claude）と LINE Bearer でリクエスト前に所有 `String` へ複製／`ac_opt_key` が serde 由来の元キーを未 zeroize で drop／`load_env_map` が `NOTIFY_*_SECRET` を除外せず | ヘッダ値を借用 `&str` に（OpenAI の `Bearer …` は `Zeroizing` バッファで構築、LINE は `reqwest` `bearer_auth`）＝残るは許容の reqwest `HeaderValue` コピーのみ。`ac_opt_key` は元バッファを zeroize。`load_env_map` は `NOTIFY_<n>_SECRET` を除外（多重防御） |
| D | §5／言語同期 | §5 が「高エントロピー埋め込み（`include_bytes!`）なし」と断言——現物は WASM UI を `include_dir!`、セットアップPNGを `include_bytes!` で埋め込んでおり事実と矛盾 | これらは `.rsrc`（アイコン）節でなく `.rdata` の正規資源と訂正（JA/EN）。§0.1 のキー例外を J-Quants から HTTPヘッダー送信境界（全クラスAキー）に一般化 |
| C | §4.2 | 重複ロジック | 全監査グループを解消（表下の注記参照。18件を単一ソース化／#19 は別実装＝重複ではないと確認し据え置き） |

**監査で合格（違反なし＝“核”は看板どおり）：** SOT数値完全性（`TechnicalDataGuard`：全 `entry.*` 書込は `technical/types.rs` のセッター経由・バイパスなし、総合値は `composite.rs` の1箇所で算出）／**f64統一・計算経路に丸めやキャストなし**（桁区切り関数は表示専用の `f64 → String`、SOTの `f64` は不変で計算に戻らない）／CLI=GUI=Web 単一コード（`api_data_equals_cli_json_row` テスト）／全プロバイダの LLM 出力整合ガード／§4 サーバ制御（セッション分離・Origin/CSRF・`Json<T>` ゲート・nonce strict CSP・503 受入上限・per-session ロック）／境界サニタイズ／パス解決／通知ホスト許可リスト（anti-SSRF）／`#![forbid(unsafe_code)]` かつ engine に `std::process::Command` なし／到達可能性ベースの `unwrap`/`expect` 方針。

**§4.2 の重複——解消（全て挙動保存）。** 単一ソース化：#1 スコア調整行（10コピー→1 `adjusted_score_line`）／#3 `send_cloud_prompt`（3送信→1）／#5 `key_present_in_env_file`→`read_key_from_env_file`／#6 スコア不明ラベルを「情報なし/unavailable」に統一（utils を単一源）／#7 `colored_price_diff`／#8 `cloud_provider_rows`／#10 `send_chat_turn_dispatch`／#2 `group_thousands`（f64→整数変換は各呼出側が保持・表示専用）／#4 `parse_env_line`（＋`live_ollama_instances` は `load_env_map` を利用。除外条件は各自保持でクラスA値を材料化しない）／#9 `sanitize_symbol_result`（エラーを返し4つの報告サイトの文言を保持）／#11 演算子記号を `rule_operator_catalog` から／#12 `fundamental_value_lines`（revenue〜次期FYの値行。表示/LLMのヘッダは別ビューで各関数に残置）／#13 `cross_status`/`CrossLabels`／#14 `deviation_line`／#15 `symbol_of` 再利用／#16 `ChatTokenUsage::accumulate`／#17 `on_off`／#18 context.rs の前足比を `displayed_price_diff` 経由に。**真の重複でないと確認し据え置き：** #19——`fmt_num` と `fmt_currency_auto` は出力が異なる（記号なし4桁 vs 記号+カンマ2桁）。共有は `100.0` 閾値のみで、これは §1 の出力整合ガードも使うため、統合すると表示精度とセキュリティ閾値を誤って結合してしまう。別概念の下位部分も据え置き（非重複）：#11 の `parse_alert_condition` の演算子解析順・eval のセマンティクス、#18 の生バー(indicators.rs)・リロード差分(chat)の前足比計算。

**現物依存項目は保留（出荷時に本節へ記録・C.4 の慣例通り）：** 現物別 SHA-256・出荷ハッシュへの VirusTotal（v2.7.0 同様、コード署名まで延期）・OWASP ZAP パッシブスキャン・OS別 SBOM 再生成は、tag→CI が凍結した現物に対して実測する。本記録は作業ツリー時点のソース級証跡であり、現物実測の代替ではない。

### v2.9.10（macOS）— CI ビルド現物を 2026-10-02 検査・署名・公証 — 合格（静的／SOT／ランタイム／ペネトレ／ペイロード同一性／SCA／SBOM／署名・公証／VirusTotal／ZAP）。**2026-10-03 07:09 JST 公開**（タグ `v2.9.10` は `b3ef8e1`）

2.9.10 の macOS 側。Windows 側はまだビルドしていない。引継ぎ書 §1.3 は両 OS を同じ `main` から作ることを求めるので、この版の Windows ビルドも以下に記すコミットを使うこと。

**ビルドの出所 — GitHub Actions `Release build (macOS)` run `36981982521`、ワークフロー `.github/workflows/release-macos.yml`、ランナー `macos-14`、ソースコミット `ce4c589`（`main`、#181 の squash マージ。#180 のリリース作業を内包する）。** ツールチェーンは `rust-toolchain.toml` の固定（1.98.1）。所要 7 分。**ビルドログにコンパイラ警告は 0 件**。`warning:` で始まる唯一の行は `proc-macro-error2 v2.0.1` に対する cargo の将来非互換通知で、これは `[patch.crates-io]` が WASM を変えてしまうため v2.9.9 で vendor 修正を意図的に取り下げた、あの依存である（下記の記録）。

**これは 2.9.10 の 2 組目の現物であり、1 組目を破棄した理由は記しておく価値がある。** 1 回目のビルド（`73319c3`・run `36871609030`）は署名・公証・検査まで済ませたが、その検査の静的ゲートで `an_ollama_response_within_the_cap_is_read_whole` が落ちた——この版で追加した試験そのものである。実測で 30 回中 1 回。原因は試験側のループバックサーバがリクエストを読まずに応答していたことで、閉じるときに RST が応答を破棄し得る。製品側には何も無い。検査済みの現物を公開して試験だけ後から直す選択もあり得たが、それこそ §C.1 の規則 5 が禁じていることである——現物は 1 つのソースに束縛され、動いたソースは別のソースだからである。したがって修正を取り込み（#181）、ビルド以降をすべてやり直した。

**作り直しは、その規則が守ろうとしていた主張を実証した。** run `36981982521` のエンジン・デスクトップシェル・設定アプリ・WASM は、run `36871609030` のものと**バイト単位で同一**である（いずれも `54f9ac1d…`・`d9c68bb4…`・`ea29e6f6…`・`4aa431b1…`）。`#[cfg(test)]` のコードは release バイナリに入らないためである。違うのは時刻を含む `.tar.gz` の外装だけ。これで 2 つが推定ではなく実測になった——試験の修正は利用者が受け取るものを何も変えないこと、そして**本プロジェクトの macOS CI ビルドは、同じ実効ソースなら run をまたいで再現する**こと。後者は §C.1 がこれまで WASM について 1 台のマシン上でしか主張していなかった性質である。

**凍結した macOS 現物**（署名・公証・staple 済み。署名前に CI 公開の `SHA256SUMS` と全件照合——5/5 一致）：

| 成果物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| `XOKSA-2.9.10-arm64.dmg`（配布物） | `b01570387a5b7e96ccc4fbac6f53c678f732fd8edb151255b6d14bc5a29626e1` | 11,716,532 B |
| `XOKSA.app/Contents/MacOS/xoksa`（エンジン） | `ecc4a49d79f0a17618133ff1d8ed5d1f143ed1e27909499bbea887d49c72715f` | 12,014,544 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop`（シェル） | `44482043031dde209aa0410870594f299f25860ab9ca1d5f0fa310c98d73ea17` | 6,346,000 B |
| `XOKSA.app/Contents/MacOS/xoksa-setup`（設定アプリ） | `88f4f3cc9b58eac5f675a0d4a0b09ec5890078d7c3135338ca3b9679499af4db` | 5,914,400 B |
| `xoksa-webui-2fc5f86e0258a0ca_bg.wasm`（エンジンに埋め込むフロントエンド） | `4aa431b1ee5d84067b7345d62e8859cae037ad8cec3e3e2c1f48fe1209a782b0` | 710,967 B |

**署名の経路 — 入力は CI 現物であり、作り直しではない。** 内側のバイナリ 3 本とバンドルを `Developer ID Application: Kohzoh Tsuchiya (463YK5WF6H)` で hardened runtime ＋セキュアタイムスタンプ付きで署名し、公証（提出 `c0adf8b8-2c15-4604-bf8e-1037609f9cc4`・**Accepted**）して staple。DMG は staple 済みの `.app` から `/Applications` シンボリックリンクつきでローカルに作成し、同様に署名・公証（`0b5ce636-c3a1-40f2-8e84-c6af50ee6854`・**Accepted**）・staple した。Gatekeeper は app・DMG とも **`source=Notarized Developer ID`** を報告し、リポジトリの `dist/` へコピーした後も DMG は同じ判定を保つ。

**署名の段で 2 つに阻まれた。どちらも再発するので記す。** Apple の公証サービスは `notarytool history` を含む全呼び出しに **HTTP 403「A required agreement is missing or has expired」**を返し続け、開発者アカウントで Apple Developer Program License Agreement に同意し直すまで解消しなかった。資格情報も証明書も有効なまま（証明書は 2031-07-12 まで）で、手元の情報では切り分けられない。もう 1 つ、`codesign` がバンドルを **「resource fork, Finder information, or similar detritus not allowed」**で拒否した。作業コピーに `com.apple.FinderInfo` と `com.apple.fileprovider.fpfs#P` が付いていたためで、原因はリポジトリが iCloud 管理下の `~/Documents` にあること（引継ぎ書 §3.1）。したがって**署名は iCloud の外で行う**——同期対象外のディレクトリで展開・署名・公証・staple まで済ませ、完成した DMG と staple 済み `.app` の zip だけを `dist/` に戻す。その場で属性を消すのは対策にならない（ファイルプロバイダが付け直す）。

**ペイロード同一性——差は 1 バイトで、それは署名が動かさざるを得ないバイトである。** 署名を除去すると、各バイナリは CI 原本と**ちょうど 1 バイト**だけ違う。`__LINKEDIT` の `vmsize` が署名を収めるために 16 KiB 1 ページぶん増えた箇所である（エンジン `0x28000`→`0x2c000`、シェル `0x18000`→`0x1c000`、設定アプリ `0x14000`→`0x18000`）。サイズは不変で、コードとデータは CI が作ったものとバイト単位で同一。v2.9.9 と同じ結果だが、推定ではなく再度実測した。

**静的 — 合格。** `ce4c589` で `scripts/check.sh`：15 ゲート緑・**試験 491 件**・失敗 0（エンジン・WASM フロントエンド・デスクトップシェル・`xoksa-paths` の `cargo fmt --check`／`cargo clippy -D warnings`／`cargo test`、および両 workspace の `cargo audit`・`cargo deny`・`cargo machete`）。**緑であることが全てではない。** 上で述べた試験の揺れを捕まえたのは、`73319c3` で回した同じゲートであり、しかもその日の 2 回目の実行で初めて落ちた。3% で揺れる試験をたいてい通してしまうゲートが報告しているのは「よくある結果」であって「欠陥が無いこと」ではない。だからこそ、単に「失敗 0」と書かず、失敗した事実と 30 回の実測と修正を記録する。

**SOT — 合格。** csv == json == api の出力等価（AAPL：`latest_observed_price` 330.32・`final_score` 2.0 が `--save-technical-log --stdout-log --log-format csv`／`--log-format json`／`/api/symbol/AAPL/summary` の 3 経路で一致。指標ごとの 9 スコアも一致）。

**ランタイム — 合格・9/9。** 署名済みエンジンに対する `scripts/inspect.sh`：埋め込み UI 200／CSP ヘッダあり／クロスオリジン POST 403／過大 body 413／`text/plain` POST 415／同一オリジン POST 200／CLI 分析がレポートを生成／バックテスト API が結果を返す／ビルド元を名乗らない。*後から読む人への注記：項目 6 は `spec_json: "{}"` を送るが、2.9.10 はこれを検証層で拒否する。HTTP ステータスは 200 のまま（ハンドラが `{"ok":false}` を返す）なので、この項目が測っている「同一オリジンのリクエストが受理されるか」は従来どおり測れており、保存の成功を測っているわけではない。*

**フロントエンドの束縛 — 合格。** `webui-leptos/dist` の無いディレクトリからエンジンを起動し、`/` は `xoksa-webui-2fc5f86e0258a0ca` を参照した。配信された `.wasm` のハッシュは `4aa431b1…` で現物の WASM と同一。検査は作業ツリーではなく現物に束縛されている。

**ペネトレーション — 測定した項目はすべて合格。1 項目は再現できていない。** パストラバーサル：4 通りの表記すべて 404。メソッド上書きと誤ったメソッド：422／405。`/api/config` に秘密は無い（UI 語彙・既定値・版数のみ。鍵の形をした文字列は 0 件）。クロスオリジンの SSE：403。セッション分離：2 クライアントが別々の 32 桁 hex `xoksa_sid` を受け取る。`--private`：`logs/` を作らず `~/.xoksa_history` も不変で、エンジン自身が `Diagnostics log: disabled (private mode)` と表示する。**再現できていないのは容量超過の shed（K3）。** `/api/health` へ 300 同時でも全件 `200` で、`503` は観測できない——ハンドラの応答が速すぎて 256 件が同時に滞留しないためである。上限は存在し、いまは 256（従来は `u16::MAX`。所見 B.3 と §4 の設計記述を参照）。shed 自体は開発中に上限を 2 に下げたビルドで実測した。リリース時点での実測には遅いエンドポイントが要るため、ここでは主張しない。

**SCA — 合格。** 同一ソースの使い捨て `cargo auditable` ビルドに対する `cargo audit bin`：**埋め込み依存 245 件・脆弱性 0**。`osv-scanner --lockfile Cargo.lock`：338 パッケージ・**問題なし**。`cargo audit` と `cargo deny`（advisories・ライセンス・ソース）は上記の静的ゲートで両 workspace とも緑。

**SBOM — 再生成したが、インベントリは動いていない。** `sbom/xoksa-native-macos-arm64.cdx.json`（**library 221 件**）と `sbom/xoksa-webui-leptos.cdx.json`（**188 件**）。いずれも `syft` 1.46.0 による CycloneDX 1.5 で、ネイティブ側は中立ディレクトリに置いた使い捨ての `cargo auditable` ビルドから読んだ（出荷バイナリには auditable データが無いので、そこからは読めない）。2.9.9 のインベントリとコンポーネント単位で比較したところ、差は**本プロジェクト自身のクレート版数だけ**（`xoksa` 2.9.9 → 2.9.10、`xoksa-webui` も同様）で、この版で依存の追加・削除・変更は 1 件も無い。Windows のインベントリは触っていない——公開済みの 2.9.9 Windows 現物に対応するものであり、Windows で 2.9.10 をビルドしたときに再生成する。

**VirusTotal — 出荷 4 現物すべて 0 検知**（2026-10-02・操作者が提出）：DMG `b01570387a…` **0/61**、エンジン `ecc4a49d…` **0/63**、デスクトップシェル `44482043…` **0/62**、設定アプリ `88f4f3cc…` **0/63**。いずれも署名済みの現物として提出し、Mach-O 3 本は VirusTotal 側でも `signed` と表示される。多エンジンが clean であることは監査ではない——このハッシュに束縛された 1 つの実測値である。

**OWASP ZAP パッシブスキャン — 8 種、すべて受容**（2026-10-02・別ホストから LAN 越しに `--host 0.0.0.0` へ。トークン必須で動作）。うち 2 件はログイン経路から出たもので、過去のスキャンが到達していなかった範囲である。いずれも所見ではなく設計どおりの挙動である。

- **`/login` に Anti-CSRF トークンが無い。** 防御はフォームのトークンではなく **256bit のアクセストークンそのもの**で、それを持たないクロスサイト POST は拒否される。ここで実測した——誤ったトークンでのクロスオリジン `POST /auth` は `401` を返し、`xoksa_auth` cookie を発行しない（出るのはセッション識別子だけで、認証ではない）。この経路をサーバ側 Origin 検査から意図的に外しているのは、`Referrer-Policy: no-referrer` により正規の同一オリジンのフォーム POST でも `Origin: null` になり、検査すると正規のログインが壊れるためで、除外とその理由はコードに明記してある。
- **`POST /auth` の Cookie ポイズニング。** ZAP はリクエストのパラメータが cookie に入ることを指摘する。これは設計した流れそのもので——cookie はトークン自体を運ぶ。これは [security-design.md §0.2](./security-design.md) に記録した範囲の決定である——値は定数時間で比較されるため、クライアントが選んだ値は単に通らない。

残る 6 件は v2.9.9 と同じ読みである：SRI（クロスオリジンの資産が無い）、Unix タイムスタンプの露見（分析データそのもの）、セッション管理レスポンスの特定と「モダン Web アプリケーション」（情報レベル）、`localStorage`（UI の状態のみ。鍵はサーバ側）、生成された wasm-bindgen の JavaScript 中の疑わしいコメント。v2.9.9 と同様、`/api/chat/stream`（SSE）はプロキシ経由でスキャンできないため、その制御は直接測った（クロスオリジン `403`／過大 `413`／同一オリジン `200`）。

**2026-10-03 07:09 JST 公開**（タグ `v2.9.10` は `b3ef8e1`）。

### v2.9.10（Windows）— CI ビルド現物を 2026-10-02 検査・未署名 — 合格（静的／SOT／ランタイム／ペネトレ。**入場上限のシェッドを初めて実測**／ペイロード同一性／ZAP／SCA／SBOM）。VirusTotal は署名まで保留。**2026-10-03 07:09 JST 公開**（タグ `v2.9.10` は `b3ef8e1`）

2.9.10 の Windows 側であり、2.x 系列の最後の検査である。

**本リリースの 2 つの現物は別のコミットに束縛されている。これを曖昧にせず明記する。** macOS は `ce4c589`、Windows は同じ `main` の 2 コミット先 `ba7f397` からビルドした。§C.1 規定 5 は「現物は 1 つのソースに束縛され、動いたソースは別のソースである」と定める——したがって問うべきはプログラムが動いたかである。動いていない。`git diff ce4c589 ba7f397` を `src/`・`webui-leptos/src/`・`xoksa-desktop/src/`・`xoksa-setup/src/`・`xoksa-paths/`・すべての `Cargo.toml` と `Cargo.lock`・`tauri.conf.json` に対して取ると**空**である。差分の全体は**文書 2 ファイル・326 行追加・削除 0**——上記 macOS 記録の 84 行と、チャット通知チャンネルのために `docs/manual/setup.md` が得た 242 行（#183）である。マニュアルは Windows ビルドの**前**にツリーへ入っていなければならない。出荷するマニュアル zip を `scripts/make-manual-zip.sh` が `docs/manual/` から作るためである。よって各現物は自分のコミットを名乗り、両者が含むプログラムは同一である。

**ビルド来歴 — GitHub Actions `Release build (Windows)` run `36997241294`、workflow `.github/workflows/release-windows.yml`、runner `windows-latest`、ソースコミット `ba7f397`（`main`）**、起動は `workflow_dispatch`。所要 58 分 36 秒（10:45:32–11:44:08 UTC）。18 ステップすべて緑で、アップロードを止めるはずの `Gate — no identifying build path in any artifact` も含む。**ツールチェーンはビルドログではなく出荷エンジン自身から読み出した：`rustc 1.98.1 (48a229cea)`**——同じコミットハッシュがデスクトップシェルと設定アプリにも入っている——これは `rust-toolchain.toml` の pin と一致する。`trunk 0.21.14` と `tauri-cli 2.11.4` は workflow で pin。未署名。検査時の作業ツリーは同じコミットで clean。

**凍結した Windows 現物 — CI 公開の `SHA256SUMS` と全件照合：5/5 一致。検査終了後にも再照合して無変化。**

| 現物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| `XOKSA-2.9.10-x64.msi`（インストーラ。下の 3 本を内包） | `f86084dcef2f32b384a9fcbe3f357ba4f1336c14f66012c9488d40503b1b50f7` | 11,714,560 B |
| `xoksa.exe`（エンジン） | `0433ea81556738370a91ce60cbba4576b8b519b1c3d69cd6a100b9775455f2c9` | 14,857,216 B |
| `xoksa-desktop.exe`（接続シェル） | `63f865ee4dd8b0f87f1a72866fa2b48cd8c90531410aee5ced56df93dcf6f3b3` | 9,629,696 B |
| `xoksa-setup.exe`（設定アプリ） | `4efe9432755494c1ab744cb94ce0185afa0bcc83cdf153a738e6f8a78c28c29e` | 8,414,720 B |
| `xoksa-webui-22fbb44a3ccaa34a_bg.wasm`（エンジンが内蔵するフロントエンド） | `983596d5fb4502bec51581129b343aa384bb7280b815f83a4128576e9061a043` | 724,577 B |

**この WASM は macOS が出荷したものではない。§C.3 の決定性の主張には、この境界が要る。** macOS の 2.9.10 フロントエンドは `xoksa-webui-2fc5f86e0258a0ca`／`4aa431b1…`／710,967 B、こちらは `xoksa-webui-22fbb44a3ccaa34a`／`983596d5…`／724,577 B——同じソース・同じ `webui-leptos/Cargo.lock` で、バイトが違う。§C.3 は WASM を決定的と呼び、macOS の記録は**同一 runner の run 間**で再現することを実測した。どちらも**OS をまたいで**再現するとは述べておらず、実際しない。将来の読み手が一方のハッシュを他方への矛盾と読まないために記録する——依存集合がホスト非依存だから WASM のインベントリは 1 本であり、ビルドがそうでないから WASM のバイナリは 2 本ある。

**自分のビルド者を名乗る現物は無い。** `scripts/check-no-host-paths.sh` は 5 本すべてで通過。デスクトップシェル・設定アプリ・WASM・MSI はいかなるユーザーパスも**0 件**。エンジンは **142 件**で、いずれも GitHub ホストランナー自身の組み込みアカウント——`C:\Users\runneradmin`（97）とその 8.3 短縮形 `C:\Users\RUNNER~1`（45。`aws-lc-sys` の C ビルド由来）——であり、誰も特定しない。件数まで v2.9.9 と同一。

**ペイロード同一性 — MSI を展開して実測。v2.9.9 を正確に再現した。** CAB は 3 ファイルを持つ（`Bin_xoksa.exe`・`Bin_xoksa_setup.exe`、およびデスクトップシェル。7-Zip はこれを `Path` という名で一覧する）。エンジンと設定アプリは単体現物と**バイト一致**。デスクトップシェルは **`0x77190f`〜`0x771911` のちょうど 3 バイト**だけ違い、そこは Tauri がインストール形態を刻む箇所（`__TAURI_BUNDLE_TYPE_VAR_UNK` → `…_MSI`）である——v2.9.8・v2.9.9 と同じ 3 バイト・同じオフセットなので、インストーラは他の改変を持たない。6 本すべて（単体 3・展開 3）が `FileVersion` **2.9.10** を報告：sidecar ドリフトは無い。

**AV 誤検知レシピは CI 上でも維持されている。** エンジン `.rsrc` **2,048 B・エントロピー 4.176**、`.rdata` **5,198,848 B・5.887**。デスクトップ `.rsrc` **32,768 B・3.898**、設定アプリ `.rsrc` **32,768 B・3.900**。いずれも v2.9.9 の値と誤差 0.02 以内で、挙動 ML エンジンが採点する軸は動いていない。（エンジンの `.text` は 9,227,776 B・6.291、`.pdata` は 321,536 B・6.564。エントロピーの見出しがどのセクションを指すのか次に迷う人のために書いておく——`.rsrc` と `.rdata` であって、コードではない。）

**静的 — 合格。** `ba7f397` で `scripts/check.sh`：**15 ゲート緑・491 テスト・失敗 0**（エンジン 473＋WASM フロントエンド 11＋デスクトップシェル 6＋`xoksa-paths` 1）。作業ツリーは clean で、これが §C.1 規定 1 である。

**SOT — 合格。今回は全列の行き先を明示した。** CSV ログは位置指定なので、ヘッダ（`--show-log-header`）で 51 列すべてを JSON ログのキーと `/api/symbol/NVDA/summary` へ戻した：**3 つのキー集合は構成要素として完全一致（51・51・51）。49 列が 3 経路で一致し、不一致は 0 列。** 比較しない 2 列は `analyzed_at` と `analyzed_at_timestamp` で、これは*実行した時刻*であって計算値ではない。3 回の呼び出しの間隔ぶんだけ違っており（08:58・08:58・09:00）、これは正しい挙動である。3 経路は同じ足 `2026-10-01`・同じ銘柄を名乗る。3 経路で同一：`bar_close` **230.86000061035156**、`rsi` **66.52194618…**、`macd` **3.140415954…**、`signal` **2.559756341…**、`adx` **16.0682252…**、`ema_short` **228.3516487…**、`ema_long` **223.9319608…**、`bb_upper` **235.332715…**、`bb_lower` **211.6892852…**、`latest_volume` **98,369,800**、`final_score` **7**。

*SOT の数値の読み方について 1 点。これはスナップショットである。2 時間後、米国市場が開いてから走らせた CLI 分析は、形成中の 2026-10-02 足で RSI 74.78 を報告した。測っているのは「ある一瞬において 3 経路が互いに一致すること」であり、後の実行と一致することは求めていない。*

**フロントエンドの束縛 — 合格。** エンジンは `dist/2.9.10-windows-x86_64/`——`webui-leptos/dist` が存在しないディレクトリ——から起動したので、`read_asset` が作業ツリーのファイルを埋め込みより優先することはあり得ない。`/` は `xoksa-webui-22fbb44a3ccaa34a` を参照し、エンジンが配信した `.wasm` のハッシュは `983596d5…`——現物自身の WASM と一致する。ページの参照・配信されたバイト・凍結ファイルの 3 つが 1 つの値になる。

**ランタイム — 合格、9/9。** 凍結エンジンに対する `scripts/inspect.sh`：内蔵 UI 200、CSP ヘッダあり、クロスオリジン POST 403、過大 body 413、`text/plain` POST 415、同一オリジン POST 200、CLI 分析がレポートを出す（3,650 B・99 行・exit 0・標準エラーは空）、backtest API が結果を返す、識別可能なビルドパス無し。

**ブラウザ描画 — 要素を数えるだけでなく、素の HTML と差を取って測った。** ヘッドレス Edge は WASM アプリをマウントした：エンジンが配信するページは **1,480 B・`div` 0 個**、描画後の DOM は **5,392 B・`div` 16／button 7／section 5／select 3／input 4／label 2、加えて form・textarea・svg** である。この 2 つの差こそが、UI をサーバが送ったのではなく WASM が組み立てたことを示す。DOM 内の `<script>` は 1 つ、nonce 付きのモジュール bootstrap のみ。

**ペネトレ — 凍結エンジン `0433ea81…` に対する §C.2 マトリクス。K3 はもう未実測ではない。**

- **G1／K3 — 入場上限のシェッドを、リリース時として初めて実測した。** macOS の 2.9.10 記録は `503` を示せなかったことを明記している：`/api/health` へ 300 並行しても全部 200 で、ハンドラが速すぎて 256 本が同時に飛ばないためである。ここでも 2 回、同じ形で失敗した。その理由は記録に値する——**`curl --limit-rate` は body だけでなくリクエスト*ヘッダ*も絞る**ので、TCP 接続は 512 本張れていたのに `/api/health` は 0.7 ms で応答した。ヘッダが届いていないリクエストはまだリクエストではなく、permit を取らないからである。半端に送られた 512 本を平然と受け流すこと自体は結果だが、シェッドではない。シェッドに必要なのは**受け入れ済みで待っている**リクエストである：ヘッダを送り切り、`Content-Length` を宣言し、body を保留する。raw socket で行うと数値は厳密に出た——**保留 192 本では `/api/health` は 200、256 本では 503（`server at capacity`）を返し、`/` と `/api/config` も同様。半分を解放すると 200 に戻り、全部解放すれば完全に復帰する。** したがって `MAX_CONCURRENT_CONNECTIONS = 256` は実際の上限であり、シェッドはキューではなく即時であり、ラッチもしない。
- **K1 — クライアント別分離を双方向で実測。** 2 つのブラウザは異なる 32 桁 hex の `xoksa_sid`（`HttpOnly`・`SameSite=Strict`）を受け取る。クライアント A が `/api/llm/select` で `gemini`、B が `claude` を選択したのち、`/api/llm/options` は A=`gemini`・B=`claude` を報告した——どちらも相手の選択を見ることも奪うこともできない。全桁ゼロの偽造 `sid` は、誰かのセッションに触れるのではなく自分用の空の名前空間を得るだけである。
- **K2 — 遅いチャット 1 ターンは何も止めない。** 実機のローカルモデル（Ollama・`qwen3:8b`）のターンをクライアント A で少なくとも 14 秒ストリームさせ、その間に B 側で実測：`/api/symbol/NVDA/summary` **1.5 ms**、`/api/health` **0.6 ms**、`/api/config` **1.6 ms**。
- **A1〜A4 — SSRF／スキーム・パス注入。** 細工した 8 種のシンボル（`http://evil.example/x`・`//evil.example/a`・`NVDA/../../etc/passwd`・`NVDA@evil.example`・`NVDA#@evil`・`file:///c:/windows/win.ini`・`NVDA?x=1`・`NVDA%0d%0aHost:evil`）は、外向きリクエストが組み立てられる前にすべて許可文字検査で拒否された。CRLF のペイロードは **JSON エスケープ**された形（`\r\n` の 2 文字）で返るのでヘッダ注入はできず、またその反射は `application/json`＋`nosniff` で配信されるので描画もされない。
- **B1〜B5 — 不正入力。** 2,000 段ネストした JSON → **422**、`NaN` → **400**、`1e400` → **400**、文字列内の NUL → **400**。panic も hang も無い。未知の `timeframe`（`../../etc`）と不合理な `period`（`99999y`）は**無視**され、バックテストは足の既定へフォールバックする——正当なリクエストと比較して実測：`timeframe: "daily"`・`bars: 64` が同一。OS メタ文字を含むシンボル（`NVDA|calc.exe & whoami`）は `note: "invalid symbol"` で拒否。
- **C1〜C2 — パストラバーサル。** 8 種のエンコードすべて **404**：`../../../../Windows/win.ini`・`..%2f…`・`....//....//…`・`/C:/Windows/win.ini`・`%2e%2e%5c…`、加えて `.git/config`・`xoksa.env`・`../xoksa.exe`。
- **D1〜D5 — リクエストポリシー。** `text/plain`・`application/x-www-form-urlencoded`・`Content-Type` 無しは、`/api/backtest`・`/api/llm/select`・`/api/alerts`・`/api/analysis/multi-timeframe`・`/api/backtest/rules` のいずれでも **415**。クロスオリジンの JSON POST はすべてで **403**、SSE の `GET /api/chat/stream` も **403**——`Origin` でも `Referer` でも。6 MB の body は **413**（上限は 64 KiB）、70 KB のルールも **413**。GET への `X-HTTP-Method-Override: POST` は無視され何も変わらない（アラート一覧は前後でバイト一致）。`PUT`・`PATCH` は **405**。**D5＝保存ルールの件数上限：** 210 本投入して**ちょうど 200 本保持**、最古（`rule1`）が evict され最新（`rule210`）が残った。`../../../evil` という名のルールは唯一の固定ファイルの中の*フィールド値*として保存され、他のどこにもファイルを作らない。測定は `HOME` を隔離して行い、操作者自身の保存ルール 4 本には一切触れていない——無変化を確認済み（200 B・4 件）。
- **E1〜E5 — ブラウザ攻撃面。** 反射 XSS の試験（シンボルとして `<img src=x onerror=alert(1)>`）は `application/json`＋`nosniff` の中で返り、そこでマークアップは実行されない。出荷フロントエンドの `inner_html` の注入元はいずれもエスケープ済みである：`analysis_html` は全行を `html_escape` したうえで、数値を 2 要素のリテラル集合から選んだクラスの `<span>` で包むだけ。`chart_doc_html` はシンボルと足のラベルをエスケープし、その理由をコメントに書いている。**`postMessage` は出荷現物にまったく出現しない**——JS glue に 0 件、WASM に 0 件——ので、所見 E4 の撤去はソースの性質ではなくバイナリの性質である。ニュース URL は `safe_http_url` が受理した場合にだけ `href` になり、それ以外はテキストとして `<span>` に描かれ、リンクには `rel="noopener noreferrer"` が付く。CSP は nonce ベースで `'unsafe-inline'` を含まない：`default-src 'self'`・`script-src 'self' 'wasm-unsafe-eval' 'nonce-…'`・`style-src 'self'`・`img-src 'self' data:`・`connect-src 'self'`・`object-src`／`base-uri`／`frame-ancestors 'none'`・`form-action 'self'`。加えて `X-Frame-Options: DENY`・`nosniff`・`Referrer-Policy: no-referrer`、および 8 機能を拒否する Permissions-Policy。
- **F1〜F4 — 漏洩。** 未知シンボル・不正な足・存在しない経路からのエラー応答に、絶対パス・`.rs` の位置・panic 文言・アカウント名は**いずれも無い**。7 エンドポイントを鍵らしい文字列（`sk-`・`AIza`・`sk-ant-`・`xoxb-`・Slack の `/services/` パス・LINE の push パス・長い `Bearer`）で走査して**0 件**。`/api/config` は「有無のみ」より強い：**鍵・secret・token のフィールドが 1 つも無く**、存在フラグさえ無い——そこにある `"key"` は指標名と演算子の語彙である。**`--private` は何も書かなかった：** `serve --private` のセッションを銘柄サマリ・バックテスト・言語変更で動かしたうえで、正準の設定ディレクトリはバイト単位で無変化——言語の書き込みは行われるのではなく*拒否*される（§0.1 の要求どおり）——そしてエンジン自身がそう述べる（`Private mode: nothing is saved to disk.`／`Diagnostics log: disabled (private mode)`）。
- **J1 — 非ループバックは fail-closed。現物で実測。** `--host 0.0.0.0` では、有効なトークンが無い限り**すべての**経路が **401** を返す——`/api/health`・`/api/config`・`/` も同じ——誤ったトークンも空の `Bearer` も 401。正しいトークンでは 200。ブラウザの交換は `/auth` へのフォーム POST で、誤トークンは **401**、正しいトークンは **303** で `/` へ、`xoksa_auth` を `HttpOnly; SameSite=Strict; Path=/` として設定する。その cookie では **200**、偽造した 64 桁 hex の cookie では **401**。比較は定数時間（`ct_eq`。長さを先に確認し XOR を累積）であり、**起動ログはトークンを出力しない**——ログに 40 文字以上の英数字連続は 0 件で、代わりに取得方法を出す（`Show token with \`xoksa serve --show-token\``）。本検査中、トークンはシェル変数に取っただけで一度も表示していない。
- **H1 — プロンプトインジェクション：境界は検証した。ランタイム注入は行っていない。** 制御は `sanitize_news_text`／`sanitize_news_url` で、タイトルを 1 行に潰し、プロンプトの*構造*マーカー（改行・`===`・`【】`・コードフェンス・制御文字・長さ上限）を無害化する一方、語はプレーンテキストとして意図的に残す——構造の偽造は出力ガードが検出できないものであり、だからこの境界が除去する。これを 3 本の決定論テストが覆い、上記の静的ゲートで緑だった。**主張しない**のは、現物に対するランタイム注入である：タイトルは Brave から来るので、本検査がその内容を指定することはできない。
- **H2 — LLM の数値改ざん。** 同日に Windows で独立に再実測した。このエンジンから読み直した確定値に対して、受入集合は **17/17**（反転 10 件・回帰 5 件・調査記録 §9.2 により対象外 2 件）。台と所見は調査フォルダにあり、規則そのものは security-design §1 に規範として置かれている。
- **I1〜I2 — 下の SCA を参照。**

**暗黙の `./xoksa.env` は無い — ログの 1 行を読むのではなく、挙動で測った。** 起動ディレクトリにエンジンの複製と並べてデコイの `xoksa.env`（`SELL_RSI=99`・`LANG=en`・`STANCE=decoy`）を置いた。得られた分析は `SELL_RSI=70` と `stance: Holder`——正準側の値——を使い、RSI 74.78 を*買われすぎ*と報告した。これは閾値が 70 のときにだけ起こり、99 では起こらない。正準ファイルは無変化（mtime 2026-09-27・デコイの文字列は 0 件）なので、一度きりの移行も発動していない。§6 の単一設定契約は現物の上で成立している。

**OWASP ZAP パッシブスキャン（`0.0.0.0:8787` に bind した凍結エンジン `0433ea81…` に対し LAN 経由）— 8 種・17 件・High 0、いずれも欠陥ではない。** カテゴリと判定は v2.9.8・v2.9.9 と同一である。**この実行はこの現物に束縛されている：** アラートは `xoksa-webui-22fbb44a3c…`——このエンジンが内蔵し配信するフロントエンド——を名指ししており、エンジンは `webui-leptos/dist` が無いディレクトリから起動した。以下の各項目は v2.9.9 の文面を写したものではなく、ここで測り直した値である。

| アラート | 件数 | 実測 |
| :--- | --: | :--- |
| Anti-CSRF トークンが使用されていない（`GET /login`） | 1 | 意図的。下記参照 |
| Sub Resource Integrity 属性が無い | 1 | バイナリ URL に HTML 用ルールを適用したもの |
| タイムスタンプの露見 — Unix（`/api/symbol/NVDA/summary`） | 3 | 市場・分析の時刻 |
| Cookie ポイズニング（`POST /auth`・`token`） | 1 | 記録済みの設計判断を正しく観測したもの |
| セッション管理レスポンスの特定 | 3 | 正しい検出 |
| モダン Web アプリケーション（`GET /`） | 1 | 正しい検出 |
| 情報漏洩 — ブラウザ内の localStorage（`GET /`） | 6 | 表示状態のみ |
| 情報漏洩 — 疑わしいコメント | 1 | wasm-bindgen 自身の生成出力 |

- **`GET /login` の Anti-CSRF トークン欠如（Medium・信頼度 Low）** — 従来どおりで意図的である。ZAP が挙げる証拠は `<form method="post" action="/auth">` であり、`/auth` は Origin 検査を免除した唯一の POST（理由は `src/server/mod.rs` に明記）で、代わりに 256bit のトークン自体で守られている。本検査中にこのエンジンで実測：誤ったトークンでの `/auth` への POST は **401**、空のトークンも **401**、正しいトークンだけが **303** を返す。提出される秘密が**認証情報そのもの**である以上、CSRF トークンを足してもクロスサイトの攻撃者が当てられない対象が増えるだけで、何も変わらない。
- **SRI 属性が無い — アラートの URL は HTML 文書ではなくフロントエンドの資産である。** このエンジンが実際に配信する HTML（1,480 B）は、**3 つのサブリソースすべて**に `integrity="sha384-…"` を付けている——スタイルシート・JS の `modulepreload`・WASM の `preload`——実測で `integrity` 属性 3 個／サブリソース参照 3 個。ZAP はバイナリの内容に HTML のパッシブルールを当てており、これは v2.9.9 と同じで、そこでは証拠が WASM のデータセクションにコンパイルされた Rust の `format!` テンプレートに辿られている。
- **タイムスタンプの露見（Low・3 件）** — 実測：`bar_timestamp` 1790947800（2026-10-02 13:30:00 UTC）、`market_data_latest_timestamp` 1790970171（19:42:51 UTC）、`analyzed_at_timestamp` 1790970172（19:42:52 UTC）。市場と分析の時刻であり §0.1 のクラス C、かつ §1 の要求（読み取り値には算出した足が伴う）そのものである。ホストに関する情報は出ていない。
- **`POST /auth` の Cookie ポイズニング（Info）** — `token` パラメータが cookie に到達するという ZAP の観測は正しい。実測：交換に成功すると **303** で `/` へ飛び、`xoksa_auth=<トークンそのもの>` と `xoksa_sid=<32 桁のランダム hex>` を設定する。どちらも `HttpOnly`・`SameSite=Strict` で、**`Max-Age` も `Expires` も持たない**（セッション cookie）。認証 cookie は派生したセッション ID ではなく**認証情報そのもの**であり、これは §0.2 が明示的な範囲の決定として記録している——分離にはセッション表・期限・失効が要るが、得られるのは端末ごとの失効だけである。代償も隠さず記録する：LAN 上の受動的な観測者が 1 リクエストを捕捉すれば、CLI／API も開く主たる認証情報を得る。`--rotate-serve-token` は全ブラウザセッションを一度に失効させる。
- **情報漏洩 — ブラウザ内の localStorage（Info・6 件）** — フロントエンドのソースから 9 つのキーを全列挙した。v2.9.9 の記録が落としていた名前空間の接頭辞が付いている：`xoksa.theme`・`xoksa.font`・`xoksa.symbol`・`xoksa.tickers`・`xoksa.targets`、およびペインの寸法 `xoksa.right_w`・`xoksa.fund_h`・`xoksa.chat_h`・`xoksa.news_h`。表示状態とティッカー記号（クラス C）であり、認証情報もセッション ID も無い——`xoksa_sid` は `HttpOnly` cookie でサーバ側が保持する。
- **疑わしいコメント（Info）** — 配信された glue JS（50,628 B）で実測：**`//` コメント 19 件と `TODO` 1 件**（*"// TODO we could test for more things here, like `Set`s and `Map`s."*）。いずれも wasm-bindgen 自身の生成出力であって xoksa のソースではない。出荷 `.wasm` には **`/*` が 1 つも無く**、`//` の 3 件は URL の中である。
- **セッション管理レスポンスの特定**と**モダン Web アプリケーション**（Info）— `xoksa_sid` cookie と JS 駆動の UI を正しく検出したものであり、所見ではない。

`/api/chat/stream`（SSE）はスキャンの対象外である——プロキシがバッファして 502 になる——その制御は上記で直接検証済みである（クロスオリジンは `Origin` でも `Referer` でも 403、同一オリジンは 200）。

**SCA — 出荷 Windows 現物に exploitable 0。** `cargo audit`：エンジンの 338 クレート依存に対し**脆弱性 0・警告 0**。`cargo audit -f webui-leptos/Cargo.lock`：**脆弱性 0・警告 3**（`paste` と `proc-macro-error2` が unmaintained、`event-listener` 5.4.1 が unsound）。`cargo deny check` の advisories／bans／licenses／sources と `cargo machete` は両 workspace で緑。`osv-scanner` を lockfile ごとに：エンジン **0**（338 パッケージ）、`xoksa-paths` **0**（18）、WASM **3**（188）、`xoksa-desktop` **8**（420）、`xoksa-setup` **8**（446）——後ろ 2 つは実際には 7 件で、`GHSA-wrw7-89jp-8q8g` が `RUSTSEC-2024-0429` の別名である。

出荷 Windows ファイルに何が届くかは、lockfile から推測せず**現物由来**のインベントリで確認した。`glib` 0.18.5（`RUSTSEC-2024-0429`・CVSS 6.9。ここで CVSS を持つ唯一のアドバイザリ）は gtk-rs の Linux バインディングにあり、`glib`・`gtk`・`gdk` は **Windows の 3 本すべてに不在**である。`proc-macro-error` 1.0.4 はビルド時の proc macro で、これも**不在**。実際に出荷に載るのは `unic-*` 0.9.0 の 5 本で、デスクトップシェルと設定アプリにのみ——5 本すべて **unmaintained であり、脆弱性ではなく、CVSS も持たない**。使い捨ての `cargo auditable` ビルドに対する `cargo audit bin` が、実際に埋め込まれた依存集合でこれを裏づける：エンジン **254** 依存／脆弱性 **0**、デスクトップシェル **251**／**0**（unmaintained 警告 5 件）、設定アプリ **267**／**0**（同じ 5 件）。

**`RUSTSEC-2026-0221` は出荷に載る。到達できない理由をこの WASM で実測した。** `event-listener` 5.4.1 は unsound である——`StackSlot` が `!Send` のタグをスレッド境界を越えさせる——そしてビルド時クレートではない：`cargo tree` は leptos → `reactive_graph` → `async-lock` の経路を示し、その文字列は出荷 `.wasm` の中にある。§C.2 の「WASM の所見は配布 WASM 非搭載」という注記は proc-macro 2 件のために書かれたもので、**この件には及ばない**。スレッド境界を越えるにはスレッドが要る：出荷バイナリから解析すると、メモリセクションは 1 エントリで **`limits_flags=0x00`・min 18 ページ・非共有**——macOS の記録が自分の WASM で測ったのと同じ値であり、wasm のスレッド化は共有メモリを前提とする。この現物では到達不能。Leptos が動いたときに再確認する。

**v2.9.9 で記録した結論が誤っていた。実測を示す。** あの記録は `deny.toml` の `RUSTSEC-2026-0173` について「依存が消えており、エントリは stale。削除すれば警告が有用に戻る」と述べていた。stale ではない。1 つの `deny.toml` を 2 つの workspace が共有している：`paste`・`proc-macro-error2`・`event-listener` は **`webui-leptos/Cargo.lock` にしか存在せず**（ルートの `Cargo.lock` には 0 件）、したがって 2 件の `ignore` は **webui workspace では必須で、ルートでは未一致**である——`advisory-not-detected` 警告は、死んだエントリの帰結ではなく、この共有の帰結である。`deny.toml` 本体には触れず、コピーで実測：エントリがあると `cargo deny --manifest-path webui-leptos/Cargo.toml check advisories` は **`advisories ok`**、外すと **`advisories FAILED`** になる。v2.9.9 の推奨に従えばリリースゲートを壊していた。（同じ実行から、`event-listener` の unsound アドバイザリは ignore されていないのにこのゲートを落とさないことも分かる——この設定では落とすのは `unmaintained` であり、`unsound` ではない。）

**SBOM — Windows の 3 本を再生成し、結果を信用する前に比較方法そのものを直した。** `sbom/xoksa-native-windows-x86_64.cdx.json`（library **225** 件）、`sbom/xoksa-desktop-windows-x86_64.cdx.json`（**196**）、`sbom/xoksa-setup-windows-x86_64.cdx.json`（**212**）。いずれも `syft` 1.49.0 による CycloneDX 1.5 で、中立ディレクトリに置いた使い捨ての `cargo auditable` ビルドから読み取ったものであり、出荷バイナリからの抽出では**ない**（CI は `auditable` でビルドしていない）。絶対パス・ユーザー名・`.exe` のいずれも含まない（実測）。公開済み v2.9.9 のインベントリと比べて各々の差は**自クレートの版数のみ**（`xoksa`・`xoksa-desktop`・`xoksa-setup`：2.9.9 → 2.9.10）で、依存の追加・削除・移動は無い。

この比較は最初クレート**名**でキー化して行い、2 つの版で存在するクレートを黙って畳んでいた——225 コンポーネントが 217 名に縮み、重複していた 8 名のいずれかで版が動いても見えなかった。**(名前, 版) の多重集合**で取り直すと件数はインベントリと完全に一致し、重複の集合も前後で同じだった：エンジンでは `windows-sys` が 4 版、Tauri の 2 アプリでは `schemars` が 3 版、等々。結論は生き残ったが、最初の根拠は生き残らなかった。版の移動が見えない比較は、比較ではない。

**次の Windows リリースのための手順上の事実 2 つ。** 第 1 に、`sbom/README.md` は `cp target-sbom/release/xoksa /tmp/sbomwork/xoksa` と書いているが、Windows の Git Bash では**拡張子なしのファイルを作れない**：MSYS は `xoksa` を既存の `xoksa.exe` に解決し、これは `cp` でもシェルのリダイレクトでも同じなので、どちらも黙って違う名前に書く。これが問題になるのは、syft の `pe-binary-package-cataloger` が `.exe` 拡張子で発動し、226 個目のコンポーネント（type `application`・`cpe:2.3:a:xoksa:xoksa:2.9.10` と相対パス `\xoksa.exe` を伴う）を足すためで、そうなるとこのインベントリだけが 5 本の中で `library` のみでなくなる。複製は PowerShell の `Copy-Item` で作る必要がある。第 2 に、`sbom/xoksa-webui-leptos.cdx.json` は現行であり、そのまま残した：これは `webui-leptos/Cargo.lock` から catalogue され、その lockfile の最終変更は `73319c3`（2026-10-01）、インベントリは macOS の検査中に `ce4c589`（2026-10-02）で再生成されている——lockfile より後なので、既にこのツリーを記述している。使い捨てツリー（671 MB＋1.2 GB＋1.2 GB）は削除し、出荷ファイルには触れていない。

**持ち越しの所見 — 2 件ともこのツリーで再検証し、2 件とも不変。**

- **`native-tls` が未使用のまま出荷エンジンにコンパイルされている。** `cargo tree -i native-tls` の親は今も 1 つ、xoksa 自身の `Cargo.toml` だけ。現物由来のインベントリに `hyper-tls` は **0 件**なので reqwest に native-tls の経路は無く、`native-tls` 0.2.18 と `schannel` はインベントリに載る＝デッドコードとして出荷されている。`[package.metadata.cargo-machete] ignored = ["native-tls"]` が、これを報告する唯一の検査を今も抑止しており、だから上記の `cargo machete` は clean である。脆弱性ではないが、クラス A の認証情報を扱うバイナリにおける不要な面である。
- **設定アプリは今も静的ゲート集合の外にある。** `xoksa-setup` は `scripts/check.sh` に **0 回**、`.github/workflows/audit.yml` に **0 回**しか現れない。`ci.yml` での 2 箇所だけの言及は、`tauri-build` が検証する sidecar スタブを作るコメントと `touch` である。対して `xoksa-desktop` は `check.sh` に 5 回名指しされている。つまりクラス A の認証情報を収集する出荷バイナリ（§6）が、`fmt --check` も `clippy -D warnings` も `test` も、自分の lockfile に対する `cargo audit`／`cargo deny` も受けていない。上記のその SBOM と `cargo audit bin` は、ゲートではなく本検査が作ったものである。

**測っていないもの、およびその理由。**

- **VirusTotal は今回対象外であり、「未了」ではない。** オーナーが示した順序は、公開が署名に先行する必要があるということであり、未署名かつ新規のハッシュでは [§6.2／§6.3](./av-false-positive-case-study.md) の初見レプテーション減点を再現するだけである（Windows の署名は Certum Open Source Code Signing、購入済み・発行待ち）。したがって未完了タスクとしては持たない。*（2026-10-03 訂正：本ファイルの 8 箇所が証明書を「Certum 標準 CodeSigning」と記していた。注文しているのは **Open Source Code Signing in the Cloud**（SKU `bd_005`）で、2026-08-04 に Certum から直接 58 USD で購入している——別製品であり、かつクラウド版なので署名は SimplySign を使い USB トークンは要らない。リセラー経由の先行注文は、Certum が**リセラー向けに**販売終了したためキャンセルした。発行はまだ着手していない。その理由は確認できていないので、本ファイルには書かない。）*
- **MSI のインストール／実行／アンインストール**は昇格が要るため未実施。同梔物は上記のとおりハッシュで検証済みである——CAB 内の 3 本のうち 2 本は単体現物とバイト一致で、残る 1 本の差は Tauri が刻むインストール形態 3 バイトだけである。
- **チャット経路の LLM 部分は通していない。** OpenAI が終始 `429 Too Many Requests` を返したためで、そのためセッション状態の検査は `/api/llm/select`（モデル呼び出しなし）を用い、K2 はローカルの Ollama モデルを用いた。SOT と整合性の測定に影響は無い：いずれもエンジンが算出した確定値を読んでおり、整合性ガードは stub のプロバイダで通している。
- **Stooq のフェイルオーバー試験**（`cargo test -- --ignored failover_adopts_stooq`）はこのネットワークでは実行できない：Stooq が CSV ではなく anti-bot の web ページを返す。この試験は到達不能なプロキシで primary を落とし、実物の secondary が応答することを要求するので、これは製品の不具合ではなく環境条件である——そしてエンジンがそこで行ったことは §3 が約束する挙動そのものだった：両方の原因を名指しし（`Market data API request failed … the fallback provider did not answer either: Stooq answered with a web page instead of CSV data (its anti-bot check, most likely); the fallback provider is unavailable on this network`）、何も創作しなかった。
- **ランタイムのプロンプトインジェクション（H1）**。理由は上記のとおり。

**本リリースは公開済みである**——2026-10-03 07:09 JST（タグ `v2.9.10` は `b3ef8e1`）。

### v2.9.9（macOS）— CI ビルド現物を 2026-09-27 検査・署名・公証 — 合格（静的／SOT／ランタイム／ペネトレ／ペイロード同一性／SCA／SBOM／署名・公証／VirusTotal／ZAP）

2.9.9 の macOS 側。**この版で 2 組目**の macOS 現物であり、1 組目を破棄した理由が本記録の主題である。

**1 組目は、公開済み Windows 側と共有しないソースから作られていた。** rustc が将来ハードエラーにすると予告している E0365 を消すため、vendor した `proc-macro-error2` に 1 行の修正を入れていた。`[patch.crates-io]` はパッケージの source id を変え、Cargo はユニットの `-C metadata` を依存（proc-macro を含む）から計算するため、それが wasm32 にコンパイルされる 8 クレートまで伝播する。**推測ではなく実測**：無関係な proc-macro クレート（`paste` 1.0.15）を **1 バイトも変えずに**複製し（`diff -r` で同一を確認）patch で差し込んだだけで WASM が変わった（`7cc029f2…` → `5e12fc6d…`）。`data`・`type`・`import`・`export`・`memory`・`global`・`table` の各セクションはバイト単位で同一のまま、動いたのは `code`（−135 B）・`function`（+4 B）・`element`（+6 B）だけ。同じプログラムで違うバイト——そして §C.1 規則 5 の下では、この修正を載せた v2.9.9 は**公開済みで既にダウンロードもされた** Windows の MSI を再現できなくなる。よって修正は本版から外し、現物を作り直した。

**本ビルドは、公開済み Windows のソースとプログラム内容が同一である。** `git diff 6f6657f 5082f32` を `src/`・`webui-leptos/`・`xoksa-desktop/src/`・`xoksa-setup/src/`・`xoksa-paths/`・全 `Cargo.toml` / `Cargo.lock`・両 `tauri.conf.json` に対して取ると**空**。差がある 8 ファイルはワークフロー・`.gitignore`・文書・`rust-toolchain.toml` で、いずれもコンパイルされない。

**ビルド出所 — GitHub Actions `Release build (macOS)` run `36283764077`、ワークフロー `.github/workflows/release-macos.yml`、ランナー `macos-14`、ソースコミット `5082f32`（`main` 上。ブランチではない——`workflow_dispatch` はデフォルトブランチからしか登録されず、§C.1 の要求とも一致する）。** ツールチェインは `rustc 1.98.1 (48a229cea 2026-09-01)`・`cargo 1.98.1`・`trunk 0.21.14`・`tauri-cli 2.11.4`。**ビルドログにコンパイラ警告は 0 件。**

**ツールチェインを固定し、その固定を仮定ではなく検証した。** `rust-toolchain.toml` は `1.98.1`——公開済み Windows 現物（run 36134676892）を作った版であり、記録済みのハッシュが再現可能なまま保たれる。各ジョブはこのファイルから版を読む。`dtolnay/rust-toolchain` は `rustup default` を叩くだけなので、`stable` のままにすると固定した版が最初の cargo 実行時に**自動インストール**される——rustup が非推奨と表示する経路である。CI ログ：`rustup toolchain install 1.98.1`・`rustup default 1.98.1`、`stable` の出現 **0 回**。手元では `rustup show` が `active because: overridden by '…/rust-toolchain.toml'` と報告する。`stable` が今日たまたま 1.98.1 であるため、版番号だけではどちらの仕組みが選んだか言えない——決定理由まで見るのが要点である。

**凍結した macOS 現物**（署名・公証・staple 済み。署名前に CI 公開の `SHA256SUMS` と全件照合——5/5 一致）：

| 現物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| `XOKSA-2.9.9-arm64.dmg`（配布物） | `700303e4598c1181c62b1b0e0367ff82297737e165ba72b47f0f5c839c38a3d5` | 11,700,991 B |
| `XOKSA.app/Contents/MacOS/xoksa`（エンジン） | `34159ab7a827e7eb08bba2c3cb9790380b93f0f479c107473153b578c7aa5848` | 11,997,984 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop`（シェル） | `de07bb115a379a24a5696cc8aaea2ffae97103778db2ed402f78f96ef3025436` | 6,346,000 B |
| `XOKSA.app/Contents/MacOS/xoksa-setup`（設定アプリ） | `5af2432d5c46cdc86c8c3aab1f3ef097f05ffc991f14dc6e6ffe5d2f08189c9c` | 5,914,400 B |
| `xoksa-webui-6d3d14e56100a0b9_bg.wasm`（エンジンが埋め込むフロントエンド） | `1967db8dadb2591bf085f367c611a087d6d5c28867b4dec4997860535407568b` | 710,943 B |

**WASM は Windows 側とバイト単位では一致しない。これは想定内である。** Windows は `xoksa-webui-9516f21d2d5de561`（`bce0f631…`）を出荷している。ソースは同一なので、差はビルドホストによるもの——エンジンのバイナリ自体が OS ごとの現物であるのと同じ種類の差である。§C.1 が言う決定性は測り直して**ホスト内では成立**した（同一マシンで 2 回ビルドし、WASM はバイト同一）。ホストをまたいだバイト同一性は主張しておらず、要求もされていない。要求されているのは両者が同一ソースから来ていることであり、それは上記のとおり実測されている。

**どの現物もビルド者を名乗らない。** `scripts/check-no-host-paths.sh` は 4 本とも通過。`/Users/<任意>` の素の走査は、エンジン・デスクトップシェル・WASM で **0 件**。設定アプリのみ **1 件**（`/Users/runner/work/xoksa-dev/xoksa-dev/xoksa-setup`）で、これはランナー自身の作業領域であり誰も特定しない。未署名の CI 出力にも同じ 1 件があり、ビルド時マクロがデータとして埋め込むため `--remap-path-prefix` では届かない。対比として、2.9.8 の macOS 現物（開発者機ビルド）は `/Users/koz` をエンジンに **616 件**・シェルに 336・設定アプリに 280・WASM に 85 件含んでいた。ゲートは**署名後**にも回し、通過している。

**署名の経路——入力は CI の現物であり、再ビルドではない。** 内部 3 本とバンドルを `Developer ID Application: Kohzoh Tsuchiya (463YK5WF6H)` でハードランタイム付き署名し、公証・staple。DMG は staple 済みの `.app` から `/Applications` シンボリックリンクとともに手元で作成し、これも署名・公証・staple。`notarytool` は両方 **Accepted**、Gatekeeper は app・DMG とも **`source=Notarized Developer ID`**。

**ペイロード同一性——1 バイト、それも署名が動かさざるを得ないバイトである。** 署名を除去して比較すると、各バイナリは CI の原本と**ちょうど 1 バイト**だけ違い、位置は `__LINKEDIT` の `vmsize`。署名ブロブを収めるため 16 KiB ページ 1 枚ぶん繰り上がる（エンジン `0x28000`→`0x2c000`、シェル `0x18000`→`0x1c000`、設定アプリ `0x14000`→`0x18000`）。コードとデータは CI が作ったものとバイト単位で同一。

**静的 — 合格。** 固定したツールチェイン上で `check.sh` 15 ゲート緑。`cargo test` は **485 通過 / 0 失敗 / 1 ignored**（ignored は実 Stooq のフェイルオーバ e2e）。

**SOT — 合格。列ごとに照合した。** 東京・ニューヨークとも閉場の日曜に測定したため、3 回の読み取りの間に系列は動かない。CSV ログは位置指定なので、`--show-log-header` が出すヘッダと突き合わせて **51 列**を JSON ログと `/api/symbol/7203/summary` に対応させた結果、**51 中 50 が一致**。一致しない 1 つは `analyzed_at_timestamp`——実行時刻であって計算値ではなく、呼び出し間の秒数だけ違う（Windows の記録と同じ）。3 経路で同一：`final_score` **−6.0**、`rsi` **40.681851**、`macd` **−7.415265**、確定終値 **2989.5**。

**ランタイムとペネトレ（凍結した署名済みエンジン `34159ab7…` に対する B.5）— 合格。** `inspect.sh` **9/9**（検査 9 はビルドパスのゲート）。セキュリティヘッダ 6 種を確認。CSP はリクエストごとの nonce を持ち、`unsafe-inline` の出現は **0**。2 クライアントが異なる 32 桁 hex の `xoksa_sid` を受け取る（`HttpOnly`・`SameSite=Strict`）。パストラバーサル 6 payload はすべて **404**。`text/plain` **415**、クロスオリジン POST **403**、5 MB の正当なルール **413**。

**検査したフロントエンドは出荷するものである。** エンジンは `webui-leptos/dist` の**無い**ディレクトリから起動した（`--web-dir` の既定が相対パスで、`read_asset` がディスクを埋め込みより優先するため。本リリースでは一度これでスキャンを破棄している）。配信されたページは `xoksa-webui-6d3d14e56100a0b9` を参照しており、上表の WASM と一致する。

**実動作 — 検査しただけでなく、実際に起動した。** `HOME` を隔離して起動し、操作者の実設定に触れていないことを前後の mtime で確認済み。デスクトップシェルはエンジンを**エフェメラルポート**の子プロセスとして起動し（`serve --ui --host 127.0.0.1 --port 57555`。§6 の契約どおり）、`/api/health` は `xoksa-web 2.9.9` を返し、配信されたページはこの現物自身の WASM を参照した。

**SCA — 脆弱性 0。** `osv-scanner`：エンジン **0**（338 パッケージ）、`xoksa-paths` **0**（18）、WASM **3**（188）、`xoksa-desktop` **7**（420）、`xoksa-setup` **7**（446）。後ろ 2 つが 8 行に見えるのは `GHSA-wrw7-89jp-8q8g` が `RUSTSEC-2024-0429` の別名であるため。`cargo audit`・`cargo deny check`・`cargo machete` は両 workspace で通過。問題になり得る 2 件はいずれも出荷 macOS ファイルに届かない：`glib` 0.18.5（6.9。唯一 CVSS を持つ）は gtk-rs の Linux バインディングにあり WKWebView のバイナリにはコンパイルされない。`event-listener` 5.4.1 の `RUSTSEC-2026-0221`（`!Send` がスレッド境界を越える）はスレッドを要するが、出荷 WASM のメモリセクションは `limits_flags=0x00`＝非共有メモリである。

**SBOM — 1 本を再生成し、1 本は測ったうえで据え置いた。** `sbom/xoksa-native-macos-arm64.cdx.json`：CycloneDX 1.5、**221 `library` コンポーネント**。**同一 lockfile の使い捨て `cargo auditable` ビルドからの導出であって、出荷バイナリからの抽出ではない**（CI は `auditable` でビルドしていない）。公開済みインベントリと比べて library 集合は変わらず、差は自クレートの版数のみ（`xoksa@2.9.8` → `xoksa@2.9.9`）。あわせて、走査したバイナリを絶対パス（`/private/tmp/sbom298/xoksa`）で名指していた 222 番目の `file` コンポーネントが消えた。手順が `SYFT_FILE_METADATA_SELECTION=none` を設定するようになったためで、これで 5 本すべてが `library` のみになる。`sbom/xoksa-webui-leptos.cdx.json` は再生成して比較した結果 **library 集合の差が 0 件**だったため、公開済みファイルをそのまま残す——vendor の patch を外したことで lockfile が元の状態に戻ったためである。使い捨てに埋め込まれた **245** 依存に対する **`cargo audit bin`**：**脆弱性 0**。使い捨てツリーは削除し、出荷ファイルには触れていない。

**VirusTotal（出荷ハッシュに対して・2026-09-27）— 4 本ともクリーン。** DMG `700303e4…` **0/61**、エンジン `34159ab7…` **0/62**、デスクトップシェル `de07bb11…` **0/62**、設定アプリ `5af2432d…` **0/63**。

**OWASP ZAP パッシブスキャン（Parrot ホストから LAN 越しに `0.0.0.0`、凍結した署名済みエンジンに対して）— 8 alerts、0 High / 2 Medium / 1 Low / 5 Info。カテゴリと判定は Windows 側と同一。** このスキャンは*この*現物に紐づいている——alert が `xoksa-webui-6d3d14e56100a0b9` を名指している。Medium 2 件は据え置かず、ここで測り直した。

- **`GET /login` に Anti-CSRF トークンが無い。** `/auth` は Origin 検査を免除される唯一の POST であり、代わりに 256bit のトークン自体が門になる。このエンジンでの実測：誤ったトークンでのクロスオリジン POST は **401**、空トークンは **401**、正しいトークンは **303**（文書化済みのトークン→cookie 交換）。セッション cookie は `HttpOnly`・`SameSite=Strict`。
- **SRI 属性の欠落**、報告対象は `.wasm`。エンジンが実際に配信する HTML は **3 サブリソースすべて**（スタイルシート・JS グルー・WASM）に `integrity="sha384-…"` を持つ——実測済み。ZAP が HTML 向けのパッシブ規則をバイナリに当てている。

Low は `/api/symbol/…/summary` の Unix タイムスタンプ露見で、これは設計どおりの市場データ時刻である。Info 5 件は文書化済みのベースライン：`POST /auth` の cookie ポイズニング（同じトークン→cookie 交換）、セッション管理レスポンスの特定、モダン Web アプリケーション、localStorage の情報開示（UI の表示状態。セッション id はサーバ側の `HttpOnly` cookie であって localStorage には無い）、`wasm-bindgen` 自身のグルー内の疑わしいコメント。

### v2.9.9（Windows）— CI ビルド現物を 2026-09-26 検査／**2026-09-26 15:38 JST 公開**（タグ `v2.9.9` は `83cfb38`）— 静的／SOT／ランタイム／ペネトレ／ZAP／SCA／SBOM 合格。VirusTotal は署名まで保留。macOS は別途出荷（上記の記録）

v2.9.9 は、2.9.8 の文書監査が指標コードに到達したことから生まれた。4 つのスコア階段が価格差の絶対値を比較していたため、同じ閾値が銘柄によって 0.2 % の変動にも 124 % の変動にも対応していた。9 つの階段のうち 3 つが、閾値ちょうどの値を満たすとみなすかで残り 6 つと食い違っていた。`normalize_ticker_input` は指数名に連動投信を、`FANG+` に日次で QQQ の 3.1 倍を実測した 3 倍レバレッジ ETN を返していた。そして取得失敗はプロバイダ自身の理由を捨ててから一般的な失敗を報告していた。本リリースは算出スコアを変えるため、すべてのゲートを新しい現物で測り直し、前版からの引き継ぎは行わない。

**なぜこの現物で、最初の一式ではないのか。** 開発機でビルドした現物に対する検査を 2026-09-24／25 に実施したが破棄した。理由は、その検査が探していなかった欠陥である——rustc は panic メッセージのために各クレートのソースパスを記録するので、それらの現物はビルド者のアカウント名を載せていた（エンジンに `C:\Users\<名前>` が **689 件**、ブラウザがダウンロードする WASM に **85 件**）。`strip = true` はこれを消さない。`--remap-path-prefix` は実測した（690 → 168 件）が塞ぎ切れない——rustup が配る `std` の rlib に焼き込まれたパスにも、C ツールチェイン（`aws-lc-sys`）が cc-rs 経由で埋めるパスにも届かず、`trim-paths` はこの Cargo では安定化されていない。したがってリリースは CI でビルドする。CI のビルドアカウントは誰も特定しない。**以下の測定はすべて CI 現物に対して行った。** 破棄した回から引き継ぐのは、現物ではなく*ソース*についての所見だけである。

**ビルド由来 — GitHub Actions `Release build (Windows)` run `36134676892`、ワークフロー `.github/workflows/release-windows.yml`、ランナー `windows-latest`、ソースコミット `6f6657f`**（#166 の Squash & Merge。CI が公開した `SHA256SUMS` が自身のヘッダで名指しするコミットと同一）。ツールチェインは **`rustc 1.98.1`（`48a229ce`）——ビルドログではなく出荷エンジン自身から読み出した**、`trunk 0.21.14`、`tauri-cli 2.11.4`（いずれもワークフローで固定）。未署名。検査時の作業ツリーは同じコミットで clean。

**この現物は、破棄した現物と同じプログラムである。** `git diff a17c3f0 6f6657f` を `src/`・`webui-leptos/src/`・`xoksa-desktop/src/`・`xoksa-setup/src/`・`xoksa-paths/`・すべての `Cargo.toml` と `Cargo.lock`・`tauri.conf.json`・アイコン一式に対して取ると**空**である。変更された 15 ファイルは文書・SBOM・スクリプト・CI 設定である。バイナリが違うのは、ビルド者とコンパイラが違う（`1.97.1` → `1.98.1`）からであって、プログラムが違うからではない。

**凍結した Windows 現物 — 全件を CI 公開の `SHA256SUMS` と照合：5/5 一致。**

| 現物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| `XOKSA-2.9.9-x64.msi`（インストーラ。下記 3 本を同梱） | `8460f11c061435ca50da57d019552945747566f69c86613aa9830a5d975f8849` | 11,702,272 B |
| `xoksa.exe`（エンジン） | `087f2ba97d4f73e5b196ac2e36d48245facc6066dec939d432b35207fc70ce17` | 14,812,160 B |
| `xoksa-desktop.exe`（接続シェル） | `fa26f6ad5365a11673a118d49bf9ae117e169834b5862a7dc62c2d3a2f46a014` | 9,629,696 B |
| `xoksa-setup.exe`（設定アプリ） | `41f6d732774c76fcbefea116499c5ec18f9b67b432600bc7717bfe181aeacb85` | 8,414,720 B |
| `xoksa-webui-9516f21d2d5de561_bg.wasm`（エンジンが埋め込むフロントエンド） | `bce0f63124767841b5dc596f2be92a3ceba090968a81443a05f872ea20022727` | 711,103 B |

**どの現物もビルド者を名乗らない——再ビルドの理由そのものを実測。** `scripts/check-no-host-paths.sh` は 5 件すべて通過。デスクトップシェル・設定アプリ・WASM はユーザーパスを **1 件も持たない**。エンジンは **142 件**持つが、そのすべてが GitHub ホストランナーの組み込みアカウント——`C:\Users\runneradmin`（97 件）とその 8.3 短縮形 `C:\Users\RUNNER~1`（45 件。`aws-lc-sys` の C ビルド由来）——であり、誰も特定しない。破棄したローカルのエンジンは 689 件、その WASM は 85 件だった。このゲートは `inspect.sh` の検査 9 であり、リリースワークフローの必須ステップでもあるため、これを再発させるビルドはアップロード前に落ちる。

**同梱物の同一性 — MSI を展開して実測。** インストーラ内のエンジンと設定アプリは、上表の単体現物と**バイト一致**。デスクトップシェルだけが **`0x77190f`〜`0x771911` のちょうど 3 バイト**違い、Tauri がインストール形態を刻む箇所である（`__TAURI_BUNDLE_TYPE_VAR_UNK` → `…_MSI`）——v2.9.8 の所見をそのまま再現しており、インストーラは他に一切手を加えていない。6 本すべて（単体 3 本・展開 3 本）が `FileVersion` **2.9.9** を名乗る。v2.9.9 の最初のビルドで起きたサイドカーの版数ずれは再発しない。`scripts/bundle-engine.sh` が、自身の `Cargo.toml` の版数を名乗らないサイドカーのステージを拒否するようになったためである。

**AV 誤検知のレシピは CI でも保たれた。** エンジンの `.rsrc` は **2,048 B・エントロピー 4.168**、`.rdata` は **5,181,440 B・5.875**。デスクトップの `.rsrc` は **32,768 B・3.897**、設定アプリは **32,768 B・3.899**——いずれも破棄したローカルビルドの実測値と 0.01 以内であり、ビルドを CI へ移したことは、挙動 ML エンジンが採点する軸を動かしていない。

**静的 — 合格。** CI がこのコミットに対し `scripts/check.sh` のゲート集合を実行（#166 green）。同じツリーで再実測：`cargo audit` は **338** クレート依存に対し **脆弱性 0**、`cargo deny check` は advisories／bans／licenses／sources **すべて ok**、`cargo machete` は未使用依存なし、`osv-scanner` は lockfile ごとにエンジン **0**・`xoksa-paths` **0**・WASM **3**・デスクトップ **8**・設定アプリ **8**（lockfile が無変更のため、破棄した回と同数）。

**SOT — 合格。3 つの代表値ではなく列単位で照合した。** CSV ログは位置指定なので、51 列を JSON ログのキーへ逆写像した：**51 列中 50 列を同定し、その 50 列すべてが `/api/symbol/AAPL/summary` と一致**。同定できなかった 1 列は `analyzed_at_timestamp`——実行時刻であって算出値ではない。API 側の値がログ側と 20 秒違うのは 2 回の呼び出しの間隔であり、不一致ではなく正しい挙動である。三経路で同一：`latest_observed_price` **341.07**、`final_score` **5.0**、`bar_close` **341.07000732421875**、`rsi` **70.41048490649925**。

**ランタイム／ペネトレ（凍結エンジン `087f2ba9…` に対する B.5）— 21/21 合格。** `inspect.sh` **9/9**（ホストパス・ゲートを検査 9 として含む）。拡張 **12/12**：`--version`・`/api/health`・配信ページの 3 者が `xoksa-web` **2.9.9** で一致。セキュリティヘッダ 6 種。CSP はリクエスト毎の nonce を持ち `unsafe-inline` を含まない（2 回取得して別の nonce）。`xoksa_sid` は 32 桁 hex・`HttpOnly`・`SameSite=Strict` で、2 クライアントには別 id。パストラバーサル 6 種すべて 404。クロスオリジン `GET /` は 200 で、Content-Type ゲートとクロスオリジン POST は 415／403。5 MB ボディは 413。**暗黙の `./xoksa.env` は無い**——起動ディレクトリに置いた囮（`SELL_RSI=99`・`LANG=en`）は無視され、§6 の単一設定場所の契約が現物で成立する。**ブラウザ描画**——Edge headless で WASM アプリがマウント（DOM 5,020 文字・select 3・input 4・button 7・日本語 UI）。

**検査したフロントエンドが出荷物であることを、仮定ではなく証明した。** 配信ページは `xoksa-webui-9516f21d2d5de561` を参照し、配信された `_bg.wasm` のハッシュは `bce0f631…` で、上表の WASM とバイト一致する。これを確認したのは、本リリースの最初の ZAP スキャンがまさにこの理由で無効だったためである——`--web-dir` の既定は**相対パス** `webui-leptos/dist` で、`read_asset` はディスク上のファイルを埋め込みより優先する（`src/server/mod.rs` にコメントで明記された意図的な開発時の利便性）。したがってリポジトリ直下から起動したエンジンは、自身の埋め込みではなく作業ツリーのフロントエンドを配信する。§C.1 に、検査対象のエンジンは `webui-leptos/dist` が存在しないディレクトリから起動すること、を追記した。

**公開モード（`--host 0.0.0.0`）— 凍結エンジンに対し LAN 経由で実測。** LAN アドレスは未認証で **401**、64 桁 hex の Bearer トークン付きで **200**（`{"status":"ok","service":"xoksa-web","version":"2.9.9"}`）を返す。`--private` と非ループバックの併用は起動時に理由つきで拒否される：*"Refusing to bind non-loopback host '0.0.0.0' in --private mode: an auth token cannot be persisted."*

**OWASP ZAP パッシブスキャン（Parrot ホスト・LAN 経由の `0.0.0.0`・凍結エンジン `087f2ba9…`）— 8 件（High 0／Medium 2／Low 1／Info 5）、いずれも欠陥ではなく、分類と判定は v2.9.8 と同一。** 実行は*この*現物に束縛されている——アラートは、このエンジンが埋め込み配信する `xoksa-webui-9516f21d2d5de561` を名乗る。

- **SRI 属性欠如（Medium）——バイトまで辿った。** アラートの URL は HTML 文書ではなく `.wasm` であり、その根拠 `<link rel="stylesheet" href="…">` は `webui-leptos/src/main.rs:3199` の Rust `format!` テンプレートが*この現物の* wasm データセクション、オフセット **`0x09ce6e`** にコンパイルされたものである（`{origin}` と `{file}` のプレースホルダは非表示バイトとして読める）。ZAP は HTML 用のパッシブ規則をバイナリに当てている。エンジンが実際に配信する HTML は、**3 つのサブリソースすべて**に `integrity="sha384-…"` を持つ（実測）。フロントエンドが実行時に組み立てるポップアップ用スタイルシートは `style-src 'self'` 下の同一オリジンであり、第三者ホストに対する対策である SRI には守るものが無い。
- **Anti-CSRF トークン欠如（Medium・`GET /login`）**——従来どおりで、意図的である。`/auth` は Origin 検査を免除した唯一の POST であり（理由はコードに明記）、代わりに 256 bit のトークン自身が守る。実測：誤ったトークンでのクロスオリジン POST は **401**、空のトークンも **401**。
- **Cookie ポイズニング（Info・`POST /auth`）**——ZAP は `token` パラメータが cookie に到達することを正しく捉えている。実測：交換成功時に `xoksa_auth=<トークンそのもの>` と `xoksa_sid=<ランダム 32 桁 hex>` を発行し、いずれも `HttpOnly`・`SameSite=Strict`、`Max-Age` 無し（セッション cookie）。認証 cookie は派生セッション id ではなく**資格情報そのもの**である。これを許容と判断した根拠：トークンはもともと操作者がその端末に渡すものであること、`HttpOnly` がスクリプトからの読み出しを、`SameSite=Strict` がクロスサイト送信を塞ぐこと、ブラウザのプロファイルは §0.2 が対象外とする OS 信頼境界の内側であること、そして `--rotate-serve-token` が全ブラウザセッションを一度に失効させること（派生 id では自動的には得られない性質）。代償も省かずに記録する：LAN の受動的な観測者が 1 リクエストを捕捉すると、タブとともに死ぬブラウザ限定の id ではなく、CLI／API も開くマスター資格情報を得る。サーバ側でランダム id を発行する案は、後段の任意 hardening に挙げた。
- **Unix タイムスタンプ露見（Low・3 件・`/api/symbol/AAPL/summary`）**——実測：`bar_timestamp` 1790343000（2026-09-25 13:30 UTC）、`market_data_latest_timestamp` 1790366401（20:00:01 UTC）、`analyzed_at_timestamp` 1790388074（2026-09-26 02:01:14 UTC）。市場と分析の時刻であり §0.1 のクラス C、かつ §1 が要求するもの（読み取り値は算出した足と一緒に運ぶ）。ホストについては何も露見しない。
- **情報漏洩・ブラウザ内 localStorage（Info・5 件）**——ソースから 9 キーを全数列挙：`theme`・`font`・`symbol`・`tickers`・`targets`、およびペイン寸法の `right_w`・`fund_h`・`chat_h`・`news_h`。表示状態と銘柄コード（クラス C）であり、資格情報もセッション id も無い（`xoksa_sid` は `HttpOnly` cookie でサーバ側）。
- **疑わしいコメント（Info）**——配信されたグルー JS で実測：`//` コメント 19 件と `TODO` 1 件——*"// TODO we could test for more things here, like `Set`s and `Map`s."*——いずれも wasm-bindgen 自身の生成出力であって xoksa のソースではない。出荷 `.wasm` には**コメントが 1 つも無い**。唯一の `/*` は、オフセット `0x09d2c9` のバイナリ中に偶然並んだバイト対である。
- **セッション管理レスポンスの特定**・**モダン Web アプリケーション**（Info）——`xoksa_sid` cookie と JS 駆動 UI を正しく検出しただけで、所見ではない。

`/api/chat/stream`（SSE）はスキャン対象外である（プロキシが緩衝して 502 になる）。そのコントロールは直接検証した。

**SCA — 合格。ただし 1 層だけ前回より精度が落ちており、理由はビルドホストにある。** `cargo audit` は 338 依存に対し脆弱性 0、`cargo deny check` は advisories／bans／licenses／sources すべて ok、`cargo machete` は未使用依存なし、`osv-scanner` は上表のとおり。リリースワークフローは `cargo build --release` でビルドしており、**`cargo auditable build` ではない**。したがって出荷エンジンは依存リストを埋め込んでおらず、`cargo audit bin` は panic 文字列から **109 件**しか復元できない（全数ではない）。前版の記録にある「埋め込み依存 254 件・脆弱性 0」は、この現物については現物実測として**再掲できない**——だから再掲していない。

**SBOM — 変更なし。そう言える根拠を、仮定ではなく明示する。** 公開中の 5 つのインベントリは最新である：`xoksa-native-windows-x86_64.cdx.json` **225** 件、`xoksa-native-macos-arm64.cdx.json` **222** 件（Mac が 2.9.9 をビルドするまで 2.9.8 の macOS 現物に束縛されたまま）、`xoksa-desktop-windows-x86_64.cdx.json` **196** 件、`xoksa-setup-windows-x86_64.cdx.json` **212** 件、`xoksa-webui-leptos.cdx.json` **188** 件——すべて CycloneDX 1.5 で、絶対パスを含まない（実測）。これらは破棄したローカル検査時に使い捨ての `cargo auditable` ビルドから生成したものだが、CI 現物に対しても有効である。**`git diff a17c3f0 6f6657f` をすべての `Cargo.toml` と `Cargo.lock` に取ると空**であり、ビルドが解決する依存集合は lockfile・feature 選択・ターゲットで決まるが、そのいずれも変わっていないためである。**主張していないのは**、これらを*この*バイナリから読み出したということである——上記 `cargo auditable` の理由により、それはできない。**次リリースでの是正として記録する：** ワークフローに使い捨ての `cargo auditable` ビルドを追加して SBOM の入力とし破棄するか、SBOM 生成自体をリリースビルドの隣で CI に移すか、いずれかである。それが入るまで、インベントリは出荷依存集合を*導出*によって記述しており、*抽出*によってではない。

**次版へ持ち越す所見 — いずれも本ツリーで再検証し、いずれも変わっていない。**

- **`native-tls` が未使用のまま出荷エンジンにコンパイルされている。** `cargo tree -i native-tls` の親は 1 つ、xoksa 自身の `Cargo.toml` だけである。`reqwest` 0.13.5 は rustls に解決し、`hyper-tls` はバイナリ由来 SBOM に存在しないため、#159 以降 reqwest に native-tls 経路は無い。よって宣言に付いた 2 つのコメントはどちらも誤りであり、`[package.metadata.cargo-machete] ignored = ["native-tls"]` がこれを報告する唯一の検査を抑止している（上記で `cargo machete` が clean なのはこのためである）。`native-tls` 0.2.18 と、その唯一の固有の子である `schannel` 0.1.28 が死んだコードとして出荷されている。脆弱性ではないが、クラス A 資格情報を扱うバイナリにおける不要な攻撃面である。
- **設定アプリが静的ゲートの集合外にある。** `xoksa-setup` は `scripts/check.sh` にも `.github/workflows/audit.yml` にも現れない。`ci.yml` での 2 箇所の言及は、`tauri-build` が検証するサイドカーの stub を作るコメントと `touch` だけである。したがって `fmt --check` も `clippy -D warnings` も `test` も、自身の lockfile に対する `cargo audit` / `cargo deny` も受けていない——クラス A 資格情報を収集する出荷バイナリである（§6）。

**本リリースで新規（軽微）：** `deny.toml` が `RUSTSEC-2026-0173`（`proc-macro-error2`・unmaintained）を無視しているが、`cargo deny` は**どのクレートも該当しない**と警告するようになった。依存が無くなり、この項目が古くなっている。削除すれば警告の有用性が戻る。

**任意の hardening（オーナー判断）：** serve トークンをそのまま `xoksa_auth` に載せるのではなく、サーバ側でトークンに紐づくランダムなセッション id を発行する。捕捉された cookie がマスター資格情報にならない（前掲 ZAP の cookie 項を参照）。

**現物依存で保留の項目（この C.4 の慣行どおり、出荷時に記録する）。** **VirusTotal** は、オーナーが示した順序により今回は対象外とする——公開が署名に先行する必要があり、未署名かつ新規のハッシュでは [§6.2／§6.3](./av-false-positive-case-study.md) の初見レピュテーション減点を再現するだけである（Windows の署名は Certum Open Source Code Signing、購入済み・発行待ち）。**MSI のインストール／実行／アンインストール**は昇格が要るため未実施。同梱物は上記のとおりハッシュで検証済み。**Stooq フェイルオーバーの実地 e2e が唯一の ignored テスト**であり、この回線の anti-bot に阻まれる。**もう 1 つの外部条件も隠さず記録する：** SOT 検査に用いた CLI 分析は、OpenAI が `429 Too Many Requests` を返したため終了コードが非ゼロだった。テクニカル計算と両ログは完了しているので SOT の照合に影響は無いが、その実行では CLI 経路の LLM 部分を通していない。**macOS** 側——自身の CI ビルド、Developer ID 署名と公証、自身の検査、macOS エンジンの SBOM——は Mac に引き継ぐ。

### v2.9.8（macOS：2026-09-23 検査）— 合格（静的／SOT／ランタイム／ペネトレ／SCA／SBOM／署名・公証／VirusTotal／ZAP）

2.9.8 の macOS 側。出荷元の `main` には、Windows 検査の時点では存在しなかった修正が 2 件入っている。**#159** は依存 5 件（serde・csv・dirs 6→7・reqwest 0.12→0.13・axum 0.7→0.8 とそれが要求するルート書式の変更）をまとめたが、`main` の CI を赤にしたまま着地した：reqwest 0.13 は TLS 検証を `rustls-platform-verifier` に任せ、それが `webpki-root-certs`（Mozilla の CA バンドル）を **CDLA-Permissive-2.0** で引き込む——許可リストが一度も問われたことのないライセンスだった。**#160** で理由を併記して許可した（コードではなく証明書データ、コピーレフトなし、MIT 配布と両立）。あわせて、#159 が `xoksa-paths` にだけ当てた `dirs` の更新を、デスクトップの lockfile が取り込んだ。

**テスト全体を止めた事象には検知を入れた。** リポジトリが `~/Documents` 配下にあるため、iCloud が同期競合コピーを **72 件**静かに溜めていた（`Cargo 2.toml`・`icon 3.png` など。いずれも git 管理外で `git status` にも出ない）。そのうち `tests/no_drift 2.rs` がクレート名になり、**453 件のテスト全体**がコンパイルできなくなった（`invalid character ' ' in crate name`）。失敗一覧は空で、原因を名指ししたのは raw な cargo 出力だけだった。**#161** で `no_icloud_conflict_copies` を追加し、ツリーを走査して該当パスを示して失敗させる。これは封じ込めであり、解決はリポジトリを iCloud 管理外へ移すこと——署名のたびに繰り返している `codesign … detritus not allowed` の退避作業も同時に終わる。

**ビルドの出所 — ソースコミット `76120d5`（`main`）、ビルド時点で作業ツリーは clean。** 手順は同一：`trunk build --release` → `cargo build --release --bin xoksa` → `scripts/bundle-engine.sh` → `xoksa-setup` を 2 本目の sidecar へ → `APPLE_SIGNING_IDENTITY` 付きの `cargo tauri build`。host `aarch64-apple-darwin`。一度ビルドして署名・公証し凍結、以降の検査はすべてその現物に対する実測である。

**凍結した macOS 現物**（署名・公証・staple 済み）：

| 現物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| `XOKSA-2.9.8-arm64.dmg`（ダウンロード） | `008f2142c9854ae5219b0905a4cf684ba650ce02ffb20cb0176c12866a42aa50` | 11,826,991 B |
| `XOKSA.app/Contents/MacOS/xoksa`（エンジン） | `aec27e19787b8e84fd599905df2d8eed3315e2ffebab589fe8259088c3cc009e` | 11,997,920 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop`（シェル） | `aec1a1ad3f2e75e493d8aaf6bd97a3b4cfebb36d0520441ec88c3bc8c4f7f987` | 6,395,376 B |
| `XOKSA.app/Contents/MacOS/xoksa-setup`（設定アプリ） | `cc60654568e0fd4946095c0cf1727a9f9fc1aed3d42afc7c906cf6b91ce20637` | 5,963,792 B |

**静的 — 合格。** `check.sh` の全 16 ゲートが green。`cargo test` は **454 passed / 0 failed / 1 ignored**——lib 438・integration 10・**ドリフト検知 6**（iCloud 検知が 6 本目）・`xoksa-paths`。ignored は実ネットワークの Stooq フェイルオーバー e2e。

**SOT — 合格。** 実行間で系列が動かないよう東証の引け後に実測：`final_score` **−10.0**・RSI **45.98**・MACD **−0.2630**・確定足の終値 **3025.0** が、CSV ログ・JSON ログ・`/api/symbol/7203/summary` で一致。

**ランタイム／ペネトレ（凍結署名エンジン `aec27e19…` に対する B.5）— 合格。** `inspect.sh` **8/8**；`--version` と `/api/health` はいずれも 2.9.8；セキュリティヘッダあり；2 クライアントに異なる 32 桁 hex の `xoksa_sid`；パストラバーサルは **6 パターンとも 404**。

**payload 同一性。** 署名除去後、`xoksa` と `xoksa-setup` はビルド出力と**バイト一致**、`xoksa-desktop` のみ `__LINKEDIT` の `vmsize` が **1 バイト**差——2.6.7 以降の macOS リリースと同じ。

**署名・公証 — 合格。** `Developer ID Application: Kohzoh Tsuchiya (463YK5WF6H)`、bundle と内部 3 本に hardened runtime。`notarytool` は app・DMG とも Accepted、両方 staple 済み。Gatekeeper は両方 **accepted / source=Notarized Developer ID**。

**SCA — 出荷エンジンは 0 件。** `osv-scanner`：エンジン **0**（338 パッケージ。reqwest 0.13 の依存で 306 から増加）、`xoksa-paths` **0**（18）、WASM **3**（188）、`xoksa-desktop` **7**（420）、`xoksa-setup` **7**（446）。CVSS を持つのは gtk-rs の Linux バインディング `glib` 0.18.5（6.9）のみで、macOS の WKWebView バイナリにはコンパイルされない。残りは未保守クレートの通知（`proc-macro-error`・`unic-*` 群）。

**SBOM — 再生成。** `sbom/xoksa-native-macos-arm64.cdx.json`：CycloneDX 1.5・**221 `library` コンポーネント**・絶対パスの混入なし——2.9.4 の 209 から増加し、差分の大半は reqwest 0.13 の platform-verifier の依存群。**`cargo audit bin`** は埋め込み **245** 依存に対し **脆弱性 0**。`sbom/xoksa-webui-leptos.cdx.json` は lockfile と突き合わせて再生成不要（#149 以降変化なし）。`target-sbom` の使い捨てツリーは削除し、出荷バイナリには触れていない。

**VirusTotal（出荷ハッシュに対して・2026-09-23）— 4 本ともクリーン。** DMG `008f2142…` **0/60**；エンジン `aec27e19…` **0/63**；デスクトップ `aec1a1ad…` **0/63**；設定アプリ `cc606545…` **0/63**。

**OWASP ZAP パッシブスキャン（Parrot ホストから LAN 経由・凍結署名エンジンに対して）— 8 件、High 0・Medium 1。カテゴリと判定は v2.9.4 から不変。** 本リリースは axum 0.7 → 0.8 でルーティング層が変わるため、前回結果の流用ではなく再実行した。Medium（`GET /login` に Anti-CSRF トークンが無い）は今回の現物で測り直した：外部 Origin からの誤トークン POST は **401**、256bit のトークンを既に持つ者だけがセッションを得る（**303**）が、その者はエンジンに直接到達できる。セッション cookie は `HttpOnly` ＋ `SameSite=Strict` のまま。残りは文書化済みのベースライン：同一オリジンの WASM バンドルに対する SRI 欠落、`/api/symbol/…/summary` の Unix タイムスタンプ露見（設計どおりの市場データ時刻）、`POST /auth` の Cookie ポイズニング（文書化されたトークン↔cookie の交換）、セッション管理レスポンスの特定、モダン Web アプリケーション、`wasm-bindgen` 生成 glue 内の疑わしいコメント、localStorage の露見——UI 状態の **9 キー**（`theme`／`font`／`symbol`／`tickers`／`targets` と、2.1.0 のフロントエンド以来ある pane 幾何の `right_w`／`fund_h`／`chat_h`／`news_h`）。いずれも表示状態であり、認証情報もセッション ID も含まない（`xoksa_sid` は `HttpOnly` cookie でサーバ側）。言語は 2.9.4 で `xoksa.env` へ移っている。 *（2026-09-25・v2.9.9 の検査中に訂正：本段落は localStorage を 5 キーと記していたが、pane 幾何の 4 キーは 2.1.0 のフロントエンド以来存在する。判定は不変。）*

### v2.9.8（Windows：2026-09-15 検査／未出荷）— 静的／SOT／ランタイム／ペネトレ／ZAP／SCA／SBOM 合格。現物依存（VirusTotal・署名・macOS）は保留

v2.9.8 が存在する理由は 2 つある。2.9.7 のマージ前レビューで整合性ガードの不具合 3 件が見つかったこと、そして 2.9.7 の検査で `cargo audit` がクリーンだった**翌日**に助言が公開されたことである。不具合 3 件はいずれも、ガードが「捏造値を通す」側ではなく**「正しい文を消す」**側に誤るもので、いずれも英語の回答でだけ踏む。裸価格チェックと数値単位チェックが各数値を切り出した断片から読み直しており、断片にはそれを日付や期間の長さにしている語が入っていない（`Support has held since 2019.` が捏造価格として削除されていた）。`contains_any_term` が部分一致で判定しており、`low` が *below* に、`line` が *decline* に当たって、英語の回答のほとんどで価格文脈のゲートが開いていた。`safe_ratio` は 2.9.7 でプロバイダごとのガードを集約した際に条件が `abs() > f64::EPSILON` から `!= 0.0` に狭まり、EPS が `1e-300` のとき有限な `1.8e302` の PER が確定データに入っていた。**`rustls` 0.23.32 → 0.23.45** で RUSTSEC-2026-0285（TLS 1.3 のハンドシェイクを暗号化レベルの境界をまたいで受理する。5.3 medium）を解消した。この助言は 2026-09-14 の公開で、**v2.9.7 の検査が `cargo audit` のクリーンを記録した翌日**である。2.9.7 をそのまま出荷していれば、これを抱えたまま出すことになっていた。CHANGELOG [2.9.8] 参照。

**既知の制限は、直したと書かずに制限として書く。** 文中のどこかに書かれた年が、別の節にある読み取り値まで日付づけする状態は残っている。年の修飾が「自分の節に無ければ文中でいちばん近いものを採る」フォールバックを持つためで、これは `In 2019, revenue was …` では必要であり、`Price has been in this range since 2019, and RSI is 45.2.` では有害になる。両者を分けるには [security-design.md §1](./security-design.md) の規範を変える必要があるため、出荷直前に変えず、実測とともに Issue #146 で追跡する。

**ビルドの出所 — ソースコミット `e8b973d`（ブランチ `release/v2.9.8`、`main` `c79d5ad`［v2.9.7］から分岐）、ビルド時点で作業ツリーは clean。** 手順は同一：`trunk build --release` → `cargo build --release --bin xoksa` → `scripts/bundle-engine.sh` → `xoksa-setup` を 2 本目の sidecar へ → `cargo tauri build`。`rustc 1.97.1`・LLVM 22.1.6・`tauri-cli` 2.11.4・host `x86_64-pc-windows-msvc`・未署名。1 回だけ作って凍結し、以下のすべてをこのファイルに対して実測した。エンジンのハッシュは `sha256sum` と `CertUtil` で突合。

**凍結した Windows 現物：**

| 現物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| `XOKSA-2.9.8-x64.msi`（インストーラ。下の 3 本を内包） | `7868ea00803b596debc352bb34f7761dff650ef9e3fbdf85f5e58e629b530d98` | 10,612,736 B |
| `xoksa.exe`（エンジン） | `7fff208573fc8b4e9add1950201d4d3bb5d3b6fda5a7a15ec0507b5e3499a0a3` | 12,625,408 B |
| `xoksa-desktop.exe`（接続シェル） | `cf42dd2e908dc65b045f0e05bb9328065a84f74f6d45a4ceafb3473304d69e7f` | 9,646,080 B |
| `xoksa-setup.exe`（設定アプリ） | `859273b1024f9189daff94b90af60df828bf1bf242d97f3cfed0957ad2543d84` | 8,427,520 B |

MSI は `target/release/XOKSA-2.9.8-x64.msi`（唯一の置き場・常に 1 本）へ移し、NSIS の出力は捨て、置き換えとなった v2.9.7 の MSI は削除した。

**payload 同一性 — MSI を展開して実測。** インストーラ内のエンジンと設定アプリは、ビルド出力と**バイト一致**。接続シェルだけが `0x773FFF` の **3 バイト**だけ違い、これは Tauri がインストール経路を刻む箇所である（`__TAURI_BUNDLE_TYPE_VAR_UNK` → `…_MSI`）。1 本目の sidecar に据えたエンジンは `target/release/xoksa.exe` とバイト一致。

**AV 誤検知レシピは維持。** desktop の `.rsrc` は **32,768 B・エントロピー 3.897**、**PNG フレーム 0 個**、`VS_VERSION_INFO` も完全——v2.9.7 から変化なし。

**静的 — 合格。** `check.sh` の 15 ゲートすべて green——v2.9.7 と同じ集合で、テストのゲートも含む。`cargo test --lib` は **438 passed / 0 failed / 1 ignored**（v2.9.7 時点の lib 433 本から増加。上記の不具合に対する `llm.rs` の回帰テスト 4 本と、分母の帯に対する `fundamental.rs` の 1 本）。ignored の 1 本は実網の Stooq フェイルオーバー e2e。**`rustls` の助言を捕まえたのは、このゲートの 1 回目の実行である**——`cargo fmt` と `cargo audit` が落ち、どちらも現物をビルドする前に解消した。

**SOT — 合格。** 東京の引け後に測定し、実行間で系列が動かない状態で確認：記録の **24 項目のうち 22 項目**が CSV ログ・JSON ログ・`/api/symbol/7203.T/summary` で完全一致——`bar_date` **2026-09-15**、`bar_close` **3021.0**、`rsi` **44.39993302**、`macd` **2.856642277**、`final_score` **-2.0**。除いた 2 項目は分析の実施時刻そのもの。

**ランタイム／ペネトレ（凍結エンジン `7fff2085…` に対する B.5）— 合格。** `inspect.sh` **8/8**、追加実測 **19/19**。`--version` と `/api/health` はいずれも **2.9.8**（`service: xoksa-web`）。安全関連ヘッダ 6 本、`unsafe-inline` を含まない nonce CSP、`Set-Cookie: xoksa_sid=<32 桁 hex>; HttpOnly; SameSite=Strict`。2 クライアントは別々の 32 桁 hex の `xoksa_sid` を受け取る。パストラバーサルは 6 種すべて 404。クロスオリジンの `GET /api/health` は 200。起動ディレクトリのおとり `./xoksa.env` は読まない。

**露出モード（`--host 0.0.0.0`）— 凍結エンジンに対し LAN 経由で実測。** `--private` と非ループバックの併用は起動時に拒否される。LAN アドレスへの `/api/health` は認証なしで **401**、`Authorization: Bearer <token>` 付きで **200**（`xoksa-web`・`2.9.8`）。既に OS キーチェーンにあるトークンを再利用したので、検査のために生成したものは無い。

**OWASP ZAP パッシブスキャン（Parrot ホストから LAN 経由で `0.0.0.0` に対して。相手は凍結エンジン `7fff2085…`）— 8 アラート（High 0／Medium 2／Low 1／Info 5）。欠陥は 1 件も無く、分類・判定とも v2.9.7 と同一。** 測定がこのリリースの現物に紐づいていることは、アラートが名指しする `xoksa-webui-1b33333186c56cac_bg.wasm`（v2.9.7 は `ce27a3bf84e5938a`）で確認した。新しいアラートは出ておらず、Medium 2 件は、従来からの同一オリジン WASM バンドルの SRI 欠落と、2.9.2 のトークン認証で生まれた `GET /login` の Anti-CSRF トークン欠如（`/auth` は Origin 検査を意図的に免除している唯一の POST）である。`POST /auth` の Cookie ポイズニングは v2.9.7・v2.9.3 の記録どおり Info で、cookie を書くのはエンジン自身のトークンとの定数時間比較が一致した後だけ。 *（2026-09-25・v2.9.9 の検査中に訂正：本段落は 2 つ目の Medium を Cookie ポイズニングと記していたが、ZAP の分類は Info であり、v2.9.7・v2.9.3 の記録がそう書いている。件数は不変。）* `/api/chat/stream`（SSE）はスキャン対象外のまま。

**SCA — lockfile 5 本すべてで脆弱性 0。** `osv-scanner`：engine **0**（306 パッケージ）・`xoksa-paths` **0**（18）・WASM **3**（187）・`xoksa-desktop` **7**（419）・`xoksa-setup` **7**（445）。v2.9.7 と同じ 17 件の RustSec *informational* で、脆弱性は 1 件も無く、`rustls` は engine の指摘から消えている。`cargo audit`（ソース）と `cargo deny` は両 workspace で green。

**SBOM — 再生成。現物ベースの助言チェックもクリーン。** `sbom/xoksa-native-windows-x86_64.cdx.json`（**library 212 件**）と `sbom/xoksa-webui-leptos.cdx.json`（**187 件**）。`syft` 1.49.0 で CycloneDX 1.5、絶対パスなし。v2.9.7 からの差は `rustls` 0.23.32 → 0.23.45、`rustls-webpki` 0.103.13 → 0.103.15、およびルートコンポーネント自身の版数だけ。使い捨ての `cargo auditable` ビルドに埋め込まれた **234** 依存に対する **`cargo audit bin`：脆弱性 0**。`target-sbom` ツリーは削除し、出荷バイナリには触れていない。

**現物依存項目は保留（出荷時に本節へ記録・C.4 の慣例通り）。** **VirusTotal** は今回オーナーの判断で外す：現物は未署名で、新規ハッシュは [§6.2／§6.3](./av-false-positive-case-study.md) の初回レピュテーション減点を再現するだけである（Windows の署名は Certum Open Source Code Signing、購入済み・発行待ち）。**MSI のインストール／実行／アンインストール**は昇格が必要なため未実施で、payload は上記でハッシュ検証済み。**macOS** 側——Developer ID の署名・公証、macOS 自身の検査、macOS エンジンの SBOM（Issue #142・まだ 2.9.4）——は Mac へ申し送る。出荷するのは 2.9.7 ではなく **2.9.8** である。

### v2.9.7（Windows：2026-09-13 検査／未出荷）— 静的／SOT／ランタイム／ペネトレ／ZAP／SCA／SBOM 合格。現物依存（VirusTotal・署名・macOS）は保留

v2.9.7 は、整合性ガードの照合基準を**プロンプトから外した**。LLM の回答を照合する数値は `TechnicalDataGuard` と `FundamentalData` から構築し、リクエストと共に運ぶ。プロンプトには会話・利用者自身の発言・他モデルの見解も載っており、そこに書かれたラベルはエンジンが書いたものと区別できない——区別できなければ、利用者が打った数字が「確定値」になってしまうためである。同じ系列でアラート面も締めた。EXPLAIN 注記は発火したルールを再掲してよいが、比較を**構文として解析**してルールと突き合わせるので、向きの違い・ずれた閾値・閾値に付いた単位は、読み取り値の照合へ戻さず**条件として**拒否する（戻していたために、終値 3091 の銘柄で `close > ¥3091` が通っていた）。あわせて永続化の欠陥 4 件を閉じた——ルールはどの画面から作っても残る、書き込みは既存に積み増さず自分の枠に入る、16 本の上限は 1 箇所にある、`xoksa.env` の writer は**ちょうど 1 つ**（共有ロック・パーミッション維持・ローダーが持たない変数は触らない）。UI シェル 2 つは、設定ファイルを作業ディレクトリ経由ではなく明示的に解決するようにした。ドキュメントの訂正も同じ重みを持つ：監視が評価するのは**取得した最新の足**で、日本株の分足では確定足ではなく形成中の足である——ダッシュボードの説明文・`AlertRule` の doc コメント・マニュアル 2 本が、そう書いていなかった。CHANGELOG [2.9.7] 参照。

**ビルドの出所 — ソースコミット `5065da8`（ブランチ `release/v2.9.7`、`main` `4a3ab94`（v2.9.6）から分岐して 16 コミット先）、ビルド時点で作業ツリーは clean。** 手順は同一：`trunk build --release` → `cargo build --release --bin xoksa` → `scripts/bundle-engine.sh` → `xoksa-setup` を 2 本目の sidecar へ → `cargo tauri build`。`rustc 1.97.1`・LLVM 22.1.6・`tauri-cli` 2.11.4・host `x86_64-pc-windows-msvc`・未署名。1 回だけ作って凍結し、以下のすべてをこのファイルに対して実測した。ハッシュは `sha256sum` と `CertUtil` で突合。

**凍結した Windows 現物：**

| 現物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| `XOKSA-2.9.7-x64.msi`（インストーラ。下の 3 本を内包） | `c265617c9aaf80c1f222323a7d0aa93c7e52fa03a641287ec61e3be07bc7abff` | 10,616,832 B |
| `xoksa.exe`（エンジン） | `b6593b0d5ff98f1cc6e0a0254fbe6837ceeb63c6815882981f03de6a3fd5b307` | 12,624,896 B |
| `xoksa-desktop.exe`（接続シェル） | `36f9384bed05a2ade94fbb81c6af17e9c221b93cb79a58f4c51c78bd2429e4da` | 9,646,080 B |
| `xoksa-setup.exe`（設定アプリ） | `d52de6092a6784fec89070ccfcd3516432636d05af922d14169b624529b00b76` | 8,427,520 B |

MSI は `xoksa-desktop/target/release/bundle/msi/` から `target/release/XOKSA-2.9.7-x64.msi`（唯一の置き場・常に 1 本）へ移し、NSIS の出力は捨てた。そこにあった 2026-09-12 の MSI は**古いのではなく置き換え対象**である——`eb17cde` より前のもので、出荷してはならない。

**payload 同一性 — MSI を展開して実測。** インストーラ内のエンジンと設定アプリは、ビルド出力と**バイト一致**。接続シェルだけが `0x773FEF` の **3 バイト**だけ違い、これは Tauri がインストール経路を刻む箇所である（`__TAURI_BUNDLE_TYPE_VAR_UNK` → `…_MSI`）。それ以外の差は無い。1 本目の sidecar に据えたエンジンは `target/release/xoksa.exe` とバイト一致。

**AV 誤検知レシピは維持。** desktop の `.rsrc` は **32,768 B・エントロピー 3.897**、**PNG フレーム 0 個**、`VS_VERSION_INFO` は 6 項目すべて揃う。エンジン側は 2,048 B・4.172。PE のセクション数は v2.6.8 の 6 ではなく 7 で、増えた 1 本は `rustc 1.97.1` が出す `_RDATA`——リソースの変化ではない。§5 の対策はこの 2 つの数値の性質であり、どちらもレシピの内側にある。

**静的 — 合格。** `check.sh` の 15 ゲートすべて green（engine・`webui-leptos`［wasm32］・`xoksa-desktop`・`xoksa-paths` の fmt / clippy `-D warnings` / test、`cargo audit` ×2、`cargo deny` ×2、`cargo machete`）。`cargo test` は **448 passed / 0 failed / 1 ignored**——lib 433・統合 10・ドリフトガード 5——に加えて `xoksa-paths` で 1。ignored の 1 本は実網の Stooq フェイルオーバー e2e。

**SOT — 合格。** 実行間で系列が動かないよう、閉まっている市場（7203.T・週末の東京）で測定：記録の **24 項目のうち 22 項目が、CSV ログ・JSON ログ・`/api/symbol/7203.T/summary` で完全一致**——`final_score` **-2.0**、`rsi` **46.107725849147485**、`macd` **9.113332858421927**、`bar_close` **3031.0**、足と市場データの時刻まで一致。除いた 2 項目は分析の実施時刻そのもの（`analyzed_at`・`analyzed_at_timestamp`）で、実行ごとに変わるのが仕様である。

**ランタイム／ペネトレ（凍結エンジン `b6593b0d…` に対する B.5）— 合格。** `inspect.sh` **8/8**。`--version` と `/api/health` はいずれも **2.9.7**（`service: xoksa-web`）。安全関連ヘッダ 6 本——`unsafe-inline` を**含まない** nonce CSP・`Permissions-Policy`・`X-Frame-Options: DENY`・`X-Content-Type-Options: nosniff`・`Referrer-Policy: no-referrer`・`Set-Cookie: xoksa_sid=<32 桁 hex>; HttpOnly; SameSite=Strict`。2 クライアントは別々の 32 桁 hex の `xoksa_sid` を受け取る。パストラバーサルは **6 種すべて 404**。クロスオリジンの `GET /api/health` は **200**（塞ぐのは状態変更だけ）。ヘッドレスの `--dump-dom` で WASM アプリの mount を確認——ヘッダに `XOKSA` と `v2.9.7`、パネル見出しと ツール／チャット／銘柄／設定 の目印、module script の nonce は消費済み（nonce hiding）、script と stylesheet は SRI ダイジェストを持つ。

**2.9.7 自身の修正を、現物で確認。** **暗黙の `./xoksa.env` は無い**——起動ディレクトリに `LANG=en` のおとりを置いても読まず、正準の `ja` を報告する。**writer は 1 つ**——`POST /api/config/lang` による言語の書き込みは **`LANG=` の行だけ**を変え、コメント・ローダーが持たない変数・5 行の `ALERT_1_*` ブロック・`STANCE` は無傷で、ファイルの ACL も前後で同一だった。

**検査の衛生 — 検査が操作者のチャンネルに届かないようにした。** `serve` の起動はすべて `--env-file` で検査専用の設定（`ALERT_<n>_*` も `NOTIFY_<n>_*` も持たない）を指した。この機の正準 config には有効なルール 3 本と通知先が入っており、それで検査すれば監視が最初に見た rising edge で実際の通知——と EXPLAIN の LLM 呼び出し——を出しかねない。これでバイナリの検査が甘くなることはない：設定ファイルは入力であり、`inspect.sh` も同じ凍結エンジンに、このフラグを足す薄い wrapper 経由で到達している。

**SCA — lockfile 5 本すべてで脆弱性 0。** `osv-scanner`：engine **0**（306 パッケージ）・`xoksa-paths` **0**（18）・WASM **3**（187）・`xoksa-desktop` **7**（419）・`xoksa-setup` **7**（445）。17 件はすべて RustSec の **informational**（*unmaintained* または *unsound*）で、**脆弱性は 1 件も無い**。`glib` RUSTSEC-2024-0429（osv が唯一スコアを付ける 6.9）と `proc-macro-error` RUSTSEC-2024-0370 は、**Windows の依存グラフに存在しない**——`cargo tree --target x86_64-pc-windows-msvc` で実測。Linux の WebKitGTK 経路のものである。`unic-*` の 5 件は `urlpattern` → `tauri-utils` 経由で実行時にも到達するが、上流の持ち物で修正版は出ていない。`event-listener` 5.4.1 RUSTSEC-2026-0221 は *unsound* のみ（`StackSlot` が無条件に `Send`/`Sync` を実装）で、出荷 WASM では到達しない——単一スレッドの `wasm32-unknown-unknown` ビルドで、設定のどこでも atomics もスレッドも有効にしていない。5.4.2 で修正済みなので、この現物への修正ではなく次版での lockfile 更新事項である。`cargo audit`（ソース）と `cargo deny`（advisories＋ライセンス＋bans＋sources）は両 workspace で green。

**SBOM — 再生成。現物ベースの助言チェックもクリーン。** `sbom/xoksa-native-windows-x86_64.cdx.json`（**library 212 件**）と `sbom/xoksa-webui-leptos.cdx.json`（**187 件**）。`syft` 1.49.0 で CycloneDX 1.5、絶対パスなし。依存の集合は **v2.9.6 と同一**で、差はルートコンポーネント自身の版数だけ（`xoksa`／`xoksa-webui` 2.9.6 → 2.9.7）。使い捨ての `cargo auditable` ビルドに埋め込まれた **234** 依存に対する **`cargo audit bin`：脆弱性 0**。`target-sbom` ツリーは削除し、出荷バイナリには一切触れていない。

**露出モード（`--host 0.0.0.0`）— 凍結エンジンに対し LAN 経由で実測。** ZAP の実行にあわせて fail-closed の経路を端から端まで確認した。`--private` と非ループバックの併用は**起動時に拒否される**（「an auth token cannot be persisted」）ので、無痕跡セッションが黙って無認証で動くことはない。bind した状態で、LAN アドレスへの `GET /api/health` は認証なしで **401**、`Authorization: Bearer <token>` 付きで **200**（`xoksa-web`・`2.9.7`）。**ループバックも同じく塞がる**——非ループバック bind 中は `127.0.0.1` からでも 401 になり、ゲートは接続元アドレスに依存しない。トークンは以前の LAN セッションで既に OS キーチェーンにあり、検査のために新規生成されたものは無い。なお他ホストからポートに到達するには、Windows がこのエンジンに対して作っていた受信 **Block** 規則 2 本の削除が必要だった（ファイアウォールの確認をキャンセルしたときに残る "Query User" 規則）。これは運用側の条件であり、アプリケーションの問題ではない。

**OWASP ZAP パッシブスキャン（Parrot ホストから LAN 経由で `0.0.0.0` に対して。相手は凍結エンジン `b6593b0d…`）— 8 アラート（High 0／Medium 2／Low 1／Info 5）。欠陥は 1 件も無い。** 測定が*この*リリースの現物に紐づいていることは、アラートが名指しする `xoksa-webui-ce27a3bf84e5938a_bg.wasm` と `…ce27a3bf84e5938a.js`（このエンジンが内蔵するフロントエンド）で確認した。6 分類は従来どおり受容済みのもの——SRI（クロスオリジンの資産が無い）、Unix タイムスタンプ（`/api/symbol/NVDA/summary` に出る分析データそのもの）、セッション管理レスポンス（`xoksa_sid` という対策自体）、モダン Web アプリケーション、localStorage（UI の状態のみ。鍵はサーバ側に留まる）、疑わしいコメント（WASM のグルー JS 内）。**2 分類は v2.6.8 の基準以降に増えたもので、どちらも 2.9.2 でトークン認証を入れたときに生まれたログイン画面のものであり、退行ではない：**

- **Anti-CSRF トークンが無い（Medium・信頼度 Low）／`GET /login`** — 証拠は `<form method="post" action="/auth">`。`/auth` はサーバ側 Origin 検査を意図的に免除している唯一の POST で、理由はコードに記録してある：この経路を守るのは推測不能な 256bit トークン自体であり（トークン無しのクロスサイト POST は `401`）、かつ厳格な `Referrer-Policy: no-referrer` のためブラウザは同一オリジンのフォーム POST でも `Origin: null` を送るので、検査を掛けると正規のログインを拒否してしまう。ここでの login-CSRF はトークンを知っていることが前提であり、それは既に全アクセス権を持つということなので、権限の獲得にならない。他の状態変更 POST は検査を維持している。
- **Cookie ポイズニング（Info）／`POST /auth`（`token`）** — フォームの値が cookie の値になる形。書き込むのはエンジン自身のトークンとの定数時間比較が一致した後だけなので、保存される値は常にエンジンが生成した 256bit hex である。送られた文字列が `Set-Cookie` に到達することはなく、`HeaderValue::from_str` が制御文字を拒否するのでヘッダ分割も起こらない。

`/api/chat/stream`（SSE）はスキャン対象から外している——プロキシがバッファして 502 になるため——その制御は直接検証済み。

**現物依存項目は保留（出荷時に本節へ記録・C.4 の慣例通り）。** **VirusTotal** は今回オーナーの判断で外す：現物は未署名で、新規ハッシュは [§6.2／§6.3](./av-false-positive-case-study.md) の初回レピュテーション減点を再現するだけである（Windows の署名は Certum Open Source Code Signing、購入済み・発行待ち）。**MSI のインストール／実行／アンインストール**は昇格が必要なため未実施で、payload は上記でハッシュ検証済み。**macOS** 側——Developer ID の署名・公証、macOS 自身の検査、macOS エンジンの SBOM（Issue #142・まだ 2.9.4）——は Mac へ申し送る。

### v2.9.4（macOS：2026-08-21 検査）— 合格（静的／SOT／ランタイム／ペネトレ／SCA／SBOM／署名・公証／VirusTotal／ZAP）

v2.9.4 は、正典に実装を合わせるのではなく、**出荷バイナリを正典に照らして検査した**版である。発端はオーナーの問い——「マニュアルを直すとなぜ現物のハッシュが変わるのか」。マニュアルがバイナリにコンパイルされ `/manual/:slug` で配信されていたからで、その面は**どの UI からもリンクされておらず、CLI にも出口が無かった**。埋め込みスクリーンショット・Markdown レンダラ・誰も呼ばない分析エンドポイント・呼び出し元のないデスクトップコマンドとともに削除した。続く監査でさらに 4 件を発見し、いずれも修正した：`--private` がユーザーの打ったチャット行を `~/.xoksa_history` に書いていた／設定アプリが実行していない再起動を成功と報告していた／`SERVE_PORT` は読まれるのに設定画面に欄が無かった／UI 言語の情報源が**4 つに分裂**しており、設定アプリで日本語を選んでも接続画面が英語のままになり得た。現在は `xoksa.env` の `LANG` が唯一の情報源で、ダッシュボードのセレクタが書き戻す（ループバック限定・`--private` は `409`）。**見つからなかったことも同じく重要である**：指標の実装は各 1 箇所、総合スコアも 1 実装、表示・ログ・レポート・LLM プロンプト・CSV・JSON はすべて同じガードから 1 本のテキストビルダーを通る——SOT の中核は無傷だった。

**ドリフト検知を追加。** 上記の欠陥はいずれも*欠落*——2 つのファイル間の接続が無いという、diff に現れない種類の問題だった。`tests/no_drift.rs` がその接続を `cargo test` で表明する（シェルスクリプトではないので Windows でも同じ検査が走る）：`apply-config` の項目と設定フォームの項目、辞書キーとそれを描画する要素、`/api/*` の各ルートと `#[tauri::command]` の各コマンドとその呼び出し元、そして UI 言語の単一情報源。各ガードは、守る対象の不変条件を実際に壊して失敗することを確認済み。

**ビルドの出所 — ソースコミット `dcaf7a4`（ブランチ `release/v2.9.4`、`main` `05f280e` から分岐し `release/v2.9.3` を fast-forward で取り込み）、ビルド時点で作業ツリーは clean。** 手順は同一：`trunk build --release` → `cargo build --release --bin xoksa` → `scripts/bundle-engine.sh` → `xoksa-setup` を 2 本目の sidecar へ → `APPLE_SIGNING_IDENTITY` 付きの `cargo tauri build`。`rustc 1.96.0`・host `aarch64-apple-darwin`。

**凍結した macOS 現物**（署名・公証・staple 済み）：

| 現物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| `XOKSA-2.9.4-arm64.dmg`（ダウンロード） | `a26b11181fd69ce9ab6a07b02f0d34b770bd1ff12cd8c48747644ec926fdc235` | 10,473,031 B |
| `XOKSA.app/Contents/MacOS/xoksa`（エンジン） | `870be676ece35e24de2b1d593d45ea0cc9364f325f9a2a84a600c910f95facd3` | 9,447,168 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop`（シェル） | `5a44dbad4fcef6f851b87c45dce39770f920a62453cc327699942d3dcd81a96c` | 6,395,376 B |
| `XOKSA.app/Contents/MacOS/xoksa-setup`（設定アプリ） | `bca912c2cd34c482c817d672ee98bed999cfa1fb6384cbf2523c25f93ec7b539` | 5,963,792 B |

エンジンは **v2.9.3 より 1.4 MB 小さい**——マニュアルとスクリーンショットがもう入っていない。凍結バイナリで実測：出荷 2.9.3 に存在したマニュアル由来の 4 文字列は**すべて 0 件**。

**静的 — 合格。** `check.sh` の全 16 ゲートが green（エンジン・`webui-leptos`(wasm32)・`xoksa-desktop`・`xoksa-paths`）。`cargo test` は **299 passed / 0 failed / 1 ignored** —— lib 284・integration 10・**ドリフト検知 5**（`tests/no_drift.rs`）・`xoksa-paths` 10。ignored は実ネットワークの Stooq フェイルオーバー e2e。

**SOT — 合格。** 実行間で系列が動かないよう、閉場中の市場（7203.T・東証は取引時間外）で実測：`final_score` **7.0**・RSI **67.78**・MACD **37.0873**・確定足の終値 **3132.0** が、CSV ログ・JSON ログ・`/api/symbol/7203/summary` で完全に一致。

**ランタイム／ペネトレ（凍結署名エンジン `870be676…` に対する B.5）— 合格。** `inspect.sh` 8/8；`--version` と `/api/health` はいずれも 2.9.4；セキュリティヘッダ 6 種；2 クライアントに異なる 32 桁 hex の `xoksa_sid`；パストラバーサルは 6 パターンとも 404。**削除面を現物で確認**：`/manual/setup`・`/manual/analysis-guide`・`/manual.css`・`/images/*`・`POST /api/analysis/context-pack` はすべて **404**。**言語 SOT を現物で確認**：`/api/config` とページの `<meta name="xoksa-lang">` が一致、未知の値は **400**、`--private` セッションは **409** を返し設定ファイルは不変。

**payload 同一性。** バンドル種別マーカーは `__TAURI_BUNDLE_TYPE_VAR_UNK` のまま。署名除去後、`xoksa` と `xoksa-setup` はビルド出力と**バイト一致**、`xoksa-desktop` のみ `__LINKEDIT` の `vmsize` が **1 バイト**差。

**署名・公証 — 合格。** `Developer ID Application: Kohzoh Tsuchiya (463YK5WF6H)`、bundle と内部 3 本に hardened runtime。`notarytool` は app・DMG とも Accepted、両方 staple 済み。Gatekeeper は両方 **accepted / source=Notarized Developer ID**。

**SCA — v2.9.3 から不変。** `osv-scanner`：エンジン **0**（306 パッケージ）、`xoksa-paths` **0**（18）、WASM **3**（`event-listener` 5.4.1 は unsound のみで単一スレッドの wasm32 では到達不能、加えて未保守のビルド時 proc-macro `paste`・`proc-macro-error2`）、`xoksa-desktop` **17**・`xoksa-setup` **17**（macOS の WKWebView バイナリにはコンパイルされない gtk-rs 0.18 の Linux バインディングと、未保守のビルド時依存）。

**SBOM — 再生成。** `sbom/xoksa-native-macos-arm64.cdx.json`：CycloneDX 1.5・**209 `library` コンポーネント**（v2.9.3 は 212。`pulldown-cmark` とその依存 2 件が、バイナリ内マニュアルとともに消えた）、絶対パスの混入なし。**`cargo audit bin`** は埋め込み **225** 依存に対し **脆弱性 0**。`--target-dir target-sbom` の使い捨てツリーは削除し、出荷バイナリには触れていない。

**VirusTotal（出荷ハッシュに対して・2026-08-21）— 4 本ともクリーン。** DMG `a26b1118…` **0/60**；エンジン `870be676…` **0/62**；デスクトップ `5a44dbad…` **0/61**；設定アプリ `bca912c2…` **0/63**。

**OWASP ZAP パッシブスキャン（別ホストから LAN 経由・凍結現物に対して）— 8 件：High 0 / Medium 1 / Low 1 / Info 6。** すべて v2.9.3 で文書化済みのベースラインで、本ビルドに対して再測定した：

- **`/login` に Anti-CSRF トークンが無い（Medium）— 再測定のうえ受容。** 外部 Origin からの誤トークン POST は **401**。256bit のトークンを既に持つ者だけがセッションを得る（**303**）が、その者はエンジンに直接到達できる。セッション cookie は `HttpOnly` ＋ `SameSite=Strict`。
- **`POST /auth` の Cookie ポイズニング（Info）— 仕様どおり。** `token` パラメータが `Set-Cookie` に影響するために発火する。これは文書化された交換そのもの（トークンを提示し、`HttpOnly`／`SameSite=Strict` の cookie を受け取る）で、現物で確認済み。
- WASM バンドルの SRI 欠落（Low）——同一オリジンのハッシュ付き資産で `script-src 'self'` 配下、CDN 不使用。Unix タイムスタンプの露見 ×4 ——設計どおりの市場データ時刻。セッション管理レスポンスの特定・モダン Web アプリケーション・疑わしいコメント（`wasm-bindgen` 生成の glue であってプロジェクトのコードではない）。
- **localStorage の露見 — 6 キーから 5 キーに減少。** `xoksa.theme`／`xoksa.font`／`xoksa.symbol`／`xoksa.tickers`／`xoksa.targets`、いずれもクラス C。言語は本リリースでこの一覧から外れた——言語は設定であり、設定は `xoksa.env` にあるべきだからである。

**Windows** — 2.9.4 の MSI は Windows 側で再ビルド・検査され、その記録は Windows 側のものである。

### v2.9.3（macOS：2026-08-21 検査）— 合格（静的／SOT／ランタイム／ペネトレ／SCA／SBOM／署名・公証／VirusTotal／ZAP）

v2.9.3 は、v2.9.2 のビルドを起動して露呈した問題を直したものである。デスクトップのローカル自動起動は固定ポートを奪い合うのではなく **OS にエフェメラルポートを求め**（`:0` に bind → 採番 → 解放）、WebView を接続する前に `/api/health` が `xoksa-web` として**このアプリと完全一致する版数**を名乗ることを要求し、自動起動の失敗は**原因つきで**報告する（起動失敗・子の即時終了・ポート占有と占有者・HTTP 401 と到達不能の区別）——素のタイムアウトにしない。LAN アクセス有効時はリモート端末が頼る固定ポートを維持し、spawn 前に占有を検査する。この規則は [security-design.md §6](./security-design.md) に設計契約として明文化された（*ローカル自動起動は自分以外のエンジンを掴まない*）。設定アプリの LAN 警告と接続画面のトークン案内も、v2.8.0 で出荷済みの fail-closed 動作に合わせて修正された。

**両方の修正を、凍結した署名済みビルドに対して macOS で検証した**——元の不具合を踏んだプラットフォームである：

| 条件 | 結果 |
| :--- | :--- |
| LAN 有効・居残りの `xoksa serve` が `127.0.0.1:8787` を占有 | **即座に**占有者を名指しして拒否：「ポート 8787 は別の xoksa エンジン（v2.9.3）が使用中です。」（v2.9.2 は 10 秒後に素のタイムアウトだった） |
| LAN 無効・同じポートを占有したまま | 子エンジンはエフェメラル **50569** で起動、8787 の居残りは**無傷**（`/api/health` は 200 のまま）、ダッシュボードが開いた |

**ビルドの出所 — ソースコミット `3762eed`（ブランチ `release/v2.9.3`、`main` `05f280e` から分岐）、ビルド時点で作業ツリーは clean。** 手順は v2.9.2 と同一：`trunk build --release` → `cargo build --release --bin xoksa` → `scripts/bundle-engine.sh` → `xoksa-setup` を 2 本目の sidecar へ → `APPLE_SIGNING_IDENTITY` 付きの `cargo tauri build`。`rustc 1.96.0`・host `aarch64-apple-darwin`。

**凍結した macOS 現物**（署名・公証・staple 済み）：

| 成果物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| `XOKSA-2.9.3-arm64.dmg`（配布物） | `206653bdb93a8c7b5c2feb1937bef063144b8944a7e5407c0e6427a4be2397f8` | 11,463,321 B |
| `XOKSA.app/Contents/MacOS/xoksa`（エンジン） | `a90ee619e5ed1a33c0ec9c44bc8e021a938781cf462ceee0754834e22d25c7e9` | 10,865,808 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop` | `ea3d71162976623cc45cdc4e59c5b7bf33a45213c779fadf5428789c3bbd0728` | 6,411,776 B |
| `XOKSA.app/Contents/MacOS/xoksa-setup`（設定アプリ） | `a913c133a3a15df302c957519327d4a82372bb545a63d7be5a06a39ddb826548` | 5,946,992 B |

**静的 — 合格。** `check.sh` の 15 ゲートすべて green（エンジン・`webui-leptos`（wasm32）・`xoksa-desktop`・`xoksa-paths`）、`cargo test` **294 passed / 0 failed / 1 ignored**（lib 283 ＋ 統合 10 ＋ `xoksa-paths` 1。ignored 1 は実 Stooq フェイルオーバ e2e）。

**SOT — 合格。** `latest_observed_price` 311.3・`final_score` −4.0 が csv／json／`/api/symbol/AAPL/summary` で一致。

**ランタイム／ペネトレ（凍結した署名済みエンジン `a90ee619…` への B.5・`serve --private`）— 合格。** `inspect.sh` 8/8、`--version` と `/api/health` はともに 2.9.3、セキュリティヘッダ 6 種、K1 で異なる 32 桁 hex の `xoksa_sid` 2 つ、パストラバーサル 6 パターンすべて 404。

**payload 同一性。** バンドル種別マーカーは `__TAURI_BUNDLE_TYPE_VAR_UNK` のまま。署名を除去すると 3 本とも、差分は `__LINKEDIT` の `vmsize` の **1 バイト**のみ。

**署名／公証 — 合格。** `Developer ID Application: Kohzoh Tsuchiya (463YK5WF6H)`、バンドルと内側 3 本すべてに hardened runtime。`notarytool` は app・DMG とも Accepted、双方 staple 済み。Gatekeeper は両方とも **accepted・source=Notarized Developer ID**。

**SCA — 出荷現物に exploitable 0。v2.9.2 から変化なし。** `osv-scanner`：エンジン **0**（309 パッケージ）、`xoksa-paths` **0**（18）、WASM 3、`xoksa-desktop` 17、`xoksa-setup` 17。WASM の 3 件は `paste` / `proc-macro-error2`（未メンテのビルド時 proc-macro）と `event-listener` 5.4.1（RUSTSEC-2026-0221・*unsound*。単一スレッドの `wasm32-unknown-unknown` ビルドでは到達しない。5.4.2 で修正済みのため、以降の版での lockfile 更新として扱う）。デスクトップ／設定アプリの 17 件は gtk-rs 0.18 の Linux バインディング（macOS の WKWebView バイナリには非搭載）と未メンテのビルド時依存。

**SBOM — 再生成。依存セットは不変。** `sbom/xoksa-native-macos-arm64.cdx.json`：CycloneDX 1.5・**`library` 212 件**・絶対パス混入なし。v2.9.3 はデスクトップ／設定アプリのコードのみを触るため、`serialNumber` を除いて v2.9.2 のインベントリと内容は同一である。**`cargo audit bin`** は埋め込み **228 依存**に対し **0 件**。`--target-dir target-sbom` の使い捨ては削除し、出荷用バイナリには触れていない。

**VirusTotal（出荷ハッシュに対して・2026-08-21）— 4 本ともクリーン。** DMG `206653bd…` **0/59**、エンジン `a90ee619…` **0/62**、デスクトップ `ea3d7116…` **0/63**、設定アプリ `a913c133…` **0/63**。

**OWASP ZAP パッシブスキャン（別ホストから LAN 経由・`192.168.0.3:8787`・アクセストークン有効）— 8 アラート：High 0 / Medium 2 / Low 1 / Info 5。** うち 6 件は文書化済みのベースライン（WASM バンドルの SRI 欠落＝同一オリジン資産のみで CDN 不使用、`/api/symbol/…/summary` の Unix タイムスタンプ露見 ×4＝設計上の市場データ時刻、セッション管理レスポンスの特定・Modern Web App・localStorage ×6＝UI 状態のみで鍵はサーバ側、疑わしいコメント）。**残る 2 件は新規で、いずれも本リリースで初めて ZAP に晒された v2.8.0 のログインフロー由来である：**

- **`/login` に Anti-CSRF トークンが無い（Medium）— 実測のうえ受容。** ログインフォームは `/auth` に POST する。`/auth` はオリジン検査（`reject_cross_origin_mutations`）から**意図的に除外**されている——`Referrer-Policy: no-referrer` を厳格に適用しているため、正規の同一オリジンのフォーム POST でもブラウザは `Origin: null` を送り、検査が正当なログインを弾いてしまうからである。この除外が安全なのは、当該エンドポイントが 256-bit トークン自体で守られているためで、凍結ビルドに対して実測した：**クロスサイトから誤ったトークンで `/auth` に POST すると 401** を返し、発行されるのは分析用の `xoksa_sid` cookie だけで、**認証済みセッションは作られない**。トークンを持たない攻撃者は何も達成できず、持っている攻撃者はエンジンに直接到達できるので、被害者のブラウザを経由させる利得がない。ログイン CSRF の古典的な害（被害者を*攻撃者の*アカウントにログインさせる）も、エンジンも利用者も 1 つである本構成では意味を持たない。セッション cookie は `HttpOnly` ＋ `SameSite=Strict`。
- **`POST /auth` の Cookie ポイズニング（Info）— 設計どおり。** リクエストのパラメータ（`token`）が `Set-Cookie` に影響することで発火するが、それはまさに文書化された交換そのものである——アクセストークンを一度提示し、`HttpOnly` / `SameSite=Strict` のセッション cookie を受け取る。

上記の確認中にあわせて実測：**`--private` は非ループバックのバインドを明確に拒否する**（「認証トークンを永続化できない」）。無痕跡セッションが未認証の LAN 面を開くことはあり得ない。

**Windows** — `XOKSA-2.9.3-x64.msi` は Windows 側で再ビルド済み（payload ハッシュ照合 PASS）。その検査記録は Windows 側の担当。

### v2.9.2（macOS：2026-08-21 検査）— 実測項目は合格。**検査は中断・現物は置き換え**（起動時に接続画面の欠陥を発見）

v2.9.2 系は Tauri 面を 2 つに分割した：**デスクトップアプリ**は接続シェルになり（自動起動するローカルエンジン、またはアドレス＋アクセストークンで指定するリモートエンジン）、設定はすべてパスワードで保護された**設定アプリ（`xoksa-setup`）**へ移った。設定パスの唯一の正を持つ共有クレート `xoksa-paths` も加わった。したがって出荷 `.app` は署名済みバイナリを **3 本**（エンジン・デスクトップシェル・設定アプリ）同梱する（v2.6.8 は 2 本）。

**ビルドの出所 — ソースコミット `6368308`（ブランチ `release/v2.9.2`）、ビルド時点で作業ツリーは clean。** `trunk build --release` → `cargo build --release --bin xoksa` → `scripts/bundle-engine.sh` → `xoksa-setup` を `cargo build --release` して 2 本目の sidecar へ配置 → `APPLE_SIGNING_IDENTITY` 付きの `cargo tauri build`。`rustc 1.96.0`・host `aarch64-apple-darwin`。

**凍結した macOS 現物**（署名・公証・staple 済み）：

| 成果物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| `XOKSA-2.9.2-arm64.dmg`（配布物） | `b87909ce12e0dee1b66be127d1f696ef94013fbeadf75de8c903945e5cf326e7` | 11,463,587 B |
| `XOKSA.app/Contents/MacOS/xoksa`（エンジン） | `98493aaa5f4d987faad78c0f48d5e602fff46ba387d3e3233bd87682f4bdac3c` | 10,884,016 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop` | `731fde2befa7dc81d124e06e678e70b068a4b0932f4ad07442aea4d24a5f55fa` | 6,411,760 B |
| `XOKSA.app/Contents/MacOS/xoksa-setup`（設定アプリ） | `fa9ad8323d4c749a40ca3291874dfde663bbd3839229275e2ddbfbc10601278a` | 5,946,992 B |

**静的 — 合格。** `check.sh` の 15 ゲートすべて green：エンジン・`webui-leptos`（wasm32）・`xoksa-desktop`・`xoksa-paths` の `cargo fmt --check` と `clippy -D warnings`、`cargo test` **294 passed / 0 failed / 1 ignored**（lib 283 ＋ 統合 10 ＋ `xoksa-paths` 1。ignored 1 は実 Stooq フェイルオーバ e2e で外部 anti-bot による遮断）、`cargo audit` ×2・`cargo deny` ×2・`cargo machete` クリーン。

**SOT — 合格。** `latest_observed_price` 311.3・`final_score` −4.0 が `--log-format csv`／`--log-format json`／`/api/symbol/AAPL/summary` の三者で一致。

**ランタイム／ペネトレ（凍結した署名済みエンジン `98493aaa…` への B.5・`serve --private`）— 合格。** `inspect.sh` 8/8（埋め込み UI 200・CSP 有・クロスオリジン POST 403・64KiB 超 body 413・`text/plain` 415・同一オリジン POST 200・CLI 分析 非空 exit 0・backtest API 有効 JSON）。追加実測：`--version` と `/api/health` がともに 2.9.2、セキュリティヘッダ 6 種、K1 で異なる 32 桁 hex の `xoksa_sid` 2 つ、パストラバーサル 6 パターンすべて 404。

**payload 同一性 — 出荷バイナリとビルド物の差はコード署名だけ。** Tauri のバンドラは macOS でバンドル種別マーカーをパッチせず（双方とも `__TAURI_BUNDLE_TYPE_VAR_UNK`）、署名を除去すると 3 本とも差分は **1 バイト**、`__LINKEDIT` の `vmsize`（署名サイズに従属する値）のみである。

**署名／公証 — 合格。** `Developer ID Application: Kohzoh Tsuchiya (463YK5WF6H)`、バンドルと内側 3 本すべてに hardened runtime。`notarytool` は app・DMG とも Accepted、双方 staple 済み（`stapler validate` ok）。Gatekeeper は `.app`・DMG とも `spctl -a` が **accepted・source=Notarized Developer ID**。

**SCA — 出荷現物に exploitable 0。** `osv-scanner` を 5 つの lockfile に：エンジン **0**（309 パッケージ）、`xoksa-paths` **0**（18）、WASM フロントエンド 3、`xoksa-desktop` 17、`xoksa-setup` 17。WASM の 3 件は `paste` と `proc-macro-error2`（未メンテのビルド時 proc-macro）に加え、**`event-listener` 5.4.1・RUSTSEC-2026-0221** — `StackSlot` が無条件に `Send`/`Sync` を実装するという *unsound* の助言で、**出荷 WASM では到達しない**（単一スレッドの `wasm32-unknown-unknown` ビルドであり、越えるべきスレッド境界が存在しない）。5.4.2 で修正済みのため、本現物の修正ではなく次版の lockfile 更新として扱う。デスクトップと設定アプリの 17 件は従来リリースと同一集合：gtk-rs 0.18 バインディング 11 件（Linux WebKitGTK backend であり **macOS の WKWebView バイナリには非搭載**。唯一の Medium `glib`・RUSTSEC-2024-0429 を含む）、`unic-*` 5 件、`proc-macro-error` 1 件（未メンテのビルド時／Unicode 依存）。

**SBOM — 本ホストで再生成。** `sbom/xoksa-native-macos-arm64.cdx.json`：**CycloneDX 1.5・`library` コンポーネント 212 件**（v2.6.8 は 195 件＝v2.9.2 の依存セット）、インベントリに絶対パスの混入なし。**現物ベースの `cargo audit bin`** は埋め込み **228 依存**に対し **0 件**。使い捨ての `--target-dir target-sbom` は削除し、出荷用の `target.nosync/release` のバイナリには触れていない。

**VirusTotal（出荷ハッシュに対して・2026-08-21）— 4 本ともクリーン。** DMG `b87909ce…` **0/60**、エンジン `98493aaa…` **0/63**、デスクトップ `731fde2b…` **0/62**、設定アプリ `fa9ad832…` **0/63**。新規追加の `xoksa-setup` も初回スキャンからクリーンで、macOS の傾向（最初から Developer ID 署名＋公証が効く）と整合する。Windows デスクトップがアイコン符号化の作り込みを要した経緯とは対照的である（§C.4 の v2.6.7）。

**検査は中断。これらの現物は置き換わる。** 署名済みビルドを起動したところ、新しい接続画面に欠陥が見つかった（下記）。修正にはコード変更が要るため上記のバイナリは作り直して再検査することになり、本記録は**出荷しなかった**ビルドの測定値として残す。見つかった内容：

1. **ローカル自動起動が、原因を示さないメッセージで失敗する。** 別のエンジンが既にポートを掴んでいる状態（実測：`127.0.0.1:8787` で稼働中のインスタンス）では、起動した子プロセスが bind できず ready にならないが、利用者に返るのは 10 秒待った後の `the local engine did not become ready in time` だけである。要因は 3 つ：`spawn_local_engine` は `spawn()` の成功をもって成功とし子の即死を確認しない、`engine_reachable` が bool を返すため **HTTP 401 と到達不可を区別できない**、そしてポートが空いているかの事前確認が無い。
2. **v2.6.4 の堅牢化が自動起動の経路から失われている。** 同リリースは、居残りエンジンをアプリ自身のものと取り違えないために、デスクトップを**エフェメラルポート**＋`/api/health` の同一性確認へ移した。v2.9.2 の接続画面は固定・可視の `host:port`（既定 `8787`）を使い、その確認が無い。明示アドレスはリモートエンジン対応の意図的な帰結だが、*ローカル*経路での居残り対策の喪失はそうではなく、今回の失敗はまさにそこを踏んだ。
3. **本書の 2 箇所が、いまだエフェメラルポートを現行の挙動として記述している**（§C.4 の v2.6.4 の行と、v2.6.8 macOS の起動確認の記述）。現在の実装と矛盾する。
4. **設定アプリの LAN 警告が、エンジンより 1 世代古い。** 非ループバックのバインドには*ログインが無く*、到達できる者は誰でも設定済みの LLM を使えデータを閲覧できる、と利用者に伝えている。しかし v2.8.0 で非ループバックの `serve` は **fail-closed** になり、256-bit の `SERVE_AUTH_TOKEN` を自動生成して未認証のリクエストを拒否する（実測：ポートを占有していたエンジンは `/api/health` に `authentication required` を返した）。この文言は安全側ではなく警戒側に誤っているとはいえ、§4 と矛盾し、実際には認証済みの機能から利用者を遠ざける。日本語の文字列のみが見つかり、対応する英語表現は見当たらなかったため、言語同期も崩れている可能性がある。

**未実測。** **OWASP ZAP パッシブスキャン** — 本ビルドでは未実施。**Windows** — ビルドと検査は Windows 側の担当で、PR #137 で追跡。

### v2.6.8（macOS：2026-07-27 検査／Windows：2026-07-27 再検査）— 両 OS とも合格
v2.6.8 は、v2.6.7 の出荷検査に続けて行った**セキュリティ・設計文書とソースの 1 行ずつの突き合わせ**が、文書の誤りではなく**実際の欠陥**を見つけたために存在する：ダッシュボードからアラートルールを追加すると `ALERT_<n>_*` が**無条件で** `xoksa.env` に追記され、`--private` セッションでも利用者の監視銘柄と閾値がディスクに書かれていた。`--private` は無痕跡セッションとして4 か所（本書 §4・design-philosophy §8.2・README・使い方ガイド）に明記されており、ログと保存戦略の経路は守っていたが、アラートの経路だけが守っていなかった。`persist_add_to_env`／`persist_remove_from_env` を `is_private()` で早期リターンさせて修正し（ルールはセッション中は従来どおり動作する）、両方向を検証する回帰テストを追加した。チャンネルの秘密は OS キーチェーン管理のため元から影響なし。同リリースでは、`unsafe` を含まないがコンパイラ強制がなかったデスクトップクレートに `#![forbid(unsafe_code)]` を追加し、突き合わせで見つかった 4 件の記載乖離（通知許可リストからの `discordapp.com` の欠落、サブコマンド列挙からの `test-notify` の欠落、design-philosophy §8.2 の「ディスク書き込みはこれだけ」という記述、§8.3 の「全一覧」から漏れていた `#[cfg]` 継ぎ目 4 件）を修正した。

**ビルドの出所。** ブランチ `xoksa-dev-v2.6.8`（`origin/main` `1879aae` 起点）でビルドしており、v2.6.8 の作業中に入った `serde_json` 1.0.151 の更新（dependabot #104）を含む。この rebase 前に作った v2.6.8 のビルドは**破棄して作り直した**——WASM フロントエンドが `serde_json` 1.0.150 でビルドされており、マージ後のソースから再現できないためである。以下は作り直した再現可能な現物である。破棄したビルドのハッシュは意図的に記録しない（出荷物ではなかったため）。

**凍結した macOS 現物**（署名・公証・staple 済み）：

| 成果物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| `XOKSA-2.6.8-arm64.dmg`（配布物） | `c7a225ae246f159633fb836683d1b0d7bf4cab6eee4c47dfb916f5691e4dc25d` | 8,425,005 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop` | `ea6e8f5b69e45a9895c972846948e9b4706f6d62d3bd6af13e784dafff4d40be` | 6,510,688 B |
| `XOKSA.app/Contents/MacOS/xoksa`（engine） | `6efc1559ff57be2e9cac9b90757ad7ea0cd7e9db5f605fd65b916ed5d09b2336` | 10,387,312 B |
| `xoksa-macos-arm64.zip`（CLI・エンジンはバイト同一） | `1c1c979ab28bfa988ff4f5418dfc2553cce1d0c538ba36b8461cec11cd53dc99` | — |

**新しい現物に対して全項目を再実測 — 合格。** **Static** — `check.sh` の 12 ゲートすべて green、`cargo test` **266 passed / 0 failed / 1 ignored**（lib 256 ＋ 統合 10。増えた 1 件は `--private` アラートの回帰テスト）。**payload 同一性** — v2.6.7 の所見を再現：バンドル種別マーカーは `__TAURI_BUNDLE_TYPE_VAR_UNK` のままで、署名を除去するとビルド物と出荷物の差は `__LINKEDIT` の `vmsize` 1〜2 バイトのみ。**署名・公証** — app（`73794b25…`）・DMG（`1c6fa7af…`）・CLI zip（`1ce67222…`）とも `notarytool` Accepted、app と DMG は staple 済み、Gatekeeper は `accepted / Notarized Developer ID`。**SOT** — `latest_observed_price` 333.02・`final_score` 1.0 が csv／json／`/api/symbol/AAPL/summary` で一致。**Runtime／ペネトレ** — 凍結した署名済みエンジンに対し `inspect.sh` 8/8、`/api/health` は 2.6.8、セキュリティヘッダ 6 種、K1 で異なる 32 桁 hex の sid 2 つ、パストラバーサル 6 パターンすべて 404。**SCA** — v2.6.7 から変化なし（native 0／WASM は未メンテのみ 2／desktop 17・唯一の Medium は Linux 専用 `glib`）。**SBOM** — `sbom/xoksa-native-macos-arm64.cdx.json` を本ホストで再生成（195 コンポーネント・絶対パス混入なし）。**現物ベースの `cargo audit bin`** は埋め込み 210 依存に対し 0 件。**VirusTotal（出荷ハッシュ）** — DMG **0/61**・desktop **0/62**・engine **0/60**、いずれもクリーン。

**凍結 Windows 現物**（`cargo build --release`・既定 `embedded-ui`／MSI は `build-msi.ps1`／`rustc 1.96.0`・host `x86_64-pc-windows-msvc`・LLVM 22.1.2）：

| 成果物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| `xoksa.exe`（Windows x86_64・engine） | `fad0a1acf654df411578073799bb11499c9fa9e33f1078eb25565375082883f1` | 13,065,728 B |
| `xoksa-desktop.exe`（Windows x86_64） | `3abe6aefbad79a1754ee1cb11b39c25b4fb901b19328fb36315c07256d4a728a` | 9,904,128 B |
| `XOKSA-2.6.8-x64.msi`（インストーラ・上記 2 本を同梱） | `fba768d2248c84acf50138a199ec1dde708ddca7aceff10bac5771ec394bfdfe` | 8,327,168 B |

**v2.6.7 の Windows 現物は単に古いのではなく置き換え対象** — ここで直した `--private` アラート欠陥を含むため、`dist/v2.6.7/` は DO-NOT-SHIP を明示し、v2.6.7 リリースは削除のまま（タグは残置）。

**新しい Windows 現物で全項目を再実測 — 合格。** **`--private` 修正・現物ベース** — `persist_add_to_env` は `is_private()` で早期リターン、回帰テスト（`private path is a no-op`）は下記 266 テストに含まれ、稼働中の `serve --private` エンジンで周辺の B.5 ゲートも実測。**レシピ維持** — desktop の `.rsrc` は再び **32,768 B @ entropy 3.982**（6 sections）、バンドルマーカーは `__TAURI_BUNDLE_TYPE_VAR_UNK`（MSI パッチ回避）、アイコンの PNG フレーム 0、バージョンメタデータ 4 項目完備。`build-msi.ps1` の payload ハッシュ照合を通過＝MSI は `3abe6aef…` と `fad0a1ac…` をそのまま同梱。**Static** — `check.sh` 全ゲート緑（`cargo-deny` 0.20.2）、`cargo test` **266 passed / 0 failed / 1 ignored**。**SOT** — `latest_observed_price` 333.02・`final_score` 12.0 が csv／json／`/api/symbol/AAPL/summary` の三者で一致。**Runtime／ペネトレ（`serve --private` への B.5）** — `/api/health` は 2.6.8、`unsafe-inline` なしの nonce-CSP、`xoksa_sid` HttpOnly/SameSite=Strict、K1 で 32 桁 hex の異なる sid 2 つ、パストラバーサル 6 パターン 404、Content-Type／クロスオリジン POST／クロスオリジン GET が 415／403／200、5MB ボディ 413、`/api/backtest` 200・正当 JSON、H1/H2 ガード合格。**公開モード** — `--host 0.0.0.0` に対し LAN IP 経由で同じコントロールを再実測し同一に成立。**SCA** — `osv-scanner` ×3 が記録済み件数を再現（native 0／WASM 未メンテのみ 2／desktop 17・唯一の Medium は Linux 専用 `glib`）、`cargo audit`（source）0、**現物埋め込みの `cargo audit bin`**（220 依存）で脆弱性 0。**SBOM** — `sbom/xoksa-native-windows-x86_64.cdx.json` を本ホストで再生成（199 コンポーネント・絶対パスなし）、依存セットは **v2.6.7 と同一**（差はルート自身の版数 `xoksa` 2.6.7→2.6.8 のみ）。**OWASP ZAP パッシブ（Parrot ホスト・LAN）** — 6 アラート（High 0／Medium 1 SRI 欠落／Low 1 Unix タイムスタンプ／Info 4）で**件数・分類ともベースラインと一致**、新規なし。**VirusTotal（出荷ハッシュ）** — MSI **0/62**・engine **0/69**・desktop **0/70** で全クリーン。*ただし再検査者に重要なので注意書きを残す*：desktop 現物は**初回アップロードで 1/70**（Microsoft `Trojan:Win32/Wacatac.C!ml`）となり、**ファイルを一切変えずに Reanalyze すると 0/70** に解消した。2 つの対照が原因をファイルではなくレピュテーション軸に置いた：v2.6.7 desktop（`5d267218…`）を同じ日に再スキャンしても 0/70 のまま（レシピに対するモデルドリフトはない）で、変えていない v2.6.8 現物の Reanalyze が判定を撤回した。これは新規・未署名ハッシュへの初見ペナルティで、機序と配布上の帰結は [av-false-positive-case-study.md §6.2](./av-false-positive-case-study.md) に記す。**新規ビルドの初回スキャンでは、レピュテーションが蓄積するまで一過性で再び出うる。** MSI のインストール／起動／アンインストールは v2.6.8 では再実行していない（昇格が必要）。ツール（`build-msi.ps1`・WiX）は上で実測した v2.6.7 の実行から不変で、payload はハッシュ照合済み・スキャンもクリーン。

### v2.6.7（Windows：2026-07-23 検査・2026-07-24 再検査／macOS：2026-07-27 検査）— 合格（ソース／動的／SBOM／ZAP。VirusTotal は出荷現物すべてクリーン：Windows engine **0/69**・desktop **0/70**・MSI **0/62**／macOS DMG **0/61**・desktop **0/62**・engine **0/62**）
v2.6.7 は `keyring` 4.0 の既定機能が引き込んでいた turso ベースの組込 SQL DB を除去（No-Database 復元・`Cargo.lock` 約 −3000 行・native SBOM 337→200 コンポーネント・バイナリ 28.7MB→13.1MB）、貼付 webhook 秘密を平文で `xoksa.env` に書き得た通知設定プロンプトを修正、Windows デスクトップのチャート/HELP ポップアップ白紙（同期 `#[tauri::command]` が WebView2 コントローラ初期化をデッドロック → `async` 化で Windows/macOS を単一経路に統一）を修正、`--debug-prompt`（プロンプトをファイルに書く）と `--no-llm`（LLM 送信を止める）を独立オプションに分離、engine 起動時の Windows コンソール窓を抑止。

**凍結ローカル現物**（`cargo build --release`・既定 `embedded-ui`）：

| 成果物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| `xoksa.exe`（Windows x86_64・engine） | `a7d59038b93050fb49d6cd879bca69ab4e655d36362721caa6d719e806da7ca1` | 13,058,048 B |
| `xoksa-desktop.exe`（Windows x86_64） | `5d2672185c1d3461cb6cad5c297a3ccadf6ac6c96045a96a85843bb594e92dee` | 9,904,128 B |
| `XOKSA-2.6.7-x64.msi`（Windows インストーラ・上記 2 本を同梱） | `a7f978382e725b6e1f285c81c0bd9366d7b7dcd0cb5fce5ae4d26cbbd491fa73` | 8,327,168 B |

**静的 — 合格。** `cargo fmt --check`（差分なし）／`cargo clippy -- -D warnings`＝0（3ワークスペース）／`cargo test`＝266 合格（lib 256＋統合 10）0 失敗／`cargo audit`＝native 288 依存で 0 件／`cargo deny check`＝advisories/bans/licenses/sources ok／`cargo machete`＝未使用なし。

**SOT — 合格。** csv == json == api の出力等価（AAPL：`latest_observed_price` 325.89・`final_score` 13.0 が `--stdout-log --log-format csv`／`--log-format json`／`/api/symbol/AAPL/summary` の3経路で一致）。

**ランタイム／ペネトレ（`xoksa serve --private` への B.5 playbook）— 合格。** `--version` 2.6.7／`/api/health` 200／per-request nonce の strict CSP・`unsafe-inline` なし（`object-src`/`base-uri`/`frame-ancestors 'none'`）／`xoksa_sid` HttpOnly・SameSite=Strict／K1 クロスクライアント `sid` 分離（異なる 32-hex ×2）／パストラバーサル 全404（6種）／CSRF・Content-Type・過大 body 403/415/413／CLI 分析 非空 exit 0／`/api/backtest` 有効 JSON／H1 プロンプトインジェクション（`news_title_is_single_line_no_forged_markers`）・H2 全プロバイダ出力整合ガード（`output_guard_applies_to_every_provider_and_respects_flag`）テスト合格。K3 容量 shed は admission semaphore（`MAX_CONCURRENT_CONNECTIONS = u16::MAX`）＝構造的に存在。

**SCA（正＝`osv-scanner` を Cargo.lock 3本＋`cargo audit`/`cargo deny`）— 出荷現物に exploitable 0。** native engine：**0**。WASM フロントエンド：未メンテのみ 2 件（`paste`・`proc-macro-error2`＝ビルド時 proc-macro・出荷 WASM 非搭載）。デスクトップ（Windows）：17 件 — gtk-rs 0.18 バインディング 11 件（`atk`/`gdk`/`glib`/`gtk`…）は **Linux WebKitGTK backend で Windows WebView2 バイナリに非搭載**、`unic-*` 5 件と `proc-macro-error` 1 件は未メンテのビルド時/Unicode 依存。唯一の Medium（`glib`・RUSTSEC-2024-0429）はその Linux 専用バインディングで Windows 現物に不在。

**SBOM — 再生成（CycloneDX・`cargo auditable build`→`syft file:`・パスクリーン）。** `sbom/xoksa-native-windows-x86_64.cdx.json`（macOS 分の追加に伴い `xoksa-native.cdx.json` から改名）は in-binary **199** コンポーネント（v2.2.3 の 337 から減＝turso 除去）、`sbom/xoksa-webui-leptos.cdx.json` は依存集合が不変（ソースのみの変更）。

**露出モード（`--host 0.0.0.0`）— 実測（未検査ではない）。** LAN IP 経由で B.5 制御を再実行し同一に機能：per-request nonce の strict CSP＋`xoksa_sid`、パストラバーサル 404、CSRF はリクエスト自身の `Host` 一致判定（同一オリジン LAN の `text/plain` POST→415・GET→200・クロスオリジン POST→403）。差分は文書化済みの無認証（公開運用時の認証/TLS/レート制限は運用者責任＝リバースプロキシ）のみ。

**OWASP ZAP パッシブスキャン（Parrot ホスト・`0.0.0.0` LAN 経由）— 6 アラート（Info 4・Low 1・Medium 1）、いずれも既認容 / not-applicable**（単一ユーザ・同一オリジン・CDN なし・DB なし構成）：SRI 欠落（Medium＝同一オリジン資産のみ・クロスオリジン CDN なし）、Unix タイムスタンプ露見（Low＝設計上の市場データ時刻）、セッション cookie 特定・Modern Web App・localStorage（UI 状態のみ・鍵はサーバ側）・疑わしいコメント（すべて Info）。文書化済みベースラインに対し新規アラートなし。

**再検査（2026-07-24）— デスクトップ現物が変わったため全項目を再実測、すべて合格。** 誤検知対策でデスクトップのアイコンを再符号化したため desktop 現物は新規（`5d267218…`）。一方 **engine 現物は 07-23 に検査したものとビット単位で同一**（`a7d59038…`・前後でハッシュ照合済）であり、engine に紐づく結果は検査済みハッシュに紐づいたまま維持される。検査はすべてその現物に対し dev プロファイルで実施し、**リリース現物には触れていない**（実施後にハッシュ再照合）。**Static** — `cargo fmt --check` は 3 workspace とも差分なし、`clippy -D warnings` も 3 workspace とも 0、`cargo test` は **265 passed / 0 failed / 1 ignored**（lib 255 ＋ integration 10。ignored 1 は実 Stooq フェイルオーバ e2e で外部 anti-bot による遮断＝欠陥ではない）。**SCA** — `cargo audit` は 1169 advisories に対し 288 依存を走査し exit 0、`cargo deny check` は advisories/bans/licenses/sources ok、`cargo machete` は未使用依存なし、`osv-scanner` は 3 lockfile とも上記の件数を完全に再現（native 0／WASM は未メンテのみ 2／desktop 17・唯一の Medium は Linux 専用 `glib` バインディング）。**SBOM** — `Cargo.lock` 3 本に差分がないため、公開済み SBOM は出荷依存セットに紐づいたまま有効。**SOT** — `latest_observed_price` 321.66・`final_score` 10.0 が `--log-format csv`／`--log-format json`／`/api/symbol/AAPL/summary` の三者で一致。**Runtime／ペネトレ（B.5）** — `--version` 2.6.7／`/api/health` 200／リクエスト毎 nonce の strict CSP（`unsafe-inline` なし）／`xoksa_sid` HttpOnly・SameSite=Strict／K1 で 32桁 hex の異なる sid 2 つ／パストラバーサル 6 パターンすべて 404／Content-Type・クロスオリジン POST・クロスオリジン GET の各ゲートが 415／403／200／5MB ボディで 413／`/api/backtest` POST 200 かつ正当な JSON／CLI 解析 exit 0／H1・H2 のガードテスト合格。**公開モード** — 同じコントロールを `--host 0.0.0.0` に対し LAN IP 経由で再実測し、同一に成立。**OWASP ZAP パッシブ（Parrot ホスト・LAN）** — **6 アラート・High 0／Medium 1／Low 1／Info 4 で、07-23 のベースラインと件数・分類ともに完全一致**。新規アラートなし＝アイコン変更が Web 層に一切影響していないことを実測で確認。欠陥ではない申し送り：`deny.toml` に `RUSTSEC-2026-0173`（`proc-macro-error2`）の ignore が残っているが、当該 crate は既に依存ツリーに存在しない（"no crate matched advisory criteria"）。

**MSI 配布（2026-07-24 追加）— インストーラと同梱物 2 本がすべてクリーン。インストール／起動／アンインストールを実測。** exe 2 本を利用者が同じ場所に置く必要がある構成は導入時の事故要因（デスクトップはエンジンを**同一ディレクトリの隣**として解決する）なので、v2.6.7 に Windows インストーラを追加した。ただし **`cargo tauri build` では作らない**：Tauri のバンドラはデスクトップバイナリに 3 バイトのマーカーをパッチし（`BUNDLE_TYPE_VAR_UNK` → `…_MSI`）、**その ASCII 3 文字だけで** Microsoft が `Trojan:Win32/Wacatac.B!ml` に転じる（他はバイト単位で同一のクリーンビルドとの比較で実測。ケーススタディ §4.3）。よって出荷 MSI は [`xoksa-desktop/build-msi.ps1`](../../xoksa-desktop/build-msi.ps1) で生成する：`cargo build --release` でビルドし、Tauri には WiX ソースだけ生成させ、**パッチされていない**バイナリを置いた状態で WiX `light` を再実行、そして**梱包された payload がビルド物とハッシュ一致しなければビルドを失敗させる**。結果 — MSI `a7f97838…` は **0/62・clean**、同梱物は `5d267218…`（0/70）と `a7d59038…`（0/69）で**検査済み現物とバイト単位で同一**＝利用者がインストールするものは検査したものそのもの。実測したインストール結果：per-machine で `C:\Program Files\XOKSA\` に 2 本を同一ディレクトリ配置（隣接解決の要件を満たす）、スタートメニューとアンインストールのショートカット、アンインストール用レジストリ登録（`XOKSA` / 2.6.7 / `kozo2000`）。インストール版からエンジンが起動し、`/api/health` は 2.6.7、ループバック UI は検査時と同じ nonce CSP と `xoksa_sid` cookie を返す。アンインストールは、インストールした全ファイル・ショートカット 2 種・自身のレジストリ項目を削除し、プロセスも残さない。**削除されないもの 2 つ**を明記する（「残骸なし」では言い過ぎになるため）：`%APPDATA%\app.xoksa.desktop`（設定・ログ）は意図的に保持、そして初回起動プロンプトに応答した際に作られる **Windows ファイアウォールの許可規則**は残る — この規則を作り所有するのは Windows であり、MSI は自分がインストールしていないものを削除できない。ループバック既定ならこの問題自体が生じない：`DESKTOP_LAN_ACCESS` 未設定/false で実測したところエンジンは `127.0.0.1` にバインドし（`netstat` と、LAN アドレスが到達不可でループバックのみ応答することで確認）、ファイアウォールのプロンプトが出ないため当該規則も作られない。`%APPDATA%` の保持には 1 点、明記すべき帰結がある：API キーを `xoksa.env` 方式で保存している場合、**アンインストール後も資格情報がディスクに残る**（設計上の未決事項として記録。本ビルドの欠陥ではない）。欠陥ではない観測を 2 件記録：**単一インスタンス制御がない**（起動ごとに各自のエンジンを別エフェメラルポートで立てる）、初回起動時の Windows ファイアウォール警告は `DESKTOP_LAN_ACCESS=true` による `0.0.0.0` バインド由来（ループバック既定なら出ない）。NSIS も実測し**不採用**とした：`overlay` 構造のためコンテナ自体が 3〜4/69。

**VirusTotal（出荷ハッシュに対して）。** engine `xoksa.exe` — **0/69・clean**。desktop `5d267218…` — **0/70・clean**。署名ではなく、ML 検知の原因をビルド側で特定して到達した。インストーラ `a7f97838…` — **0/62・clean**。初回ビルド（`61987ec4…`）は 2/70 — Microsoft `Trojan:Win32/Wacatac.B!ml` と Trapmine `Malicious.moderate.ml.score`（いずれも ML ヒューリスティック）。統制された単一変数ビルドにより、**どちらのエンジンもプログラムコードには反応していない**ことを確認：両者とも PE のリソースセクション（`.rsrc` ＝ アイコン）を、*別々の*測定量で見ている。**Trapmine は `.rsrc` のエントロピー**（7.928 で flag、5.381 / 4.176 / 2.835 は clean）。**Microsoft は `.rsrc` のサイズ**（2,048 B・51,200 B は clean、297,472 B・約 351,000 B は flag。境界は挟み込みであり二分探索していない）で、加えて**実測済の必要条件が 2 つ**：PE バージョンメタデータと、engine 起動時の `CREATE_NO_WINDOW`。後者は出荷レシピから当該フラグだけを外して再ビルドし（`b80d49bb…`・`.rsrc` は出荷現物とビット単位で同一）再スキャンして確立した：**1/70・Microsoft `Program:Win32/Wacapew.C!ml`**。3 条件は同時に成立する必要があり、どれか 1 つを外せば再び flag する。したがって出荷アイコンは **非圧縮 BMP フレーム・最大 64×64** とし、`.rsrc` 32,768 B @ entropy 3.982 ＝ 両者の clean 領域の内側に置いた。バージョンメタデータは engine 同等を維持：`tauri.conf.json` の `bundle.publisher` / `bundle.copyright`（→ CompanyName / LegalCopyright）、desktop `Cargo.toml` の `[package.metadata.tauri-winres]` による `OriginalFilename`、説明的な `FileDescription` は**ビルド後に `rcedit`** で適用（tauri-build が当該テーブル読込後に `FileDescription = productName` を強制し、`build.rs` に `winresource` を足すと `RT_VERSION` が二重になりリンクエラー＝tauri #10154 のため、ソースからは設定できない）。**退行制約：** 256×256 または PNG 圧縮のアイコンフレームを戻すと再び flag する。128×128 の BMP フレームは `.rsrc` を約 98KB とし、clean 実測 51KB と flag 実測 297KB の**間＝未検証**（追加には再スキャンが必要）。**過去記述の訂正：** 本節は Trapmine 残存を「メタデータではなく挙動由来…コードでは消せない」と記載していたが、**これは誤り**。実体は `.rsrc` のエントロピーであり、ビルドで解消済み。手法・実験行列・ツールの全体は [av-false-positive-case-study.md](./av-false-positive-case-study.md)。決定打であるコード署名（§5 / §6）は引き続き見送り。

**macOS 現物（2026-07-27 実測）— 署名・公証・staple 済み。実測した全項目 合格。** 同一ソース（v2.6.7 系の `main`）からビルド：`trunk build --release` → `cargo build --release --bin xoksa` → `scripts/bundle-engine.sh` → `APPLE_SIGNING_IDENTITY` 付きの `cargo tauri build`。`rustc 1.96.0`・host `aarch64-apple-darwin`。

| 成果物 | SHA-256 | サイズ |
| :--- | :--- | :--- |
| `XOKSA.app/Contents/MacOS/xoksa`（engine・署名済み） | `39443ca8aec64c561344e8a416cbbd47d0420c3aedc3a847342d2456344b96bc` | 10,370,816 B |
| `XOKSA.app/Contents/MacOS/xoksa-desktop`（署名済み） | `c6dd20cf890e570277147b60af5f2db248c41c749ed085940da1f62706c84a0f` | 6,510,672 B |
| `XOKSA-2.6.7-arm64.dmg`（配布物・公証＋staple） | `c1ef8761180fdc596285c8edcbbb8deb5440046f6bc9933c271dde33aaed0823` | 8,430,808 B |

**payload 同一性 — 実測。Windows から持ち越した前提を訂正する。** macOS では Tauri のバンドラは**バンドル種別マーカーをパッチしない**：`cargo build` の出力と `.app` 内バイナリはどちらも `__TAURI_BUNDLE_TYPE_VAR_UNK` のまま（Windows では `…_MSI` に書き換わる）。双方から署名を除去（`codesign --remove-signature`）するとサイズは完全一致し、差分は `__LINKEDIT` セグメントの `vmsize` フィールド内の **1〜2 バイトのみ**＝署名サイズに従属する値である。**したがって出荷バイナリとビルド物の差はコード署名だけ**（署名前：engine `2c8a00da…` 10,352,576 B・desktop `e5f6f458…` 6,492,368 B）。DMG 内の実体は staging の署名済み現物とバイト単位で同一であり、staple はバイナリを変更しない。

**署名／公証 — 合格。** `Developer ID Application: Kohzoh Tsuchiya (463YK5WF6H)`・hardened runtime（`--options runtime --timestamp`）。`notarytool` は app（`d4f417da-457e-4bff-bbd9-285778187126`）・DMG（`46eecabc-d029-45bf-b61b-80b4bbf92c0b`）とも **Accepted**、両方 staple 済み（`stapler validate` ok）。Gatekeeper：DMG への `spctl -a -t open`、DMG 内 `.app` への `spctl -a -t exec` とも **accepted・source=Notarized Developer ID**。`codesign --verify --deep --strict` は valid on disk かつ Designated Requirement を満たす。

**ビルド環境の制約（次回リリースで時間を失わないよう記録）。** 本リポジトリは iCloud 管理下の `~/Documents` にあり、file provider が生成された `.app` ディレクトリに `com.apple.FinderInfo` / `com.apple.fileprovider.fpfs#P` / `com.apple.macl` を付与する。この状態で `codesign` は *"resource fork, Finder information, or similar detritus not allowed"* で中断し、その場で属性を消しても即座に付け直されるため解決しない。よってバンドルを同期ツリー外へ staging し、`xattr -cr` してからそこで署名する。DMG もその staging から作る（署名済み `.app` ＋ `/Applications` シンボリックリンクに対し `hdiutil create -format UDZO`）→ 署名 → 公証 → staple。

**静的 — 合格。** `cargo fmt --check` は 3 workspace とも差分なし／`clippy -D warnings` も 3 workspace とも 0／`cargo test` **265 passed / 0 failed / 1 ignored**（lib 255＋統合 10。ignored 1 は実 Stooq フェイルオーバ e2e）／`cargo audit` は 1169 advisories に対し 288 依存で 0／`cargo deny check` は advisories/bans/licenses/sources **ok**／`cargo machete` 未使用依存なし。ツールの申し送り：`cargo-deny` **0.18.5 は CVSS 4.0 の助言（`RUSTSEC-2026-0073`）で中断**し、0.20.2 は解釈できる。Windows と同じ housekeeping 警告（`deny.toml` の `RUSTSEC-2026-0173` ignore が "no crate matched advisory criteria"）は残存。`scripts/check.sh` は webui workspace に対し `cargo deny check --config deny.toml` を呼ぶが、cargo-deny 0.20 以降は `--config` がサブコマンド前のグローバルオプションになったため引数エラーになる。

**SOT — 合格。** AAPL（確定足 2026-07-24）で `latest_observed_price` 333.02・`final_score` 1.0 が `--stdout-log --log-format csv`（f64 全桁では `333.0199890136719`）／`--log-format json`／`/api/symbol/AAPL/summary` の三経路で一致。

**ランタイム／ペネトレ（凍結した署名済み engine `39443ca8…` への B.5 playbook・`serve --private`）— 合格。** `scripts/inspect.sh` 8/8（埋め込み UI 200・CSP 有・クロスオリジン POST 403・過大 body 413・`text/plain` 415・同一オリジン POST 200・CLI 分析 非空 exit 0・backtest API 有効 JSON）。追加実測：`/api/health` が 2.6.7／per-request nonce の CSP（`default-src 'self'`・`script-src 'self' 'wasm-unsafe-eval' 'nonce-…'`・`connect-src 'self'`・`object-src 'none'`）で `unsafe-inline` なし／位置情報・カメラ・マイク・決済・USB・モーションを拒否する Permissions-Policy／`X-Frame-Options: DENY`・`nosniff`・`Referrer-Policy: no-referrer`／`xoksa_sid` HttpOnly・SameSite=Strict／K1 で異なる 32 桁 hex の sid 2 つ／パストラバーサル 6 パターンすべて 404／クロスオリジン GET は 200（ゲート対象は POST のみ）。H1（`news_title_is_single_line_no_forged_markers`）・H2（`output_guard_applies_to_every_provider_and_respects_flag`）合格。

**SCA — Windows 記録と同一件数を本ホストで再現。** `osv-scanner` を `Cargo.lock` 3 本に：native **0**／WASM は未メンテのみ 2（`paste`・`proc-macro-error2`）／desktop 17 で、うち gtk-rs 0.18 バインディング 11 件（唯一の Medium `glib`・RUSTSEC-2024-0429 を含む）は **Linux WebKitGTK backend であり macOS の WKWebView バイナリには非搭載**。

**デスクトップ起動 — エンジンが起動し配信することを確認。** 署名済み `.app` は起動してエフェメラルポートで隣のエンジンを立ち上げ、`/api/health` の版数一致後に WebView が接続する。欠陥ではない観測 2 件（いずれも Windows で既記録）：**単一インスタンス制御がない**（2 度目の起動は自分専用のエンジンを別ポートで立てる）、およびバインド先は利用者自身の設定に従う — 本マシンでは `~/Library/Application Support/app.xoksa.desktop/xoksa.env` に `DESKTOP_LAN_ACCESS=true`（2026-07-21 設定）があるためエンジンは `0.0.0.0` にバインドした。ビルドの既定はループバック（`xoksa-desktop/src/main.rs::bind_host` は設定が `true` でない限り `HOST` を返す）であり、この LAN バインドは運用者の明示的オプトインであって現物の性質ではない。

**ポップアップ回帰（`async` 化が壊し得た唯一の挙動）— 合格。2026-07-27 に画面で確認。** チャートのポップアップは描画され（QQQ 日足、価格／VWAP／EMA と RSI・MACD・出来高ペイン）、HELP のポップアップはチャットコマンド一覧を描画する。いずれも白紙にならない。Windows 専用の注入ワークアラウンドは削除済みで、両 OS が単一の `async` 経路で動作する。

**VirusTotal（出荷 macOS ハッシュに対して・2026-07-27）— 3 本ともクリーン。** `XOKSA-2.6.7-arm64.dmg`（`c1ef8761…`）— **0/61**、署名済み DMG（`app.xoksa.desktop`）として認識。desktop `xoksa-desktop`（`c6dd20cf…`）— **0/62**、署名済み Mach-O 64-bit ARM。engine `xoksa`（`39443ca8…`）— **0/62**、署名済み Mach-O 64-bit ARM。コンテナがクリーンでも同梱物の保証にはならないため（Windows では MSI 0/62 に対し同梱物が一時 1/70）、現物ごとに個別提出した。3 本とも検知ゼロ — 0/70 に到達するのにアイコン符号化の作り込みを要した Windows の desktop とは異なり、macOS 現物は Developer ID 署名＋公証が最初から効いた状態で初回スキャンからクリーンだった。

**SBOM — OS ごとのインベントリを用意した。1 本では両方を賄えないため。** 公開済みのネイティブ SBOM は Windows host 生成で、実測すると **Windows 固有 17 crate**（`schannel`・`winapi`・`windows-sys`・`windows-registry`・**`windows-native-keyring-store`** ほか）を含み、macOS 固有は **0** だった — ソースも `Cargo.lock` も同一だが、lockfile が全ターゲットの和集合であるのに対し SBOM は*そのバイナリに入っているもの*を列挙するためである。よって `xoksa-native.cdx.json` を **`sbom/xoksa-native-windows-x86_64.cdx.json`**（199 コンポーネント）へ改名し、**`sbom/xoksa-native-macos-arm64.cdx.json`**（**195** コンポーネント）を §C.3 の方式で本ホストにて生成した — `cargo auditable build --release`（使い捨て）→ 中立ディレクトリから `syft file:` で読み取り → `cargo build --release` でクリーンな出荷バイナリを復元（確認済み：`2c8a00da…` に復帰、凍結済み `dist/v2.6.7/` の現物もハッシュ再照合で不変）。macOS 側は `security-framework` / `core-foundation` / **`apple-native-keyring-store`** を含み Windows crate は 0、出力に絶対パスの混入なし。WASM の SBOM は 1 本のまま（ターゲット `wasm32-unknown-unknown` はビルドホストに依存しないため）。

**現物ベースの SCA（新規・`cargo audit bin`）— 脆弱性 0。** 同じ使い捨て auditable バイナリに対して実行、すなわち lockfile の 288 crate 和集合ではなく **macOS エンジンに実際に埋め込まれた 210 依存**を対象とした：exit 0・助言なし。これは lockfile レベルのスキャンに対する現物束縛の対応物であり、Linux 専用の gtk-rs 系助言が出荷現物に適用され得ない理由（そもそも入っていない）を測定として示すものでもある。
