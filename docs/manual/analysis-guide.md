# XOKSA Analysis Guide

[日本語はこちら。](#ja)

> **Language sync:** the English and Japanese sections are both authoritative. If a rule or behavior appears in only one language, it applies to both; the missing side is a documentation defect.

How to read a stock with XOKSA: the approach (for first-timers and traders), what each indicator and the total score mean, and ready-to-use strategies. Setup is in [setup.md](setup.md); every command is in [command-reference.md](command-reference.md).

<a id="en"></a>

---

# Part 1 — Reading the analysis

## 1. The Role of xoksa in Stock Investment Analysis

Stock investment analysis broadly falls into two categories: "fundamental analysis" and "technical analysis."

- **Fundamental analysis**: Assesses business value by analyzing a company's finances, performance, and news (catalysts).
- **Technical analysis**: Assesses timing context from past price and volume patterns.

xoksa integrates **"technical analysis (objective numbers)" and "latest catalysts (news)"** using an AI (LLM) brain, and delivers an interpretable report of "the market's current temperature."

> For detailed calculation methods and interpretation of each technical indicator, refer to the **Indicator Guide**.

---

## 2. Tool Characteristics and Recommended Play Style

### The Chart, and Verbalizing What's Behind It

xoksa's browser dashboard includes an interactive chart — indicator overlays, and drag-select a window to ask the AI about exactly that stretch. But out-charting the specialists isn't the goal: TradingView and the sophisticated charting of brokerage services are excellent, and xoksa doesn't try to compete with them on drawing tools. Its edge is **"verbalizing the invisible data backing"** the "visible chart" — and making it answerable on the spot. So the two roles complement rather than compete:

1. **A dedicated charting service (TradingView, your broker)**: rich visual confirmation of the overall trend, resistance lines, and patterns.
2. **xoksa**: a readable chart *plus* the logical read — how the internal numbers (RSI, MACD, news context, etc.) are interacting at that moment, put into words.

Use xoksa on its own, or alongside your favorite charting service; either way it adds depth and objectivity to analysis, aiming to be a "first officer" that quietly supports your decision-making process.

### Not a Stock Picker

xoksa never *proactively* recommends or selects stocks — a deliberate design choice, not an omission (see [Design Philosophy §6](../dev-prog/design-philosophy.md)). Its role is to make a stock you're *already looking at* legible: a readable chart, an indicator's meaning made concrete for that specific stock, and the reasoning available on the spot. It organizes the material for your judgment, and when you ask it directly it gives a grounded, conditional read — which way the data leans and what would change that — never a naked buy/sell order. The decision stays yours.

### "Ordering" Rather Than "Receiving" Answers

xoksa is designed around the philosophy that **"users themselves order (customize) the analysis logic"** rather than receiving a pre-packaged answer.

Where to set the RSI threshold, how many days for the moving average period, and whether to prioritize technical or news — dozens of parameters can be freely changed with a single command argument.

Rather than following "tool-determined evaluation," this is about **"projecting your investment philosophy (logic) onto the tool and having AI verify it as objective data."** The user always retains control over what is being analyzed and how.

### Optimizing Stock Characteristics Through Parameters

Investment strategies vary from person to person: short-term trading, swing trading, long-term investing, etc. xoksa allows you to freely change the "strictness" and "perspective" of analysis through numerous arguments.

- **When wanting to buy**: Analyze with `-s buyer` (buyer's perspective).
- **When wanting to sell**: Check warning points with `-s seller` (seller's perspective).

> For customizing analysis strategies, refer to the **Strategy Guide**.

### Note on Short-Interval Analysis (1m / 5m / 15m / 30m / 60m)

When using intraday bar modes, the statistical reliability of certain indicators changes in ways that are inherent to their design — not specific to xoksa. The shorter the interval, the more pronounced these effects are — they are strongest on **1-minute** bars (which also carry only a few days of history), and mildest on 60-minute bars.

**Indicators with significantly reduced reliability on short intervals:**

| Indicator | Reason |
| :--- | :--- |
| **Ichimoku** | xoksa computes the Tenkan-sen (9) and the Kijun-sen (26) only — there is no Senkou Span, and so no band between them. Both are calibrated for daily bars; on 5-minute bars the Kijun-sen spans about 130 minutes, which loses the medium-term equilibrium concept the indicator is designed to express. |
| **VWAP** | VWAP is designed to reset at each session open. When computed across multiple days of intraday bars, it becomes a cross-session average and loses its intraday anchoring meaning. Within a single trading session it remains valid. |
| **Fibonacci** | Fibonacci levels are derived from the high/low range of the data. On short intervals where the price range is very narrow, the resulting levels fall within noise thresholds and carry less discriminative power. |

**Indicators requiring a different interpretive lens:**

| Indicator | Note |
| :--- | :--- |
| **Bollinger Bands** | Statistically valid with 20+ periods. On 5-minute bars, the bands reflect roughly 100 minutes of volatility — structurally sound, but the interpretation of "wide" or "narrow" is on a different scale than daily analysis. |
| **ADX** | 14 periods on 5-minute bars corresponds to approximately 70 minutes of directional strength. The value is computed correctly, but represents intraday momentum rather than a sustained trend. |

Indicators such as RSI, MACD, EMA, SMA, ROC, and Stochastics are routinely applied in intraday analysis and carry no specific caveats at short intervals.

When using short-interval modes, treat Ichimoku, VWAP, and Fibonacci values as contextual background rather than primary signals.

### How to Read Every Score Table in This Guide

**A threshold is met at the threshold.** One rule covers every score in xoksa, so you do not need a different reading per indicator: a reading that lands exactly on a boundary belongs to the band that boundary defines, and a neutral band is therefore open at both ends. An RSI of exactly 30 is oversold, a deviation of exactly +2.0% scores `+2`, a close sitting exactly on the upper Bollinger band is outside it, and a close on the 38.2% Fibonacci level scores `+2`.

Up to 2.9.8 the code held four different conventions and the tables in this guide did not always agree with the one that applied. Each table below now states its bands as inequalities, with no overlap between rows.

---

## 3. Volume (出来高) — What It Is and Why It Matters

### What Volume Measures

Volume is the total number of **shares traded** during a bar period. It is not the number of transactions (order fills), and it is not the yen or dollar value of those trades. When xoksa displays `92,980,700 shares`, that means 92.98 million shares changed hands in the period covered by the latest bar.

### Why Volume Belongs Alongside Price

A price move without volume is like a court ruling without evidence — the verdict may stand, but the conviction behind it is uncertain. Volume tells you **how many participants agreed with the price at that level**.

- A price rise on high volume means buyers were numerous and willing to pay up. The move has broad backing.
- A price rise on thin volume means only a few participants drove the price, and there are fewer buyers waiting above to absorb selling pressure. These moves are more fragile.
- A price fall on high volume is usually the more serious event — participants were motivated enough to sell (or stop out) in size.
- A price fall on thin volume often means sellers are few and the decline lacks conviction.

None of these patterns are certainties, but they change the probability distribution of what happens next.

### What xoksa Displays

```
📈 Volume: 92,980,700 shares
📈 Avg volume (20 bars): 240,374,445 shares / ratio: 0.39x
   Low vol + up: sustainability uncertain
```

| Value | Meaning |
| :--- | :--- |
| **Volume** | Shares traded in the most recent bar with confirmed trades. Yahoo Finance sometimes appends a zero-volume placeholder at the tail of intraday data; xoksa skips it and uses the last bar that actually has volume. |
| **Avg volume** | Simple average of the last 20 bars with non-zero volume. The period shares the `sma_long_period` setting (default 20). |
| **Ratio** | Latest ÷ average. 1.00× = ordinary session. 0.39× = roughly 60% of typical participation was absent. |

### Absolute Volume Is Not Comparable Across Stocks

A ratio of 0.39× on NTT (9432.T) and a ratio of 0.39× on a small regional bank both signal the same thing — below-average participation — even though NTT's absolute numbers may be 100× larger. **The ratio is the meaningful signal; the absolute number in shares is a characteristic of the stock, not a quality judgment.**

Similarly, comparing absolute volume between a Japanese `.T` stock and a US stock is meaningless — share count conventions, lot sizes, and market structures differ.

### Volume in Intraday, Daily, and Higher-Timeframe Modes

| Mode | What one bar's volume represents |
| :--- | :--- |
| Daily (`--analysis-mode daily`) | Total shares traded on that calendar day |
| Intraday (1m / 5m / 15m / 30m / 60m) | Shares traded in that interval only — a low-volume 5-minute bar during lunch hour is normal, not alarming |
| Weekly / Monthly | Shares traded during one week or one month |

In intraday mode, the 20-bar average reflects the mean participation level across the last 20 intervals of that length — not 20 full trading days. In weekly/monthly mode, 20 bars means 20 weeks or 20 months. A ratio below 1.0× during a thin intraday session is structurally different from a ratio below 1.0× on a full daily, weekly, or monthly bar.

### Data Source

Volume data is sourced from Yahoo Finance OHLCV bars. It represents reported exchange volume for the primary listing. For dual-listed stocks or stocks with significant off-exchange trading, the figure may undercount total market activity.

> For practical interpretation patterns and recipe-by-recipe guidance, refer to **Strategy Guide — Section 4: Reading Volume Data**.

---

## 4. Supported Markets

xoksa can support markets globally, as long as the market data API in use provides data for that market.

- **Japanese market**: Append `.T` to the ticker (e.g., `7203.T`)
- **US market**: Enter the ticker as-is (e.g., `AAPL`, `NVDA`)
- **Other markets**: If the ticker is searchable via the market data API in use, the logic applies universally.

---

## 5. ⚠️ Important Notes Regarding Investment (Disclaimer)

Investing carries the risk of losses contrary to expectations. By using this tool, you are deemed to have agreed to the following.

1. **Principle of Self-Responsibility**: Investment decisions must always be made at your own risk and judgment. The analysis results of this tool do not recommend the purchase or sale of any specific financial instruments.
2. **Accuracy of Information**: External market data APIs, news search APIs, and AI (LLM) generated results may contain delays or errors.
3. **No Warranty**: The developer assumes no responsibility whatsoever for any damages (direct, indirect, or derivative losses) arising from the use of this tool.

Markets are always changing, and past patterns do not guarantee future results. Please manage within the range of your disposable funds and operate calmly.

---

---

# Part 2 — Indicators & the score

## Indicator Display Order (weight-ranked)

Indicators are presented — in the terminal analysis, in `/show technical`, and in the LLM prompt (one shared order) — **ranked by their configured weight, highest first.** The weight is the number you set to express how much you value each indicator (`WEIGHT_BASIC`, `WEIGHT_EMA`, `WEIGHT_SMA`, `WEIGHT_BOLLINGER`, `WEIGHT_ROC`, `WEIGHT_ADX`, `WEIGHT_STOCHASTICS`, `WEIGHT_FIBONACCI`, `WEIGHT_VWAP`, `WEIGHT_ICHIMOKU`). `WEIGHT_BASIC` defaults to `2.0`, the rest to `1.0` — so out of the box the basic block leads. RSI and MACD share the single `WEIGHT_BASIC`.

- **The most-weighted indicator always leads.** There is no fixed category order; the lead reflects *your* priorities for the situation, not a hardcoded sequence.
- **Equal weights keep a fixed order** — the basic block first, then the enabled extensions in their canonical order — so the same settings always produce the same ranking.
- Only enabled indicators appear; a deactivated indicator is still computed and stored but not shown or sent.
- Ordering is presentation only — it never changes any computed value or the final score.

---

## Analysis Candles and Indicator Interpretation (Daily / Intraday / Weekly / Monthly)

The indicator formulas in xoksa are identical for daily, intraday, weekly, and monthly modes.
However, because the meaning of "one bar" changes, the time horizon each indicator covers and its interpretation for trading decisions also change.

- Daily mode: input is daily candlestick bars
- 1-minute mode: 1 bar = 1 minute (most granular; only a few days of history are available)
- 5-minute mode: 1 bar = 5 minutes
- 15-minute mode: 1 bar = 15 minutes
- 30-minute mode: 1 bar = 30 minutes
- 60-minute mode: 1 bar = 60 minutes
- Weekly mode: 1 bar = 1 week
- Monthly mode: 1 bar = 1 month

Therefore, even with the same RSI(14) or MACD(12,26,9), the mathematical definition does not change, but the context of interpretation switches between daily bars, intraday bars (1/5/15/30/60 min), weekly bars, or monthly bars. Intraday mode is not a full day-trading solution — it is a short-term analysis mode for observing near-term trends. Weekly and monthly modes are higher-timeframe views, not separate scoring models.

### Representative Examples

- RSI(14)
  - Daily: momentum over 14 daily bars
  - Intraday: momentum over 14 intraday bars of the specified interval
  - Weekly/Monthly: momentum over 14 weekly or monthly bars

- MACD(12,26,9)
  - Daily: momentum based on 12 / 26 / 9 daily bars
  - Intraday: short-term momentum based on 12 / 26 / 9 intraday bars of the specified interval
  - Weekly/Monthly: momentum based on 12 / 26 / 9 weekly or monthly bars

- EMA(20)
  - Daily: moving average over 20 daily bars
  - Intraday: moving average over 20 intraday bars of the specified interval
  - Weekly/Monthly: moving average over 20 weekly or monthly bars

- Bollinger Bands
  - Daily: medium-term price range and overheating
  - Intraday: short-term range, short-term band expansion/contraction, band walks

- VWAP
  - Useful as a short-term price benchmark in intraday mode
  - In intraday mode, only same-session bars matching the indicator calculation bar date are used, with daily reset
  - In non-intraday modes (daily/weekly/monthly), treated as a volume-weighted average over the specified period

- Fibonacci
  - Calculable in intraday mode as well
  - When using the full acquisition period high/low as reference, treated as "support/resistance within the intraday range"

## Market Data Timestamps and Latest Price Handling (the four times)

At the top of the basic data, xoksa shows time-related information as **four separate fields**.
"When the analysis ran", "how recent the fetched data is", "which bar the latest price falls in", and "which bar the indicators were computed on" are **different things**, and they diverge during trading hours — especially on higher timeframes. That divergence is itself meaningful.

The four on-screen fields (internal field name in parentheses):

- **📅 Analysis time** (analyzed_at) — when this analysis was generated; essentially "now".
- **🕒 Latest data time** (market_data_latest_time) — the timestamp of the newest market data obtainable from the data source.
- **🕯️ Bar of latest price** — the bar that the latest data time falls in (the timestamp floored to the timeframe's interval). During trading hours this is the **still-forming (not-yet-closed) bar**.
- **📊 Indicator bar** — the **last bar of the series** that RSI / MACD and the rest were actually computed on.

### Why they are separated

During trading hours these two usually point at the same, still-forming bar: the source's newest bar *is* the bar its newest quote falls in. They diverge when the quote is newer than the newest bar the source carries — then the latest price sits in a bar the series does not have yet, and the indicators stay on the bar before it. The longer the timeframe, the more visible that gap becomes.

**After close / on a market holiday** there is no forming bar, so all four fields show the **same value** (which is why they appear duplicated on screen — this is expected, not a bug).

```
  ┌──────────────┬──────────────┐
  │  15:00 bar   │  15:30 bar   │   ← the closing bar is the last bar of the series
  └──────────────┴──────────────┘
                        ▲
                        └ 📅 / 🕒 / 🕯️ / 📊 all = 2026-06-26 15:30
```

### What the values mean

"Latest data time" means **the newest market data time that could be obtained**, not a guaranteed real-time quote time. How close it is to the present depends on the source.

- **Japanese stocks** — the international chart API reports Tokyo quotes about 15 minutes late, so xoksa takes the latest price and its time from the public Yahoo! Finance Japan quote page, which is real time. If that fetch fails, the chart value is used instead; nothing is fabricated to fill the gap.
- **Other markets** — the latest price and time come from the chart response. If the source's newest intraday bar only reaches 13:30 while the analysis runs at 14:11, the latest data time and the indicator bar are 13:30.

MarketData is candlestick bar data: `MarketData.timestamp` is the bar time and `MarketData.close` is that bar's close. On Japanese **intraday** timeframes the real-time observation is folded into the bar it belongs to — that bar's close becomes the traded price, and its high/low widen only if the trade falls outside them. **Volume is never invented**: a newly started bar carries no volume, and the display falls back to the most recent measured volume. Daily, weekly and monthly series are left exactly as the source returned them.

Because indicators are computed on the last bar of the series, this means that on Japanese intraday timeframes **the indicators reflect the real-time price** rather than a reading frozen at the previous bar's close.

When integrating with a DBMS, treat the time with the semantics of `market_data_latest_time` / `source_latest_time` rather than hardcoding it as a quote time.

## Basic Analysis (RSI / MACD)

The "basic analysis" in xoksa aims to quantify the directionality of "buy / sell / hold" by combining two representative technical indicators: RSI and MACD.

This section first briefly explains the meaning of each indicator, then describes how xoksa evaluates and scores them.

### What is RSI (Relative Strength Index)?

RSI (Relative Strength Index) is an indicator that uses the ratio of price gains to price losses over a fixed period to show whether the current price is "overbought" or "oversold."

The general interpretations are as follows.

- High RSI → possibility of short-term overbought
- Low RSI → possibility of short-term oversold
- RSI near the midpoint → neutral state, neither overbought nor oversold

In xoksa, RSI is treated as a **"thermometer" for determining whether prices are in an extreme state**.

### What is MACD (Moving Average Convergence Divergence)?

MACD is an indicator derived from the difference between short-term and long-term moving averages, used to identify the trend direction and changes in momentum.

The general interpretations are as follows.

- MACD above Signal → upward force is dominant
- MACD below Signal → downward force is dominant
- Large gap between MACD and Signal → strong momentum (but may also indicate overheating)

In xoksa, MACD is treated as a directional indicator showing "which way the price is trying to move."

### The Thinking Behind Basic Analysis in xoksa

In xoksa's basic analysis, RSI and MACD are not used as standalone criteria — they are evaluated in combination.

The concept is simple:

- RSI looks at "position (too high / too low)"
- MACD looks at "direction (up or down)"

This is the division of roles.

### Scoring Premises

In xoksa, scores are built on the following premises.

- Low RSI → "There may be room for a rebound"
- High RSI → "The market may be overheated"
- MACD above Signal → "Moving upward"
- MACD below Signal → "Moving downward"

The **combination** of these conditions evaluates buy/sell/neutral directionality in gradations.

### How xoksa Handles RSI

- **Calculation engine**: `ta` crate (RelativeStrengthIndex)
- xoksa classifies RSI as follows.

- RSI ≤ buy-rsi → "Undervalued zone"
- RSI ≥ sell-rsi → "Overheated zone"
- In between → "Neutral zone"

> **Configurable thresholds:** `BUY_RSI` (default **30**) and `SELL_RSI` (default **70**) set the two boundaries above. Lower `BUY_RSI` / higher `SELL_RSI` = stricter (fewer "extreme" reads); the reverse = more sensitive.

The key point here is that **RSI alone does not determine buy/sell decisions**.
RSI is used only as supplementary information indicating "whether the price is currently in an extreme position."

### How xoksa Handles MACD

- **Calculation engine**: `ta` crate (MovingAverageConvergenceDivergence)

MACD evaluates the following two points simultaneously:

Whether MACD is above or below Signal

The magnitude of the gap (divergence) between MACD and Signal

In xoksa, a small divergence and a large divergence are handled differently:

- Small gap → beginning of movement, or weak momentum
- Large gap → strong momentum but may also indicate overheating

> **Configurable thresholds** (the boundaries between small / medium / large divergence): `MACD_DIFF_LOW` (default **2**), `MACD_DIFF_MID` (default **10**), `MACD_DIFF_EXTREME` (default **100**). Raise them for instruments whose MACD naturally runs large (e.g. high-priced stocks) so the "strong" band is not triggered too easily.

### The Scoring Concept from RSI × MACD

The basic score in xoksa is determined by combining the state of RSI and MACD.

Representative examples are as follows.

- RSI undervalued + MACD upward → strong buy signal
- RSI undervalued + MACD downward → rebound expected but cautious (weak buy)
- RSI overheated + MACD upward → overheating caution (sell direction)
- RSI neutral + MACD upward → possibility of trend continuation evaluated
- RSI neutral + MACD downward → weakness or holding pattern

In this way, judgment is broken down into the combination of "position (RSI)" and "direction (MACD)."

### Handling MACD in the Negative Zone (macd-minus-ok)

In xoksa, how MACD behaves when in the negative zone can be controlled via an option.

- macd-minus-ok disabled → buy-direction evaluation is suppressed when MACD is in the negative zone
- macd-minus-ok enabled → even in the negative zone, evaluation for counter-trend rebounds is permitted

This allows explicitly switching between different styles such as:

- Trend-following emphasis
- Counter-trend (mean-reversion) emphasis

### Summary
- RSI and MACD states are organized with **consistent rules**
- The premises for judgment are **explicitly stated as numerical conditions**
- Provided in a form **combinable with other indicators and recipes**


## EMA (Exponential Moving Average)

In xoksa, EMA (Exponential Moving Average) is used as an indicator to quantitatively understand the trend direction and strength of prices.

While RSI and MACD look at "overheating degree" and "momentum," EMA plays the role of determining whether the trend is upward, downward, or nonexistent.

### What is EMA (Exponential Moving Average)?

EMA is a type of moving average characterized by giving greater weight to more recent prices.

- Reacts sensitively to recent price movements
- Can capture trend reversals relatively early
- Noise remains but lag is small

Due to this property, EMA is widely used to understand **"whether the current flow is continuing or changing."**

### How xoksa Calculates EMA

- **Calculation engine**: `ta` crate (ExponentialMovingAverage)

In xoksa, the `ta` crate — a Rust technical analysis library — is used for EMA calculation.

This library is designed around a standard TA (Technical Analysis) trait model where each indicator is calculated by "inputting sequential data to obtain results."

xoksa uses this mechanism as-is:

- Feed closing price data chronologically
- Update EMA to its final value in a single pass
- No recalculation or double-calculation

### Short EMA and Long EMA

xoksa uses the following 2 EMA lines.

- Short EMA (default: **5 periods** / `--ema-short-period` / configurable via env var `EMA_SHORT_PERIOD`)
  → Sensitive to recent price changes
- Long EMA (default: **20 periods** / `--ema-long-period` / configurable via env var `EMA_LONG_PERIOD`)
  → Reflects the overall trend

> Short < Long constraint applies. Reversing them will cause an error.

By comparing these 2 lines, we evaluate whether the short-term is above or below the long-term, or at approximately the same level.

### The Thinking Behind EMA in xoksa

In xoksa, **the "difference" between short-term and long-term is more important than the absolute value of EMA itself**.

> [!NOTE]
> **The score is read from the deviation rate, so it means the same thing on every
> instrument.** The boundaries are **±2.0% and ±0.5%** of `(short EMA − long EMA)
> ÷ close × 100` — the rate printed on the line above the score — so a stock
> priced in thousands of yen and one priced in tens of dollars are judged on the
> same footing. Before this change the boundaries were a fixed price difference
> (2.0 / 0.5 in the instrument's own currency), which put essentially every
> Japanese listing at ±2 and left low-priced US listings at 0 whatever their
> actual separation. See the Ichimoku section for the measured figures.

#### Short EMA − Long EMA
→ The value indicating the direction and strength of the current trend

This difference is interpreted as:

- Large positive → upward trend is clear
- Large negative → downward trend is clear
- Near zero → no trend (range-bound)

### EMA Scoring Design

In xoksa, the deviation rate between short EMA and long EMA is converted to a 5-level score.

#### deviation rate (%) = (short EMA − long EMA) / close × 100

|Condition (deviation rate)	|Interpretation	|Score|
|---	|---	|---|
|rate ≥ +2.0%	|Strong uptrend	|+2|
|+0.5% ≤ rate < +2.0%	|Uptrend	|+1|
|−0.5% < rate < +0.5%	|No trend	|0|
|−2.0% < rate ≤ −0.5%	|Downtrend	|-1|
|rate ≤ −2.0%	|Strong downtrend	|-2|

The key point here is that **EMA alone does not complete the buy/sell judgment**.
EMA is simply a material indicating "in which direction prices are currently flowing," and is used in combination with RSI, MACD, and other indicators.

### EMA's Positioning in xoksa

In xoksa, EMA is:

- Not an indicator for directly making counter-trend judgments
- Not an indicator for measuring "momentum"

It is treated as a **reference axis indicating the presence and direction of a trend**.

Therefore:

- In counter-trend recipes → EMA score is set low (or weight is reduced)
- In trend-following / breakout plays → EMA score is emphasized

### Why xoksa Adopts EMA

The reasons xoksa adopts EMA as a fundamental indicator are: it is a standard indicator that is widely used; its calculation logic is clear with high reproducibility; and it can be safely calculated via the TA library.

EMA is not an "indicator that outputs judgment" — it is an indicator showing the "flow" that forms the premise of judgment.

### Summary: What EMA Shows

In xoksa, EMA is a tool for organizing with numerical values and scores:

- Up or down
- Strong or weak
- Whether there is any flow at all



## SMA (Simple Moving Average)

In xoksa, SMA (Simple Moving Average) is treated as an indicator for confirming the average price level and trend stability.

While EMA is a "trend indicator sensitive to recent changes," SMA is positioned as a baseline for smoothing noise and confirming a more straightforward flow.

### What is SMA (Simple Moving Average)?

SMA is the simple average of closing prices over a fixed period.

- Gives equal weight to all prices
- Calculation method is intuitive and easy to understand
- Not easily swayed by short-term fluctuations

Therefore, SMA is often used when wanting to broadly confirm the direction of a trend, or wanting to understand whether prices are "high or low on average."

### How xoksa Calculates SMA

- **Calculation engine**: `ta` crate (SimpleMovingAverage)

In xoksa, the `ta` crate is also used for SMA calculation, as with EMA.

This library adopts a sequential calculation model based on the standard TA (Technical Analysis) traits, feeding closing price data chronologically, finalizing the latest SMA in a single pass, with no unnecessary recalculation or double processing.

### Short SMA and Long SMA

xoksa uses the following 2 SMA lines.

- Short SMA (default: **5 periods** / `--sma-short-period` / configurable via env var `SMA_SHORT_PERIOD`)
  → Average price level in the near term
- Long SMA (default: **20 periods** / `--sma-long-period` / configurable via env var `SMA_LONG_PERIOD`)
  → Medium-term average price level

> Short < Long constraint applies. Reversing them will cause an error.

By observing the relationship between these 2 lines, we judge whether prices are trending upward or downward on average, or whether there is any directional sense.

### The Thinking Behind SMA in xoksa

In xoksa, not the absolute value of SMA itself, but the **difference between short SMA and long SMA** is the subject of evaluation.

> [!NOTE]
> **The score is read from the deviation rate, so it means the same thing on every
> instrument.** The boundaries are **±3.0% and ±1.0%** of `(short SMA − long SMA)
> ÷ close × 100` — the rate printed on the line above the score. They are wider
> than the EMA pair on purpose: both legs run the same 5/20 periods, but a simple
> average lags further behind in a trend, so the same market gives a larger SMA
> separation (measured median 1.60% against EMA's 1.03%). Before this change the
> boundaries were a fixed price difference shared with EMA and Ichimoku. See the
> Ichimoku section for the measured figures.

#### Short SMA − Long SMA
→ Deviation in average price = slope of the trend

This difference is interpreted as:

- Large positive → stable flow in the upward direction
- Large negative → stable flow in the downward direction
- Near zero → average prices converging = no directional sense

### SMA Scoring Design

In xoksa, the deviation rate between short SMA and long SMA is converted to a 5-level score.

#### deviation rate (%) = (short SMA − long SMA) / close × 100

|Condition (deviation rate)	|Interpretation	|Score|
|---	|---	|---|
|rate ≥ +3.0%	|Strong golden cross	|+2|
|+1.0% ≤ rate < +3.0%	|Gradual uptrend	|+1|
|−1.0% < rate < +1.0%	|No trend	|0|
|−3.0% < rate ≤ −1.0%	|Gradual downtrend	|-1|
|rate ≤ −3.0%	|Strong dead cross	|-2|

This score is designed to show whether a trend "exists or not" and how stable that flow is — it is not intended to directly indicate buy/sell timing.

### Role Difference Between EMA and SMA

In xoksa, EMA and SMA are **intentionally used together**.

- EMA → Sensitive to recent movements (easy to detect early signs of change)
- SMA → Reflects the average flow (resistant to noise)

Therefore:

- EMA and SMA in the same direction → trend is relatively straightforward
- EMA and SMA diverging → turning point, or unstable market

### SMA's Positioning in xoksa

In xoksa, SMA is used as a supplementary axis for measuring trend "stability" — it is not a direct basis for counter-trend judgment, nor an indicator for catching the initial move of a breakout.

Therefore, in trend-following it is emphasized together with EMA; in counter-trend plays, its weight is reduced or treated as reference information.

### Summary: What SMA Shows

In xoksa, SMA is an indicator that simply shows in which direction prices are tilting on average, and whether that flow is stable.

If EMA shows "the speed of change," SMA shows "the settledness of the flow."

By presenting these two side by side, xoksa leaves room for the user to judge "whether this is a market to chase or a market to wait."

## ADX (Average Directional Index)

In xoksa, ADX (Average Directional Index) is treated as an indicator for measuring the "strength" of a trend, not its "direction."

While EMA/SMA show "whether upward or downward," ADX provides material for judging **"whether the market is in a trending state or a range-bound state in the first place."**

### What is ADX (Average Directional Index)?

ADX is an indicator calculated using Directional Movement (+DM/-DM) and True Range (TR), which quantifies whether a trend exists in the market and how strong it is.

The general understanding is as follows.

- High ADX → trend is strong ("tends to run" in either direction)
- Low ADX → trend is weak (tends to be range-bound / prone to fluctuation)

The key point here is that **ADX does not indicate whether the direction is up or down**.
ADX is purely a strength meter; direction is supplemented by other indicators (moving averages, DI, etc.).

### How xoksa Calculates ADX

- **Calculation engine**: **Original calculation** (arithmetic implementation, Wilder smoothing)

In xoksa, ADX is **calculated internally**.
The reason is that the `ta` crate in use does not provide ADX as a standard implementation.

xoksa's ADX evaluates over **N periods** (default: **14** / `--adx-period` / configurable via env var `ADX_PERIOD`) using the Wilder smoothed moving average (RMA) of the following elements:

- True Range (TR): the actual amplitude of price movement
- +DM / -DM: "dominance" in the upward / downward direction
- +DI / -DI: directional strength normalized by ATR
- DX: directional strength derived from the difference between +DI and -DI

The implementation conforms to the Wilder (1978) smoothing method.

1. **Initial values for ATR / +DM14 / -DM14**: initialized with the sum of the first N TR / +DM / -DM values
2. **Sequential update via RMA (Wilder smoothed moving average)**:
   `ATR_new = ATR_old − ATR_old / N + TR_new` (same for +DM / -DM)
3. **DX calculation**: +DI / -DI → DX calculated at each step
4. **Initial ADX value**: simple average of the first N DX values
5. **Sequential ADX update**: `ADX_new = (ADX_old × (N−1) + DX_new) / N`

The minimum number of data points required is **2×N** (default: 28 bars).

### ADX's Positioning in xoksa

In xoksa, ADX functions as a **gatekeeper for determining whether a given strategy fits the current market**.

For example:

- Breakout play / trend-following → higher ADX (trend present) is better fit
- Counter-trend / range reversion → lower ADX (no trend) is better fit

ADX is an indicator whose evaluation meaning changes depending on the recipe (strategy).

### ADX Scoring Design

In xoksa, ADX is converted to a 5-level score.

|ADX Level	|Interpretation	|Score|
|---	|---	|---|
|50 or above	|Very strong trend	|+2|
|30 or above	|Strong trend	|+1|
|20 or above	|Trend established (neutral)	|0|
|10 or above	|Weak trend (range-leaning)	|-1|
|Below 10	|No trend (strongly range-bound)	|-2|

Note: This score is not for deciding "buy/sell direction"; its purpose is to quantify whether the market is in a state that "tends to run."

### Summary: What ADX Shows

In xoksa, ADX is an indicator that organizes with numerical values and scores whether the current market has a trend, and how strong that trend is.

"Momentum (ROC, etc.)" and "trend strength (ADX)" are different things — xoksa treats them as separate knobs.

ADX is important prerequisite information for judging — depending on the recipe — whether this is "a market to chase" or "a market to wait."


## ROC (Rate of Change)

In xoksa, ROC (Rate of Change) is treated as an indicator that shows in percent how much prices have changed (risen/fallen) over a fixed period.

While EMA/SMA look at trends from the "positional relationship of moving average lines" and ADX looks at "trend strength," ROC plays the role of directly measuring momentum — "how much has it moved?"

### What is ROC (Rate of Change)?

ROC compares the price at a given point with the price a fixed number of periods earlier, expressing the difference as a percentage.

- Positive ROC → prices have risen during the period
- Negative ROC → prices have fallen during the period
- ROC near 0 → little change during the period

While the indicator is simple, "how much has it moved" appears clearly as a numerical value, making it easy to use as a basis for judgment in breakout plays and trend-following strategies.

### How xoksa Calculates ROC

- **Calculation engine**: **Original calculation** (arithmetic formula based on the difference from the previous bar)

xoksa's ROC is calculated using the most recent closing price and the **closing price N bars ago** (default: **10** / `--roc-period` / configurable via env var `ROC_PERIOD`).

#### roc = ((latest_close - close_N_bars_ago) / close_N_bars_ago) * 100

In the implementation, N = 10 is assumed, comparing the latest closing price (latest_close) and the closing price equivalent to N bars ago (data[len - (N+1)]) to calculate ROC. Therefore, calculation of ROC requires at least N+1 or more bars of data.

### ROC's Positioning in xoksa

In xoksa, ROC is used as material for **confirming whether the current price movement has momentum**.

However, ROC is momentum that includes direction:

- Strong ROC → moving a lot (in either direction)
- Weak ROC → not moving much (range-bound)

Therefore, in xoksa, ROC is not interpreted in isolation — it is used in combination with ADX (trend strength) and other indicators.

### ROC Scoring Design

In xoksa, ROC is converted to a 5-level score. The aim is to clearly classify "whether there is momentum or not."

|Condition (ROC)	|Interpretation	|Score|
|---	|---	|---|
|ROC ≥ +10%	|Very strong upward momentum	|+2|
|+3% ≤ ROC < +10%	|Moderate upward momentum	|+1|
|−3% < ROC < +3%	|Flat zone (small change)	|0|
|−10% < ROC ≤ −3%	|Moderate downward momentum	|-1|
|ROC ≤ −10%	|Very strong downward momentum	|-2|

The scores here are not for determining "buy/sell" outright — they are for summarizing the magnitude of the price movement momentum (rate of change).

### Summary: What ROC Shows

In xoksa, ROC is an indicator that simply quantifies how much prices have risen/fallen in a given period, and whether there is momentum in the price movement.

While ROC indicates momentum, "whether that momentum will continue as a trend" is a separate question.

In xoksa, ROC (momentum) and ADX (trend strength) are treated as separate knobs, with a design that allows adjusting weights according to the recipe (strategy).

## Stochastics

In xoksa, Stochastics quantifies where within the price range (high to low) over a fixed period the closing price is located, and is treated as an indicator for understanding short-term "extremes (overheating/oversold)."

While RSI looks at overheating from the "strength ratio of gains to losses," Stochastics is an **oscillator that uses "position within the range"** to observe overheating.

### What is Stochastics?

Stochastics is generally described by two lines:

- %K: price position (main indicator)
- %D: smoothed %K (signal)

The basic formula for %K is as follows.

### %K = (Close - LowestLow) / (HighestHigh - LowestLow) × 100

Where:

- HighestHigh: the highest high within the period
- LowestLow: the lowest low within the period
- Close: the closing price of the current bar

and %K ranges from 0 to 100.

Generally, a high value (e.g., 80 or above) suggests a high price zone (tends to be seen as overbought), and a low value (e.g., 20 or below) suggests a low price zone (tends to be seen as oversold).

### Calculation Method in xoksa

- **Calculation engine**: **Original calculation** (arithmetic formula based on extracting period high/low)

xoksa's Stochastics calculates **%K (default: 14 periods / `--stochastics-period` / configurable via env var `STOCHASTICS_PERIOD`) and %D (3-period average)**.

1) Required data volume — at least N bars of data are required (to build the N-period high/low range)

2) Build the N-period HighestHigh / LowestLow — xoksa takes the "most recent N bars" for each bar and calculates and retains the highest high and lowest low in that range.

3) Calculate %K for the latest bar using the high / low / close of the latest bar.

#### If high == low (range width = 0): %K is treated as 0.0 to avoid division by zero.

4) Calculate %D — xoksa's %D is the average of the most recent 3 %K values.

#### %D = Simple average of the most recent 3 %K values

### Scoring in xoksa (5 Levels)

In xoksa, the Stochastics score is determined based on %K.
(%D is calculated and retained, but the score judgment itself is based on %K)

The score has the following 5 levels.

|%K Level	|Interpretation (reference)	|Score|
|---	|---	|---|
|90 or above	|Strongly overheated (high price zone)	|-2|
|80 or above	|Overheated (high price zone)	|-1|
|20 or below	|Slightly oversold (low price zone)	|+1|
|10 or below	|Strongly oversold (low price zone)	|+2|
|Other	|Neutral	|0|

This score is designed not to output "trend direction" but to summarize short-term extremes as a numerical value.

### Reading the Values (Reference)

Since Stochastics uses a 0–100 scale, users can easily be conscious of thresholds.

- Raising the threshold (e.g., 80→85) → only more extreme high price zones are considered overheated (fewer signals)
- Lowering the threshold (e.g., 80→70) → wider range is considered high price zone (more signals)

Similarly for the low side:

- Lowering the threshold (e.g., 20→15) → only very extreme low price zones are captured (fewer signals)
- Raising the threshold (e.g., 20→30) → wider range is considered low price zone (more signals)

xoksa retains %K / %D and scores in a struct so that this kind of "knob" can be utilized on the recipe side.

### Summary: What Stochastics Shows

Stochastics in xoksa provides the closing price position within the N-period range (%K), its smoothed value (%D), and a score for overheating/oversold based on %K, functioning as a supplementary indicator for quantifying short-term extremes.

By combining with trend indicators (EMA/SMA) and momentum indicators (ROC), it is possible to judge "is it running" and "is it going too far" on separate axes.


## Bollinger Bands

Bollinger Bands are an indicator based on the price distribution (mean and dispersion) over a fixed period that captures whether the indicator calculation bar's closing price is at a "high/low position" and whether price movement is compressed (contraction) or expanded (expansion).

In xoksa, the following are calculated:

- Upper band (Upper)
- Lower band (Lower)
- %B (position within the band)
- Bandwidth (band width = degree of compression/expansion)

### Calculation Method in xoksa

- **Calculation engine**: `ta` crate (BollingerBands)
- **Period: 20 bars** (default / `--bollinger-period` / configurable via env var `BOLLINGER_PERIOD`)
- **Standard deviation: 2.0** (default / `--bollinger-stddev-multiplier` / configurable via env var `BOLLINGER_STDDEV_MULTIPLIER`)
- Center line (Mid): (Upper + Lower) / 2 (due to 2σ symmetry)
- Derived values:
  - %B: `%B = (Close - Lower) / (Upper - Lower)` → Closer to 0.0 means near the lower band; closer to 1.0 means near the upper band
  - Bandwidth (%): `Bandwidth(%) = (Upper - Lower) / Mid × 100` → Smaller value means price movement is more compressed

### Scoring (Price Position)

In xoksa, the closing price of the indicator calculation bar is evaluated in 5 levels based on whether it has broken outside the band.

- At or more than 2% above the upper band → -2
- At or above the upper band, below 2% above it → -1
- Strictly inside the band → 0
- At or below the lower band, above 2% below it → +1
- At or more than 2% below the lower band → +2

A band with **no width** scores `0` whatever the price. With zero volatility — a close that has not moved for the whole period — the two edges coincide, so there is nothing to be outside of and the position carries no information.

Note: What is being evaluated here is "position" — no directional determination is made.

### What is a Squeeze?

A squeeze refers to the state where Bandwidth has become small and price movement is compressed.

This does not indicate "going up/going down" — rather, it is a concept for understanding that price range is not expanding and directional sense has not solidified.

In xoksa, a squeeze is **not treated as a buy/sell signal**.

### How to Interpret a Squeeze (Reference)

When a squeeze is occurring, the situation where "not moving much" can be explained by Bandwidth is in effect.
Therefore, a squeeze can be used as material for understanding "It's not running right now" and "Why is it hard to make a judgment?"

### Squeeze Detection Option
#### --bb-bandwidth-squeeze-pct \<pct\>

If Bandwidth is at or below this value, xoksa treats it as a **squeeze state**.

- Making the value smaller → only stronger compression is considered a squeeze (more selective)
- Making the value larger → looser compression is also included as a squeeze (wider capture)

This parameter is the knob for adjusting **"what degree of compression to call a squeeze."**

### Summary

In xoksa, Bollinger Bands are an indicator for numerically understanding the **position** of prices (Upper/Lower and %B) and the **state** of price movement (Bandwidth and squeeze).

A squeeze is not for deciding buy/sell direction — it is treated as information for understanding whether the market is in a compressed state.


## Fibonacci Retracement

In xoksa, Fibonacci is an indicator for numerically expressing **"where the price is within the recent price range"** using rules determined only once.

It does not predict trends — instead, it plays the role of organizing as positional information whether we are "in the middle of a pullback," whether the pullback is deep or shallow, and whether it is hesitating near the center (50%).

### What is Fibonacci?

Fibonacci Retracement is a method of drawing "retracement/pullback" reference levels at certain ratios relative to the range between the recent high (High) and recent low (Low).

The commonly used representative levels are 38.2%, 50.0%, and 61.8%.

xoksa focuses on these 3 levels.

### Calculation Method in xoksa

- **Calculation engine**: **Original calculation** (arithmetic formula based on the golden ratio)

1) Determining high and low — the highest high and lowest low over the entire period are obtained from the MarketData of the input period.

#### span = high - low

- Note: If span ≤ 0 (no movement) → treated as indeterminate, set to neutral (0).

2) Calculating Fibonacci levels — in xoksa, these are determined **only once** here.

