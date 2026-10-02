# Changelog

> **Language sync:** English and Japanese entries are both authoritative. If a change note appears in only one language, treat it as applying to both; the missing side is a documentation defect to be updated.

All notable changes to this project will be documented in this file.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html)

# 変更履歴

> **言語同期:** 英語と日本語の変更記録はいずれも有効です。変更内容が片方の言語にだけ存在する場合でも、両方に適用されます。欠落している側は文書不備として更新対象です。

このファイルには、本プロジェクトの重要な変更を記録します。
形式: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) に準拠します。
バージョニング: [Semantic Versioning](https://semver.org/spec/v2.0.0.html) に準拠します。

---

## [2.9.10] — unreleased

**A pass over the paths that touch a key, the network and the LLM's own output.** Every item below was measured before it was changed — on the shipped 2.9.9 binary where that was possible — and the two that turned out worse than the review that proposed them say so.

### Fixed
- **The output-integrity guard bound a figure to an indicator the claim never named.** Measured on the shipped binary across 231 cases: a number searched for the nearest *registered* indicator name and that search crossed from one claim into the next, so `RSI is 56.93 and the signal is 56.93` (false) survived while `RSI is 56.93 and the signal is 2.0764` (true) was deleted — `signal` was not in the vocabulary, so the second claim borrowed `RSI` from the first. Not an English problem: 転換ライン and 基準ライン break identically on a Japanese listing. Three changes together, because each alone reads as a regression — the binding closes to the claim (for the indicator only; instrument, bar and year legitimately look across the sentence), nine indicators gain the English concept names the comment above the table already promised, and detector 3's price-context gate is gone because every number it guarded is verified by attribution anyway. ([src/integrity.rs](src/integrity.rs), [src/llm.rs](src/llm.rs), [docs/dev-prog/security-design.md](docs/dev-prog/security-design.md))
- **`Zeroizing` wiped the buffer a `String` ended with, not the ones it abandoned on the way.** Three places built key material by growing a string from empty, so every reallocation freed the previous buffer with the secret still in it. The worst is `random_token_256`, which is the moment `SERVE_AUTH_TOKEN` is born: the raw 32 bytes left scope unzeroized and 32 `format!` temporaries carried the hex. Also the interactive key prompt and the keychain migration path, where the intermediate holds **other** keys' values. ([src/keystore.rs](src/keystore.rs), [src/setup.rs](src/setup.rs))
- **Redirects were followed blindly, and `reqwest` does not strip `x-api-key`.** Twelve client builders named no redirect policy, so every call ran on the default of up to ten hops — which takes the notification host allowlist off at the first 3xx (OWASP API7). Worse, measured in `reqwest` 0.13.5 (`redirect.rs:239`): a cross-host redirect removes `AUTHORIZATION`, `COOKIE`, `cookie2`, `PROXY_AUTHORIZATION` and `WWW_AUTHENTICATE` — and nothing else. The Claude and J-Quants keys travel in `x-api-key`, so a hop handed a Class A key to whatever host answered next. Every path that carries a key now refuses redirects outright; market and news, which carry none and do meet real redirects, follow at most three. ([src/llm.rs](src/llm.rs), [src/chat/llm.rs](src/chat/llm.rs), [src/fundamental.rs](src/fundamental.rs), [src/notify.rs](src/notify.rs), [src/market.rs](src/market.rs), [src/news.rs](src/news.rs), [src/setup.rs](src/setup.rs))
- **The connection-admission cap was larger than anything it could bound.** §4 promises to shed over capacity with `503`, but the cap was `u16::MAX` — 65,535 permits, so memory and file descriptors ran out first. It is 256 now (a browser holds six connections per host; measured at 6, 30 and 100 concurrent requests, no `503`). The review that raised it assumed a live SSE stream holds a permit; measured with the cap at 2 and two streams open, two ordinary requests still succeeded and only the third was shed — the permit is released when the handler returns, before the body streams — so no SSE quota was needed and the comments that said otherwise are corrected. What a permit *is* held for is one handler, bounded only by the outbound timeout, which `xoksa.env` could set without limit; that is now capped at 15 minutes on every surface. ([src/server/mod.rs](src/server/mod.rs), [src/config.rs](src/config.rs), [src/llm.rs](src/llm.rs))
- **An Ollama response was read whole, with no bound, and `Content-Length` cannot provide one.** `reqwest` 0.13 asks for compressed bodies by default (`ClientBuilder::new` takes `Accepts::default()`), so every client here inflates what comes back whether or not it says `.gzip(true)` — measured on the Ollama path, which says it nowhere. A 406 KB body that expands to 202 MB took the process to **784 MB RSS**; after the fix the same body is refused and peak RSS is 23.9 MB. `Content-Length` counts the compressed bytes, so the limit counts what arrives decoded; the cheap pre-check is kept for an oversize *uncompressed* body. Bounded on the Ollama path only — plaintext HTTP to a user-configured host — because the other providers are TLS to hosts fixed in code. ([src/llm.rs](src/llm.rs), [src/setup.rs](src/setup.rs))
- **A failed notification put the webhook secret in the log and in the browser.** For Slack, Discord and Google Chat the secret *is* the webhook URL. A transport failure was reported by printing `reqwest`'s error, whose `Display` ends with ` for url (…)`, and that string went to the diagnostic log (the alert monitor) and to the dashboard as JSON (`/alert test`) — both forbidden by §0.1 and §4. Measured with a canary token. The message is now rebuilt from the error's classification and source chain, neither of which carries the URL, and the secret is scrubbed from whatever is about to be shown; the cause survives. ([src/notify.rs](src/notify.rs), [docs/dev-prog/security-assessment.md](docs/dev-prog/security-assessment.md))
- **Four things an English-UI review found.** The rule editor's template list showed Japanese names, because `rule_templates_catalog` was the one entry in that vocabulary with no English member. The page declared `lang="ja"` to an English session — the attribute a screen reader announces, which no meta tag replaces. A rule with no conditions could be saved: `{}` deserializes fine, and `eval_rules` treats an empty set as "no signal", so the rule can never trade and its backtest reports nothing. And the chart drew a session divider plus a date label at **every** bar on a daily chart, because every daily bar is a new calendar date — 64 labels overlapping into a band across the price panel — while the volume axis printed a share count as `358691760.00`. ([src/backtest.rs](src/backtest.rs), [src/server/api.rs](src/server/api.rs), [src/server/mod.rs](src/server/mod.rs), [webui-leptos/src/main.rs](webui-leptos/src/main.rs))
- **A mistyped ticker stayed a chip with nothing to say it was bad.** The engine's half is correct — a turn sent `AAPL,APPL` warns once, keeps going, and echoes back the set with the unusable symbol dropped, and it is not remembered. The dashboard applied that echo to its target set but never looked at what it had asked for, so the typo sat in the header indefinitely and errored whenever it was selected. It is marked now, with the reason in the tooltip; marked rather than removed, because this signal cannot tell a wrong code from a provider that did not answer. ([webui-leptos/src/main.rs](webui-leptos/src/main.rs))

### Added
- **A failed alert notification is visible instead of silent.** The monitor sends once, swallows the failure so one bad channel cannot stop it, and does not retry — so a rule that fired and failed to deliver looked exactly like a rule that never fired, with one line in the diagnostic log as the only trace. LINE's free tier running out fails in exactly that shape. Each rule now holds the outcome of its last attempt — time, success or failure, and the reason — shown under the rule in the alerts panel, recorded at every terminus including the four that never reach the network (no channel, no secret, keychain unreadable, secret invalid). Live state, not persisted, for the same reason `active` is not. ([src/server/monitor.rs](src/server/monitor.rs), [webui-leptos/src/main.rs](webui-leptos/src/main.rs), [docs/dev-prog/security-design.md](docs/dev-prog/security-design.md))

### Documentation
- **Two statements the code never matched.** §0.1 said the OpenAI `Bearer …` value is built in a `Zeroizing` buffer — it is not, and letting `reqwest` assemble it is the stronger of the two. §4 said a browser "exchanges" the token for a session cookie — no session value is minted; the cookie carries the token itself. Both corrections are marked as corrections rather than silently rewritten.
- **The decisions behind three absences are written down.** No TLS on the engine↔GUI and engine↔Ollama links, no separation between the auth token and the session cookie, no API versioning or RBAC — each sits in §0.2 with what makes it safe, so a reader sees a decision rather than an omission.
- **The OWASP API Security Top 10 (2023) mapping is now B.8 of the assessment**, re-read against the code rather than copied: API7 moves to PASS now that redirects are bounded, API10 off "none", API4 improved. What it does not cover is stated. ([docs/dev-prog/security-design.md](docs/dev-prog/security-design.md), [docs/dev-prog/security-assessment.md](docs/dev-prog/security-assessment.md))

## [2.9.9] — shipped 2026-09-26 (Windows) / 2026-10-01 (macOS)

**A score threshold is now a rate, not an amount of money, and the code that nothing called is gone.** 2.9.8 was inspected on both platforms but never shipped, and its two halves had already been built from different commits; 2.9.9 replaces it so that both are cut from one `main`. The substantive change is that an indicator whose score compared a **price difference** against a fixed constant could not mean the same thing on a ¥3,000 stock and a $13 one — the score was not even monotonic in the distance it claimed to measure.

### Fixed
- **The shipped binaries no longer name the account that built them, because releases are built on CI.** rustc records each crate's source path so a panic can name a file, and on a developer machine those paths run through the home directory: the engine carried `C:\Users\<name>` **689 times**, the desktop shell 253, the settings app 189, and the WASM every browser downloads **85**. `--remap-path-prefix` was measured and removes most of them, but not the paths baked into the `std` rlibs rustup ships nor the ones a C toolchain (`aws-lc-sys`) embeds through cc-rs, so the fix is to build where the account identifies nobody. `.github/workflows/release-windows.yml` builds the whole chain on a runner and refuses to finish if any artifact still names a non-build account; `scripts/check-no-host-paths.sh` is that check, and `scripts/inspect.sh` runs it as gate 9 so a locally built binary cannot pass a shipping inspection either. This also brings the release in line with the rule that an inspection binds to the artifact CI produced.
- **The bundler stages both sidecars, and refuses one that reports the wrong version.** `bundle-engine.sh` staged only the engine, leaving the settings app to a manual step, and the recipe written in the release records had lost the word "build" for that second sidecar after v2.9.2 — so a 2.9.9 installer was built carrying a settings binary that still reported 2.9.8, and following the written procedure could not have caught it. Both are staged now, and both are resolved and checked before either is copied (one fresh binary beside one stale one is the state that would still bundle): a binary whose version does not match `Cargo.toml` fails the bundle instead of shipping. The check reads the version as ASCII *or* UTF-16LE, because the settings app carries only the PE resource copy. §C.1 of the security assessment now states the build recipe once, so a release record names its commit and hashes rather than restating steps that drift, and `.gitattributes` keeps the shell scripts at LF on every platform this project builds on.
- **The VWAP score is a deviation rate, so it means the same thing on every instrument.** The five bands were compared against `close − VWAP`, a price difference in the instrument's own currency, against a fixed 4.0 / 1.0 — so the score was not monotonic in the distance it claimed to measure. Measured on daily bars: F, 5.03% below its VWAP at $13, scored **0**, while 7203.T at 2.19% below scored **−2**; MSFT at 0.47% above scored +1 while T at 1.54% below scored 0. The score now comes from `(close − VWAP) / VWAP × 100` against 3.0% / 1.0% — the same figure the display already printed as the deviation rate, so the reading and the score can no longer disagree — and a VWAP the division cannot be trusted on leaves the score absent rather than a stand-in 0. Intraday scores sit near 0 far more often, which is the reading and not a fault: VWAP restarts each session, and the close was measured within 0.9% of the session average. The guide's contrarian VWAP recipe also had the sign inverted — it asked how far price was stretched **below** VWAP and then named `+2`, the case where the close is above it. ([src/technical/indicators.rs](src/technical/indicators.rs), [src/utils.rs](src/utils.rs), [docs/manual/analysis-guide.md](docs/manual/analysis-guide.md))
- **The EMA, SMA and Ichimoku scores are deviation rates too, each with its own calibration.** All three compared a price difference against one shared 2.0 / 0.5 constant, so the score read the price level rather than the separation: measured across 20 instruments on daily bars, **every Japanese listing reached ±2 on Ichimoku whatever its actual separation — 1605.T at 0.16% from its base line scored −2 while F at 1.89% scored 0** — and the same held for EMA and SMA. Each now scores on the rate its own display already prints: EMA and SMA on `(short − long) / close × 100`, Ichimoku on `(tenkan − kijun) / kijun × 100`. The pairs are separate because the indicators measure structurally different separations — EMA and SMA share 5/20 periods but a simple average lags further in a trend, and Ichimoku's 9/26 is wider still, giving measured median deviations of 1.03% / 1.60% / 2.47% — so the bands are **EMA ±2.0% / ±0.5%, SMA ±3.0% / ±1.0%, Ichimoku ±4.0% / ±1.0%**, each splitting that sample across the neutral, slight and strong bands instead of collapsing into one. The five match arms, previously written out four times, are now one `five_band_score` the four indicators call with their own pair, and a close or base line the division cannot be trusted on leaves the score absent rather than a stand-in 0. The Ichimoku display had a workaround for exactly this defect — it softened "well above" to "above" on an intraday bar whose gap was under 1%, because a ±2 could come from a separation of a fraction of a percent. A ±2 now requires 4%, so that state cannot arise: the workaround and its two tests are gone, and both timeframes read from the one wording table. ([src/technical/indicators.rs](src/technical/indicators.rs), [docs/manual/analysis-guide.md](docs/manual/analysis-guide.md))
- **The Fibonacci score line states the rule the score applied.** It named a distance from the 50% level — "Close >2.00 above 50%", "within ±0.50 of 50%" — but ±2 is decided by the 38.2% / 61.8% level, not by a distance from the midpoint, so the text could contradict the score it explained: a close **1.64** below the 50% level was reported as "more than 2.00 below". The ±1 / 0 bands do use a distance, but the text hardcoded 0.50 whatever the setting was. Both are now stated as the code decides them, from the same function the score uses. ([src/render_indicators.rs](src/render_indicators.rs))
- **The Fibonacci neutral band is a share of the room the ±1 bands have, and is capped, so those bands can never become unreachable.** `FIBONACCI_NEUTRAL_EPS` was a price difference (default 0.5), which meant something different on every instrument: measured on daily bars it was 0.2% of that room on 9984.T — so `0` was effectively unreachable — and **larger than the whole room on F, where `+1` required a close both above 15.080 and below 14.984, an empty interval, so `+1` and `−1` could not be scored at all.** On PFE and T it still ate 76% and 60% of the room. It is now `FIBONACCI_NEUTRAL_RATIO` (`--fibonacci-neutral-ratio`, chat `/set fib-ratio`), a share of the 50%→38.2% distance, default **0.05** and clamped to **0.0–0.5** at every surface that accepts it — the cap is what makes an unreachable band structurally impossible rather than merely unlikely. The guide's table called `0` "near center, hesitation"; measured across 20 instruments, 19 closes sat more than 6% of the range from the midpoint, so it is documented as what it is — a boundary guard that is rarely reached. **The old key is not read any more: a `FIBONACCI_NEUTRAL_EPS` line in an existing `xoksa.env` is ignored and should be replaced.** ([src/config.rs](src/config.rs), [src/technical/indicators.rs](src/technical/indicators.rs), [src/setup.rs](src/setup.rs), [src/chat/exec.rs](src/chat/exec.rs), [xoksa-setup/src/main.rs](xoksa-setup/src/main.rs), [xoksa-setup/frontend/index.html](xoksa-setup/frontend/index.html), [xoksa.env.sample](xoksa.env.sample))
- **The recipes read the scores the way the engine computes them.** The score tables were corrected, but the worked examples that use them were not, and two of them inverted a sign. xoksa's Bollinger score is a **mean-reversion** reading — a close above the band is overheated and scores negative — yet the breakout recipe said an upward break "jumps to `+1`~`+2`" and the bottom-fishing recipe called `-2` "broke below the lower band", which is the score for the opposite side of the band. Both now state the sign the engine produces and say plainly that the sign reads mean reversion rather than direction. Four places claimed ADX `+1` at 25 and `+2` at 40; the bands are 30 and 50. One claimed the RSI value is "highlighted red above 65" — it is printed plain, and 65 is not a threshold in the code at all: the sell zone is `sell-rsi`, default 70, which this recipe lowers. The Fibonacci neutral row also read "within eps" where exactly eps scores `±1`, so it now reads "less than eps", which is also correct when the band width is zero. All of these are in both languages. ([docs/manual/analysis-guide.md](docs/manual/analysis-guide.md))
- **The degenerate cases of that boundary rule, and the display lines that had to follow it.** Making the comparisons inclusive introduced two scoring defects, both found in review. A Bollinger band with **no width** — a close that has not moved for the whole period, so the two edges coincide — matched the upper arm first and **penalised a perfectly flat series `-1`**; there is nothing to be outside of, so a zero-width band now scores `0`. And with `fibonacci_neutral_ratio = 0`, an accepted setting that removes the neutral band, the `+1` and `-1` conditions met at the 50% level and arm order alone **made the midpoint lean up**; each `±1` arm now also requires the close to be on its own side of the midpoint, so the midpoint is neutral whatever the band width while a positive width still puts the boundary in `±1`. Two display lines had to follow as well: the Bollinger position line was decided by `%B > 1.0` while the score compares the price to the band inclusively, so a close equal to the upper band printed "within bands → neutral" beside a `-1` — it now reads the score, which also retires a "within bands, upper-biased" hint that was dead code under the old strict comparison — and the Fibonacci lines said "above the 38.2% level" where the level itself scores `+2`. Four tests now pin the degenerate cases and run computation through to the rendered text. ([src/technical/indicators.rs](src/technical/indicators.rs), [src/render_indicators.rs](src/render_indicators.rs), [docs/manual/analysis-guide.md](docs/manual/analysis-guide.md))
- **One boundary rule, for every score: a threshold is met at the threshold.** The codebase held four conventions at once. The deviation indicators, ADX and Stochastics put a reading that lands exactly on a boundary in the **stronger** band; ROC and Bollinger put it in the **weaker** one; Fibonacci put it in the **inner** band and needed a fall-through arm to stay total. A reader had to carry a different rule per indicator, and the manuals did not always agree with the one that applied. Six of the nine ladders already followed "the threshold is met at the threshold", so the other three moved to it: ROC now *is* `five_band_score` (its bespoke match is gone), Bollinger's comparisons are inclusive, and Fibonacci's five arms cover every close with no gap, no overlap and no fall-through. This changes the score only at exact equality: ROC at exactly ±3% or ±10%, a close sitting exactly on a Bollinger band or 2% beyond it, and a close exactly on the 38.2% / 61.8% level or on the edge of the Fibonacci neutral band. **Nothing pinned any of this before** — every per-indicator test used off-boundary values, which is how four conventions coexisted unnoticed; two tests now assert the rule at every ladder's exact boundaries. The guide states the rule once, up front, and every score table reads as inequalities with no overlap between rows. ([src/technical/indicators.rs](src/technical/indicators.rs), [docs/manual/analysis-guide.md](docs/manual/analysis-guide.md))
- **The displayed lines agree with the score, at the boundary as well as away from it.** A review of the deviation-rate change found the display had not followed it. The EMA block still classified the two legs by a **price difference** under ±0.01 and printed "near equal → no score change": on a 0.459 close an EMA gap of 0.00747 is a 1.63% deviation, so the score was `+1` while the same block denied it — and scaling the series by 1,000 changed only the wording. It now compares the legs by sign, as SMA and Ichimoku do, and reports where the lines sit rather than re-deriving a classification the score already made. The EMA block was also the only one using `unwrap_or(0.0)`, so a score that could not be computed was displayed as a neutral `0` — the one distinction §1 of the security design exists to keep; it now handles the absent case like the other eight indicators. The Fibonacci `±1` wording said "below the 38.2% level" where the level itself scores `+1`, so it reads "at or below" now, and the note claiming the logic always tilts to `+` or `-` at a boundary was simply not true. In the manuals the four deviation tables read "Above +0.5%" / "+0.5% 超" where the code is `>=`, so `+0.5%` exactly scored `+1` while the table said `0`, and the neutral row overlapped both neighbours at its endpoints; all eight tables (four indicators × two languages) now state each band as an inequality that matches the code with no overlap. The Bollinger bullets had the opposite error — "2% or more above the upper band → -2" where the code is strictly `> upper × 1.02`, so exactly 2% above scores `-1` — and the English Fibonacci section still carried the old fixed ±0.50 neutral band beside the new ratio. Two tests now run computation through to the rendered text, which is where every one of these hid. ([src/render_indicators.rs](src/render_indicators.rs), [docs/manual/analysis-guide.md](docs/manual/analysis-guide.md))
- **A failed fetch says what actually went wrong, and a provider's page never lands in the message.** Three things went wrong at once for an unknown ticker. The HTTP layer read only the status on a non-2xx reply and discarded the body, so Yahoo's own reason — `{"chart":{"error":{"code":"Not Found","description":"No data found, symbol may be delisted"}}}`, sent *with* the 404 — was thrown away and the user got "Market data API request failed". The failover then dropped the primary's error entirely (`Err(_)`), so when the Stooq fallback also failed only the fallback's complaint surfaced. And Stooq's anti-bot JS challenge, an HTML page, was treated as a malformed CSV and **quoted whole into the error**, putting a `<!DOCTYPE html>…` line in the terminal under the heading "unexpected header". Now: the error body is read and its reason reported (`HTTP 404 (Not Found — No data found, symbol may be delisted)`); the primary's reason leads and the fallback's outcome follows as context, because the primary is the one that knows an unknown ticker; an HTML response is named as a blocked fallback rather than quoted; and all untrusted provider text passes through `bounded_provider_text`, which caps it and turns control characters into spaces so a response cannot rewrite the line it is printed on. ([src/market.rs](src/market.rs), [docs/dev-prog/source-map.md](docs/dev-prog/source-map.md))
- **`--env-file`'s help text no longer promises a fallback the engine does not have.** It read "canonical app-data path, else ./xoksa.env", while §6 of the security design states there is no implicit `./xoksa.env` — and the code agrees, so only the help string was wrong. ([src/config.rs](src/config.rs))

### Removed
- **An index name is no longer answered with a fund, and never with a leveraged one.** `normalize_ticker_input` silently rewrote the ticker before anything else saw it: `S&P500` / `SNP500` / `SP500` → `SPY`, `NASDAQ100` / `ナスダック100` → `QQQ`, `DOW` / `DJIA` / `ダウ平均` → `DIA`, `日経平均` / `NIKKEI225` → `1321.T`, `TOPIX` → `1306.T`. The user asked for an index and every indicator, score and LLM comment was computed on a tracking fund, with nothing on screen saying so. **`FANG+` / `FANGプラス` resolved to `FNGU`, a 3× leveraged ETN** — measured at 3.1× QQQ's daily move and 2.5× its Bollinger bandwidth — so the readings were of an instrument whose swings are tripled while the user believed they were looking at FANG+. `全世界` / `オールカントリー` pointed at the iShares ACWI ETF although it is the retail nickname of a different product, and `ACWI` → `ACWI` / `VTI` → `VTI` were identity no-ops. All of these are gone: an index name xoksa cannot serve now fails as an unknown ticker instead of being answered with something else. The one expansion kept is `全米` / `トータルマーケット` → `VTI`, which names the instrument rather than substituting for a different one. The display names also stop appending the tracked index in brackets — `Invesco QQQ Trust (NASDAQ100)` read as though the fund were the index — and the `FANG+` entry there was unreachable anyway, because the rewrite happened first and `sanitize_ticker` does not admit `+` in a ticker. None of this was documented for users; the source map described the function as only trimming and upper-casing (which it does not even do to its return value) and described `HardcodedInfo` as holding a country code it has never had. ([src/bootstrap.rs](src/bootstrap.rs), [src/technical/indicators.rs](src/technical/indicators.rs), [docs/dev-prog/source-map.md](docs/dev-prog/source-map.md))
- **Four functions nothing called.** `pub` is not evidence of use here: the engine exposes a library that no crate depends on — the desktop and settings apps launch the engine as a child process rather than linking it — so `dead_code` never fires on a `pub` item. Each was checked individually, and the 18 apparent orphans in `src/server/api.rs` were confirmed live through their router registration. `score_to_string` ([src/utils.rs](src/utils.rs)) was documented as unused; the scores shown are formatted `{:.1}` at each call site. `fetch_market_data` ([src/market.rs](src/market.rs)) called itself a compatibility entry point for a library consumer that does not exist. `verify_text` ([src/integrity.rs](src/integrity.rs)) was a thin wrapper over `verify`. **`get_entry`** ([src/technical/types.rs](src/technical/types.rs)) handed out `&TechnicalDataEntry` whole — an immutable reference, so nothing could be written through it, but a way to read the confirmed data without passing the guard, which §1 of the security design exists to prevent. Closing it costs nothing while it has no caller.

## [2.9.8] — inspected, superseded by 2.9.9

**The integrity guard stopped removing correct sentences.** Three defects found in a pre-merge review of 2.9.7. All three are the guard erring toward deleting a correct sentence rather than toward admitting a fabricated number, and all three bite an English answer where a Japanese one is unaffected.

### Fixed
- **A number's role is read from its sentence, not from the fragment that captured it.** The bare-price and numeric-unit checks re-read each figure from the isolated capture, where the words that make it a date or a window length are not visible. `Support has held since 2019.` became an observation with no confirmed counterpart and the sentence was removed; so did `The 200-day moving average has acted as support.` — the verdict disagreed with check 1, which classifies the same figures correctly because it reads the whole sentence. The numbers now come from the sentence, filtered to the ones the capture covers, and each is verified at its own position rather than at the fragment's. On the alert EXPLAIN path this was worse than a trim: a note is dropped in full on any issue, so a correct note was lost whenever it mentioned a year. ([src/llm.rs](src/llm.rs))
- **The price- and trade-context gates match words, not substrings.** `contains_any_term` used `contains`, so `low` matched *below* / *allow* / *following*, `line` matched *decline* / *headline* / *online* and `close` matched *closely* — the price-context gate opened on almost any English answer, and with the defect above that turned the gate into a sentence remover. Both gates now go through `integrity::term_positions`, the same boundary rule the indicator binding uses: an ASCII term must stand as a word (an English plural still counts), while Japanese terms match as-is because they have no boundary. ([src/llm.rs](src/llm.rs), [src/integrity.rs](src/integrity.rs))
- **`safe_ratio` refuses a denominator indistinguishable from zero.** Concentrating the per-provider ratio guards into one function in 2.9.7 narrowed the condition from `abs() > f64::EPSILON` to `!= 0.0`. Only an exact zero was refused, so an EPS of `1e-300` against a price of 180 produced `1.8e302` — finite, therefore stored in `FundamentalData`, displayed, sent to an LLM, and registered as a fact the guard would verify a model's citation against. The doc comment described the old behaviour; the code now matches it. ([src/fundamental.rs](src/fundamental.rs))

### Security
- **`rustls` updated past RUSTSEC-2026-0285.** The advisory was published on 2026-09-14, the day after the v2.9.7 shipping inspection measured `cargo audit` clean — so 2.9.7 would have shipped carrying it. TLS 1.3 handshake messages were accepted across encryption level boundaries (5.3 medium); `rustls` 0.23.32 → 0.23.45, with `rustls-webpki` 0.103.13 → 0.103.15. The crate is on every outbound HTTPS path: market data, news, LLM providers and chat notifications.

### Known limitation
- A year stated anywhere in a sentence still dates a reading written in another clause: `Price has been in this range since 2019, and RSI is 45.2.` is refused although 45.2 is the confirmed value. The clause splitting is correct — the year qualifier deliberately falls back to the nearest mention in the sentence when its own clause names none, which is required for `In 2019, revenue was 13.7兆円.` and harmful here. Telling the two apart means stating in the normative rule that a technical reading is dated by its bar and not by a year written elsewhere in the sentence, so it is tracked in #146 rather than changed on the eve of a release.

## [2.9.7] — unreleased

**Confirmed data and the AI output check are now enforced the same way regardless of data kind or display language.** Four gaps between what the design documents promise and what the code did: an indicator that could not be computed was evaluated as a valid zero; four of the five integrity checks fired only on Japanese wording; a number was accepted merely because the same digits appeared somewhere in the input; and fundamental values could be constructed and changed without passing any validation.

### Fixed
- **An uncomputed indicator no longer counts as zero.** `TechnicalDataGuard` records which indicators were actually computed — a setter marks its key only when it runs with a finite value, and a later failed recomputation clears both the value and its flag rather than leaving the previous reading standing — so a value that was never produced — never computed, or a computation that failed — is absent, while a legitimately computed `0.0` is present. `guard_indicator_map` inserts only computed indicators, so with 10 bars a 20-period EMA is absent rather than 0.0, and `close > ema_l` no longer holds. A condition referencing a missing indicator is false, which leaves the existing AND/OR semantics unchanged. The same map serves the backtest and the live alert monitor, so both paths are covered. Extension failures are logged instead of being discarded. ([src/technical/types.rs](src/technical/types.rs), [src/backtest.rs](src/backtest.rs))
- **The integrity checks apply to English answers, not only Japanese ones.** The checks are defined once, in a shared vocabulary that carries the Japanese and the English members of the *same* check; the checks are no longer per language. Numeric units, price context, trade-action context and the VWAP direction comparison previously matched Japanese wording only. Normalisation also removed every space, which destroyed English word boundaries — the direction check now matches a space-preserving form as well. ([src/llm.rs](src/llm.rs))
- **A number must belong to what the sentence says it belongs to.** The check "do these digits appear in the input?" is replaced by verification against structured confirmed data — symbol, indicator, value, unit, sign and the bar it was computed on. **That reference is built from `TechnicalDataGuard` and `FundamentalData` and passed to the checker; no text is read as a source.** The prompt cannot serve as the reference, because it also carries the conversation, the user's own turns and other models' opinions, and a label written by any of them is indistinguishable from one the engine wrote — so a number a user typed can no longer become "confirmed". Every call path that reaches an LLM now carries this data: the CLI single analysis, chat (including `/basic`, `/forum` and the debate), the web session, the Web multi-timeframe analysis (one entry per bar, each naming its timeframe), and the alert EXPLAIN note. A reload replaces the confirmed data together with the display text and the prompt context, so a refreshed value is accepted and the value it replaced is not. The three fundamental outcomes are distinguished: not re-fetched keeps display, prompt and reference; a fetch that failed drops all three together; a successful fetch replaces all three. The quoted currency travels from the market data into the confirmed data, so a US listing's dollar figures are accepted whether or not fundamentals were fetched. Attribution is decided per claim, not per answer: one sentence may carry several claims, so the instrument, the bar and the year are each bound to the claim they were written in — separated at 、 , ; and at *and* / *while* / *but*, which reads a qualifier placed before the figure (Japanese) and after it (English) alike. "AAPL is $168.8 while MSFT is $400" attributes each figure to its own issuer and the exchanged pair is refused; the same holds for two bars and for two fiscal years in one sentence. A loaded code that is an ordinary word (`IT`) or a single letter (`A`, `T`) is matched as an instrument before any ordinary-word filter, and a bar that was never analysed carries no value at all. Generic indicator wording is recognised beside specific wording, so "RSI is 45.2 and EMA is 166.88" binds each figure to its own indicator, and an indicator name counts only when it stands as a word — `ema` inside "remains" and `per` inside "period" are not mentions, while a Japanese `VWAP168.96` and an English plural still bind. Two legs of one indicator differ only by their window, so the window travels with the value: `EMA(20)`, `EMA(20 days)`, `EMA（20日）`, `20日EMA` and `The 20-day EMA` are one statement on a daily analysis (and `EMA(20 months)` / `The 20-month EMA` are on a monthly one) and select the leg they name; a window that was never computed carries nothing; and a window applies to the indicator it was written on (an `RSI(14)` beside an EMA does not select a leg of it). What is compared is the period count, not the stated time unit, which is not checked against the analysis timeframe. A number is first read for what it is doing — an observation, a period parameter (`RSI(14)`, `1時間足`) or a date (`2026-09-08`, `in 2019`) — so a window length and a date are never checked as readings, and a close of 2025 is a price rather than a year. A reading offered for an indicator that has no confirmed value at all is refused instead of passing unexamined. Rounding is judged at the magnitude written (約13.7兆円 for a confirmed 13.704兆円; 13.8兆円 refused); a magnitude is a scale only, so a share count may be written "12 million" and not "$12 million", and a currency may only be written on an amount (`Volume is $1000` is refused); a currency prefix, magnitude word and trailing currency word are read as one expression (`13704 billion dollars` is not a yen figure); brackets are not an exemption, so `RSI(14) is 45.2` states one value while `RSI (value: 99.99)` states a fabricated one; a figure carrying money is never a bar size (`$5m` is an amount, not a five-minute bar); the spans of numbers, dates, period specifications and tickers are settled in one pass per sentence and shared by the claim-splitting and every binding, so a thousands separator does not split a claim (`13,704,000,000,000` behaves as `13704000000000` does) and one date reads the same as `2026-09-08`, `2026年9月8日` or `September 8, 2026` — a full date stamps the observation while only a year written as a year dates the claim; an exemption applies only to a token's whole form, so month names are the real ones anchored inside a date construction (an open reading also spells *margin* and *declined*), an amount includes its magnitude suffix whether it is attached to the digits or written apart (`$168.8m`, `$168.8 m` and `$168.8 million` are one amount; 百万 and 十億 read as million and billion do), so the correctly scaled spelling is accepted and a figure off by orders of magnitude is not, and a suffix the reader does not understand is never dropped; a sign may stand on either side of a currency symbol (`-$2.50` and `$-2.50` are one amount); the engine's own per-share and per-lot fundamental lines are confirmed values, and the `1` in `1株あたり` / `per share` names a basis rather than a reading; the change-versus-previous-bar label is confirmed both as an amount and as a percentage, and a Fibonacci ratio is a level identifier only where the writing marks it as one; a technical reading is checked against the bar's year and a fundamental one against its own fiscal year, and English sentence boundaries are read without splitting on a decimal point or a ticker's dot — the removal uses those same boundaries, so in prose a correct sentence survives beside a wrong one in either language, while a table row, a bullet or a single-sentence line is dropped whole and an alert's EXPLAIN note is dropped in full. The VWAP direction is likewise read from the confirmed values rather than from the prompt's wording, so relabelling a display line cannot change the verdict. ([src/integrity.rs](src/integrity.rs), [src/llm.rs](src/llm.rs), [src/main.rs](src/main.rs), [src/chat/mod.rs](src/chat/mod.rs), [src/server/monitor.rs](src/server/monitor.rs))
- **Fundamental values cannot be built or changed without validation.** `FundamentalData`'s fields are private and every write path validates what it writes: the initial construction from a provider's result is concentrated in `FundamentalData::build`, which takes the in-flight `FundamentalInputs`, rejects non-finite figures, and computes PER/PBR/ROE itself — once, from validated inputs, instead of separately in each provider path. `Default` yields an empty value carrying no figures, and a later BPS or price update goes through a validated public method. `recompute_derived`/`set_bps` keep the ratios consistent with their inputs, so a ratio cannot survive beside inputs that have moved on, and the price a ratio came from is stored with it. Division by zero and overflow leave a ratio undefined rather than producing infinity. A legitimate zero and a meaningful negative (a loss, a negative EPS) are kept. `TechnicalDataGuard.entry` is private too, closing the same hole on the technical side. ([src/fundamental.rs](src/fundamental.rs), [src/technical/types.rs](src/technical/types.rs))

- **An alert rule is saved whichever surface created it.** `/alert add` in chat only ever lived in memory, and `/alert del` left the `ALERT_<n>_*` entries in `xoksa.env`, so a deleted rule came back on the next start while an added one did not survive at all; only the dashboard persisted. Both surfaces now go through the same persistence, and a write that fails is reported instead of being answered with success — `AlertOpResponse` carries a `warning` (applied in memory, not saved) distinct from `error` (nothing happened), and the dashboard shows it. The limit was hardcoded as `1..=16` in both the scanner and the add path; it is now `config::ALERT_MAX`, one definition, and rule numbers above it are listed in a startup warning instead of being ignored in silence. A rule is written into its own slot, replacing what is there rather than being appended beside it — the previous behaviour left two entries for one number once a non-intraday rule had freed a number in the store but not in the file, and because `EXPLAIN` is written only when true, an old `true` outlived a rule saved with `explain=false`. ([src/config.rs](src/config.rs), [src/server/monitor.rs](src/server/monitor.rs), [src/server/api.rs](src/server/api.rs), [src/chat/exec.rs](src/chat/exec.rs), [webui-leptos/src/main.rs](webui-leptos/src/main.rs))
- **One writer for `xoksa.env`.** Saving several alert rules at once lost most of them — four saves all answered Ok and one rule could be read back — because each read-modify-write raced the others. Reading, editing and replacing the file is now a single exclusive section shared by the alert path and the settings path (`utils::rewrite_env_file`, beside the path resolution), with number allocation and writing inside the same lock, so an alert write and a `LANG=en` write no longer erase each other. The replacement is atomic: a temporary file (`create_new`, `mode(0o600)` on Unix) is written, the original's permissions are copied onto it, and it is renamed over the target — a failure leaves the old file intact and no temporary behind. Line matching goes through the loader's own `parse_env_line`, so `export ALERT_3_EXPLAIN=true` and a leading BOM are recognised the way the loader recognises them, while comments and `ALERT_10_*` are left alone. `env::vars()` is replaced by `vars_os()`: an unrelated environment variable whose value is not valid Unicode no longer panics the scan. ([src/utils.rs](src/utils.rs), [src/server/monitor.rs](src/server/monitor.rs), [src/config.rs](src/config.rs))
- **The two UI shells resolve `xoksa.env` explicitly.** The desktop app and the settings app read it by a relative path, which lands on the canonical file only because both `set_current_dir` into the config directory at startup — and that result is discarded. Both now resolve it through `xoksa_paths::env_file()`, as §6 of the security design requires (there is no implicit `./xoksa.env`). ([xoksa-desktop/src/main.rs](xoksa-desktop/src/main.rs), [xoksa-setup/src/main.rs](xoksa-setup/src/main.rs))
- **An alert's EXPLAIN note may restate the rule that fired, and only that rule.** The notification carries the condition the engine evaluated (`score <= 5`), so the model may quote it; the comparison in the sentence is parsed rather than inferred from a word occurring somewhere, and the parsed indicator, direction, equality and value must all match. `score >= 5` and `score < 5` are different rules and are refused, as is `score <= 5` where the rule said 5.4 — display rounding may change a reading's last digit, never a condition. The number must be written bare, as the rule wrote it: `score <= $5`, `score <= 5%` and `score <= 5foo` state something the rule does not, and a unit on the threshold makes the condition unacceptable rather than unrecognised, so it is refused **as a condition** instead of being handed back to the reading check — where `close > ¥3091` had passed on an instrument whose close is 3091. The operator qualifies the number it compares, so in "score is at or below 5 (current value: 0.0)" the 5 is checked as the rule and the 0.0 as a reading. A sentence that states the indicator *is* that number stays a fabrication. ([src/integrity.rs](src/integrity.rs), [src/server/monitor.rs](src/server/monitor.rs))
- **The alert panel and the English manuals said "a confirmed bar".** The monitor evaluates the **latest fetched bar** — on Japanese intraday timeframes the still-forming one carrying the real-time quote — and does not wait for the bar to close. The Japanese manual sections were corrected along with the limit and the restart note; the dashboard's alert description, the `AlertRule` doc comment and the English manual sections still described the old behaviour. ([webui-leptos/src/main.rs](webui-leptos/src/main.rs), [src/config.rs](src/config.rs), [docs/manual/usage-guide.md](docs/manual/usage-guide.md), [docs/manual/command-reference.md](docs/manual/command-reference.md))

### Changed
- **Manual and canon wording for alerts.** The manuals now state the 16-rule limit, that the monitor watches the ticker written in the rule and needs no symbol on screen, that it evaluates the latest fetched bar, and that a hand edit of `xoksa.env` takes effect on the next start. The security design's "in-session monitor" is corrected to a background task inside `xoksa serve` — one task for the whole process, with no background residency and no dependency on an open dashboard — and now says what it watches and that rules are read once, at startup. ([docs/manual/usage-guide.md](docs/manual/usage-guide.md), [docs/manual/command-reference.md](docs/manual/command-reference.md), [docs/manual/setup.md](docs/manual/setup.md), [docs/dev-prog/security-design.md](docs/dev-prog/security-design.md), [docs/dev-prog/source-map.md](docs/dev-prog/source-map.md))
- **Versions aligned to 2.9.7** across the engine, desktop app, settings app, and web UI, including each sub-crate's `Cargo.lock`; MSI name `XOKSA-2.9.7-x64.msi`.

### Known limitations
- Two paths run **without** confirmed data: free-form conversation with no instrument loaded, and news triage (the model sorts the titles it was given, so its numbers must come from those titles rather than from the market data). On both the guard falls back to the older presence test — the number must occur in the input it was given — and does not claim attribution was verified. Everywhere else the confirmed data travels with the request, the Ollama benchmark included.
- A bare uppercase symbol (`MSFT`) is read as naming an instrument only in a session whose own instruments are written that way, only from two characters up, and only when it is neither part of the guard's vocabulary nor an enumerated ordinary word or acronym (`I`, `IT`, `AI`, `CEO` …). A bare foreign US symbol quoted inside a Japanese-listing session is therefore not flagged as foreign; its number is still verified against the confirmed values.
- When the provider reports no currency (the labelled Stooq fallback), a written currency is not refused — an unknown cannot contradict it. The value itself is still verified.

---

## [2.9.10] — 未リリース

**鍵・ネットワーク・LLM の出力に触れる経路の点検。** 以下はすべて、変更の前に実測している（可能なものは出荷済みの 2.9.9 バイナリに対して）。提案元のレビューより実態が悪かった 2 件は、その旨を明記した。

### Fixed
- **出力整合性ガードが、文が名乗っていない指標に数値を結び付けていた。** 出荷バイナリに対する 231 件の実測：数値は最も近い*登録済み*の指標名を探し、その探索が主張の境界を越えていた。そのため `RSI is 56.93 and the signal is 56.93`（誤）が通り、`RSI is 56.93 and the signal is 2.0764`（正）が消えていた——`signal` が語彙に無く、2 つ目の主張が 1 つ目から `RSI` を借りたためである。英語固有ではない：転換ライン・基準ラインは日本株でも同じように壊れる。3 つを同時に入れた（単独ではどれも退行に見えるため）——指標の結び付けだけを主張の内側に閉じ（銘柄・足・年は文全体を見るのが正しい）、表の上のコメントが既に約束していた英語の概念名を 9 指標に足し、検知器 3 の価格文脈ゲートを外した（守っていた数値はいずれにせよ帰属検証を受けるため）。([src/integrity.rs](src/integrity.rs), [src/llm.rs](src/llm.rs), [docs/dev-prog/security-design.md](docs/dev-prog/security-design.md))
- **`Zeroizing` が消すのは `String` が最後に持っていたバッファだけで、途中で捨てたものは消えない。** 3 箇所が空の文字列を伸ばして鍵素材を作っており、再確保のたびに秘密を抱えたバッファが解放されていた。最も重いのは `random_token_256`——`SERVE_AUTH_TOKEN` が生まれる瞬間そのもので、生の 32 バイトが未ゼロ化のままスコープを抜け、`format!` の一時文字列 32 個が 16 進を運んでいた。対話的な鍵入力と、キーチェーン移行の経路（中間バッファに**他の鍵**の値が入る）も同様。([src/keystore.rs](src/keystore.rs), [src/setup.rs](src/setup.rs))
- **リダイレクトを無条件に追従しており、`reqwest` は `x-api-key` を除去しない。** 12 のクライアント構築がリダイレクト方針を一つも指定しておらず、既定の最大 10 ホップで動いていた——通知の送信先許可リストは最初の 3xx で外れる（OWASP API7）。さらに悪いことに、`reqwest` 0.13.5（`redirect.rs:239`）の実測で、クロスホストのリダイレクトが除去するのは `AUTHORIZATION`・`COOKIE`・`cookie2`・`PROXY_AUTHORIZATION`・`WWW_AUTHENTICATE` **だけ**である。Claude と J-Quants の鍵は `x-api-key` で運ばれるため、ホップは次に応答したホストへクラス A の鍵を手渡していた。鍵を運ぶ経路はすべてリダイレクトを拒否し、鍵を運ばず実際にリダイレクトに出会う市場とニュースは 3 回までとした。([src/llm.rs](src/llm.rs), [src/chat/llm.rs](src/chat/llm.rs), [src/fundamental.rs](src/fundamental.rs), [src/notify.rs](src/notify.rs), [src/market.rs](src/market.rs), [src/news.rs](src/news.rs), [src/setup.rs](src/setup.rs))
- **接続受入の上限が、それ自身より小さいものしか抑えられない値だった。** §4 は容量超過を `503` で shed すると約束しているが、上限は `u16::MAX`——65,535 の許可証では、その前にメモリと fd が尽きる。256 にした（ブラウザは 1 ホストに 6 接続。6・30・100 同時で実測し `503` 無し）。提案元のレビューは「稼働中の SSE が許可証を握る」ことを前提にしていたが、上限 2・SSE 2 本で実測すると通常リクエスト 2 本が通り、落ちたのは 3 本目だけだった——許可証はハンドラが応答を返した時点、本文が流れる前に解放される——したがって SSE の別枠は不要で、逆のことを書いていたコメントを訂正した。許可証が保持されるのはハンドラ 1 つの間で、その長さを律速するのは外向きタイムアウトだけであり、`xoksa.env` から無制限に設定できた。これに 15 分の上限を全経路で設けた。([src/server/mod.rs](src/server/mod.rs), [src/config.rs](src/config.rs), [src/llm.rs](src/llm.rs))
- **Ollama の応答を無制限に読み切っており、`Content-Length` では上限にならない。** `reqwest` 0.13 は既定で圧縮本体を要求する（`ClientBuilder::new` が `Accepts::default()` を取る）ため、`.gzip(true)` と書いていなくても展開する——`.gzip(true)` がどこにも無い Ollama 経路で実測した。406 KB の本体が 202 MB に展開し、プロセスの最大 RSS は **784 MB** に達した。修正後は同じ本体を拒否し、最大 RSS は 23.9 MB。`Content-Length` は圧縮後のバイト数なので、上限は**展開後に到着したバイト数**で数える。無圧縮の過大本体を 1 バイトも読まずに拒否する事前検査は残した。上限は **Ollama 経路のみ**（平文 HTTP で接続先が利用者設定）——他のプロバイダはホストがコード固定の TLS 経路だからである。([src/llm.rs](src/llm.rs), [src/setup.rs](src/setup.rs))
- **通知の送信失敗が、ログとブラウザに webhook の秘密を出していた。** Slack・Discord・Google Chat では、秘密は webhook URL そのものである。転送失敗は `reqwest` のエラーをそのまま出力しており、その `Display` は末尾に ` for url (…)` を付ける。その文字列が診断ログ（アラート監視）とダッシュボード（`/alert test` の JSON）へ出ていた——いずれも §0.1・§4 が禁じている。カナリアトークンで実測。本文はエラーの分類と source 連鎖（どちらも URL を含まない）から組み直し、表示直前に秘密を除去する。原因は残る。([src/notify.rs](src/notify.rs), [docs/dev-prog/security-assessment.md](docs/dev-prog/security-assessment.md))
- **英語 UI のレビューで見つかった 4 件。** ルール編集のテンプレート一覧が日本語のままだった（`rule_templates_catalog` だけが、その語彙の中で英語を持たない項目だった）。英語セッションにページが `lang="ja"` を宣言していた（スクリーンリーダーが読むのはこの属性であり、meta では代替できない）。条件ゼロのルールが保存できた——`{}` は問題なくデシリアライズでき、`eval_rules` は空集合を「シグナル無し」として扱うため、そのルールは決して取引せず、バックテストは何も報告しない。そしてチャートは、日足でも**全ての足**に日区切り線と日付ラベルを引いていた（日足はどの足も新しい暦日だから）——64 個のラベルが価格パネルに重なって帯になる——一方で出来高軸は株数を `358691760.00` と表示していた。([src/backtest.rs](src/backtest.rs), [src/server/api.rs](src/server/api.rs), [src/server/mod.rs](src/server/mod.rs), [webui-leptos/src/main.rs](webui-leptos/src/main.rs))
- **打ち間違いの銘柄が、何の印も無いままチップとして残っていた。** エンジン側は正しい——`AAPL,APPL` を送ったターンは 1 回警告し、処理を続け、使えない銘柄を除いた集合を返し、それを記憶しない。ダッシュボードはその返答を対象集合に反映する一方、**自分が何を要求したか**を見ていなかったため、打ち間違いはヘッダに残り続け、選ぶたびにエラーになっていた。いまは理由をツールチップに添えて印を付ける。消さずに印を付けるのは、この信号ではコードの誤りと提供元の不応答を区別できないからである。([webui-leptos/src/main.rs](webui-leptos/src/main.rs))

### Added
- **アラートの送信失敗が、無言ではなく見えるようになった。** 監視は 1 回だけ送り、1 つの不調なチャンネルで止まらないよう失敗を飲み込み、再送もしない——そのため、発火して届かなかったルールと、そもそも発火していないルールが見分けられず、痕跡は診断ログの 1 行だけだった。LINE の無料枠が尽きたときの失敗は、まさにこの形で起きる。いまは各ルールが直近の試行の結果——時刻・成否・理由——を保持し、アラート欄のルールの下に表示する。記録はネットワークに到達しない 4 経路（チャンネル無し・秘密未登録・キーチェーン読取失敗・秘密不正）を含む全終端で行う。保持は実行中のみで、`active` と同じ理由から永続化しない。([src/server/monitor.rs](src/server/monitor.rs), [webui-leptos/src/main.rs](webui-leptos/src/main.rs), [docs/dev-prog/security-design.md](docs/dev-prog/security-design.md))

### Documentation
- **実装と一致していなかった記述 2 件。** §0.1 は OpenAI の `Bearer …` を `Zeroizing` バッファで構築すると書いていたが、実装はそうではなく、`reqwest` に組み立てさせる方が強い。§4 は「トークンをセッション cookie と交換する」と書いていたが、セッション値は発行しておらず、cookie が運ぶのはトークンそのものである。いずれも黙って書き換えず、訂正として明記した。
- **3 つの「無い」ことの理由を書き残した。** エンジン↔GUI とエンジン↔Ollama に TLS を実装しない、認証トークンとセッション cookie を分離しない、API のバージョニングと RBAC を行わない——それぞれ §0.2 に、何が安全を担保しているかとともに置いた。読む側には、欠落ではなく決定として見える。
- **OWASP API Security Top 10 (2023) の判定を診断の B.8 に置いた。** 写すのではなくコードへ再突合し、リダイレクトを制限した結果 API7 は適合へ、API10 は「未対応」から外れ、API4 は改善。覆っていない範囲も明記した。([docs/dev-prog/security-design.md](docs/dev-prog/security-design.md), [docs/dev-prog/security-assessment.md](docs/dev-prog/security-assessment.md))

## [2.9.9] — 出荷済み（Windows 2026-09-26／macOS 2026-10-01）

**スコアの閾値を金額ではなく率にし、誰も呼んでいないコードを消した。** 2.9.8 は両プラットフォームで検査したが出荷しておらず、しかも 2 つの半分が別のコミットからビルドされていた。2.9.9 はこれを置き換え、1 つの `main` から両方を作れるようにする。実質的な変更は、スコアが**価格の絶対差**を固定の定数と比べていた指標が、3,000 円の銘柄と 13 ドルの銘柄で同じ意味を持ち得なかったことへの対処である——スコアは、測っているはずの距離に対して単調ですらなかった。

### 修正
- **出荷バイナリがビルドした者のアカウント名を名乗らなくなった。リリースを CI でビルドするため。** rustc は panic がファイルを示せるよう各クレートのソースパスを記録する。開発機ではそのパスがホームディレクトリを通るため、エンジンには `C:\Users\<名前>` が **689 箇所**、デスクトップシェルに 253、設定アプリに 189、そしてブラウザが毎回ダウンロードする WASM に **85 箇所**含まれていた。`--remap-path-prefix` は実測したが、rustup が配る `std` の rlib に焼き込まれたパスと、C ツールチェーン（`aws-lc-sys`）が cc-rs 経由で埋めるパスには届かない。よって、アカウント名が誰も特定しない場所でビルドするのが解である。`.github/workflows/release-windows.yml` が一連のビルドを runner 上で行い、どれか 1 つでもビルド用でないアカウントを名乗る現物があればビルドを失敗させる。その判定が `scripts/check-no-host-paths.sh` で、`scripts/inspect.sh` もゲート 9 として実行するため、ローカルビルドのバイナリは出荷検査も通らない。あわせて「検査は CI が生成した現物に紐づく」という規約にも一致する。
- **バンドラが sidecar 2 本を配置し、版数の合わない現物を拒否する。** `bundle-engine.sh` はエンジンだけを配置し、設定アプリは手作業に任せていた。さらにリリース記録に書かれた recipe が v2.9.2 以降、その 2 本目について「ビルドする」の語を落としていた——そのため 2.9.9 のインストーラが 2.9.8 を報告する設定アプリを抱えて作られ、書かれた手順どおりに進めても気づけなかった。いまは両方を配置し、どちらかを複製する前に両方を解決・検査する（片方だけ新しい状態が、まさに古い現物を同梱する状態である）。`Cargo.toml` と版数が合わない現物は、出荷ではなくバンドルの失敗になる。検査は版数を ASCII **または** UTF-16LE として読む——設定アプリは PE リソースの写しだけを持つためである。あわせてセキュリティ診断の §C.1 にビルド recipe を 1 回だけ記し、リリース記録は劣化する手順を書き直すのではなくコミットとハッシュを名乗る形にした。`.gitattributes` は、このプロジェクトがビルドするすべてのプラットフォームで shell script を LF に保つ。
- **VWAP スコアを乖離率で採るようにし、どの銘柄でも同じ意味になるようにした。** 5 段階の判定を `終値 − VWAP`——銘柄自身の通貨建ての価格差——に対して固定の 4.0 / 1.0 で行っていたため、スコアが「測っているはずの距離」に対して単調ではなかった。日足での実測: 13 ドルの F は VWAP より 5.03% 下で **0**、3,000 円台の 7203.T は 2.19% 下で **−2**。MSFT は 0.47% 上で +1、T は 1.54% 下で 0 だった。判定を `(終値 − VWAP) ÷ VWAP × 100` に対する 3.0% / 1.0% に変更した——表示が既に「対VWAP比」として出していた値そのものなので、表示とスコアが食い違うことはなくなる——また、割り算が信頼できない VWAP ではスコアを代用の 0 にせず欠損のままにする。分足ではスコアが 0 に留まることが格段に多くなるが、これは不具合ではなく読み取り結果である: VWAP はセッションごとに再スタートし、終値はセッション平均から 0.9% 以内に収まっていた（実測）。あわせて、ガイドの逆張り VWAP レシピが符号を逆に書いていた——価格が VWAP より**下**にどれだけ伸びたかを問いながら、終値が上にある場合の `+2` を挙げていた。([src/technical/indicators.rs](src/technical/indicators.rs), [src/utils.rs](src/utils.rs), [docs/manual/analysis-guide.md](docs/manual/analysis-guide.md))
- **EMA・SMA・一目のスコアも乖離率にし、指標ごとに校正した。** 3 指標とも価格差を 1 つの共有定数 2.0 / 0.5 と比べていたため、スコアが「開き」ではなく株価水準を読んでいた。20 銘柄の日足での実測では、**日本株は実際の開きにかかわらず一目で全て ±2 に達し——1605.T は基準線から 0.16% で −2、F は 1.89% で 0**——EMA・SMA も同様だった。各指標は、自分の表示が既に出している率で採点する: EMA と SMA は `(短期 − 長期) ÷ 終値 × 100`、一目は `(転換線 − 基準線) ÷ 基準線 × 100`。対を分けたのは、3 指標が構造的に別の開きを測っているからである——EMA と SMA は 5/20 期間を共有するが単純平均のほうがトレンド中に遅れ、一目の 9/26 はさらに広い。実測の中央値は 1.03% / 1.60% / 2.47% だった。よって帯は **EMA ±2.0% / ±0.5%、SMA ±3.0% / ±1.0%、一目 ±4.0% / ±1.0%** とし、いずれも標本が中立・やや・強いの各帯に分かれるようにした（1 つの帯に潰れない）。4 箇所に書き写されていた 5 段階の match は 1 つの `five_band_score` になり、4 指標が自分の対を渡して呼ぶ。割り算が信頼できない終値・基準線ではスコアを代用の 0 にせず欠損のままにする。 一目の表示側にはこの欠陥そのものへの回避策があった——分足で開きが 1% 未満のときに「大幅に」を「上回る」へ和らげていた（±2 が 1% 未満の開きから出ていたため）。±2 が 4% を要するようになった今その状態は起こり得ないので、回避策と付随する試験 2 件を削除し、日足も分足も同じ文言表から読む。([src/technical/indicators.rs](src/technical/indicators.rs), [docs/manual/analysis-guide.md](docs/manual/analysis-guide.md))
- **フィボナッチのスコア行が、スコアの適用したルールを述べるようにした。** 50% 水準からの距離を名乗っていた（「終値が50%より+2.00超」「50%±0.50内」）が、±2 を決めるのは中点からの距離ではなく 38.2% / 61.8% 水準である。したがって文言が、説明しているスコアと矛盾し得た——50% 水準より **1.64** 下の終値が「2.00 超下」と報告されていた。±1 / 0 の帯は確かに距離で決まるが、文言は設定を変えても 0.50 をハードコードしていた。いずれもコードの判定どおりに、スコアが使うのと同じ関数から述べる。([src/render_indicators.rs](src/render_indicators.rs))
- **フィボナッチの中立帯を `±1` の帯が持つ幅に対する割合にし、上限を設けて `±1` が到達不能になり得ないようにした。** `FIBONACCI_NEUTRAL_EPS` は価格の絶対差（既定 0.5）で、銘柄ごとに別の意味になっていた。日足での実測では 9984.T ではその幅の 0.2% にすぎず `0` が実質到達不能で、**F では幅そのものより大きく、`+1` の条件が「終値が 15.080 超かつ 14.984 未満」という空区間になっていたため `+1`・`−1` が一切出なかった。** PFE と T でも幅の 76%・60% を食っていた。これを `FIBONACCI_NEUTRAL_RATIO`（`--fibonacci-neutral-ratio`、チャットは `/set fib-ratio`）に変更し、50%→38.2% 距離に対する割合、既定 **0.05**、受け取るすべての経路で **0.0〜0.5** にクランプする——この上限が、到達不能を「起こりにくい」ではなく「構造的に起こり得ない」ものにしている。ガイドの表は `0` を「中央付近・迷い」と書いていたが、20 銘柄の実測では 19 銘柄の終値が値幅の 6% 超離れていたので、実態どおり「めったに入らない境界のブレ止め」と書き直した。**旧キーはもう読まない: 既存の `xoksa.env` にある `FIBONACCI_NEUTRAL_EPS` の行は無視されるので、書き換えること。** ([src/config.rs](src/config.rs), [src/technical/indicators.rs](src/technical/indicators.rs), [src/setup.rs](src/setup.rs), [src/chat/exec.rs](src/chat/exec.rs), [xoksa-setup/src/main.rs](xoksa-setup/src/main.rs), [xoksa-setup/frontend/index.html](xoksa-setup/frontend/index.html), [xoksa.env.sample](xoksa.env.sample))
- **レシピが、エンジンの採点どおりにスコアを読むようにした。** スコア表は直したが、それを使う活用例を直しておらず、2 箇所で符号が逆だった。xoksa のボリンジャースコアは**平均回帰**の読みで——バンドより上の終値は過熱としてマイナスに出る——それなのにブレイクアウトのレシピは上抜けを「`+1`〜`+2` に跳ねる」と書き、底拾いのレシピは `-2` を「下側バンド下抜け」と説明していた（`-2` はバンドの反対側のスコアである）。どちらもエンジンが出す符号に直し、符号が方向ではなく平均回帰を読んでいることを明記した。ADX `+1` を 25、`+2` を 40 と書いていた箇所が 4 つあったが、帯は 30 と 50 である。RSI の値を「65 以上で赤く強調」と書いていた箇所もあったが、値は色を付けずに表示されており、そもそも 65 はコード上の閾値ではない（売り圏は `sell-rsi`、既定 70。このレシピがそれを下げている）。フィボナッチの中立行も「eps 以内」と書いていたが、ちょうど eps は `±1` なので「eps 未満」に直した——こちらは帯の幅がゼロの場合も正しくなる。いずれも日英両方。([docs/manual/analysis-guide.md](docs/manual/analysis-guide.md))
- **その境界規約の退化ケースと、それに追随すべき表示行。** 比較を境界込みにしたことで採点の欠陥を 2 件作ってしまい、いずれもレビューで見つかった。幅が**ゼロ**のボリンジャーバンド——期間中ずっと終値が動かず上下の端が一致する場合——では上限側の条件が先に成立し、**完全に横ばいの系列を `-1` で減点していた**。外に出るという状態が存在しないので、幅ゼロのバンドはスコア `0` とする。また `fibonacci_neutral_ratio = 0`（中立帯を無くす、許可された設定）では `+1` と `-1` の条件が 50% 水準で重なり、分岐順だけで**中央が上へ傾いていた**。各 `±1` の分岐に「終値が中点のどちら側にあるか」を加えたので、帯の幅が何であれ中点は中立であり、幅が正のときは境界が `±1` に属する規約も保たれる。表示行も 2 つ追随させた。ボリンジャーの位置行は `%B > 1.0` で判定していたが、スコアは価格とバンドを境界込みで比べるため、終値が上限ちょうどのとき `-1` の隣に「バンド内 → 中立」と出ていた——いまはスコアを読む。あわせて「バンド内で上側優位」というヒント行を廃止した（旧来の厳密比較では到達不能な死んだコードだった）。フィボナッチの行は「38.2% 水準より上」と書いていたが、その水準ちょうどは `+2` である。退化ケースを固定し、計算から表示テキストまで通す試験を 4 件追加した。([src/technical/indicators.rs](src/technical/indicators.rs), [src/render_indicators.rs](src/render_indicators.rs), [docs/manual/analysis-guide.md](docs/manual/analysis-guide.md))
- **境界の規約を 1 つに統一した——閾値ちょうどは、その閾値を満たしたものとする。** コードベースには 4 通りの規約が同居していた。乖離率の各指標・ADX・ストキャスは境界ちょうどの値を**強い**側の帯に入れ、ROC とボリンジャーは**弱い**側に入れ、フィボナッチは**内側**の帯に入れたうえで全域を覆うために fall-through の分岐を必要としていた。読む側は指標ごとに別の規則を覚える必要があり、マニュアルも、適用される規約と一致していない箇所があった。9 つの段階のうち 6 つは既に「閾値ちょうどは満たしたものとする」に従っていたので、残る 3 つをこれに合わせた: ROC は `five_band_score` そのものになり（独自 match を削除）、ボリンジャーの比較は境界を含み、フィボナッチの 5 つの分岐は隙間も重複も fall-through も無しに全終値を覆う。スコアが変わるのは同値のときだけである——ROC がちょうど ±3% か ±10%、終値がボリンジャーのバンドちょうどかその 2% 外ちょうど、終値が 38.2%／61.8% 水準ちょうどかフィボナッチ中立帯の端ちょうど。**これを固定する試験は 1 つも無かった**——指標ごとの試験がすべて境界を外した値を使っており、そのために 4 通りの規約が気付かれずに共存していた。各段階の境界ちょうどを検査する試験を2 件追加した。ガイドは冒頭でこの規約を 1 回だけ述べ、各スコア表は行の間に重複の無い不等号表記にした。([src/technical/indicators.rs](src/technical/indicators.rs), [docs/manual/analysis-guide.md](docs/manual/analysis-guide.md))
- **表示している行が、境界でもそれ以外でもスコアと一致するようにした。** 乖離率への変更をレビューしたところ、表示側が追随していなかった。EMA ブロックは 2 本の脚を依然として**価格差**（±0.01 未満）で分類し「同値圏 → スコア変動なし」と出していた——終値 0.459 で EMA の差 0.00747 は乖離率 1.63% なのでスコアは `+1` であり、同じブロックがそれを否定していた。しかも同じ系列を 1,000 倍すると文言だけが変わった。今は SMA・一目と同じく符号で比較し、スコアが既に下した分類を作り直さず、線の位置だけを述べる。EMA ブロックは `unwrap_or(0.0)` を使う唯一の指標でもあり、算出できなかったスコアが中立の `0` として表示されていた——security-design §1 がまさに守っている区別である。他の 8 指標と同じく欠損を欠損として扱う。フィボナッチの `±1` の文言は「38.2% 水準未満」と書いていたが、その水準ちょうどは `+1` になるので「以下」に改めた。「境界に乗っても必ず + / - に倒す」という注記も事実ではなかった。マニュアルでは乖離率の 4 つの表が「+0.5% 超」と書いていたがコードは `>=` なので、ちょうど +0.5% は `+1` なのに表は `0` と読め、中立行は両隣と端点で重複していた。8 つの表（4 指標 × 日英）すべてを、コードと一致し重複のない不等号表記に改めた。ボリンジャーは逆の誤りで、「上限より 2%以上上 → -2」と書いていたがコードは厳密に `> upper × 1.02` なのでちょうど 2% 上は `-1` である。英語のフィボナッチ節には、新しい割合の説明の隣に旧来の固定 ±0.50 の中立帯が残っていた。計算から表示テキストまで通す試験を 2 件追加した——今回の不一致はすべてそこに隠れていた。([src/render_indicators.rs](src/render_indicators.rs), [docs/manual/analysis-guide.md](docs/manual/analysis-guide.md))
- **取得失敗が、実際に何が起きたかを述べるようにした。プロバイダのページがメッセージに載ることもない。** 未知のティッカーに対して 3 つのことが同時に起きていた。HTTP 層が非 2xx の応答でステータスだけを読んで本文を捨てていたため、Yahoo 自身が 404 と**一緒に**返す理由——`{"chart":{"error":{"code":"Not Found","description":"No data found, symbol may be delisted"}}}`——が失われ、利用者には「Market data API request failed」しか届かなかった。次に failover が主プロバイダのエラーを丸ごと捨てており（`Err(_)`）、Stooq の代替も失敗したときに代替側の不満だけが表に出ていた。そして Stooq の anti-bot JS チャレンジ（HTML ページ）を壊れた CSV として扱い、**そのまま引用**していたため、「unexpected header」という見出しの下に `<!DOCTYPE html>…` が端末に出ていた。修正後は、エラー本文を読んでその理由を報告し（`HTTP 404 (Not Found — No data found, symbol may be delisted)`）、主プロバイダの理由を見出しにして代替の結果を後ろに添える（未知のティッカーを知っているのは主プロバイダの側）。HTML の応答は引用せず「代替プロバイダが遮断された」と名指しし、プロバイダ由来の非信頼テキストはすべて `bounded_provider_text` を通して長さを切り、制御文字を空白に置き換える（応答が、自分が印字される行を書き換えられないようにするため）。([src/market.rs](src/market.rs), [docs/dev-prog/source-map.md](docs/dev-prog/source-map.md))
- **`--env-file` のヘルプ文が、エンジンに無いフォールバックを約束していた。** 「canonical app-data path, else ./xoksa.env」と書いてあったが、security-design §6 は暗黙の `./xoksa.env` は無いと定めており、コードもその通りだった。誤っていたのはヘルプ文字列だけである。([src/config.rs](src/config.rs))

### 削除
- **指数名にファンドを返すのをやめた。レバレッジ商品を返すことは二度としない。** `normalize_ticker_input` が、他のどこよりも先にティッカーを黙って書き換えていた: `S&P500`／`SNP500`／`SP500` → `SPY`、`NASDAQ100`／`ナスダック100` → `QQQ`、`DOW`／`DJIA`／`ダウ平均` → `DIA`、`日経平均`／`NIKKEI225` → `1321.T`、`TOPIX` → `1306.T`。利用者は指数を求めたのに、全指標・全スコア・LLM の解説が連動ファンドに対して計算され、その旨は画面のどこにも出なかった。**`FANG+`／`FANGプラス` は `FNGU`——3 倍レバレッジ ETN——に解決されており**（実測で QQQ の 3.1 倍の値動き、ボリンジャー幅は 2.5 倍）、利用者が FANG+ を見ていると思っている間、値動きが 3 倍に増幅された別物の数値を読んでいた。`全世界`／`オールカントリー` は別商品の通称なのに iShares ACWI ETF を指しており、`ACWI` → `ACWI`・`VTI` → `VTI` は恒等の無駄行だった。これらはすべて削除した——xoksa が扱えない指数名は、別のもので答えるのではなく未知のティッカーとして失敗する。残した展開は `全米`／`トータルマーケット` → `VTI` の 1 つだけで、これは別物へのすり替えではなく行き先そのものを指す通称である。表示名も連動先の指数を括弧で添えるのをやめた（`Invesco QQQ Trust (NASDAQ100)` はファンドが指数そのものだと読めた）。そこにあった `FANG+` の項目はもともと到達不能だった——書き換えが先に起き、そもそも `sanitize_ticker` はティッカーに `+` を認めない。これらはユーザー向けにどこにも記載が無く、source-map はこの関数を「前後スペース除去・大文字化」とだけ説明し（返り値にはそれさえしていない）、`HardcodedInfo` が持っていたことのない国コードを持つと書いていた。([src/bootstrap.rs](src/bootstrap.rs), [src/technical/indicators.rs](src/technical/indicators.rs), [docs/dev-prog/source-map.md](docs/dev-prog/source-map.md))
- **誰も呼んでいない関数 4 件。** ここでは `pub` は「使われている」ことの証拠にならない。エンジンは lib を公開しているが、それに依存するクレートが無い——デスクトップと設定アプリはエンジンをリンクせず子プロセスとして起動する——ため、`pub` な項目に `dead_code` が出ない。1 件ずつ確認し、`src/server/api.rs` で未使用に見えた 18 件はルータ登録で使われていることを確かめた。`score_to_string`（[src/utils.rs](src/utils.rs)）は未使用と doc に明記済みで、実際に表示されるスコアは各呼び出し側の `{:.1}` による。`fetch_market_data`（[src/market.rs](src/market.rs)）は存在しない lib 利用者のための「互換の入口」を自称していた。`verify_text`（[src/integrity.rs](src/integrity.rs)）は `verify` の薄いラッパー。**`get_entry`**（[src/technical/types.rs](src/technical/types.rs)）は `&TechnicalDataEntry` を丸ごと返していた——不変参照なので書き込みはできないが、guard を経由せず確定データを読める口であり、security-design §1 がまさに防いでいるものである。呼び出しが無いうちに閉じるなら代償は無い。

## [2.9.8] — 検査済み・2.9.9 で置き換え

**整合性ガードが、正しい文を消さなくなった。** 2.9.7 のマージ前レビューで見つかった 3 件。いずれもガードが「捏造値を通す」側ではなく「正しい文を消す」側に誤る不具合で、いずれも英語の回答でだけ踏み、日本語では影響が無い。

### 修正
- **数値の役割を、捕捉した断片ではなく、その文から読む。** 裸価格チェックと数値単位チェックが、各数値を切り出した断片から読み直していた。断片には、それを日付や期間の長さにしている語が入っていない。`Support has held since 2019.` は確定データに対応の無い観測値と見なされ、文ごと削除されていた。`The 200-day moving average has acted as support.` も同様である。文全体を読む検知器1は同じ数値を正しく分類するので、判定が互いに食い違っていた。数値は文から読み、捕捉範囲に入るものだけを対象にし、断片の位置ではなく**その数値自身の位置**で検証するようにした。アラートの EXPLAIN 注記では影響がより大きく、指摘が 1 件でも注記は丸ごと破棄されるため、年号に触れた正しい注記が失われていた。([src/llm.rs](src/llm.rs))
- **価格文脈・売買文脈の判定は、部分一致ではなく単語で行う。** `contains_any_term` が `contains` で判定していたため、`low` が *below*・*allow*・*following* に、`line` が *decline*・*headline*・*online* に、`close` が *closely* に当たっていた。英語の回答のほとんどで価格文脈のゲートが開き、上記の不具合と重なって文を消す装置になっていた。両ゲートとも `integrity::term_positions` を通すようにした。指標名の結び付けが使っているのと同じ境界規則で、ASCII の語は語として立っているときだけ数え（英語の複数形は同じ語とみなす）、境界を持たない日本語の語はそのまま照合する。([src/llm.rs](src/llm.rs), [src/integrity.rs](src/integrity.rs))
- **`safe_ratio` が、ゼロと区別できない分母を拒否する。** 2.9.7 でプロバイダごとに散っていた比率のガードを 1 つの関数へ集約した際、条件が `abs() > f64::EPSILON` から `!= 0.0` に狭まっていた。厳密なゼロしか拒否しないため、EPS が `1e-300`、価格が 180 のとき PER が `1.8e302` になる。有限なので `FundamentalData` に保存され、画面に出て、LLM へ送られ、モデルの引用を照合する確定値として登録される。doc コメントは旧来の動作を説明したままだったので、コードをそちらに合わせた。([src/fundamental.rs](src/fundamental.rs))

### セキュリティ
- **`rustls` を RUSTSEC-2026-0285 の対象外まで更新。** この助言は 2026-09-14 に公開された。v2.9.7 の出荷検査で `cargo audit` がクリーンだった翌日であり、**2.9.7 をそのまま出していれば脆弱性を抱えたまま出荷していた**ことになる。TLS 1.3 のハンドシェイクメッセージを暗号化レベルの境界をまたいで受理する問題（5.3 medium）。`rustls` 0.23.32 → 0.23.45、あわせて `rustls-webpki` 0.103.13 → 0.103.15。この crate は外向き HTTPS のすべての経路——市場データ・ニュース・LLM プロバイダ・チャット通知——に入っている。

### 既知の制限
- 文中のどこかに年があると、別の節に書かれた読み取り値までその年の主張として扱われる。`Price has been in this range since 2019, and RSI is 45.2.` は 45.2 が確定値そのものであるにもかかわらず拒否される。節の切り分け自体は正しく働いており、原因は年の修飾が「自分の節に無ければ文中でいちばん近いものを採る」というフォールバックを持つことである。これは `In 2019, revenue was 13.7兆円.` のような形で必要であり、ここでは有害になる。両者を区別するには「テクニカルの読み取り値は対象足で日付が決まり、文中の別の場所に書かれた年では再日付しない」と規範に書く必要があるため、出荷直前に変えず #146 で追跡する。

## [2.9.7] — 未リリース

**確定データと AI 出力の検査を、データ種別と表示言語によらず同じように効かせた。** 文書が掲げる保証と実装の差が 4 点あった。計算できなかった指標が有効なゼロとして判定に入る、5 つの整合性検査のうち 4 つが日本語表現でしか発動しない、同じ数字が入力のどこかにあるだけで数値が通る、ファンダメンタルの値が検証を経ずに構築・変更できる、の 4 点である。

### 修正
- **未計算の指標をゼロとして扱わない。** `TechnicalDataGuard` が「実際に計算された指標」を記録する（セッターが有限値で呼ばれたときだけ記録し、後の再計算に失敗したときは前回の値と記録を両方消す）ため、生成されなかった値——未計算・計算失敗のいずれも——は欠損となり、正常に計算された `0.0` は存在する。`guard_indicator_map` は計算済みの指標だけを載せるので、10 本のデータで 20 期間 EMA は欠損となり、`終値 > EMA` は成立しない。欠損指標を参照する条件は false となり、既存の AND／OR 規則は変わらない。この表はバックテストとアラート監視の共通経路なので両方に効く。拡張指標の失敗は破棄せず記録する。([src/technical/types.rs](src/technical/types.rs), [src/backtest.rs](src/backtest.rs))
- **整合性検査を英語の回答にも効かせる。** 検査を 1 か所の共通語彙で定義し、同じ検査の日本語表現と英語表現を同居させた。検査項目を言語ごとに別管理しない。数値単位・価格文脈・売買文脈・VWAP の方向比較は、従来は日本語表現でしか一致しなかった。正規化が空白をすべて削っていたため英語の単語境界が壊れており、方向判定は空白を保った形とも照合する。([src/llm.rs](src/llm.rs))
- **数値は、その文が主張する対象に帰属していなければ通さない。**「同じ数字が入力にあるか」という判定を、確定データ（銘柄・指標・値・単位・符号・対象足）との照合に置き換えた。**照合の基準は `TechnicalDataGuard` と `FundamentalData` から構築して検査側へ渡す。テキストを出所として読まない。** プロンプトは基準になり得ない——会話・利用者自身の発言・他モデルの見解も載っており、そこに書かれたラベルはエンジンが書いたものと区別できないためで、利用者が打った数字が「確定値」になることはこれで無くなった。LLM に到達する全経路がこのデータを持つ（CLI 単発分析・チャット（`/basic`・`/forum`・ディベートを含む）・Web セッション・Web のマルチタイムフレーム分析（足ごとに 1 件、各件が自分の足を名乗る）・アラートの EXPLAIN 注記）。再取得では表示テキストとプロンプト文脈と同時に確定データも入れ替えるため、更新後の値は通り、更新前の値は通らない。ファンダメンタルは 3 つの結果を区別する：再取得しなかった場合は表示・プロンプト・基準をそのまま保持し、取得に失敗した場合は 3 つとも落とし、取得に成功した場合は 3 つとも入れ替える。市場データの通貨は確定データまで運ぶので、ファンダメンタルの取得有無にかかわらず米国株のドル表記が通る。帰属は回答単位ではなく**主張 1 つずつ**に判定する。1 文が複数の主張を運ぶため、銘柄・足・年はそれが書かれた主張に結び付ける（切れ目は 、 , ; と *and* / *while* / *but*。日本語のように図の前に置く修飾も、英語のように後に置く修飾も同じに読む）。「AAPL は $168.8、MSFT は $400」は各数値をそれぞれの発行体に帰属させ、入れ替えた対は拒否する。2 つの足、2 つの会計年度を 1 文に並べた場合も同様である。通常語と綴りが同じコード（`IT`）や 1 文字のコード（`A`・`T`）も、読み込み済みなら通常語フィルタより先に銘柄として照合し、分析していない足には確定値が無いので拒否する。具体的な指標語と汎用語の併記でも汎用語を認識するので、「RSI is 45.2 and EMA is 166.88」は各数値を自分の指標に結び付ける。指標名は**語として**一致した場合にだけ数える——`remains` の中の `ema` や `period` の中の `per` は指標名ではなく、日本語の `VWAP168.96` や英語の複数形は従来どおり結び付く。同じ指標の 2 本の脚は計算期間だけが違うので、期間を値と一緒に持つ：日足の分析では `EMA(20)`・`EMA(20 days)`・`EMA（20日）`・`20日EMA`・`The 20-day EMA` が、月足の分析では `EMA(20 months)`・`The 20-month EMA` が同じ主張として、名乗った脚を選ぶ。計算していない期間には確定値が無く、期間はそれが書かれた指標に適用する（EMA の隣の `RSI(14)` が EMA の脚を選ぶことはない）。照合するのは期間の本数であり、述べられた時間単位と分析足の整合は検証していない。数値はまず「文中で何をしているか」で読む——観測値・期間パラメータ（`RSI(14)`・`1時間足`）・日付（`2026-09-08`・`in 2019`）——ので、窓長や日付を指標値として照合せず、終値 2025 は年ではなく価格として扱う。確定値がまったく無い指標に読み取り値を与える文は素通りさせず拒否する。丸めは書かれた桁の単位で判定し（確定値 13.704兆円 に対する「約13.7兆円」は通し、13.8兆円 は拒否）、倍率は桁だけを表すので株数は「12 million」と書けても「$12 million」とは書けず、通貨は金額にしか付けられない（`Volume is $1000` は拒否）。通貨記号・倍率語・後置の通貨語を一体として解析し（`13704 billion dollars` は円建てではない）、括弧は免除ではないので `RSI(14) is 45.2` は 1 つの値の主張、`RSI (value: 99.99)` は捏造値の主張として扱う。通貨を伴う数値は足にならない（`$5m` は 5 分足ではなく金額）。数値・日付・期間指定・ティッカーの範囲は 1 文につき 1 回の走査で確定し、主張の分割と全帰属判定が同じ結果を共有するので、桁区切りは主張を分断せず（`13,704,000,000,000` は `13704000000000` と同じ挙動）、同一の日付は `2026-09-08`・`2026年9月8日`・`September 8, 2026` のどれでも同じに読む——完全な日付は観測の時点を示し、主張の時点を与えるのは年として書かれたものだけである。免除はトークン全体の形で判定するので、月名は実在名を日付構文の中で固定して読み（開いた読み方は *margin* や *declined* にも一致する）、金額は倍率接尾辞を含めて解析し、接尾辞が数字に直結していても離れていても同じに読む（`$168.8m`・`$168.8 m`・`$168.8 million` は同一の金額。百万・十億も million・billion と同じ）。桁の違う表記は拒否する。理解できない接尾辞は読み捨てない。符号は通貨記号のどちら側に書かれていてもよい（`-$2.50` と `$-2.50` は同じ金額）。エンジン自身が出す 1 株あたり・1 単元あたりのファンダメンタル行は確定値であり、`1株あたり`／`per share` の `1` は読み取り値ではなく基準の一部として読む。前足比は金額と百分率の両方を確定値とし、フィボナッチの比率は水準と書かれている場合にだけ識別子として扱う。テクニカルは対象足の年・ファンダメンタルは自身の会計年度と照合し、英語の文境界は小数点やティッカーのドットで切らない。除去も同じ文境界を使うため、散文では正しい文は誤った文の隣にあっても日英どちらでも残る。表の行・箇条書き・1 文しかない行は行ごと除去し、アラートの EXPLAIN 注記は丸ごと破棄する。VWAP の向きも確定値から読むため、表示行のラベルを変えても判定は変わらない。([src/integrity.rs](src/integrity.rs), [src/llm.rs](src/llm.rs), [src/main.rs](src/main.rs), [src/chat/mod.rs](src/chat/mod.rs), [src/server/monitor.rs](src/server/monitor.rs))
- **ファンダメンタルを検証なしに構築・変更できないようにした。** `FundamentalData` のフィールドを非公開にし、書き込む経路はいずれも書き込む値を検証するようにした。プロバイダの取得結果からの初期構築は `FundamentalData::build` に集約し、取得途中の `FundamentalInputs` を受け取り、非有限値を排除し、PER・PBR・ROE を自身で計算する（各プロバイダ経路で別々に計算していたものを 1 か所に集約）。`Default` は数値を持たない空の値を作るだけで、BPS と価格の後からの更新は検証付きの公開メソッドを通る。`recompute_derived`／`set_bps` により派生値が入力と食い違ったまま残らず、比率の元になった価格も一緒に保持する。ゼロ除算とオーバーフローは無限大を出さず未定義とする。正常なゼロと、赤字・負の EPS のような意味のある負値は保持する。テクニカル側の `TechnicalDataGuard.entry` も非公開にし、同じ抜け道を閉じた。([src/fundamental.rs](src/fundamental.rs), [src/technical/types.rs](src/technical/types.rs))

- **アラートのルールは、どの画面から作っても保存される。** チャットの `/alert add` はメモリ上にしか存在せず、`/alert del` は `xoksa.env` の `ALERT_<n>_*` を残していたため、消したルールは再起動で復活し、足したルールは残らなかった（保存していたのはダッシュボードだけ）。両方の経路を同じ永続化に通し、書き込みに失敗したときは成功と答えずに報告する——`AlertOpResponse` に `warning`（メモリには反映・保存は失敗）を持たせ、`error`（何も起きていない）と区別し、画面に表示する。上限 `1..=16` は走査側と追加側に二重で埋め込まれていたが、`config::ALERT_MAX` の 1 定義にまとめ、上限を超える番号は黙って無視せず起動時の警告に列挙する。ルールは自分のスロットに上書きして書き、隣に足さない——従来は、分足でないルールがストア上の番号だけ空けたときに同じ番号の記述が二重になり、`EXPLAIN` は true のときしか書かないため `explain=false` で保存しても古い `true` が生き残っていた。([src/config.rs](src/config.rs), [src/server/monitor.rs](src/server/monitor.rs), [src/server/api.rs](src/server/api.rs), [src/chat/exec.rs](src/chat/exec.rs), [webui-leptos/src/main.rs](webui-leptos/src/main.rs))
- **`xoksa.env` の書き手を 1 つにした。** アラートのルールを同時に保存するとほとんどが失われた（4 件すべて Ok を返して読み戻せたのは 1 件）。読み取り・編集・置換が互いに競合していたためである。この一連を単一の排他区間にまとめ、アラート経路と設定経路が同じ `utils::rewrite_env_file`（パス解決の隣）を通るようにし、番号の割り当てと書き込みを同じロックの中に入れた。アラートの書き込みと `LANG=en` の書き込みが互いを消すことはなくなった。置換は原子的である——一時ファイル（`create_new`、Unix では `mode(0o600)`）に書き、元の権限を写してから rename する。失敗しても元のファイルは無傷で、一時ファイルの残骸も出ない。行の判定はローダ自身の `parse_env_line` を通すので、`export ALERT_3_EXPLAIN=true` も先頭の BOM もローダと同じに認識し、コメントと `ALERT_10_*` は触らない。`env::vars()` は `vars_os()` に置き換えた——無関係な環境変数の値が非 Unicode でも走査が panic しない。([src/utils.rs](src/utils.rs), [src/server/monitor.rs](src/server/monitor.rs), [src/config.rs](src/config.rs))
- **2 つの UI シェルが `xoksa.env` を明示的に解決する。** デスクトップアプリと設定アプリは相対パスで読んでいた。正準ファイルに当たっていたのは、どちらも起動時に設定ディレクトリへ `set_current_dir` しているからにすぎず、しかもその結果は破棄している。両方とも `xoksa_paths::env_file()` で解決するようにした（セキュリティ設計 §6 のとおり、暗黙の `./xoksa.env` は無い）。([xoksa-desktop/src/main.rs](xoksa-desktop/src/main.rs), [xoksa-setup/src/main.rs](xoksa-setup/src/main.rs))
- **アラートの EXPLAIN 注記は、発火したルールだけを再掲してよい。** 通知にはエンジンが判定した条件（`score <= 5`）が載るのでモデルは引用してよい。文中の比較は語の出現から推測せず構文として解析し、解析した指標・向き・等号の有無・値のすべてが一致することを求める。`score >= 5` や `score < 5` は別のルールなので拒否し、ルールが 5.4 のときの `score <= 5` も拒否する——表示丸めが変えてよいのは読み取り値の末尾であって条件ではない。数値はルールが書いたとおり裸で書かれていなければならない。`score <= $5`・`score <= 5%`・`score <= 5foo` はルールが言っていないことを述べている。閾値に単位が付いた場合は認識されなくなるのではなく許容されなくなるので、読み取り値の照合へ戻さず**条件として**拒否する（戻していたために、終値 3091 の銘柄で `close > ¥3091` が通っていた）。演算子が掛かるのはそれが比較している数値なので、「score is at or below 5（現在のスコアは0.0）」では 5 をルールとして、0.0 を読み取り値として別々に検査する。指標が**その値である**と述べる文は依然として捏造として扱う。([src/integrity.rs](src/integrity.rs), [src/server/monitor.rs](src/server/monitor.rs))
- **アラート画面と英語マニュアルが「確定足で」と書いていた。** 監視が判定に使うのは**取得した最新の足**であり、日本株の分足ではリアルタイム気配を含む形成中の足になる。足の確定は待たない。日本語のマニュアルは上限・再起動の注記とあわせて訂正済みだったが、ダッシュボードのアラート説明文・`AlertRule` の doc コメント・英語マニュアルは旧挙動のままだった。([webui-leptos/src/main.rs](webui-leptos/src/main.rs), [src/config.rs](src/config.rs), [docs/manual/usage-guide.md](docs/manual/usage-guide.md), [docs/manual/command-reference.md](docs/manual/command-reference.md))

### 変更
- **アラートに関するマニュアルと正典の文言。** マニュアルに、ルールは 16 件までであること、監視するのはルールに書いた銘柄でその銘柄を画面に出しておく必要はないこと、判定に使うのは取得した最新の足であること、`xoksa.env` を直接編集した場合は次回起動から効くことを明記した。セキュリティ設計の「in-session monitor」は `xoksa serve` 内のバックグラウンドタスク（プロセス全体で 1 つ、バックグラウンド常駐なし、ダッシュボードを開いているかにも依存しない）へ訂正し、何を監視するのか、ルールを起動時に一度だけ読むことを書き加えた。([docs/manual/usage-guide.md](docs/manual/usage-guide.md), [docs/manual/command-reference.md](docs/manual/command-reference.md), [docs/manual/setup.md](docs/manual/setup.md), [docs/dev-prog/security-design.md](docs/dev-prog/security-design.md), [docs/dev-prog/source-map.md](docs/dev-prog/source-map.md))
- **エンジン・デスクトップアプリ・設定アプリ・Web UI のバージョンを 2.9.7 に統一。** 各サブクレートの `Cargo.lock` も含む。MSI 名は `XOKSA-2.9.7-x64.msi`。

### 既知の制限
- 確定データを**渡さない**経路は 2 つある——銘柄を読み込んでいない自由文の会話と、ニュース仕訳（渡したタイトルを仕分ける作業であり、数値の出所はタイトルであって市場データではない）。いずれも従来の存在確認（与えた入力にその数値が存在するか）にフォールバックし、帰属を検証したとは主張しない。それ以外の経路では確定データがリクエストと共に運ばれる（Ollama ベンチマークを含む）。
- ドット無しの大文字トークン（`MSFT`）を銘柄と読むのは、そのセッション自身の銘柄がその書式である場合に限り、2 文字以上で、かつ guard 自身の語彙にも列挙した通常語・略語（`I`・`IT`・`AI`・`CEO` 等）にも該当しない場合に限る。したがって日本株のセッション内で引用された米国銘柄コードは「別銘柄」として検出しない。その数値は確定値との照合を受ける。
- プロバイダが通貨を報告しない場合（ラベル付きの Stooq フォールバック）、書かれた通貨は拒否しない——不明は書かれた内容を否定できないためである。数値そのものは検証する。

---

## [2.9.6] — unreleased

**A fabricated figure in a section heading used to reach the screen.** The integrity guard sanitized body lines but emitted headings unchecked, so an out-of-input number in a heading was counted as an issue and displayed anyway — the guarantee stated in [security-design.md](docs/dev-prog/security-design.md) §1 did not hold for headings. Fixed, and the benchmark's assessment scale is corrected alongside it.

### Fixed
- **Section headings are now sanitized like any other line.** `render_guarded_ollama_section` puts the heading through the same `sanitize_ollama_line` check; when the text is dropped the section number is preserved so the response keeps its structure. `count_ollama_removed_chars` counts headings too — it previously scanned bodies only, understating what was withheld. ([src/llm.rs](src/llm.rs))
- **A truncated answer no longer loses its removal measurement.** `removed_chars` was computed only when the old `status` was `guarded`, so a run that hit the generation limit reported "N issue(s) / removed=0" — a contradiction. It is now measured independently of `status`. ([src/llm.rs](src/llm.rs))

### Changed
- **`--ollama-bench` `status` reports completion only: `ok` / `length` / `error`.** The `guarded` value is removed. Completion and integrity are orthogonal axes, and folding them into one field made an answer that states nothing score best — the detection count rises with how much specific content an answer contains and falls to zero for an answer that says nothing. Integrity is reported separately as the issue count and the removed character count. ([src/llm.rs](src/llm.rs))
- **The benchmark table shows the detected share of the response** (`removed=N / M issue(s) (detected X%)`), always, replacing the conditional `yield` figure. `response_chars = 0` prints `-` rather than `0%`. The share is an operational fact — how much of what the model wrote never reached the screen — not a quality score. The struct, CSV and JSON fields are unchanged. ([src/llm.rs](src/llm.rs))

### Documentation
- **[security-design.md](docs/dev-prog/security-design.md) §1 now states the detection algorithm**: the five checks with their firing conditions and whether each compares against the input, the two axes they police (fabricated numbers vs fabricated grounds), what is deliberately not detected (a forward-looking statement is not an integrity violation; the derived-term list is a fixed enumeration, not exhaustive coverage), and why a detection is not a demerit. Both language sections updated.
- Two dead cross-references to `usage-guide.md`, which carries no integrity content, now point at [command-reference.md](docs/manual/command-reference.md); the detection rules themselves are normative and live in security-design.md.
- [command-reference.md](docs/manual/command-reference.md) describes the new `--llm-benchmark` output and the meaning of each `status` value. Both language sections updated.
- **Versions aligned to 2.9.6** across the engine, desktop app, settings app, and web UI; MSI name `XOKSA-2.9.6-x64.msi`.

**節見出しに混じった捏造数値が、そのまま画面に出ていました。** 整合性ガードは本文行を検査する一方、見出しは無検査で出力していたため、入力に無い数値を含む見出しは指摘として計上されながら表示されていました——[security-design.md](docs/dev-prog/security-design.md) §1 の保証が見出しに適用されていない状態です。これを修正し、あわせてベンチマークの査定尺度を是正しました。

### Fixed（日本語）
- **節見出しを本文行と同じ検査にかけた。** `render_guarded_ollama_section` は見出しを `sanitize_ollama_line` に通し、文言が落ちる場合も節番号を残して応答の構造を保つ。`count_ollama_removed_chars` も見出しを集計対象に加えた——従来は本文のみを走査しており、除去量を過少に報告していた。([src/llm.rs](src/llm.rs))
- **打ち切られた回答でも除去量を測るようにした。** `removed_chars` は旧 `status` が `guarded` のときだけ計算していたため、生成上限に達した実行は「N件指摘・removed=0」という矛盾した値を報告していた。`status` と切り離して常時計測する。([src/llm.rs](src/llm.rs))

### Changed（日本語）
- **`--ollama-bench` の `status` を完了性のみに変更：`ok` / `length` / `error`。** `guarded` を廃止。完了性と整合性は直交する軸であり、1つのフィールドに畳むと「何も述べない回答」が最良になっていた——検知件数は回答が具体的な内容を含むほど増え、何も述べなければゼロになるためである。整合性は指摘件数と除去文字数として別に報告する。([src/llm.rs](src/llm.rs))
- **ベンチマーク表に検知率を常時表示**（`removed=N / M issue(s) (detected X%)`）。条件付き表示だった `yield` を置き換える。`response_chars = 0` では `0%` ではなく `-` を表示する。この割合は「モデルが書いたうち画面に届かなかった量」という運用上の事実であり、品質の点数ではない。構造体・CSV・JSON の項目は不変。([src/llm.rs](src/llm.rs))

### Documentation（日本語）
- **[security-design.md](docs/dev-prog/security-design.md) §1 に判定アルゴリズムを明記。** 5つの検知器の発動条件と入力照合の有無、取り締まる2つの軸（数値の捏造／根拠の捏造）、意図的に検出しないもの（将来に向けた記述は整合性違反ではない。派生語リストは固定の列挙であり網羅ではない）、および検知が減点ではない理由。英日両セクションを更新。
- 整合性の記述を持たない `usage-guide.md` への切れた相互参照2箇所を [command-reference.md](docs/manual/command-reference.md) に付け替えた。検出規則そのものは規範であり security-design.md に置く。
- [command-reference.md](docs/manual/command-reference.md) に新しい `--llm-benchmark` の出力項目と各 `status` 値の意味を記載。英日両セクションを更新。
- **エンジン・デスクトップアプリ・設定アプリ・Web UI のバージョンを 2.9.6 に統一。** MSI 名は `XOKSA-2.9.6-x64.msi`。

---

## [2.9.5] — unreleased

**Default LLM answers now weave technicals, fundamentals, and news together — the model no longer hides behind its own constraints.** The prompt frame gains an affirmative duty (use the headlines, state the pivot conditions) alongside the existing prohibitions. Every number is still computed by the program (same SOT); the integrity guard is unchanged.

### Changed
- **News headlines are woven in proactively, on every path.** The SOT system prompt (all providers, chat and standard analysis) and the chat news-index boundary previously said only what was forbidden (no body inference, no price-impact claims); a defensively-tuned model read that as "avoid news". Both now add the duty: weave the topics and sentiment the headlines show into the answer as the material environment — even for technical-centric questions — stating that bodies are unverified and citing URLs. ([src/llm.rs](src/llm.rs), [src/chat/prompt.rs](src/chat/prompt.rs))
- **Always-on answer composition: balance the three elements.** Independent of any knob, when the input holds technicals, fundamentals, and news, the answer weaves all three in reasonable balance; fundamentals are non-score context checked against the technicals; absent elements are named, never fabricated. ([src/chat/prompt.rs](src/chat/prompt.rs))
- **The `/depth` scale is wider and the default is slightly looser.** `mid` (default) now also states the pivot conditions at which the view changes and may state a direction the data clearly supports; `deep` additionally names the main driver and lays out the opposite-scenario conditions. `shallow` is unchanged.
- **`/scope wide` is now proactive** — it brings in relevant industry/macro/seasonal context on its own initiative instead of only engaging factors the user raises.
- **Versions aligned to 2.9.5** across the engine, desktop app, settings app, and web UI; MSI name `XOKSA-2.9.5-x64.msi`.

### Verification
- **The regression case that motivated this** — 「この期間動きが激しいんだけど」on 9432.T, default settings, first answer: previously number-recitation with zero news (three rounds of prodding required); now the first answer integrates positive **and** cautionary headlines (URLs cited, bodies-unverified stated), folds in fundamentals as non-score context, gives pivot levels in both directions, and explains the mechanism (profit-taking near the band top).
- **Standard analysis** (`xoksa -t 9432.T`): the summary now covers the material environment and both scenarios with chained pivot levels; direction stated.
- The integrity guard operated unchanged in both runs. `cargo test` **299 passed** (the prompt-budget ceiling updated 2400→2950 for the larger fixed instruction block); `clippy -D warnings` / `fmt --check` clean.

**既定の LLM 回答が、テクニカル・ファンダメンタル・ニュースを最初から織り込むようになりました——モデルが制約の陰に隠れなくなります。** プロンプト枠に、従来の禁止事項と並ぶ肯定的義務（見出しを使う・分岐条件を示す）を追加。数値はすべて従来どおりプログラムが計算し（SOT 同一）、整合性ガードも不変です。

### Changed（日本語）
- **ニュース見出しを全経路で自発的に織り込む。** SOT システムプロンプト（全プロバイダ・チャット/通常分析共通）とチャットのニュース境界文は、従来「禁止」だけを書いており（本文推測禁止・価格影響断定禁止）、防御的なモデルは「ニュースは避けろ」と読んでいた。両方に義務を追加：質問がテクニカル中心でも、見出しが示す話題とセンチメントを材料環境として織り込み、本文未確認を明示し URL を併記する。([src/llm.rs](src/llm.rs)、[src/chat/prompt.rs](src/chat/prompt.rs))
- **常時適用の「回答の基本構成」＝3要素バランス。** ノブと無関係に、入力にテクニカル・ファンダメンタル・ニュースがあれば3要素をバランスよく織り込む。ファンダはスコア外の客観情報としてテクニカルと突き合わせ、無い要素は創作せず無い旨のみ述べる。([src/chat/prompt.rs](src/chat/prompt.rs))
- **`/depth` の尺度を拡幅し、既定を若干緩めた。** `mid`（既定）に「見立てが変わる分岐条件の提示」と「データが明確に支持する場合の方向性言及（許可）」を追加。`deep` はさらに主因の特定と反対シナリオの成立条件まで。`shallow` は不変。
- **`/scope wide` を能動形に**——ユーザーが挙げていなくても、関連する業界・マクロ・季節性の文脈を自発的に補う。
- **エンジン・デスクトップアプリ・設定アプリ・Web UI のバージョンを 2.9.5 に統一。** MSI 名は `XOKSA-2.9.5-x64.msi`。

### Verification（日本語）
- **今回の動機となった再現ケース**——9432.T に「この期間動きが激しいんだけど」（既定設定・初回回答）：従来は数値の読み上げのみでニュースはゼロ（3往復の催促が必要だった）。修正後は初回から、前向き**と**警戒の両見出しを URL 併記・本文未確認明示つきで統合し、ファンダをスコア外の文脈として織り込み、両方向の分岐水準と機構（バンド上限近辺の利確圧力）まで説明。
- **通常分析**（`xoksa -t 9432.T`）：総評が材料面と両シナリオ（分岐水準の連鎖）をカバーし、方向を明言。
- 整合性ガードは両テストで従来どおり作動。`cargo test` **299 passed**（プロンプト予算テストの上限を固定指示ブロックの増加に合わせ 2400→2950 に更新）。`clippy -D warnings`・`fmt --check` clean。

---

## [2.9.4] — 2026-08-21

**An audit of the shipped binary against the canon: an undocumented in-app manual removed, a `--private` leak closed, and the settings app's engine controls made honest.** Analysis and scoring are unchanged (same SOT).

### Removed
- **Manuals are no longer compiled into the binary.** `setup.md` and `analysis-guide.md` were embedded with `include_str!` and served at `/manual/:slug`, together with `/manual.css`, `/images/:file` and eight embedded screenshots — but **nothing in any UI linked to them**, and no CLI flag exposed them. An undocumented HTTP surface that only its author knew how to reach is not a feature. The routes, the embedded assets, the `pulldown-cmark` dependency, and the desktop's never-called `open_manual` command are gone; the engine binary loses ~1.4 MB. Documentation lives on GitHub, where editing it no longer changes a shipped binary's hash. ([src/server/mod.rs](src/server/mod.rs), [xoksa-desktop/src/main.rs](xoksa-desktop/src/main.rs))
- **`POST /api/analysis/context-pack` removed** — no UI ever called it; the dashboard uses `/api/analysis/multi-timeframe`, which builds the same pack internally. ([src/server/api.rs](src/server/api.rs))
- **The settings app's "Start engine" button is gone**, folded into one "Restart engine" action (below).

### Fixed
- **`--private` wrote the chat history to disk.** Every line typed in chat mode was appended to `~/.xoksa_history` even in a no-trace session — the user's own words, which is exactly the trace the flag rules out. A private session now neither reads nor writes the file; arrow-key recall still works in memory. Guarded by a regression test in both directions. ([src/chat/mod.rs](src/chat/mod.rs), [src/chat/run.rs](src/chat/run.rs))
- **The language setting had four independent sources.** The dashboard kept its own copy in the browser's `localStorage`, the connection screen followed the OS locale, the settings app followed the OS locale until unlocked and `LANG` after, and the CLI followed `LANG` — so choosing Japanese in the settings app could leave the connection screen in English and the CLI in another language again. `LANG` in `xoksa.env` is now the single source: the engine stamps it into the page it serves, the dashboard's selector writes it back through `POST /api/config/lang`, and the connection screen reads it from the engine. The OS locale survives only as the first-run fallback, before any config exists. Writing is loopback-only — a LAN client is served the configured language but cannot rewrite settings on someone else's machine (403) — and a `--private` session refuses (409) instead of reporting a save it did not make. ([src/server/api.rs](src/server/api.rs), [src/server/mod.rs](src/server/mod.rs), [webui-leptos/src/main.rs](webui-leptos/src/main.rs), [xoksa-desktop/src/main.rs](xoksa-desktop/src/main.rs))
- **English was written as a commented-out key** (`#LANG=en`), which made "English" and "never chosen" indistinguishable in the config file. Both languages are now written explicitly. ([src/setup.rs](src/setup.rs))
- **Reading the news re-ran the whole analysis.** The news endpoint built a full analyzed guard — a second market fetch and a second full indicator pass — to learn the company name, the only thing it used it for (that and the ticker). It now takes a `NewsSubject` carrying exactly those two values; the CLI and chat pass the ones they already computed, and the server resolves the name from the alias file or the last analysis of that symbol. Same news, half the provider traffic. ([src/news.rs](src/news.rs), [src/server/api.rs](src/server/api.rs))
- **`#![forbid(unsafe_code)]` was missing from `xoksa-paths`**, the one crate the engine, the desktop app, and the settings app all link. It contained no `unsafe`; now the compiler enforces that. ([xoksa-paths/src/lib.rs](xoksa-paths/src/lib.rs))
- **The settings app reported a restart it had not performed.** It only tracked the child it spawned in the current session, so after reopening the app "Restart" killed nothing, spawned an engine that died on the occupied port, and still reported success — leaving the old token in force. Restart now pre-checks the port, names the occupant when it is a process this app did not start (and never kills it), and waits for the new engine to listen before reporting success; an immediate exit is reported with its status. ([xoksa-setup/src/main.rs](xoksa-setup/src/main.rs))

### Added
- **A Quit button, and a warning when you would lose edits.** The settings app could only be closed from the title bar, and it closed silently — typed-but-unsaved changes went with it. There is now a **Quit** button beside Save; quitting (or closing the window) with unsaved edits asks first, and quitting with nothing pending leaves immediately. The confirmation is an in-page dialog, not `window.confirm`, which the Tauri WebView suppresses (the same trap as the 2.7.1 chat-clear fix). Quitting leaves the engine running on purpose — the settings app is transient, and stopping the engine would drop the connection just configured. ([xoksa-setup/frontend/index.html](xoksa-setup/frontend/index.html), [xoksa-setup/src/main.rs](xoksa-setup/src/main.rs))
- **The manuals ship as a zip beside the app.** With the in-binary manual gone, `scripts/make-manual-zip.sh` packages every manual plus the images they reference into `xoksa-manuals-<version>.zip`, published on the release page — offline-readable, and editing a document no longer touches a binary. Two dead links inside the settings app were removed with it: the Advanced-settings "Indicator Guide" link had no click handler at all (it did nothing in 2.9.3 either), and a "Setup Guide" string pair was left in both dictionaries with no element referencing it. The advanced-settings note now points at `analysis-guide.md` in the zip. ([scripts/make-manual-zip.sh](scripts/make-manual-zip.sh), [xoksa-setup/frontend/index.html](xoksa-setup/frontend/index.html))
- **The dashboard port is a setting.** `SERVE_PORT` was read from `xoksa.env` but had no field in the settings app — a hidden setting only reachable by hand-editing. It is now on the Connection card (default 8787), written through the same `apply-config` path as everything else. ([xoksa-setup/frontend/index.html](xoksa-setup/frontend/index.html), [src/setup.rs](src/setup.rs))

### Changed
- **Settings screen regrouped.** "Network" and "Access" were separated by "Chat notifications" although they describe the same thing, and the settings password sat inside "Access" as if it were the access token. LAN access, port, access token, and the restart button are now one **Connection** card; the settings password is its own card at the end. ([xoksa-setup/frontend/index.html](xoksa-setup/frontend/index.html))
- **Versions aligned to 2.9.4** across the engine, desktop app, settings app, and web UI.
- **The canon records the settings app's engine restart** as a design contract, including that the engine it starts outlives the app (the settings app is transient; killing the engine would drop the connection just configured). ([docs/dev-prog/security-design.md](docs/dev-prog/security-design.md))

### Verification
Windows measurements (macOS measurements are on the release page and in §C.4):
- `cargo test` **299 passed** (284 lib / 10 integration / 5 no-drift invariants); the engine is **1,435,648 bytes smaller** (13,819,904 → 12,384,256).
- **Settings app, measured live**: restart listens on the configured port (`SERVE_PORT` persisted); reopening the app and pressing restart against an engine it did not start is **refused immediately, naming the port** — the occupant is left running, no false success reported.
- **Language single-source, end to end**: a settings-app save writes explicit `LANG=en` (the legacy `#LANG` line disappears in the full rewrite); the dashboard gear writes `LANG=ja` back via `POST /api/config/lang`; `--private` answers **409** and writes nothing; the removed routes (`/manual/:slug`, `/manual.css`, `/images/:file`, `POST /api/analysis/context-pack`) answer **404**.
- **SCA on Windows**: `cargo audit` 0 over 306 deps; `cargo audit bin` 0 over the artifact's 234 embedded deps; `cargo deny` advisories / bans / licenses / sources ok; `cargo machete` clean; `osv-scanner` over all four lockfiles **0 Critical / 0 High / 2 Medium** — both the Linux-only `glib` binding in the Tauri gtk tree, which is not compiled into the shipped Windows or macOS artifacts — plus 35 unmaintained notices in that same Linux-only tree.
- **OWASP ZAP** against the LAN-exposed engine from a separate host: the token sign-in page is served with the full header set (nonce CSP, `X-Frame-Options: DENY`, nosniff, `SameSite=Strict`); 8 alert categories with a single Medium — no anti-CSRF token on the `/auth` sign-in form — **accepted with rationale**: forging that POST requires possessing the access token itself, and the session cookie is `SameSite=Strict`. The rest are Info/Low, consistent with the recorded baseline.
- **MSI rebuilt and payload-verified**: `XOKSA-2.9.4-x64.msi` `0e9144c3…` carrying engine `7a1b9816…`, desktop `2f06ed7f…`, settings app `03f1419b…`; published on the release page with the expected hashes.

**出荷バイナリを正典と突き合わせた監査：導線のないアプリ内マニュアルを削除し、`--private` の漏れを塞ぎ、設定アプリのエンジン操作を実態に合わせました。** 分析・スコアリングは不変（SOT 同一）です。

### Removed（日本語）
- **マニュアルをバイナリに埋め込むのをやめました。** `setup.md` と `analysis-guide.md` を `include_str!` で埋め込み `/manual/:slug` で配信し、`/manual.css`・`/images/:file`・スクリーンショット8枚も同梱していましたが、**どの UI からもリンクされておらず**、CLI にも出口がありませんでした。作者だけが到達方法を知る非公開の HTTP 面は機能とは呼べません。ルート・埋め込み資産・`pulldown-cmark` 依存・デスクトップの未呼び出しコマンド `open_manual` を削除し、エンジンは約 1.4 MB 縮みました。文書は GitHub にあり、修正しても出荷バイナリのハッシュは変わりません。
- **`POST /api/analysis/context-pack` を削除** — UI から呼ばれていませんでした。ダッシュボードが使うのは `/api/analysis/multi-timeframe` で、同じ pack を内部で構築します。
- **設定アプリの「エンジン起動」ボタンを廃止**し、「エンジンを再起動」1つに統合しました（下記）。

### Fixed（日本語）
- **`--private` がチャット履歴をディスクに書いていました。** 無痕跡セッションでも、チャットで打った行が `~/.xoksa_history` に追記されていました——ユーザー自身の言葉であり、このフラグが排除する痕跡そのものです。プライベートセッションでは読み書きとも行いません（↑↓キーの呼び戻しはメモリ上で従来どおり）。両方向を守る回帰テストを追加しました。
- **言語設定の情報源が4つに分裂していました。** ダッシュボードはブラウザの `localStorage` に自前のコピーを持ち、接続画面は OS ロケール、設定アプリは解錠前が OS ロケールで解錠後が `LANG`、CLI は `LANG` ——設定アプリで日本語を選んでも接続画面が英語のまま、ということが起きていました。`xoksa.env` の `LANG` を唯一の情報源とし、エンジンが配信するページに刻み、ダッシュボードのセレクタは `POST /api/config/lang` で書き戻し、接続画面はエンジンから受け取ります。OS ロケールは設定が存在しない初回のみのフォールバックです。書き込みはループバック限定——LAN のクライアントには設定言語で配信しますが、他人の端末の設定を書き換えることはできません（403）——`--private` セッションは保存したふりをせず 409 を返します。
- **英語がコメントアウト（`#LANG=en`）で表現されていました。** 「英語」と「未選択」が設定ファイル上で区別できませんでした。現在はどちらの言語も明示的に書きます。
- **ニュースを読むだけで分析が丸ごと再実行されていました。** ニュースのエンドポイントは会社名（と銘柄コード）を得るためだけにフル解析——2 回目の市場データ取得と 2 回目の全指標計算——を実行していました。現在はその 2 値だけを運ぶ `NewsSubject` を受け取り、CLI とチャットは計算済みの値をそのまま渡し、サーバは別名ファイルまたは直近の分析結果から名前を解決します。ニュースの内容は同じで、プロバイダへの通信は半分になります。
- **共有クレート `xoksa-paths` に `#![forbid(unsafe_code)]` がありませんでした。** エンジン・デスクトップ・設定アプリのすべてがリンクする唯一のクレートです。`unsafe` は元から 0 件でしたが、コンパイラに強制させます。
- **設定アプリが、実行していない再起動を「成功」と報告していました。** 子プロセスは現在のセッションで自分が起動した分しか把握していないため、アプリを開き直したあとの「再起動」は何も停止せず、ポートが埋まったまま起動したエンジンは即死し、それでも成功と表示されていました（＝古いトークンのまま）。現在は、事前にポートを確認し、自分が起動していないプロセスが占有していればポート番号を示して拒否し（kill はしません）、新しいエンジンが待ち受けを開始してから成功を返します。起動直後の終了は終了ステータス付きで報告します。

### Added（日本語）
- **終了ボタンと、未保存の変更に対する警告。** 設定アプリはタイトルバーからしか閉じられず、しかも黙って閉じるため、入力したが保存していない変更が失われました。保存ボタンの隣に **終了** を追加し、未保存の変更があるまま終了（またはウィンドウを閉じる操作）をすると確認を出します。保留が無ければそのまま終了します。確認は `window.confirm` ではなく画面内ダイアログです（Tauri の WebView では `confirm` が抑止されるため——2.7.1 のチャット消去と同じ罠）。終了してもエンジンは動いたままにします——設定アプリは一時的なアプリであり、エンジンを止めると設定したばかりの接続が切れるためです。
- **マニュアルはアプリと並ぶ ZIP で配布します。** バイナリ内マニュアルを廃止したのに合わせ、`scripts/make-manual-zip.sh` が全マニュアルと参照画像を `xoksa-manuals-<版数>.zip` にまとめ、リリースページで配布します——オフラインで読め、文書を直してもバイナリに触れません。あわせて設定アプリ内の死んだ導線を 2 つ削除しました：上級設定の「指標ガイド」リンクは**クリックハンドラが存在せず**（2.9.3 でも押しても何も起きませんでした）、「セットアップの手引き」は辞書の文字列だけが残り参照する要素がありませんでした。上級設定の説明文は ZIP 内の `analysis-guide.md` を指すようにしました。
- **ダッシュボードのポートを設定項目にしました。** `SERVE_PORT` は `xoksa.env` から読まれていたのに設定画面に欄がなく、手でファイルを編集した人だけが使える隠し設定でした。「接続」カードに追加し（既定 8787）、他の設定と同じ `apply-config` 経路で保存します。

### Changed（日本語）
- **設定画面を組み替えました。** 同じ事柄を扱う「ネットワーク」と「アクセス」の間に「チャット通知」が挟まり、設定パスワードがアクセストークンと同じカードに同居していました。LAN アクセス・ポート・アクセストークン・再起動ボタンを **「接続」** カードにまとめ、設定パスワードは独立したカードとして末尾に置きました。
- **エンジン・デスクトップアプリ・設定アプリ・Web UI のバージョンを 2.9.4 に統一。**
- **設定アプリによるエンジン再起動を正典に明記。** 起動したエンジンがアプリより長生きすること（設定アプリは一時的なアプリで、エンジンを道連れにすると設定したばかりの接続が切れる）も含めて設計契約としました。

### Verification（日本語）
Windows の実測（macOS の実測はリリースページと §C.4）：
- `cargo test` **299 passed**（lib 284／integration 10／no-drift 不変条件 5）。エンジンは **1,435,648 バイト縮小**（13,819,904 → 12,384,256）。
- **設定アプリの実機確認**：再起動は設定したポートで待ち受け（`SERVE_PORT` 永続化）。アプリを開き直し、自分が起動していないエンジンが居る状態で再起動 → **即座にポート番号を示して拒否**。占有エンジンは無傷のまま、嘘の成功報告なし。
- **言語一本化の端から端まで**：設定アプリの保存は明示形式の `LANG=en` を書く（旧 `#LANG` 行は全書き換えで消える）。ダッシュボードの歯車は `POST /api/config/lang` で `LANG=ja` を書き戻す。`--private` は **409** を返し何も書かない。削除したルート（`/manual/:slug`・`/manual.css`・`/images/:file`・`POST /api/analysis/context-pack`）は **404**。
- **Windows の SCA**：`cargo audit` 306 依存で 0 件。`cargo audit bin` は現物の埋め込み 234 依存で 0 件。`cargo deny` は advisories／bans／licenses／sources すべて ok。`cargo machete` 未使用なし。`osv-scanner` は 4 lockfile 全体で **0 Critical／0 High／2 Medium** —— 2件とも Tauri gtk ツリーの Linux 専用 `glib` バインディングで、出荷する Windows・macOS の現物にはコンパイルされない——ほか同じ Linux 専用ツリーの unmaintained 通知 35 件。
- **OWASP ZAP**（別ホストから LAN 公開中のエンジンへ）：トークンサインインページが全ヘッダ（nonce CSP・`X-Frame-Options: DENY`・nosniff・`SameSite=Strict`）つきで配信されることを確認。アラートは 8 カテゴリ・Medium は 1 件のみ——`/auth` サインインフォームの anti-CSRF トークン不在——**根拠つきで受容**：この POST の偽造にはアクセストークンそのものの所持が必要で、セッション cookie は `SameSite=Strict`。残りは Info/Low で記録済みベースラインと同質。
- **MSI 再ビルド・payload 検証済み**：`XOKSA-2.9.4-x64.msi` `0e9144c3…`（engine `7a1b9816…`／desktop `2f06ed7f…`／設定アプリ `03f1419b…` を格納）。期待値ハッシュとともにリリースページへ登載。

---

## [2.9.3] — 2026-08-21

**Desktop local auto-start is collision-proof again, and connection failures name their cause.** The auto-started loopback engine returns to an OS-assigned ephemeral port (the v2.6.4 hardening, lost in the v2.8.1 fixed-port change), and every auto-start failure now says what actually went wrong. Analysis and scoring are unchanged (same SOT).

### Fixed
- **Local auto-start could fail opaquely — or silently adopt a leftover engine — when the fixed port 8787 was taken.** The auto-started loopback engine once again runs on an ephemeral port chosen by the OS: it can no longer contend with a stale `xoksa serve` or any foreign process, and multiple app instances coexist again. The engine identity check is strengthened from service-name to an exact version match (the desktop and engine ship as a matched pair). When LAN access is on, the engine keeps the fixed, user-known port that other devices rely on; that port is now pre-checked before spawning. ([xoksa-desktop/src/main.rs](xoksa-desktop/src/main.rs))
- **Connection failures now state their cause, in the UI language.** Instead of a uniform timeout after 10 seconds, auto-start reports the specific failure: engine spawn failure; engine exited right after startup (with its exit status); port in use — identifying the occupant (a token-requiring xoksa engine / another xoksa version / a foreign process); authentication required (HTTP 401, now distinguished from "nothing listening"); no free local port; or timeout with the last observed state. ([xoksa-desktop/src/main.rs](xoksa-desktop/src/main.rs), [xoksa-desktop/frontend/index.html](xoksa-desktop/frontend/index.html))
- **The settings app's LAN warning described the previous generation.** It said LAN mode had "NO login"; since v2.8.0 a non-loopback bind is fail-closed — an auto-generated access token is always required, unauthenticated access is refused, and only private-network sources are accepted. The warning now describes that behaviour. ([xoksa-setup/frontend/index.html](xoksa-setup/frontend/index.html))
- **Connection screen hint clarified.** "An auto-started local engine needs no token" — a token-gated engine already listening on the entered address is no longer implied to work without one. ([xoksa-desktop/frontend/index.html](xoksa-desktop/frontend/index.html))

### Changed
- **Versions aligned to 2.9.3** across the engine, desktop app, and settings app.
- **Windows MSI output name** updated to `XOKSA-2.9.3-x64.msi`.

### Verification
- **Stale engine on 8787, loopback auto-start** — a running `xoksa serve --port 8787` in place: Connect succeeded; the child engine came up on ephemeral port 59416 and the squatter was untouched (previously: an opaque failure after 10 s).
- **Two app instances concurrently** — three engines on three ports (8787 squatter / 59416 / 58807), both dashboards independent.
- **LAN mode with 8787 occupied** — Connect showed, immediately and in the UI language: 「ポート 8787 は別の xoksa エンジン（v2.9.3）が使用中です。」
- **Kill-on-exit** — closing each window terminated its child engine; both ephemeral ports released.
- `cargo +stable clippy -- -D warnings` and `cargo fmt --check` clean (xoksa-desktop); MSI rebuilt with payload hashes verified (`XOKSA-2.9.3-x64.msi`).

**デスクトップのローカル自動起動が再び衝突知らずになり、接続失敗が原因を名乗るようになりました。** 自動起動のループバックエンジンは OS が割り当てるエフェメラルポートに回帰し（v2.8.1 の固定ポート化で失われていた v2.6.4 の堅牢化）、自動起動の失敗はすべて実際の原因を表示します。分析・スコアリングは不変（SOT 同一）です。

### Fixed（日本語）
- **固定ポート 8787 が使用中のとき、ローカル自動起動が原因不明で失敗する——または居残りエンジンを黙って掴む——問題を修正。** 自動起動のループバックエンジンは OS が選ぶエフェメラルポートで起動するよう回帰。居残りの `xoksa serve` や他プロセスと競合せず、アプリの多重起動も再び共存する。エンジンの同一性確認をサービス名からバージョン完全一致に強化（デスクトップとエンジンは対で出荷される）。LAN アクセス有効時は、他端末が番号を知る必要があるため従来どおり固定ポートを使用し、spawn 前に占有を事前チェックする。([xoksa-desktop/src/main.rs](xoksa-desktop/src/main.rs))
- **接続失敗が UI の言語で原因を表示。** 10秒後の一律タイムアウトに代わり、自動起動は具体的な失敗を報告する：エンジン起動失敗／起動直後の終了（終了ステータス付き）／ポート使用中——占有者を識別（認証が必要な xoksa エンジン／別バージョンの xoksa／別プロセス）／認証要求（HTTP 401 を「応答なし」と区別）／空きポート確保失敗／最終観測状態付きのタイムアウト。([xoksa-desktop/src/main.rs](xoksa-desktop/src/main.rs)、[xoksa-desktop/frontend/index.html](xoksa-desktop/frontend/index.html))
- **設定アプリの LAN 警告が1世代前の記述だった問題を修正。** 「ログインはありません」という記述は v2.8.0 以降の実装と不一致——非ループバックの bind は fail-closed（自動生成のアクセストークンが常に必要・未認証の接続は拒否・接続元はプライベートネットワークのみ）。警告文を現在の動作に合わせた。([xoksa-setup/frontend/index.html](xoksa-setup/frontend/index.html))
- **接続画面の案内を明確化。** 「自動起動のローカルエンジンにトークンは不要」とし、同じアドレスで待ち受け中のトークン必須エンジンにトークンなしで繋がるかのような含みを解消。([xoksa-desktop/frontend/index.html](xoksa-desktop/frontend/index.html))

### Changed（日本語）
- **エンジン・デスクトップアプリ・設定アプリのバージョンを 2.9.3 に統一。**
- **Windows MSI の生成ファイル名**を `XOKSA-2.9.3-x64.msi` に更新。

### Verification（日本語）
- **8787 を居残りエンジンが占有した状態でのループバック自動起動** — `xoksa serve --port 8787` を稼働させたまま接続：成功。子エンジンはエフェメラルポート 59416 で起動し、居残りは無傷（従来は10秒後に原因不明の失敗）。
- **アプリ2重起動** — 3エンジンが3ポートで共存（8787 居残り／59416／58807）、両ダッシュボードは独立動作。
- **LAN モード＋8787 占有** — 接続時に即時・UI 言語で「ポート 8787 は別の xoksa エンジン（v2.9.3）が使用中です。」を表示。
- **終了時の道連れ停止** — 各ウィンドウを閉じると子エンジンが終了し、エフェメラルポートは解放。
- `cargo +stable clippy -- -D warnings`・`cargo fmt --check` clean（xoksa-desktop）。MSI 再ビルド済み・payload ハッシュ一致検証 PASS（`XOKSA-2.9.3-x64.msi`）。

---

## [2.9.2] — 2026-08-21

**Japanese stocks no longer display ~15 minutes behind.** Real-time price and time are pulled from Yahoo! Finance Japan and folded into the current intraday bar, and a live volume-0 tick is no longer mistaken for a post-close empty bar. How every number is computed is unchanged (same SOT); the fix corrects which price and time are treated as the latest.

### Fixed
- **Japanese stock prices displayed ~15 minutes late.** The delayed TSE price returned by the `query1`/`query2.finance.yahoo.com` Chart API was being used as-is as the latest price. For Japanese stocks, the latest price and time are now supplemented from the public Yahoo! Finance Japan page; if that fetch fails, the existing Chart API price is used so the whole analysis never fails. ([src/market.rs](src/market.rs))
- **Real-time price reflected into the current intraday bar.** On 1m/5m/15m/30m/60m timeframes the real-time price is applied to the current time bucket: an existing current bar has its close updated and its high/low range extended as needed; if the current bar does not yet exist, a forming bar is appended from the latest price. Volume that cannot be fetched is not fabricated — the most recent measured volume is shown and used in calculations.
- **A live volume-0 tick was erroneously deleted.** The "latest-price tick with volume 0" that Yahoo appends during trading hours was mistaken for a post-close empty bar and removed. When it belongs to the same time bucket it is now merged into the existing bar (reflecting the latest price / high / low); a genuine empty bar that Yahoo adds as a separate bucket after the close is still removed as before.
- **Japanese daily bars also get real-time price and data time.** The daily OHLCV history still comes from the existing Chart API, while the "latest fetched price" and "latest data time" now use the Yahoo! Finance Japan values.

### Changed
- **Versions aligned to 2.9.2** across the engine, desktop app, and settings app.
- **Windows MSI output name** updated to `XOKSA-2.9.2-x64.msi`.

### Verification
- **9432.T / daily** — before: analysis 10:33, data 10:18; after: analysis 10:41, data 10:41.
- **7203.T / 15m** — before: data ~15 min behind the analysis time; after: latest price, data time, and the indicator-calculation bar advance to the current 15-minute bucket.
- `cargo test --lib`: 283 passed / 0 failed / 1 ignored.
- Added market-data regression tests: parsing the Yahoo Japan real-time price/time; appending and updating the current intraday bar; merging a volume-0 tick into the same bucket; and preserving post-close empty-bar removal.

**日本株の株価が約15分遅れて表示される問題を修正。** リアルタイムの価格と時刻を Yahoo!ファイナンス日本版から補い、現在の分足へ反映。取引時間中の出来高0ティックを引け後の空バーと誤判定しなくなりました。数値の算出方法は不変（SOT 同一）で、修正したのは「どの価格・時刻を最新として扱うか」です。

### Fixed（日本語）
- **日本株の株価が約15分遅れて表示される問題を修正。** 従来は `query1`/`query2.finance.yahoo.com` の Chart API が返す東証向け遅延価格を、そのまま最新価格として使用していた。日本株については、Yahoo!ファイナンス日本版の公開ページからリアルタイム価格と時刻を補完するよう変更。取得に失敗した場合は従来の Chart API 価格を使用し、分析全体を失敗させない。([src/market.rs](src/market.rs))
- **リアルタイム価格を現在の分足へ反映。** 1分・5分・15分・30分・60分足では、リアルタイム価格を現在の時間バケットへ適用。現在足が存在する場合は終値を更新し、必要に応じて高値・安値の範囲を拡張。現在足がまだ存在しない場合は、最新価格を基に形成中の足を追加。取得できない出来高は創作せず、直近の実測出来高を表示・計算に使用。
- **出来高0の最新ティックが誤って削除される問題を修正。** Yahoo が取引時間中に追加する「出来高0の最新価格ティック」を、引け後の空バーと誤判定して削除していた。同一時間バケットに属する場合は既存バーへ統合し、最新価格・高値・安値を反映。引け後に別バケットとして追加される本当の空バーは、従来どおり削除。
- **日本株の日足でも最新価格とデータ時刻をリアルタイム化。** 日足の OHLCV 履歴は従来の Chart API を維持しつつ、「最新取得価格」と「データの最新時刻」は Yahoo!ファイナンス日本版の値を使用。

### Changed（日本語）
- **エンジン・デスクトップアプリ・設定アプリのバージョンを 2.9.2 に統一。**
- **Windows MSI の生成ファイル名**を `XOKSA-2.9.2-x64.msi` に更新。

### Verification（日本語）
- **9432.T / daily** — 修正前：分析時刻 10:33、データ時刻 10:18／修正後：分析時刻 10:41、データ時刻 10:41。
- **7203.T / 15m** — 修正前：分析時刻に対してデータが約15分遅延／修正後：最新価格・データ時刻・指標計算足が現在の15分バケットまで進行。
- `cargo test --lib`: 283 passed / 0 failed / 1 ignored。
- 市場データの回帰テストを追加：Yahoo Japan リアルタイム価格・時刻の解析／現在の分足バケットの追加・更新／出来高0ティックの同一バケット統合／引け後の空バー削除の維持。

---

## [2.9.1] — unreleased

**Observation-based dashboard refresh, a single-row header with a settings popover, a news relevance filter on by default, and US fundamentals that follow the latest quarterly disclosure.** The buy/sell scoring is unchanged (same SOT); the fundamentals panel now reflects the most recent 10-Q/10-K rather than the latest full fiscal year.

### Added
- **Observation-based refresh (data-change detection).** The dashboard tracks Yahoo's own data time and moves at that ~1-minute granularity instead of a fixed clock: the browser polls at 60 s, and the server caches each `(lang|timeframe|symbol)` build for 60 s (TTL), answering requests inside the window from cache (flagged `from_cache`) instead of re-fetching + re-analyzing. The basic-data panel shows the source's data time; the internal cadence is not surfaced. ([src/server/api.rs](src/server/api.rs), [design-philosophy §8.4](docs/dev-prog/design-philosophy.md))
- **News relevance filter, on by default, with an opt-out.** The dashboard news panel narrows to investor-relevant results by default (a finance-qualified query), with a **「フィルタを外す」** checkbox to show everything. The filter now also applies to free-text searches (previously bypassed), so a bare company-name search no longer returns same-name sports/entertainment noise. And the chat/analysis LLM now reasons over exactly the news the panel is displaying — the panel's fetch drives the model's news buffer, so a symbol change, a search, or the filter toggle updates the LLM's news immediately (SOT), never a copy frozen at session start. ([src/news.rs](src/news.rs), [src/server/api.rs](src/server/api.rs), [src/chat/](src/chat/))

### Changed
- **Header redesign — single row, controls consolidated.** The header never wraps and the loaded symbol chips take the space; the set-once controls (auto-refresh, theme, language, font size) moved into a ⚙ settings popover, leaving only symbols, timeframe, AI selection, tools, help, and ⚙ on the row. The +ticker entry is a Windows-Start-style search box (theme-aware: white on dark, accent-blue on light) with a magnifier and an instructive placeholder; the LLM picker is labelled **「AI の選択」**. ([webui-leptos/src/main.rs](webui-leptos/src/main.rs), [webui-leptos/style.css](webui-leptos/style.css))
- **Market data drawn from the single SOT snapshot.** The 市況データ panel renders the same summary snapshot as the basic-data panel; the separate bar-accumulating market log (a different fetch timing) was removed — one analysis, one source.
- **US fundamentals now follow the latest disclosed period (quarterly).** The SEC EDGAR panel previously locked to the latest full fiscal year, so a US ticker could show ~6-month-old figures mid-year. It now anchors on the most recent 10-Q/10-K and labels the period (e.g. `2026-04-26 (Q1)` / `2026-01-25 (FY)`); every income-statement item comes from that period and the ratios are on that basis (a quarterly panel's PER/ROE are on a single quarter's earnings). This matches the J-Quants (JP) path, which already tracked the latest disclosure. Shares outstanding fall back to the cover-page count (`dei:EntityCommonStockSharesOutstanding`), disclosed every quarter, so quarterly panels still yield BPS/PBR. ([src/fundamental.rs](src/fundamental.rs))
- **Versions aligned to 2.9.1** across the engine, `webui-leptos`, `xoksa-desktop`, `xoksa-setup`, `tauri.conf.json`, README, and the manuals.

### Documentation
- **design-philosophy §8.4** documents observation-based refresh; **CONTRIBUTING** branch-naming now records the actual conventions (`release/vX.Y.Z`, `docs/`); **setup.md** (EN+JA) and **usage-guide.md** updated with dashboard screenshots.

**観測ベースのダッシュボード更新、設定ポップオーバー付きの1行ヘッダー、既定ONのニュース関連性フィルタ、そして最新の四半期開示に追随する米国ファンダ。** 売買スコアに変更はありません（SOT 同一）。ファンダメンタル欄は最新の通期ではなく直近の 10-Q/10-K を反映するようになりました。

### Added（日本語）
- **観測ベース更新（データ変化検知）。** ダッシュボードは Yahoo 自身のデータ時刻を追い、固定時計ではなくその約1分粒度で動く：ブラウザは60秒ポーリング、サーバは `(lang|足種|銘柄)` ごとに直近ビルドを60秒（TTL）保持し、窓内の要求は再取得・再分析せずキャッシュから返す（`from_cache` を付与）。基本データ欄にデータ源の時刻を表示し、内部の周期は見せない。
- **ニュース関連性フィルタを既定ON＋オプトアウト。** ニュース欄は既定で投資関連（金融クエリ）に絞り、**「フィルタを外す」**で全件表示。検索ボックス（フリーテキスト）にもフィルタが効くようになり（従来はバイパス）、会社名だけの検索で同名のスポーツ・芸能が混入しない。さらにチャット/分析のLLMは表示中のニュースそのものを根拠にする——パネルの取得がモデルのニュースバッファを駆動し、銘柄変更・検索・フィルタ切替が即座にLLMの入力へ反映される（SOT。セッション開始時に固定されたコピーではない）。

### Changed（日本語）
- **ヘッダー再設計——1行・操作を集約。** 折返さず、読み込み済み銘柄チップを主役に。set-once の操作（自動更新・テーマ・言語・文字サイズ）を ⚙ ポップオーバーへ移し、第1階層は銘柄・足種・AI選択・ツール・ヘルプ・⚙ のみ。＋銘柄入力を Windows スタート風の検索ボックス（テーマ連動：ダーク白／ライト青）に、マグニファイア＋説明プレースホルダ付きで刷新。LLM 選択は **「AI の選択」**。
- **市況データを単一SOTスナップショット由来に。** 市況データ欄は基本データ欄と同じ summary スナップショットを描画し、別タイミングでバーを蓄積する市況ログを廃止——一つの分析・一つの源。
- **米国ファンダを最新の開示期（四半期）に追随。** SEC EDGAR パネルは従来「最新の通期（会計年度）」に固定され、米国銘柄では年央に約半年前の数字が出ることがあった。今は直近の 10-Q/10-K をアンカーにし、期種別ラベル（例 `2026-04-26 (Q1)` / `2026-01-25 (FY)`）を付す。損益項目はすべて同一期から取り、指標もその期基準（四半期パネルの PER/ROE は単一四半期の利益ベース）。これは元から最新開示を追っていた J-Quants（日本株）経路に US を揃えたもの。発行済株式数は表紙値（`dei:EntityCommonStockSharesOutstanding`。四半期ごとに開示）にフォールバックし、四半期パネルでも BPS/PBR を得る。([src/fundamental.rs](src/fundamental.rs))
- **版を 2.9.1 に統一**（engine・`webui-leptos`・`xoksa-desktop`・`xoksa-setup`・`tauri.conf.json`・README・マニュアル）。

### Documentation（日本語）
- **design-philosophy §8.4** に観測ベース更新を記載。**CONTRIBUTING** の命名規則に実運用（`release/vX.Y.Z`・`docs/`）を追記。**setup.md**（EN+JA）・**usage-guide.md** をダッシュボードのスクリーンショットで更新。

---

## [2.8.1] — unreleased

**A standalone settings app and a connection screen — the desktop connects to a local *or* remote engine — plus an authenticated non-loopback `serve`, and chat/config fixes.** Consolidates the unreleased 2.7.3–2.8.1 line; no change to the numbers or analysis (same SOT).

### Added
- **Standalone settings app (`xoksa-setup`).** All configuration — API keys, LLM / indicators / news / notify, the access token, and a settings password — now lives in a separate app opened from **Settings**, not an in-desktop form. It is password-gated (default `XOKSA_password`, changeable, keychain-stored) and drives the engine's CLI subcommands (`config-json` / `apply-config` / `settings-password` / `conn-token`), so it writes `xoksa.env` + the OS keychain without a running server. ([xoksa-setup/](xoksa-setup/))
- **Desktop connection screen.** The desktop first asks *which engine to talk to* — a local engine it auto-starts, or a remote `xoksa serve` entered by `host:port` + access token — instead of an onboarding form. ([xoksa-desktop/](xoksa-desktop/))
- **Authenticated non-loopback `serve` (2.8.0).** A non-loopback bind (`--host 0.0.0.0`, or the desktop LAN toggle) is **fail-closed**: the engine auto-generates a 256-bit access token (`SERVE_AUTH_TOKEN`, Class A, keychain), shown by `xoksa serve --show-token` and rotatable with `--rotate-serve-token`; clients present `Authorization: Bearer <token>` or exchange it once on `/login` for an `HttpOnly` / `SameSite=Strict` session cookie. Only private/loopback source addresses are accepted. Loopback is unchanged (no auth — the OS account is the trust boundary). ([src/server/mod.rs](src/server/mod.rs), [src/keystore.rs](src/keystore.rs))

### Changed
- **The desktop is a thin connection shell (SOT).** The old onboarding/settings form and its commands (`apply_onboarding` / `load_config` / `list_ollama_models` / `test_notify` / `setup_status` …) were removed from the desktop; all configuration lives in the settings app. ([xoksa-desktop/src/main.rs](xoksa-desktop/src/main.rs))
- **Chat: established general knowledge, tighter answers (2.7.3).** The model may use well-established general knowledge (judging a user's claim with its own knowledge rather than accepting it blindly), makes clearer subject–verb assertions with less vague hedging, and does not add un-asked technical analysis to a specific question. Numbers remain program-computed (SOT unchanged).
- **Command set flattened (2.7.3).** `show` → `tech` / `funda`; news folded into `news` (`nx` removed); forum `crit` / `keep` promoted to top-level; HELP ordered by usage flow; `/debate` · `/board` removed.
- **Config single-source (2.7.4).** `Config::default()` is the one source for every default (clap, build-config, the setup template, and `xoksa.env.sample` all derive from it); 13 real default drifts were fixed (e.g. bare-CLI output-token truncation, user-dependent composite-score weights). §4.2: 18 duplicate-logic groups consolidated.
- **Stable per-client LLM selection (2.7.5).** Switching the timeframe no longer re-selects a random model (a billing risk) — one deterministic per-client selection, so the model shown always equals the one used. A long-open chat now refreshes its grounding when the confirmed bar advances, instead of answering from the snapshot it opened with.
- **Versions aligned to 2.8.1** across the engine, `webui-leptos`, `xoksa-desktop`, `xoksa-setup`, `tauri.conf.json`, README, and the manuals.

### Fixed
- **Ollama cold-start timeout (2.7.4).** The Ollama request timeout floors at 300 s so a large local model no longer times out at 120 s on first use (a higher `OLLAMA_TIMEOUT_SECONDS` still wins).
- **News availability hint (2.7.6).** When the confirmed news section is dropped from a chat prompt for context budget, a compact `news_titles=N` hint is surfaced so the model does not answer "no news" while the panel holds articles (titles omitted; fabrication still forbidden).

### Documentation
- Setup Guide, usage-guide, and command-reference rewritten for the connection screen, the settings app, and the `serve` access token.

**独立した設定アプリと接続画面——デスクトップはローカル／リモートのエンジンに接続でき、非ループバックの `serve` は認証必須に。あわせてチャット／設定の修正。** 未リリースの 2.7.3〜2.8.1 系を集約。数値・分析に変更はありません（SOT 同一）。

### Added（日本語）
- **独立した設定アプリ（`xoksa-setup`）。** すべての設定——API キー・LLM／指標／ニュース／通知・アクセストークン・設定パスワード——を、デスクトップ内フォームではなく **設定** から開く別アプリに集約。パスワードで保護（既定 `XOKSA_password`・変更可・keychain 保管）し、エンジンの CLI サブコマンド（`config-json` / `apply-config` / `settings-password` / `conn-token`）を駆動して、サーバ稼働なしに `xoksa.env` ＋ OS キーチェーンへ書き込む。
- **デスクトップの接続画面。** デスクトップはまず*どのエンジンに接続するか*を尋ねる——自動起動するローカルエンジン、または `host:port` ＋アクセストークンで指定するリモートの `xoksa serve`——onboarding フォームは廃止。
- **非ループバック `serve` の認証（2.8.0）。** 非ループバック bind（`--host 0.0.0.0`・デスクトップの LAN トグル）は **fail-closed**：256bit のアクセストークン（`SERVE_AUTH_TOKEN`・クラスA・keychain）を自動生成し、`xoksa serve --show-token` で表示・`--rotate-serve-token` でローテート。クライアントは `Authorization: Bearer <token>` を提示するか、`/login` で一度 `HttpOnly`／`SameSite=Strict` の cookie と交換する。接続元はプライベート／ループバックのみ許可。ループバックは不変（認証なし＝OS アカウントが信頼境界）。

### Changed（日本語）
- **デスクトップは薄い接続シェルに（SOT）。** 旧 onboarding／設定フォームとそのコマンド（`apply_onboarding` / `load_config` / `list_ollama_models` / `test_notify` / `setup_status` …）をデスクトップから撤去。設定は設定アプリに一本化。
- **チャット：確立した一般知識の活用・断定の明確化（2.7.3）。** 確立した一般知識の使用を許可（ユーザー主張を鵜呑みにせず自分の知識で正誤判断）、主語述語を明確にした断定・曖昧な逃げの抑制、個別質問に問われていないテクニカル分析を足さない。数値はプログラム算出のまま（SOT 不変）。
- **コマンド体系のフラット化（2.7.3）。** `show`→`tech`／`funda`、ニュースを `news` に統合（`nx` 廃止）、forum の `crit`／`keep` をトップレベルへ、HELP を使用フロー順に整列、`/debate`・`/board` 廃止。
- **設定の単一源化（2.7.4）。** `Config::default()` を全既定値の唯一の源に（clap・build-config・setup テンプレート・`xoksa.env.sample` がここから導出）。13 件の既定ドリフト（＝実バグ。素の CLI での出力トークン切り詰め、ユーザー依存の合成スコア重み 等）を修正。§4.2：重複ロジック 18 群を統合。
- **クライアント別の安定した LLM 選択（2.7.5）。** 時間足の切替でモデルがランダムに選び直される課金リスクを解消——クライアント別の決定的な単一選択にし、表示モデル＝使用モデルを保証。長時間開いたチャットは、確定足が進んだら grounding を更新（開いた時点のスナップショットで答え続けない）。
- **バージョンを 2.8.1 に統一**（engine・`webui-leptos`・`xoksa-desktop`・`xoksa-setup`・`tauri.conf.json`・README・各マニュアル）。

### Fixed（日本語）
- **Ollama コールドスタートのタイムアウト（2.7.4）。** Ollama リクエストのタイムアウト下限を 300 秒に。大きなローカルモデルの初回ロードが 120 秒で切れないように（`OLLAMA_TIMEOUT_SECONDS` の上位指定は尊重）。
- **ニュース有無のヒント（2.7.6）。** 確定ニュース節がコンテキスト予算でチャットプロンプトから落ちたとき、`news_titles=N` の簡潔なヒントを提示し、パネルに記事があるのにモデルが「ニュースなし」と答えないようにする（タイトルは省略・創作は依然禁止）。

### Documentation（日本語）
- セットアップガイド・usage-guide・command-reference を、接続画面・設定アプリ・`serve` アクセストークンに合わせて書き換え。

---

## [2.7.2] — 2026-08-08

**Critical fix: re-configuring the LLM in the desktop Settings no longer loses your existing setup (cloud API keys + local models).** The Settings form now recalls the current configuration — including which API keys are set — from the engine, the single source of truth.

### Fixed
- **Desktop Settings recalls the current config (incl. keychain key state).** The desktop parsed `xoksa.env` with its **own** cwd-relative reader, separate from the engine — a second source of truth. Re-saving from that divergent view could overwrite the real config, so re-configuring the LLM appeared to *forget the cloud API keys* and drop the local-model setup. The desktop is now **presentation-only**: it asks the engine (`xoksa config-json`) for the current config, so the form always reflects reality and a re-save preserves everything. ([xoksa-desktop/src/main.rs](xoksa-desktop/src/main.rs), [src/setup.rs](src/setup.rs))
- **Key fields show a "configured" marker.** API keys already stored (env or keychain) now show asterisks in the Settings form, so it is clear a key is set; leaving the field blank keeps it. The key **value** never crosses to the WebView — only a presence flag (§4/§6). ([xoksa-desktop/frontend/index.html](xoksa-desktop/frontend/index.html))

### Changed
- **The desktop no longer contains config-reading logic (SOT).** Its private `xoksa.env` parser was removed; the new engine subcommand `config-json` is the single source. macOS-relevant too — the same shared engine logic backs both platforms.

**重大バグ修正：デスクトップの設定で LLM を再設定しても、既存の設定（クラウド API キー＋ローカルモデル）を失わなくなりました。** 設定フォームは現在の設定（どの API キーが設定済みかを含む）を、単一の情報源であるエンジンから呼び戻します。

### Fixed（日本語）
- **デスクトップ設定が現在の設定（keychain のキー有無を含む）を呼び戻す。** デスクトップはエンジンとは別の cwd 相対パーサで `xoksa.env` を**自前解析**していた（＝二重管理）。その食い違った内容から再保存すると本当の設定を上書きし得たため、LLM を再設定すると**クラウドの API キーを忘れ**、ローカルモデルの設定も失われたように見えた。デスクトップを**表示専用**にし、現在の設定をエンジン（`xoksa config-json`）へ問い合わせるようにした。フォームは常に実態を反映し、再保存で設定を失わない。
- **キー欄に「設定済み」表示。** 既に保存済みの API キー（env または keychain）は設定フォームでアスタリスク表示され、設定済みか一目で分かる。空欄のまま保存すれば保持。キーの**値**は WebView に出さない（有無のフラグのみ・§4/§6）。

### Changed（日本語）
- **デスクトップから設定読取りロジックを撤去（SOT）。** 独自の `xoksa.env` パーサを削除し、エンジンの新サブコマンド `config-json` を単一ソースに。共有ロジックのため macOS でも同修正が効く。

---

## [2.7.1] — 2026-08-05

**Two SOT (single-source-of-truth) fixes: chat 消去 now clears on every surface, and `xoksa.env` has one canonical location.** No change to the numbers or analysis (same SOT).

### Fixed
- **Chat 消去 clears immediately everywhere.** The button gated `/clear` behind `window.confirm`, which the Tauri WebView suppresses — so it worked in a browser but was silently ignored in the desktop. It now sends `/clear` directly: an immediate clear, identical to the CLI. ([webui-leptos/src/main.rs](webui-leptos/src/main.rs))
- **One canonical `xoksa.env` location — no split by launch method.** The config path was resolved cwd-relative in ~9 read/write sites, so the CLI (its cwd) and the desktop (app-data) could read different files. It now resolves through one function (`utils::env_path`): `--env-file` > the canonical `<config_dir>/xoksa/xoksa.env`, with **no** implicit `./xoksa.env` fallback, so it cannot split by launch directory. On first run a **one-time, non-destructive migration** imports an existing config into the canonical location — a pre-2.7.1 desktop config (`app.xoksa.desktop`), else `./xoksa.env` — so upgrades keep their settings. Adds `--env-file` to the CLI and `serve`. ([src/utils.rs](src/utils.rs), [xoksa-paths/](xoksa-paths/))
- **One malformed line in `xoksa.env` no longer wipes every setting.** The env reader failed the *whole file* on any single non-ASCII-safe line (a control char, an over-long line, or a BOM / zero-width character) and returned an empty config — so an Ollama value added via Settings → LLM with a stray pasted zero-width/BOM character silently reset the LLM provider to the default and dropped every local model (and other setting) on the next launch. The reader now **skips only the offending line** (with a warning) and keeps the rest, across all four `xoksa.env` read sites; the Ollama-settings writer also strips zero-width/BOM/`"` characters and caps by bytes so it can no longer emit a line the reader rejects. ([src/utils.rs](src/utils.rs), [src/bootstrap.rs](src/bootstrap.rs), [src/chat/llm.rs](src/chat/llm.rs), [src/setup.rs](src/setup.rs))

### Changed
- **New internal `xoksa-paths` crate** defines the canonical config directory once; both the engine and the desktop depend on it so the location cannot drift (the `dirs` direct dependency is removed from both). ([xoksa-paths/](xoksa-paths/))
- security-design §6 updated for the single-source config resolution.

**2つの SOT 修正：チャット「消去」がどの画面でも効くようになり、`xoksa.env` の場所が単一の正準パスに。** 数値・分析に変更はありません（SOT 同一）。

### Fixed（日本語）
- **チャット「消去」がどこでも即クリア。** 消去は `/clear` を `window.confirm` の裏に置いており、Tauri WebView がこれを抑止するためブラウザでは効くがデスクトップで無反応だった。`/clear` を直接送るようにし、CLI と同一の即時クリアに。
- **`xoksa.env` の場所を単一の正準パスに（起動方法で割れない）。** 設定パスが約9箇所で cwd 相対だったため、CLI（その cwd）とデスクトップ（app-data）が別ファイルを読み得た。単一関数 `utils::env_path` に集約：`--env-file` > 正準 `<config_dir>/xoksa/xoksa.env`（**暗黙の `./xoksa.env` フォールバック無し**＝起動 dir で割れない）。初回起動時に**一度きり・非破壊の移行**で既存設定（2.7.1 以前のデスクトップ設定、無ければ `./xoksa.env`）を正準へインポートするので、更新時も設定を保持。CLI と `serve` に `--env-file` を追加。
- **`xoksa.env` の1行の破損が全設定を消す不具合を修正。** env の読取りは、非ASCII安全な行（制御文字・長すぎる行・BOM/零幅文字）が1つでもあると*ファイル全体*を失敗扱いにし空の設定を返していた。そのため、設定→LLM でローカルLLMを追加した際に値へ零幅/BOM文字が貼り付けで混入しただけで、再起動時に LLM プロバイダが既定へ戻り、ローカルモデル（や他の設定）がすべて消えていた。読取りは**問題の行だけをスキップ**（警告表示）して残りを読むよう、`xoksa.env` を読む4箇所すべてで変更。あわせて Ollama 設定の書き込み側も零幅/BOM/`"` を除去しバイト長で制限し、読取りが弾く行を出さないようにした。

### Changed（日本語）
- **新規内部クレート `xoksa-paths`** が正準の設定ディレクトリを1箇所だけ定義。engine と desktop が両方依存＝場所がドリフトしない（`dirs` 直接依存は両方から撤去）。
- security-design §6 を単一源の設定解決に合わせて更新。

---

## [2.7.0] — 2026-08-03

**Desktop aligned to 2.7.0 and decoupled from the engine version, plus a far easier Japanese-stock name setup (no Excel needed).** No change to the numbers or analysis (same SOT).

### Changed
- **The desktop no longer version-checks the engine.** The desktop (now 2.7.0) attaches to its bundled engine by *service identity* (`/api/health` `service` field), not by version — replacing the same-major check added in #112. The desktop and engine are versioned independently but a release ships them as a matched pair; a deliberately mismatched engine is covered by documentation, not a gate. ([xoksa-desktop/src/main.rs](xoksa-desktop/src/main.rs))
- **Versions aligned to 2.7.0** across the engine, `webui-leptos`, `xoksa-desktop`, `tauri.conf.json`, and README.

### Added
- **The Japanese company-name list can now be an Excel file, read directly.** The setup form gained a "Japanese stock name file (Excel/CSV)" field with a native file picker; `ALIAS_CSV` now accepts the JPX "Listed Securities List" (`data_j.xls`/`.xlsx`) as-is (via `calamine`), so there is no Excel → CSV (UTF-8) conversion step. A `.csv` still works. ([src/bootstrap.rs](src/bootstrap.rs), [src/setup.rs](src/setup.rs), [xoksa-desktop/frontend/index.html](xoksa-desktop/frontend/index.html))

### Fixed
- **Dashboard empty state.** With no ticker loaded, the Technical and News panels now read "Enter a ticker" instead of a misleading "Loading…"; a genuine fetch still shows "Loading…". ([webui-leptos/src/main.rs](webui-leptos/src/main.rs))
- **Removing the last ticker now clears the Market panel** — a removed ticker's snapshot no longer lingers. ([webui-leptos/src/main.rs](webui-leptos/src/main.rs))

### Security
- **New dependencies, both scanned:** `calamine` (engine — reads the JPX `.xls` for the name map) and `tauri-plugin-dialog` (desktop — the file picker, granted `dialog:allow-open` only; the WebView gets no filesystem access). `cargo deny` (licenses / advisories / bans) passes. ([Cargo.toml](Cargo.toml), [xoksa-desktop/capabilities/default.json](xoksa-desktop/capabilities/default.json))

### Documentation
- **av-false-positive-case-study §6.3** records the empty-`fn main() {}` experiment: an unsigned Rust/MSVC GUI exe is flagged by some engines even with no code — evidence the false positive is unsigned-new-hash reputation, not content. ([docs/dev-prog/av-false-positive-case-study.md](docs/dev-prog/av-false-positive-case-study.md))
- Setup guide: the JP name list no longer needs an Excel conversion. Usage guide: the desktop attaches without a version gate. Per-artifact SBOM regenerated. ([docs/manual/setup.md](docs/manual/setup.md), [docs/manual/usage-guide.md](docs/manual/usage-guide.md), [sbom/](sbom/))

**デスクトップを 2.7.0 に揃えてエンジンのバージョンから切り離し、日本株の銘柄名設定を大幅に簡単化（Excel 不要）。** 数値・分析に変更はありません（SOT 同一）。

### Changed（日本語）
- **デスクトップはエンジンのバージョンを確認しなくなりました。** デスクトップ（2.7.0）は同梱エンジンに*サービス同一性*（`/api/health` の `service`）で接続し、バージョンでは判定しません（#112 の同一メジャー確認を置き換え）。デスクトップとエンジンは独立して版管理しますが、リリースは組（matched pair）で出荷します。意図的にバージョンを食い違わせた場合はゲートではなくドキュメントで補完します。
- **バージョンを 2.7.0 に統一**（engine・`webui-leptos`・`xoksa-desktop`・`tauri.conf.json`・README）。

### Added（日本語）
- **日本株の銘柄名一覧を Excel のまま直接読めるようにしました。** 設定フォームに「日本株の銘柄名ファイル（Excel/CSV）」項目とネイティブのファイル選択を追加。`ALIAS_CSV` は JPX の「上場銘柄一覧」（`data_j.xls`/`.xlsx`）をそのまま受け付けます（`calamine` 使用）。Excel → CSV（UTF-8）変換は不要になりました。CSV も従来どおり使えます。

### Fixed（日本語）
- **ダッシュボードの空状態。** 銘柄未読込のとき、テクニカルとニュースのパネルが誤解を招く「読み込み中…」ではなく「銘柄を入力してください」を表示するようにしました（実際のフェッチ中は従来どおり「読み込み中…」）。
- **最後の銘柄を削除すると市況パネルがクリアされる** ようにし、削除した銘柄のスナップショットが残らないようにしました。

### Security（日本語）
- **依存を2つ追加（いずれもスキャン済み）:** `calamine`（エンジン — JPX の `.xls` を読む）と `tauri-plugin-dialog`（デスクトップ — ファイル選択。付与は `dialog:allow-open` のみで、WebView にファイルシステムアクセスは与えない）。`cargo deny`（ライセンス/脆弱性/bans）通過。

### Documentation（日本語）
- **av-false-positive-case-study §6.3** に空の `fn main() {}` 実験を記録：未署名の Rust/MSVC GUI exe はコードが無くても一部エンジンが flag する — 誤検知が中身ではなく「未署名・新規ハッシュのレピュテーション」由来であることの証拠。
- セットアップ手順：日本株の銘柄名一覧に Excel 変換が不要に。使い方ガイド：デスクトップはバージョンゲートなしで接続。現物別 SBOM を再生成。

---

## [2.6.8] — 2026-07-27

**A `--private` promise that the alert panel had been breaking, and the documentation audited against the source.** No change to the numbers or analysis (same SOT).

### Fixed
- **`--private` no longer leaves alert rules on disk.** Adding a rule from the dashboard's alert panel appended `ALERT_<n>_*` to `xoksa.env` unconditionally, so a no-trace session still wrote the user's watched ticker and threshold to disk — contradicting `--private` as stated in [security-design.md §4](docs/dev-prog/security-design.md), [design-philosophy.md §8.2](docs/dev-prog/design-philosophy.md), the README and the usage guide. `persist_add_to_env` / `persist_remove_from_env` now return early under `is_private()` (the rule still runs for the session; it is simply not written), matching the log and saved-strategies paths, and a regression test asserts both directions. The channel secret was never affected — it lives in the OS keychain. ([src/server/monitor.rs](src/server/monitor.rs), [src/private.rs](src/private.rs))

### Security
- **`#![forbid(unsafe_code)]` now covers the desktop crate too.** The engine and the WASM frontend were already compiler-enforced; `xoksa-desktop` contained no `unsafe` but carried no attribute, so the "xoksa's own code contains no `unsafe`, compiler-enforced" claim in [security-design.md §0.2](docs/dev-prog/security-design.md) was not actually enforced there. ([xoksa-desktop/src/main.rs](xoksa-desktop/src/main.rs))

### Documentation
- **Security and design docs audited line-by-line against the source; four discrepancies corrected.** (1) The notification host allowlist also permits `discordapp.com`, which §4 did not list. (2) The desktop spawns a fourth engine subcommand, `test-notify` — the settings-form channel test, which carries the channel secret over the child's stdin — missing from the enumerations in §0.2, §6 and design-philosophy §6. (3) design-philosophy §8.2 claimed the only disk write besides the log was the saved-strategies file; `xoksa.env`, `--out`, `--debug-prompt` and `--save-technical-log` also write, and the omission is what hid the `--private` defect above. (4) design-philosophy §8.3 called its `#[cfg]` list "the complete set of seams" while four were missing: `CREATE_NO_WINDOW`, the per-OS credential-store registration, the diagnostics-log fallback directory, and the Windows rename workaround.
- **Documentation set consolidated 25 → 20 files.** `why-xoksa-rust.md` folded into design-philosophy as §0 (keeping the C++ comparison, dropping what §4.4 / §5 / §6 already said); `file-naming-convention.md` folded into `CONTRIBUTING.md`; the upstream Tauri report draft and the completed macOS handoff removed. `system-design.md` re-synced with the implementation (LLM model names, SEC EDGAR, the alert/notify subsystem, Linux not shipped, signing status, per-OS SBOM). `error-codes.md` gained the missing `XK-ALERT` row and is now linked from the usage guide. `setup.md` gained the download-and-install section it never had.
- **SBOMs are now per target.** `xoksa-native.cdx.json` (Windows-host generated, carrying 17 Windows-only crates) is renamed `xoksa-native-windows-x86_64.cdx.json`, and `xoksa-native-macos-arm64.cdx.json` is published beside it; `sbom/README.md` is rewritten to the actual generation method and now includes the binary-bound `cargo audit bin` step.
- **`cargo-deny` invocation fixed for 0.20+.** `--config` moved before the subcommand, so the webui workspace gate ran instead of erroring out. ([scripts/check.sh](scripts/check.sh), [.github/workflows/ci.yml](.github/workflows/ci.yml))
- **Version set to 2.6.8** across `Cargo.toml` (native + `webui-leptos` + `xoksa-desktop`), `tauri.conf.json` and README.

**`--private` の約束をアラートパネルが破っていた件の修正と、文書のソース突き合わせ。** 数値・分析に変更はありません（SOT 同一）。

### Fixed（日本語）
- **`--private` でアラートルールがディスクに残らないようにした。** ダッシュボードのアラートパネルからルールを追加すると `ALERT_<n>_*` が無条件で `xoksa.env` に追記され、無痕跡セッションでも利用者の監視銘柄と閾値がディスクに書かれていた（[security-design.md §4](docs/dev-prog/security-design.md)・[design-philosophy.md §8.2](docs/dev-prog/design-philosophy.md)・README・使い方ガイドの記載に反する状態）。`persist_add_to_env`／`persist_remove_from_env` が `is_private()` で早期リターンするようにし（ルールはセッション中は動作し、書き込まないだけ）、ログ・保存戦略と同じ扱いに揃えた。両方向を検証する回帰テストを追加。チャンネルの秘密は OS キーチェーン管理のため元から影響なし。

### Security（日本語）
- **`#![forbid(unsafe_code)]` をデスクトップクレートにも適用。** エンジンと WASM フロントエンドは既にコンパイラ強制済みだったが、`xoksa-desktop` は `unsafe` を含まないものの属性が無く、[security-design.md §0.2](docs/dev-prog/security-design.md) の「xoksa 自身のコードに `unsafe` なし（コンパイラ強制）」が実際には強制されていなかった。

### Documentation（日本語）
- **セキュリティ・設計文書をソースと 1 行ずつ突き合わせ、4 件の乖離を修正。** (1) 通知の許可ホストには `discordapp.com` も含まれるが §4 に未記載だった。(2) デスクトップが起動する 4 つ目のサブコマンド `test-notify`（設定フォームのチャンネルテスト。チャンネル秘密を子プロセスの stdin で運ぶ）が §0.2・§6・design-philosophy §6 の列挙から漏れていた。(3) design-philosophy §8.2 は「ログ以外のディスク書き込みは保存戦略ファイルだけ」としていたが、`xoksa.env`・`--out`・`--debug-prompt`・`--save-technical-log` も書く。この記載漏れが上記 `--private` 欠陥を見えなくしていた。(4) design-philosophy §8.3 は `#[cfg]` の「全一覧」と称しながら 4 件が欠けていた（`CREATE_NO_WINDOW`・OS 別の資格情報ストア登録・診断ログのフォールバック先・Windows の rename 回避）。
- **文書を 25 本から 20 本へ統合。** `why-xoksa-rust.md` は design-philosophy §0 へ吸収（C++ との比較を残し、§4.4／§5／§6 と重複する内容は削除）、`file-naming-convention.md` は `CONTRIBUTING.md` へ吸収、Tauri への報告下書きと対応済みの macOS 引き継ぎ書を削除。`system-design.md` を実装に再同期（LLM モデル名・SEC EDGAR・アラート/通知サブシステム・Linux 非配布・署名状況・OS 別 SBOM）。`error-codes.md` に欠けていた `XK-ALERT` を追加し、使い方ガイドから導線を張った。`setup.md` に無かった入手とインストールの節を追加。
- **SBOM をターゲット別に分離。** Windows ホスト生成で Windows 固有 17 crate を含んでいた `xoksa-native.cdx.json` を `xoksa-native-windows-x86_64.cdx.json` に改名し、`xoksa-native-macos-arm64.cdx.json` を併置。`sbom/README.md` を実際の生成手順に書き直し、現物ベースの `cargo audit bin` 手順を追加。
- **`cargo-deny` 0.20 以降の引数順を修正。** `--config` をサブコマンドの前に移し、webui workspace のゲートが実行されるようにした。
- **バージョンを 2.6.8 に設定**（`Cargo.toml`（native ＋ `webui-leptos` ＋ `xoksa-desktop`）・`tauri.conf.json`・README）。

---

## [2.6.7] — 2026-07-23

### Fixed
- **Desktop chart and HELP popups now open on Windows (they had opened blank).** `open_popup_window` and `open_manual` were synchronous `#[tauri::command]`s; a sync command runs on the main/UI thread, and on Windows `WebviewWindowBuilder::build()` there deadlocks — WebView2's `CreateCoreWebView2Controller` completion callback must be pumped by that thread's message loop, but the running command occupies it, so the popup's webview never initialized and its first navigation aborted (`net::ERR_ABORTED`), leaving `about:blank`. macOS/WKWebView was unaffected, which is why the same code rendered there. Making these commands `async` moves `build()` off the event loop; the popup now loads the loopback dashboard in popup mode (`?popup=chart` / `?popup=help`) and self-renders on Windows exactly as on macOS. The former Windows-only injection workaround is removed (`set_popup_content`, its initialization script, `frontend/popup.html`), so both platforms now share one code path. (docs.rs `WebviewWindowBuilder` "Known issues"; wry#583, tauri#3597) ([xoksa-desktop/src/main.rs](xoksa-desktop/src/main.rs), [webui-leptos/src/main.rs](webui-leptos/src/main.rs))
- **No console window flashes when the desktop app launches the engine on Windows.** Engine child processes (`serve` / `apply-config` / `test-notify` / `ollama-models`) are spawned with `CREATE_NO_WINDOW`, so the GUI no longer pops a black console window. ([xoksa-desktop/src/main.rs](xoksa-desktop/src/main.rs))
- **`--debug-prompt` and `--no-llm` are now independent.** `--debug-prompt` had short-circuited the LLM call (it saved the prompt, then `return`ed before sending), and `--no-llm` had suppressed the prompt-save (the save lived inside the `--no-llm`-gated dispatch), so `--no-llm --debug-prompt` wrote no file at all. The single-analysis dispatch now builds the prompt once and applies each flag on its own: `--debug-prompt` only writes `debug_prompt_<ts>.txt`; `--no-llm` only skips the LLM call. So `--debug-prompt` writes the file **and** sends, while `--no-llm --debug-prompt` writes the file **without** sending. ([src/main.rs](src/main.rs), [src/prompt.rs](src/prompt.rs))

### Security
- **Removed the turso-backed embedded SQL database from the engine binary.** `keyring` 4.0's default features transitively linked `db-keystore` — an embedded SQL database (via `turso`) — into the shipped binary, contradicting the No-Database design ([design-philosophy §8.2](docs/dev-prog/design-philosophy.md)) and inflating the supply-chain surface. The engine now depends on `keyring-core` plus the platform-native credential-store crates directly (`windows-native-keyring-store` / `apple-native-keyring-store` / `linux-keyutils-keyring-store`) and registers the OS-native store in `keystore.rs`; the `keyring` meta-crate is no longer used. **API keys are unchanged** — still stored in the OS keychain (Windows Credential Manager / macOS Keychain / Linux keyutils). This drops `turso`, `tantivy`, the sync SDK, and their transitive deps (Cargo.lock shrinks ~3000 lines), restoring the No-Database property and shrinking the binary. Caught by the v2.6.6 shipping inspection. ([Cargo.toml](Cargo.toml), [src/keystore.rs](src/keystore.rs))
- **Notification setup no longer prompts for a channel "name" before the secret.** The unmasked `Channel name for /alert` prompt was a step where a pasted webhook URL (which contains the secret) was written in plaintext to `xoksa.env` and echoed to the terminal; webhook-channel registration now goes straight to the masked secret prompt, and the channel label defaults to the platform name. ([src/setup.rs](src/setup.rs))
- **Windows desktop binary hardened against anti-malware false positives.** The desktop artifact scanned 2/70 on VirusTotal (Microsoft `Trojan:Win32/Wacatac.B!ml` plus Trapmine, both ML heuristics) while the engine scanned 0/69 — the gap was thin PE version metadata: empty `LegalCopyright` and `OriginalFilename`, and a `FileDescription` that was just the product name. The desktop now carries engine-parity metadata — `bundle.publisher` / `bundle.copyright` in [tauri.conf.json](xoksa-desktop/tauri.conf.json) (→ CompanyName / LegalCopyright) and `OriginalFilename` via `[package.metadata.tauri-winres]` in [xoksa-desktop/Cargo.toml](xoksa-desktop/Cargo.toml); the descriptive `FileDescription` is applied post-build with `rcedit`, because tauri-build force-sets it to the product name and adding `winresource` to `build.rs` would embed a second `RT_VERSION` resource and fail to link. **Microsoft now reports the binary clean** (2/70 → 1/70); the residual single detection is a behavioural ML score for a GUI app that spawns its own engine, and has been reported to the vendor. ([security-assessment.md §C.4](docs/dev-prog/security-assessment.md))

---

## [2.6.6] — 2026-07-21

### Changed
- **Desktop engine bundling unified via Tauri `externalBin` (sidecar) on Windows and macOS.** `xoksa-desktop/tauri.conf.json` now sets `bundle.externalBin: ["binaries/xoksa"]`, so the engine is placed next to the app binary and included in the installer on both OSes — replacing the previous macOS manual `cp` of the engine into `XOKSA.app`. A new `scripts/bundle-engine.sh` stages the built engine as the Tauri sidecar `xoksa-desktop/binaries/xoksa-<target-triple>[.exe]` (host triple from `rustc -vV`); run it after `cargo build --release --bin xoksa` and before `cargo tauri build`. `engine_bin_path()` is unchanged — the sidecar lands exactly where it already looks (next to the app binary). `xoksa-desktop/binaries/` is a build artifact and is gitignored. ([xoksa-desktop/tauri.conf.json](xoksa-desktop/tauri.conf.json), [scripts/bundle-engine.sh](scripts/bundle-engine.sh))

### Documentation
- **design-philosophy.md §8.3 "Single Source, Multiple Platforms" added.** Codifies that xoksa is one source tree compiled per target, with OS differences absorbed by a bounded set of `#[cfg(...)]` seams (engine binary name, default-browser open handler, Windows resource embedding, unix-only `0600` file-permission hardening, Windows SChannel TLS). ([docs/dev-prog/design-philosophy.md](docs/dev-prog/design-philosophy.md))

---

## [2.6.5] — 2026-07-20

### Fixed
- **Ollama thinking-budget truncation now auto-recovers.** When a reasoning model (e.g. a think-by-default Gemma build) spends the `num_predict` budget on hidden thinking and the answer is cut off at the generation limit (`done_reason=length`), XOKSA now steps the reasoning effort down and retries — `…→ low → false` — instead of stopping at `low`. Previously the auto-retry only fired on a fully-empty answer and only stepped to `low`, so a model that still over-thinks at `low` never recovered and the body was suppressed with a "raise OLLAMA_NUM_PREDICT" notice. The retry now also covers a partially-truncated (non-empty) body, and disabling thinking gives the whole budget to the answer. If the answer is genuinely longer than the budget with no thinking involved, the notice to raise `OLLAMA_NUM_PREDICT` still applies. Decision is driven by the runtime response, not the model name. ([src/llm.rs](src/llm.rs))

---

## [2.6.4] — 2026-07-20

### Changed
- **Desktop loopback hardened.** The desktop shell now starts the engine on an ephemeral free port instead of a fixed `8787`, and points the WebView at it only after `/api/health` reports a version matching the shell's own. A stale `xoksa serve` or any foreign process holding the port can no longer be mistaken for the app's engine.
- **CI/lint coverage widened.** `.github/workflows/ci.yml` and `scripts/check.sh` now run `cargo fmt --check` and `cargo clippy -D warnings` on the `webui-leptos` and `xoksa-desktop` workspaces (previously only the root crate), so a format or lint regression there is caught before push.
- **Shipping-inspection claim scoped.** The README now states each *shipped* release is binary-inspected, and in-development lines pass the source/dynamic layers first with binary-bound checks recorded at ship time; `security-assessment.md` notes the v2.6.x desktop line as in-development, binary-bound pending.

### Fixed
- **Format violations** in `src/server/monitor.rs` and `webui-leptos/src/main.rs`.
- **`webui-leptos/Cargo.lock`** version was stale (`2.6.0`); now synced to the crate version.
- **`scripts/inspect.sh`** referenced the removed `release-inspection-v2.2.3.md`; repointed to `security-assessment.md`.

---

## [2.6.3] — 2026-07-20

### Changed
- **Setup and integration guides merged.** `integration-guide.md` is folded into [`setup.md`](docs/manual/setup.md) (now the "Setup & Integration Guide") — setup plus reusing xoksa's CSV / JSON / prompt output in external tools. The manual set is now four: `analysis-guide`, `usage-guide`, `setup`, `command-reference`.

---

## [2.6.2] — 2026-07-20

### Changed
- **Analysis manuals consolidated.** `indicator-guide.md`, `investor-guide.md`, and `strategy-guide.md` are merged into a single [`analysis-guide.md`](docs/manual/analysis-guide.md) with three parts — Reading the analysis, Indicators & the score, Strategies & recipes. The in-app 📖 guide (settings form) and `/manual/analysis-guide` now serve this doc; all cross-references were repointed.

---

## [2.6.1] — 2026-07-20

### Changed
- **Manuals consolidated.** `webui-guide.md`, `chatmode.md`, and `ollama-guide.md` are merged into a single [`usage-guide.md`](docs/manual/usage-guide.md) with three parts — Dashboard (GUI), Chat & commands, Local LLM (Ollama) — so there is one "how to use it" guide instead of three overlapping ones. The alert panel is documented there (each field's meaning, operator direction with examples, when a rule fires, the confirmed-bar timestamp, and persistence). All cross-references were repointed.
- **Header dropdown renamed** from "Analyze" to "Tools" (it holds analysis actions plus backtest and alerts).

---

## [2.6.0] — 2026-07-20

**Chat push notifications with a dashboard alert panel, and J-Quants is now v2-only.**

### Added
- **Alerts panel in the dashboard.** Alert rules are a first-class product feature: a panel in the Web UI lists your rules and channels and lets you add / delete / enable / disable a rule and send a channel test, backed by `/api/alerts`. A rule created here is persisted to `xoksa.env` (`ALERT_<n>_*`) so it survives a restart; the chat `/alert` command remains session-only.
- **Chat push notifications (alerts).** While `xoksa serve` runs, an in-session monitor watches the confirmed intraday bars and, when a user-defined condition on a **computed value** is met (e.g. `RSI ≤ 30`), pushes a one-line message to a chat platform — the confirmed value and threshold, never a trade call (see [security-design.md](docs/dev-prog/security-design.md) §4). Supported platforms: **Slack / Discord / Google Chat** (incoming webhook) and **LINE** (Messaging API); outbound is restricted to a fixed per-platform host allowlist, `https`-only. Channels are defined by `NOTIFY_<n>_*` and rules by `ALERT_<n>_*` in `xoksa.env`; the channel secret is Class A (OS keychain, never the env file). Rules can be managed live from the dashboard chat with **`/alert`** (`list` / `on` / `off` / `add` / `del` / `test`), with an optional short LLM note that passes the §1 output-integrity guard. Channel secrets are registered via `xoksa --update-key` or the desktop settings form (a new **Chat notifications** section).
- **Test a notification channel from the desktop settings form.** Each channel row has a **Test** button that sends one message through the engine (`xoksa test-notify`) using the secret typed in the form — so a channel can be verified before it is saved. The secret travels over the child's stdin (never argv) and the engine enforces the host allowlist.

### Removed
- **J-Quants V1 API support.** The J-Quants V1 API was discontinued on 2026-06-01. The V1 token auth (id/refresh token, email+password → `auth_user`/`auth_refresh`) and the `/v1/*` fetch path are removed; J-Quants is now **v2-only**, a single `JQUANTS_API_KEY`. The v1 credential keys (`JQUANTS_ID_TOKEN` / `JQUANTS_REFRESH_TOKEN` / `JQUANTS_EMAIL` / `JQUANTS_PASSWORD`) are gone from `--update-key`, `--check-keys`, and the config.

---

## [2.4.0] — 2026-07-20

**Desktop is now a UI-only shell that launches the engine as a child process.** No changes to the numbers or analysis (same SOT) — this is an architecture change, not a calculation change.

### Changed
- **The desktop app no longer embeds the engine.** Previously `xoksa-desktop` linked the `xoksa` crate and ran the analysis server in-process — a second copy of the engine runtime (an SOT violation). It now links no engine code (the `xoksa = { path = ".." }` dependency was removed) and instead launches the `xoksa` engine binary shipped alongside it as a child process (`xoksa serve --ui`), then points the WebView at the loopback dashboard. The desktop is a pure UI shell: it performs no indicator calculation, news search, or LLM analysis. This is enforced structurally — with no `xoksa` dependency the desktop crate cannot compile engine code — and is verifiable in one line. The dashboard and all tunings are unchanged (they live in the shared `src/` + webui), so there is no behavioural difference; the desktop binary drops from ~22.8 MB to ~6.6 MB.

### Added
- **Engine subcommands for the desktop UI shell (non-interactive).** `xoksa apply-config` reads a config JSON payload on stdin and writes `xoksa.env` + the OS keychain through the same `apply_setup` path as `--init` (API keys travel over stdin, never argv/HTTP; the buffer is zeroized after use). `xoksa ollama-models --host H --port P` prints the reachable Ollama instance's installed model names as a JSON array. The desktop calls these via child processes instead of linking the engine, so config writing and keychain access live in one place (SOT).

---

## [2.3.5] — 2026-07-19

**Opt-in LAN access for the desktop dashboard.** No engine/behaviour changes to the numbers (same SOT).

### Added
- **Desktop "LAN access" setting (default off).** The desktop app bound its in-process server to `127.0.0.1` only, so the dashboard could not be opened from another device on the network. A new **Network → Allow LAN access** toggle in the setup/settings form binds the server to `0.0.0.0` when enabled (the WebView still connects to `127.0.0.1`, so the local app is unchanged). It is **off by default and gated by a prominent warning**: the dashboard has no login, so exposing it lets anyone on the network use the configured LLM (cost) and see the user's data — enable only on a trusted network, otherwise front it with an authenticating reverse proxy. The bind mode is persisted as `DESKTOP_LAN_ACCESS` in `xoksa.env`. The 0.0.0.0 surface was security-tested: all app-layer controls (strict CSP, `sid` isolation, CSRF `Host`-match origin gate, body limit, path traversal) hold identically to loopback; the only difference is the documented no-auth (see [security-assessment.md](docs/dev-prog/security-assessment.md) C.4).

---

## [2.3.4] — 2026-07-19

**Desktop chart hover tooltips.** No engine/behaviour changes to the numbers (same SOT).

### Fixed
- **Chart hover tooltips now show in the desktop app.** The chart's point/volume tooltips relied on SVG `<title>`, which WKWebView (the Tauri webview) does not render — so tooltips appeared in the browser build but not the desktop popup. Tooltip text now lives in a `data-tip` attribute and is shown in a cursor-following DOM element (positioned via CSSOM, so it works under the strict popup CSP), consistently in both the browser and the desktop.

---

## [2.3.3] — 2026-07-19

**Ollama reasoning-model robustness + a design-doc note.** No engine/behaviour changes to the numbers (same SOT).

### Fixed
- **Reasoning models no longer return an empty answer.** A reasoning model (e.g. gpt-oss:20b) can spend its whole `num_predict` budget on hidden thinking and return nothing (`done_reason=length`), which was surfaced as a "response truncated" notice. The Ollama request now detects this (empty content + thinking present) and retries once with reduced reasoning effort (`think=low`), so a usable answer fits — keyed on the runtime response, not the model name. Complements the `num_ctx` out-of-memory step-down from 2.3.2.

### Docs
- **design-philosophy §7.3** now records both runtime adaptations (num_ctx OOM step-down; reasoning-effort reduction on an empty answer) as a design principle — the decision is the runtime response, never the model name (same family as the §7.4 guard retries).

---

## [2.3.2] — 2026-07-19

**Security hardening, a ticker-selection UX addition, an LLM output-cap fix, and desktop dashboard layout fixes.** No engine/behaviour changes to the numbers (same SOT).

### Added
- **Multiple Ollama instances in the desktop setup form.** The onboarding / settings form previously accepted only one Ollama instance, though the engine (and CLI `--init`) already supported many (`OLLAMA_<n>_*`). The Ollama section is now a repeatable list: add/remove rows, each with its own alias/model/host/port, a **selectable model dropdown** filled by a per-row "fetch models (reachability check)" (with a manual-entry fallback), and remove buttons. All rows are written on save and restored in settings mode; the LLM picker then lists every instance. Frontend-only change — the `apply_onboarding` / `load_config` bridge already handled a vector. **Aliases must be unique**: the picker addresses an instance as `ollama:<alias>`, so two rows sharing an alias would silently run the first match's model (e.g. picking a 12 GB model but running a resident 65 GiB one → out-of-memory). New rows get a unique default (`local`, `local2`, …), duplicates are flagged red, and save is blocked until they are distinct.
- **Per-ticker analysis-target selection.** Each loaded ticker chip now has a checkbox; `/basic` and chat cover exactly the **checked** subset (1..=all) instead of always every loaded ticker. The label still switches the dashboard's active ticker and `×` still unloads it; at least one target is required.

### Security / Fixed
- **Prompt injection via news titles (found & fixed).** News titles/URLs are attacker-influenceable and were inserted into the LLM prompt **raw**. They are now sanitised at the single ingestion boundary — single line, control chars stripped, structural markers (`=== 比較対象 ===`, `【制約】`, `---`, code fences) neutralised, length-capped; URLs kept only if `http(s)` — so a crafted title cannot forge the prompt's structure. The **output-integrity guard** (rejects out-of-input numbers / bare ranges / independent trade levels) was **generalised from Ollama-only to every provider** (openai/gemini/claude/ollama). Deterministic tests added. Recorded as Finding #7 in [security-assessment.md](docs/dev-prog/security-assessment.md).
- **Desktop `open_external` command injection (Windows).** The Windows path used `cmd /C start`, which treats `&` `|` `^` in a URL as shell metacharacters (a query string could break or inject a command). Now uses `rundll32 url.dll,FileProtocolHandler` (no shell parsing); `http(s)`-only (macOS `open` / Linux `xdg-open` were already safe).
- **Desktop IPC least-privilege.** The main window's Tauri capability was split: the loopback **dashboard** origin (which carries news / LLM content) now gets only `open_popup_window` + `open_external` + the event channel — **no `core:default`**. The local onboarding form keeps `core:default`.
- **Settings-window IPC.** The ⌘, settings window (same first-run form, re-opened) now has the explicit ACL grant (`allow-setup-commands`) it needs to read/write config, so re-saving from settings and restarting works.
- **Ollama out-of-memory now auto-recovers instead of failing.** A large `OLLAMA_NUM_CTX` (default 32768) makes the model's KV cache huge — e.g. gpt-oss:20b needs ~65 GiB at 32768 — so loading it while another model is still resident (`keep_alive`) returned `500 … model requires 65.4 GiB but only 20.8 GiB are available`. The Ollama request now steps `num_ctx` down (32768 → 16384 → 8192 → 4096 → 2048) and retries on an out-of-memory error, so a local model "just works" without hand-tuning. The user-facing failure message also now appends Ollama's actual error detail (matching the Gemini path) instead of a bare status.
- **Unified golden/dead-cross wording and icons.** EMA said "ゴールデンクロス進行中" (🟢) while SMA / Ichimoku said "発生中" (📈) for the same short-above-long **state** — inconsistent phrasing, not a semantic difference. All three now read "進行中 / in progress" with a matching chart-pair icon (📈 golden / 📉 dead). MACD's distinct "→ possible reversal" signal line is unchanged.
- **Indicator display order no longer reshuffles.** `render_ranked` broke weight ties with a per-render random key, so equal-weight indicators (the default — every weight `1.0`) were re-ordered on every auto-reload. Ties now fall back to the canonical insertion order (basic first, then the `enabled_extensions` order) via a stable sort; the weight ranking itself is unchanged. Affects the terminal display, `/show technical`, the Web panel, and the LLM prompt (one shared ordering).
- **Chart-interval chat grounding no longer leaks raw floats.** The brushed chart interval attached to a chat message (`【選択区間 … の確定値（全足）】`) formatted each per-bar value with `f64::to_string()` — full precision — so the LLM echoed values like `6.464188372238425`. It is now display-rounded (≤4 dp, trailing zeros trimmed) to match the panels; still the same SOT numbers. The other LLM paths (chat seed, context pack, ticker snapshot) already rounded.
- **Desktop dashboard layout robustness.** The right column (fundamentals / news / market) no longer bleeds into the chat panel below: the shell is flex-based (the header takes its natural height even when the controls wrap to two rows; the dashboard fills the rest) and the right column clips instead of overflowing. The **market panel is now always visible at startup** — a measured clamp reserves room for it (a CSS `max-height:%` does not resolve against the stretch-sized flex parent in WKWebView), shrinking the fundamentals panel first so news headlines stay visible on short windows. The column divider defaults to a fraction of the **actual** window width (measured after mount, since `inner_width` at init is premature), and the panel titles were tightened.
- **Onboarding env-value hygiene.** Values written to `xoksa.env` are now control-char-stripped and length-capped, so a pasted newline cannot inject a second env line.
- **`/basic` no longer truncates on verbose reasoning models.** The output-cap default was raised `8192 → 16384` (`llm_max_output_tokens` / `claude_max_tokens`), and Claude now surfaces a truncation notice (as Gemini already did).

### Changed / Docs
- **Consolidated the security documentation** into two: **[security-design.md](docs/dev-prog/security-design.md)** (design) and **[security-assessment.md](docs/dev-prog/security-assessment.md)** (everything else — Reliability & Reproducibility + Security assessment + Shipping inspection). `reliability.md` and the versioned `release-inspection-*.md` were merged in; a Desktop (Tauri) surface assessment (DT1–DT9) was added.
- **Startup disclaimer consent** on the desktop first-run form (must acknowledge before proceeding); analysis output and forecast carry no extra disclaimers (over-disclaiming reads as suspicious).

---

## [2.3.1] — 2026-07-18

**Desktop GUI display fixes.** Interactions that worked in the browser dashboard but were broken inside the desktop app's WebView are now fixed, plus a light-theme rework and an LLM output-cap fix. No engine/behaviour changes (same SOT).

### Fixed
- **Chart opens as a native window in the desktop app.** The WebView returns `null` from `window.open("")`, so the browser popup path never opened a chart there. The chart now opens as a real OS window (multi-display friendly), rendering the same SOT bar series and indicator toggles; drag-select on it attaches the interval to the main chat as reference (delivered across windows via a Tauri event, since the popup has no shared JS realm with the dashboard).
- **Help opens as a native window** the same way, with the command list and click-to-insert into the chat input.
- **External links (news) open in the OS default browser.** The WebView cannot follow `target="_blank"`; such links are now routed to the OS browser (http(s) only).
- **`/basic` no longer cuts off mid-answer on reasoning models.** A thinking model (e.g. `gemini-3.5-flash`) shares its output budget between hidden reasoning and the answer, so the default cap starved the reply. Raised `llm_max_output_tokens` / `claude_max_tokens` default to **8192** (was 4096), and Gemini now surfaces a truncation notice instead of silently stopping.
- **Light theme.** Buttons ignored the theme (a backtest-modal `.btn` rule leaked globally and forced dark buttons everywhere) — scoped to the modal. Reworked the light palette into three clear levels — grey backdrop, lighter panels, white input wells — so entry fields are obvious.

**デスクトップ GUI の表示不具合を修正。** ブラウザ・ダッシュボードでは動くがデスクトップの WebView 内で壊れていた操作を修正し、ライトテーマの刷新と LLM 出力上限の修正を加えました。エンジン/挙動の変更はありません（SOT 同一）。

### 修正
- **デスクトップでチャートがネイティブ別ウィンドウで開く。** WebView は `window.open("")` が `null` を返すため、従来のブラウザ用ポップアップ経路ではチャートが開かなかった。実際の OS ウィンドウとして開くようにし（マルチディスプレイ対応）、同一の SOT バー系列・指標トグルを描画。ドラッグで区間選択するとメインのチャットに参照として添付される（別窓は同一 JS realm を共有しないため、Tauri イベント経由で受け渡し）。
- **Help も同様にネイティブ別ウィンドウで開く。** コマンド一覧＋クリックでチャット欄へ挿入。
- **外部リンク（ニュース）を OS 既定ブラウザで開く。** WebView は `target="_blank"` を辿れないため、http(s) のリンクを OS ブラウザへ回す。
- **`/basic` が推論モデルで途中で切れない。** 思考モデル（例 `gemini-3.5-flash`）は出力予算を隠れた思考と回答で共有するため、既定上限では回答が不足していた。`llm_max_output_tokens` / `claude_max_tokens` の既定を **8192**（旧 4096）に引き上げ、Gemini は打ち切りを黙って行わず注記を表示するようにした。
- **ライトテーマ。** ボタンがテーマを無視していた（バックテスト用モーダルの `.btn` 規則が全体に漏れ、全ボタンをダーク固定）ため、モーダル内に限定。ライトの配色を3階層（グレーの地・淡いパネル・白い入力ウェル）に組み直し、入力欄が一目で分かるようにした。

---

## [2.3.0] — 2026-07-17

**A desktop app for non-terminal users.** XOKSA now ships a beginner-friendly desktop app (Tauri) alongside the CLI — the same engine over one codebase, delivered as two artifacts. First-run onboarding, in-app settings, and a signed build lower the barrier for people who won't open a terminal, while the CLI stays a lean single self-contained binary.

### Added
- **Desktop app (Tauri).** A thin wrapper that starts the same engine in-process and hosts the dashboard in an OS-native WebView. Includes a **first-run onboarding form**, an in-app **manual viewer** (`📖` links open `/manual/*`, jumping to the reader's language), a native **Settings** menu (⌘,) that re-opens the form pre-filled with the current config, and **Restart**. It lives in its own `xoksa-desktop` workspace, so the CLI / engine gains no Tauri/WebView dependency. API keys are entered over the Tauri IPC boundary and stored in the OS keychain (never over HTTP). Design / security: [design-philosophy.md](docs/dev-prog/design-philosophy.md), [system-design.md](docs/dev-prog/system-design.md), [security-design.md §6](docs/dev-prog/security-design.md).
- **Cloud LLM model selection.** The onboarding / settings form now lets you set the OpenAI / Gemini / Claude model (previously only Ollama had a model field); leaving it blank uses the default.

### Changed
- **Updated default cloud models** to current balanced-tier releases: `gpt-5.6-terra`, `gemini-3.5-flash`, `claude-sonnet-5` (were `gpt-5.4-nano`, `gemini-2.5-flash`, `claude-sonnet-4-6`). Applied across config, `xoksa.env.sample`, and the manuals.
- **Setup Guide reworked** ("Getting Started"): desktop-first, with a screenshot walkthrough of the onboarding form; CLI-only material (`--init`, the wizard) moved to the Command Reference.
- **User-facing copy now says "the program computes …" instead of naming "Rust"** (the language name is meaningless to the target reader); developer docs still name Rust where it is a fact.
- **Two-binary distribution documented** across the design docs (one engine, two artifacts: CLI + desktop).
- **Version set to 2.3.0** across `Cargo.toml` (native + `webui-leptos`), README, and the manual / dev docs.

### Fixed
- **`xoksa --init` no longer silently overwrites an existing config.** When `xoksa.env` already exists, `--init` now confirms first and offers to show the current file before recreating it.

**ターミナルを使わない人向けのデスクトップアプリ。** XOKSA は CLI に加えて、初心者向けのデスクトップアプリ（Tauri）を配布するようになりました——同一エンジン・同一コードベースを2つの成果物として提供します。初回設定・アプリ内設定・署名付きビルドで、ターミナルを開かない人の敷居を下げつつ、CLI は軽量な単一自己完結バイナリのままです。

### 追加
- **デスクトップアプリ（Tauri）。** 同じエンジンをインプロセスで起動し、OS ネイティブの WebView でダッシュボードをホストする薄いラッパー。**初回起動フォーム**、アプリ内の**マニュアル表示**（`📖` リンクが `/manual/*` を読者の言語で開く）、現在の設定を事前入力して再表示する**設定**メニュー（⌘,）、**再起動**を備える。独自の `xoksa-desktop` workspace にあり、CLI / エンジンは Tauri/WebView 依存を持たない。APIキーは Tauri IPC 境界を通り OS キーチェーンに保管（HTTP を通らない）。設計 / セキュリティ: [design-philosophy.md](docs/dev-prog/design-philosophy.md)・[system-design.md](docs/dev-prog/system-design.md)・[security-design.md §6](docs/dev-prog/security-design.md)。
- **クラウド LLM のモデル選択。** 初回 / 設定フォームで OpenAI / Gemini / Claude のモデルを指定できるように（従来は Ollama のみ）。空欄は既定を使用。

### 変更
- **クラウド既定モデルを現行の中位ティアに更新**：`gpt-5.6-terra`・`gemini-3.5-flash`・`claude-sonnet-5`（旧 `gpt-5.4-nano`・`gemini-2.5-flash`・`claude-sonnet-4-6`）。config・`xoksa.env.sample`・マニュアルに反映。
- **セットアップガイドを刷新**（「Getting Started」）：デスクトップ優先、初回フォームのスクリーンショット解説。CLI 専用（`--init`・ウィザード）はコマンドリファレンスへ移動。
- **利用者向けの文言を「プログラムで計算」に**（言語名 "Rust" は対象読者に無意味）。開発者向け文書は事実として Rust を明記。
- **二本立ての配布アーキを設計文書に明記**（一つのエンジン・二成果物：CLI ＋ デスクトップ）。
- **バージョンを 2.3.0 に**（`Cargo.toml`（native ＋ `webui-leptos`）・README・マニュアル / 開発文書）。

### 修正
- **`xoksa --init` が既存設定を無警告で上書きしなくなった。** `xoksa.env` が既にある場合、作り直す前に確認し、現在のファイルの表示を選べる。

---

## [2.2.5] — 2026-07-15

**Multi-ticker chat now analyzes every loaded ticker.** With more than one ticker loaded, a comparison question ("of these, which would you buy?") could end up analyzing only one of them. The per-ticker data model was hardened and the single-symbol heuristic removed so every command spans the whole loaded set.

### Fixed
- **Multi-ticker questions now cover every loaded ticker.** With several tickers loaded, a comparison question could analyze only one of them: the Web chat guessed whether a turn was a "comparison" from keywords/ticker mentions and, when it guessed "no," scoped the answer to the single active symbol. That heuristic is removed — every command now targets the full loaded set unconditionally (the handling does not change with ticker count). Internally the per-ticker data moved from seven parallel arrays into a single `TickerEntry` table, so a ticker's fields can no longer desync and a ticker can no longer be silently dropped. Presentation and all confirmed (Rust-computed) values are unchanged (SOT).

### Changed
- **Two more runtime notices routed to the coded logger.** An invalid ticker-format rejection (`XK-TICKER-INVALID`) and a market-timezone parse fallback (`XK-MARKET-TZ`) now go through the structured logger with stable codes instead of an ad-hoc `eprintln!`, and are listed in [`docs/dev-oper/error-codes.md`](docs/dev-oper/error-codes.md).
- **Version set to 2.2.5** across `Cargo.toml` (native + `webui-leptos`), README, and the manual / dev docs.

**複数銘柄チャットが、読み込んだ全銘柄を解説するようになりました。** 複数銘柄を読み込んだ状態で比較（「この中で買うならどれ？」）を依頼すると、1銘柄しか解説しないことがありました。銘柄ごとのデータ構造を堅牢化し、1銘柄に絞る推測処理を撤去して、すべての指令が読み込んだ全銘柄を対象にします。

### Fixed（日本語）
- **複数銘柄の質問が、読み込んだ全銘柄を対象にするようになった。** 複数銘柄を読み込んだ状態で比較を依頼しても1銘柄しか解説しないことがあった：Web チャットがキーワードや銘柄名の有無から「比較かどうか」を推測し、「比較でない」と判定すると回答を表示中の1銘柄に絞っていた。この推測処理を撤去し、すべての指令が読み込んだ全銘柄を無条件に対象とする（銘柄数で扱いは変えない）。内部でも銘柄ごとのデータを7本の並列配列から単一の `TickerEntry` テーブルへ移し、銘柄のフィールドがずれる・銘柄が黙って欠落することが起き得ない構造にした。表示形式と確定値（Rust 計算済み）は不変（SOT）。

### Changed（日本語）
- **実行時通知2件をコード付きロガーへ追加。** 銘柄フォーマット不正の却下（`XK-TICKER-INVALID`）と市場タイムゾーンのパースフォールバック（`XK-MARKET-TZ`）を、場当たり的な `eprintln!` ではなく安定コード付きで構造化ロガー経由にし、[`docs/dev-oper/error-codes.md`](docs/dev-oper/error-codes.md) に記載。
- **バージョンを 2.2.5 に設定**（`Cargo.toml`（native + `webui-leptos`）・README・マニュアル/開発ドキュメント）。

---

## [2.2.4] — 2026-07-12

**Signed macOS build, a conversational chat overhaul, weight-ranked indicators, file-based error logging, plus News search and robustness fixes.** The macOS binary is now signed with a Developer ID Application certificate and notarized by Apple, so Gatekeeper no longer shows the "unidentified developer / cannot verify" prompt on first run. Chat now answers the question directly instead of reciting every indicator: the response-style controls were reworked (`/depth`, `/scope`, `/shape`, `/cast`), the SOT frame is delivered as a provider `system` message so even small local models follow it, and indicators are presented ranked by their configured weight. Runtime diagnostics now go to a coded local log file. Also adds News free-text search, a news-client timeout, and three Web-chat UX fixes.

### Added
- **News free-text search (Web UI).** The News panel gains a search box, an "銘柄情報を含む" (include ticker) checkbox, and Search / Reset buttons. Typing terms and searching queries Brave with exactly that text (via `custom_news_query`; URL-encoded at the request boundary); the checkbox appends the active ticker (e.g. `6740.T`) to the box; Reset clears the box/checkbox and returns to the default ticker-derived news.
- **Cancel button for an in-flight chat request (Web UI).** While a reply is streaming, a `✕` button appears next to Send; clicking it aborts the request (a generation guard stops the SSE task and closes the connection), keeps whatever was already received, and returns the input to a ready state.
- **Reworked chat response-style controls.** `/depth` (shallow·mid·deep), `/scope` (narrow·mid·wide), `/shape` (**talk**·points·scenario, default talk), and `/cast` (off·soft·bold) replace the older tone/hypothesis/sensitivity axes. The default is a conversational answer that leads with a direct response to the question, not a uniform indicator recital.
- **Runtime diagnostics moved to the coded logger, with a relocated log file.** The remaining ad-hoc `eprintln!` notices now go through the structured logger (stable `XK-<CATEGORY>-<NAME>` code, timestamp column; text or NDJSON via `serve --log-format json`). Expected/recoverable conditions (an ETF's missing fundamentals, a news-fetch failure, the Ollama integrity exclusion) are file-only (no console noise). The log moved from `~/.xoksa.log` to **`logs/xoksa-error.log` under the working directory** (`logs/` auto-created; absolute path printed at startup as `Diagnostics log: …`; if the cwd isn't writable, a machine-wide **system** fallback — `%PROGRAMDATA%\xoksa\logs` on Windows, `/tmp/xoksa` on Unix — **never the user home**), and rotation is now **generational** (`.1`/`.2` past 5 MB) instead of a wipe. Codes are documented in [`docs/dev-oper/error-codes.md`](docs/dev-oper/error-codes.md).

### Changed
- **Chat answers the question first (BLUF).** The reply opens with a direct answer to what was asked; under `/cast`, it leads with the directional read and the pivotal level/condition rather than reciting every indicator and closing on a vague "watch the market."
- **Indicators are presented ranked by their configured weight** (highest first; equal weights ordered randomly), replacing the fixed category order — in the terminal analysis, `/show technical`, and the LLM prompt alike. The lead indicator now reflects *your* weights, not a hardcoded sequence. Ordering is presentation only; no computed value or score changes.
- **The SOT reinforcement + guard are delivered as a provider `system` message** (OpenAI/Ollama system role, Gemini `system_instruction`, Claude `system` field) instead of folded into the user turn — identically for every provider. Small local models obey a system message far more reliably.
- **`/scope wide` now actively engages the macro premises you raise** (e.g. a summer lull, a shipping disruption) and ties them to the read, instead of only permitting general knowledge.
- **Advice boundary refined (design §6).** XOKSA never *proactively* recommends or prescribes order prices, but when you explicitly ask ("is now a good time to buy in?") it gives a grounded, conditional read — the direction the data leans and what would change it — not a naked buy/sell order. General reasoning and use of provided news *titles* are explicitly allowed; fabricating numbers, facts, or news stays forbidden.
- **macOS binary is Developer ID–signed and notarized.** Signed with `Developer ID Application` (hardened runtime + secure timestamp) and verified through Apple notarization; a quarantined download now launches without the Gatekeeper warning. Verified against the published `xoksa-macos-arm64.zip` hash and a VirusTotal scan of the shipped binary — recorded in the release inspection.
- **Chat input now defaults to two lines (Web UI).** The textarea opens at a two-row height (still auto-grows to a cap) so a typical question fits without scrolling.
- **Version set to 2.2.4** across `Cargo.toml` (native + `webui-leptos`), README, and the manual / dev docs.

### Fixed
- **ETFs no longer print a "Fundamental fetch failed" error.** A ticker without SEC/J-Quants filings (e.g. an ETF) has no fundamentals by design; the absence is now recorded file-only (`XK-FUND-MISSING`) instead of a console warning.
- **Web UI Help no longer pre-selects a value.** Clicking a value-list command (e.g. `/depth shallow|mid|deep`) now inserts just the command (`/depth `) so you pick the value, instead of pre-filling the first option (`/depth shallow`).
- **News free-text search routes through the shared sanitizer.** The Web UI search now passes its query through the CLI's `sanitize_news_query` (single source of truth) instead of a raw trim.
- **News (Brave) HTTP requests now time out.** The Brave news client had no request timeout, so a stalled Brave endpoint or proxy could hang the CLI and the Web UI `…/news` fetch indefinitely. It now uses a 15s timeout, matching the other outbound clients.
- **Chat input reliably clears after sending.** Submitting a question empties the box (and resets the two-row height); the ↑/↓ history recall is unchanged.

### Security
- **Runtime diagnostics go to a `0600` local log file** that respects `--private` (no-trace: nothing written) and, per [security-design.md](docs/dev-prog/security-design.md) §2, never contains credentials or chat/user-input content.
- **macOS: signed + notarized.** Gatekeeper accepts the download without the "unidentified developer" prompt.
- **Windows: unchanged (still unsigned).** SmartScreen may warn on first run; verify via the published SHA-256 and the VirusTotal result. Windows code signing remains deferred.
- **Availability:** the Brave news timeout above removes an unbounded-wait path (`docs/dev-prog/security-design.md` §3).

**署名済み macOS ビルド・会話化したチャット・重み順の指標表示・ファイルへのエラーログ、加えてニュース検索と堅牢性修正。** macOS バイナリを Developer ID Application 証明書で署名し、Apple の公証を通したため、初回起動時に Gatekeeper の「開発元を検証できません」プロンプトが出なくなりました。チャットは全指標を並べる代わりに質問に直接答えるようになりました：応答スタイルを再構成（`/depth`・`/scope`・`/shape`・`/cast`）し、SOTフレームをプロバイダの `system` メッセージで届けることで小型ローカルモデルでも従うようにし、指標は設定した重みの順で提示します。実行時の診断はコード付きのローカルログファイルへ落とします。ニュースのフリーテキスト検索・ニュースクライアントの timeout・Web チャットの UX 修正3点も含みます。

### Added（日本語）
- **ニュースのフリーテキスト検索（Web UI）。** News パネルに検索ボックス・「銘柄情報を含む」チェックボックス・検索/リセットボタンを追加。語を入れて検索すると、そのテキストで Brave を検索する（`custom_news_query` 経由・送信境界で URL エンコード）。チェックを入れると現在のティッカー（例 `6740.T`）を検索ボックスに追記。リセットでボックスとチェックを消し、通常の銘柄ニュースに戻す。
- **チャット依頼の取り消しボタン（Web UI）。** 応答のストリーミング中、送信ボタンの隣に `✕` ボタンが表示され、クリックで依頼を中断（世代ガードで SSE タスクを停止し接続を閉じる）。受信済みの内容は残し、入力欄を送信可能な状態に戻す。
- **チャット応答スタイルの再構成。** `/depth`（shallow・mid・deep）・`/scope`（narrow・mid・wide）・`/shape`（**talk**・points・scenario、既定 talk）・`/cast`（off・soft・bold）が、旧来の tone/hypothesis/sensitivity 軸を置き換える。既定は、質問への直接の答えから入る会話体の回答（全指標の一様な列挙ではない）。
- **実行時診断をコード付きロガーへ移行、ログファイルを移設。** 残っていた場当たり的な `eprintln!` を構造化ロガー経由に（安定コード `XK-<カテゴリ>-<名前>`・日時列・text または `serve --log-format json` で NDJSON）。想定内/回復可能な状況（ETF のファンダ欠如・ニュース取得失敗・Ollama 整合性による除外）はファイルのみ（コンソールに出さない）。ログは `~/.xoksa.log` から **作業ディレクトリ直下の `logs/xoksa-error.log`** へ移設（`logs/` 自動作成・絶対パスは起動時に `Diagnostics log: …` として表示・cwd が書込不可ならマシン共通の**システム**へフォールバック（Windows は `%PROGRAMDATA%\xoksa\logs`、Unix は `/tmp/xoksa`）で**ホームには一切書かない**）、ローテートは全消しから **世代式**（5MB超で `.1`/`.2`）に変更。コードは [`docs/dev-oper/error-codes.md`](docs/dev-oper/error-codes.md) に記載。

### Changed（日本語）
- **チャットは結論から答える（BLUF）。** 回答は問われたことへの直接の答えで始まり、`/cast` 時は全指標を並べて最後に曖昧な「注視」で締めるのではなく、方向の見立てと転換の水準・条件を先に述べる。
- **指標は設定した重みの順で提示**（大きいものが先頭、同値はランダム）。固定のカテゴリ順を置き換え、端末分析・`/show technical`・LLM プロンプトのいずれも同じ順。先頭の指標は*あなたの*重みを反映し、ハードコードの序列ではない。並び順は表示のみで、計算値やスコアは不変。
- **SOT強化＋ガードをプロバイダの `system` メッセージで配信**（OpenAI/Ollama は system ロール、Gemini は `system_instruction`、Claude は `system` フィールド）。user ターンへの畳み込みをやめ、全プロバイダ同一。小型ローカルモデルは system メッセージに遥かに強く従う。
- **`/scope wide` はユーザーが挙げたマクロ前提**（例: 夏枯れ、物流の混乱）**に具体的に触れて見立てに結びつける**ようになった（従来は一般知識の使用を許可するだけ）。
- **助言境界の明確化（設計 §6）。** XOKSA は*能動的には*推薦も注文価格の指示もしないが、ユーザーが明示的に問うたとき（「今は仕込みどきか」）にはデータの読みとして方向と転換条件を示す（裸の売買指示はしない）。一般的な推論と、提供された*ニュースタイトル*の利用は明示的に許可（数値・事実・ニュースの捏造は禁止のまま）。
- **macOS バイナリを Developer ID 署名＋公証。** `Developer ID Application`（hardened runtime＋セキュアタイムスタンプ）で署名し、Apple の公証を通して検証済み。検疫属性付きでダウンロードしても Gatekeeper 警告なしで起動する。公開する `xoksa-macos-arm64.zip` のハッシュと、出荷バイナリの VirusTotal スキャンで検証し、出荷検査に記録する。
- **チャット入力欄の既定を2行に（Web UI）。** テキストエリアが2行の高さで開く（従来どおり上限まで自動拡張）ため、通常の質問がスクロールなしで収まる。
- **バージョンを 2.2.4 に設定** — `Cargo.toml`（native + `webui-leptos`）・README・manual / dev 各ドキュメント。

### Fixed（日本語）
- **ニュース（Brave）の HTTP リクエストに timeout を追加。** Brave ニュースクライアントには timeout が無く、Brave エンドポイントやプロキシが停止すると CLI・Web UI の `…/news` 取得が無限に待つ可能性があった。他の送信クライアントと同じ 15秒 の timeout を設定。
- **送信後にチャット入力欄が確実にクリアされる。** 質問を送信すると入力欄が空になり（2行の高さもリセット）、↑/↓ の履歴呼び戻しは従来どおり。

### Security（日本語）
- **macOS：署名＋公証済み。** Gatekeeper が「開発元を検証できません」プロンプトなしでダウンロードを受け入れる。
- **Windows：不変（未署名のまま）。** 初回起動で SmartScreen が警告する場合あり。公開 SHA-256 と VirusTotal 結果で検証すること。Windows のコード署名は引き続き保留。
- **可用性：** 上記 Brave ニュースの timeout により、無制限待ちの経路を解消（`docs/dev-prog/security-design.md` §3）。

## [2.2.3] — 2026-07-09

**Patch release.** A reliable LLM commentary badge on every answer, a fix for the header LLM dropdown flipping to the env-default provider, a new `/date` command, and a lower default news count to cut token cost.

### Added
- **LLM commentary badge on every answer** — each LLM answer is now prefixed with `🧠 <provider>/<model> が解説:` (e.g. `🧠 ollama/gemma4 が解説:`), from a single shared source across the CLI, Web chat, `/basic`, and multi-timeframe. The numbers stay the program's; the badge names who wrote the prose. For one-shot Ollama analysis it also carries the guard state (`guarded: integrity check` / `unguarded`).
- **`/date` command** — prints the current date and time (local, with weekday and UTC offset, plus UTC). Available in the CLI, the Web chat, and the Help catalog / command reference.

### Fixed
- **Header LLM dropdown flipped to the env-default provider (Web UI).** Multiple causes, all addressed: (1) after a `/llm` switch typed in the chat box the picker wasn't refreshed → re-sync after each chat turn (`llm_sync`); (2) the 30s auto-refresh re-fetched the picker while a chat turn held the session lock, so the server fell back to the env default → the picker no longer re-fetches on the 30s poll, and `/api/llm/options` now reports `busy` when the session is mid-turn so the client keeps its current selection instead of adopting env; (3) the `<select>` display didn't follow the data when its options re-rendered → the active option is applied to the element after its options render (node ref + `request_animation_frame`). `/status` and the picker now agree regardless of how the model was switched.

### Changed
- **Default `NEWS_COUNT` lowered from 50 to 30** (news-OFF default). Fewer news items in the analysis context shrinks the prompt and token cost; the max (50) is unchanged and an explicit `NEWS_COUNT` in `xoksa.env` still overrides.
- **Version set to 2.2.3** across `Cargo.toml` (native + `webui-leptos`), README, and the manual / dev docs — a patch on the 2.2 line.

**パッチリリース。** 全回答への解説者バッジ、ヘッダー LLM ドロップダウンが env 既定へ飛ぶ不具合の修正、新コマンド `/date`、そしてトークン節約のためのニュース既定件数の引き下げ。

### Added（日本語）
- **全回答に解説者バッジ** — 各 LLM 回答の先頭に `🧠 <provider>/<model> が解説:`（例 `🧠 ollama/gemma4 が解説:`）を付与。CLI・Web チャット・`/basic`・マルチタイムフレームで単一ソース共有。数値はプログラムの仕事、バッジは文章を書いた主体を明示する。ワンショットの Ollama 分析ではガード状態（`ガード: 整合性チェック` / `ガード無効`）も併記。
- **`/date` コマンド** — 現在の日付と時刻を表示（ローカル：曜日・UTCオフセット付、および UTC）。CLI・Web チャット・ヘルプカタログ / コマンドリファレンスで利用可。

### Fixed（日本語）
- **ヘッダー LLM ドロップダウンが env 既定プロバイダへ飛ぶ（Web UI）。** 原因は複数で、すべて対処：(1) チャット欄の `/llm` 切替後にピッカーが再取得されない → 各チャット完了時に再同期（`llm_sync`）；(2) 30秒自動更新がチャットのセッションロック保持中にピッカーを再取得し、サーバが env 既定へフォールバック → 30秒pollでは再取得しない＋`/api/llm/options` はセッションがターン中なら `busy` を返し、クライアントは現在の選択を維持；(3) options 再描画時に `<select>` 表示がデータに追従しない → options 描画後に選択値を要素へ適用（node ref ＋ `request_animation_frame`）。切替経路に関わらず `/status` とピッカーが一致するようになった。

### Changed（日本語）
- **`NEWS_COUNT` 既定を 50 → 30 に引き下げ**（ニュースOFF時の既定）。分析文脈のニュース件数が減り、プロンプトとトークンコストを縮小。上限（50）は不変で、`xoksa.env` の明示的な `NEWS_COUNT` は従来どおり優先。
- **バージョンを 2.2.3 に設定** — `Cargo.toml`（native + `webui-leptos`）・README・manual / dev 各ドキュメント。2.2 系のパッチ。

## [2.2.2] — 2026-07-06

**First public release.** 2.2.2 is a stateless, no-database design — analysis, backtest, and chart are always computed fresh from the market-data provider. It ships as a **self-contained binary** (the Web UI is embedded), adds **market-data provider failover**, and includes an SOT (source-of-truth) hardening pass with runtime tuning: the analysis serializers were collapsed to one producer each and pinned with tests so the CLI, the REST API, and the Web UI can never drift, and indicator weights and the active-indicator set became runtime-tunable via `/set`. A pre-release security-diagnosis pass (external + whitebox) added defensive HTTP headers, hardened the embedded dashboard against reflected/`postMessage` XSS, and fixed an availability defect where one slow chat turn could stall the summary endpoint (see **Security** / **Fixed**).

### Added
- **`/set` now tunes weights, active indicators, and stance at runtime (session-only).** Editing `xoksa.env` + restarting just to experiment was painful. `/set weight-<indicator> <n>` sets an indicator's score weight; `/set indicator <name> on|off` narrows which indicators the **analysis** (score / display / LLM) uses; `/set stance <buyer|holder|seller>` sets the interpretation stance (gauge orientation + LLM lean, not the composite score). All are temporary session overrides (the env file is never rewritten — that stays the deliberate way to change a *default*). Crucially, `/set indicator off` does **not** change what is computed or stored: `enabled_extensions` (env) still decides the log/CSV column set, so the stored record keeps a stable schema; a deactivated indicator is still computed and stored, just not scored/shown/sent. `/set` lists the params, weights, active-vs-enabled sets, and stance; `/set reset` restores env defaults. Shared CLI/Web via the unified chat dispatch. With these, a strategy recipe needs **zero** env editing — see the reoriented strategy guide.
- **Deterministic SOT gate tests.** The single-source guarantees are enforced by tests that fail the build on divergence: `api_data_equals_cli_json_row` (CLI `--log-format json` == API `data`), `csv_columns_match_json_keys` / `csv_values_match_json_raw` (the CSV log == the JSON record), and `indicator_catalog_matches_evaluator_keys` (the rule-editor vocabulary == the evaluator's keys). Enforcement lives in code, not in memory.
- **Self-contained release binary — the Web UI is embedded.** New `embedded-ui` cargo feature (`include_dir!` over `webui-leptos/dist`): a release build (`cargo build --release --features embedded-ui`) serves the dashboard with no external `dist/` files. `server::read_asset` serves `--web-dir` from disk first (dev / custom builds), then falls back to the embedded assets — so a downloaded binary is fully self-contained while source builds still use `trunk build`.
- **Market-data provider failover (reliability).** Two layers: Yahoo is fetched from `query2.finance.yahoo.com`, and on host failure retries the alternate `query1` host (same data → identical numbers, pure resilience); if Yahoo fully fails, it falls back to **Stooq**, a genuine second vendor (daily-only here — intraday/weekly/monthly `Err` rather than substitute daily bars). A successful Yahoo fetch always wins; a Stooq snapshot is adopted **whole** (never spliced across providers — SOT) and **clearly labelled** with a `⚠️ Fallback data source in use` warning line, since a different vendor's numbers may differ.
- **Release automation.** `.github/workflows/release.yml` builds self-contained binaries for **Windows / macOS / Linux** on a `v*` tag, emits **SHA-256** checksums, and attaches them to the GitHub Release. Optional Windows Authenticode signing runs when a signing-cert secret is configured.

### Changed
- **SOT hardening — one serializer per representation.** The machine-readable analysis record now has a single producer, `output::technical_json_value`, that both the CLI `--log-format json` and the REST API `data` field return byte-for-byte; the CSV log and that JSON share one field definition (`output::analysis_fields`). The market-snapshot line (`chat::ticker::market_identity_price`), the `provider/model` badge (`chat::llm::env_llm_label`), and the LLM SOT reinforcement frame (`llm::frame_with_sot`) were each de-duplicated to a single source. The backtest report, the rule-editor vocabulary/templates, and the period labels moved into the engine (served over `/api/config` + `BacktestResult.report`); the Web UI renders them verbatim instead of re-deriving. The HTTP API integration doc was trimmed to the one read endpoint (`/summary`) with its full JSON response documented.
- **Log / JSON format refinements (technical log, `--log-format json`, API `data`).** As a consequence of unifying the CSV↔JSON serializers: numbers are now **raw f64** in both (no rounding — the CSV drops `{:.2}`/`{:.4}`; scores are no longer cast to int); the redundant JSON alias keys `date`/`close`/`prev_close`/`diff`/`diff_pct` were removed (use `bar_date`/`bar_close`/`previous_bar_close`/`bar_diff`/`bar_diff_pct`); the CSV column `prev_bar_close` was renamed to `previous_bar_close` and gained `datetime`/`timestamp`. These refine formats that are **new in the 2.2.x line** (2.1.0 never had them), so no prior-stable behaviour is broken. An existing accumulated CSV will column-mismatch on `--data-append` — the append is skipped (guarded, not corrupted); start a fresh file.
- **Frontend upgraded to Leptos 0.8.20** (from 0.6) — the last chance to move the WASM dashboard onto the current line before shipping; the E1/E3 XSS fixes below were re-applied on the upgraded tree.
- **Dependency bumps to clear advisories:** `anyhow` 1.0.102 → 1.0.103, `memmap2` 0.9.10 → 0.9.11, `crossbeam-epoch` 0.9.18 → 0.9.20 (**RUSTSEC-2026-0204** — invalid pointer deref in a `fmt::Pointer` impl; a transitive dep via `keyring`, unreachable in XOKSA but cleared; surfaced by re-running the SCA on the settled release binary). (`paste` / `proc-macro2-error2` remain only as build-time proc-macros pulled transitively by Leptos — not shipped in the WASM, not vulnerabilities; removable only upstream.)
- **Version set to 2.2.2** across `Cargo.toml` (native + `webui-leptos`) and the manual / dev docs — a patch on the 2.2 line.

### Removed
- **The opt-in local database (SQLite) was removed entirely.** It stored analysis runs / OHLCV / indicators / fundamentals / news / LLM turns / backtest results, but **nothing read that data back into any computation** — the analysis, backtest, and chart are always computed fresh from the market-data provider (Yahoo). Its only surfaced use was a score-trend History view and saved backtest rules; and a personal tool can't run continuously while the market is open, so the accumulated store was sparse and effectively unused. Removed: the `src/db/` module, the `sqlx` dependency, `migrations/`, the `--db` / `DB_PATH` flag, the `/api/db/*` + `/history` + `/analysis-runs` + `/ohlcv` + `/indicators` endpoints, the DB status/persistence badge, and the 📊 History view. **Kept:** saved backtest rules (the rule editor's save/reuse) now persist to a small JSON file (`~/.xoksa.strategies.json`); `--private` remains a no-trace flag (skips the log file and the strategies file).

### Security
- **Reflected-XSS hardening in the embedded chart popout (E1).** The chart document interpolated the requested `symbol` into HTML; it is now HTML-escaped (`html_escape` also escapes `'`) before interpolation, so a crafted symbol can't inject markup.
- **`postMessage` origin validation (E3).** The dashboard's `window` message listener now rejects any event whose `origin` is not this page's own, closing a cross-origin `postMessage` injection path.
- **`Permissions-Policy` response header** added (defense-in-depth): denies browser features the dashboard never uses (`geolocation`, `camera`, `microphone`, `payment`, `usb`, `accelerometer`, `gyroscope`, `magnetometer`). Complements the existing `Content-Security-Policy`, `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`, and `Referrer-Policy: no-referrer`.

### Fixed
- **Web summary could hang indefinitely under a concurrent chat turn (availability / self-DoS).** The persistent Web chat sessions lived behind a single process-wide mutex that was held across the LLM streaming call, so one slow or stuck `/api/chat/stream` turn froze `/api/symbol/:symbol/summary` (which reads the session's model badge) for **every** symbol — an unauthenticated request could stall the dashboard. The session store is now a per-session lock: the outer map is locked only briefly (never across an await) to look up / insert / evict a session, and each session has its own mutex for the turn; the badge / config reads are non-blocking (`try_lock`, falling back to the env default). A chat turn can no longer stall requests for other sessions.

**初の公開リリース。** 2.2.2 はステートレス（データベースなし）設計——分析・バックテスト・グラフは常に市場データ・プロバイダから新規取得して計算します。**自己完結バイナリ**（Web UI を埋め込み）として配布し、**市場データのプロバイダ・フェイルオーバー**を追加、さらに SOT（信頼できる唯一の情報源）厳格化と実行時チューニングを含みます：分析シリアライザを表現ごとに単一生成へ集約しテストで固定（CLI・REST API・Web UI が食い違わない）、指標の重みと有効指標セットは `/set` で実行時に調整可能に。

### Added（日本語）
- **`/set` で重み・有効指標・スタンスを実行時（セッション限定）に調整できるように。** 試すたびに `xoksa.env` を編集して再起動するのは面倒だった。`/set weight-<指標> <n>` で指標のスコア重み、`/set indicator <名> on|off` で**解析**（スコア/表示/LLM）に使う指標、`/set stance <buyer|holder|seller>` で解釈スタンス（ゲージの向き＋LLMの傾き。合成スコアは変えない）を切り替えられる。いずれも一時的なセッション上書き（envは書き換えない＝**デフォルト**を変えるのは従来どおり手動の意図的操作）。重要な点として `/set indicator off` は**計算・保存を変えない**：`enabled_extensions`（env）が ログ/CSV の列集合を決めるので記録の列は安定、無効化した指標も計算・保存され続け、外れるのは解析だけ。`/set` はパラメータ・重み・「有効/計算対象」の指標セット・スタンスを一覧表示、`/set reset` で env 既定に復帰。CLI/Web は統一ディスパッチで共通。これにより戦略レシピは env 編集が**一切不要**に（再構成した戦略ガイド参照）。
- **決定論的なSOT関門テスト。** 単一ソース保証を、発散でビルドを落とすテストで強制：`api_data_equals_cli_json_row`（CLI `--log-format json` == API `data`）、`csv_columns_match_json_keys` / `csv_values_match_json_raw`（CSVログ == JSONレコード）、`indicator_catalog_matches_evaluator_keys`（ルール語彙 == 評価器キー）。強制は記憶ではなくコードに置く。
- **自己完結のリリースバイナリ — Web UI をバイナリに埋め込み。** 新 `embedded-ui` フィーチャ（`webui-leptos/dist` を `include_dir!`）：リリースビルド（`cargo build --release --features embedded-ui`）は外部 `dist/` 無しでダッシュボードを配信。`server::read_asset` は `--web-dir`（ディスク＝開発/カスタム）を優先し、無ければ埋め込みアセットにフォールバック。DL したバイナリは完全自己完結、ソースビルドは従来どおり `trunk build`。
- **市場データのプロバイダ・フェイルオーバー（信頼性）。** 2層：Yahoo は `query2.finance.yahoo.com` から取得し、ホスト障害時は代替ホスト `query1` を再試行（同一データ＝数値不変・純粋な冗長化）。Yahoo が完全に落ちた場合は **Stooq**（真の第2ベンダー。ここでは日足のみ＝分足/週足/月足は代替せず `Err`）へフォールバック。Yahoo 成功時は必ず優先。Stooq のスナップショットは**丸ごと**採用（プロバイダをまたいで splice しない＝SOT）し、別ベンダーゆえ数値が異なりうるため `⚠️ 代替データ元を使用` の警告1行で**明示**。
- **リリース自動化。** `.github/workflows/release.yml` が `v*` タグで **Windows / macOS / Linux** の自己完結バイナリをビルドし、**SHA-256** を生成して GitHub Release に添付。Windows の Authenticode 署名は署名証明書シークレット設定時のみ発火。

### Changed（日本語）
- **SOT厳格化 — 表現ごとにシリアライザは1つ。** 機械可読の分析レコードは唯一の生成関数 `output::technical_json_value` を持ち、CLI `--log-format json` と REST API `data` がバイト単位で同一を返す。CSVログとJSONは単一のフィールド定義 `output::analysis_fields` を共有。市況行（`chat::ticker::market_identity_price`）・`provider/model` バッジ（`chat::llm::env_llm_label`）・LLMのSOT補強フレーム（`llm::frame_with_sot`）も各1ソースに集約。バックテストのレポート・ルール語彙/テンプレ・期間ラベルは engine 側に移し（`/api/config` ＋ `BacktestResult.report` で配信）、Web UI はそれをそのまま表示。HTTP API連携ドキュメントは読み取り1エンドポイント（`/summary`）に絞り、JSONレスポンスを完全記載。
- **ログ/JSON形式の調整（テクニカルログ・`--log-format json`・API `data`）。** CSV↔JSONのシリアライザ統一に伴い：数値は両方とも**生f64**（丸めなし＝CSVの `{:.2}`/`{:.4}` を廃止、スコアのint化も廃止）。JSONの重複別名キー `date`/`close`/`prev_close`/`diff`/`diff_pct` を削除（`bar_date`/`bar_close`/`previous_bar_close`/`bar_diff`/`bar_diff_pct` を使用）。CSV列 `prev_bar_close` を `previous_bar_close` に改名し `datetime`/`timestamp` を追加。いずれも **2.2.x 系で新規の**形式のみの変更（2.1.0 には存在しない）＝従来安定版の挙動は壊れない。既存の蓄積CSVは `--data-append` で列不一致となり追記はスキップ（破損しない）＝新規ファイルで開始。
- **フロントエンドを Leptos 0.8.20 へ更新**（0.6 から）— 配布前に WASM ダッシュボードを現行系へ移す最後の機会。下記 E1/E3 の XSS 修正は更新後のツリーに再適用。
- **アドバイザリ解消の依存更新:** `anyhow` 1.0.102 → 1.0.103、`memmap2` 0.9.10 → 0.9.11、`crossbeam-epoch` 0.9.18 → 0.9.20（**RUSTSEC-2026-0204** — `fmt::Pointer` impl の無効ポインタ参照。`keyring` 経由の推移依存で XOKSA では到達不能だが解消。確定リリースバイナリで SCA を測り直して判明）。（`paste` / `proc-macro2-error2` は Leptos が推移的に引くビルド時 proc-macro としてのみ残存＝WASM には同梱されず・脆弱性でもなく・除去は上流依存。）
- **バージョンを 2.2.2 に設定** — `Cargo.toml`（native + `webui-leptos`）および manual / dev 各ドキュメント。2.2 系のパッチ。

### Removed（日本語）
- **オプトインのローカルDB（SQLite）を完全撤去。** 分析run・OHLCV・指標・ファンダ・ニュース・LLM発話・バックテスト結果を保存していたが、**そのデータを計算に読み戻すロジックは存在せず**、分析・バックテスト・グラフは常に市場データ・プロバイダ（Yahoo）から新規取得して算出していた。表に出ていた用途はスコア推移の履歴ビューと保存戦略だけで、個人ツールは市場が開いている間ずっと起動できないため記録は歯抜け＝実質未使用だった。撤去：`src/db/` モジュール・`sqlx` 依存・`migrations/`・`--db` / `DB_PATH` フラグ・`/api/db/*`＋`/history`＋`/analysis-runs`＋`/ohlcv`＋`/indicators` エンドポイント・DBステータス/永続化バッジ・📊履歴ビュー。**存続：** 保存戦略（ルールエディタの保存/再利用）は小さな JSON ファイル（`~/.xoksa.strategies.json`）に永続化。`--private` は痕跡なしフラグとして存続（ログとルールファイルを書かない）。

### Security（日本語）
- **埋め込みチャート・ポップアウトの反射XSS対策（E1）。** チャート文書が要求された `symbol` を HTML に埋め込んでいたのを、埋め込み前に HTML エスケープ（`html_escape` は `'` もエスケープ）するよう修正。細工したシンボルでマークアップを注入できない。
- **`postMessage` のオリジン検証（E3）。** ダッシュボードの `window` メッセージ・リスナが、`origin` が自ページ自身でないイベントを拒否するように。クロスオリジンの `postMessage` 注入経路を封鎖。
- **`Permissions-Policy` レスポンスヘッダを追加**（多層防御）：ダッシュボードが使わないブラウザ機能（`geolocation`・`camera`・`microphone`・`payment`・`usb`・`accelerometer`・`gyroscope`・`magnetometer`）を拒否。既存の `Content-Security-Policy`・`X-Frame-Options: DENY`・`X-Content-Type-Options: nosniff`・`Referrer-Policy: no-referrer` を補完。

### Fixed（日本語）
- **並行チャットターン中に Web の summary が無限にハングしうる不具合（可用性 / 自己DoS）。** 永続 Web チャットセッションがプロセス全体で単一の Mutex に載っており、それを LLM ストリーミング呼び出しの間ずっと保持していたため、遅い/詰まった `/api/chat/stream` ターン1本が `/api/symbol/:symbol/summary`（セッションのモデルバッジを読む）を**全シンボルで**凍結させた——未認証の1リクエストでダッシュボードを止められた。セッションストアを per-session ロックに変更：外側マップは get/insert/evict の一瞬だけ（await 跨ぎ無し）ロックし、各セッションは自前の Mutex でターンを保持。バッジ/コンフィグ読み取りは非ブロッキング（`try_lock`、失敗時は env 既定へフォールバック）。チャットターンが他セッションのリクエストを止めることはなくなった。

---

## [2.2.0] — 2026-07-04

Start of the local DB / analysis-history / multi-timeframe / backtest line. Design is fixed (sqlx + bundled SQLite, `AnalysisMode` timeframe tokens, OHLCV as the market-data source of truth with indicators recomputed via the engine, UTC ISO8601 timestamps, opt-in DB, daily-first backtest); built in small phases on `release/v2.2`.

### Added
- **Backtest — usable overhaul (rule editor, honest benchmark, realistic sizing).** The skeleton grew into a real feature. A **rule editor** (🧪 panel) lets you build buy/sell conditions (indicator → operator → value/indicator, AND/OR, add/remove rows, crossovers), with **templates and save/reuse** shown in one list (built-ins carry a `_template` suffix; user rules save to `saved_strategies`). Every result now prints the **actual rule** (name + conditions) in plain language, always alongside a **Buy & Hold benchmark** so a weak rule can't be mistaken for the stock's own move. **Scale-aware period picker**: choosing a timeframe shows/limits the periods the provider actually allows (no manual/help lookup). **Money model made realistic**: the amount is **cash** (not shares) in the **instrument's own currency** (from the provider's `meta.currency`, e.g. JPY/USD — never guessed); trades in **whole minimum lots** (100 for `.T`, 1 otherwise — a documented assumption) with the leftover kept as cash; sizing is user-set — **buy a start fraction up front (default 50%)** and **add/trim a per-signal cash amount** — and the report shows the assumptions, a per-trade cash ledger, and warnings (cash can't afford a lot; sell signal with nothing held). Returns/`%` are the headline (currency-independent; no FX conversion).
- **`xoksa serve --log-format json` — structured (NDJSON) logs for the REST server.** The coded logger can now emit one JSON object per line (`{"ts","level","code","msg"}`) instead of the human `⚠️ [CODE] msg` text, for both the console and the file. Default stays `text`; `--log-format json` selects JSON (dependency-free encoder, minimal spec-correct escaping).
- **Chart interval → attach as reference to your comment (brush-select).** On the chart popup, **drag to select an interval**; a selection rectangle spans the panels, and on release the interval is **attached as reference** (a removable 📎 chip above the chat input) — it does **not** auto-explain. You then write your own comment/question and send; the interval's **confirmed** values (built client-side from the already-computed chart series — same SOT numbers, no fabrication) are prepended as data: an aggregate line (price start→end/%/high-low, RSI start→end+range, MACD start→end, volume avg/peak) **plus a per-bar table** (each bar's `time: close / volume / RSI / MACD`, downsampled only for very long intervals). No instruction/bias is injected — just the data + your message, so the LLM answers *your* question. The AI never picks the interval or reads an image. Implemented by attaching mouse listeners from the main WASM window onto the popup's `<svg>` (same-origin), mapping x→bar→time.
- **Structured logging (v1) — coded, deduplicated, file-backed.** New dependency-free `src/logging.rs`: `logging::warn/error/info(code, msg)` emit a **stable error code** (e.g. `XK-IND-EVAL`), print WARN/ERROR to the console **once per unique (code, msg)** so repeats don't flood, and append the full record to a local log file (`~/.xoksa.log`, 0600, truncated past ~5 MB). Skipped entirely in private mode (no-trace). Security: credentials (Class A) must never be passed as code/msg. First adopter: the extended-indicator-evaluation failure (the warning that previously spammed). Migrating the remaining ad-hoc `eprintln!` notices (ollama integrity, etc.) to coded entries is incremental follow-up. (Principle: a warning is a notification that something should be improved — coded and recorded, not printed-and-forgotten.)
- **Backtest — Phase 5b (skeleton).** New `src/backtest.rs` + `POST /api/backtest` run a minimal backtest: bars come from the **provider as one contiguous history fetch** (never the sparse local DB — the DB is not assumed to mirror the market), and the strategy reuses the **live SOT score** (long-only: enter when the composite score ≥ `entry_score`, exit when ≤ `exit_score`). Indicators at each bar are recomputed by the same engine over the bar prefix, and fills are at the signal bar's close (the bar is closed → look-ahead-free; fees not modelled yet). Returns total return %, trade count, win rate, max drawdown, equity, and the trade list; saves `backtest_runs` + `backtest_trades` when a DB is configured. UI: a header **🧪 BT** button runs a preset (current timeframe, last 250 bars) for the active symbol and appends the summary to the chat. Bar source and look-ahead discipline follow the agreed sparse-DB design.
- **Personal data — Phase 5a-2 (chat commands + read-only UI panel).** Entry is via SOT chat commands shared by CLI and Web (unified `execute` dispatch): `/cash <amount> [currency]`, `/buy <symbol> <qty> <price> [fee]`, `/sell <symbol> <qty> <price> [fee] [tax]`, and `/portfolio` (alias `/assets`). These are slash commands handled before any model turn, so the confirmation is shown to the user but the personal data **never enters the LLM context** (Class D). A header **💼 Portfolio** button opens a read-only popup rendering `GET /api/portfolio` (cash, positions with avg/now/value/P&L, total assets, and an "unvalued" note when a position has no stored bar). Help (`/help`) lists the new commands under a "Portfolio / personal data" section. All require a DB (`--db` / `DB_PATH`).
- **Personal data — Phase 5a (Class D cash/positions/trades/portfolio).** New `src/db/portfolio.rs` and `/api/portfolio*` endpoints record and read the user's cash, trades, positions, and portfolio snapshots. `POST /api/portfolio/cash` sets the balance; `POST /api/portfolio/trade` appends a BUY/SELL to the ledger and, in one transaction, updates the position (qty/average, re-averaged on BUY incl. fee) and **auto-adjusts cash** (BUY −(qty·price+fee), SELL +(qty·price−fee−tax)); `GET /api/portfolio` returns cash + positions + partial totals; `GET /api/portfolio/trades`; `POST /api/portfolio/snapshot` computes and stores a `portfolio_snapshots` row. **Security (Class D):** this data is stored only in the local DB, never logged (write handlers return only a boolean ack), and never sent to the LLM. **Sparse-DB honesty:** a position is valued from the latest *stored* OHLCV bar and left blank when none is stored (the local DB is not assumed to mirror the market); `unvalued_positions` is reported so partial totals aren't mistaken for complete. UI and chat-command entry are a later sub-phase; the backtest skeleton is Phase 5b.
- **Multi-timeframe analysis — Phase 4b (LLM interpretation + UI preset).** `POST /api/analysis/multi-timeframe` builds the Phase-4a context pack and has the LLM interpret it: the pack is the confirmed data (SOT), prefixed with the same chat guard constraint (no invented numbers/news; forecasts labeled). Returns the pack text + the model's interpretation, and saves both the context (`analysis_contexts`) and the turn (`llm_outputs`, role `multi_timeframe`) when a DB is configured. UI: a header **📐 MTF** button POSTs a fixed preset (monthly MACD + daily Bollinger + 5m price) for the active symbol and appends the pack + interpretation to the chat. `ContextSpec` gained a `lang` field so the pack/answer match the browser.
- **Multi-timeframe analysis — Phase 4a (ContextSpec + ContextPackBuilder).** A new `src/context.rs` builds a multi-timeframe context for the LLM: a `ContextSpec` lists `{type, timeframe, indicator_name}` items (e.g. monthly MACD + daily Bollinger + 5m price); each requested timeframe is analyzed once through the shared engine (`build_analyzed_guard`, with exactly the requested indicators enabled), and the result is a **Rust-built summary** (`context_pack_text`) plus structured `context_pack_json` — never raw bulk data (SOT + token control). New endpoint `POST /api/analysis/context-pack`; the pack is saved to `analysis_contexts` when a DB is configured. Timeframes are normalized through `AnalysisMode`. Phase 4a builds the "now" context (latest data per timeframe); a historical `as_of` is recorded but honored later with the backtest data feed. The LLM-interpretation endpoint (`/multi-timeframe`) and a UI preset are Phase 4b.
- **DB layer — Phase 3 (symbol-history read APIs + minimal UI).** Read side of the DB (`src/db/read.rs`, parameterized, newest-first): `GET /api/symbol/{symbol}/analysis-runs` (score trend), `/ohlcv?timeframe=`, `/indicators?timeframe=`, and a convenience `/history` bundle (recent runs + news). All require a configured DB and return empty when none is connected; `timeframe` is normalized through `AnalysisMode` and `limit` is clamped (≤1000). Minimal UI: a header **📊 History** button opens a separate popup that fetches `/history` for the active symbol and renders the analysis-run score trend (green/red) plus recent news. The popup opens synchronously (no popup-blocker issue) then fills after the fetch.
- **DB layer — Phase 2 (persist analysis).** When a DB is configured, every analysis (CLI and Web — the persistence hook lives in the shared `build_analyzed_guard`, so it happens once for both) now writes: the `symbols` row (upsert), the full `ohlcv` bar series for the timeframe (idempotent via `UNIQUE(symbol, timeframe, timestamp)`; `is_complete = 1` for closed bars; `open` is NULL — the data source provides high/low/close/volume only), and an `analysis_runs` snapshot (the engine's composite score), deduped per analyzed bar via `UNIQUE(symbol, timeframe, market_data_timestamp)` so the 30-second Web auto-refresh poll does not pile up runs. Timestamps are stored UTC ISO 8601. Writes go through `db::store` (parameterized queries) and are **best-effort** — a DB failure logs a warning and never blocks the analysis. **Phase 2b** adds the remaining writers: per-indicator `technical_indicators` snapshots (one row per indicator at the analyzed bar, deduped by `(symbol, timeframe, timestamp, indicator_name, params_hash)` where `params_hash` is a stable signature of the indicator periods), `fundamentals` (upsert per symbol+period+report-date, hooked into `/summary`), `news_items` (title/URL only, deduped by `(symbol, url)`, hooked into `/news`), and `llm_outputs` (free-text chat turns: prompt + response + token count). Verified idempotent — repeating an identical analysis writes nothing new.
- **DB layer — Phase 1 (opt-in SQLite via `sqlx`).** `xoksa serve --db <path>` (or `DB_PATH`) connects a local SQLite database, applies the schema, and reports status; with neither set the server runs exactly as before (no DB). New module `src/db/` is the single SQL seam (future-Postgres-friendly): `DbStore` (pooled handle), opt-in process-global, `resolve_db_path`. Initial schema (`migrations/0001_init.sql`) creates all agreed tables — Market (symbols, ohlcv, technical_indicators, fundamentals, news_items), Analysis (analysis_runs, llm_outputs, analysis_contexts, symbol_history_summaries), and Personal/Backtest skeletons (cash_balances, positions, trades, portfolio_snapshots, backtest_runs, backtest_trades) — with UTC-ISO8601 timestamps, `AnalysisMode` timeframe tokens, and `is_complete` for look-ahead-free backtests. New endpoints `GET /api/db/status` and `POST /api/db/init`; the header **DB badge turns green when connected** (`meta.db_status` reflects the real state). SQLite is **bundled** (no system lib → still a self-contained binary). Supply chain: `sqlx`'s `macros`/`migrate` features were intentionally avoided (they pull every driver incl. `sqlx-mysql` → the vulnerable `rsa`); the schema is embedded with `include_str!` + `raw_sql`. `cargo audit` reports no vulnerabilities.
- **Version bumped to 2.2.0** across `Cargo.toml` (native + `webui-leptos`), `README.md`, and the manual/dev docs.

### Changed
- **Chat input is now a multi-line textarea (LLM-app style).** The single-line `<input>` became an auto-growing `<textarea>`: **Enter sends, Shift+Enter inserts a newline**, and the box grows with the content up to a cap then scrolls. Command-history recall (↑/↓) still works but only at the caret boundaries (top/bottom), so moving between lines isn't hijacked. Matches how mainstream LLM chat UIs behave.
  **チャット入力を複数行テキストエリアに（LLMアプリ標準）。** 単一行 `<input>` を自動拡張する `<textarea>` に変更：**Enterで送信・Shift+Enterで改行**、内容に応じて高さが伸び上限でスクロール。コマンド履歴（↑/↓）はカーソルが端（先頭/末尾）の時のみ動作し、行移動を妨げない。主要なLLMチャットUIと同じ挙動。
- **`/run` chat command renamed to `/basic` (基本分析); `/run` kept as a deprecated alias.** The command name now matches the "基本分析 / Basic" label. `/basic` is canonical; `/run` still works but prints a one-line migration notice (`/run → /basic`). Help lists `basic`; `/prompt`'s rename hint now points at `/basic`; the header menu issues `/basic`.
  **`/run` チャットコマンドを `/basic`（基本分析）にリネーム。`/run` は非推奨エイリアスとして存続。** コマンド名を「基本分析 / Basic」ラベルに一致させた。`/basic` が正式、`/run` も当面使えるが移行ヒント（`/run → /basic`）を1行表示。help は `basic` を掲載、`/prompt` の移行先も `/basic` に更新、ヘッダーメニューは `/basic` を発行。
- **Analysis actions grouped into one "Analyze" header dropdown.** 🧠 Basic (`/run`), 📐 Multi-timeframe, 🧪 Backtest, and 📊 History are now a single **Analyze** dropdown instead of four separate buttons, to keep the header uncluttered. Picking an item runs it (same handlers/dispatch as before) and the menu resets to its label. The `/run` item is labeled **基本分析 / Basic** (not "分析") so it doesn't collide with the menu name. "分析" is used (over "解析") for consistency with the rest of the UI. 💼 Portfolio (Class D) stays a separate control.
  **分析アクションを1つの「分析」ヘッダードロップダウンに集約。** 🧠 基本分析（`/run`）・📐 マルチタイムフレーム・🧪 バックテスト・📊 履歴 を、4つの個別ボタンから単一の「分析」ドロップダウンに統合（ヘッダーの混雑を解消）。選ぶと実行され（ハンドラ/ディスパッチは従来同一）、メニューはラベルに戻る。`/run` 項目はメニュー名と衝突しないよう **基本分析 / Basic**（「分析」ではなく）と命名。表記は既存UIとの一貫性から「解析」ではなく「分析」に統一。💼 資産（クラスD）は別カテゴリのため単独のまま。
- **`/run` is now a header button (🧠 分析 / Analyze).** Running the LLM analysis of the current symbol was only reachable by typing `/run` in chat. A header button now triggers it — it sends `/run` through the exact same chat-stream dispatch (SOT), so the analysis streams into the chat panel just as if typed. Placed alongside the other mouse actions (📐 MTF, 🧪 BT), matching the principle that primary actions should be mouse-operable, not command-only.
  **`/run` をヘッダーボタン化（🧠 分析）。** 現在銘柄のLLM分析はチャットで `/run` と打つ以外に導線がなかった。ヘッダーのボタンで実行できるようにし、内部は同じチャットストリーム・ディスパッチ（SOT）を通すのでチャット欄に分析がストリーム表示される。他のマウス操作（📐 MTF・🧪 BT）と並べ、主要アクションはコマンド専用でなくマウスでも行える、という方針に合わせた。
- **LLM is now a mouse-selectable header dropdown (was chat-command only).** The read-only `model:` badge is replaced by a `<select>` next to the timeframe picker, so choosing the commentary LLM is a mouse action like every other header control — fixing the inconsistency where the timeframe was selectable but the model was display-only. **No free-text model entry**: the dropdown is built from the known available set only — cloud providers that have an API key (with their default model) + live ollama instances — via `GET /api/llm/options`. Selecting one routes through the unified `/llm` dispatch (`POST /api/llm/select`, single code path with the chat command), so it persists per session and applies to chat, the dashboard badge, and multi-timeframe alike; the chat panel is not cluttered (the switch runs with a discard sink). The chat `/llm` command still works for power users.
  **LLM選択をヘッダーのマウス用ドロップダウンに（従来はチャットコマンドのみ）。** 読み取り専用の `model:` バッジを足セレクタ隣の `<select>` に置換し、解説LLMの選択も他のヘッダー操作と同じくマウスで行えるように（足は選べるのにモデルは表示のみ、という不整合を解消）。**キーボードでのモデル文字列入力は不可**＝ドロップダウンは既知の利用可能セット（APIキーのあるクラウド＋既定モデル、live な ollama インスタンス）だけを `GET /api/llm/options` から構築。選択は統一 `/llm` ディスパッチ（`POST /api/llm/select`、チャットコマンドと同一コード）を通すのでセッションに永続し、チャット・ダッシュボードのバッジ・マルチタイムフレームに一貫適用（破棄シンクで実行しチャット欄は汚さない）。チャットの `/llm` コマンドは上級者向けに維持。
- **Multi-timeframe result now attributes the LLM commentary to the model.** The context pack (【月足 MACD】…) is the deterministic data; the prose after it is the LLM's interpretation, so it is now prefixed with `🧠 <provider/model> が解説:`. This is the deliberate mirror of removing the model from the analysis-run history — the model is shown exactly where it does the work (commentary) and hidden where it does not (the deterministic score). `/api/analysis/multi-timeframe` gained a `model` field; the attribution appears only when the LLM actually ran.
  **マルチタイムフレームの結果で、LLMの解説にモデル名を明示。** context pack（【月足 MACD】…）は確定データ、その後の文章はLLMの解釈なので、冒頭に `🧠 <provider/model> が解説:` を付与。これは分析履歴からモデルを外したことの**意図的な対**で、モデルは「実際に仕事をする場所（解説）」にだけ表示し、「しない場所（確定スコア）」では出さない。`/api/analysis/multi-timeframe` に `model` を追加し、LLMが実行された時のみ表示。
- **Saving is on by default; "DB" removed from the user-facing UI.** Persistence is not a capability the user has to enable — SQLite is bundled, so the only real question is whether to save. The server now saves to a local SQLite store **by default** (`~/.xoksa.db`, or `--db <path>` / `DB_PATH` to override). A new **`--private`** flag turns saving off entirely for a no-trace session. The header no longer shows a `DB:` badge (DB is an internal mechanism, not a connection the user makes); instead it shows nothing while saving, **🔒 プライベート** in private mode, or **⚠️ 保存停止** if the store can't be opened. The classic terminal CLI is unchanged — persistence only initializes in `serve` mode. Chat-command/manual wording moved from "needs a DB" to "saved unless in private mode". ("DB" terminology is reserved for a future external-DBMS connection, which does not exist yet; docs describe the mechanism as local SQLite.)
  **保存を既定で有効化し、UIから「DB」表記を排除。** SQLiteをバンドルしている以上ユーザーが「DBを使うか」を選ぶ意味はなく、論点は「保存するか否か」だけ。サーバは既定でローカルSQLite（`~/.xoksa.db`、または `--db`/`DB_PATH` で上書き）に**保存**するようになりました。新フラグ **`--private`** で全保存を無効化（痕跡を残さない実行）。ヘッダーの `DB:` バッジは廃止（DBは内部の仕組みでありユーザーが行う接続ではない）。代わりに、保存中は何も表示せず、プライベートモードは **🔒 プライベート**、保存先を開けない時は **⚠️ 保存停止** を表示。従来のターミナルCLIは不変（保存は `serve` モードでのみ初期化）。チャット/マニュアルの文言も「DBが必要」→「プライベートモードでなければ保存」に変更。（「DB」という語は将来の外部DBMS接続を作る時のために留保。現状はローカルSQLite利用と記述。）
- **Market chart is now driven by the market data, not polling samples.** The chart popup previously accumulated one point per 30-second auto-reload, so its point count grew over time and did not match the actual bars. It now fetches a per-bar series from the new `GET /api/symbol/{symbol}/chart?timeframe=&bars=N` endpoint and plots that: the **actual market-data bars** (close + volume) plus the engine's indicators (VWAP/EMA/SMA/RSI/MACD) computed **at each bar** by re-running the same engine over the bar prefix (SOT — identical to the analysis). The x-axis is the real bar times and the point count equals the bar count (default last 120). While the chart popup is open it refetches on each auto-reload; the client-side point accumulation (and the market-line parsing that fed it) is removed. **Bollinger bands** (upper/lower envelope) are now available as a toggleable price overlay (**BB** checkbox), computed per bar like the other indicators.
  **市況チャートを「ポーリング蓄積」から「市況データ（実際の足）」ベースに変更。** 従来は30秒の自動更新ごとに1点ずつ蓄積し、点数が増え続けて実際の足と一致しなかった。新エンドポイント `GET /api/symbol/{symbol}/chart?timeframe=&bars=N` から**足ごとの系列**を取得して描画：**実際の足**（終値＋出来高）＋各足の指標（VWAP/EMA/SMA/RSI/MACD）を、同一エンジンを足の前方系列に対して再実行して算出（SOT＝分析と同一値）。横軸は実際の足時刻、点数＝足数（既定は直近120）。ポップアップ表示中は自動更新ごとに再取得。クライアント側の点蓄積（および市況ラインの解析）は撤去。**ボリンジャーバンド**（上下バンド＝レンジ）を価格オーバーレイとして追加（**BB** チェックボックスで表示切替）。他の指標と同様に足ごとに算出。

### Fixed
- **Chart popup now scales to fill the window (was fixed 860px, top-left).** The chart SVG had a hardcoded pixel size, so a large window left most of it empty. The popup is now a flex column (header/checkboxes/footer fixed, the SVG fills the rest) and the chart scales to fit via `viewBox` + `preserveAspectRatio` — as big as possible, centered, no distortion, no scroll. The drag-select brush maps mouse coordinates through the SVG screen-CTM so selection stays exact under any scaling.
  **市況チャートのポップアップがウィンドウに追従（従来は幅860px固定で左上に小さく表示）。** SVGがピクセル固定サイズだったため、ウィンドウを広げても大半が空いていた。ポップアップをflexレイアウト（見出し/チェックボックス/脚注は固定・SVGが残り全域）にし、`viewBox`＋`preserveAspectRatio` でウィンドウに合わせて最大化（中央寄せ・歪みなし・はみ出しスクロールなし）。区間選択ドラッグはSVGのscreen-CTMで座標を逆変換し、拡大しても選択位置が正確。
- **Backtest panel: dropdowns now reflect their selected value (Leptos `prop:value` bug).** Selecting a template/loading a rule left the condition dropdowns showing their first option (e.g. a "score ≥2" row showed "終値 >"), and pressing Load changed nothing on screen — causing anxiety about whether settings applied. All backtest-panel selects switched from `prop:value` to `selected` on each `<option>`, so the shown option always matches the state and loading a rule visibly repopulates the editor.
  **バックテストパネル：ドロップダウンが選択値を反映（Leptos `prop:value` の不具合）。** テンプレ選択/ルール読込をしても条件のドロップダウンが先頭項目のまま（例：「総合スコア≧2」の行が「終値 >」表示）で、読込を押しても画面が変わらず、設定が効いたか不安になっていた。パネルの全セレクトを `prop:value` から各 `<option>` の `selected` に変更し、表示が常に状態と一致・読込で条件が目に見えて反映されるようにした。
- **Market chart: hover shows the bar's time; double-click no longer selects axis text.** (1) The hover tooltip now reads `<time> — <name>: <value>` (e.g. `2026-06-30 14:35 — Price: 141.20`), so you can read *when* a sharp move happened — previously only the value was shown. (2) The chart SVG is now `user-select: none`, so double-clicking a line no longer highlights an axis number (which looked like a meaningless "flatten"/edit and was misleading).
  **市況チャート：ホバーに足の時刻を表示、ダブルクリックでの軸テキスト選択を無効化。** (1) ホバーのツールチップが `<時刻> — <名前>: <値>`（例 `2026-06-30 14:35 — Price: 141.20`）になり、急変が**いつ**起きたか読めるように（従来は値のみ）。(2) チャートSVGを `user-select: none` にし、線をダブルクリックしても軸の数値が選択されない（無意味な「平坦化/編集」に見えるミスリードを解消）。
- **Web UI: the change line (前足比) is colored again.** Disabling the engine's ANSI colors server-side (so raw escape codes never reach the browser) also left the price-change line plain. The dashboard now re-colors it with CSS instead: in the basic-data / fundamental panels, a signed change delta — a line ending in `%)` after a `: `, e.g. `📊 前足比: +0.06 (+0.03%)` — has its value wrapped in a green/red span (`.analysis .pos/.neg`). Text is HTML-escaped first; this is the Web equivalent of the CLI's ANSI colors (the values themselves remain the single source of truth).
- **Multi-timeframe analysis ignored a chat `/llm` switch.** After switching the model in chat (e.g. `/llm gpu1`), the 📐 MTF commentary was still produced by the env-default model (it built a fresh config per request instead of using the persisted chat session). The endpoint now starts from the persisted Web chat session's config (`exec::web_session_config`, keyed by the header timeframe sent with the request, falling back to any session for the symbol), so the LLM commentary uses the same provider/model the switch selected. Verified: `/llm gemini` → MTF then reports `gemini/gemini-2.5-flash` (was `openai/…`).
- **Analysis history no longer shows a "model" column (it was misleading).** Each run's score is computed by **deterministic Rust (SOT)**; the LLM is only a commentator and does not produce the score. Showing the active model next to each per-bar score implied that model had performed the evaluation. The model column is removed from the 📊 history view, and `model_name` is removed from `analysis_runs` entirely (schema + writer + reader) — the model is recorded only with the LLM turn it actually belongs to (`llm_outputs`).
  **分析履歴の「モデル」列を廃止（ミスリードだったため）。** 各足のスコアは**確定的なRustコード（SOT）**が集計しており、LLMは解説役でスコアを作りません。各スコアの隣に選択中モデルを出すと、そのモデルが評価したように見えてしまう。📊 履歴からモデル列を削除し、`analysis_runs` からも `model_name` を完全に除去（スキーマ＋書き込み＋読み出し）。モデルは本来属する場所＝LLMの発話記録（`llm_outputs`）にのみ残します。
- **DB wrote a new analysis run + indicator rows every minute (all timeframes).** `save_analysis` keyed `analysis_runs` and `technical_indicators` on `market_data_latest_timestamp` — the latest observation, which advances every minute inside a forming bar — so polling persisted fresh rows once a minute even for daily data (OHLCV was already deduped by the bar timestamp, so only these two were affected). They now key on the confirmed analysis-bar timestamp (`get_bar_timestamp`), so exactly one run + one set of indicator rows is written per bar, for every timeframe (5m/15m/1h/daily). Verified: two daily analyses in the same bar → 1 `analysis_runs` row.
  **DBが全足で毎分「分析実行＋指標」行を書き込んでいた不具合を修正。** `save_analysis` が `analysis_runs`・`technical_indicators` の重複判定を `market_data_latest_timestamp`（＝形成中の足の中で毎分進む最新観測時刻）でキーしていたため、日足でもポーリングのたび（毎分）に新しい行が増えていた（OHLCV は足タイムスタンプでdedup済みのため影響なし＝この2つだけの問題）。確定足のタイムスタンプ（`get_bar_timestamp`）でキーするよう変更し、**全足（5分/15分/1時間/日足）で足ごとに1実行・1指標セット**だけ書き込むようにした。検証：同一足での日足分析2回 → `analysis_runs` は1行。
- **市況データ log appended a row every minute in intraday modes.** In e.g. 15-minute mode the market-data log grew once per minute instead of once per bar, because it was keyed on `market_data_latest_time` — the latest observation, which ticks every minute within a forming bar. The summary now also returns `bar_time` (the confirmed analysis-bar boundary, e.g. 12:30, which only advances when a new bar forms), and the log keys on that, so exactly one row is appended per bar.
  **市況データのログがザラ場で毎分1行増えていた不具合を修正。** 例えば15分足で、足ごとではなく毎分1行ずつ増えていた。原因はログの重複判定キーが `market_data_latest_time`（＝最新観測時刻。形成中の足の中で毎分進む）だったこと。サマリに `bar_time`（確定足の境界＝例 12:30。新しい足が形成された時だけ進む）を追加し、これをキーにしたので、足ごとに1行だけ追記される。
- **Market chart popup: three readability fixes.** (1) Volume bars no longer overflow their frame — each panel now has its own SVG clip path, and the chart renders at a fixed pixel size (was `width="100%"` with no height, which let the aspect-scaled chart spill past the popup). (2) Removed the duplicate panel label — every panel drew both a standalone title and a legend whose first entry repeated it (`Price Price`, `RSI RSI`, `MACD MACD`, `Volume Vol`); panels are now labeled by the legend only. (3) Hovering a series shows its value — line points carry a small dot with a `<title>` tooltip and volume bars carry a `<title>`, so pointing at any point/bar reveals `name: value` (native SVG tooltip, no scripting).
  **市況チャート（ポップアップ）の見やすさを3点修正。** (1) ボリュームの棒がフレームを超える問題を解消（各パネルにSVGクリップを設定＋チャートを固定ピクセルサイズで描画。従来は `width="100%"`・高さ未指定でアスペクト拡大されポップアップをはみ出していた）。(2) パネル名の二重表示を解消（各パネルが「タイトル＋凡例」を両方描画し先頭が重複＝`Price Price`／`RSI RSI`／`MACD MACD`／`Volume Vol`）。今後は凡例のみで表示。(3) 系列にカーソルを重ねると数値を表示（折れ線は各点に小さなドット＋`<title>`、ボリュームは棒に`<title>`を付与し、`名前: 値` をネイティブのSVGツールチップで表示＝スクリプト不要）。
- **Web UI: raw ANSI escape codes in analysis text.** The engine colors terminal output via the `colored` crate, and when `serve` ran from a terminal those escape codes were embedded in the JSON the browser renders (e.g. the price-change line showed `←[31m-0.10 (-0.07%)←[0m`). The server now forces `colored` off process-wide at startup (`colored::control::set_override(false)`), so all rendered strings reach the browser as plain text. The CLI is a separate invocation and keeps its terminal colors.
  **Web UI：分析テキストにANSIエスケープが生表示される不具合を修正。** エンジンは端末向けに `colored` で着色しており、`serve` を端末から起動すると、その制御コードがブラウザ表示用のJSONに混入していた（前足比などが `←[31m…←[0m` のように表示）。サーバ起動時に `colored` をプロセス全体で無効化（`colored::control::set_override(false)`）し、全レンダリング文字列をプレーンテキストでブラウザへ渡すよう修正。CLIは別起動なので端末の色は維持。

### Removed
- **Personal financial data / portfolio (Class D) — removed before release.** The cash/positions/trades/portfolio feature added earlier in this cycle (chat commands `/cash` `/buy` `/sell` `/portfolio`, the 💼 panel, `/api/portfolio*`, `src/db/portfolio.rs`, and the Class-D tables `cash_balances`/`positions`/`trades`/`portfolio_snapshots`) was taken out: it is the most sensitive data yet was never fed to the analysis, so it carried the highest risk for the least unique value — a brokerage covers it better. A future brokerage-API integration would delegate personal-data security to the broker's own app. Existing rows in an old DB are simply no longer served.
  **個人金融データ／ポートフォリオ（クラスD）— リリース前に撤去。** 本サイクルで一度追加した現金/保有/取引/資産機能（チャットコマンド `/cash` `/buy` `/sell` `/portfolio`、💼パネル、`/api/portfolio*`、`src/db/portfolio.rs`、クラスDテーブル `cash_balances`/`positions`/`trades`/`portfolio_snapshots`）を撤去。最も機密なのに分析へ渡さない＝リスク最大・独自価値最小で、証券会社の方が上。将来の証券API連携では個人情報保護を証券側アプリに委ねる。旧DBの既存行は今後配信されないだけ。

### Security
- **Release binary hardened against anti-malware false positives.** A static review confirmed the binary carries **none** of the classic malware markers (no `std::process::Command`/shell-out, no `unsafe`/winapi/`LoadLibrary`/`VirtualAlloc`, no registry writes/self-persistence/packing, no `include_bytes!` blobs; file-ACL tightening is `#[cfg(unix)]`-only; Windows TLS uses SChannel via `native-tls`, no bundled OpenSSL). The realistic false-positive driver is an *unsigned* binary that both serves on loopback and reads the OS keyring while making outbound HTTPS. Mitigations: the `release` profile is now `strip = true` + `lto = true` + `codegen-units = 1` (symbol-free, ~half the debug size) and **only the release build should be distributed/scanned**; security-design.md §4 documents the full hardening checklist (Authenticode/EV signing, reproducible build + SHA-256, Microsoft FP submission).
  **配布バイナリのアンチウイルス誤検知対策を実施。** 静的レビューで、バイナリが**古典的マルウェアの痕跡を一切持たない**ことを確認（`std::process::Command`/シェル実行なし、`unsafe`/winapi/`LoadLibrary`/`VirtualAlloc`なし、レジストリ書込/自己永続化/パッキングなし、`include_bytes!`埋め込みなし、ファイルACL厳格化は`#[cfg(unix)]`限定、WindowsのTLSは`native-tls`経由のSChannelでOpenSSL同梱なし）。現実的な誤検知要因は「未署名バイナリがループバックで配信しつつOS keyringを読み外向きHTTPSする」点。対策：`release` プロファイルを `strip = true`＋`lto = true`＋`codegen-units = 1`（シンボルなし・debug比約半分）に強化し、**配布・スキャンは release のみ**。security-design.md §4 に完全な対策チェックリスト（Authenticode/EV署名、再現ビルド＋SHA-256、Microsoft誤検知申請）を記載。
- **Personal financial data classified as Class D** in `docs/dev-prog/security-design.md` (cash / positions / trades / portfolio): local DB only (0600), never logged, and **never placed in LLM prompts/context by default**. Added the DB SQL-injection rule (Repository/Store layer, parameterized `sqlx` queries, dynamic identifiers restricted to known values).

---

このリリース系列のはじまり：ローカルDB・分析履歴・マルチタイムフレーム・バックテストの基盤。設計は確定済み（sqlx + bundled SQLite、`AnalysisMode` の足トークン、OHLCV を市場データのSOTとし指標はエンジンで再計算、UTC ISO8601、DBはオプトイン、バックテストは日足中心）。`release/v2.2` で小フェーズに分けて実装。

### Added（日本語）
- **バックテスト — 実用化の刷新（ルールエディタ・正直なベンチマーク・現実的なサイジング）。** 骨格を実用機能に拡張。**ルールエディタ**（🧪パネル）で買い/売り条件を作成（指標→演算子→数値/指標・AND/OR・行の追加削除・クロス対応）、**テンプレと保存/呼び出しを同一リストに**（内蔵は `_template` 接尾辞、ユーザールールは `saved_strategies` に保存）。結果には毎回**実際のルール**（名前＋条件）を平易な言葉で表示し、常に**Buy&Hold（ただ持ち続け）を併記**して、弱いルールを銘柄自体の騰落と誤認しないようにした。**足種連動の期間ピッカー**：足種を選ぶとプロバイダが実際に許す期間だけを表示/制限（手動やHELP不要）。**資金モデルを現実的に**：金額は**株数ではなく現金**で、単位は**銘柄自身の通貨**（プロバイダの `meta.currency` 由来、例 JPY/USD＝推測しない）。売買は**最小単元**単位（`.T`＝100株・他＝1株の明記した仮定）で端数は現金保持。サイジングはユーザー指定で、**開始時に元手の一定割合を買い（既定50%）**、**シグナルごとに一定額を増減**。出力に前提・売買明細（現金の動き）・警告（元手で1単元買えない／売りシグナルだが保有ゼロ）を表示。リターン`%`を主役に（通貨非依存・為替換算なし）。
- **`xoksa serve --log-format json` — RESTサーバ向けの構造化ログ（NDJSON）。** コード付きロガーが、人間向けの `⚠️ [CODE] msg` テキストの代わりに**1行1JSONオブジェクト**（`{"ts","level","code","msg"}`）をコンソール・ファイル双方へ出力できるようになりました。既定は `text`、`--log-format json` でJSONを選択（外部クレート不使用の自前エンコーダ、JSON仕様に沿う最小限のエスケープ）。
- **バックテスト — フェーズ⑤b（骨格）。** 新規 `src/backtest.rs` ＋ `POST /api/backtest` で最小バックテストを実行：足は**プロバイダから連続履歴を1回取得**（疎なローカルDBは使わない＝DBは市場を映す前提にしない）、戦略は**ライブと同じSOTスコア**（ロングのみ：総合スコアが `entry_score` 以上で買い、`exit_score` 以下で売り）。各足の指標は同一エンジンで前方系列から再計算し、約定は確定足の終値（足が閉じている＝先読みなし。手数料は未モデル）。総リターン%・取引数・勝率・最大ドローダウン・資産・取引一覧を返し、DB有効時は `backtest_runs`＋`backtest_trades` に保存。UI：ヘッダーの **🧪 BT** ボタンでアクティブ銘柄のプリセット（現在の足・直近250本）を実行しチャットに要約を追記。足ソースと先読み防止は合意済みの疎DB設計に準拠。 入力はCLI/Web共有のSOTチャットコマンド（統一 `execute` ディスパッチ）：`/cash <金額> [通貨]`・`/buy <銘柄> <数量> <価格> [手数料]`・`/sell <銘柄> <数量> <価格> [手数料] [税]`・`/portfolio`（別名 `/assets`）。これらはモデル実行前に処理されるスラッシュコマンドのため、確認はユーザーに表示されるが個人データは**LLMコンテキストに渡らない**（クラスD）。ヘッダーの **💼 資産** ボタンで読み取り専用ポップアップを開き `GET /api/portfolio`（現金・保有〔平均/現在/評価額/含み損益〕・合計資産・保存足が無い保有の「未評価」注記）を表示。`/help` に「資産・個人データ」セクションを追加。要DB（`--db` / `DB_PATH`）。 新規 `src/db/portfolio.rs` と `/api/portfolio*` でユーザーの現金・取引・保有・資産スナップショットを記録/参照。`POST /api/portfolio/cash` で残高設定、`POST /api/portfolio/trade` でBUY/SELLを台帳に追記し、同一トランザクションで保有（数量・平均取得単価、BUY時は手数料込みで再平均）を更新し**現金を自動連動**（BUY −(数量·価格+手数料)、SELL +(数量·価格−手数料−税)）、`GET /api/portfolio` で現金＋保有＋（部分）合計、`GET /api/portfolio/trades`、`POST /api/portfolio/snapshot` で `portfolio_snapshots` に保存。**セキュリティ（クラスD）：** ローカルDBのみ・ログ非出力（書き込みは真偽のackのみ返す）・LLMへ渡さない。**疎DBの正直さ：** 保有の現在値は**保存済み**OHLCVの最新足から算出し、無ければ空欄（ローカルDBは市場を映す前提にしない）。`unvalued_positions` を返し、部分合計を完全と誤認させない。UI・チャットコマンド入力は後続サブフェーズ、バックテスト骨格はフェーズ⑤b。 `POST /api/analysis/multi-timeframe` が④aの context pack を構築し、LLMに解釈させる：pack が確定データ（SOT）で、チャットと同じガード制約を前置（数値・ニュース創作禁止、予測は明示）。pack テキスト＋モデルの解釈を返し、DB有効時は context（`analysis_contexts`）とturn（`llm_outputs`・role `multi_timeframe`）を保存。UI：ヘッダーの **📐 MTF** ボタンでアクティブ銘柄の固定プリセット（月足MACD＋日足ボリンジャー＋5分足価格）をPOSTし、pack＋解釈をチャットに追記。`ContextSpec` に `lang` を追加（pack/回答をブラウザ言語に合わせる）。 新規 `src/context.rs` がLLM向けのマルチタイムフレーム文脈を生成：`ContextSpec` に `{type, timeframe, indicator_name}`（例：月足MACD＋日足ボリンジャー＋5分足価格）を列挙し、各足を共有エンジン（`build_analyzed_guard`・要求指標だけ有効化）で1回ずつ分析、結果を**Rust側で要約**した `context_pack_text` ＋構造化 `context_pack_json` として返す（生データは渡さない＝SOT・トークン管理）。新エンドポイント `POST /api/analysis/context-pack`、DB有効時は `analysis_contexts` に保存。足は `AnalysisMode` で正規化。④aは「現在」の文脈（各足の最新）を生成し、過去 `as_of` は記録のみ（バックテストのデータ供給で後対応）。LLM解釈エンドポイント（`/multi-timeframe`）とUIプリセットは④b。 DBの読み出し側（`src/db/read.rs`・パラメータ化・新しい順）：`GET /api/symbol/{symbol}/analysis-runs`（スコア推移）・`/ohlcv?timeframe=`・`/indicators?timeframe=`・便利バンドル `/history`（最近のrun＋news）。いずれもDB設定時のみ、未接続なら空配列。`timeframe` は `AnalysisMode` で正規化、`limit` は上限1000にクランプ。最小UI：ヘッダーの **📊 履歴** ボタンで別ウィンドウを開き、アクティブ銘柄の `/history` を取得して分析runのスコア推移（緑/赤）＋最近ニュースを表示。ポップアップは同期的に開いて（ブロッカー回避）から取得後に中身を流し込む。 DB有効時、各分析（CLI・Web 両方——保存フックは共有の `build_analyzed_guard` にあるため一度で両対応）で次を書き込み：`symbols`（upsert）、その足の **OHLCV系列**（`UNIQUE(symbol,timeframe,timestamp)` で冪等、確定足は `is_complete=1`、`open` はデータソース非提供のためNULL）、`analysis_runs` スナップショット（エンジンの総合スコア）。`analysis_runs` は `UNIQUE(symbol,timeframe,market_data_timestamp)` で**足ごとにデデュープ**し、30秒の自動更新ポーリングでrunが溜まらない。時刻はUTC ISO8601。書き込みは `db::store`（パラメータ化クエリ）経由で**ベストエフォート**——失敗は警告ログのみで分析は止めない。 **フェーズ②b**で残りの書き込みを追加：per-indicator の `technical_indicators` スナップショット（`(symbol,timeframe,timestamp,indicator_name,params_hash)` でデデュープ。`params_hash` は指標期間の安定シグネチャ）、`fundamentals`（symbol+会計期+報告日でupsert・`/summary` にフック）、`news_items`（タイトル/URLのみ・`(symbol,url)` でデデュープ・`/news` にフック）、`llm_outputs`（自由質問のチャットturn：prompt+response+トークン数）。同一分析の再実行で新規書き込みなし＝冪等を確認。
- **DB層 — フェーズ①（オプトインのSQLite／`sqlx`）。** `xoksa serve --db <path>`（または `DB_PATH`）でローカルSQLiteに接続・スキーマ適用・状態表示。どちらも未指定なら従来どおりDBなしで動作。新モジュール `src/db/` を唯一のSQL接点とし（将来のPostgres移行容易）、`DbStore`（プール）・オプトインのプロセスグローバル・`resolve_db_path` を提供。初期スキーマ（`migrations/0001_init.sql`）で合意した全テーブルを作成——Market（symbols/ohlcv/technical_indicators/fundamentals/news_items）、Analysis（analysis_runs/llm_outputs/analysis_contexts/symbol_history_summaries）、個人/バックテスト骨格（cash_balances/positions/trades/portfolio_snapshots/backtest_runs/backtest_trades）——UTC-ISO8601、`AnalysisMode` の足トークン、先読み防止用 `is_complete` 付き。新エンドポイント `GET /api/db/status`・`POST /api/db/init`、ヘッダーの **DBバッジは接続時に緑**（`meta.db_status` が実状態を反映）。SQLiteは**bundled**（システムライブラリ不要＝単一バイナリ維持）。サプライチェーン：`sqlx` の `macros`/`migrate` は意図的に不使用（全ドライバ＝`sqlx-mysql`→脆弱な`rsa` を引くため）。スキーマは `include_str!`＋`raw_sql` で埋め込み。`cargo audit` 脆弱性ゼロ。

### Security（日本語）
- **個人金融データをクラスDとして分類**（`docs/dev-prog/security-design.md`。現金/保有/取引/ポートフォリオ）：ローカルDB限定（0600）・ログ非出力・**既定でLLMプロンプト/コンテキストに入れない**。DBのSQLi対策（Repository/Store層・`sqlx`バインド引数・動的識別子は既知値に限定）も明記。

---

## [2.1.0] — 2026-06-26

### Added
- **1-minute bar mode (`1m`)** added to `AnalysisMode` (requested by a reviewer). Available everywhere the other timeframes are — CLI `--analysis-mode 1m` / `ANALYSIS_MODE=1m`, chat `/mode 1m`, `/show t 1m`, and the Web UI timeframe dropdown (data-driven from `AnalysisMode::ALL`, so it appeared automatically). Fetches the `1m` interval over a ~5-day window (Yahoo's 1m limit). While here, `60m`/`1h`/`hourly` — already valid via env/`from_value` — were also added to the `--analysis-mode` CLI value list and the setup wizard, closing a prior gap where the flag rejected them. Labels (`1分足` / `1min bar`) come from the shared `AnalysisMode` methods (SOT), so CLI and Web read identically.
  **1分足モード（`1m`）を `AnalysisMode` に追加**（レビュアー要望）。他の足と同じく全所で利用可——CLI `--analysis-mode 1m` / `ANALYSIS_MODE=1m`、チャット `/mode 1m`・`/show t 1m`、Web UI の時間足ドロップダウン（`AnalysisMode::ALL` 由来で自動反映）。`1m` 間隔を約5日窓で取得（Yahoo の 1m 制限）。あわせて、env/`from_value` では有効だが CLI の値リストから漏れていた `60m`/`1h`/`hourly` も `--analysis-mode` とセットアップウィザードに追加（既存の不整合を解消）。ラベル（`1分足` / `1min bar`）は共有の `AnalysisMode` メソッド（SOT）由来で、CLI と Web が同一表記。
- **Web UI (GUI mode) manual** (`docs/manual/webui-guide.md`, JA + EN): launching the dashboard (`xoksa serve --ui`, flags, building the frontend with `trunk`), the screen layout, ticker chips, the single timeframe shared by dashboard and chat, the market chart popup (price overlays + RSI + MACD + toggles), the chat being a CLI superset, browser-language behavior, the native/WASM boundary, known limitations, and **why there is no HTTPS mode** (local-first, loopback only). `docs/manual/setup.md` gains a short "Launching the Web UI (GUI)" pointer (§6-3) linking to it.
  **Web UI（GUIモード）マニュアル**（`docs/manual/webui-guide.md`・日英）: ダッシュボードの起動（`xoksa serve --ui`・フラグ・`trunk` でのフロントエンドビルド）、画面構成、銘柄チップ、ダッシュボードとチャットで共有する単一の時間足、市況チャート別窓（価格重ね＋RSI＋MACD＋トグル）、チャットがCLIの上位互換である点、ブラウザ言語の挙動、ネイティブ/WASM境界、既知の制約、そして**HTTPSモードがない理由**（ローカルファースト＝ループバックのみ）を記載。`docs/manual/setup.md` に「Web UI（GUI）の起動」への短いポインタ（§6-3）を追加。
- **Rust/WASM frontend (Leptos)**: the Web UI is now a client-side-rendered Leptos app (`webui-leptos/`, a standalone crate built with `trunk`), replacing the dependency-free JS UI. The boundary is unchanged — it is presentation only and talks to the native engine solely over the JSON API (`/api/*`); no analysis / indicators / LLM / DB / backtest logic lives in WASM. New UI capabilities: **dark/light theme toggle**, **font size S/M/L**, and **reactive auto-refresh** — a Leptos `Resource` refetches automatically when the symbol or timeframe changes plus a periodic 30s tick, so no manual "Analyze" press is required (a toggle disables it; "↻" forces a refresh). Theme/font/panel-sizes persist to `localStorage`. Panels remain independently resizable (drag gutters) and scrollable. The crate is its own workspace, so the native `xoksa` crate and CI are unaffected.
  **Rust/WASM フロントエンド（Leptos）**: Web UI を、依存ゼロの JS UI から Leptos のクライアントサイドレンダリング（CSR）アプリ（`webui-leptos/`・`trunk` でビルドする独立クレート）へ刷新。境界は不変で表示専用、ネイティブエンジンへは JSON API（`/api/*`）経由でのみ接続（分析・指標・LLM・DB・バックテストのロジックは WASM に置かない）。新UI機能：**ダーク/ライトのテーマ切替**、**フォントサイズ 大中小**、**リアクティブ自動更新**（Leptos の `Resource` が銘柄・時間足の変更で自動再取得＋30秒間隔のtickで定期更新。手動 Analyze 不要。トグルで無効化、「↻」で即時更新）。テーマ/フォント/パネルサイズは `localStorage` に保存。各パネルは独立リサイズ（ドラッグ）・独立スクロール。クレートは独立ワークスペースのため、ネイティブ `xoksa` クレートと CI は無影響。
- **News panel + Help screen** (Web UI): a News panel sits between the fundamental and market panels, listing latest article titles as clickable links with the URL shown and the publish time. Backed by a new `GET /api/symbol/{symbol}/news` endpoint (Brave Search, best-effort — a missing `BRAVE_API_KEY` is a note, not an error) that the frontend fetches **only when the symbol changes** (not on the auto-refresh tick) to avoid hammering the API. A header "? Help" button opens a modal listing the chat-mode slash commands, served by `GET /api/chat/commands`.
  **News パネル ＋ Help 画面**（Web UI）: ファンダメンタルと市況の間に News パネルを追加。最新ニュースのタイトルをクリック可能なリンクとして表示し、URL と公開時刻も併記。新エンドポイント `GET /api/symbol/{symbol}/news`（Brave Search、ベストエフォート。`BRAVE_API_KEY` 未設定はエラーではなく注記）を、API 連打を避けるため**銘柄変更時のみ**取得する（自動更新の tick では取得しない）。ヘッダーの「? Help」ボタンでチャットモードのスラッシュコマンド一覧モーダルを表示（`GET /api/chat/commands` が供給）。
- **Web UI chat is now real** (no longer a mock): `POST /api/chat` calls the LLM (`llm::send_chat_turn`) grounded in the symbol's confirmed analysis (`collect_display_lines`) with an inline SOT constraint (no fact creation outside the data; forecasts labeled). Stateless for now — no cross-turn memory, and Council/Debate are not yet wired. **Consistency rule:** the Web UI chat does not re-implement CLI commands (a command must never yield a different result across CLI/Web UI) — `/help` shows the command reference, all other slash commands run in CLI chat (`--chat`), and UI settings (theme / font / symbol / timeframe / refresh) are header controls, not commands. The Help list mirrors the CLI commands (adds `/council` / `/board`; clarifies `/ticker` / `/reload` / `/llm`) with a note that they run in CLI chat. A true superset (all CLI commands working identically in the Web UI) requires the stateful chat-engine integration, tracked as a follow-up.
  **Web UI チャットを実動作化**（モック廃止）: `POST /api/chat` が銘柄の確定分析（`collect_display_lines`）を根拠に LLM（`llm::send_chat_turn`）を呼ぶ（インライン SOT 制約付き＝データ外の事実創作なし・予測は明示）。現状ステートレス（ターン跨ぎのメモリ無し、Council/Debate は未接続）。**整合ルール:** Web UI チャットは CLI コマンドを再実装しない（同じコマンドが CLI と Web UI で違う結果になってはならない）——`/help` はコマンド一覧を表示、その他のスラッシュコマンドは CLI チャット（`--chat`）で実行、表示設定（テーマ／フォント／銘柄／足／更新）はコマンドではなくヘッダーのコントロール。Help 一覧は CLI コマンドに準拠（`/council`・`/board` を追加、`/ticker` / `/reload` / `/llm` を明確化）し、CLI チャットで動作する旨を注記。真の上位互換（全 CLI コマンドが Web UI でも同一に動く）はステートフルなチャットエンジン統合が必要で、フォローアップとして管理。

### Removed
- **Legacy dependency-free JS Web UI (`webui/`) retired**, fully superseded by the Rust/WASM (Leptos) UI which is the default. With it go the `webui/` assets and the only endpoint it used: `POST /api/chat` (plus its `chat` handler, `ChatRequest`/`ChatResponse`, and the now-unused `exec_web_command` + `ChatOut::Buffer` sink). The Web UI chat is SSE-only now (`GET /api/chat/stream`). Removing the second UI shrinks the attack surface and deletes a duplicate (non-streaming) chat path — one fewer place for the CLI/Web behaviors to drift.
  **レガシーの依存ゼロ JS Web UI（`webui/`）を廃止**。既定の Rust/WASM（Leptos）UI が完全に上位互換のため。`webui/` アセットと、それだけが使っていた `POST /api/chat`（および `chat` ハンドラ・`ChatRequest`/`ChatResponse`・未使用化した `exec_web_command` と `ChatOut::Buffer` シンク）を削除。Web UI のチャットは SSE のみ（`GET /api/chat/stream`）に統一。二つ目の UI を除去して攻撃面を縮小し、非ストリームのチャット重複経路を削除（CLI/Web の挙動が乖離する箇所を一つ減らす）。

### Fixed
- **Web chat now persists config-mutating commands across requests** (e.g. `/llm` provider/model switch, cumulative token count). The CLI keeps one `Config` for the whole REPL, but the Web rebuilt `Config` from env on every `/api/chat` request, so any command that writes to `Config` was discarded — `/llm ollama:gpu1` reported success yet `/status` still showed the old provider. `WEB_SESSIONS` now stores `{ session, config, tokens }` per `symbol|timeframe` and runs against the persisted `config`/`tokens`, applying only per-request overrides (browser language, header timeframe, news-as-data). State that already lived on `ChatSession` (`/tune`, memory, `/forum` participants + debate buffer, `/sym` set, `/nx`) was unaffected; `/mode` already round-tripped via the header. Note: switching symbol or timeframe starts a fresh session (env defaults), like the CLI starting a new chat.
  **Web チャットで config 変更系コマンドがリクエストを跨いで保持されるよう修正**（例: `/llm` のプロバイダー/モデル切替、トークン累計）。CLI は REPL 全体で 1 つの `Config` を持つが、Web は `/api/chat` 毎に env から `Config` を作り直していたため、`Config` を書き換えるコマンドが破棄されていた——`/llm ollama:gpu1` は成功表示でも `/status` は旧プロバイダーのまま。`WEB_SESSIONS` を `symbol|timeframe` ごとに `{ session, config, tokens }` で保持し、保持した `config`/`tokens` に対して実行（リクエスト由来＝ブラウザ言語・ヘッダー足・news-as-data のみ上書き）。`ChatSession` 側に持つ状態（`/tune`・メモリ・`/forum` 参加者＆ディベートバッファ・`/sym`・`/nx`）は元から無影響、`/mode` はヘッダー往復で既に保持。注意: 銘柄や足を変えると新規セッション（env 既定）から開始（CLI で新しいチャットを始めるのと同じ）。

### Changed
- **Bar-mode validation derives from a single source (`AnalysisMode::from_value`)**. The `--analysis-mode` / `--chat-analysis-mode` clap flags previously hard-coded their accepted-token lists (which had already drifted — `60m`/`1h`/`hourly` were valid via env but rejected by the flag), and the setup wizard kept its own list too. Both now validate through `from_value`, and the flag's error message lists the canonical modes from `AnalysisMode::ALL`. Adding a future bar mode now needs no list edits and can't drift. (Verified: the flag now accepts `1h`/`60m`; an invalid value is rejected with the derived list.)
  **足モードの検証を単一ソース（`AnalysisMode::from_value`）由来に**。`--analysis-mode` / `--chat-analysis-mode` の clap フラグは受理トークンを手書き列挙しており（既にドリフト＝`60m`/`1h`/`hourly` は env では有効なのにフラグでは拒否）、セットアップウィザードも独自リストを持っていた。両者とも `from_value` で検証し、フラグのエラーメッセージは `AnalysisMode::ALL` から正準モードを列挙。今後モードを足してもリスト編集不要・ドリフト不能。（検証：フラグが `1h`/`60m` を受理、無効値は派生リストで拒否。）
- **Clearer wording for the four header time fields** (CLI + Web UI, same SOT output): the data-freshness labels were renamed so the distinction between the latest price's bar and the indicator's confirmed bar is obvious — `市場データ最新時刻`→**`データの最新時刻`** (Latest data time), `最新取得価格が属する足`→**`最新価格が入る足`** (Bar of latest price), `指標計算最終足`→**`指標を計算した足（確定足）`** (Indicator bar (confirmed)); `分析時刻` (Analysis time) is unchanged. The labels are now a single source of truth (`utils::freshness_labels()`) shared by the on-screen display (`render.rs`) and the LLM context (`llm.rs`), which also fixes a prior EN-wording divergence between the two. No fields were added or removed — all four are still shown (they coincide after close/holiday, and diverge intraday; the longer the timeframe, the wider the gap). `docs/manual/indicator-guide.md` now explains this with ASCII-art timeline diagrams (JA + EN).
  **ヘッダーの4つの時刻ラベルを分かりやすい文言に変更**（CLI・Web UI 共通の SOT 出力）: 「最新価格の足」と「指標の確定足」の違いが一目で分かるようリネーム——`市場データ最新時刻`→**`データの最新時刻`**、`最新取得価格が属する足`→**`最新価格が入る足`**、`指標計算最終足`→**`指標を計算した足（確定足）`**（`分析時刻` は変更なし）。ラベルは `utils::freshness_labels()` を単一ソースとして画面表示（`render.rs`）と LLM 文脈（`llm.rs`）で共有し、従来あった英語表記の食い違いも解消。項目の増減はなし（4項目すべて表示。引け後・休場は一致し、ザラ場ではズレる。足が長いほど差が大きい）。`docs/manual/indicator-guide.md` にアスキーアートのタイムライン図解で説明を追加（日英）。
- `src/server/mod.rs`: `serve --ui` now defaults `--web-dir` to the Leptos build output (`webui-leptos/dist`); pass `--web-dir webui` to serve the legacy JS UI. Static serving now sets `application/wasm` for `.wasm` (required for WASM instantiation). The legacy JS UI (`webui/`) is kept for now as a fallback.
  `src/server/mod.rs`: `serve --ui` の `--web-dir` 既定値を Leptos ビルド出力（`webui-leptos/dist`）に変更（旧 JS UI を使う場合は `--web-dir webui`）。静的配信が `.wasm` に `application/wasm` を返すようにした（WASM インスタンス化に必須）。旧 JS UI（`webui/`）は当面フォールバックとして残置。
- **DRY: the Web UI server reuses native logic instead of duplicating it.** The chat SOT guard text is now sourced from `chat::guard::{constraint_text, forecast_clause}` (made `pub(crate)`) rather than a parallel constant. The chat-command help is now a single catalog in `chat::help::command_catalog` — both the CLI `/help` and the Web UI Help modal (`/api/chat/commands`) render from it, so the command reference can never drift. The fundamental panel now serves the native `fundamental::render_fundamental_display` lines (same text as the CLI) instead of a structured DTO re-formatted in the WASM frontend — removing the only client-side number-formatting copy. The analysis-mode label moved to `AnalysisMode::as_str()`. (The analysis pipeline, env loading, indicator/fundamental/news fetching, config building and ticker sanitization were already shared via `app::build_analyzed_guard`, `bootstrap::load_env_map`, etc.)
  **DRY：Web UI サーバはネイティブロジックを複製せず再利用する。** チャットの SOT ガード文は並行定義をやめ `chat::guard::{constraint_text, forecast_clause}`（`pub(crate)` 化）から取得。チャットコマンドの Help は `chat::help::command_catalog` の単一カタログに統一し、CLI `/help` と Web UI の Help モーダル（`/api/chat/commands`）の双方がそこから生成する（一覧がドリフトしない）。（分析パイプライン・env 読込・指標/ファンダ/ニュース取得・Config 構築・ティッカー sanitize は既に `app::build_analyzed_guard`・`bootstrap::load_env_map` 等で共有済み。）
- **Unified command dispatch (CLI = Web UI, one implementation).** Every chat command now runs through a single `ChatSession::execute` writing to a `ChatOut` sink (the CLI prints/streams; the Web buffers to JSON), so a command can never yield a different result across CLI and Web. The Web UI chat runs all slash commands and free-text through the same path (`exec_web_command` → `session.build_prompt` / shared helpers) — no parallel prompt assembly. The earlier "Web chat does not re-implement CLI commands" limitation is gone: the Web is a true superset.
  **コマンドの単一ディスパッチ（CLI ＝ Web UI、実装は一つ）。** 全チャットコマンドは `ChatOut` シンクに書く単一の `ChatSession::execute` を通る（CLI は即時出力/ストリーミング、Web は JSON バッファ）。よって同じコマンドが CLI と Web で違う結果になることはない。Web UI チャットはスラッシュコマンドも自由文も同じ経路（`exec_web_command` → `session.build_prompt`／共有ヘルパ）で実行し、並行プロンプト組み立ては無い。以前の「Web チャットは CLI コマンドを再実装しない」制限は解消し、Web は真の上位互換。
- **Command system redesign (BREAKING).** All command words are now ≤6 chars with a uniform `cmd [sub] [args]` grammar, and `/help` is grouped by category. Renames: `/memory`→`/mem`, `/ticker`→`/sym`, `/autoreload`→`/auto`, `/prompt`→`/run`, `/news-extra`→`/nx` (now `nx add`/`nx in`). The six response-style commands collapse into `/tune <field> <val>` (tone/hypo/sens/shape/depth/cast). `/council` + `/debate` + `/board` + `/criticize` merge into `/forum` (`set` / `ask [rN] <q>` / `sum` / `crit` / `log` / `keep <mode>` / `chair <p>` / `clear`). Old names print a one-time migration hint pointing at the new name.
  **コマンド体系の再設計（破壊的変更）。** 全コマンド語を ≤6 文字に統一、文法は `cmd [sub] [args]` に統一、`/help` はカテゴリ別。改名：`/memory`→`/mem`、`/ticker`→`/sym`、`/autoreload`→`/auto`、`/prompt`→`/run`、`/news-extra`→`/nx`（`nx add`/`nx in`）。応答スタイル6コマンドは `/tune <項目> <値>`（tone/hypo/sens/shape/depth/cast）に集約。`/council`＋`/debate`＋`/board`＋`/criticize` は `/forum` に統合（`set` / `ask [rN] <質問>` / `sum` / `crit` / `log` / `keep <mode>` / `chair <p>` / `clear`）。旧名は新名を案内する移行ヒントを1回表示。
- **`/llm` now lists usable models.** `/llm` (no arg) lists each cloud provider's effective model (env/default), API-key presence, the current selection, and ollama instances — locally, no network. `/llm net` queries each provider's API for current model names (a discovery aid for filling `*_model` in `xoksa.env`).
  **`/llm` で使えるモデルを一覧。** `/llm`（引数なし）は各クラウドプロバイダの実効モデル（env/初期値）・APIキー有無・現在の選択・ollama インスタンスをローカル（ネット不要）で一覧。`/llm net` は各社 API から現行モデル名を取得（`xoksa.env` の `*_model` 記入の参考）。
- **Web UI: SOT and locale fixes.** The market-line indicators now reuse the CLI's `notice_indicator_summary` (config-driven; no hardcoded subset); the dashboard's default ticker/timeframe and the timeframe list come from a new `GET /api/config` (no hardcoded `6740.T`); the market log advances per bar and never mixes timeframes; the GUI language follows the browser (`ja*` → Japanese, else English) and is sent to the API so server text matches; the Help command list opens in a separate popup window whose rows pre-fill the chat input.
  **Web UI：SOT とロケールの修正。** 市況行の指標は CLI の `notice_indicator_summary` を再利用（config 駆動・ハードコード廃止）。ダッシュボードの既定銘柄/足と足種リストは新 `GET /api/config` から取得（`6740.T` ハードコード廃止）。市況ログは足ごとに進み、足種が混ざらない。GUI 言語はブラウザ準拠（`ja*`→日本語、それ以外→英語）で API にも送り、サーバ生成テキストも一致。Help 一覧は別ウィンドウで開き、行クリックでチャット欄に挿入。

### Security
- **CI/CD hardening against supply-chain / pipeline-takeover** (the "Cordyceps" class). `.github/workflows/ci.yml`: added `permissions: contents: read` (least privilege), `persist-credentials: false` on checkout, pinned all actions to commit SHAs (Dependabot keeps them current), and added a `cargo audit` job that fails on known RustSec advisories. Added `.github/dependabot.yml` (cargo for root + `webui-leptos`, and github-actions). Added `docs/dev-oper/ci-cd-security.md` documenting why the core attack vector does not apply (CI uses `pull_request` not `pull_request_target`, holds no secrets, has no comment/auto-merge automation, manual maintainer Squash, hosted runners) plus the repository-settings checklist the maintainer must enable and the rules for any future secret-bearing workflow.
  **サプライチェーン／パイプライン乗っ取り（「Cordyceps」系）への CI/CD 硬化。** `.github/workflows/ci.yml`：`permissions: contents: read`（最小権限）、checkout `persist-credentials: false`、全 Action のコミット SHA 固定（Dependabot が追従）、既知 RustSec アドバイザリで失敗する `cargo audit` ジョブを追加。`.github/dependabot.yml` を追加（cargo：ルート＋`webui-leptos`、github-actions）。`docs/dev-oper/ci-cd-security.md` を追加し、中核ベクタが成立しない理由（CI は `pull_request` で `pull_request_target` ではない／シークレット不使用／コメント・自動マージ自動化なし／管理者手動 Squash／ホストランナー）と、管理者が有効化すべきリポジトリ設定チェックリスト、将来シークレットを扱うワークフロー追加時の鉄則を明文化。

### Docs
- Bumped version metadata to v2.1.0 across `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/dev-prog/source-map.md`, `docs/manual/command-reference.md`, `docs/manual/setup.md`, and `docs/manual/strategy-guide.md`. README documents the `trunk build` step for the WASM UI.
  バージョン表記を v2.1.0 に統一（`Cargo.toml`・`Cargo.lock`・`README.md`・`docs/dev-prog/source-map.md`・`docs/manual/command-reference.md`・`docs/manual/setup.md`・`docs/manual/strategy-guide.md`）。README に WASM UI 用の `trunk build` 手順を記載。

---

## [2.0.0] — 2026-06-26

### Added
- **Web UI / local HTTP server** (`xoksa serve --ui --port 8787`): initial local-first analysis workbench. A new `src/server/` module (axum) serves a browser dashboard from `webui/` (header bar + technical / fundamental / market panels + chat box) and a JSON API: `GET /api/health`, `GET /api/symbol/{symbol}/summary`, and `POST /api/chat`. **`/summary` runs the real native pipeline** (market snapshot → technical analysis → optional fundamentals) and returns the exact CLI analysis text (`collect_display_lines`) plus the real provider name, a market-snapshot line, and real fundamentals when keys are configured (a clear "未取得" note otherwise — never fake data). Dashboard panels are **independently resizable** via drag gutters (column width, fundamental height, chat height), persisted to `localStorage`, and stack vertically on narrow screens. `POST /api/chat` is still a mock reply (real chat engine is the next step). The `serve` subcommand is intercepted in `main()` before the analysis CLI parser, so the existing flag-based CLI and chat mode are untouched. Frontend is dependency-free HTML/CSS/JS, layered (`api` / `render` / `splitter` / `state`) for a later Rust/WASM swap. **Architecture boundary:** analysis, indicator math, fundamentals, the LLM call, the DB, and backtests stay native and are reached only via HTTP — never moved to the browser/WASM layer. New dependency: `axum` (thin layer over the tokio/hyper stack already pulled in by reqwest); static serving is in-house with a path-traversal guard (unit-tested), no `tower-http` added. TODOs left in `src/server/` for: DB-backed history, backtest screen, SSE chat streaming, single-binary asset embedding, and WASM UI.
  **Web UI / ローカル HTTP サーバ**（`xoksa serve --ui --port 8787`）：ローカルファースト分析ワークベンチの初期版。新規 `src/server/` モジュール（axum）が `webui/` のブラウザ用ダッシュボード（ヘッダーバー＋指標／ファンダメンタル／市況パネル＋チャット欄）と JSON API（`GET /api/health`、`GET /api/symbol/{symbol}/summary`、`POST /api/chat`）を配信する。**`/summary` は実際のネイティブパイプライン**（市場スナップショット→テクニカル分析→任意でファンダメンタル）を実行し、CLI と同一の分析テキスト（`collect_display_lines`）に加え、実プロバイダ名・市況スナップショット行・（キー設定時の）実ファンダメンタルを返す（未設定時は明確な「未取得」注記で、偽データは出さない）。ダッシュボードの各パネルはドラッグ用ガター（左右幅・ファンダ高さ・チャット高さ）で**独立リサイズ可能**、`localStorage` に保存、狭幅では縦積み。`POST /api/chat` は引き続きモック応答（実チャットエンジン接続は次段階）。`serve` サブコマンドは分析 CLI パーサより前に `main()` で先回り判定するため、既存のフラグ式 CLI とチャットモードは無変更。フロントエンドは依存ゼロの HTML/CSS/JS で層分離（`api` / `render` / `splitter` / `state`）。**アーキテクチャ境界：** 分析・指標計算・ファンダメンタル・LLM・DB・バックテストはネイティブに残し HTTP 経由でのみ到達 — ブラウザ／WASM 層へは移さない。新規依存は `axum`（reqwest が既に持つ tokio/hyper スタック上の薄い層）、静的配信は自前実装でパストラバーサル防御付き（ユニットテスト済）・`tower-http` 非依存。`src/server/` に今後の TODO（DB 履歴・バックテスト画面・SSE チャットストリーミング・単一バイナリ埋め込み・WASM UI）を記載。

### Changed
- Extracted the market-data → technical-analysis pipeline from `main.rs` into `app::build_analyzed_guard()`, and the `xoksa.env` parsing from `bootstrap` into `bootstrap::load_env_map()`. Both are now shared by the CLI and the Web UI server (no duplication; single source of truth for the analysis sequence). Behavior-preserving; existing CLI output unchanged.
  市場データ→テクニカル分析のパイプラインを `main.rs` から `app::build_analyzed_guard()` へ、`xoksa.env` のパースを `bootstrap` から `bootstrap::load_env_map()` へ抽出。いずれも CLI と Web UI サーバで共有（重複排除・分析シーケンスの単一情報源化）。挙動は不変で既存 CLI 出力に変更なし。

### Docs
- Bumped version metadata to v2.0.0 across `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/dev-prog/source-map.md`, `docs/manual/command-reference.md`, `docs/manual/setup.md`, and `docs/manual/strategy-guide.md`. (The `/api/health` version — shown in the Web UI connect message — is derived from `CARGO_PKG_VERSION`, so it now reports v2.0.0 automatically.)
  バージョン表記を v2.0.0 に統一（`Cargo.toml`・`Cargo.lock`・`README.md`・`docs/dev-prog/source-map.md`・`docs/manual/command-reference.md`・`docs/manual/setup.md`・`docs/manual/strategy-guide.md`）。`/api/health` のバージョン（Web UI の接続メッセージに表示）は `CARGO_PKG_VERSION` 由来のため自動で v2.0.0 になる。

---

## [1.6.9] — 2026-06-14

### Added
- `build.rs` + `winresource` (Windows-only build-dependency): embed PE version metadata (ProductName, FileDescription, CompanyName, LegalCopyright, FileVersion/ProductVersion derived from `CARGO_PKG_VERSION`, OriginalFilename) and an application manifest (`asInvoker` — no elevation; Windows 7–11 `supportedOS` compatibility) into the Windows binary. An unsigned CLI carrying no version resource or manifest reads as more suspicious to antivirus/heuristic scanners; this narrows that false-positive surface (it is **not** a substitute for code signing). Gated on a Windows host via `[target.'cfg(windows)'.build-dependencies]` and `#[cfg(windows)]`, so non-Windows builds — including the Linux CI — pull in no extra dependency and compile an empty build script. A missing resource compiler (`rc.exe` / `windres`) emits a non-fatal warning and never breaks the build.
  `build.rs` + `winresource`（Windows限定の build-dependency）：Windowsバイナリに PEバージョン情報（ProductName・FileDescription・CompanyName・LegalCopyright・`CARGO_PKG_VERSION` 由来の FileVersion/ProductVersion・OriginalFilename）とアプリケーションマニフェスト（`asInvoker`＝昇格なし、Windows 7〜11 の `supportedOS` 互換性）を埋め込む。バージョンリソースもマニフェストも持たない未署名CLIはアンチウイルス／ヒューリスティック検査で疑わしく見えやすく、その誤検知の余地を狭める（コード署名の**代替ではない**）。`[target.'cfg(windows)'.build-dependencies]` と `#[cfg(windows)]` でWindowsホストに限定し、非Windowsビルド（Linux CI を含む）は追加依存を取り込まず空のビルドスクリプトをコンパイルする。リソースコンパイラ（`rc.exe` / `windres`）が無い場合は致命的エラーにせず警告のみ出す。

### Fixed
- `src/setup.rs`: `--init` / `--update-key` prompts (`prompt_line`, `prompt_secret`) double-counted every keystroke on Windows (e.g. selecting `1` registered as `11`). crossterm delivers both `Press` and `Release` key events on Windows, while macOS/Linux deliver `Press` only; the raw-mode input loops processed every `Event::Key` without checking its kind. Both loops now skip non-`Press` events (`KeyEventKind::Press`), so one physical tap registers once on every platform. No behavior change on macOS/Linux (they never sent the extra events). Chat-mode input is unaffected — it uses `rustyline`, not a raw crossterm event loop.
  `src/setup.rs`: `--init` / `--update-key` の入力プロンプト（`prompt_line`・`prompt_secret`）が、Windowsで1打鍵を二重にカウントしていた不具合を修正（例：`1` を選ぶと `11` になる）。crossterm はWindowsでキーの `Press` と `Release` の両イベントを配信する一方、macOS/Linux は `Press` のみを配信するが、raw モードの入力ループがイベント種別を見ずにすべての `Event::Key` を処理していた。両ループが非 `Press` イベント（`KeyEventKind::Press` 以外）をスキップするようにし、全プラットフォームで物理1打鍵が1回だけ登録されるようにした。macOS/Linux は元々追加イベントを送らないため挙動変更なし。チャットモードの入力は `rustyline` を使い生の crossterm イベントループではないため影響なし。

### Changed
- `src/chat/{mod,news_extra,tests,ticker}.rs`: reformatted by `cargo fmt` under the CI toolchain (stable 1.96.0 / rustfmt 1.9.0) — `assert!` overflow style, `pub use` ordering, a stray blank line, and tall-layout function signatures that an older local rustfmt had left unwrapped. Whitespace and layout only; no logic change. Restores a green `cargo fmt -- --check` on the current stable toolchain.
  `src/chat/{mod,news_extra,tests,ticker}.rs`: CI toolchain（stable 1.96.0 / rustfmt 1.9.0）の `cargo fmt` で整形 — `assert!` のオーバーフロー整形・`pub use` の並び順・余分な空行・tall レイアウトの関数シグネチャ（旧ローカル rustfmt が折り返していなかった箇所）。空白・レイアウトのみでロジック変更なし。現行 stable toolchain で `cargo fmt -- --check` を green に戻す。

### Docs
- Bumped version metadata to v1.6.9 across `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/dev-prog/source-map.md`, `docs/manual/command-reference.md`, `docs/manual/setup.md`, and `docs/manual/strategy-guide.md`.
  バージョン表記を v1.6.9 に統一（`Cargo.toml`・`Cargo.lock`・`README.md`・`docs/dev-prog/source-map.md`・`docs/manual/command-reference.md`・`docs/manual/setup.md`・`docs/manual/strategy-guide.md`）。

---

## [1.6.8] — 2026-06-13

### Changed
- `src/chat/`: Completed the semantic decomposition of `chat/mod.rs` (was ~5.5k lines) via six behavior-preserving extractions. Logic submodules: `chat/llm.rs` (`/llm` provider switching, model resolution, chat-turn dispatch — `parse_llm_switch_command`, `active_llm_model`, `resolve_llm_default_model`, `send_chat_input_to_llm`, `live_ollama_instances`, `LlmSwitchCmd`), `chat/ticker.rs` (ticker load/switch/reload + autoreload notice — `load_ticker_analysis`, `apply_ticker_*`, `reload_chat_tickers`, `autoreload_*`, `notice_indicator_summary`, `LoadedTickerContext`, `TickerSwitchData`), `chat/news_extra.rs` (`/news-extra` buffer — `handle_news_extra_command`, `build_news_extra_inject_text`, `show_news_filtered_llm`, `NewsExtraEntry`), `chat/run.rs` (`run_chat_loop`: the interactive REPL and slash-command dispatch, re-exported as `chat::run_chat_loop`), and `chat/prompt.rs` (a second `impl ChatSession` block holding all prompt/context assembly — regular chat, `/criticize`, Council/Facilitator, and `/prompt` builders; 21 methods). The unit-test module moved to `chat/tests.rs` (`#[cfg(test)] mod tests;`). Moved free items and prompt methods are exposed `pub(super)`; cross-module struct fields that the parent constructs/destructures are `pub(super)`. `chat/mod.rs` dropped from 5,481 → 855 lines (now just the `ChatSession` definition, shared types, and small free helpers). Byte-identical `sed` moves; no behavioral change (211 unit + 10 integration tests pass, `clippy -D warnings` clean).
  `src/chat/`: `chat/mod.rs`（旧約5,500行）の意味単位分解を、挙動不変の6抽出で完了。ロジックのサブモジュール：`chat/llm.rs`（`/llm` プロバイダ切替・モデル解決・チャットターン送信 — `parse_llm_switch_command`・`active_llm_model`・`resolve_llm_default_model`・`send_chat_input_to_llm`・`live_ollama_instances`・`LlmSwitchCmd`）、`chat/ticker.rs`（銘柄のロード/切替/リロード＋自動リロード通知 — `load_ticker_analysis`・`apply_ticker_*`・`reload_chat_tickers`・`autoreload_*`・`notice_indicator_summary`・`LoadedTickerContext`・`TickerSwitchData`）、`chat/news_extra.rs`（`/news-extra` バッファ — `handle_news_extra_command`・`build_news_extra_inject_text`・`show_news_filtered_llm`・`NewsExtraEntry`）、`chat/run.rs`（`run_chat_loop`：対話REPLとスラッシュコマンド振り分け、`chat::run_chat_loop` として再公開）、`chat/prompt.rs`（2つ目の `impl ChatSession` ブロック＝プロンプト/コンテキスト構築一式：通常チャット・`/criticize`・Council/Facilitator・`/prompt` ビルダー、21メソッド）。ユニットテストは `chat/tests.rs` へ移動（`#[cfg(test)] mod tests;`）。移動した自由項目とプロンプトメソッドは `pub(super)` 公開、親が構築/分解する越境構造体のフィールドも `pub(super)`。`chat/mod.rs` は 5,481 → 855 行に減少（`ChatSession` 定義・共通型・小さな自由関数ヘルパのみ）。バイト等価の `sed` 移動で挙動の変更なし（ユニット211＋統合10テスト通過、`clippy -D warnings` クリーン）。

### Docs
- Bumped version metadata to v1.6.8 across `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/dev-prog/source-map.md`, `docs/manual/command-reference.md`, `docs/manual/setup.md`, and `docs/manual/strategy-guide.md`. Added `chat/prompt.rs`, `chat/run.rs`, `chat/llm.rs`, `chat/ticker.rs`, `chat/news_extra.rs`, and `chat/tests.rs` to the `source-map.md` submodule list.
  バージョン表記を v1.6.8 に統一（`Cargo.toml`・`Cargo.lock`・`README.md`・`docs/dev-prog/source-map.md`・`docs/manual/command-reference.md`・`docs/manual/setup.md`・`docs/manual/strategy-guide.md`）。`source-map.md` のサブモジュール一覧に `chat/prompt.rs`・`chat/run.rs`・`chat/llm.rs`・`chat/ticker.rs`・`chat/news_extra.rs`・`chat/tests.rs` を追加。

---

## [1.6.7] — 2026-06-07

### Added
- `src/chat/mod.rs`: The autoreload notice now shows a second, indented line with the enabled indicators' readings (always-on `RSI` / `MACD` plus each enabled extension: `EMA`/`SMA` short-vs-long, Bollinger `%B`, `ROC`, `ADX`, Stochastics `K/D`, Fibonacci 50%, `VWAP`, Ichimoku Tenkan/Kijun). Values are read directly from `TechnicalDataGuard` at reload time (SOT — never re-parsed from display text or invented). `LoadedTickerContext` carries the summary, refreshed into the session on each reload.
  `src/chat/mod.rs`: 自動リロード通知に、有効化した指標の読み値を2行目（インデント）として表示するようにした（常時の `RSI` / `MACD` ＋ 有効な各拡張指標：`EMA`/`SMA` の短期長期、ボリンジャー `%B`、`ROC`、`ADX`、ストキャス `K/D`、フィボナッチ50%、`VWAP`、一目の転換/基準）。値はリロード時に `TechnicalDataGuard` から直接取得する（SOT — 表示テキストの再パースや創作はしない）。`LoadedTickerContext` がサマリを運び、リロードごとにセッションへ反映する。

### Changed
- `src/chat/`: Continued the semantic decomposition of `chat/mod.rs` (the largest remaining file). `chat/report.rs` extracted — token/context reporting (`print_token_summary`, `print_chat_status`, `autoreload_status_text`, `localized_on_off` / `localized_yes_no`). The `ContextSizeBreakdown` struct and the `yes_no` helper remain in `chat/mod.rs` (used elsewhere); `report.rs` reaches them via descendant-module access. Behavior-unchanged `sed` move.
  `src/chat/`: 最大の残存ファイル `chat/mod.rs` の意味単位分解を継続。`chat/report.rs` を抽出 — token/context レポート（`print_token_summary`・`print_chat_status`・`autoreload_status_text`・`localized_on_off` / `localized_yes_no`）。`ContextSizeBreakdown` 構造体と `yes_no` ヘルパは他所でも使うため `chat/mod.rs` に残置（`report.rs` から子モジュールアクセス）。挙動不変の `sed` 移動。

### Fixed
- `src/chat/mod.rs`: Auto-reload did not fire on its own. The chat loop checked the reload deadline only at the top of the loop, which was reached only after the blocking `rustyline` readline returned — i.e. after the user submitted a line — so during idle the timer elapsed but nothing reloaded (the `/status` countdown still showed a number, masking it). The input wait now races the reload timer via `tokio::select!`: the editor runs on `spawn_blocking` (ownership returned with each line), and on a timer tick the reload fires autonomously and its notice is printed above the active prompt with a `rustyline` `ExternalPrinter` (no line corruption).
  `src/chat/mod.rs`: 自動リロードが自律発火していなかった不具合を修正。リロード期限の判定がループ先頭でしか行われず、そこへ到達するのはブロッキングな `rustyline` の readline が返った後＝ユーザーが入力を送信した後だったため、アイドル中はタイマーが経過しても何もリロードされなかった（`/status` のカウントダウンは数値を表示し続け、問題を覆い隠していた）。入力待ちを `tokio::select!` でリロードタイマーと競合させるように変更：エディタを `spawn_blocking` で実行（各入力で所有権を返す）し、タイマー発火時はリロードを自律実行して通知を `rustyline` の `ExternalPrinter` でプロンプト上に表示する（行を壊さない）。
- `src/chat/report.rs`: aligned the `/status` response-style row labels with their command names (`tone`→`answer-tone`, `hypothesis`→`hypothesis-mode`, `news`→`news-sensitivity`, `shape`→`response-shape`, `depth`→`read-depth`, `forecast`→`forecast-mode`), so the displayed `key=value` maps directly to the `/key value` command and no mental translation is needed.
  `src/chat/report.rs`: `/status` の応答スタイル行のラベルをコマンド名に一致させた（`tone`→`answer-tone`、`hypothesis`→`hypothesis-mode`、`news`→`news-sensitivity`、`shape`→`response-shape`、`depth`→`read-depth`、`forecast`→`forecast-mode`）。表示の `key=value` がそのまま `/key value` コマンドに対応し、ユーザーの脳内変換が不要になった。
- `src/chat/mod.rs`: Fixed a CI `clippy::collapsible_match` failure (`cargo clippy -- -D warnings` on Rust stable 1.96.0) in `build_council_summary_prompt` — a latent lint not flagged by the older local toolchain. The inner `if current_q.is_some()` was folded into the `match` arm as a guard; behavior unchanged (the `_ => {}` arm covers the `None` case). Local toolchain bumped to match CI.
  `src/chat/mod.rs`: CI の `clippy::collapsible_match` 失敗（Rust stable 1.96.0 の `cargo clippy -- -D warnings`）を `build_council_summary_prompt` で修正。古いローカル toolchain では出ていなかった潜在 lint。内側の `if current_q.is_some()` を `match` アームのガードに畳み込み（挙動不変、`None` は `_ => {}` で従来どおり何もしない）。ローカル toolchain を CI に合わせて更新。

### Docs
- Bumped version metadata to v1.6.7 across `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/dev-prog/source-map.md`, `docs/manual/command-reference.md`, `docs/manual/setup.md`, and `docs/manual/strategy-guide.md`. Added `chat/report.rs` to the `source-map.md` submodule list.
  バージョン表記を v1.6.7 に統一（`Cargo.toml`・`Cargo.lock`・`README.md`・`docs/dev-prog/source-map.md`・`docs/manual/command-reference.md`・`docs/manual/setup.md`・`docs/manual/strategy-guide.md`）。`source-map.md` のサブモジュール一覧に `chat/report.rs` を追加。
- `README.md`: Reworked Quick Start for binary distribution — download from Releases, then `xoksa --init` / `xoksa -t <ticker>` — instead of `git clone` + `cargo run` (building from source is now a one-line pointer for developers). Removed all `cargo run` invocations from the user manuals (`docs/manual/ollama-guide.md` → `xoksa …`). Added the chat mode (both entry paths) to the design-philosophy §2 runtime flow chart (EN + JA).
  `README.md`: Quick Start をバイナリ配布前提に刷新 — Releases からダウンロード → `xoksa --init` / `xoksa -t <ticker>`（`git clone`＋`cargo run` をやめ、ソースビルドは開発者向け1行ポインタに）。利用者向けマニュアルから `cargo run` を全廃（`docs/manual/ollama-guide.md` → `xoksa …`）。design-philosophy §2 の実行フローチャートにチャットモード（2つの入口）を追加（EN + JA）。

---

## [1.6.6] — 2026-06-07

### Changed
- `src/chat.rs` → `src/chat/`: Decomposed the ~6.7k-line chat module into a module directory by behavior-preserving extraction (`sed` byte-identical moves; `git mv` preserves history). `chat/guard.rs` (constraint/forecast/`criticize` text), `chat/help.rs` (`/help`), `chat/council.rs` (Council/Board), and `chat/debate.rs` (`/criticize` helpers) were split out; `ChatSession` and `run_chat_loop` remain in `chat/mod.rs` (6688 → 5671 lines). No `pub(crate)` changes were needed — submodules reach the parent's private items via descendant access. No behavioral change.
  `src/chat.rs` → `src/chat/`: 約6,700行の chat モジュールを、挙動を保ったままモジュールディレクトリへ分解（`sed` によるバイト単位移動・`git mv` で履歴保持）。`chat/guard.rs`（制約/予測/`criticize` テキスト）・`chat/help.rs`（`/help`）・`chat/council.rs`（Council/Board）・`chat/debate.rs`（`/criticize` 補助）を分離し、`ChatSession` と `run_chat_loop` は `chat/mod.rs` に残置（6688 → 5671 行）。`pub(crate)` 化は不要（子モジュールは親の private 項目へアクセス可能）。挙動の変更なし。

### Docs
- `README.md`: Added an owner-voice positioning lead at the top — "most AI stock tools give a one-way answer; XOKSA lets you put your own thinking to the AI, with rich customization and your choice of LLM," tied to open-source culture (freedom / no black box / MIT) — so the product's distinctive stance is clear up front (and the design-philosophy docs no longer read as detached). Refreshed Key Features to showcase the full product (SOT/anti-hallucination, tunable indicators, JP/US fundamentals, all timeframes incl. `60m`, cloud+local LLMs, the chat workbench beyond Council, and the trust/security posture) instead of over-indexing on chat Council mode. Rewrote the Product Uniqueness section: §1 "Customization That Changes the Outcome" (the full breadth of tunable parameters, not just RSI) and a new §5 "Interactive, Multi-LLM Collaboration" (chat, Council deliberation on a user-set theme, local-LLM benchmarking). Linked the three previously-unlisted manuals (`chatmode.md`, `ollama-guide.md`, `integration-guide.md`) so every manual is discoverable (EN + JA).
  `README.md`: 冒頭にオーナーの主張（位置づけ）を追加 — 「多くのAI株分析ツールは一方的な答えを返すだけ。XOKSA はあなたの考えをAIに直接ぶつけられ、豊富なカスタマイズとLLMの自由選択ができる」をOSS文化（自由・ブラックボックスでない・MIT）と接続——プロダクトの立ち位置を最初に明示（設計思想ドキュメントが浮かないように）。Key Features を刷新し、Council モード偏重をやめてプロダクト全体（SOT/ハルシネーション抑止・調整可能な指標・日米ファンダ・`60m` 含む全足・クラウド/ローカルLLM・Council を超えたチャット作業台・信頼性/セキュリティ）を魅力的に提示。「プロダクトの独自性」も改稿：§1「カスタマイズが結果を変える」（RSIだけでない調整可能パラメータの全体像）と新 §5「対話と複数LLMの協調」（チャット・ユーザー設定テーマでのCouncil会議・ローカルLLMベンチマーク）。未掲載だった3マニュアル（`chatmode.md`・`ollama-guide.md`・`integration-guide.md`）をリンクし、全マニュアルを到達可能にした（EN + JA）。
- `docs/manual/strategy-guide.md`: Refreshed §2 "Interactive Strategy Exploration with Chat Mode" to cover the current chat workbench as a recipe-validation tool — `/read-depth`, `/forecast-mode`, `/debate`, `/council`, multi-ticker comparison, and `--fundamental` — instead of just `/mode` / `/llm` / `/reload`. Added recipe ⑪ "[Value] Fundamental Check with Technical Timing" (PER/PBR/ROE via `--fundamental` with an EMA/SMA timing overlay) and rebranded the section from 10 to 11 recipes (EN + JA).
  `docs/manual/strategy-guide.md`: §2「チャットモードで戦略をインタラクティブに試す」を刷新し、現行のチャット作業台をレシピ検証手段として記載 — `/read-depth`・`/forecast-mode`・`/debate`・`/council`・複数銘柄比較・`--fundamental` を追加（従来は `/mode`・`/llm`・`/reload` のみ）。レシピ⑪「【バリュー】ファンダメンタル確認＋テクニカルのタイミング」（`--fundamental` の PER/PBR/ROE ＋ EMA/SMA タイミング）を追加し、レシピ数を10→11に変更（EN + JA）。
- `docs/manual/indicator-guide.md`: Added a "Fundamental Metrics" section explaining PER / PBR / ROE / EPS / BPS / dividend — what is fetched from the filing vs. computed in Rust (PER = price/EPS, PBR = price/BPS, ROE = net income/equity, zero-denominator guarded), how to read each, and key caveats (not part of the composite score; price-derived ratios are point-in-time and not recomputed on `/reload`; J-Quants vs SEC EDGAR differences) (EN + JA).
  `docs/manual/indicator-guide.md`: 「ファンダメンタル指標」節を追加し PER / PBR / ROE / EPS / BPS / 配当 を解説 — 決算から取得する値と Rust で計算する値の区別（PER＝価格/EPS、PBR＝価格/BPS、ROE＝純利益/自己資本、ゼロ除算ガード）、各指標の読み方、注意点（スコア合成に含めない／価格由来比率は時点固定で `/reload` 再計算なし／J-Quants と SEC EDGAR の差異）を記載（EN + JA）。
- Bumped version metadata to v1.6.6 across `Cargo.toml`, `README.md`, `docs/dev-prog/source-map.md`, `docs/manual/command-reference.md`, `docs/manual/setup.md`, and `docs/manual/strategy-guide.md`. Updated `source-map.md` to document the new `chat/` submodule layout and `design-philosophy.md` §8 to note the chat decomposition.
  バージョン表記を v1.6.6 に統一（`Cargo.toml`・`README.md`・`docs/dev-prog/source-map.md`・`docs/manual/command-reference.md`・`docs/manual/setup.md`・`docs/manual/strategy-guide.md`）。`source-map.md` に `chat/` サブモジュール構成、`design-philosophy.md` §8 に chat 分解を追記。

---

## [1.6.5] — 2026-06-04

### Added
- `src/chat.rs`: Added two chat-mode response axes. `/read-depth [low|mid|high]` (default `mid`) controls how far the LLM interprets the confirmed data — `mid`/`high` forbid self-evident "if X then Y" tautologies and require mechanism-based, value-grounded explanation; no new numbers are introduced. `/forecast-mode [off|soft|bold]` (default `soft`) controls forecasting — `soft` gives a near-term directional outlook, `bold` gives concrete projections (one hour / tomorrow / next week) labeled as predictions. In every mode the computed confirmed values are never rewritten; forecasts are forward projections grounded on those values. The forecast clause is injected at guard priority; interpretation depth is a style directive.
  `src/chat.rs`: チャットモードに応答軸を2つ追加。`/read-depth [low|mid|high]`（既定 `mid`）は確定データの解釈の踏み込みを制御し、`mid`/`high` は「もし〜なら」の自明な仮定形を禁じ、数値根拠に基づく機序の説明を求める（新たな数値は持ち込まない）。`/forecast-mode [off|soft|bold]`（既定 `soft`）は将来予測を制御し、`soft` は近い将来の方向性、`bold` は1時間後・明日・来週などの具体的予測を「予測」と明示して提示する。いずれのモードでも計算済み確定値は書き換えず、予測はその確定値を根拠とした外挿である。forecast 句はガード優先で注入し、解釈の踏み込みはスタイル指示として扱う。
- `src/config.rs`: The six chat response-style axes are now configurable from `xoksa.env` as session defaults: `ANSWER_TONE`, `HYPOTHESIS_MODE`, `NEWS_SENSITIVITY`, `RESPONSE_SHAPE`, `READ_DEPTH`, `FORECAST_MODE` (validated, fall back to defaults). `/command` still overrides at runtime. `--init` (`src/setup.rs`) and `xoksa.env.sample` now write these as commented entries, and also fill prior `--init` gaps: indicator calculation parameters (periods/multipliers/eps), Ollama advanced options (`OLLAMA_TIMEOUT_SECONDS` / `OLLAMA_BENCH_*` / `OLLAMA_DEBUG`), and `EXTRA_NOTE`.
  `src/config.rs`: チャット応答スタイル6軸を `xoksa.env` のセッション既定として設定可能にした：`ANSWER_TONE`・`HYPOTHESIS_MODE`・`NEWS_SENSITIVITY`・`RESPONSE_SHAPE`・`READ_DEPTH`・`FORECAST_MODE`（検証つき・不正値は既定へ）。`/command` による実行時上書きは従来どおり。`--init`（`src/setup.rs`）と `xoksa.env.sample` がこれらをコメント付きで出力し、従来の `--init` 抜け（指標計算パラメータ＝期間/乗数/eps、Ollama詳細＝`OLLAMA_TIMEOUT_SECONDS`/`OLLAMA_BENCH_*`/`OLLAMA_DEBUG`、`EXTRA_NOTE`）も補完した。

### Removed
- Removed unused legacy code (no callers since v1.3.0): the `OPENAI_EXTRA_NOTE` env alias (use `EXTRA_NOTE`), `AnalysisMode::is_intraday_30m()` (use `is_intraday()`), the `MarketQuote` type alias (use `MarketLatestObservation`), and the deprecated `set_quote_*` / `get_quote_*` accessors on the technical data guard (use the `*_latest_observed_*` / `*_market_data_latest_*` methods). No behavioral change.
  未使用のレガシーコードを削除（v1.3.0 以降呼び出しなし）：`OPENAI_EXTRA_NOTE` env 別名（`EXTRA_NOTE` を使用）、`AnalysisMode::is_intraday_30m()`（`is_intraday()` を使用）、`MarketQuote` 型別名（`MarketLatestObservation` を使用）、テクニカルデータガードの deprecated な `set_quote_*` / `get_quote_*` アクセサ（`*_latest_observed_*` / `*_market_data_latest_*` を使用）。挙動の変更なし。

### Changed
- `src/market.rs`, `src/main.rs`, `src/chat.rs`: Introduced `PriceFetcherKind` enum dispatch and a `build_price_fetcher()` factory behind the existing `PriceFetcher` trait. Market-data provider construction is now centralized to a single extension point, so a future provider (e.g. a brokerage API) can be added with one enum variant, one trait impl, and one factory arm — without touching call sites. No behavioral change: Yahoo Finance remains the sole provider, and `MarketDataSnapshot` is still adopted whole from a single provider (one analysis, one provider) to preserve SOT consistency.
  `src/market.rs`・`src/main.rs`・`src/chat.rs`: 既存の `PriceFetcher` トレイトの背後に `PriceFetcherKind` enum dispatch と `build_price_fetcher()` ファクトリを導入。市場データプロバイダの構築を単一の拡張点に集約し、将来のプロバイダ（例：証券会社API）を enum バリアント1つ・トレイト実装1つ・ファクトリのアーム1つの追加だけで、呼び出し側を変更せずに足せるようにした。挙動は不変：Yahoo Finance が唯一のプロバイダのままであり、SOT 一貫性のため `MarketDataSnapshot` は引き続き単一プロバイダから丸ごと採用する（1分析につき1プロバイダ）。

### Docs
- `docs/dev-prog/design-philosophy.md`: Added §8.1 "Data Source Abstraction (Provider Seam)" documenting the trait seam, enum-dispatch rationale, and the one-analysis-one-provider SOT rule (EN + JA).
  `docs/dev-prog/design-philosophy.md`: §8.1「データ源の抽象化（プロバイダの縫い目）」を追加。トレイトの縫い目・enum dispatch 採用理由・1分析1プロバイダの SOT 原則を記載（EN + JA）。
- Version metadata bumped to v1.6.5 across `Cargo.toml`, `README.md`, `docs/dev-prog/source-map.md`, `docs/manual/command-reference.md`, `docs/manual/setup.md`, and `docs/manual/strategy-guide.md` (the v1.6.4 bump had left several files at v1.6.3).
  `Cargo.toml`・`README.md`・`docs/dev-prog/source-map.md`・`docs/manual/command-reference.md`・`docs/manual/setup.md`・`docs/manual/strategy-guide.md` のバージョン表記を v1.6.5 に統一（v1.6.4 のバンプで一部が v1.6.3 のまま取り残されていた）。
- Documented the forecasting capability as a tool function grounded on computed confirmed values: `CLAUDE.md` (principle note), `docs/dev-prog/security-design.md` (integrity: forecasting does not alter SOT), `docs/dev-prog/design-philosophy.md` §7.1, and the `/read-depth` / `/forecast-mode` command tables in `docs/manual/command-reference.md` and `docs/manual/chatmode.md` (EN + JA).
  将来予測を「計算済み確定値を根拠とするツール機能」として明文化：`CLAUDE.md`（原則注記）・`docs/dev-prog/security-design.md`（完全性：将来予測はSOTを改変しない）・`docs/dev-prog/design-philosophy.md` §7.1、および `docs/manual/command-reference.md`・`docs/manual/chatmode.md` のコマンド表に `/read-depth`・`/forecast-mode` を追記（EN + JA）。
- `docs/dev-prog/design-philosophy.md`: Added §4.5 documenting why fundamental data (`FundamentalData`) intentionally has no `TechnicalDataGuard`-style wrapper — finite validation happens at the fetch/parse boundary and the struct is build-once-read-many, so there is no incremental-write surface to guard (EN + JA).
  `docs/dev-prog/design-philosophy.md`: ファンダメンタルデータ（`FundamentalData`）が `TechnicalDataGuard` 型ラッパーを意図的に持たない理由を §4.5 として明文化 — 有限性検証は取得・パース境界で行われ、構造体は build-once-read-many のためガードすべき逐次書き込み面が無い（EN + JA）。
- `docs/dev-prog/reliability.md`: New document consolidating data-source priority, failure/fallback behavior (market retries, LLM retries, news/`--no-news`), and reproducibility/determinism scope. Linked from README and design-philosophy (EN + JA).
  `docs/dev-prog/reliability.md`: データソース優先順位・失敗時/フォールバック挙動（市場リトライ・LLMリトライ・ニュース/`--no-news`）・再現性/決定性の範囲を集約した新規ドキュメント。README と design-philosophy からリンク（EN + JA）。

### Fixed
- `.github/workflows/ci.yml`: CI triggered only on the `xoksa-dev-main` branch, so pull requests to `main` (and feature branches) never ran fmt/clippy/test. Triggers are now `push` on `main` / `xoksa-dev-**` and `pull_request` on `main`, so the required checks actually run before merge. Also applied `cargo fmt` across the codebase (the previously dormant CI had let formatting drift).
  `.github/workflows/ci.yml`: CI が `xoksa-dev-main` ブランチでしか発火せず、`main` へのプルリクエスト（およびフィーチャーブランチ）で fmt/clippy/test が走っていなかった。トリガーを `push`＝`main`/`xoksa-dev-**`、`pull_request`＝`main` に修正し、マージ前に必須チェックが実際に走るようにした。あわせて `cargo fmt` をコードベース全体に適用（CI が休眠していた間に書式が乖離していた）。

---

## [1.6.4] — 2026-06-04

### Added
- `src/config.rs`: Added `Intraday60m` analysis mode. `--analysis-mode 60m` (aliases: `1h`, `hourly`) uses `interval=60m`, `range=2mo`. Auto-reload fires every 60 minutes in chat mode.
  `src/config.rs`: 1時間足分析モード `Intraday60m` を追加。`--analysis-mode 60m`（別名: `1h`, `hourly`）は `interval=60m`, `range=2mo` を使用。チャットモードの自動リロードは60分ごとに発火する。

### Changed
- `src/chat.rs`: Replaced self-built crossterm `LineEditor` with `rustyline` (v14). Eliminates the raw-mode / Japanese IME conflict that caused romaji leakage and phantom newlines on macOS and Windows. Cross-platform: Windows TSF/IMM32 and macOS IME are now handled by the library. Input history, Ctrl+A/E/U/W/L, and arrow-key navigation are preserved as rustyline built-in bindings.
  `src/chat.rs`: crossterm 自前実装の `LineEditor` を `rustyline`（v14）へ置き換え。raw mode と日本語IMEの競合（ロー文字の混入・改行の誤挿入）を根本解消。クロスプラットフォーム対応: Windows（TSF/IMM32）・macOS IMEをライブラリが処理する。入力履歴・Ctrl+A/E/U/W/L・矢印キーナビゲーションは rustyline 組み込みバインディングで維持。

### Fixed
- `src/chat.rs`: `build_analysis_prompt` was called with hardcoded `None` for `fundamental_data` in chat mode; `--fundamental` data now correctly reaches the LLM prompt.
  `src/chat.rs`: チャットモードで `build_analysis_prompt` に `fundamental_data` が渡されず `None` ハードコードになっていたバグを修正。`--fundamental` データが正しく LLM プロンプトに渡されるようになった。
- `src/llm.rs`: `--no-llm` flag was not respected by `send_gemini_prompt` and `send_claude_prompt`; both now return early when `config.no_llm` is set.
  `src/llm.rs`: `--no-llm` フラグが `send_gemini_prompt` と `send_claude_prompt` で無視されていたバグを修正。両関数が `config.no_llm` を確認して早期リターンするよう修正。
- `src/llm.rs`: `gemini_request` and `claude_request` made `MAX_RETRIES + 1` total API calls when all attempts returned 429/503; the trailing unconditional request was removed.
  `src/llm.rs`: 全試行が 429/503 を返した場合に `gemini_request` / `claude_request` が `MAX_RETRIES + 1` 回のAPI呼び出しを行っていたバグを修正。ループ末尾の無条件リクエストを削除。
- `src/fundamental.rs`: `JQUANTS_REFRESH_TOKEN` was embedded in reqwest transport-error messages via `{e}` formatting; the error message now suppresses the URL to prevent token leakage.
  `src/fundamental.rs`: reqwest の輸送エラーメッセージに `{e}` フォーマットで `JQUANTS_REFRESH_TOKEN` が URL ごと含まれていたセキュリティバグを修正。エラーメッセージから URL を除外するよう変更。
- `src/fundamental.rs`: `is_single_year_period` returned `true` (pass) when either date string failed to parse, admitting multi-year entries; now returns `false` to exclude them.
  `src/fundamental.rs`: `is_single_year_period` で日付文字列のパースに失敗した場合に `true`（通過）を返し、複数年エントリが混入するバグを修正。パース失敗時は `false` を返してエントリを除外するよう変更。
- `src/fundamental.rs`: `dividend_is_forecast` was evaluated by a second independent `parse_f64("DivAnn")` call; now shares the same `div_actual` binding as `dividend` to ensure consistent fallback behaviour.
  `src/fundamental.rs`: `dividend_is_forecast` が `parse_f64("DivAnn")` を独立して再評価していたバグを修正。`dividend` と同じ `div_actual` 変数を共有し、フォールバック挙動を一致させた。
- `src/chat.rs`: `news_extra_pending_text` was cleared before the LLM future was polled; Ctrl+C cancellation silently consumed the one-shot injection. The pending text is now restored on cancellation.
  `src/chat.rs`: `news_extra_pending_text` が LLM の future をポーリングする前にクリアされていたため、Ctrl+C キャンセル時にワンショットインジェクションが無告知で消失するバグを修正。キャンセル時にペンディングテキストを復元するよう変更。
- `src/output.rs`: `--data-append` silently appended rows with a different column count when new volume columns were added, corrupting existing CSV logs. A schema check now warns and skips the write when column counts differ.
  `src/output.rs`: 新しい volume 列追加後に `--data-append` が列数の異なる行を無告知で追記し、既存 CSV ログを破損するバグを修正。列数が一致しない場合は警告してスキップするスキーマチェックを追加。

### Docs
- `docs/manual/command-reference.md`, `docs/manual/chatmode.md`: Added `60m` / `1h` / `hourly` to `--analysis-mode`, `/mode`, and `/show t` descriptions (EN + JA).
  `docs/manual/command-reference.md`・`docs/manual/chatmode.md`: `--analysis-mode`・`/mode`・`/show t` の説明に `60m` / `1h` / `hourly` を追記（EN + JA）。

---

## [1.6.3] — 2026-05-30

### Added
- `src/config.rs`, `src/market.rs`: Added higher-timeframe analysis modes. `--analysis-mode weekly` (aliases: `week`, `1wk`) uses `interval=1wk`, `range=2y`; `--analysis-mode monthly` (aliases: `month`, `1mo`) uses `interval=1mo`, `range=10y`
  `src/config.rs`・`src/market.rs`: 上位足分析モードを追加。`--analysis-mode weekly`（別名: `week`, `1wk`）は `interval=1wk`, `range=2y`、`--analysis-mode monthly`（別名: `month`, `1mo`）は `interval=1mo`, `range=10y` を使用
- `src/chat.rs`: Chat `/mode` and `/show t` now accept `weekly` and `monthly`; switching modes still auto-reloads loaded tickers, and `/show t <mode>` still fetches on demand without changing the active session mode
  `src/chat.rs`: チャットの `/mode` と `/show t` が `weekly` / `monthly` に対応。`/mode` の切り替え時はロード済み銘柄を自動再取得し、`/show t <mode>` は従来どおりセッションの足を変更せずオンデマンド取得する
- `src/llm.rs`, `src/render.rs`: Weekly/monthly modes now display explicit analysis-mode metadata and use higher-timeframe prompt headings instead of daily/intraday wording
  `src/llm.rs`・`src/render.rs`: 週足/月足モードでは分析モードのメタ情報を明示表示し、日足・分足向けではない上位足用のプロンプト見出しを使用

### Changed
- Indicator formulas, thresholds, weights, and buy/sell scoring logic remain unchanged for weekly/monthly modes; only the data interval/range, labels, and prompt timeframe context change
  週足/月足モードでも、指標計算式・しきい値・重み・売買スコア判定ロジックは変更しない。変更対象はデータ取得の interval/range、表示ラベル、プロンプト上の時間軸文脈のみ
- `src/setup.rs`, `xoksa.env.sample`: Setup wizard and sample environment now include `weekly` / `monthly` as valid chat analysis modes
  `src/setup.rs`・`xoksa.env.sample`: セットアップウィザードとサンプルenvに、チャット分析足として `weekly` / `monthly` を追加
- Clippy cleanup: `cargo clippy --all-targets -- -D warnings` now passes after reducing existing warning patterns such as needless borrowing, duplicated branch structure, and oversized argument lists via small local structs
  Clippy対応: 既存の警告パターン（不要なborrow、重複分岐、引数過多）を小さなローカル構造体化などで整理し、`cargo clippy --all-targets -- -D warnings` が通過する状態にした

### Docs
- `docs/manual/command-reference.md`, `docs/manual/chatmode.md`, `docs/manual/indicator-guide.md`, `docs/manual/investor-guide.md`, `docs/manual/strategy-guide.md`, `docs/dev-prog/source-map.md`: Documented weekly/monthly modes, interval/range values, bar interpretation, VWAP period behavior, and chat command usage
  `docs/manual/command-reference.md`・`docs/manual/chatmode.md`・`docs/manual/indicator-guide.md`・`docs/manual/investor-guide.md`・`docs/manual/strategy-guide.md`・`docs/dev-prog/source-map.md`: 週足/月足モード、interval/range、足の解釈、VWAP期間の扱い、チャットコマンドでの使い方を追記

---

## [1.6.2] — 2026-05-29

### Refactored — Codebase-wide refactoring to address design-philosophy violations

- `src/technical/types.rs`: Added `SignalStrength` enum (`StrongBuy`/`Buy`/`Neutral`/`Sell`/`StrongSell`) with `from_f64()` / `to_f64()` conversions; added `get_signal_strength()` to `TechnicalDataGuard` — eliminates float literal pattern matching in render layer
  `src/technical/types.rs`: `SignalStrength` enum（`StrongBuy`/`Buy`/`Neutral`/`Sell`/`StrongSell`）を追加、`from_f64()` / `to_f64()` 変換メソッドおよび `get_signal_strength()` を `TechnicalDataGuard` に追加 — renderレイヤーでのfloatリテラルマッチングを廃止
- `src/render.rs`: Replaced float literal `match score` with `match signal_strength` on `SignalStrength` enum — coupling between indicators.rs score values and render descriptions is now type-enforced
  `src/render.rs`: `match score`（floatリテラル）を `SignalStrength` enumの `match signal_strength` に置き換え — 指標スコアと表示説明の対応が型レベルで保証されるようになった
- `src/render.rs`: Extracted `verdict_mark_and_text(percent, is_buy_direction, lang)` helper — replaced 4 near-identical Buyer/Seller match blocks (threshold values now defined in one place)
  `src/render.rs`: `verdict_mark_and_text(percent, is_buy_direction, lang)` helperを抽出 — ほぼ同一のBuyer/Seller matchブロック4つを統合（閾値が1箇所で管理されるようになった）
- `src/render.rs`: Split `compose_final_score_lines_stance` (was 185 lines, mixed Buyer/Seller and Holder logic) into `compose_buyer_seller_lines` + `compose_holder_lines` — main function is now a dispatcher
  `src/render.rs`: `compose_final_score_lines_stance`（185行・Buyer/SellerとHolderの混在）を `compose_buyer_seller_lines` + `compose_holder_lines` に分割 — 主関数はディスパッチャーになった
- `src/llm.rs`: Replaced `bool` return from `sanitize_ollama_line` with `RemovalState` enum (`Kept`/`PartiallyFiltered`/`FullyRemoved`) — eliminates multi-state boolean and clarifies semantics in `render_guarded_ollama_section`
  `src/llm.rs`: `sanitize_ollama_line` の `bool` 戻り値を `RemovalState` enum（`Kept`/`PartiallyFiltered`/`FullyRemoved`）に置き換え — 多値booleanを廃止し、`render_guarded_ollama_section` での意味を明確化
- `src/technical/indicators.rs`: Replaced unlabeled 4-tuple 11-arm match for signal score with named boolean predicates — each condition now states its intent in code
  `src/technical/indicators.rs`: シグナルスコアの4要素タプル11armマッチを名前付きboolean述語に置き換え — 各条件が意図をコードで明示するようになった
- `src/render.rs`: Added explanatory comments to `render_unipolar_gauge_rtl` (rounding arithmetic) and `render_bipolar_gauge_lr` (center divider index, fill asymmetry)
  `src/render.rs`: `render_unipolar_gauge_rtl`（丸め算術）と `render_bipolar_gauge_lr`（中央区切りインデックス・塗りつぶし非対称性）に説明コメントを追加

### Fixed
- `src/chat.rs`: Council round logic rewritten — removed odd/even parity checks (`% 2`) and post-loop auto-append block; replaced with explicit cycle loop `for round in 1..=rounds` where each cycle = participants in parallel → Facilitator; eliminates implicit state encoding
  `src/chat.rs`: Council ラウンドロジックを再設計 — 奇偶判定（`% 2`）とループ後の自動追加ブロックを廃止；`for round in 1..=rounds` のサイクルループに置き換え（1サイクル = 全参加者並列 → Facilitator）
- `src/chat.rs`: Facilitator is now skipped if all participant calls in a round fail (previously Facilitator ran with empty opinions)
  `src/chat.rs`: ラウンド内の全参加者呼び出しが失敗した場合、Facilitatorをスキップするように修正（従来は空の見解でFacilitatorが起動していた）
- `src/chat.rs`: Facilitator error label now shows correct round identifier (`R{n}` for mid-session, `Final` for last round); previously always showed `Final`
  `src/chat.rs`: Facilitatorエラーラベルが正しいラウンド識別子（中間: `R{n}`、最終: `Final`）を表示するように修正（従来は常に `Final` と表示されていた）
- `src/chat.rs`, `docs/manual/chatmode.md`, `docs/manual/command-reference.md`: Council cost formula updated from `ceil(N/2)×(P+1)` to `N×(P+1)`; round mechanics description updated to reflect cycle-based design
  `src/chat.rs`・`docs/manual/chatmode.md`・`docs/manual/command-reference.md`: Councilコスト計算式を `ceil(N/2)×(P+1)` から `N×(P+1)` に更新；ラウンド仕組みの説明をサイクルベース設計に合わせて修正

---

## [1.6.1] — 2026-05-28

### Added
- `src/chat.rs`: Council/Board mode — `/council set`, `/council ask [--rounds N]`, `/council facilitator`, `/council summary`, `/board`; multiple LLMs debate in parallel rounds; Facilitator synthesizes after each participant round; closing Facilitator round auto-appended when final round is a participant round; session history (`[Session History]` / `[会議履歴]`) carried across multiple `/council ask` calls via `build_session_history(before_id)`
  `src/chat.rs`: Council/Boardモード — `/council set`, `/council ask [--rounds N]`, `/council facilitator`, `/council summary`, `/board`；複数LLMが並列ラウンドで議論；Facilitatorが各ラウンドを整理；最終ラウンドが参加者の場合は最終Facilitatorラウンドを自動追加；`build_session_history(before_id)` により `/council ask` をまたいでセッション履歴を引き継ぐ
- `src/chat.rs`: Council participant prompts now include news data, role instruction, and structured output format; Facilitator prompt includes session history, news, and neutral synthesis mandate
  `src/chat.rs`: Council参加者プロンプトにニュースデータ・役割指示・出力形式を追加；Facilitatorプロンプトにセッション履歴・ニュース・中立整理指示を追加
- `src/llm.rs`: Gemini API error responses now read `error.message` from the JSON body and surface it in error output (previously silently discarded)
  `src/llm.rs`: Gemini APIエラー時にレスポンスボディの `error.message` を読み取り、エラー出力に付記するよう変更（これまではサイレント破棄）
- `docs/manual/chatmode.md`: Council Mode section added (EN/JA); Scenario Guide chapter (`chat-guide.md`) merged in as new chapter — Feature Map, scenario navigation, Council Cost Calculator `ceil(N/2)×(P+1)`, News Constraints, Recommended Combinations
  `docs/manual/chatmode.md`: Council Modeセクション追加（EN/JA）；シナリオガイド（`chat-guide.md`）を新章として統合 — 機能マップ・シナリオ別導線・コスト試算・ニュース制約・推奨組み合わせ
- `docs/manual/command-reference.md`: Council/Board commands added to chat command tables (EN/JA)
  `docs/manual/command-reference.md`: Council/Boardコマンドをチャットコマンド一覧に追加（EN/JA）

### Fixed
- `src/chat.rs`: `build_facilitator_prompt` now scopes `CouncilProposal` entries to the current ask via `e.id > user_question_id`; previously mixed proposals from previous asks on 2nd+ `/council ask`
  `src/chat.rs`: `build_facilitator_prompt` が `CouncilProposal` を `e.id > user_question_id` で現在のaskに絞るよう変更；2回目以降の `/council ask` で過去の参加者回答が混入していた不具合を修正
- `docs/manual/command-reference.md`: `/llm ollama` description corrected — alias required (not raw model name); example updated from `ollama:llama3` to `ollama:gpu1`
  `docs/manual/command-reference.md`: `/llm ollama` の説明をエイリアス必須仕様に修正；例を `ollama:llama3` から `ollama:gpu1` に変更

### Changed
- `src/chat.rs`: test `llm_cmd_model_name_with_colon` renamed to `llm_cmd_ollama_alias_with_colon` with corrected comment
  `src/chat.rs`: テスト `llm_cmd_model_name_with_colon` を `llm_cmd_ollama_alias_with_colon` に改名し、コメントを修正
- `docs/manual/chat-guide.md` removed — content merged into `chatmode.md`
  `docs/manual/chat-guide.md` を削除 — 内容を `chatmode.md` に統合

---

## [1.5.4] — 2026-05-27

### Added
- `src/keystore.rs` (new), `src/lib.rs`, `src/utils.rs`, `src/llm.rs`, `src/news.rs`, `src/fundamental.rs`, `src/setup.rs`, `Cargo.toml`: Secure Key Storage — API keys are stored in the OS Keychain (macOS Keychain / Linux Secret Service / Windows Credential Manager) as individual per-key entries (`Entry("xoksa", "<KEY_NAME>")`); 9 keys managed: `OPENAI_API_KEY`, `GEMINI_API_KEY`, `CLAUDE_API_KEY`, `BRAVE_API_KEY`, `JQUANTS_API_KEY`, `JQUANTS_EMAIL`, `JQUANTS_PASSWORD`, `JQUANTS_REFRESH_TOKEN`, `JQUANTS_ID_TOKEN`; `--init` wizard prompts for keys and writes them directly to the OS Keychain (commented-out placeholders written to xoksa.env); xoksa.env key entries take priority over the OS Keychain when present; `--doctor` reports the source of each key (xoksa.env or os-keyring); dependencies added: `keyring = "4"`, `keyring-core = "1"`
  `src/keystore.rs`（新規）, `src/lib.rs`, `src/utils.rs`, `src/llm.rs`, `src/news.rs`, `src/fundamental.rs`, `src/setup.rs`, `Cargo.toml`: セキュアキー保管機能を追加 — APIキーをOSキーチェーン（macOS Keychain / Linux Secret Service / Windows Credential Manager）にキー別エントリ（`Entry("xoksa", "<KEY_NAME>")`）として保管；管理対象9項目: `OPENAI_API_KEY`, `GEMINI_API_KEY`, `CLAUDE_API_KEY`, `BRAVE_API_KEY`, `JQUANTS_API_KEY`, `JQUANTS_EMAIL`, `JQUANTS_PASSWORD`, `JQUANTS_REFRESH_TOKEN`, `JQUANTS_ID_TOKEN`；`--init` ウィザードでキーを入力しOSキーチェーンに直接保存（xoksa.env にはコメントアウトプレースホルダーのみ記載）；xoksa.env にキーが記述されている場合は常に優先；`--doctor` でキーの保管元（xoksa.env または os-keyring）を表示；追加依存: `keyring = "4"`, `keyring-core = "1"`
- `src/setup.rs`, `src/bootstrap.rs`, `src/config.rs`: `--update-key` — interactive menu to update any individual API key stored in the OS Keychain; accepts a new value and overwrites the existing entry without re-running `--init`; supports all 9 managed keys including J-Quants v1 credentials
  `src/setup.rs`, `src/bootstrap.rs`, `src/config.rs`: `--update-key` — OSキーチェーンに保管された個別APIキーをインタラクティブメニューから更新；`--init` を再実行せずにキーを上書き保存できる；J-Quants v1認証情報を含む9項目に対応
- `src/setup.rs`, `src/bootstrap.rs`, `src/config.rs`: `--check-keys` — prints presence/source (xoksa.env or os-keyring) for all 9 managed API keys without displaying values
  `src/setup.rs`, `src/bootstrap.rs`, `src/config.rs`: `--check-keys` — 管理対象9つのAPIキーの保管状況（xoksa.env または os-keyring）を値を表示せずに確認する

### Security
- `src/keystore.rs`: `resolve_key_presence()` now delegates to `get_key()` so diagnostic reads (`--doctor`, `--check-keys`) never hold credential material in a plain `String`; the retrieved value is `Zeroizing<T>` and dropped immediately after the presence test (security-design.md Class A requirement)
  `src/keystore.rs`: `resolve_key_presence()` が `get_key()` に委譲するよう変更。`--doctor` / `--check-keys` の診断パスで認証情報が通常の `String` に保持されることがなくなった。取得した値は `Zeroizing<T>` のまま存在確認後に即ドロップする（security-design.md クラスA要件）
- `src/keystore.rs`: added `delete_key()` for rollback support in `run_init()`
  `src/keystore.rs`: `run_init()` のロールバック用に `delete_key()` を追加
- `src/setup.rs`: `run_init()` snapshots existing keychain values before any write; aborts if the snapshot read fails (preventing partial write with no rollback path); rollback failures are reported to the user instead of being silently discarded
  `src/setup.rs`: `run_init()` が書き込み前に既存キーチェーン値をスナップショット取得するよう変更。スナップショット読み取り失敗時は処理を中断し、ロールバック失敗はユーザーに報告する（サイレント破棄を廃止）
- `src/utils.rs`, `src/llm.rs`, `src/news.rs`, `src/fundamental.rs`: `resolve_api_key()` return type changed from `Option<Zeroizing<String>>` to `Result<Option<Zeroizing<String>>>` — keyring access failures are propagated as errors rather than silently treated as absent keys
  `src/utils.rs`, `src/llm.rs`, `src/news.rs`, `src/fundamental.rs`: `resolve_api_key()` の戻り値を `Option<Zeroizing<String>>` から `Result<Option<Zeroizing<String>>>` に変更。キーリングアクセス失敗をエラーとして伝播し、未設定として無視しない

### Fixed
- `src/chat.rs`: `/news-extra` now stores articles as `Vec<(title, url)>` pairs; URLs are never truncated; a boundary note is injected into every prompt that includes extra news (matching the standard news channel's SOT rules)
  `src/chat.rs`: `/news-extra` が記事を `Vec<(title, url)>` ペアで保管するよう変更。URLの切り捨てがなくなり、追加ニュースを含む全プロンプトに境界テキストを注入する（標準ニュースチャネルのSOTルールに準拠）
- `src/setup.rs`: `--doctor` J-Quants key display order now matches the runtime resolution priority (`JQUANTS_API_KEY` → `JQUANTS_ID_TOKEN` → `JQUANTS_REFRESH_TOKEN` → `JQUANTS_EMAIL`+`JQUANTS_PASSWORD`)
  `src/setup.rs`: `--doctor` のJ-Quants表示順を実行時の解決優先順位（`JQUANTS_API_KEY` → `JQUANTS_ID_TOKEN` → `JQUANTS_REFRESH_TOKEN` → `JQUANTS_EMAIL`+`JQUANTS_PASSWORD`）に合わせた
- `src/config.rs`: `--update-key` and `--check-keys` are now mutually exclusive (`conflicts_with_all`)
  `src/config.rs`: `--update-key` と `--check-keys` を相互排他（`conflicts_with_all`）に設定

---

## [1.5.3] — 2026-05-26

### Added
- `src/technical/types.rs`, `src/technical/indicators.rs`, `src/render.rs`, `src/llm.rs`, `src/output.rs`, `src/utils.rs`: Volume data (`latest_volume`, `avg_volume`, `volume_ratio`) added to SoT — extracted from Yahoo Finance OHLCV bars already fetched; average period uses `sma_long_period` (default 20 bars); displayed in terminal output and injected into LLM prompt with directional comment and no-speculation instruction; added as tail columns to CSV/JSON logs; gracefully absent when Yahoo Finance returns no volume
  `src/technical/types.rs`, `src/technical/indicators.rs`, `src/render.rs`, `src/llm.rs`, `src/output.rs`, `src/utils.rs`: 出来高データ（`latest_volume`, `avg_volume`, `volume_ratio`）をSoTに追加 — 既存 Yahoo Finance OHLCV 取得経路から抽出；平均期間は `sma_long_period`（デフォルト20本）を使用；ターミナル表示およびLLMプロンプトに方向コメント付きで注入（「推測しない」指示を付記）；CSV/JSONログに末尾列として追加；出来高なしの場合はグレースフルに非表示
- `src/chat.rs`: `/news-extra` command — circular buffer of 16 extra news slots; `/news-extra <keyword>` searches Brave News (max 256-char keyword, 1,000-char stored text per slot; writes to first empty slot when available, circular overwrite only when all full); `/news-extra inject <n>` injects a single specified slot (one-shot); `/news-extra inject all` one-shot injects all filled slots; `/news-extra inject all on` / `off` toggles always-inject mode (auto-injects all filled slots into every subsequent LLM message while ON); `/news-extra del <n>` deletes a single slot (stays as empty placeholder, no compaction); `/news-extra list` / `clear` for buffer management; requires `BRAVE_API_KEY` and `NO_NEWS=false`
  `src/chat.rs`: `/news-extra` コマンドを追加 — 16スロットの追加ニュースバッファ；`/news-extra <検索ワード>` でBrave Newsを検索（キーワード最大256文字、1スロット1,000文字；空きスロットがあれば優先的に書き込み、全スロット埋まっている場合のみ循環上書き）；`/news-extra inject <番号>` で指定スロット単体をワンショット注入；`/news-extra inject all` で全スロットをワンショット注入；`/news-extra inject all on` / `off` で常時注入モードを切り替え（ON中は以降の全メッセージに全スロットを自動注入）；`/news-extra del <番号>` でスロット単体を削除（空欄として残り詰め直しなし）；`/news-extra list` / `clear` でバッファ管理；`BRAVE_API_KEY` 設定と `NO_NEWS=false` が必要
- `src/chat.rs`: `/criticize last` now attaches data snapshot timestamps to each debate entry (`data_as_of`); criticize prompt shows the session's current data time vs. the opinion's referenced data time, enabling temporal mismatch detection
  `src/chat.rs`: `/criticize last` が各デバッフエントリにデータスナップショット時刻（`data_as_of`）を付加するよう変更；批評プロンプトに現在のセッションデータ時刻と見解参照データ時刻を併記し、データ時点差の検出を可能にした
- `src/chat.rs`: `/show t` now accepts an optional bar mode argument (`daily|5m|15m|30m`); if the specified mode differs from the current `/mode`, data is fetched on the fly without modifying the session
  `src/chat.rs`: `/show t` にオプションの足種引数（`daily|5m|15m|30m`）を追加；現在の `/mode` と異なる足が指定された場合、セッションを変更せずにオンデマンドでデータ取得・表示する
- `src/bootstrap.rs`, `src/chat.rs`, `src/main.rs`, `xoksa.env.sample`: `CHAT_DEFAULT_TICKER` env setting — comma-separated list (up to 5) of tickers loaded automatically when `--chat` is used without `--ticker`; excess tickers (>5) trigger a warning and are truncated; invalid tickers are skipped with a warning; CLI `--ticker` always takes precedence and suppresses this setting
  `src/bootstrap.rs`, `src/chat.rs`, `src/main.rs`, `xoksa.env.sample`: `CHAT_DEFAULT_TICKER` 環境変数でカンマ区切りの複数銘柄（最大5件）を指定可能；`--chat` をティッカーなしで起動した際に自動ロードする；6件以上は警告を表示し先頭5件を使用；無効銘柄はスキップ；CLIの `--ticker` が常に優先され、この設定は無視される
- `src/config.rs`: `OllamaInstance` struct and `ollama_instances` field; Ollama servers defined via `OLLAMA_N_ALIAS/HOST/PORT/MODEL` (N=1–16); lowest-numbered defined entry is the startup default; gaps allowed
  `src/config.rs`: `OllamaInstance` 構造体と `ollama_instances` フィールドを追加；Ollamaサーバは `OLLAMA_N_ALIAS/HOST/PORT/MODEL`（N=1〜16）で定義；番号が最小の定義済みエントリが起動時デフォルト；番号の抜けを許容
- `src/chat.rs`: `/llm ollama:<alias>` runtime instance switching; `ShowCurrent` displays `host:port` for Ollama; `live_ollama_instances()` re-reads xoksa.env on every call
  `src/chat.rs`: `/llm ollama:<alias>` によるOllamaインスタンスのランタイム切り替え；`ShowCurrent` がOllama使用時に `host:port` を表示；`live_ollama_instances()` が毎回 xoksa.env を再読み込み
- `src/setup.rs`: `--init` wizard collects Ollama instances and generates `OLLAMA_N_ALIAS/HOST/PORT/MODEL` entries; alias is required for all instances (no special-casing for the first)
  `src/setup.rs`: `--init` ウィザードでOllamaインスタンスを収集し `OLLAMA_N_ALIAS/HOST/PORT/MODEL` 形式で出力；全インスタンスでエイリアス必須（1台目の特別扱いを廃止）
- `xoksa.env.sample`: Ollama section updated to `OLLAMA_N_*` numbered key format
  `xoksa.env.sample`: Ollamaセクションを `OLLAMA_N_*` 番号付きキー形式に更新
- `Cargo.toml`: Added `unicode-width = "0.2"` dependency for terminal display-width calculation
  `Cargo.toml`: 端末表示幅計算のため `unicode-width = "0.2"` を追加

### Fixed
- `src/chat.rs`: `/llm ollama:<alias>` now shows an error with available alias list when the alias is not found; previously silently treated the input as a raw model name
  `src/chat.rs`: `/llm ollama:<alias>` でエイリアスが見つからない場合、利用可能なエイリアス一覧を表示してエラーにするよう修正；以前は入力をモデル名として無言で通していた
- `src/chat.rs`: Chat prompt cursor position now calculated using display width (`unicode_width`) instead of character count, fixing display corruption when typing wide (2-byte) characters via IME
  `src/chat.rs`: チャットプロンプトのカーソル列位置をバイト数・文字数ではなく表示幅（`unicode_width`）で計算するよう修正；IME経由の全角文字入力時に画面が崩れる問題を解消

### Changed
- `src/chat.rs`: Chat constraint (all levels, EN+JA) now explicitly allows general definitions and explanations of technical/fundamental terms and financial concepts; previously small models over-applied the constraint and refused to explain basic terminology such as "oversold"
  `src/chat.rs`: 全チャット制約（high/mid/low × EN/JA）にテクニカル・ファンダメンタル用語・金融概念の一般的な定義説明を許可する文を追加；以前は小規模モデルが制約を過剰適用し「オーバーソールドとは」のような基本用語の説明まで拒否していた
- `src/chat.rs`: `/mode <bar>` now automatically re-fetches data for all loaded tickers on mode change; manual `/reload t` reminder removed
  `src/chat.rs`: `/mode <足>` 実行時、銘柄ロード済みであればデータを自動再取得するよう変更；手動 `/reload t` の案内を削除
- `src/chat.rs`: Chat constraint (all levels, EN+JA) now requires the LLM to include the URL of every news item it references
  `src/chat.rs`: 全チャット制約（high/mid/low × EN/JA）にニュース言及時のURL必須表示ルールを追加
- `src/chat.rs`: `/autoreload notice` output now prefixes each line with the OS local date and time (`YYYY/MM/DD HH:MM:SS`); blank line between notice outputs removed
  `src/chat.rs`: `/autoreload notice` の通知行の先頭にOSローカルの日付・時刻（`YYYY/MM/DD HH:MM:SS`）を追加；通知間の空行を削除
- `src/chat.rs`: `/autoreload notice` price diff now compares against the price from the previous reload event, not the previous market bar; label changed from `前足比` to `前回比` (EN: `vs prev reload`)
  `src/chat.rs`: `/autoreload notice` の価格差分を「前の市場バーとの比較」から「前回リロード時の価格との比較」に変更；ラベルを `前足比` → `前回比`（EN: `vs prev reload`）に変更
- `src/technical/indicators.rs`: VWAP calculation now skips bars with zero volume (e.g. illiquid intervals, pre/post-market gaps) instead of failing; only fails if no bars in the period have non-zero volume
  `src/technical/indicators.rs`: VWAP計算でvolume=0のバーをスキップするよう変更（流動性が低い時間帯・市場外時間のバーに対応）；期間内に非ゼロボリュームのバーが1本もない場合のみ失敗
- `src/chat.rs`: `/autoreload notice` now blocked when no ticker is loaded; immediately fetches and displays current prices as baseline on activation (first notice shows `---` for diff)
  `src/chat.rs`: `/autoreload notice` は銘柄未ロード時に使用不可に変更；コマンド投入直後に現在価格を即時取得・表示してベースラインを設定（初回の前回比は `---` 表示）
- `src/chat.rs`: `/criticize` classification scheme updated to 6 categories: `xoksaデータで確認可能 / xoksaデータと矛盾 / データ時点差による不一致の可能性 / 一般的なテクニカル解釈 / xoksaデータでは確認不能 / ニュース本文未取得のため本文根拠なし`; instruction added to classify temporal mismatches as "データ時点差による不一致の可能性" instead of contradiction
  `src/chat.rs`: `/criticize` 分類を6カテゴリに更新：`xoksaデータで確認可能 / xoksaデータと矛盾 / データ時点差による不一致の可能性 / 一般的なテクニカル解釈 / xoksaデータでは確認不能 / ニュース本文未取得のため本文根拠なし`；データ時点差は「矛盾」でなく「データ時点差による不一致の可能性」に分類するよう指示を追加


### Docs
- `docs/manual/setup.md`: Wizard flow table updated (install mode row, Step 6A/6B/6C); backup behavior clarified; OLLAMA_INSTANCES section added (EN+JA)
  `docs/manual/setup.md`: ウィザードフロー表を更新（インストールモード行・Step 6A/6B/6C）；バックアップ動作の説明を修正；OLLAMA_INSTANCESセクションを追加（EN+JA）
- `docs/manual/investor-guide.md`: Removed competitive comparison language; added short-interval indicator reliability section — Ichimoku/VWAP/Fibonacci degrade on 5m/15m/30m; Bollinger/ADX require different interpretive lens; RSI/MACD/EMA/SMA/ROC/Stochastics unaffected (EN+JA)
  `docs/manual/investor-guide.md`: 競合比較表現を削除；短時間足インジケーター信頼性セクションを追加（一目/VWAP/フィボナッチは信頼性低下、ボリンジャー/ADXは解釈軸が異なる、RSI等は影響なし）（EN+JA）
- `docs/manual/strategy-guide.md`: Chat mode interactive exploration subsection; bar-interval notes on recipes ②⑦⑨ (EN+JA)
  `docs/manual/strategy-guide.md`: チャットモードのインタラクティブ活用サブセクション；レシピ②⑦⑨に足の間隔注記を追加（EN+JA）
- `docs/manual/chatmode.md`: `/news-extra` section added with buffer mechanics, `inject <n>` / `inject all` usage distinction, `del <n>` for single-slot deletion, token warning, and slot/character limits (EN+JA)
  `docs/manual/chatmode.md`: `/news-extra` セクションを追加（バッファ動作、`inject <n>` / `inject all` 使い分け、`del <n>` によるスロット単体削除、トークン警告、スロット数・文字数制限）（EN+JA）
- `docs/manual/strategy-guide.md`: "Reading Volume Data" section added — explains the three displayed fields (latest, avg, ratio), all six comment patterns with thresholds and interpretation, recipe-by-recipe volume checkpoints, and data source notes (shares vs. transactions, intraday vs. daily averaging) (EN+JA)
  `docs/manual/strategy-guide.md`: 「出来高データの読み方」セクションを追加 — 3つの表示値の意味（最新・平均・倍率）、6パターンのコメントとその解釈、レシピ別の活用ポイント、データの補足（株数／件数の違い、分足と日足の平均の違い）を詳解（EN+JA）
- `docs/manual/investor-guide.md`: "Volume — What It Is and Why It Matters" section added — conceptual explanation of what volume measures (shares, not transactions or yen value), why price+volume must be read together, xoksa's three displayed values, why ratio matters more than absolute count, intraday vs. daily mode differences, data source note, cross-reference to strategy guide (EN+JA); existing sections 3 and 4 renumbered to 4 and 5
  `docs/manual/investor-guide.md`: 「出来高（Volume）— 何を測り、なぜ重要か」セクションを追加 — 出来高の概念説明（株数の合計であり件数・金額ではないこと）、価格と出来高をセットで読む理由、xoksaの3つの表示値の意味、絶対値より倍率が重要な理由、分足と日足の違い、データソース注記、ストラテジーガイドへのクロスリファレンス（EN+JA）；既存セクション3・4を4・5に繰り下げ
- `docs/manual/chatmode.md`: `/mode` description updated (auto-reload); `/autoreload notice` description updated (inter-reload diff, `前回比`); timestamp format updated to `YYYY/MM/DD HH:MM:SS`; timezone note added; blocked-when-no-ticker behavior documented (EN+JA)
  `docs/manual/chatmode.md`: `/mode` の説明を自動再取得に更新；`/autoreload notice` の説明を前回リロード比（`前回比`）に更新；タイムスタンプ形式を `YYYY/MM/DD HH:MM:SS` に更新；タイムゾーン注記追加；銘柄未ロード時の制限を記載（EN+JA）
- `README.md`: Quick Start step 2 updated to `--init` wizard; LLM engine and chat mode feature descriptions updated (EN+JA)
  `README.md`: Quick Start ステップ2を `--init` ウィザードに更新；LLMエンジン・チャットモードの機能説明を更新（EN+JA）

---

## [1.5.0] — 2026-05-20

### Added
- `src/setup.rs`: Added output language selection (Step 1) to `--init` wizard; supports `en` (default) and `ja`
  `src/setup.rs`: `--init` ウィザードに出力言語選択（Step 1）を追加；`en`（デフォルト）と `ja` に対応
- `src/setup.rs`: `--lang ja` CLI flag now pre-selects language in `--init`, skipping Step 1 interactively
  `src/setup.rs`: `--init` 実行時に `--lang ja` を指定すると言語が事前選択され、Step 1 プロンプトをスキップ

### Changed
- `src/setup.rs`: All wizard messages (Steps 2–5, banner, completion) now switch to Japanese when `ja` is selected
  `src/setup.rs`: ウィザード全メッセージ（Steps 2–5、バナー、完了メッセージ）が `ja` 選択時に日本語に切り替わるよう変更
- `src/setup.rs`: Renumbered wizard steps: LLM→Step 2, News→Step 3, Fundamental→Step 4, Proxy→Step 5
  `src/setup.rs`: ウィザードのステップ番号を変更：LLM→Step 2、ニュース→Step 3、ファンダメンタル→Step 4、プロキシ→Step 5
- Version notation unified to `v1.5.0` (SemVer) across all documentation
  全ドキュメントのバージョン表記を `v1.5.0`（SemVer）に統一

### Docs
- `docs/manual/indicator-guide.md`: New bilingual (EN/JA) document covering all 10 indicators, score synthesis, and gauge table
  `docs/manual/indicator-guide.md`: 全10指標・スコア合成・ゲージ表を網羅した英日バイリンガル文書を新規作成
- Completed bilingual (English-first, Japanese-second) conversion for all documents under `docs/` and project root
  `docs/` 配下およびプロジェクトルートの全ドキュメントを英語先・日本語後のバイリンガル形式に統一完了
- `docs/manual/setup.md`: Updated `--init` wizard step table and proxy step reference to match implementation
  `docs/manual/setup.md`: `--init` ウィザードのステップ表とプロキシ手順の参照を実装に合わせて更新

---

## [1.4.1] — 2026-05-18

### Added
- `src/setup.rs`: New implementation of `--init` setup wizard (self-contained per step, collects credentials within each step) and `--doctor` configuration diagnostics
  `src/setup.rs`: `--init` セットアップウィザード（ステップ内完結型・認証情報をステップ内で収集）および `--doctor` 設定診断を新規実装
- `utils::build_proxy()`: Added reqwest Proxy builder helper that accepts proxy_url + NO_PROXY
  `utils::build_proxy()`: proxy_url + NO_PROXY を受け取る reqwest Proxy ビルダーヘルパーを追加
- Added bidirectional sync tests between SAMPLE_DEFAULTS ↔ xoksa.env.sample (key existence, value match, reverse coverage)
  SAMPLE_DEFAULTS ↔ xoksa.env.sample 双方向同期テストを追加（キー存在・値一致・逆方向カバレッジ）

### Fixed
- `config.rs` / `market.rs` / `news.rs` / `fundamental.rs` / `llm.rs`: Applied HTTPS_PROXY, HTTP_PROXY, and NO_PROXY to all reqwest clients (previously, settings were read but not reflected in actual network communications)
  `config.rs` / `market.rs` / `news.rs` / `fundamental.rs` / `llm.rs`: HTTPS_PROXY・HTTP_PROXY・NO_PROXY を全 reqwest クライアントに適用（従来は設定を読むだけで実通信に反映されていなかった）
- `setup.rs`: Changed to write SAMPLE_DEFAULTS before the wizard configuration section. Fixed a bug where `NO_LLM=true` when selecting `llm_provider=None` was overwritten by `NO_LLM=false` due to bootstrap's last-write-wins behavior
  `setup.rs`: SAMPLE_DEFAULTS をウィザード設定セクションより前に書き出すよう変更。bootstrap の後勝ち仕様により、`llm_provider=None` 選択時の `NO_LLM=true` が `NO_LLM=false` で上書きされる不具合を修正
- Unified Gemini default model to `gemini-2.5-flash` across all code paths (`config.rs` fallback was still `gemini-2.0-flash`)
  Gemini デフォルトモデルを全コードパスで `gemini-2.5-flash` に統一（`config.rs` フォールバックが `gemini-2.0-flash` のままだった）
- Moved `xoksa.env.sample` to the project root (previously: `Binary/xoksa.env.sample`)
  `xoksa.env.sample` をプロジェクトルートに移動（旧: `Binary/xoksa.env.sample`）

### Changed
- `--init` wizard: Changed design to complete selection and credential collection within each step
  `--init` ウィザード: 各ステップで選択と認証情報収集を完結させる設計に変更
- Added host / port prompts to the Ollama step (with default value display, configurable)
  Ollama ステップに host / port プロンプトを追加（デフォルト値表示・変更可能）
- Changed proxy step to Y/N gate style
  プロキシステップを Y/N ゲート方式に変更
- Changed `OLLAMA_THINK` to commented-out in `--init` generated files
  `OLLAMA_THINK` を `--init` 生成ファイルでコメントアウトに変更
- Changed `ALIAS_CSV` to commented-out in xoksa.env.sample / SAMPLE_DEFAULTS
  `ALIAS_CSV` を xoksa.env.sample / SAMPLE_DEFAULTS でコメントアウトに変更
- Added ALIAS_CSV file existence check to `--doctor`
  `--doctor` に ALIAS_CSV ファイル存在チェックを追加

### Security
- `bootstrap.rs`: Completely eliminated `unsafe` blocks (`std::env::set_var`) and migrated to `env_map: HashMap<String, String>` design (addressing requirements for Rust 1.81 and later)
  `bootstrap.rs`: `unsafe` ブロック（`std::env::set_var`）を完全撤廃し `env_map: HashMap<String, String>` 設計に移行（Rust 1.81以降の要件対応）
- Eliminated all `unwrap()` / `expect()` calls from all production and test code
  全 production コードおよびテストコードの `unwrap()` / `expect()` を廃止

---

## [1.3.0] — 2026-05-11

### Fixed
- Changed `generate_llm_analysis`'s `todo!()` to `Err(...)` (panic removal)
  `generate_llm_analysis` の `todo!()` を `Err(...)` に変更（パニック除去）
- Changed `TechnicalDataGuard.entry` field from `pub` → `pub(crate)` (access control strengthening)
  `TechnicalDataGuard.entry` フィールドを `pub` → `pub(crate)` に変更（アクセス制御強化）
- Removed duplicate definition of `sanitize_ascii_file_lines` function (3 locations in `bootstrap.rs` / `llm.rs` / `news.rs`) → consolidated into `utils.rs`
  `sanitize_ascii_file_lines` 関数の重複定義を除去（`bootstrap.rs` / `llm.rs` / `news.rs` の3箇所）→ `utils.rs` に集約
- Removed redundant condition (`|| c == 'T'`) from `sanitize_ticker` validation
  `sanitize_ticker` バリデーションの冗長条件（`|| c == 'T'`）を削除

### Docs
- Unified version notation to v1.3.0 across all manuals
  全マニュアルのバージョン表記を v1.3.0 に統一
- Fixed Markdown corruption in `setup.md`
  `setup.md` の Markdown 破損を修正
- Updated `command-reference.md` to match `--help` output (added `--show-news` / `--llm-provider` / `--debug-prompt` / `--debug-args`; fixed `--openai-extra_note` typo)
  `command-reference.md` を `--help` 出力と一致させた（`--show-news` / `--llm-provider` / `--debug-prompt` / `--debug-args` 追記、`--openai-extra_note` 誤記修正）

---

## [1.2.0] — 2026-05-11

### Added
- **Enhanced test coverage**: 2 → 69 tests (60 unit + 9 integration)
  **テストカバレッジ強化**: 2件 → 69件（ユニット60件 + 統合9件）
  - `src/technical/indicators.rs`: Added 6 score helper functions, 36 boundary value tests
    `src/technical/indicators.rs`: スコアヘルパー6関数追加・境界値テスト36件
  - `src/technical/composite.rs`: 5 snapshot and gauge tests
    `src/technical/composite.rs`: スナップショット・ゲージテスト5件
  - `src/bootstrap.rs`: 17 sanitize function tests
    `src/bootstrap.rs`: サニタイズ関数テスト17件
  - `tests/integration_test.rs`: 9 pipeline, injection cross-cutting, and Engine construction tests
    `tests/integration_test.rs`: パイプライン・インジェクション横断・Engine構築テスト9件
- **GitHub Actions CI** (`.github/workflows/ci.yml`):
  - Automatically runs `cargo fmt -- --check` / `cargo clippy -- -D warnings` / `cargo test -q` on push/PR
    push/PR 時に自動実行
- Added **CI badge** to README
  **CI バッジ** を README に追加
- Added **Quick Start** section to README (4 lines of copy-pasteable commands)
  **Quick Start** セクションを README に追加（コピペ可能コマンド4行）
- Created **CHANGELOG.md**
  **CHANGELOG.md** 新規作成

### Fixed
- `clippy::type_complexity` — Changed `get_extension_evaluator` return type to `ExtensionEvaluator` type alias
  `clippy::type_complexity` — `get_extension_evaluator` 戻り値型を `ExtensionEvaluator` 型エイリアスに変更
- `clippy::redundant_guard` — Changed `Some(slice) if slice.is_empty()` → `Some([])`
  `clippy::redundant_guard` — `Some(slice) if slice.is_empty()` → `Some([])` に変更
- Applied `cargo fmt` to all files
  全ファイルに `cargo fmt` 適用

[Unreleased]: https://github.com/Kozo2000/XOKSA/compare/v2.2.5...HEAD
[2.2.5]: https://github.com/Kozo2000/XOKSA/compare/v2.2.4...v2.2.5
[2.2.4]: https://github.com/Kozo2000/XOKSA/compare/v2.2.3...v2.2.4
[2.2.3]: https://github.com/Kozo2000/XOKSA/compare/v2.2.2...v2.2.3
[2.2.2]: https://github.com/Kozo2000/XOKSA/compare/v2.2.0...v2.2.2
[2.2.0]: https://github.com/Kozo2000/XOKSA/compare/v2.1.0...v2.2.0
[2.1.0]: https://github.com/Kozo2000/XOKSA/compare/v2.0.0...v2.1.0
[2.0.0]: https://github.com/Kozo2000/XOKSA/compare/v1.6.9...v2.0.0
[1.6.9]: https://github.com/Kozo2000/XOKSA/compare/v1.6.8...v1.6.9
[1.6.8]: https://github.com/Kozo2000/XOKSA/compare/v1.6.7...v1.6.8
[1.6.7]: https://github.com/Kozo2000/XOKSA/compare/v1.6.6...v1.6.7
[1.6.6]: https://github.com/Kozo2000/XOKSA/compare/v1.6.5...v1.6.6
[1.6.5]: https://github.com/Kozo2000/XOKSA/compare/v1.6.4...v1.6.5
[1.6.4]: https://github.com/Kozo2000/XOKSA/compare/v1.6.3...v1.6.4
[1.6.3]: https://github.com/Kozo2000/XOKSA/compare/v1.6.2...v1.6.3
[1.6.2]: https://github.com/Kozo2000/XOKSA/compare/v1.6.1...v1.6.2
[1.6.1]: https://github.com/Kozo2000/XOKSA/compare/v1.5.4...v1.6.1
[1.5.4]: https://github.com/Kozo2000/XOKSA/compare/v1.5.3...v1.5.4
[1.5.3]: https://github.com/Kozo2000/XOKSA/compare/v1.5.0...v1.5.3
[1.5.0]: https://github.com/Kozo2000/XOKSA/compare/v1.4.1...v1.5.0
[1.4.1]: https://github.com/Kozo2000/XOKSA/compare/v1.3.0...v1.4.1
[1.3.0]: https://github.com/Kozo2000/XOKSA/releases/tag/v1.3.0
[1.2.0]: https://github.com/Kozo2000/XOKSA/releases/tag/v1.2.0