#### 38.2% = high - span × 0.382
#### 50.0% = high - span × 0.500
#### 61.8% = high - span × 0.618

This level is the SoT (Single Source of Truth) for both display and judgment.

### Scoring Philosophy in xoksa (Important)

xoksa's Fibonacci has a design that clearly separates "which band the price is in."

### Key Points

- Score of 0 is **only** near the 50% level
- Up or down is always tilted to one side
- ±eps exists to avoid boundary blurring — it is a guard, not a band you should expect to land in. The close usually sits well away from the midpoint: measured across 20 instruments, 19 of them were more than 6% of the range from it.

### Score Judgment Rules (Single Source of Truth)

Based on the positional relationship between the closing price (close) and each level, the score is finalized **once** here.

|Condition (close against the levels)	|Interpretation (reference)	|Score|
|---	|---	|---|
|close ≥ the 38.2% level	|Shallow pullback (strong)	|+2|
|above the 50% level and at least eps above it, below the 38.2% level	|Moderate strength while returning	|+1|
|the 50% level itself, or less than eps from it	|Sitting on the midpoint (a boundary guard, rarely reached)	|0|
|below the 50% level and at least eps below it, above the 61.8% level	|Weak pullback leaning downward	|-1|
|close ≤ the 61.8% level	|Deep pullback (weak)	|-2|

- **eps (the neutral band)**: **0.05 of the 50% → 38.2% distance** (default / `--fibonacci-neutral-ratio` / configurable via env var `FIBONACCI_NEUTRAL_RATIO`, accepted range **0.0–0.5**). It is a **share of the room the ±1 bands have**, not a price, so it means the same thing on every instrument and scales with the range: 5% of a 120-point room on a ¥4,000 stock, 5% of a 0.40 room on a $13 one. The cap at 0.5 is what guarantees ±1 stays reachable.
  Setting it to `0` removes the band: only a close sitting exactly on the 50% level is neutral then, and any distance at all leans. It does not make the midpoint lean up.
  Up to 2.9.8 this was a price difference (default 0.50), which meant a different thing on every instrument. Measured on daily bars it took 0.2% of the room on 9984.T — so `0` was effectively unreachable — and **more than the whole room on F, where `+1` required a close both above 15.080 and below 14.984, so `+1` and `−1` could never be scored at all.**
    - The range of ±eps around the 50.0% level (by default 5% of the 50% → 38.2% distance) is treated as "neutral (0)."
- Note: **A threshold is met at the threshold** — the one boundary rule every score in xoksa follows. A close landing exactly on the 38.2% or 61.8% level therefore scores `±2`, and the neutral band around the 50% level is open at both ends.

### Reading the Values (Reference)

Fibonacci is not an indicator that moves numerical values. What should be observed is "which band the price is in."

- Above 38.2% → shallow pullback, tends to be interpreted as still in the middle of a strong move
- Near 50% → buy and sell tend to be in balance
- Below 61.8% → deep pullback/retracement, requiring careful judgment

In xoksa, only "which band is it in right now?" is mechanically extracted.
The interpretation and buy/sell judgment are premised on the user combining with other indicators (RSI / EMA / ADX, etc.).

### Summary: The Role of Fibonacci

Fibonacci in xoksa is an indicator for uniquely determining levels from high and low, judging where the price is among 38.2 / 50 / 61.8, and converting positional information into a score of -2 to +2.

It does not measure trend strength or momentum — it is designed as a supplementary axis that statically indicates the "price's standing position."

Therefore, in counter-trend plays it is used as "confirmation of depth of pullback," and in breakout plays it is used to determine "whether we are still in the middle of a retracement."


## VWAP (Volume Weighted Average Price)

In xoksa, VWAP is treated as an indicator for understanding — based on the current analysis bar — "where the indicator calculation bar's closing price is relative to the recent average transaction price range."

In the current implementation, OHLCV market data is used to calculate by weighting the Typical Price by volume.
In intraday mode, all same-session bars as the indicator calculation bar (daily reset) are referenced; in daily mode, the most recent N bars (default: 14) are referenced.

The design is utility-first: self-contained with daily or intraday acquisition data, and comparable alongside other trend indicators (EMA / SMA).

### What is VWAP? (General, Minimal)

The original VWAP (Volume Weighted Average Price) accumulates price × volume and calculates the average transaction price for that day, thereby showing "at what price market participants traded on average." It is often used as an execution benchmark by institutional investors.

### Prerequisites in xoksa (Important)

xoksa operates under the following constraints.

- Intraday mode: session VWAP targeting all same-session bars as the indicator calculation bar (daily reset)
- Daily mode: rolling VWAP targeting the most recent N bars (default: 14 / `--vwap-period`)
- Not a full-fledged intraday VWAP with tick-level granularity
- Cannot perform VWAP evaluation for stocks where volume data cannot be obtained
- Consistency with other indicators is prioritized

Therefore, xoksa's VWAP should be read not as "an intraday execution benchmark" but as "the divergence between the volume-weighted price range and the indicator calculation bar's closing price."

### Calculation Method in xoksa

- **Calculation engine**: **Original calculation** (volume-weighted Typical Price × Volume)

1) Calculating Typical Price — from the price data of each bar:

#### Typical Price = (High + Low + Close) / 3

This is an approximate value representing "the representative price range of that bar."

2) Period settings

- Intraday mode: all same-session bars as the indicator calculation bar (`--vwap-period` is ignored)
- Daily mode: **most recent N bars** (default: 14 / `--vwap-period` / configurable via env var `VWAP_PERIOD`)
- Reason: not too short, and easy to compare alongside EMA/SMA

3) Calculating VWAP (volume-weighted) — for the most recent N bars:

#### VWAP = Σ(Typical Price × Volume) / Σ(Volume)

Bars with larger volume are more strongly reflected in the VWAP.

### Scoring in xoksa

VWAP evaluates only **"whether the current closing price is above or below the average transaction price range."**

- Judgment axis:
#### deviation rate (%) = (Close − VWAP) / VWAP × 100

### Score Judgment (5 Levels)

|Condition (deviation rate)	|Interpretation (reference)	|Score|
|---	|---	|---|
|rate ≥ +3.0%	|Price clearly above average	|+2|
|+1.0% ≤ rate < +3.0%	|Price slightly above average	|+1|
|−1.0% < rate < +1.0%	|Nearly the same level (neutral)	|0|
|−3.0% < rate ≤ −1.0%	|Price slightly below average	|-1|
|rate ≤ −3.0%	|Price clearly below average	|-2|

- Note: Judgment is made by the **deviation rate** — the distance from VWAP as a percentage of VWAP, which is the same figure the display shows, so the reading and the score cannot disagree
- Note: The numerical values are a reference for "positional relationship," not "momentum"

> [!NOTE]
> **The boundaries are a percentage, so the score means the same thing on every
> instrument** — a stock priced in thousands of yen and one priced in tens of
> dollars are scored on the same footing, and the score only rises as the
> deviation does. Before this change the boundaries were a price difference (±4.0
> / ±1.0 in the instrument's own currency), which made the same score mean
> different distances on different instruments; measured on daily bars, a close
> 5.03% below VWAP scored 0 while one 2.19% below scored −2. The deviation rate
> replaced it.
>
> **On intraday timeframes the score sits near 0 far more often, and that is the
> reading, not a fault.** VWAP restarts at each session open, so the close is
> usually within a fraction of a percent of the session's own average — measured
> at −0.87% and −0.12% on 60-minute bars. Reaching ±2 intraday means the price
> has genuinely run away from where the session's volume traded.

### Reading the Values (Reference)

VWAP does not create a trend — it is for observing relative position:

- Closing price > VWAP → higher position than market average
- Closing price < VWAP → lower position than market average

In xoksa, the role division is: EMA/SMA for trend direction, ROC/ADX for momentum and tendency to run, and VWAP for deviation from the average price range.

### Summary: The Role of VWAP

VWAP in xoksa weights Typical Price by volume (intraday: session VWAP, daily: most recent N-bar rolling VWAP), gives heavier weighting to price ranges with larger volume, and converts the distance from the closing price into a score of -2 to +2.

It is not a precise intraday execution indicator — it is designed as **one positional reference** for judging "whether the indicator calculation bar's closing price is above or below average" alongside other indicators.


## Ichimoku (Ichimoku Kinko Hyo)

In xoksa, the Ichimoku Kinko Hyo uses only the positional relationship between the Tenkan-sen (default: **9 periods** / `--ichimoku-tenkan-period` / configurable via env var `ICHIMOKU_TENKAN_PERIOD`) and Kijun-sen (default: **26 periods** / `--ichimoku-kijun-period` / configurable via env var `ICHIMOKU_KIJUN_PERIOD`) to compare the "short-term momentum" and "medium-term baseline" of the market.

> Tenkan period < Kijun period constraint applies. Reversing them will cause an error.

Ichimoku is originally a comprehensive indicator that also draws the band between the two Senkou Spans (the shaded area many charts label the "cloud") and the Chikou Span, but in xoksa, scoring is narrowed down to **"line crosses"** to enable treating it on the same level as other indicators.

### What is Ichimoku? (Minimal)

Ichimoku Kinko Hyo has multiple elements, but xoksa uses the following:

- Tenkan-sen: short-term baseline (default: **9** periods / configurable with `--ichimoku-tenkan-period`)
- Kijun-sen: medium-term baseline (default: **26** periods / configurable with `--ichimoku-kijun-period`)

Both are defined as "the average of the high and low within the period."

### Calculation Method in xoksa

- **Calculation engine**: **Original calculation** (average of period high/low)

1) Required data volume — at least the Kijun-sen period of data is required (because the Kijun-sen calculation uses the kijun period)

2) Tenkan-sen (tenkan period) — take the most recent tenkan period's highest high and lowest low:

#### Tenkan-sen = (Highest high over tenkan period + Lowest low over tenkan period) / 2

3) Kijun-sen (kijun period) — take the most recent kijun period's highest high and lowest low:

#### Kijun-sen = (Highest high over kijun period + Lowest low over kijun period) / 2

### Scoring in xoksa (5 Levels)

xoksa's Ichimoku judges by the sign and size of the **divergence rate** between the Tenkan-sen and the Kijun-sen.

#### divergence rate (%) = (tenkan − kijun) / kijun × 100

|Condition (divergence rate)	|Interpretation (reference)	|Score|
|---	|---	|---|
|rate ≥ +4.0%	|Strong upward direction (golden cross)	|+2|
|+1.0% ≤ rate < +4.0%	|Slightly upward	|+1|
|−1.0% < rate < +1.0%	|Near-equal zone (balanced)	|0|
|−4.0% < rate ≤ −1.0%	|Slightly downward	|-1|
|rate ≤ −4.0%	|Strong downward direction (dead cross)	|-2|

> [!NOTE]
> **The boundaries are a percentage, so the score means the same thing on every
> instrument, and a wider separation never scores lower than a narrower one.**
> Before this change the boundaries were an absolute price difference — 2.0 / 0.5
> in the instrument's own currency — which made the score a function of the price
> level rather than of the separation it claimed to read. Measured across 20
> instruments on daily bars: **1605.T sat 0.16% from its base line and scored −2,
> while F at 1.89% scored 0**; every Japanese listing in the sample reached ±2
> whatever its actual separation, and 2.0 on a ¥6,000 stock is 0.03% while the
> same ±2 on a $30 stock needed 6.7%.
>
> The ±4.0% / ±1.0% pair is wider than EMA's (±2.0% / ±0.5%) and SMA's (±3.0% /
> ±1.0%) because the 9/26 periods are further apart than those indicators' 5/20,
> so the same market gives a larger divergence — the measured median was 2.47%
> for Ichimoku against 1.60% for SMA and 1.03% for EMA.
>
> This is the same figure as the divergence rate described below, which the
> display shows unsigned; the score needs the sign as well.

#### Supplementary: Detailed Display by Divergence Rate (gap_ratio)
The same divergence rate the score is read from is also shown unsigned, with a note added at display time:
- **Divergence below 1.0%**: "Approaching state, weak for trend confirmation"
- **Divergence above 5.0%**: "Possibility of large divergence (excessive divergence)"

Note that 1.0% is also the score's own `±1` boundary, so "approaching" and a score of `0` describe the same reading from two sides. This allows not only whether a cross exists, but also the "degree of momentum overheating" to be grasped at a glance.

### Reading the Values (Reference)

xoksa's Ichimoku is used for the purpose of observing "line crosses."

- Tenkan-sen > Kijun-sen → short-term is stronger than medium-term (suggests upward direction)
- Tenkan-sen < Kijun-sen → short-term is weaker than medium-term (suggests downward direction)
- Close (within ±0.5) → directional sense is weak / balanced

The larger the difference (above ±2.0), the stronger the score in xoksa's design.

### Summary: The Role of Ichimoku (xoksa Edition)

In xoksa, Ichimoku Kinko Hyo calculates Tenkan-sen (tenkan) and Kijun-sen (kijun) and scores based only on their difference in -2 to +2, thereby incorporating Ichimoku not as a "comprehensive indicator" but as a short-term vs. medium-term strength comparison, enabling synthesis at the same layer as trend indicators like EMA/SMA.

---

## Volume

### What Volume Measures

Volume is the number of **shares traded** during a bar period. It is not the number of transactions (order fills) and not the yen or dollar value of those trades.

### What xoksa Calculates

xoksa derives three values from the Yahoo Finance OHLCV bars:

| Value | Calculation |
| :--- | :--- |
| **Latest volume** | Volume of the most recent bar with `volume > 0`. Yahoo Finance sometimes appends a zero-volume placeholder bar at the tail of intraday data; xoksa skips it and uses the preceding bar with confirmed trades. |
| **Avg volume** | Simple mean of the last N bars where `volume > 0`. N shares the `sma_long_period` setting (default 20). Zero-volume bars are excluded. |
| **Volume ratio** | `latest_volume ÷ avg_volume`. Values above 1.0 mean above-average participation; below 1.0 mean below-average. |

### Thresholds and Comment Logic

| Condition | Comment |
| :--- | :--- |
| `ratio == 0.0` | Zero volume (no trades in this bar) |
| `ratio ≥ 1.1` + price up | High vol + up: possible upward momentum |
| `ratio ≥ 1.1` + price down | High vol + down: possible selling pressure |
| `ratio ≥ 1.1` + price flat | High vol, price unchanged |
| `ratio < 0.9` + price up | Low vol + up: sustainability uncertain |
| `ratio < 0.9` + price down | Low vol + down: possible thin-volume decline |
| `ratio < 0.9` + price flat | Low vol, price unchanged |
| `0.9 ≤ ratio < 1.1` | Volume near average (within ±10%) |

The thresholds `1.1` (high) and `0.9` (low) are hardcoded constants. Price direction is derived from `price_diff` (latest close minus previous close).

### Scoring

**Volume does not contribute to the composite score.** It is purely informational context — a witness to whether price moves are backed by participation. All scored indicators (RSI, MACD, EMA, SMA, ADX, ROC, Stochastics, Bollinger, Fibonacci, VWAP, Ichimoku) remain independent of this display.

### Positioning in xoksa

Volume is displayed after the price section and before indicator scoring, so it frames the "quality" of the market conditions before the scored signals are read. It is also injected into the LLM prompt with a no-fabrication instruction — the LLM must use the computed ratio and comment, not invent its own volume assessment.

> For conceptual background, see **Investor Guide — Section 3: Volume**.
> For practical interpretation patterns, see **Strategy Guide — Section 4: Reading Volume Data**.

---

## Calculation Engine Classification

To ensure accuracy and transparency of calculations, xoksa clearly distinguishes between where the standard Rust technical analysis library `ta` crate is used, and where xoksa's own original calculations based on its design are applied.

### Indicators Using the `ta` Crate (Standard Logic)
- RSI (Relative Strength Index)
- MACD (Moving Average Convergence Divergence)
- EMA (Exponential Moving Average)
- SMA (Simple Moving Average)
- Bollinger Bands

### Original Calculation Indicators (Proprietary Algorithms)
- **ADX**: Wilder smoothing implementation of Directional Movement Index (RMA)
- **ROC**: Original range judgment for recent price rate of change
- **Stochastics**: Implementation based on period high/low extraction
- **Fibonacci**: Automatic level calculation from recent high/low
- **VWAP**: Volume-weighting of Typical Price for each bar. Intraday uses session VWAP (daily reset) for all same-session bars as the indicator calculation bar; daily uses most recent N-bar VWAP (default: 14 / configurable with `--vwap-period`)
- **Ichimoku**: Original scoring focused on Tenkan-sen and Kijun-sen

---

## Score Synthesis and Judgment Logic

xoksa synthesizes the individual indicator scores described so far to calculate the final "reference" for investment judgment.

### 1. Per-Indicator Weighting (Weights)

Each indicator has a user-configurable "weight (Weight)."

- Default is 1.0 for every extension indicator; the basic block (RSI / MACD, `WEIGHT_BASIC`) defaults to 2.0
- Increase the weight of indicators you want to emphasize (e.g., 2.0)
- Decrease the weight of indicators you want to treat as reference only (e.g., 0.5)

Set the **default** in `xoksa.env` (`WEIGHT_EMA=2.0`, …), or change it **live for a session** in chat with `/set weight-<indicator> <n>` (e.g. `/set weight-ema 1.5`). You can also drop an indicator from the analysis entirely for the session with `/set indicator <name> off` — it is still computed and stored, just not scored/shown.

### 2. Total Score Calculation (Principle of Weighted Average)

Each indicator's score (−2.0 to +2.0) is multiplied by its weight, and the total is calculated.

$$TotalScore = \sum (Score_i \times Weight_i)$$

### 3. Normalization (Gauge Conversion)

The total score is divided by the total weight to give a **score ratio** between −1.0 and +1.0. The gauge and the verdict label are both based on that ratio, so a stock with heavy weights is comparable with one that uses the defaults.

$$ScoreRatio = \frac{\sum (Score_i \times Weight_i)}{\sum Weight_i}$$

The ratio maps to nine verdict levels, symmetric around neutral:

| Score ratio | Verdict |
| ---: | :--- |
| +0.8 or above | 🟢 Strong buy |
| +0.6 or above | 🟢 Buy-leaning |
| +0.4 or above | 🟢 Mild buy |
| +0.2 or above | 🟡 Slightly bullish |
| 0.0 or above | ⚪️ Neutral |
| −0.2 or above | 🟠 Slightly bearish |
| −0.4 or above | 🟠 Mild sell |
| −0.6 or above | 🔴 Sell-leaning |
| −0.8 or above | 🔴 Strong sell |

The gauge itself is drawn as a bar with the ratio shown as a percentage (e.g. `+59%`), so you can see at a glance how far from neutral the reading sits.

### 4. Flexibility of Judgment

xoksa is not a black-box tool that says "you must buy at this value."
By outputting the breakdown of "which indicator at which weight contributed to this score" to the LLM and terminal, the purpose is to provide **"advanced deliberation material"** for users to make their own final judgments.

---

> [!TIP]
> **💡 Operational Tips**
> - When indicators "fight" each other (one is buy, the other is sell), the score converges near 0 and displays as "neutral."
> - Strong signals appear only when many indicators are pointing in the "same direction."
> - Utilizing strategy recipes to find the combination of indicators that suits your investment style is the shortcut to mastering xoksa.

---

## Fundamental Metrics (PER / PBR / ROE / EPS / BPS / Dividend)

Enabled with `--fundamental`. Supplementary company financials are fetched for the ticker — **J-Quants** for Japanese stocks (JPY) and **SEC EDGAR** for US stocks (USD) — and folded into the LLM context alongside the technicals.

> **These metrics are NOT part of the buy/sell composite score.** Unlike the technical indicators above, fundamentals are not scored or weighted. They are objective context the LLM explains; the LLM never recomputes or invents them.

### What xoksa Fetches vs. Computes

| Metric | Source | How |
| :--- | :--- | :--- |
| EPS (earnings per share), BPS (book value per share) | Filing | Fetched from the latest disclosed period (quarter or full year) |
| Revenue, operating income, net income, equity, shares outstanding | Filing | Fetched |
| Dividend (per share) | Filing | Actual when available, otherwise forecast (flagged) |
| **PER** (price / earnings) | Computed by the program | `latest price ÷ EPS` |
| **PBR** (price / book) | Computed by the program | `latest price ÷ BPS` |
| **ROE** (return on equity) | Computed by the program | `net income ÷ equity` |

All three computed ratios guard against a zero denominator (returned as "not available" rather than infinity).

### How to Read Each

- **PER (Price/Earnings ratio)**: roughly how many years of current earnings the price represents. Lower = cheaper relative to earnings; always compare against the sector and the company's own history. Not shown when EPS is zero; a loss-making company yields a negative PER, which is a signal in itself rather than a valuation.
- **PBR (Price/Book ratio)**: price versus net assets per share. Below 1.0 means trading under book value — which can be a bargain *or* a value trap.
- **ROE (Return on Equity)**: how efficiently shareholder capital generates profit. Higher is better; low PER + high ROE is the classic "cheap and good" combination.
- **EPS / BPS**: the per-share earnings / book value that PER / PBR are built on.
- **Dividend**: annual dividend per share. Check the forecast flag — a forecast value is labeled as such, an actual (reported) value is not.

### Important Notes

- **Price-derived ratios are point-in-time**: PER and PBR use the latest observed price *at the moment fundamentals were fetched*. They are **not** recomputed by `/reload` (which refreshes technicals only), so during a session the displayed PER/PBR reflect the fetch-time price.
- **Update frequency**: fundamentals are reported quarterly/annually, so they change far less often than price.
- **Reporting period (which figures)**: the panel reflects the **latest disclosed filing** — a quarterly 10-Q or an annual 10-K — shown with a period label (e.g. `2026-04-26 (Q1)`, `2026-01-25 (FY)`). Every income-statement figure and the ratios are on that same period's basis, so a **quarterly** panel's PER/ROE are computed on a single quarter's earnings and read higher (PER) / lower (ROE) than an annualized figure — the period label tells you the basis. Both markets (J-Quants for JP, SEC EDGAR for US) follow the latest disclosure.
- **Source/market differences**: field availability and accounting conventions differ between J-Quants (JP) and SEC EDGAR (US); some fields may be absent for a given company. Currency and units are shown as provided and are never converted.

---

---

# Part 3 — Strategies & recipes

## **— Master Steel-Solid Logic with 11 Recipes —**

> [!IMPORTANT]
> **⚠️ Disclaimer**
> The recipes and configuration examples described in this document are **examples only** for explaining how to use xoksa, and do not recommend buying or selling any specific stocks.
> When making actual investments, please make sure you correctly understand the nature of each indicator and **always make your final decision at your own responsibility.**

---

## 1. Purpose of This Document

This document explains **how to think about using xoksa**.

- How to design strategies (recipes)
- How to verify and reuse analysis results

---

## 2. How you analyze in XOKSA (conversation mode)

XOKSA's main workflow is the **browser dashboard + chat** (`xoksa serve --ui`, then open `http://127.0.0.1:8787`). You don't re-run a command per idea — you load a ticker, read the confirmed (SOT) analysis, and then **talk to it**: refine the reading, switch timeframe, ask "what if," and stress-test a recipe live. The indicator values stay fixed (Source of Truth); what you change is the timeframe, the response style, the model, and the questions you ask.

**A typical session**

1. **Load** a ticker in the header (add rivals with `/sym add <symbol>` — up to 5 to compare).
2. **Pick the timeframe** from the header dropdown (`1m` … `daily` … `monthly`).
3. **Read** the technical panel — the same source-of-truth scores the CLI prints.
4. **Ask** the chat: put your own read to the AI and let it explain the confirmed data (it never invents numbers).
5. **Refine live**: `/set` a threshold, `/depth` / `/scope` for how the AI interprets, `/forum` for a multi-LLM second opinion.
6. **See it**: drag-select a range on the chart popup and ask the AI about exactly that window; run the 🧪 **Backtest** panel to check whether a rule beats Buy & Hold.

**What you change live vs. what you set once**

| You want to change… | Where | Note |
| :--- | :--- | :--- |
| Timeframe | Header dropdown (or `/mode`) | live |
| Thresholds & calc params (RSI, MACD, Bollinger, ADX/ROC/Stoch/VWAP periods…) | Chat `/set <field> <value>` | live, **session-only** — your `xoksa.env` is untouched |
| **Indicator weights** | Chat `/set weight-<indicator> <n>` | live, **session-only** |
| **Which indicators are active** for the analysis | Chat `/set indicator <name> on\|off` | live, **session-only** — score/display/LLM only; still computed & stored |
| **Interpretation stance** (buyer / holder / seller) | Chat `/set stance <buyer\|holder\|seller>` | live, **session-only** — gauge orientation + LLM lean; not the score |
| Response style (interpretation depth, knowledge scope, forecasting) | Chat `/depth` `/scope` `/cast` | live |
| Model / second opinion | Header model dropdown, `/llm`, `/forum` | live |
| **Which indicators are computed/stored (the DB baseline); default weights & stance** | **`xoksa.env`** | just the **defaults** — edit only to change a default; a recipe needs none of this |

So each **recipe** below drives **everything live in chat** — no `xoksa.env` editing: pick the timeframe, `/set` the weights, `/set stance`, `/set indicator` to focus, `/set` the thresholds, and ask the AI the recipe's question. (`xoksa.env` only holds the *defaults*, and the default indicator set is already all-on.)

> `/set` is the conversational equivalent of a one-off override: it applies for the session only and never edits your file, so you can try a recipe's thresholds, **weights**, or **active indicators** and revert instantly (`/set reset`). You only edit `xoksa.env` to change a **default** — e.g. to add an indicator to the computed/stored set. See [command-reference.md](command-reference.md). (The same analysis is also available headless from the CLI — see §6.)

---

## 3. Top 11 Battle-Ready Recipes

### ① [Trend-Following] Textbook Trend Follow

The most standard setup for riding an uptrend wave and confirming that momentum has not faded.

- **Timeframe**: `daily` for a swing trend; drop to `60m` / `15m` for a shorter horizon.
- **Ask**: *"Is this a clean trend-follow entry right now — from the EMA/SMA and ADX state, and whether RSI still has pullback room?"*
- **Read the answer**
    - **EMA/SMA score**: ideal `+1`~`+2`; "Golden Cross in progress" is good, `0` or minus means the trend is stalling.
    - **ADX score**: `+1` (30+) or `+2` (50+), and rising → trend continuity is high.
    - **Gauge**: aim for `🟢 Strong buy` / `🟢 Buy-dominant`; if ADX is low and only EMA is good, suspect a "temporary rebound." **Criteria met** = MA shape good + ADX `+1`↑ + RSI holding 40+.
- **Sharpen it (optional)**: bias the read toward this recipe — `/set weight-ema 2.0 /set weight-sma 2.0 /set weight-adx 2.5`, `/set stance buyer`, `/set buy-rsi 40` (RSI slightly high to catch pullbacks); mute the rest with `/set indicator <name> off`.

### ② [Trend-Following] Ichimoku — Conversion Line Against Base Line

Emphasizes whether the short-period average of the range has pulled clear of the longer one.

- **Timeframe**: **`daily`** (required — Ichimoku's 9/26 settings (Tenkan/Kijun — xoksa draws no Senkou Span) are calibrated for daily bars; see the note).
- **Ask**: *"Has the conversion line pulled clear of the base line, and by how much? Judge from the Ichimoku reading alone."*

> **Note on bar interval**: xoksa's Ichimoku is the Tenkan-sen (9) and Kijun-sen (26) only — no Senkou Span, and so no band between them. Both are calibrated for daily bars. On short intervals (`1m`/`5m`/`15m`/`30m`/`60m`), the Kijun-sen spans only a fraction of a session and loses its medium-term equilibrium meaning.

- **Read the answer**
    - **Ichimoku score**: the divergence rate between the conversion line and the base line, cut at ±4.0% and ±1.0% — `+2` is the widest bullish gap, `0` means the two sit within 1% of each other, and a negative score means the conversion line is below.
    - **Message**: the wording follows that gap. There is no band to be above or inside — the base line is the only level the score reacts to.
    - **Gauge (holder)**: keep `🟡 Bullish tendency`↑; a drop to `⚪️ Wait-and-see` or below is a watch. **Criteria met** = conversion > base + SMA `+1`↑.
- **Sharpen it (optional)**: `/set weight-ichimoku 3.0 /set weight-sma 1.0`, `/set stance holder`; mute the rest with `/set indicator <name> off`.

### ③ [Trend-Following] Volatility Breakout

Aims for the explosion after Bollinger Bands converge (squeeze) and store power.

- **Timeframe**: `daily` or `60m` — breakouts read on either; pick the horizon you trade.
- **Ask**: *"Is this a squeeze about to break? Check the Bollinger bandwidth squeeze and whether ADX is turning up on the break."*
- **Read the answer**
    - **Bollinger message**: look for `⚠️ Bandwidth squeezed to 8.00% or below → Watch for strong trend in breakout direction`.
    - **Bollinger score**: goes to `-1`~`-2` when it starts breaking the upper band; flat during the squeeze (`0`). **The sign is a mean-reversion reading, not a direction**: xoksa scores a close above the band as overheated, so an upward break shows up negative. In a breakout recipe read its magnitude as "how far outside the band". The **direction** comes from which band the close left; **ADX gives the strength** of the move, not its direction — it does not distinguish up from down.
    - **ADX score**: if ADX rises with the breakout to `+1` (30+), the breakout is likely "real." **Criteria met** = squeeze warning + price at the upper band + ADX turning up.
- **Sharpen it (optional)**: `/set weight-bollinger 3.0 /set weight-adx 2.0`, `/set bb-squeeze 8` (the "calm before the storm" threshold); mute the rest with `/set indicator <name> off`.

### ④ [Trend-Following] Short-term Decisive Battle — Momentum Focus

Places ROC (price rate of change) as the lead and chases stocks that surge sharply in a short period.

- **Timeframe**: **short intraday** (`5m` / `15m` / `60m`) for a short decisive battle; `daily` for a multi-day surge.
- **Ask**: *"Is momentum still accelerating? Read ROC (peaking when it slips from +2 to +1) and whether the last-24h news is a tailwind."*
- **Read the answer**
    - **ROC score**: stays `+2` during the surge; a drop from `+2` to `+1` is a peak-out (short-term profit-taking).
    - **News (LLM)**: check whether the last-24h (`pd`) title+URL candidates are worth opening at the source.
    - **Gauge**: ride only while `🟢 Strong buy` shows; don't chase once the score deteriorates fast. **Criteria met** = ROC `+2` + EMA `+1`↑ + recent news a tailwind.
- **Sharpen it (optional)**: `/set weight-roc 3.0 /set weight-ema 1.0`; mute the rest with `/set indicator <name> off`. *(Last-24h news needs `NEWS_FRESHNESS=pd` in env.)*

### ⑤ [Contrarian] Oversold Rebound (Basic Form)

Logically catches the textbook rebound from "oversold."

- **Timeframe**: match your horizon — `daily` for a swing bounce, intraday for a quick one.
- **Ask**: *"Is a rebound base forming — RSI ≤30 with the MACD/signal gap converging below 1.0?"*
- **Read the answer**
    - **RSI**: below `30`; below `20` raises the rebound case ("extreme oversold") but the continued-decline risk coexists.
    - **MACD message**: with the gap below 1.0 it reads as "convergence (bottoming)"; near `0` = rebound preparation complete.
    - **Gauge**: watch for the turn from minus to plus (e.g. `🟡 Bullish tendency`). **Criteria met** = RSI ≤ 30 + MACD gap ≤ 1.0.
- **Sharpen it (optional)**: `/set stance buyer /set weight-basic 2.5` (RSI/MACD-led), `/set buy-rsi 30 /set macd-minus-ok on /set macd-diff-low 1.0`; mute extensions with `/set indicator <name> off`.

### ⑥ [Contrarian] Panic Selling — Bottom Fishing

An aggressive setup aiming for the moment when the market reaches total pessimism and a selling climax.

- **Timeframe**: the timeframe of the panic — `5m` / `15m` for a session climax, `daily` for a multi-day washout.
- **Ask**: *"Panic level? RSI ≤20, and whether price is re-entering the lower Bollinger band after breaking −2σ."*
- **Read the answer**
    - **RSI**: panic below `20`.
    - **Bollinger score**: `+2` (at least 2% below the lower band) = still falling; the return to `+1` then `0` (band re-entry) is the ideal bottom-fish point. A close below the band scores **positive** — the score reads mean reversion, not direction.
    - **Gauge / LLM**: even at `🔴`/`🟠`, a summary noting "slowing decline" or "divergence correcting" is material for a test buy. **Criteria met** = RSI ≤ 20 + band re-entry + MACD deterioration stopped.
- **Sharpen it (optional)**: `/set stance buyer /set weight-bollinger 2.5 /set weight-basic 1.5`, `/set buy-rsi 20 /set macd-minus-ok on`; mute the rest with `/set indicator <name> off`.

### ⑦ [Contrarian] VWAP Regression (Mean Reversion Strategy)

Uses the property of stock prices that have diverged far from fair value (VWAP) being pulled back like a magnet.
xoksa's VWAP treats Typical Price weighted by volume as the price band. In intraday mode, only the final indicator calculation bar and same-day session bars are used (daily reset); in non-intraday modes (daily/weekly/monthly), the most recent N bars are used (default 14 / change it with `/set vwap-period`).

> **Note on bar interval**: VWAP is designed to reset at each session open. On short intervals (`1m`/`5m`/`15m`/`30m`/`60m`) that span multiple trading days, VWAP is computed across session boundaries and loses its intraday-anchoring meaning. For this recipe, use a single intraday session or a non-intraday timeframe (`daily` / `weekly` / `monthly`) from the header.

- **Timeframe**: **a single intraday session** or **`daily`** (VWAP needs a session anchor — see the note above). `/set vwap-period <n>` sets the daily window.
- **Ask**: *"How far is price stretched below VWAP, and is the divergence starting to narrow (mean-reversion setup)?"*
- **Read the answer**
    - **VWAP score**: `−2` = statistically "oversold" — the close is 3% or more **below** VWAP. (`+2` is the opposite case, the close stretched above it.)
    - **Price vs VWAP**: see how far the last bar's close is below the printed `VWAP: XXX.X`; a narrowing gap signals regression.
    - **Gauge**: floating up to `🟡`↑ is the phase to target convergence toward the average (rebound). **Criteria met** = VWAP `−2` + RSI ≤ 35 + gap narrowing.
- **Sharpen it (optional)**: `/set stance buyer /set weight-vwap 3.0`, `/set buy-rsi 35`; mute the rest with `/set indicator <name> off`.

### ⑧ [Defensive] Holder Alert Mode

A defense-oriented setup seeking the "exit timing" for held stocks.

- **Timeframe**: **`daily`** (a holder's watch horizon; `weekly` for a longer-term position).
- **Ask**: *"Any exit signal for a holder — RSI ≥65, an upper-band rejection, and the MACD gap narrowing?"*
- **Read the answer**
    - **RSI**: the sell zone is `sell-rsi` (default `70`); this recipe lowers it to `65` so a "profit-taking preparation" reading arrives earlier. The RSI value itself is printed plain — no colour marks it.
    - **Bollinger message**: watch `Price broke through upper band (overheated zone)`; pushed back inside afterwards hints at a ceiling.
    - **Gauge (holder)**: a swing toward `🟠 Sell tendency` = consider partial selling while profit remains. **Criteria met** = RSI ≥ 65 + upper-band rejection + MACD gap narrowing.
- **Sharpen it (optional)**: `/set stance holder /set weight-bollinger 2.0`, `/set sell-rsi 65 /set macd-diff-mid 5` (earlier warning); mute the rest with `/set indicator <name> off`.

### ⑨ [All-Directional] AI Second Opinion

Run all indicators at full power and have AI smoke out "inconsistencies between indicators."

- **Timeframe**: **`daily`** (Ichimoku / VWAP / Fibonacci need it — see the note).
- **Ask**: *"With every indicator on, where do they disagree? Point out the contradictions (e.g. trend +2 but oscillator −2) and my blind spots."* — or run `/forum ask` for a multi-LLM committee.
- **Indicators**: keep **all active** (the default; `/set reset` if you narrowed earlier).

> **Note on bar interval**: This recipe includes Ichimoku, VWAP, and Fibonacci — all three of which carry reduced statistical reliability on short intervals (1m/5m/15m/30m/60m). On daily bars these work as intended. In short-interval modes, treat their scores as contextual background rather than primary signals, and weight RSI, MACD, EMA, ADX, and Bollinger more heavily.

- **Read the answer**
    - **Score inconsistencies**: contradictions like "Trend-type (+2)" but "Oscillator-type (−2)" are the basis for the AI's "has momentum but overheated" read.
    - **Fibonacci / Ichimoku**: watch whether the AI flags confluence (a wide conversion-to-base gap + a target like the 38.2% retracement). The AI is never handed a Senkou Span or the band between them, so a mention of either is a figure it invented. Daily only — unreliable on short intervals.
    - **LLM summary**: rather than a "buy/sell" verdict, pick out the indicator the AI names as a "concern" and fill your own blind spots. **Criteria met** = all pointing the same way → high confidence; split scores → reason through short- and medium-term scenarios separately.
- **Sharpen it (optional)**: nothing to bias — the point is the full radar and the AI's contradiction-spotting.

### ⑩ [Information Warfare] Deep Dive into Catalysts and Themes

Keep technical as the base, and use AI to sort news title+URL candidates for source checking.

- **Timeframe**: any — news is timeframe-agnostic; keep your chart timeframe as you like.
- **Ask**: first pull the theme with `/nx add "業績予想 修正 増配"`, then *"Sort these headlines by confirmation priority (Tier A–C); which source URLs should I open first?"*
- **Read the answer**
    - **News (LLM)**: the AI sorts title+URL candidates into Tier A (high priority) ~ Tier C (reference). Open the listed URLs for what matters.
    - **Summary**: treat comments as *title-level* hints only — the article body was not read. **Criteria met** = a cluster of Tier A candidates + your own source check → an information-led setup.
- **Sharpen it (optional)**: nothing to tune — this recipe is all about the question and reading the triage.

### ⑪ [Value] Fundamental Check with Technical Timing

Screen for a fundamentally sound, reasonably-priced stock, then use a light technical overlay for entry timing. Needs fundamentals enabled.

- **Timeframe**: **`daily`** or higher — a value play works on a slow horizon.
- **Ask**: *"Is this cheap-and-good (reasonable PER/PBR, healthy ROE) with a timing entry — EMA/SMA no longer falling and RSI lifting off its low?"*
- **Read the answer** (value metrics are fetched from J-Quants / SEC EDGAR and computed by the program — the AI explains them, never recomputes or invents ratios)
    - **PER / PBR**: lower = cheaper vs earnings / book, but read against the sector and the company's own history. A very low PBR can be a value trap, not a bargain.
    - **ROE**: profitability / quality. Low PER + high ROE is the classic "cheap and good"; low PER + low ROE warrants caution.
    - **Dividend**: check actual vs forecast (xoksa flags forecast values).
    - **Technical overlay**: use EMA/SMA + RSI only for *timing* — a cheap stock still falling is not yet a buy; wait for the trend score to stop deteriorating. **Criteria met** = reasonable PER/PBR + healthy ROE + EMA/SMA no longer declining + RSI lifting off its low.
- **Sharpen it (optional)**: `/set stance buyer /set weight-ema 1.0 /set weight-sma 1.0`, `/set buy-rsi 35`; mute the rest with `/set indicator <name> off`. *(Fundamentals need env — `FUNDAMENTAL=true` + a J-Quants/EDGAR key.)*

> **Note on data freshness**: Fundamentals are reported quarterly/annually and change far less often than price. Re-running intraday does not move the fundamental figures — only the technical overlay updates.

---

## 4. Reading Volume Data

xoksa displays three volume values and a directional comment for every analysis run. This section explains what they mean and how to use them.

### What the Display Shows

```
📈 Volume: 92,980,700 shares
📈 Avg volume (20 bars): 240,374,445 shares / ratio: 0.39x
   Low vol + up: sustainability uncertain
```

| Field | Meaning |
| :--- | :--- |
| **Volume** | The most recent bar with confirmed trades (non-zero). Yahoo Finance sometimes appends a zero-volume placeholder bar at the tail of intraday data; xoksa skips it and uses the last bar that actually has volume. |
| **Avg volume (N bars)** | Simple average of the last N bars with non-zero volume. N defaults to the `sma_long_period` (20). Zero-volume bars are excluded from the average. |
| **Ratio** | `latest ÷ average`. Ratio = 1.00x means exactly average participation. Ratio = 0.39x means only 39% of typical participation was present. |

### The Comment Patterns

| Ratio range | Price direction | Comment | Interpretation |
| :--- | :--- | :--- | :--- |
| `= 0.0` | any | Zero volume (no trades in this bar) | The bar had no confirmed trades — do not draw conclusions from it |
| `≥ 1.1×` | up | High vol + up: possible upward momentum | Buyers are committing with size; the move has backing |
| `≥ 1.1×` | down | High vol + down: possible selling pressure | Sellers are committing with size; watch for follow-through |
| `≥ 1.1×` | flat | High vol, price unchanged | High activity but no directional agreement — often a pivot or congestion zone |
| `< 0.9×` | up | Low vol + up: sustainability uncertain | Price rallied on weak participation — the example above. Rallies without volume backing often fade or reverse. |
| `< 0.9×` | down | Low vol + down: possible thin-volume decline | A decline on light volume can mean low conviction selling — easier to reverse than a high-volume drop |
| `< 0.9×` | flat | Low vol, price unchanged | Participants have stepped back; wait for a volume signal before acting |
| `0.9×–1.1×` | any | Volume near average (within ±10%) | Ordinary session; no volume-based edge either way |

### Reading the Example

```
Volume: 92,980,700 shares
Avg volume (20 bars): 240,374,445 shares / ratio: 0.39x
Low vol + up: sustainability uncertain
```

Ratio 0.39× means roughly 60% of typical volume was absent during a price rise. In technical analysis this is a classic warning: the rally is not backed by broad market participation. Possible causes include:

- Mechanical short-covering rather than genuine buying demand
- A thin-market bounce off a support level with no follow-through buyers waiting
- Time-of-day effects (pre-market open, lunch lull, pre-close thinning)

**This does not mean "sell immediately"** — price can continue higher even on low volume if the broader trend is intact. But it raises the bar for conviction. Cross-check with other indicators:

- If ADX is strong (≥ 40) and EMA/SMA are golden-cross, the trend may simply be continuing with normal intraday ebb in participation → treat the volume warning as a note, not a stop signal.
- If MACD is negative and Stochastics are in overbought territory despite the price rise → the low-volume rally is a higher-risk setup and profit-taking pressure is more likely.
- If VWAP divergence is still large and positive → the price has not yet recovered to fair value; a low-volume rally toward VWAP is structurally normal, not alarming.

### Combining Volume With Recipes

| Recipe | Volume signal to watch |
| :--- | :--- |
| ① Trend Follow | Confirm ratio ≥ 1.0× on the golden-cross bar. A cross on 0.3–0.5× volume has a higher failure rate. |
| ③ Bollinger Breakout | A squeeze breakout should show ratio ≥ 1.2× to distinguish real moves from false breaks. |
| ④ Momentum | ROC score +2 with ratio ≥ 1.5× is high-conviction. If ROC is +2 but ratio is 0.4×, momentum may be exhausted. |
| ⑤⑥ Contrarian | Look for a spike (ratio ≥ 2×) at the lowest RSI reading — that is the selling climax signal for a potential bottom. |
| ⑦ VWAP Regression | On the recovery leg back toward VWAP, ratio ≥ 1.0× validates the move. Low-volume recovery often stalls before reaching VWAP. |

### Data Notes

- Volume data comes from Yahoo Finance OHLCV bars and is measured in **shares** for equities. It is not transaction count or yen value.
- The average period is `sma_long_period` (default 20 bars). To use a longer reference, raise `--sma-long-period` or set `SMA_LONG_PERIOD` in `xoksa.env`.
- Daily mode: each bar is one trading day; volume represents that day's total shares traded.
- Intraday mode (1m/5m/15m/30m/60m): each bar is one interval; the average is the mean across the last 20 intervals of that length, not across full days.

---

## 5. Choosing the Right Timeframe

The most important decision when using xoksa is matching your **analysis intent** to the **timeframe**. The timeframe you choose determines exactly which data xoksa fetches — and therefore what the LLM can see.

> **Key principle:** If you ask the LLM to "describe the 1-month movement" but the data covers only the last 5 trading days, the LLM cannot comply. It can only work with what is in the data. Mismatching intent and timeframe produces misleading analysis.

### Data Coverage per Mode

xoksa fetches a fixed span of historical data from Yahoo Finance on every run. **There is no local accumulation** — each execution retrieves a fresh dataset for the configured period.

| Mode | Acquisition period | ~Bar count | Time depth visible to LLM |
| :--- | :--- | ---: | :--- |
| `1m` | Last 5 trading days | ~1,950 | ~5 days |
| `5m` | Last 5 trading days | ~390 | 5 days |
| `15m` | Last 1 month | ~520 | ~1 month |
| `30m` | Last 1 month | ~260 | ~1 month |
| `60m` | Last 2 months | ~250 | ~2 months |
| `daily` | Last 3 months | ~65 | ~3 months |
| `weekly` | Last 2 years | ~104 | ~2 years |
| `monthly` | Last 10 years | ~120 | ~10 years |

> **Bar count ≠ calendar depth.** `5m` mode produces ~390 bars from just 5 trading days. The same 390 bars in `daily` mode would span more than a year and a half. Bar count and calendar depth are fundamentally different things.

### Yahoo Finance Intraday Data Limits

Yahoo Finance enforces a hard upper bound on how far back intraday data is available, regardless of what you request:

| Interval | Yahoo Finance maximum retention |
| :--- | :--- |
| 1m | ~7 days |
| 5m | ~60 days |
| 15m | ~60 days |
| 30m | ~60 days |
| 60m | ~730 days |
| daily / weekly / monthly | Decades |

xoksa's acquisition periods are set well within these limits. Even if you extended the period for `5m` mode, no data beyond ~60 days would exist to fetch.

### Matching Intent to Timeframe

| Analysis intent | Recommended mode | Why |
| :--- | :--- | :--- |
| Right-now scalping / entry micro-timing | `1m` | Finest resolution — but only the last ~5 sessions exist and every indicator is very noisy |
| Today's intraday move | `5m` or `15m` | Data covers the current and recent sessions in detail |
| This week's move | `30m` or `60m` | Multi-session view with less noise than 5m/15m |
| Last month's movement | `daily` | 1 bar = 1 trading day; ~65 bars covers ~3 months |
| 6-month to 1-year trend | `weekly` | 1 bar = 1 week; 2-year range shows clear trend structure |
| Multi-year macro view | `monthly` | 1 bar = 1 month; 10-year range for long-term positioning |

**Example of a mismatch:** On the `5m` timeframe, asking the LLM "how has this stock moved over the past month?" — the LLM receives data covering only the last 5 trading days and has no information about the preceding 3+ weeks. No amount of prompt engineering can reconstruct data that was never fetched.

### Timeframe Characteristics

**`1m` — Tick-level scalping**

- Finest resolution; extreme noise — signals flip constantly
- Only the last ~5 trading sessions exist (Yahoo keeps ~7 days of 1m data)
- The indicator defaults (RSI 14, MACD 12/26/9, Ichimoku 9/26 (Tenkan/Kijun; xoksa draws no Senkou Span), Bollinger 20) were calibrated for **daily** bars — on 1m they measure minute-scale wiggles, not trend; read them as micro-momentum, never as direction
- Best for: entry/exit **micro-timing** inside a setup you already found on a higher timeframe — not for judging a stock's trend

**`5m` / `15m` — Session-level**

- Within a single or a few trading sessions; signals turn quickly
- High bar count, high noise
- Best for: intraday momentum bursts (earnings gap, news spike, opening range)
- Use with caution: Ichimoku (9 bars = 45 or 135 min — daily equilibrium meaning lost), Fibonacci (S/R spans hours only), VWAP (`15m` spans multiple sessions at 1-month range)

**`30m` — Multi-session intraday**

- Covers several weeks of intraday structure with better signal quality than 5m/15m
- VWAP spans multiple sessions: treat as context rather than primary signal

**`60m` (1-hour) — Intraday swing**

- Least noisy of the intraday modes; each bar represents a full hour of market activity
- Natural bridge between intraday and daily: shows multi-day moves at sub-daily resolution
- **Practical use:** identify a setup on `daily` (e.g., RSI oversold with EMA support), then switch to `60m` to wait for a MACD crossover or Bollinger squeeze break as the entry trigger
- VWAP: same multi-session limitation as `30m`
- Ichimoku: 9 bars = 9 hours / 26 bars = 26 hours — daily equilibrium meaning lost

**`daily` — Standard**

- All indicators function as originally designed
- RSI(14), MACD(12/26/9), Ichimoku(9/26; Tenkan/Kijun only), Bollinger(20) defaults were calibrated for daily bars
- Best starting mode for most users

**`weekly` / `monthly` — Higher timeframe**

- Dominant trend structure visible without daily noise
- A weekly bullish signal carries more duration than a daily one
- Use to establish macro bias before dropping to daily or intraday for entry timing

### Indicator Reliability by Timeframe

| Indicator | 1m–30m | 60m | Daily | Weekly/Monthly |
| :--- | :---: | :---: | :---: | :---: |
| RSI | ✓ | ✓ | ✓✓ | ✓✓ |
| MACD | ✓ | ✓ | ✓✓ | ✓✓ |
| EMA / SMA | ✓ | ✓ | ✓✓ | ✓✓ |
| Bollinger | ✓ | ✓ | ✓✓ | ✓✓ |
| ADX | ✓ | ✓ | ✓✓ | ✓✓ |
| Stochastics | ✓ | ✓ | ✓✓ | ✓✓ |
| ROC | ✓ | ✓ | ✓✓ | ✓✓ |
| Ichimoku | △ | △ | ✓✓ | ✓✓ |
| VWAP | ✓✓ (same session) / △ (multi-session) | △ | ✓ | ✓ |
| Fibonacci | △ | △ | ✓✓ | ✓✓ |

△ = Reduced reliability; treat as background context rather than a primary signal. `1m` is the noisiest end of the `1m–30m` column — read every indicator there as micro-momentum, not trend.

---

## 6. CLI (headless / automation)

The dashboard is the main way to use XOKSA, but the **same engine** runs from the command line for scripts, cron, and quick one-offs — the numbers are identical (SOT).

```bash
xoksa -t 7203.T             # one analysis, printed to the terminal
xoksa -t 7203.T --chat      # interactive chat in the terminal
xoksa -t AAPL,MSFT,NVDA     # compare several at once
```

A recipe's baseline can also be passed inline for a single run (without editing `xoksa.env`) using `-I` plus the indicator / weight / threshold flags — see [command-reference.md](command-reference.md) for the full list. Handy for automation; for exploration, the dashboard is easier.

---

## 7. Finally

> **The above are just examples of how to use the tool. Please feel free to customize and challenge yourself to create your own analysis parameters that suit you.**

**Stock Technical 'AI' Analysis Tool — Created & Designed by Kozo2000**

---

<a id="ja"></a>

---

# パート1 — 分析の読み方

[English is here.](#en)

## 1. 株式投資分析におけるxoksaの役割

株式投資の分析には、大きく分けて「ファンダメンタル分析」と「テクニカル分析」の2つがあります。

- **ファンダメンタル分析**: 企業の財務・業績やニュース（材料）から事業価値を評価します。
- **テクニカル分析**: 過去の株価や出来高のパターンからタイミングの文脈を評価します。

xoksaは、この**「テクニカル分析（客観的な数値）」と「最新材料（ニュース）」をAI（LLM）の脳を使って統合**し、あなたに「今の市場の体温」を解釈可能なレポートとして提供します。

> 各テクニカル指標の具体的な計算方法や見方は、**インジケーターガイド** を参照してください。

---

## 2. ツールとしての特性と推奨されるプレイスタイル

### チャートと、その裏づけの言語化

xoksa のブラウザ・ダッシュボードには対話的なチャートを内蔵しています——指標オーバーレイ、そして気になる区間をドラッグ選択してその範囲についてそのまま AI に聞けます。ただし作図で専門ツールと張り合うことは目的ではありません：TradingView や各証券会社の高度なチャートは非常に素晴らしく、xoksa は描画機能でそれらに対抗しようとはしません。強みは「目に見えるチャート」を補完する**「目に見えないデータの裏付け」を言語化すること**——そしてその場で問いに答えられることです。だから両者は競合ではなく補完の関係になります：

1.  **専用チャートサービス（TradingView・各証券会社）**: 全体のトレンド、抵抗線、パターンを視覚的に確認する。
2.  **xoksa**: 読めるチャートに加え、内部の数値（RSI, MACD, ニュースの文脈等）がその時どう連動しているかという論理的な読みを、言葉にして返す。

xoksa は単体でも、使い慣れたチャートサービスと併用しても機能します。いずれの場合も分析に深みと客観性を加え、あなたの判断プロセスをそっと支える「副官」のような存在を目指しています。

### 銘柄を選ぶツールではない

xoksa は銘柄の推薦・選定を*能動的には*一切行いません——機能の欠落ではなく、意図的な設計です（[Design Philosophy §6](../dev-prog/design-philosophy.md) 参照）。役割は、あなたが*すでに見ている*銘柄を読み解けるようにすること——読めるチャート、その銘柄にとっての指標の意味、その場で得られる根拠。判断材料を整理し、あなたが明示的に問えばデータの傾きと見立てが変わる条件までを（裸の売買指示ではなく）読みとして示します。判断はあなたに残します。

### 答えを「与えられる」のではなく、自ら「注文」する

xoksaの設計思想は、**「ユーザー自らが分析ロジックを注文（カスタマイズ）する」**ことを基本に置いています。

RSIの閾値をどこに置くか、移動平均の期間を何日にするか、テクニカルとニュースのどちらを重視するか。これら数十種類のパラメータをコマンド引数一つで自由自在に変更可能です。

「ツールに決められた評価」に従うのではなく、**「あなたの投資哲学（ロジック）」をツールに投影させ、それをAIに客観的なデータとして検証させる**。分析の主導権は常にユーザー側にあります。

### パラメータによる銘柄性格の最適化

投資戦略は、短期トレード、スイング、長期投資など人それぞれです。xoksaは多数の引数により、分析の「厳しさ」や「視点」を自由に変更できます。

- **買いたい時**: `-s buyer`（買い手視点）で分析。
- **売りたい時**: `-s seller`（売り手視点）で警戒ポイントを確認。

> 分析戦略のカスタマイズについては **ストラテジーガイド** を参照してください。

### 短時間足分析（1m / 5m / 15m / 30m / 60m）における注意事項

短い時間足モードを使用する場合、一部の指標は設計上の前提から乖離が生じ、数値の信頼性が変化します。これはxoksa固有の問題ではなく、各指標の統計的な性質に起因するものです。時間足が短いほどこの影響は強く、**1分足**で最も顕著になり（履歴も数日分のみ）、60分足で最も緩やかになります。

**短時間足で信頼性が大きく低下する指標：**

| 指標 | 理由 |
| :--- | :--- |
| **一目均衡表** | xoksa が計算するのは転換線（9）と基準線（26）だけで、先行スパンを持たないので、それに挟まれた帯もありません。どちらも日足を前提に設計された値です。5分足に同パラメータを適用すると、基準線が示す期間は約130分に圧縮され、中期的な均衡という本来の概念が成立しません。 |
| **VWAP** | VWAPは各セッション（1日）の始値でリセットされることを前提とした指標です。複数日の短時間足データをまたいで計算すると、日をまたいだ平均値となり、セッション内の価格重心という本来の意味を失います。当日1セッション内のデータに限れば有効です。 |
| **フィボナッチ** | データ期間の高値・安値レンジから水準を導出するため、短時間足で値幅が極端に小さい場合、各レベルの差がノイズ水準に埋もれ、判断材料としての重みが下がります。 |

**解釈の軸が異なる指標：**

| 指標 | 補足 |
| :--- | :--- |
| **ボリンジャーバンド** | 20期間以上のデータがあれば統計的には有効です。5分足では約100分間の変動幅を示すバンドとなり、計算自体は成立しますが、「広い・狭い」の判断は日足とは異なる軸で解釈する必要があります。 |
| **ADX** | 5分足で14期間を使用すると約70分間の方向性強度を表します。数値は正しく計算されますが、日足で言うところの「トレンド継続性」とは意味が異なります。 |

RSI・MACD・EMA・SMA・ROC・ストキャスティクスは短時間足での使用実績が広くあり、固有の問題はありません。

短時間足モードを使用する際は、一目均衡表・VWAP・フィボナッチの値を主要シグナルとして扱うのではなく、あくまで補足的な文脈として参照することを推奨します。

### 本ガイドのスコア表の読み方

**閾値ちょうどは、その閾値を満たしたものとする。** xoksa のすべてのスコアがこの 1 つの規約に従うので、指標ごとに別の読み方を覚える必要はありません。境界にちょうど乗った値は、その境界が定める帯に属し、中立帯は両端が開きます。RSI ちょうど 30 は売られすぎ、乖離率ちょうど +2.0% は `+2`、終値がボリンジャー上限にちょうど乗っていれば外側、終値が 38.2% 水準にちょうど乗っていれば `+2` です。

2.9.8 まではコード側に 4 通りの規約が混在しており、本ガイドの表も、適用される規約と一致していない箇所がありました。以下の各表は不等号で帯を示し、行の間に重複はありません。

---

## 3. 出来高（Volume）— 何を測り、なぜ重要か

### 出来高とは何か

出来高は、1本のバー期間中に売買が成立した**株数の合計**です。約定件数（注文の成立回数）でも、売買代金（円や米ドルの金額）でもありません。xoksa が `92,980,700株` と表示している場合、そのバーの期間中に9,298万株が取引されたことを意味します。

### 価格と出来高をセットで見る理由

出来高を伴わない価格変動は、証拠のない判決のようなものです——結論は出ていても、その背後にある確信は薄い。出来高は**「その価格水準に何人が同意したか」**を示します。

- 高い出来高を伴う上昇は、多くの買い手が積極的に参加した動きです。幅広い支持があります。
- 低い出来高での上昇は、一部の参加者だけが価格を動かした状態です。売り圧力を吸収する次の買い手が少なく、値動きが崩れやすい傾向があります。
- 高い出来高を伴う下落は、参加者が規模を持って売った（あるいは損切りした）ことを意味し、より深刻なシグナルです。
- 低い出来高での下落は、売り手が少なく下げの確信が薄いことを示すことが多く、切り返しやすい傾向があります。

いずれも確実な予測ではありませんが、次に何が起こるかの確率分布を変えます。

### xoksa の表示内容

```
📈 出来高: 92,980,700株
📈 平均出来高 (20本): 240,374,445株 / 倍率: 0.39x
   出来高減 + 価格上昇: 反発の持続性には確認が必要
```

| 値 | 意味 |
| :--- | :--- |
| **出来高** | 取引が確認された最新バーの株数。Yahoo Finance はイントラデイデータの末尾にvolume = 0のプレースホルダーを返すことがあるため、xoksa はそれをスキップして直前の実取引済みバーを採用する。 |
| **平均出来高** | 直近20本（volume > 0のバーのみ）の単純平均。期間は `sma_long_period` 設定（デフォルト20本）を共用。 |
| **倍率** | 最新出来高 ÷ 平均出来高。1.00× が平均的な参加量。0.39× は通常の約60%の出来高しか発生していないことを意味する。 |

### 絶対値ではなく倍率で比較する

NTT（9432.T）の倍率 0.39× と地方小型株の倍率 0.39× は、絶対株数が100倍以上違っていても、同じことを意味します——「平均より参加者が少ない」。**意味を持つのは倍率であり、絶対株数はその銘柄の特性に過ぎません。**

日本株と米国株の絶対出来高を比較することにも意味はありません。単元株数の慣行、市場規模、売買単位がまったく異なります。

### 分足モード・日足モード・上位足モードの違い

| モード | 1本のバーが表す出来高 |
| :--- | :--- |
| 日足（`--analysis-mode daily`）| その日の全取引時間の合計株数 |
| 分足（1m / 5m / 15m / 30m / 60m）| そのインターバル内だけの株数。昼休み前後の薄商い5分足は構造的なもので、異常ではない |
| 週足 / 月足 | 1週または1ヶ月分の株数 |

分足モードでの20本平均は、その足種の直近20インターバルの平均であり、20取引日分の合計ではありません。週足/月足モードでは、直近20本は20週または20ヶ月を意味します。昼休み中の薄い分足バーの倍率低下と、日足・週足・月足での出来高低下は同じ「0.4×」でも意味が異なります。

### データソースについて

出来高データは Yahoo Finance の OHLCV バーから取得しています。主要上場市場での報告出来高を反映します。外国証券の日本市場上場銘柄や、場外取引の比率が高い銘柄では、実際の市場全体の売買を過小に見積もる可能性があります。

> 実践的な解釈パターンやレシピ別の活用方法については、**ストラテジーガイド — Section 4: 出来高データの読み方** を参照してください。

---

## 4. 対応市場について

xoksaは、利用中の市場データAPIがデータを提供している市場であれば、グローバルに対応可能です。

- **日本市場**: ティッカー末尾に `.T` を付与（例: `7203.T`）
- **米国市場**: ティッカーをそのまま入力（例: `AAPL`, `NVDA`）
- **その他市場**: 利用中の市場データAPIで検索可能なティッカーであれば、ロジックは共通して適用されます。

---

## 5. ⚠️ 投資に関する重要事項（免責事項）

投資は、期待に反して損失を被るリスクを伴います。本ツールを利用するにあたり、以下の点を必ず承諾したものとみなします。

1.  **自己責任の原則**: 投資判断は、必ずご自身の責任と判断において行ってください。本ツールの分析結果は、特定の金融商品の売買を推奨するものではありません。
2.  **情報の正確性**: 外部市場データAPI、ニュース検索API、およびAI（LLM）の生成結果には、遅延や誤りが含まれる可能性があります。
3.  **無保証**: 本ツールの利用により発生したいかなる損害（直接的・間接的・派生的な損失）についても、開発者は一切の責任を負いません。

市場は常に変化しており、過去のパターンが未来の結果を保証することはありません。余裕資金の範囲内で、冷静な運用を心がけてください。

---

# パート2 — 指標とスコア

## 指標の表示順（重み順）

指標は — 端末の分析表示・`/show technical`・LLMプロンプト（すべて同一の順序）で — **設定した重みの降順（大きいものが先頭）**に並びます。重みは各指標をどれだけ重視するかを表す設定値です（`WEIGHT_BASIC`・`WEIGHT_EMA`・`WEIGHT_SMA`・`WEIGHT_BOLLINGER`・`WEIGHT_ROC`・`WEIGHT_ADX`・`WEIGHT_STOCHASTICS`・`WEIGHT_FIBONACCI`・`WEIGHT_VWAP`・`WEIGHT_ICHIMOKU`）。既定は `WEIGHT_BASIC` が `2.0`、それ以外は `1.0` です — 初期状態では基本解析が先頭になります。RSI と MACD は単一の `WEIGHT_BASIC` を共有します。

- **最も重みの大きい指標が常に先頭になります。** 固定のカテゴリ順はありません。先頭は状況に対する*あなた*の優先度を反映し、ハードコードされた序列ではありません。
- **重みが同値の指標は順序が固定されます** — 基本解析が先、続いて有効な拡張指標が既定の順。同じ設定なら常に同じ並びになります。
- 表示されるのは有効な指標のみです。無効化した指標は計算・保存はされますが表示・送信されません。
- 並び順は表示だけの話で、計算済みの値や総合スコアを一切変えません。

---

## 分析足と指標解釈（日足 / 分足 / 週足 / 月足）

xoksa の指標計算式は、日足・分足・週足・月足の各モードで同じです。
ただし、参照している「1本」の意味が変わるため、指標が見ている時間幅と売買判断上の解釈は変わります。

- 日足モード: 入力は日足
- 1分足モード: 1本 = 1分（最も細かい。取得できる履歴は数日分のみ）
- 5分足モード: 1本 = 5分
- 15分足モード: 1本 = 15分
- 30分足モード: 1本 = 30分
- 60分足モード: 1本 = 60分
- 週足モード: 1本 = 1週
- 月足モード: 1本 = 1ヶ月

そのため、同じ RSI(14) や MACD(12,26,9) でも、数式上の意味は変えずに、日足・1分足・5分足・15分足・30分足・60分足・週足・月足のどの時間軸として読むかを切り替える必要があります。分足モードはデイトレ完全対応ではなく、短期傾向を見るための短期分析モードです。週足・月足は上位足確認であり、別のスコアモデルではありません。

### 代表例

- RSI(14)
  - 日足: 日足14本の過熱感
  - 分足: 指定した分足14本の過熱感
  - 週足/月足: 週足または月足14本の過熱感

- MACD(12,26,9)
  - 日足: 日足12本 / 26本 / 9本ベースのモメンタム
  - 分足: 指定した分足12本 / 26本 / 9本ベースの短期モメンタム
  - 週足/月足: 週足または月足12本 / 26本 / 9本ベースのモメンタム

- EMA(20)
  - 日足: 日足20本の平均
  - 分足: 指定した分足20本の平均
  - 週足/月足: 週足または月足20本の平均

- Bollinger Bands
  - 日足: 中期的な価格レンジ・過熱感
  - 分足: 短期レンジ、短期的なバンド拡大・収縮、バンドウォーク

- VWAP
  - 分足では短期価格帯の基準として有用
  - 分足モードでは指標計算最終足と同日セッション足のみを対象とし、日次リセットを行う
  - 非分足モード（日足/週足/月足）では指定期間内の出来高加重平均として扱う

- Fibonacci
  - 分足でも計算可能
  - 取得期間全体の高値・安値を基準にする場合は「分足レンジ内の支持線・抵抗線」として扱う

## 市場データ時刻と最新取得価格の扱い（4つの時刻）

xoksa は基本データの先頭で、時刻に関する情報を **4項目** に分けて表示します。
「いつ分析したか」「データはどこまで取れているか」「最新価格はどの足か」「指標はどの足で計算したか」は**それぞれ別物**で、特に時間足・分足の取引時間中はズレます。このズレ自体に意味があります。

画面に出る4項目（カッコ内は内部フィールド名）:

- **📅 分析時刻**（analyzed_at）— この分析を生成した時刻。ほぼ「いま」。
- **🕒 データの最新時刻**（market_data_latest_time）— データソースから取得できた、いちばん新しい市場データの時刻。
- **🕯️ 最新価格が入る足** — データの最新時刻が属する足（時刻を足種の間隔で切り下げたもの）。取引時間中は「**まだ閉じていない＝形成中の足**」になる。
- **📊 指標を計算した足** — RSI / MACD などの計算に実際に使った、**系列の最後の足**。

### なぜ分けるのか

取引時間中、この2つは通常「同じ形成中の足」を指します。データソースの最新の足が、その最新値が属する足そのものだからです。ズレるのは、**取得できた最新値がデータソースの最新の足より新しいとき**です。このとき最新価格は系列にまだ無い足に属し、指標はその1本前の足に留まります。足が長いほど、この差は見えやすくなります。

**引け後・休場**は形成中の足が無いため、4項目が**同じ値**になります（画面で重複して見えるのはこのため。異常ではありません）。

```
  ┌──────────────┬──────────────┐
  │   15:00 足    │   15:30 足    │   ← 引けの足が系列の最後の足
  └──────────────┴──────────────┘
                        ▲
                        └ 📅 / 🕒 / 🕯️ / 📊 すべて = 2026-06-26 15:30
```

### 値の意味

「データの最新時刻」は、**取得できた市場データの最新時刻**という意味で、リアルタイムのQuoteタイムを保証するものではありません。どこまで現在に近いかはデータソースによります。

- **日本株** — 国際版のチャートAPIは東証の気配を約15分遅れで返すため、xoksa は最新価格とその時刻を Yahoo!ファイナンス日本版の公開ページ（リアルタイム）から取ります。取得に失敗した場合はチャートAPIの値を使い、**欠けた分を創作することはありません**。
- **その他の市場** — 最新価格と時刻はチャートAPIの応答から取ります。14:11に分析しても、データソース側の最新の分足が13:30までなら、データの最新時刻と指標を計算した足は13:30になります。

MarketData はローソク足データで、`MarketData.timestamp` は足の時刻、`MarketData.close` はその足の終値です。日本株の**分足**では、リアルタイムの観測値が属する足に反映されます — その足の終値が実勢価格になり、高値・安値はその価格が範囲外のときだけ広がります。**出来高は創作しません**。新しく始まった足は出来高を持たず、表示は直近の実測出来高にフォールバックします。日足・週足・月足の系列は、データソースが返したまま変更しません。

指標は系列の最後の足で計算されるため、日本株の分足では**指標がリアルタイム価格を反映**します。前の足の終値で止まった値ではありません。

将来DBMS連携する場合も、時刻をQuoteタイムと決め打ちせず、`market_data_latest_time` または `source_latest_time` 相当の意味で扱います。

## 基本解析（RSI / MACD）

xoksa における「基本解析」は、
RSI と MACD という2つの代表的なテクニカル指標を組み合わせて、
"買い／売り／様子見"の方向性を定量化することを目的としている。

ここではまず指標そのものの意味を簡潔に説明し、
次に xoksa がそれらを どのように評価・スコア化しているかを解説する。

### RSI（Relative Strength Index）とは

RSI（相対力指数）は、一定期間における値上がり幅と値下がり幅の比率から、
現在の価格が「買われ過ぎ」か「売られ過ぎ」かを示す指標である。

一般的な解釈は以下の通り。

- RSI が高い
→ 短期的に買われ過ぎている可能性

- RSI が低い
→ 短期的に売られ過ぎている可能性

- RSI が中間付近
→ 過熱も売られ過ぎもない中立状態

xoksa では、RSI を
**価格が極端な状態にあるかどうかを判断する"温度計"**として扱う。

### MACD（Moving Average Convergence Divergence）とは

MACD は、短期と長期の移動平均の差から算出される指標で、
価格のトレンド方向や勢いの変化を見るために使われる。

一般的な見方は以下の通り。

- MACD が Signal を上回る
→ 上昇方向の力が優勢

- MACD が Signal を下回る
→ 下落方向の力が優勢

- MACD と Signal の差が大きい
→ 勢いが強い（ただし過熱の可能性もある）

xoksa では MACD を、
「今どちら向きに動こうとしているか」を示す方向指標として扱う。

### xoksa における基本解析の考え方

xoksa の基本解析は、
RSI と MACD を単独で判断材料にするのではなく、組み合わせて評価する。

考え方はシンプルで、

- RSI は「位置（高すぎる／低すぎる）」を見る

- MACD は「向き（上か下か）」を見る

という役割分担になっている。

### スコアリングの前提条件

xoksa では、以下を前提としてスコアを組み立てる。

- RSI が低い
→ 「反発余地があるかもしれない」

- RSI が高い
→ 「過熱しているかもしれない」

- MACD が Signal を上回る
→ 「上向きに動いている」

- MACD が Signal を下回る
→ 「下向きに動いている」

これらの 組み合わせ によって、
買い・売り・中立の方向性を段階的に評価する。

### xoksa における RSI の扱い

- **計算エンジン**: `ta` クレート (RelativeStrengthIndex)
- xoksa では RSI を次のように分類する。

- RSI ≤ buy-rsi
→ 「割安ゾーン」

- RSI ≥ sell-rsi
→ 「過熱ゾーン」

- その間
→ 「中立ゾーン」

> **設定可能な閾値:** `BUY_RSI`（既定 **30**）と `SELL_RSI`（既定 **70**）が上記2つの境界を決める。`BUY_RSI` を下げる／`SELL_RSI` を上げると厳しく（「行き過ぎ」判定が減る）、逆にすると敏感になる。

ここで重要なのは、
RSI 単体では売買を決めないという点である。

RSI はあくまで
「今、極端な位置にいるかどうか」を示す補助情報として使われる。

### xoksa における MACD の扱い

- **計算エンジン**: `ta` クレート (MovingAverageConvergenceDivergence)
MACD は、以下の2点を同時に評価する。

MACD が Signal を上回っているか／下回っているか

MACD と Signal の差（乖離）がどの程度あるか

xoksa では、乖離が小さい場合と大きい場合を分けて扱い、

- 差が小さい
→ 動き始め、または勢いが弱い

- 差が大きい
→ 勢いが強いが、過熱の可能性もある

> **設定可能な閾値**（小／中／大の乖離の境界）: `MACD_DIFF_LOW`（既定 **2**）・`MACD_DIFF_MID`（既定 **10**）・`MACD_DIFF_EXTREME`（既定 **100**）。MACD が本来大きく出る銘柄（高価格株など）では、これらを上げると「強い」帯に入りにくくなる。

と解釈する。

### RSI × MACD によるスコアリングの考え方

xoksa の基本スコアは、
RSI と MACD の状態を組み合わせて決まる。

代表的な例は以下の通り。

- RSI が割安 + MACD が上向き
→ 強い買いシグナル

- RSI が割安 + MACD が下向き
→ 反発期待はあるが慎重（弱い買い）

- RSI が過熱 + MACD が上向き
→ 過熱警戒（売り方向）

- RSI が中立 + MACD が上向き
→ トレンド継続の可能性を評価

- RSI が中立 + MACD が下向き
→ 弱含み、または様子見

このように、
「位置（RSI）」と「向き（MACD）」の組み合わせで判断を分解している。

### MACD マイナス圏の扱い（macd-minus-ok）

xoksa では、MACD がマイナス圏にある場合の扱いを
オプションで制御できる。

- macd-minus-ok が無効
→ MACD がマイナス圏のとき、買い方向の評価を抑制

- macd-minus-ok が有効
→ マイナス圏でも反発狙いとして評価を許容

これにより、

- トレンド重視

- 逆張り重視

といった スタイルの違いを明示的に切り替えられる。

### まとめ
- RSI と MACD の状態を 一貫したルールで整理

- 判断の前提を 数値と条件として明示

- 他の指標やレシピと 組み合わせ可能な形で提供


## EMA（指数平滑移動平均）

xoksa における EMA（Exponential Moving Average）は、
価格のトレンド方向と強さを定量的に把握するための指標として使われる。

RSI や MACD が「過熱度」や「勢い」を見るのに対し、
EMA は トレンドが上向きか下向きか、または存在しないかを判断する役割を担う。

### EMA（指数平滑移動平均）とは

EMA は移動平均の一種で、
直近の価格により大きな重みを与える点が特徴である。

- 直近の値動きに敏感に反応する

- トレンド転換を比較的早く捉えやすい

- ノイズは残るが、遅行性は小さい

この性質から EMA は、
**「今の流れが続いているか／変わりつつあるか」**を把握するために広く使われる。

### xoksa における EMA の計算方法

- **計算エンジン**: `ta` クレート (ExponentialMovingAverage)
xoksa では、EMA の計算に
Rust のテクニカル分析ライブラリ ta クレートを使用している。

このライブラリは、
TA（Technical Analysis）の標準的なトレイト設計に基づいており、
各指標は「逐次データを入力して結果を得る」形で計算される。

xoksa ではこの仕組みをそのまま利用し、

- 終値データを時系列で投入

- EMA を 1回の処理で確定値まで更新

- 再計算や二重計算を行わない

という形で、安全かつ一貫した計算を行っている。

### 短期 EMA と長期 EMA

xoksa では、以下の 2 本の EMA を用いる。

- 短期 EMA（既定: **5本** / `--ema-short-period` / 環境変数 `EMA_SHORT_PERIOD` で変更可能）
→ 直近の価格変動に敏感

- 長期 EMA（既定: **20本** / `--ema-long-period` / 環境変数 `EMA_LONG_PERIOD` で変更可能）
→ 全体のトレンドを反映

> 短期 < 長期 の制約があります。逆転するとエラーになります。

この 2 本を比較することで、短期が長期を上回っているか下回っているか
ほぼ同じ水準かを評価する。

### xoksa における EMA の考え方

xoksa では、
**EMA の絶対値そのものよりも、短期と長期の「差」**を重視する。

#### 短期 EMA − 長期 EMA
→ 現在のトレンドの方向と強さを示す値

この差が、

- 正で大きい
→ 上昇トレンドが明確

- 負で大きい
→ 下降トレンドが明確

- ほぼゼロ
→ トレンドがない（レンジ）

と解釈される。

### EMA スコアリングの設計

xoksa では、
短期 EMA と長期 EMA の差を 5段階スコアに変換する。

> [!NOTE]
> **スコアは乖離率で採るので、どの銘柄でも同じ意味になる。** 境界は
> `(短期EMA − 長期EMA) ÷ 終値 × 100` に対する **±2.0% と ±0.5%**——スコアの1行上に
> 表示している率そのもの——なので、数千円の銘柄と数十ドルの銘柄を同じ土俵で判定する。
> この変更前は境界が価格の絶対差（銘柄自身の通貨建てで 2.0 / 0.5）だったため、日本株は
> 実際の開きにかかわらずほぼすべて ±2 になり、低価格の米国株は 0 のままだった。
> 実測値は一目均衡表の節を参照。

#### 乖離率(%) = (短期EMA − 長期EMA) ÷ 終値 × 100

|条件 (乖離率)	|解釈	|スコア|
|---	|---	|---|
|+2.0% 以上	|強い上昇トレンド	|+2|
|+0.5% 以上 +2.0% 未満	|上昇トレンド	|+1|
|-0.5% 超 +0.5% 未満	|トレンドなし	|0|
|-2.0% 超 -0.5% 以下	|下降トレンド	|-1|
|-2.0% 以下	|強い下降トレンド	|-2|

ここで重要なのは、
EMA 単体で売買判断を完結させないという点である。

EMA はあくまで
「今、価格がどちら向きに流れているか」を示す材料であり、
RSI・MACD・他の指標と組み合わせて使われる。

### xoksa における EMA の位置づけ

EMA は xoksa の中で、

- 「逆張り」を直接判断する指標ではない

- 「勢い」を測る指標でもない

トレンドの有無と向きを示す基準軸として扱われる。

そのため、逆張りレシピでは

- 逆張りレシピでは
→ EMA スコアを低く（または重みを下げる）

- トレンドフォロー・ブレイク狙いでは
→ EMA スコアを重視する

といった 使い分けが前提となっている。

### xoksa が EMA を採用する理由

xoksa が EMA を基本指標として採用している理由は、広く使われている標準的な指標である計算ロジックが明確で再現性が高いTA ライブラリにより安全に計算できる

EMA は「判断を出す指標」ではなく、判断の前提となる"流れ"を示す指標である。

### まとめ：EMA が示すもの

xoksa における EMA は、

- 上か下か

- 強いか弱いか

- そもそも流れがあるか

を 数値とスコアで整理するための道具である。



## SMA（単純移動平均）

xoksa における SMA（Simple Moving Average）は、
価格の平均的な水準と、トレンドの安定性を確認するための指標として扱われる。

EMA が「直近の変化に敏感なトレンド指標」なのに対し、
SMA は ノイズをならし、より"素直な流れ"を確認するための基準線として位置づけられている。

### SMA（単純移動平均）とは

SMA は、一定期間の終値を単純平均したものである。

- すべての価格に同じ重みを与える

- 計算方法が直感的で分かりやすい

- 短期的なブレに引きずられにくい

そのため SMA は、

- トレンドの方向を大づかみに確認したいとき

- 価格が「平均的に見て高いか低いか」を把握したいとき

に使われることが多い。

### xoksa における SMA の計算方法

- **計算エンジン**: `ta` クレート (SimpleMovingAverage)
xoksa では、SMA の計算にも
EMA と同様に ta クレートを使用している。

このライブラリは、
TA（Technical Analysis）の標準トレイトに基づいた逐次計算モデルを採用しており、

終値データを時系列で投入

最新時点の SMA を 1 回の処理で確定

不要な再計算や二重処理を行わない

という形で、安定した計算を行っている。

### 短期 SMA と長期 SMA

xoksa では、以下の 2 本の SMA を使用する。

- 短期 SMA（既定: **5本** / `--sma-short-period` / 環境変数 `SMA_SHORT_PERIOD` で変更可能）
→ 直近の平均的な価格水準

- 長期 SMA（既定: **20本** / `--sma-long-period` / 環境変数 `SMA_LONG_PERIOD` で変更可能）
→ 中期的な平均価格水準

> 短期 < 長期 の制約があります。逆転するとエラーになります。

この 2 本の関係を見ることで、

- 価格が平均的に上向いているか

- 下向いているか

- 方向感がないか

を判断する。

### xoksa における SMA の考え方

xoksa では、
SMA の絶対値そのものではなく、短期 SMA と長期 SMA の差を評価対象とする。

#### 短期 SMA − 長期 SMA
→ 平均価格のズレ＝トレンドの傾き

この差が、

- 正で大きい
→ 上昇方向に安定した流れ

- 負で大きい
→ 下降方向に安定した流れ

- ほぼゼロ
→ 平均価格が収束＝方向感なし

と解釈される。

### SMA スコアリングの設計

xoksa では、
短期 SMA と長期 SMA の差を 5段階スコアに変換する。

> [!NOTE]
> **スコアは乖離率で採るので、どの銘柄でも同じ意味になる。** 境界は
> `(短期SMA − 長期SMA) ÷ 終値 × 100` に対する **±3.0% と ±1.0%**——スコアの1行上に
> 表示している率そのもの。EMA の対より広いのは意図的である。期間はどちらも 5/20 だが、
> 単純平均のほうがトレンド中に遅れるため、同じ相場でも SMA の開きは大きく出る
> （実測の中央値は 1.60%、EMA は 1.03%）。この変更前は境界が EMA・一目と共有の
> 価格の絶対差だった。実測値は一目均衡表の節を参照。

#### 乖離率(%) = (短期SMA − 長期SMA) ÷ 終値 × 100

|条件 (乖離率)	|解釈	|スコア|
|---	|---	|---|
|+3.0% 以上	|強いゴールデンクロス	|+2|
|+1.0% 以上 +3.0% 未満	|緩やかな上昇	|+1|
|-1.0% 超 +1.0% 未満	|トレンドなし	|0|
|-3.0% 超 -1.0% 以下	|緩やかな下降	|-1|
|-3.0% 以下	|強いデッドクロス	|-2|

このスコアは、

- トレンドが「あるか・ないか」

- その流れが「どの程度安定しているか」

を示すためのものであり、
売買のタイミングを直接示すものではない。

### EMA と SMA の役割の違い

xoksa では EMA と SMA を 意図的に併用している。

- EMA
→ 直近の動きに敏感（変化の兆しを捉えやすい）

- SMA
→ 平均的な流れを反映（ノイズに強い）

そのため、

- EMA と SMA が同じ方向
→ トレンドが比較的素直

- EMA と SMA が食い違う
→ 転換期、または不安定な相場

といった 状態の違いを把握できる。

### xoksa における SMA の位置づけ

SMA は xoksa の中で、

- 逆張りの直接的な判断材料ではない

- ブレイクの初動を捉える指標でもない

- トレンドの「安定度」を測る補助軸

として使われる。

そのため、

- トレンドフォローでは
→ EMA と合わせて重視される

- 逆張りでは
→ 重みを下げる、または参考情報として扱う

といった 使い分けが前提となる。

### まとめ：SMA が示すもの

xoksa における SMA は、

- 価格が平均的にどちらへ傾いているか

- その流れが安定しているかどうか

を シンプルな形で示す指標である。

EMA が「変化の速さ」を示すなら、SMA は「流れの落ち着き」を示す。

xoksa は、この 2 つを並べて提示することで、
ユーザ自身が「今は追う相場か、待つ相場か」を判断できる余地を残している。

## ADX（Average Directional Index）

xoksa における ADX（平均方向性指数）は、
トレンドの"方向"ではなく、"強さ"を測るための指標として扱われる。

EMA / SMA が「上向きか下向きか」を示すのに対し、
ADX は **"そもそもトレンド相場なのか、レンジ相場なのか"**を判断する材料になる。

### ADX（平均方向性指数）とは

ADX は、Directional Movement（+DM / -DM）と True Range（TR）を用いて計算される指標で、
相場にトレンドが存在するか、どの程度強いかを数値化する。

一般的な理解としては以下のような位置づけになる。

- ADX が高い
→ トレンドが強い（上昇・下落のどちらかに"走りやすい"）

- ADX が低い
→ トレンドが弱い（レンジになりやすい／ブレやすい）

ここで重要なのは、ADX は 上昇か下落かを示さない点である。
ADX はあくまで 強さのメーターであり、方向は別の指標（移動平均やDI等）で補う。

### xoksa における ADX の計算方法

- **計算エンジン**: **オリジナル計算**（算術式実装・Wilder平滑化）
xoksa では ADX を 自前で演算している。
理由は、使用している ta クレートに ADX が標準実装として用意されていないためである。

xoksa の ADX は、以下の要素を使って **N期間**（既定: **14** / `--adx-period` / 環境変数 `ADX_PERIOD` で変更可能）の Wilder 平滑化移動平均（RMA）で評価する。

- True Range（TR）
→ 価格変動の実質的な振れ幅

- +DM / -DM
→ 上方向／下方向の"優勢さ"

- +DI / -DI
→ ATR で正規化した方向性の強さ

- DX
→ +DI と -DI の差分から算出される方向性の強さ

実装上は、Wilder（1978年）の平滑化方式に準拠している。

1. **ATR / +DM14 / -DM14 の初期値**：最初の N 本の TR / +DM / -DM の合計で初期化
2. **RMA（Wilder平滑化移動平均）による逐次更新**：
   `ATR_new = ATR_old − ATR_old / N + TR_new`（+DM / -DM も同様）
3. **DX の算出**：各ステップで +DI / -DI → DX を計算
4. **ADX の初期値**：最初の N 本の DX の単純平均
5. **ADX の逐次更新**：`ADX_new = (ADX_old × (N−1) + DX_new) / N`

最低データ本数は **2×N**（既定: 28本）。

### xoksa における ADX の位置づけ

xoksa での ADX は、
「その戦略が今の相場に合っているか」を見極めるためのゲート役として機能する。

例として、

- ブレイク狙い／順張り
→ ADX が高い（トレンドがある）ほど相性が良い

- 逆張り／レンジ回帰
→ ADX が低い（トレンドがない）ほど相性が良い

のように、レシピ（戦略）によって評価の意味が変わる指標である。

### ADX スコアリングの設計

xoksa では、ADX を 5段階にスコア化する。

|ADX の水準	|解釈	|スコア|
|---	|---	|---|
|50 以上	|非常に強いトレンド	|+2|
|30 以上	|強いトレンド	|+1|
|20 以上	|トレンド成立（中立）	|0|
|10 以上	|トレンド弱い（レンジ寄り）	|-1|
|10 未満	|トレンド不在（レンジ色が濃い）	|-2|

※このスコアは「売買方向」を決めるものではなく、
その相場が"走りやすい状態かどうか"を数値化することを目的としている。

### まとめ：ADX が示すもの

xoksa における ADX は、

今の相場にトレンドがあるか

そのトレンドがどれくらい強いか

を 数値とスコアで整理する指標である。

「勢い（ROCなど）」と「トレンドの強さ（ADX）」は別物であり、
xoksa では別々のノブとして扱われる。

ADX は、レシピに応じて

- "追うべき相場か"

- "待つべき相場か"

を判断するための、重要な前提情報となる。


## ROC（Rate of Change / 変化率）

xoksa における ROC（変化率）は、
一定期間で価格がどれだけ変化したか（上がったか／下がったか）をパーセントで示す指標として扱われる。

EMA / SMA が「平均線の位置関係」からトレンドを見たり、
ADX が「トレンドの強さ」を見たりするのに対し、
ROC は "どれくらい動いたか"という勢い（モメンタム）を直球で測る役割を担う。

### ROC（変化率）とは

ROC は、ある時点の価格と一定期間前の価格を比較し、
その差分をパーセントで表したもの。

- ROC がプラス
→ 期間内で上昇している

- ROC がマイナス
→ 期間内で下落している

- ROC が 0 付近
→ 期間内で大きな変化がない

指標としてはシンプルだが、
"どれだけ動いたか"が明確に数値として出るため、ブレイク狙いや順張り系の判断材料として使われやすい。

### xoksa における ROC の計算方法

- **計算エンジン**: **オリジナル計算**（前の分析足との差分に基づく算術式）
xoksa の ROC は、直近の終値と **N本前の終値**（既定: **10** / `--roc-period` / 環境変数 `ROC_PERIOD` で変更可能）を使って計算する。

#### roc = ((最新終値 - N本前終値) / N本前終値) * 100

実装では N = 10 を想定し、

最新の終値（latest_close）

N本前相当の終値（data[len - (N+1)]）

を比較して ROC を算出している。

このため、ROC の計算には最低 N+1 本以上の分析足データが必要となる。

### xoksa における ROC の位置づけ

xoksa での ROC は、
「今の値動きに勢いがあるか」を確認するための材料として使われる。

ただし、ROC は "方向"を含む勢いであり、

- ROC が強い
→ よく動いている（上にも下にも）

- ROC が弱い
→ 動いていない（レンジ寄り）

という意味になる。

そのため xoksa では、ROC を単独で解釈せず、
ADX（トレンドの強さ）などと組み合わせて使うことを前提としている。

### ROC スコアリングの設計

xoksa では、ROC を 5段階にスコア化する。
狙いは「勢いがあるか／ないか」を分かりやすく分類すること。

|条件 (ROC)	|解釈	|スコア|
|---	|---	|---|
|+10% 以上	|非常に強い上昇勢い	|+2|
|+3% 以上 +10% 未満	|緩やかな上昇勢い	|+1|
|-3% 超 +3% 未満	|同値圏（変化が小さい）	|0|
|-10% 超 -3% 以下	|緩やかな下降勢い	|-1|
|-10% 以下	|非常に強い下降勢い	|-2|

ここでのスコアは、
「買い／売り」を決め打ちするものではなく、
値動きの勢い（変化率）の大きさを要約するためのもの。

### まとめ：ROC が示すもの

xoksa における ROC は、

一定期間でどれだけ上がったか／下がったか

値動きに勢いがあるかどうか

をシンプルに数値化する指標である。

ROC は勢いを示す一方で、
"その勢いがトレンドとして続くか"は別問題である。

xoksa では、

- ROC（勢い）

- ADX（トレンドの強さ）

を別々のノブとして扱い、
レシピ（戦略）に応じて重み付けを変えられる設計になっている。

## ストキャスティクス（Stochastics）

xoksa におけるストキャスティクスは、
一定期間の価格レンジ（高値〜安値）の中で、終値がどの位置にいるかを数値化し、
短期的な「行き過ぎ（過熱／売られ過ぎ）」を把握するための指標として扱う。

RSI が"上げ下げの強弱比"から過熱感を見るのに対し、
ストキャスは **「レンジ内の位置」**を使って過熱感を見るタイプのオシレーターである。

### ストキャスティクスとは

ストキャスティクスは一般に、次の2本で語られる。

- %K：終値の位置（メイン指標）

- %D：%K の平滑化（シグナル）

- %K の基本式は以下。

### %K = (Close - LowestLow) / (HighestHigh - LowestLow) × 100

ここで、

- HighestHigh：期間内の最高値

- LowestLow：期間内の最安値

- Close：当日の終値

であり、%K は 0〜100 の範囲を取る。

一般的には、

- 高い（例：80以上）→ 高値圏（買われ過ぎと見なされやすい）

- 低い（例：20以下）→ 安値圏（売られ過ぎと見なされやすい）

という"温度計"的な使い方をする。

### xoksa における計算方法

- **計算エンジン**: **オリジナル計算**（期間内高値安値抽出による算術式）
xoksa のストキャスは、**%K（既定: 14期間 / `--stochastics-period` / 環境変数 `STOCHASTICS_PERIOD` で変更可能）と %D（3期間平均）**を計算する。

1) 必要データ量

最低N本以上のデータが必要
（N期間の高値・安値レンジを作るため）

2) N期間の HighestHigh / LowestLow を作る

xoksa は各足データ全体に対して、

各足の「直近N本（足りない部分は先頭から）」を取り

その範囲の最高値（high）と最安値（low）を計算して保持

する。

3) 最新日の %K を計算

最新日の high / low / close を用いて %K を計算する。

#### high == low（レンジ幅0）になった場合は
ゼロ割り回避のため %K を 0.0 として扱う。

4) %D を計算（直近3本の %K の平均）

xoksa の %D は、直近3本分の %K を平均したもの。

#### %D = 直近3本の %K の単純平均

### xoksa におけるスコアリング（5段階）

xoksa では、ストキャスのスコアは %K を基準に判定する。
（%D は計算・保持するが、スコア判定自体は %K に基づく）

スコアは以下の 5 段階。

|%Kの水準	|解釈（参考）	|スコア|
|---	|---	|---|
|90以上	|強い過熱圏（高値圏）	|-2|
|80以上	|過熱圏（高値圏）	|-1|
|20以下	|売られ気味（安値圏）	|+1|
|10以下	|強い売られ過ぎ（安値圏）	|+2|
|それ以外	|中立	|0|

このスコアは、
「トレンド方向」を出すためではなく、
短期的な行き過ぎを数値として要約するためのものとして設計されている。

### 値の見方（参考としての一例）

ストキャスは 0〜100 のスケールなので、
ユーザが閾値を意識しやすい。

- 閾値を 上げる（例：80→85）
→ より極端な高値圏だけを過熱とみなす（シグナルは減る）

- 閾値を 下げる（例：80→70）
→ 広い範囲を高値圏とみなす（シグナルは増える）

低値側も同様に、

- 閾値を 下げる（例：20→15）
→ かなり極端な安値圏だけを拾う（シグナルは減る）

- 閾値を 上げる（例：20→30）
→ 安値圏とみなす範囲を広げる（シグナルは増える）

xoksa はこの種の「ノブ」をレシピ側で活用できるよう、
%K / %D とスコアを構造体に保持している。

### まとめ：ストキャスが示すもの

xoksa におけるストキャスティクスは、

N期間レンジ内での終値位置（%K）

その平滑化（%D）

%Kを元にした過熱・売られ過ぎのスコア

を提供し、
短期の行き過ぎを定量化する補助指標として機能する。

トレンド系（EMA/SMA）や勢い系（ROC）と組み合わせることで、
「走っているのか」「行き過ぎているのか」を別々の軸で判断できる。


## ボリンジャーバンド（Bollinger Bands）

ボリンジャーバンドは、一定期間の価格分布（平均とばらつき） をもとに、指標計算最終足終値が「高い位置／低い位置」にいるか、また 値動きが圧縮されているか（収縮）／広がっているか（拡散） を把握する指標。

xoksaでは以下を算出する。

- 上限バンド（Upper）

- 下限バンド（Lower）

- %B（バンド内の位置）

- Bandwidth（バンド幅＝圧縮／拡散の度合い）

### xoksaでの計算方法

- **計算エンジン**: `ta` クレート (BollingerBands)
- **期間：20本**（既定: `--bollinger-period` / 環境変数 `BOLLINGER_PERIOD` で変更可能）

- **標準偏差：2.0**（既定: `--bollinger-stddev-multiplier` / 環境変数 `BOLLINGER_STDDEV_MULTIPLIER` で変更可能）

- 中心線（Mid）：(Upper + Lower) / 2（2σ対称のため）

- 派生値

- %B

- %B = (Close - Lower) / (Upper - Lower)


- → 0.0 に近いほど下限寄り、1.0 に近いほど上限寄り

- Bandwidth（%）

- Bandwidth(%) = (Upper - Lower) / Mid × 100


- → 値が小さいほど価格変動が圧縮されている

### スコアリング（価格位置）

xoksaでは 指標計算最終足終値がバンドの外側に出ているか を5段階で評価する。

- 上限の 2% 上以上 → -2

- 上限以上、上限の 2% 上未満 → -1

- バンドの内側（上限・下限そのものは外側として扱う） → 0

- 下限以下、下限の 2% 下より上 → +1

- 下限の 2% 下以下 → +2

幅が**ゼロ**のバンドは、価格がどこにあってもスコア `0` とする。ボラティリティが無い場合——期間中ずっと終値が動いていない場合——上下の端が一致するので、「バンドの外」という状態が存在せず、位置は何の情報も持たない。

※ここで評価しているのは「位置」であり、方向の断定はしない。

### スクイーズ（Squeeze）とは何か

スクイーズとは、
Bandwidth が小さくなり、値動きが圧縮されている状態を指す。

これは「上がる／下がる」を示すものではなく、

値幅が出ていない

方向感が固まっていない

という 状態を把握するための概念。

xoksaではスクイーズを 売買シグナルとしては扱わない。

### スクイーズ発生時の捉え方（参考）

スクイーズが発生しているときは、
「大きく動いていない」ことが Bandwidth で説明できる局面に入っている。

そのため、スクイーズは

「今は走っていない」
「なぜ判断がつきにくいのか」

を把握する材料として使える。

### スクイーズ判定のオプション
#### --bb-bandwidth-squeeze-pct \<pct\>

Bandwidth がこの値以下なら、xoksaは スクイーズ状態として扱う。

- 値を 小さくする
→ より強い圧縮だけをスクイーズとみなす（厳選寄り）

- 値を 大きくする
→ 緩い圧縮もスクイーズに含める（広めに拾う）

このパラメータは、
**「どの程度の圧縮を"スクイーズ"と呼ぶか」**を調整するノブ。

### まとめ

ボリンジャーバンドは xoksa において、

価格の 位置（Upper/Lower と %B）

値動きの 状態（Bandwidth とスクイーズ）

を数値で把握するための指標。

スクイーズは売買方向を決めるものではなく、
相場が圧縮状態にあるかどうかを把握するための情報として扱われる。


## フィボナッチ・リトレースメント（Fibonacci Retracement）

xoksa におけるフィボナッチは、
**価格が「直近の値幅のどの位置にいるか」**を
一度だけ決めたルールで数値化するための指標。

トレンドを予測するものではなく、

今が「戻りの途中」なのか

押しが深いのか浅いのか

中央（50%）付近で迷っているのか

を 位置情報として整理する役割を持つ。

### フィボナッチとは

フィボナッチ・リトレースメントは、

- 直近の高値（High）

- 直近の安値（Low）

の差（値幅）に対して、
一定の比率で「戻り／押し」の目安を引く手法。

一般に使われる代表的な水準は、

- 38.2%

- 50.0%

- 61.8%

xoksa もこの3点に絞って扱う。

### xoksa における計算方法

- **計算エンジン**: **オリジナル計算**（黄金比に基づく算術式）

1) 高値・安値の確定

入力された期間の MarketData から、

- 全期間の 最高値（high）

- 全期間の 最安値（low）

を取得する。

#### span = high - low

- ※ span ≤ 0 の場合（変動なし）は
→ 判定不能として 中立（0） にする。

2) フィボナッチ水準の算出

xoksa では 一度だけ、ここで確定する。

#### 38.2% = high - span × 0.382
#### 50.0% = high - span × 0.500
#### 61.8% = high - span × 0.618

この水準は 表示用・判定用ともに SoT（Single Source of Truth）。

### xoksa におけるスコアリング思想（重要）

xoksa のフィボナッチは、
「どの帯にいるか」を明確に分ける設計になっている。

### ポイント

- 0点は 50%近傍だけ

- 上か下かは必ずどちらかに倒す

- 境界のブレを避けるため、±eps を設ける。これはブレ止めであり、入ることを期待する帯ではない。終値は通常 50% 水準から離れており、20 銘柄の実測では 19 銘柄が値幅の 6% 超離れていた。

### スコア判定ルール（唯一の真実）

終値（close）と各水準の位置関係で、
ここで一度だけスコアを確定する。

|条件 (終値と水準の関係)	|解釈（参考）	|スコア|
|---	|---	|---|
|終値が 38.2% 水準以上	|押しが浅い（強い）	|+2|
|50% 水準より上、かつ eps 以上上、かつ 38.2% 水準未満	|戻り途中の強含み	|+1|
|50% 水準そのもの、または 50% 水準との差が eps 未満	|50% 水準上（境界のブレ止め。めったに出ない）	|0|
|50% 水準より下、かつ eps 以上下、かつ 61.8% 水準超	|下落寄りの弱含み	|-1|
|終値が 61.8% 水準以下	|押しが深い（弱い）	|-2|

- **eps（中立帯）**: **50% 水準から 38.2% 水準までの距離の 0.05**（既定: `--fibonacci-neutral-ratio` / 環境変数 `FIBONACCI_NEUTRAL_RATIO` で変更可能。受け付ける範囲は **0.0〜0.5**）。価格ではなく **`±1` の帯が持つ幅に対する割合**なので、どの銘柄でも同じ意味になり、値幅に合わせて伸縮する（4,000 円の銘柄では 120 ポイントの幅の 5%、13 ドルの銘柄では 0.40 の幅の 5%）。上限 0.5 が `±1` の到達可能性を保証している。
  `0` にすると帯が無くなる。その場合は 50% 水準にちょうど乗った終値だけが中立で、少しでも離れればどちらかに傾く。50% 水準そのものが上へ傾くことはない。
  2.9.8 まではこれが価格の絶対差（既定 0.50）で、銘柄ごとに別の意味になっていた。日足での実測では 9984.T では幅の 0.2% にすぎず `0` が実質到達不能で、**F では幅そのものを超えており、`+1` の条件が「終値が 15.080 超かつ 14.984 未満」という空区間だったため `+1`・`−1` が一切出なかった。**
    - 50.0% 水準の周辺 ±eps（既定は 50%→38.2% 距離の 5%）の範囲を「中立（0）」として扱う。
- ※ **閾値ちょうどは、その閾値を満たしたものとする**——xoksa のすべてのスコアが従う唯一の境界規約。したがって終値が 38.2%／61.8% 水準にちょうど乗った場合は `±2` になり、50% 水準まわりの中立帯は両端が開く。

### 値の見方（参考）

フィボナッチは 数値を動かす指標ではない。
見るべきは「帯のどこにいるか」。

- 38.2%より上
→ 押しが浅く、強い値動きの途中と解釈されやすい

- 50%付近
→ 買い・売りが拮抗しやすい

- 61.8%より下
→ 深い押し／戻りで、慎重な判断が必要

xoksa では
「今はどの帯か」だけを機械的に切り出す。

意味づけや売買判断は、
他の指標（RSI / EMA / ADX など）と
ユーザ側で組み合わせる前提になっている。

### まとめ：フィボナッチの役割

xoksa におけるフィボナッチは、

高値・安値から水準を一意に確定

38.2 / 50 / 61.8 のどこにいるかを判定

位置情報を -2〜+2 のスコアに落とす

ための指標。

トレンドの強さや勢いを測るものではなく、
「価格の立ち位置」を固定的に示す補助軸として設計されている。

そのため、

逆張りでは「深さの確認」

ブレイク狙いでは「まだ戻りの途中かどうか」

を見極める材料として使われる想定になっている。


## VWAP（Volume Weighted Average Price）

xoksa における VWAP は、
「直近の平均的な売買価格帯に対して、指標計算最終足終値がどこにいるか」
を現在の分析足ベースで把握するための指標として扱う。

現在の実装では、市場データの OHLCV を使い、Typical Price を出来高で加重して算出する。
分足モードでは指標計算最終足と同日セッション足全体（日次リセット）、日足モードでは直近N本（既定14本）を参照する。

日足または分足の取得データだけで完結

他のトレンド系指標（EMA / SMA）と並べて比較可能

という実用優先の設計になっている。

### VWAPとは（一般論・最小限）

本来の VWAP（Volume Weighted Average Price）は、

- 価格 × 出来高 を積算

- その日の平均約定価格を算出

することで、
「市場参加者が平均的にどの価格で取引したか」を示す指標。
機関投資家の執行基準として使われることが多い。

### xoksa における前提（重要）

xoksa は以下の制約を前提にしている。

- 分足モード: 指標計算最終足と同日セッション全足を対象とするセッションVWAP（日次リセット）
- 日足モード: 直近N本（既定14本 / `--vwap-period`）を対象とするローリングVWAP

- 約定単位の本格的な intraday VWAP ではない

- 出来高データが取得できない銘柄では VWAP 評価を行えない

- 他指標との一貫性を優先

そのため xoksa の VWAP は、
「当日中の執行基準」ではなく、
「出来高加重された価格帯と指標計算最終足終値の乖離」として読む。

### xoksa における計算方法

- **計算エンジン**: **オリジナル計算**（出来高加重 Typical Price × Volume）

1) Typical Price の算出

各足の価格から以下を計算する。

#### Typical Price = (High + Low + Close) / 3

これは
「その足の代表的な価格帯」を表す近似値。

2) 期間設定

- 分足モード: 指標計算最終足と同日セッション足全体（`--vwap-period` は無視される）
- 日足モード: **直近N本**（既定14本 / `--vwap-period` / 環境変数 `VWAP_PERIOD` で変更可能）

- 理由：

短期すぎず

- EMA / SMA と並べて見やすい

3) VWAP（出来高加重）の算出

直近 N 本について、以下を計算する。

#### VWAP = Σ(Typical Price × Volume) / Σ(Volume)

出来高が大きい足の価格ほど、VWAP に強く反映される。

### xoksa におけるスコアリング

VWAP は
「今の終値が、平均的な取引価格帯より上か下か」
だけを評価する。

- 判定軸
#### 乖離率(%) = (終値 − VWAP) ÷ VWAP × 100

### スコア判定（5段階）

|条件 (乖離率)	|解釈（参考）	|スコア|
|---	|---	|---|
|+3.0% 以上	|価格が平均より明確に上	|+2|
|+1.0% 以上 +3.0% 未満	|価格が平均よりやや上	|+1|
|-1.0% 超 +1.0% 未満	|ほぼ同水準（中立）	|0|
|-3.0% 超 -1.0% 以下	|価格が平均よりやや下	|-1|
|-3.0% 以下	|価格が平均より明確に下	|-2|

- ※ 判定は**乖離率**——VWAP からの距離を VWAP に対する百分率で見た値——で行う。画面に表示している値と同じものなので、表示とスコアが食い違うことはない
- ※ 数値は「勢い」ではなく 位置関係の目安

> [!NOTE]
> **境界は百分率なので、スコアの意味はどの銘柄でも同じである**——数千円の銘柄と数十ドルの
> 銘柄を同じ土俵で採点し、スコアは乖離が大きくなったときにだけ上がる。この変更前は境界が
> 価格の絶対差（銘柄自身の通貨建てで ±4.0 / ±1.0）だったため、同じスコアが銘柄ごとに別の
> 距離を意味していた。日足での実測では、VWAP より 5.03% 下の終値が 0、2.19% 下の終値が −2 と
> なっていた。これを乖離率に置き換えた。
>
> **分足ではスコアが 0 に留まることが格段に多いが、これは不具合ではなく読み取り結果である。**
> VWAP はセッションの寄り付きで再スタートするため、終値はそのセッション自身の平均から
> 1% 未満の範囲に収まるのが通常である（60 分足での実測は −0.87% と −0.12%）。分足で ±2 に
> 達するのは、価格がそのセッションの出来高が付いた水準から実際に離れたときである。

### 値の見方（参考）

VWAP は
トレンドを作る指標ではない。

- 終値 ＞ VWAP
→ 市場平均より高い位置

- 終値 ＜ VWAP
→ 市場平均より低い位置

といった 相対的位置を見るためのもの。

xoksa では、

- EMA / SMA：トレンド方向

- ROC / ADX：勢い・走りやすさ

- VWAP：平均価格帯とのズレ

という役割分担で使われる。

### まとめ：VWAPの役割

xoksa における VWAP は、

Typical Price を出来高で加重（分足: セッションVWAP、日足: 直近N本ローリングVWAP）

出来高が大きい価格帯を重く見る設計

終値との距離で -2〜+2 にスコア化

するための指標。

分足ベースの厳密な執行指標ではなく、
**「指標計算最終足終値は平均より高いか／低いか」**を
他の指標と並べて判断するための
位置基準のひとつとして設計されている。


## 一目均衡表（Ichimoku）

xoksa における一目均衡表は、
転換線（既定: **9期間** / `--ichimoku-tenkan-period` / 環境変数 `ICHIMOKU_TENKAN_PERIOD` で変更可能）と基準線（既定: **26期間** / `--ichimoku-kijun-period` / 環境変数 `ICHIMOKU_KIJUN_PERIOD` で変更可能）の位置関係だけを使って
相場の「短期の勢い」と「中期の基準」を比較するための指標として扱う。

> 転換期間 < 基準期間 の制約があります。逆転するとエラーになります。

一目均衡表は本来、先行スパンに挟まれた帯（チャート上で「雲」と呼ばれる塗り潰し部分）や遅行スパンまで含む総合指標だが、
xoksa では "線のクロス"に絞ってスコア化し、他指標と同列に扱えるようにしている。

### 一目均衡表とは（最小限）

一目均衡表には複数の要素があるが、xoksa が使うのは以下。

- 転換線（Tenkan-sen）：短期の基準（既定: **9**期間 / `--ichimoku-tenkan-period` で設定可能）

- 基準線（Kijun-sen）：中期の基準（既定: **26**期間 / `--ichimoku-kijun-period` で設定可能）

いずれも「期間内の高値と安値の平均」で定義される。

### xoksa における計算方法

- **計算エンジン**: **オリジナル計算**（期間内高値安値の平均）

1) 必要データ量

最低基準線分の期間のデータが必要
（基準線の計算に kijun 期間を使うため）

2) 転換線（tenkan 期間）

直近 tenkan 期間の

- 最高値（high）

- 最安値（low）

を取り、

#### 転換線 = (tenkan期間の最高値 + tenkan期間の最安値) / 2

3) 基準線（kijun 期間）

直近 kijun 期間の

- 最高値（high）

- 最安値（low）

を取り、

#### 基準線 = (kijun期間の最高値 + kijun期間の最安値) / 2

### xoksa におけるスコアリング（5段階）

xoksa の一目は、
転換線と基準線の**乖離率**の符号と大きさで判定する。

#### 乖離率(%) = (転換線 − 基準線) ÷ 基準線 × 100

|条件 (乖離率)	|解釈（参考）	|スコア|
|---	|---	|---|
|+4.0% 以上	|強い上方向（ゴールデンクロス）	|+2|
|+1.0% 以上 +4.0% 未満	|やや上方向	|+1|
|-1.0% 超 +1.0% 未満	|同値圏（拮抗）	|0|
|-4.0% 超 -1.0% 以下	|やや下方向	|-1|
|-4.0% 以下	|強い下方向（デッドクロス）	|-2|

> [!NOTE]
> **境界は百分率なので、スコアの意味はどの銘柄でも同じであり、開きが大きいほうが小さく
> 評点されることもない。** この変更前は境界が価格の絶対差——銘柄自身の通貨建てで
> 2.0 / 0.5——だったため、スコアは「読み取るはずの開き」ではなく株価水準の関数になって
> いた。20 銘柄の日足での実測: **1605.T は基準線から 0.16% の位置で −2、F は 1.89% で 0**。
> 標本の日本株は実際の開きにかかわらずすべて ±2 に達し、6,000 円の銘柄での 2.0 は
> 0.03% である一方、30 ドルの銘柄で同じ ±2 を得るには 6.7% を要した。
>
> ±4.0% / ±1.0% の対が EMA（±2.0% / ±0.5%）や SMA（±3.0% / ±1.0%）より広いのは、
> 9/26 の期間がそれらの 5/20 より離れており、同じ相場でも乖離が大きく出るためである
> （実測の中央値は一目 2.47%、SMA 1.60%、EMA 1.03%）。
>
> これは下に述べる乖離率と同じ値である。表示は符号なしで出すが、スコアは符号も使う。

#### 補足：乖離率（gap_ratio）による詳細表示
スコアが読んでいるのと同じ乖離率を符号なしでも表示し、
表示時に以下の補助情報を添える。
- **乖離 1.0% 未満**: 「接近状態でありトレンド確定には弱い」
- **乖離 5.0% 超**: 「大幅な乖離（乖離しすぎ）の可能性」

1.0% はスコアの `±1` の境界そのものなので、「接近状態」とスコア `0` は同じ読み取りを両側から
述べている。これにより、単なるクロスの有無だけでなく「勢いの過熱度」も一目で把握できる。

### 値の見方（参考）

xoksa での一目は "線のクロス"を見る用途。

- 転換線 ＞ 基準線
→ 短期が中期より強い（上方向の示唆）

- 転換線 ＜ 基準線
→ 短期が中期より弱い（下方向の示唆）

- 近い（±0.5以内）
→ 方向感が弱い／拮抗

差分が大きいほど（±2.0超）
xoksa ではスコアが強くなる設計。

### まとめ：一目（xoksa版）の役割

xoksa における一目均衡表は、

転換線（tenkan）と基準線（kijun）を計算し

その差分だけで -2〜+2 にスコア化
_
することで、
一目を「総合指標」ではなく 短期 vs 中期の強弱比較として取り込み、
EMA/SMA などのトレンド系指標と同じレイヤで合成できるようにしている。

---

## 出来高（Volume）

### 出来高とは

出来高は1本のバー期間中に売買が成立した**株数の合計**です。約定件数（注文の成立回数）でも、売買代金（円・ドル）でもありません。

### xoksa が算出する3つの値

| 値 | 計算方法 |
| :--- | :--- |
| **最新出来高** | `volume > 0` の最新バーの株数。Yahoo Finance はイントラデイデータの末尾にvolume = 0のプレースホルダーを返すことがあるため、xoksa はそれをスキップし直前の実取引済みバーを採用する。 |
| **平均出来高** | `volume > 0` の直近Nバーの単純平均。NはEMA/SMAの長期期間（`sma_long_period`、デフォルト20本）を共用。volume = 0のバーは平均計算から除外。 |
| **倍率（ratio）** | `最新出来高 ÷ 平均出来高`。1.0超 = 平均より多い参加、1.0未満 = 平均より少ない参加。 |

### 閾値とコメントロジック

| 条件 | 表示コメント |
| :--- | :--- |
| `ratio == 0.0` | 出来高ゼロ（直近バー取引なし） |
| `ratio ≥ 1.1` + 価格上昇 | 出来高増 + 価格上昇: 上昇の勢いを伴う可能性 |
| `ratio ≥ 1.1` + 価格下落 | 出来高増 + 価格下落: 売り圧力が強まっている可能性 |
| `ratio ≥ 1.1` + 価格横ばい | 出来高増（価格変動なし） |
| `ratio < 0.9` + 価格上昇 | 出来高減 + 価格上昇: 反発の持続性には確認が必要 |
| `ratio < 0.9` + 価格下落 | 出来高減 + 価格下落: 商い薄の下落の可能性 |
| `ratio < 0.9` + 価格横ばい | 出来高減（価格変動なし） |
| `0.9 ≤ ratio < 1.1` | 出来高変化なし（直近平均比 ±10%以内） |

閾値 `1.1`（高出来高）と `0.9`（低出来高）はコード内に固定された定数です。価格方向は `price_diff`（最新終値 - 前足終値）から判定します。

### スコアへの影響

**出来高は総合スコアに加算されません。** 純粋に情報提供のための文脈指標です——価格変動が参加者の裏付けを伴っているかを示す「証人」として機能します。スコアリング対象の各指標（RSI・MACD・EMA・SMA・ADX・ROC・ストキャスティクス・ボリンジャー・フィボナッチ・VWAP・一目均衡表）はこの表示から独立しています。

### xoksa における位置づけ

出来高は価格セクションの直後、スコア集計の前に表示されます。これにより、スコアシグナルを読む前に市場状況の「質」を確認できる設計になっています。LLMプロンプトにも注入され、「プログラムで算出済みの値とコメントを使用し、独自に出来高解釈を創作しない」という指示が付記されます。

> 概念的な背景については **Investor Guide — Section 3: 出来高** を参照してください。
> 実践的な解釈パターンについては **Strategy Guide — Section 4: 出来高データの読み方** を参照してください。

---

## 計算エンジンの分類

xoksa では、計算の正確性と透明性を担保するため、Rust の標準的なテクニカル分析ライブラリである `ta` クレートの使用箇所と、xoksa 独自の設計に基づいたオリジナル計算箇所を使い分け、明示しています。

### `ta` クレート使用指標（標準ロジック）
- RSI（相対力指数）
- MACD（移動平均収束拡散）
- EMA（指数平滑移動平均）
- SMA（単純移動平均）
- ボリンジャーバンド

### オリジナル計算指標（独自アルゴリズム）
- **ADX**: 方向性指数のWilder平滑化実装（RMA）
- **ROC**: 直近価格変化率の独自レンジ判定
- **ストキャスティクス**: 期間内高値安値抽出による実装
- **フィボナッチ**: 直近高値安値からの自動水準算出
- **VWAP**: 各足の Typical Price を出来高で加重。分足は指標計算最終足と同日セッションVWAP（日次リセット）、日足は直近N本VWAP（既定: 14本 / `--vwap-period` で変更可能）
- **一目均衡表**: 転換線・基準線に特化した独自スコアリング

---

## スコア合成と判定ロジック

xoksa は、これまで解説した個別の指標スコアを合成し、最終的な投資判断の「目安」を算出する。

### 1. 指標ごとの重み付け（Weights）

各指標には、ユーザが設定可能な「重み（Weight）」が設定されている。

- デフォルト値は拡張指標がすべて 1.0、基本解析（RSI / MACD、`WEIGHT_BASIC`）のみ 2.0
- 重要視したい指標の重みを大きく（例：2.0）
- 参考程度にしたい指標の重みを小さく（例：0.5）

することで、戦略に合わせた調整が可能である。

**デフォルト**は `xoksa.env`（`WEIGHT_EMA=2.0` …）で設定し、**セッション中だけ**変えたいときはチャットの `/set weight-<指標> <n>`（例：`/set weight-ema 1.5`）で切り替える。また `/set indicator <名> off` でその指標を解析（スコア/表示/LLM）から外せる — 計算・保存は継続され、外れるのは解析だけ。

### 2. トータルスコアの算出（加重平均の原理）

各指標のスコア（-2.0 〜 +2.0）に重みを掛け合わせ、その合計を算出する。

$$TotalScore = \sum (Score_i \times Weight_i)$$

### 3. 正規化（ゲージ変換）

合計スコアを重みの合計で割り、−1.0〜+1.0 の**スコア比率**を求める。ゲージも判定ラベルもこの比率に基づくため、重みを大きく設定した銘柄と既定のままの銘柄を同じ物差しで比べられる。

$$スコア比率 = \frac{\sum (スコア_i \times 重み_i)}{\sum 重み_i}$$

比率は中立を挟んで対称な 9 段階の判定に対応する。

| スコア比率 | 判定 |
| ---: | :--- |
| +0.8 以上 | 🟢 強い買い |
| +0.6 以上 | 🟢 買い優勢 |
| +0.4 以上 | 🟢 買い傾向あり |
| +0.2 以上 | 🟡 やや買い寄り |
| 0.0 以上 | ⚪️ 様子見（中立） |
| −0.2 以上 | 🟠 やや売り寄り |
| −0.4 以上 | 🟠 売り傾向あり |
| −0.6 以上 | 🔴 売り優勢 |
| −0.8 以上 | 🔴 強い売り |

ゲージ自体はバーで描画され、比率をパーセント（例：`+59%`）で併記する。中立からどれだけ離れているかが一目で分かる。

### 4. 判断の柔軟性

xoksa は「この数値なら必ず買え」というブラックボックスなツールではない。
「どの指標がどの重みで効いた結果、このスコアになったか」の内訳を LLM やターミナルに出力することで、最終的な判断をユーザ自身が行うための「高度な検討材料」を提供することを目的としている。

---

> [!TIP]
> **💡 運用のヒント**
> - 指標同士が「ケンカ」している（一方は買い、一方は売り）場合は、スコアが 0 付近に収束し「中立」と表示される。
> - 強いシグナルが出るのは、多くの指標が「同じ方向」を向いたときのみである。
> - 戦略レシピを活用し、自分の投資スタイルに合った指標の組み合わせを見つけることが、xoksa を使いこなす近道となる。

---

## ファンダメンタル指標（PER / PBR / ROE / EPS / BPS / 配当）

`--fundamental` で有効化。対象銘柄の補助的な財務情報を取得する — 日本株は **J-Quants**（JPY）、米国株は **SEC EDGAR**（USD） — テクニカルと併せて LLM のコンテキストに組み込む。

> **これらの指標は売買スコア合成には含めない。** 上記のテクニカル指標と異なり、ファンダメンタルはスコア化・重み付けされない。LLM が説明する客観的な文脈材料であり、LLM が再計算・創作することはない。

### 取得する値 vs. 計算する値

| 指標 | ソース | 算出方法 |
| :--- | :--- | :--- |
| EPS（1株当たり利益）, BPS（1株当たり純資産） | 決算 | 最新の開示期（四半期／通期）から取得 |
| 売上高・営業利益・純利益・自己資本・発行済株式数 | 決算 | 取得 |
| 配当（1株当たり） | 決算 | 実績があれば実績、なければ予想（フラグ付） |
| **PER**（株価収益率） | プログラムで計算 | `最新価格 ÷ EPS` |
| **PBR**（株価純資産倍率） | プログラムで計算 | `最新価格 ÷ BPS` |
| **ROE**（自己資本利益率） | プログラムで計算 | `純利益 ÷ 自己資本` |

計算する3比率はいずれもゼロ除算を回避する（無限大ではなく「該当なし」として扱う）。

### 各指標の読み方

- **PER（株価収益率）**: 株価が現在の利益の概ね何年分かを表す。低いほど利益に対して割安だが、必ず同業他社・自社の過去水準と比較する。EPSがゼロのときは表示されない。赤字企業では PER がマイナスになるが、それは割安さではなく赤字であることのサインである。
- **PBR（株価純資産倍率）**: 1株当たり純資産に対する株価。1.0 を下回ると純資産割れ — 割安の場合もバリュートラップの場合もある。
- **ROE（自己資本利益率）**: 株主資本がどれだけ効率的に利益を生むか。高いほど良い。低PER ＋ 高ROE が王道の「安くて良い」。
- **EPS / BPS**: PER / PBR の基礎となる1株当たり利益・純資産。
- **配当**: 1株当たり年間配当。予想フラグを確認する — 予想値にはラベルが付き、実績（報告）値には付かない。

### 重要な注意

- **価格由来の比率は時点固定**: PER と PBR は*ファンダメンタル取得時点*の最新価格を用いる。`/reload`（テクニカルのみ再取得）では再計算されないため、セッション中に表示される PER/PBR は取得時点の価格を反映している。
- **更新頻度**: ファンダメンタルは四半期/通期で報告されるため、価格よりはるかに更新頻度が低い。
- **対象の開示期（どの数字か）**: パネルは**最新の開示（四半期の 10-Q または通期の 10-K）**を反映し、期種別ラベル（例 `2026-04-26 (Q1)` / `2026-01-25 (FY)`）を付す。損益項目と指標はすべて同一期基準のため、**四半期**パネルの PER/ROE は単一四半期の利益で計算され、通期換算より高め（PER）/低め（ROE）に出る——基準はラベルが示す。日本株（J-Quants）・米国株（SEC EDGAR）とも最新開示に追随する。
- **ソース・市場差**: J-Quants（日本株）と SEC EDGAR（米国株）でフィールドの有無や会計慣行が異なり、銘柄によっては欠落する項目もある。通貨・単位は取得したまま表示し、変換しない。

---

# パート3 — 戦略・レシピ

## **— 鋼のロジックを、11のレシピで使い倒す —**

> [!IMPORTANT]
> **⚠️ 免責事項**
> 本ドキュメントに記載されているレシピや設定例は、あくまで xoksa の活用方法を説明するための**一例**であり、特定の銘柄の売買を推奨するものではありません。
> 実際の投資にあたっては、各指標の性質を正しく理解した上で、**必ず自己責任において最終的な判断を行ってください。**

---

## 1. このドキュメントの目的

このドキュメントは、xoksa を **どのような考え方で使うべきか** を説明します。

- 戦略（レシピ）の設計方法
- 分析結果の検証・再利用の方法

---

## 2. XOKSA での分析の進め方（会話モード）

XOKSA の主役は**ブラウザ・ダッシュボード＋チャット**です（`xoksa serve --ui` → `http://127.0.0.1:8787` を開く）。アイデアごとにコマンドを打ち直すのではなく、銘柄をロードし、確定（SOT）分析を読み、そのまま **AIと会話** して読みを詰め、足を切り替え、「もし〜なら」を尋ね、レシピをライブで検証します。指標値は確定（Source of Truth）のまま固定 — 変えるのは足・回答スタイル・モデル・問いかけです。

**典型的なセッション**

1. **銘柄をロード**（ヘッダー。`/sym add <銘柄>` で比較銘柄を最大5つ追加）。
2. **足を選ぶ**（ヘッダーのドロップダウン。`1m` … `daily` … `monthly`）。
3. **指標パネルを読む** — CLI と同じ確定スコア。
4. **チャットで問う** — 自分の読みをAIにぶつけ、確定データを解説させる（AIは数値を創作しない）。
5. **ライブで詰める**：`/set` で閾値、`/depth`・`/scope` でAIの解釈、`/forum` で複数LLMのセカンドオピニオン。
6. **見て聞く**：チャートのポップアップで区間をドラッグ選択してその区間をAIに質問／🧪 **バックテスト**パネルでルールが Buy&Hold に勝てるか検証。

**ライブで変えられるもの／一度だけ設定するもの**

| 変えたいもの | 場所 | 備考 |
| :--- | :--- | :--- |
| 足種 | ヘッダーのドロップダウン（または `/mode`） | ライブ |
| 閾値・計算パラメータ（RSI・MACD・Bollinger・ADX/ROC/Stoch/VWAP 期間 …） | チャット `/set <項目> <値>` | ライブ・**セッション限定**（`xoksa.env` は不変） |
| **指標の重み** | チャット `/set weight-<指標> <n>` | ライブ・**セッション限定** |
| 解析で**有効にする指標** | チャット `/set indicator <名> on\|off` | ライブ・**セッション限定** — スコア/表示/LLMのみ。計算・保存は継続 |
| **解釈スタンス**（buyer / holder / seller） | チャット `/set stance <buyer\|holder\|seller>` | ライブ・**セッション限定** — ゲージの向き＋LLMの傾き。スコアは変えない |
| 回答スタイル（解釈の深さ・知識の活用・将来予測） | チャット `/depth` `/scope` `/cast` | ライブ |
| モデル／セカンドオピニオン | ヘッダーのモデル選択・`/llm`・`/forum` | ライブ |
| **計算・保存する指標（DBの土台）・既定の重み・スタンス** | **`xoksa.env`** | あくまで**デフォルト**。デフォルトを変えるときだけ編集。レシピにはこれ不要 |

つまり以下の各レシピは、**すべてチャットでライブに回します**（env編集なし）：足を選び、`/set` で重み、`/set stance` でスタンス、`/set indicator` で指標を絞り、`/set` で閾値、AIにそのレシピの問いを投げる。（`xoksa.env` は*デフォルト*を持つだけで、既定の指標セットはすでに全部ON。）

> `/set` は一発上書きの会話版 — セッション限定でファイルを書き換えないので、レシピの閾値・**重み**・**有効指標**を試してすぐ元に戻せます（`/set reset`）。`xoksa.env` を編集するのは**デフォルトを変えたいとき**だけ（例：計算・保存する指標を増やす）。詳細は [command-reference.md](command-reference.md)。（同じ分析は CLI でもヘッドレスに実行できます — §6 参照。）

---

## 3. 実戦汎用レシピ 11選

### ① 【順張り】王道のトレンドフォロー

上昇トレンドの波に乗り、勢いが衰えていないかを確認する最も標準的な設定です。

- **足種**：スイングのトレンドなら `daily`／短い時間軸なら `60m`・`15m` に落とす。
- **問い**：*「いまトレンドフォローで入れる形？ EMA/SMAとADXの状態、RSIに押し目余地があるかで判断して」*
- **読み方**
    - **EMA/SMAスコア**：`+1`〜`+2` が理想。「ゴールデンクロス進行中」なら順調、`0`・マイナスは失速。
    - **ADXスコア**：`+1`(30以上) or `+2`(50以上) で上昇中ならトレンド継続性が高い。
    - **ゲージ**：`🟢 強い買い`／`🟢 買い優勢` を目指す。ADXが低くEMAだけ良いなら「一時的リバウンド」を疑う。**基準クリア** ＝ MAの形◎ ＋ ADX `+1`↑ ＋ RSI 40維持。
- **尖らせる（任意）**：`/set weight-ema 2.0 /set weight-sma 2.0 /set weight-adx 2.5`・`/set stance buyer`・`/set buy-rsi 40`（押し目狙いで高め）。他は `/set indicator <名> off` でミュート。

### ② 【順張り】一目均衡表・転換線と基準線

短い期間の値幅の中心が、長い期間の中心からどれだけ離れたかを重視します。

- **足種**：**`daily`**（必須 — 一目の 9/26 設定（転換線・基準線。先行スパンは持たない）は日足前提。下の注記）。
- **問い**：*「転換線は基準線からどれだけ離れている？ 一目の値だけで判断して」*

> **足の間隔について**: xoksa の一目均衡表は転換線（9）と基準線（26）だけで、先行スパンを持たないので、それに挟まれた帯もありません。どちらも日足を前提に設計されています。短時間足（`1m`/`5m`/`15m`/`30m`/`60m`）で同パラメータを使用すると、基準線が示す期間がセッションの一部に圧縮され、中期的な均衡という本来の意味が成立しません。

- **読み方**
    - **Ichimokuスコア**：転換線と基準線の乖離率を ±4.0% と ±1.0% で区切った値。`+2` は差が最も開いた強気、`0` は両線が 1% 以内に収まっている状態、マイナスは転換線が基準線を下回っている。
    - **メッセージ**：文言はこの差に従う。上も中も無い——帯が無いため。スコアが反応する水準は基準線だけ。
    - **ゲージ(holder)**：`🟡 買い気配`↑を維持、`⚪️ 様子見` 以下は警戒。**基準クリア** ＝ 転換線＞基準線 ＋ SMA `+1`↑。
- **尖らせる（任意）**：`/set weight-ichimoku 3.0 /set weight-sma 1.0`・`/set stance holder`。他は `/set indicator <名> off` でミュート。

### ③ 【順張り】ボラティリティ・ブレイクアウト

ボリンジャーバンドが収束（スクイーズ）し、パワーを貯めた後の爆発を狙います。

- **足種**：`daily` または `60m` — ブレイクはどちらでも読める。トレードする時間軸で。
- **問い**：*「スクイーズからブレイクしそう？ バンド幅の収束と、抜け際にADXが立ち上がるかを見て」*
- **読み方**
    - **Bollingerメッセージ**：`⚠️ Bandwidthが8.00%以下に収束（スクイーズ）→ 抜けた方向への強いトレンド発生に注意` が出ているか。
    - **Bollingerスコア**：上側バンド突破で `-1`〜`-2` になる。スクイーズ中（横ばい）は `0`。**符号は方向ではなく平均回帰の読み**である——xoksa はバンドより上の終値を「過熱」として採点するので、上抜けはマイナスで出る。ブレイク狙いのレシピでは、大きさを「バンドからどれだけ外に出たか」として読む。**方向**はどちら側のバンドを抜けたかで決まり、**ADX が与えるのは強さ**であって方向ではない——ADX は上昇と下落を区別しない。
    - **ADXスコア**：ブレイクと同時に ADX が `+1`(30以上) になれば「本物」の確率が高い。**基準クリア** ＝ スクイーズ警告 ＋ 上バンド接近 ＋ ADX立ち上がり。
- **尖らせる（任意）**：`/set weight-bollinger 3.0 /set weight-adx 2.0`・`/set bb-squeeze 8`（嵐の前の静けさの閾値）。他は `/set indicator <名> off` でミュート。

### ④ 【順張り】短期決戦・モメンタム重視

価格の変化率（ROC）を主役に据え、短期間で一気に駆け上がる銘柄を追います。

- **足種**：短期決戦なら**短時間足**（`5m`・`15m`・`60m`）／数日の急騰なら `daily`。
- **問い**：*「モメンタムはまだ加速中？ ROC（+2→+1に落ちたらピークアウト）と、直近24時間のニュースが追い風かで判断して」*
- **読み方**
    - **ROCスコア**：急騰時は `+2` に張り付く。`+2`→`+1` はピークアウト（短期利確）。
    - **ニュース(LLM)**：直近24時間(`pd`)のタイトル+URL候補に出版元で確認すべき材料があるか。出力はベタ打ち箇条書き前提。
    - **ゲージ**：`🟢 強い買い` の間だけ乗る。急減衰したら深追いしない。**基準クリア** ＝ ROC `+2` ＋ EMA `+1`↑ ＋ 直近ニュース追い風。
- **尖らせる（任意）**：`/set weight-roc 3.0 /set weight-ema 1.0`。他は `/set indicator <名> off` でミュート。*（直近24時間ニュースは env の `NEWS_FRESHNESS=pd` が必要。）*

### ⑤ 【逆張り】売られ過ぎ反発（基本形）

教科書的な「売られ過ぎ」からのリバウンドを論理的に拾います。

- **足種**：時間軸に合わせて — スイングの反発なら `daily`、素早い反発なら分足。
- **問い**：*「リバウンドの土台ができている？ RSI≤30 で、MACDとシグナルの差が1.0未満に収束しているか」*
- **読み方**
    - **RSI**：`30` 以下。`20` 割れは「極端な売られ過ぎ」で反発期待が高まるが、ズルズル下げるリスクも併存。
    - **MACDメッセージ**：差が1.0未満なら「収束（下げ止まり）」。`0` 付近ならリバウンド準備完了。
    - **ゲージ**：`🟡 買い気配` などマイナス→プラスの初動を見る。**基準クリア** ＝ RSI ≤ 30 ＋ MACD差 ≤ 1.0。
- **尖らせる（任意）**：`/set stance buyer /set weight-basic 2.5`（RSI/MACD主体）・`/set buy-rsi 30 /set macd-minus-ok on /set macd-diff-low 1.0`。拡張指標は `/set indicator <名> off` でミュート。

### ⑥ 【逆張り】パニック売り・底値拾い

市場が総悲観になり、セリングクライマックスに達した瞬間を狙う過激な設定です。

- **足種**：パニックの時間軸で — セッション内クライマックスなら `5m`・`15m`、数日の洗い越しなら `daily`。
- **問い**：*「パニック水準？ RSI≤20 で、−2σを割った後に下バンド内へ回帰し始めているか」*
- **読み方**
    - **RSI**：`20` 以下のパニック水準。
    - **Bollingerスコア**：`+2`（下側バンドの 2% 下以下）は落下の最中。`+1`→`0` への回帰（バンド内回帰）が絶好の底拾い。バンドより下の終値は**プラス**で出る——スコアは方向ではなく平均回帰を読んでいる。
    - **ゲージ/LLM**：`🔴`・`🟠` でも総評で「下げ渋り」「乖離の修正」に言及があれば打診買いの材料。**基準クリア** ＝ RSI ≤ 20 ＋ バンド内回帰 ＋ MACD悪化停止。
- **尖らせる（任意）**：`/set stance buyer /set weight-bollinger 2.5 /set weight-basic 1.5`・`/set buy-rsi 20 /set macd-minus-ok on`。他は `/set indicator <名> off` でミュート。

### ⑦ 【逆張り】VWAP回帰（平均回帰戦略）

適正価格（VWAP）から大きく乖離した株価が、磁石のように引き戻される性質を利用します。
xoksa の VWAP は、Typical Price を出来高で加重した価格帯として扱います。分足モードでは指標計算最終足と同日セッション足のみ（日次リセット）、非分足モード（日足/週足/月足）では直近N本（既定14本 / `/set vwap-period` で変更可能）が参照幅です。

> **足の間隔について**: VWAPはセッション（1日）の始値でリセットされる前提の指標です。複数日をまたぐ短時間足（`1m`/`5m`/`15m`/`30m`/`60m`）で計算すると、日境界をまたいだ平均になりセッション内の価格重心の意味を失います。このレシピでは、当日1セッション内の分足か、非分足（`daily`/`weekly`/`monthly`）をヘッダーで選んでください。

- **足種**：**当日1セッションの分足**または **`daily`**（VWAPはセッションのアンカーが要る。上の注記）。`/set vwap-period <n>` で日足の参照幅。
- **問い**：*「価格はVWAPからどれだけ下に乖離？ 乖離が縮小し始めた（平均回帰の入り）か」*
- **読み方**
    - **VWAPスコア**：`−2` が統計的な「売られすぎ」——終値が VWAP より 3% 以上**下**にある状態。（`+2` は逆に、終値が上へ伸びた状態。）
    - **価格とVWAP**：`VWAP: XXX.X` に対し最終足終値がどれほど低いか。乖離縮小で回帰のサイン。
    - **ゲージ**：`🟡`↑に浮上したら平均への収束（リバウンド）を狙える局面。**基準クリア** ＝ VWAP `−2` ＋ RSI ≤ 35 ＋ 乖離縮小。
- **尖らせる（任意）**：`/set stance buyer /set weight-vwap 3.0`・`/set buy-rsi 35`。他は `/set indicator <名> off` でミュート。

### ⑧ 【防御】ホルダーの警戒モード

保有株の「逃げどき」を探る、ディフェンス重視の設定です。

- **足種**：**`daily`**（ホルダーの警戒時間軸。長期保有なら `weekly`）。
- **問い**：*「ホルダーの逃げどきサインは？ RSI≥65、上バンド反落、MACD差の縮小で判断して」*
- **読み方**
    - **RSI**：売り圏の判定は `sell-rsi`（既定 `70`）。このレシピでは `65` に下げて「利確準備」を早めに拾う。RSI の値そのものは色を付けずに表示される。
    - **Bollingerメッセージ**：`価格が上側バンドを突破（過熱圏）` に注意。その後バンド内に押し戻されたら天井の可能性。
    - **ゲージ(holder)**：`🟠 売り気配` 方向に振れたら、利益が残るうちの一部売却を検討。**基準クリア** ＝ RSI ≥ 65 ＋ 上バンド反落 ＋ MACD差縮小。
- **尖らせる（任意）**：`/set stance holder /set weight-bollinger 2.0`・`/set sell-rsi 65 /set macd-diff-mid 5`（早めの警告）。他は `/set indicator <名> off` でミュート。

### ⑨ 【全方位】AIセカンドオピニオン

全指標をフル稼働させ、AIに「指標間の不整合」を炙り出させます。

- **足種**：**`daily`**（一目・VWAP・フィボが要る。下の注記）。
- **問い**：*「全指標ONで、どこが食い違う？ 矛盾（例：トレンド系+2なのにオシレーター−2）と自分の死角を指摘して」* — または `/forum ask` で複数LLMの委員会に。
- **指標**：**全部有効**のまま（既定。絞っていたら `/set reset`）。

> **足の間隔について**: このレシピには一目均衡表・VWAP・フィボナッチの3指標がすべて含まれています。日足では設計通りに機能しますが、短時間足（1m/5m/15m/30m/60m）ではこれら3指標の信頼性がいずれも低下します。短時間足モードで使用する場合は、これら3指標のスコアを補足的な文脈として扱い、RSI・MACD・EMA・ADX・ボリンジャーを主軸に置いてください。

- **読み方**
    - **スコアの不整合**：「トレンド系(+2)」なのに「オシレーター系(−2)」等の矛盾が、AIの「勢いはあるが過熱」解説の根拠。
    - **Fibonacci/一目**：転換線と基準線の開きと、目標価格（38.2%戻し等）の重合をAIが指摘するか。AI に先行スパンやその帯のデータは渡していないので、それらに言及したらそれは創作である。日足前提で、短時間足では参考値。
    - **LLM総評**：結論の「買い/売り」より、AIが「懸念点」に挙げる指標を特定し死角を埋める。**基準クリア** ＝ 全指標同方向→自信度高／割れ→短期と中期を分けて考える。
- **尖らせる（任意）**：バイアスは不要 — フルレーダーとAIの矛盾炙り出しが主役。

### ⑩ 【情報戦】材料・テーマ深掘り

テクニカルはベースに留め、ニュースのタイトル+URL候補をAIで確認優先度順に整理します。

- **足種**：不問 — ニュースは足種非依存。チャートの足種はお好みで。
- **問い**：まず `/nx add "業績予想 修正 増配"` で材料を取得 → *「この見出し群を確認優先度（Tier A〜C）で仕分けて。どの出版元URLを先に開くべき？」*
- **読み方**
    - **ニュース(LLM)**：タイトル+URL候補を Tier A（優先度高）〜 Tier C（参考）に仕分け。重要そうなものはURLから出版元で確認。出力はベタ打ち箇条書き前提。
    - **総評**：AIコメントは*タイトル単位*のヒント（本文は読んでいない）。**基準クリア** ＝ Tier A候補の集積 ＋ 自分の出版元確認 → 情報主導の局面。
- **尖らせる（任意）**：チューニング不要 — 問いとニュース仕分けの読みが主役。

### ⑪ 【バリュー】ファンダメンタル確認 ＋ テクニカルのタイミング

ファンダメンタルが健全で割安な銘柄を絞り込み、軽いテクニカルの重ね合わせでエントリータイミングを計ります。ファンダメンタルの有効化が必要です。

- **足種**：**`daily`** 以上 — バリューは遅い時間軸で効く。
- **問い**：*「これは"安くて良い"（妥当なPER/PBR・健全なROE）で、タイミングも来ている？ EMA/SMAの下落が止まり、RSIが底から反転しているか」*
- **読み方**（バリュー指標は J-Quants / SEC EDGAR から取得しプログラムで計算済み — AIは説明のみで再計算・創作しない）
    - **PER / PBR**：低いほど利益・純資産に対し割安だが、同業と自社過去水準で読む。極端に低いPBRはバリュートラップの兆候のことも。
    - **ROE**：収益性・質。低PER ＋ 高ROE が王道の「安くて良い」。低PER ＋ 低ROE は要警戒。
    - **配当**：実績値か予想値か（xoksaは予想にフラグ）。
    - **テクニカル重ね合わせ**：EMA/SMA ＋ RSI は*タイミング*のみ。下落継続中の割安株はまだ買いでない。トレンドスコアの悪化停止を待つ。**基準クリア** ＝ 妥当なPER/PBR ＋ 健全なROE ＋ EMA/SMAの下落停止 ＋ RSIの底からの反転。
- **尖らせる（任意）**：`/set stance buyer /set weight-ema 1.0 /set weight-sma 1.0`・`/set buy-rsi 35`。他は `/set indicator <名> off` でミュート。*（ファンダは env 有効化が必要 — `FUNDAMENTAL=true` ＋ J-Quants/EDGAR キー。）*

> **データ鮮度について**: ファンダメンタルは四半期/通期で報告され、価格よりはるかに更新頻度が低い。イントラデイで再実行してもファンダ数値は動かず、テクニカルの重ね合わせのみが更新される。

---

## 4. 出来高データの読み方

xoksa は分析のたびに出来高を3つの数値とコメントで表示します。本セクションではその意味と活用方法を説明します。

### 表示の見方

```
📈 出来高: 92,980,700株
📈 平均出来高 (20本): 240,374,445株 / 倍率: 0.39x
   出来高減 + 価格上昇: 反発の持続性には確認が必要
```

| フィールド | 意味 |
| :--- | :--- |
| **出来高** | 取引が確認された最新のバー（volume > 0）の出来高。Yahoo Finance はイントラデイデータの末尾にvolume = 0のプレースホルダーバーを返すことがあるため、xoksa はそれをスキップし、直前の実取引済みバーを採用する。 |
| **平均出来高 (N本)** | 直近N本のうち volume > 0 のバーの単純平均。Nは `sma_long_period`（デフォルト20本）を共用。volume = 0のバーは平均計算から除外。 |
| **倍率** | `最新出来高 ÷ 平均出来高`。1.00x が平均的な参加量。0.39x は平均の39%しか出来高が発生していないことを意味する。 |

### コメントパターン

| 倍率の範囲 | 価格方向 | コメント | 解釈 |
| :--- | :--- | :--- | :--- |
| `= 0.0` | 不問 | 出来高ゼロ（直近バー取引なし） | 取引が成立していないバー。このバーを根拠に判断しない。 |
| `≥ 1.1×` | 上昇 | 出来高増 + 価格上昇: 上昇の勢いを伴う可能性 | 買い手が出来高を伴って参加している。動きに根拠がある状態。 |
| `≥ 1.1×` | 下落 | 出来高増 + 価格下落: 売り圧力が強まっている可能性 | 売り手が出来高を伴って参加している。追随の売りに警戒。 |
| `≥ 1.1×` | 横ばい | 出来高増（価格変動なし） | 活動は活発だが方向感が出ていない。もみ合いや転換前夜の状態。 |
| `< 0.9×` | 上昇 | 出来高減 + 価格上昇: 反発の持続性には確認が必要 | 出来高を伴わない上昇。上記の例がこれにあたる。参加者が少ないまま価格だけ上がる状態は反落しやすい。 |
| `< 0.9×` | 下落 | 出来高減 + 価格下落: 商い薄の下落の可能性 | 出来高の少ない下落は売り圧力の弱さを意味することが多く、出来高急増を伴う急落より反転しやすい傾向がある。 |
| `< 0.9×` | 横ばい | 出来高減（価格変動なし） | 参加者が引いている。出来高シグナルが出るまで動意なしと見る。 |
| `0.9×〜1.1×` | 不問 | 出来高変化なし（直近平均比 ±10%以内） | 通常の商いであり、出来高面での優位性も不利もない。 |

### 表示例の読み方

```
出来高: 92,980,700株
平均出来高 (20本): 240,374,445株 / 倍率: 0.39x
出来高減 + 価格上昇: 反発の持続性には確認が必要
```

倍率 0.39x は、通常の約60%の出来高しか発生していない中で価格が上昇していることを意味します。テクニカル分析上のクラシックな注意サインです。主な原因としては以下が考えられます。

- 本格的な買い需要ではなく機械的な踏み上げ（ショートカバー）
- サポートラインへの接触後のリバウンドだが追随する買い手が薄い
- 時間帯の影響（前場の開始直後・昼休み前後・大引け前のポジション整理）

**「即座に売り」ではありません**。広義のトレンドが健在であれば、出来高が少なくても上昇が継続することはあります。ただし確信度を上げるためには他の指標との照合が必要です。

- ADXが強く（≥ 40）、EMA/SMAがゴールデンクロスを維持中 → トレンドは継続中で出来高の一時的な低下は許容範囲。注意サインとして留め、即行動のシグナルとして扱わない。
- MACDがマイナスで、かつストキャスティクスが売られ過ぎ域にある → 出来高薄の上昇はリスクが高く、利益確定圧力が出やすいセットアップ。
- VWAPとの乖離率がまだ大きくマイナス（価格 < VWAP）→ VWAPへの回帰途上であれば、出来高が薄くても構造的には正常な動き。

### レシピ別の活用ポイント

| レシピ | 注目する出来高シグナル |
| :--- | :--- |
| ① 王道トレンドフォロー | ゴールデンクロスが発生したバーの倍率が ≥ 1.0× かどうかを確認。0.3〜0.5×のクロスは失敗率が高い。 |
| ③ ボラティリティ・ブレイクアウト | スクイーズからのブレイクは倍率 ≥ 1.2× を確認してダマシを除外する。 |
| ④ 短期モメンタム | ROCスコア +2 ＋ 倍率 ≥ 1.5× が高確信度の組み合わせ。ROC +2 だが倍率 0.4× はモメンタム枯渇のサイン。 |
| ⑤⑥ 逆張り反発・底値拾い | RSIが最も低い水準の時に倍率 ≥ 2× のスパイク（セリングクライマックス）が出ているかを確認。これが出ていない底値拾いは時期尚早の可能性がある。 |
| ⑦ VWAP回帰 | VWAP回帰の上昇局面で倍率 ≥ 1.0× があると動きが本物。出来高薄の回帰はVWAPに届く前に失速しやすい。 |

### データに関する補足

- 出来高データは Yahoo Finance の OHLCV バーから取得し、単位は**株数**です。取引件数（約定回数）でも、売買代金（円）でもありません。
- 平均の期間は `sma_long_period`（デフォルト20本）を共用しています。より長い参照期間にしたい場合は `--sma-long-period` オプションまたは `xoksa.env` の `SMA_LONG_PERIOD` を変更してください。
- **日足モード**: 1バー = 1取引日。出来高はその日の総取引株数を表します。
- **分足モード（1m/5m/15m/30m/60m）**: 1バー = 指定インターバル。平均は同じ足種の直近20本の平均であり、1日分の合計ではありません。
- **週足/月足モード**: 1バー = 1週または1ヶ月。平均は同じ上位足種の直近20本の平均です。

---

## 5. 足の選択：意図とデータを一致させる

xoksa を使う上で最も重要な判断が、**分析の意図**と**足の選択**を一致させることです。足の設定によって xoksa が取得するデータの範囲が決まり、それが LLM に渡される「見える範囲」になります。

> **大原則:** 「1ヶ月の動きを見せて」という意図で 5分足を使っても、LLM が受け取るデータは直近5営業日分だけです。5分足はその質問に正確に答えるためのデータを持っていません。意図とデータが一致していないと、LLM はどれだけ優秀でも正確な分析を返せません。

### 足ごとの取得データ概要

xoksa は実行のたびに Yahoo Finance から新規にデータを取得します。**ローカルへの蓄積は行いません**。毎回の実行がフレッシュな取得です。

| 足モード | xoksa が取得する期間 | 取得本数（目安） | LLM が見られる時間的深さ |
| :--- | :--- | ---: | :--- |
| `1m` | 直近5営業日 | 約1,950本 | 約5日間 |
| `5m` | 直近5営業日 | 約390本 | 5日間 |
| `15m` | 直近1ヶ月 | 約520本 | 約1ヶ月 |
| `30m` | 直近1ヶ月 | 約260本 | 約1ヶ月 |
| `60m` | 直近2ヶ月 | 約250本 | 約2ヶ月 |
| `daily` | 直近3ヶ月 | 約65本 | 約3ヶ月 |
| `weekly` | 直近2年 | 約104本 | 約2年 |
| `monthly` | 直近10年 | 約120本 | 約10年 |

> **取得本数と時間的深みは別物です。** `5m` モードで約390本取得しても、それは5営業日の出来事です。`daily` で390本なら1年半以上の歴史です。「本数が多い＝過去が深い」ではありません。

### Yahoo Finance のイントラデイデータ保存制限

Yahoo Finance には分足データの保存期間に上限があります。取得期間をどれだけ長く設定しても、この上限を超えるデータは存在しません。

| インターバル | Yahoo Finance の保存期間上限 |
| :--- | :--- |
| 1m | 約7日 |
| 5m | 約60日 |
| 15m | 約60日 |
| 30m | 約60日 |
| 60m | 約730日 |
| 日足 / 週足 / 月足 | 数十年分 |

xoksa の取得期間はこの上限内に設定しています。仮に `5m` モードの取得期間を延ばしても、60日を超えるデータは取得できません。

### 分析意図から足モードを選ぶ

| 分析したいこと | 推奨足 | 理由 |
| :--- | :--- | :--- |
| 今の値動き・エントリーの微調整（スキャル） | `1m` | 最も細かい解像度。ただし直近約5営業日しか存在せず、どの指標もノイズが非常に大きい |
| 今日の動き | `5m` または `15m` | 当日〜直近セッションを詳細に把握できる |
| 今週の動き | `30m` または `60m` | 複数セッションをまたいだ視点、5m/15m よりノイズが少ない |
| 1ヶ月の動き | `daily` | 1バー=1営業日で約65本（約3ヶ月分）、中期の流れを把握 |
| 半年〜1年のトレンド | `weekly` | 1バー=1週で104本（2年分）、トレンド構造が見える |
| 超長期の流れ | `monthly` | 1バー=1ヶ月で120本（10年分） |

**ミスマッチの例:** `5m` の足で「この銘柄の先月からの動きを説明して」と LLM に依頼 → LLM が受け取るのは直近5日分のデータだけであり、先月の情報は含まれていません。プロンプトをどれだけ工夫しても、取得されていないデータを再現することはできません。

### 足種別のキャラクター

**`1m` — ティックレベルのスキャル**

- 最も細かい解像度。ノイズが極めて大きく、シグナルは絶えず反転する
- 直近約5営業日分しか存在しない（Yahoo の 1分足保存は約7日）
- 指標のデフォルト（RSI 14・MACD 12/26/9・一目均衡表 9/26（転換線・基準線。先行スパンは持たない）・ボリンジャー 20）は**日足**を前提に設計されている — 1分足では「分単位の揺れ」を測っているだけで、トレンドではない。方向判断ではなく**微視的なモメンタム**として読む
- 適する用途: 上位足で見つけたセットアップの中での**エントリー/イグジットの微調整**。銘柄のトレンド判断には使わない

**`5m` / `15m` — セッション内の動き**

- 当日〜直近数セッション内の動きを詳細に把握。シグナルの転換が速い
- バー本数が多く、ノイズも多い
- 適する用途: 寄り付き直後のギャップ、ニュース直後の反応確認、日中モメンタムの追跡
- 注意: 一目均衡表（9本=45分 or 135分 — 日単位の中期均衡として機能しない）、フィボナッチ（S/Rが数時間スパン）、VWAP（`15m` は1ヶ月分で複数日をまたぐため日次リセットの意味が薄れる）

**`30m` — 複数セッションにまたがる短期**

- 数週間分の分足構造を俯瞰できる。5m/15m よりノイズが少なくシグナルの質が向上
- VWAP: 複数セッションにまたがるため参考情報として扱う

**`60m`（1時間足）— イントラデイスイング**

- 分足モードの中で最もノイズが少ない。1本=1時間の値動きなので、シグナルの重みが増す
- 日足と分足の「橋渡し」として機能。数日にわたる値動きを分足視点で把握できる
- **実用例:** `daily` でセットアップを確認（RSI 過売り圏 + EMA サポート）→ `60m` の MACD クロスやボリンジャースクイーズ解放をエントリートリガーとして待つ
- VWAP: `30m` 同様、複数セッションにまたがるため参考情報として扱う
- 一目均衡表: 9本=9時間 / 26本=26時間 — 日単位の均衡から外れるため日足以上での使用を推奨

**`daily`（日足）— スタンダード**

- すべての指標がオリジナル設計通りに機能する
- RSI(14)・MACD(12/26/9)・一目均衡表(9/26。転換線・基準線のみ)・ボリンジャー(20) のデフォルト値は日足を前提として設計・検証されている
- ほとんどのユーザーの出発点として最適

**`weekly` / `monthly`（週足/月足）— 上位足**

- 日次のノイズを除去した支配的なトレンド構造が見える
- 週足シグナルは日足シグナルより持続性が高い
- 日足や分足でエントリーする前に、上位足でバイアスの方向を確認する目的に最適

### 足種別の指標信頼性

| 指標 | 1m〜30m | 60m | 日足 | 週足/月足 |
| :--- | :---: | :---: | :---: | :---: |
| RSI | ✓ | ✓ | ✓✓ | ✓✓ |
| MACD | ✓ | ✓ | ✓✓ | ✓✓ |
| EMA / SMA | ✓ | ✓ | ✓✓ | ✓✓ |
| ボリンジャー | ✓ | ✓ | ✓✓ | ✓✓ |
| ADX | ✓ | ✓ | ✓✓ | ✓✓ |
| ストキャスティクス | ✓ | ✓ | ✓✓ | ✓✓ |
| ROC | ✓ | ✓ | ✓✓ | ✓✓ |
| 一目均衡表 | △ | △ | ✓✓ | ✓✓ |
| VWAP | ✓✓（同セッション内）/ △（複数セッション） | △ | ✓ | ✓ |
| フィボナッチ | △ | △ | ✓✓ | ✓✓ |

△ = 信頼性が低下。主力シグナルではなく補足的な文脈として扱うことを推奨。`1m` は `1m〜30m` 列の中でも最もノイズが大きい端 — どの指標もトレンドではなく微視的モメンタムとして読むこと。

---

## 6. CLI（ヘッドレス／自動化）

主役はダッシュボードですが、**同じエンジン**をコマンドラインからも使えます（スクリプト・cron・単発用）。数値はGUIと同一（SOT）。

```bash
xoksa -t 7203.T             # 1回分析してターミナルに出力
xoksa -t 7203.T --chat      # ターミナルで対話チャット
xoksa -t AAPL,MSFT,NVDA     # 複数を一度に比較
```

レシピの土台は、`-I` ＋指標／重み／閾値フラグで単発実行時にインラインでも渡せます（`xoksa.env` を編集せず）。全一覧は [command-reference.md](command-reference.md)。自動化向け。探索はダッシュボードが楽です。

---

## 7. 最後に

> **ここに挙げたのは使い方の一例です。自由にカスタマイズしていただき、自分に合った分析パラメータ作りにチャレンジしてみてください。**

**Stock Technical 'AI' Analysis Tool — Created & Designed by Kozo2000**
