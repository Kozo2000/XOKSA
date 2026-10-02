//! 統合テスト（tests/ ディレクトリ）
//!
//! ライブラリ crate の pub 関数をまたいで、
//! 実際のユースケースに近いシナリオを検証する。
//! 外部 API (Yahoo Finance / Brave / OpenAI) は呼び出さない。

// ─── ティッカー正規化パイプライン ────────────────────────────────────────

/// normalize_ticker_input → sanitize_ticker の連鎖が正しく機能する。
/// 展開されるのは行き先そのものを指す通称だけで、指数名は展開しない
/// （`日経平均` が `1321.T` に化けていたのを 2.9.9 で削除した）。
#[test]
fn ticker_pipeline_alias_then_sanitize() {
    let raw = "全米";
    let normalized = xoksa::bootstrap::normalize_ticker_input(raw);
    let sanitized = xoksa::bootstrap::sanitize_ticker(&normalized).unwrap();
    assert_eq!(sanitized, "VTI");
}

/// 指数名は別の銘柄で答えず、そのまま通す（後段で未知として失敗する）。
#[test]
fn ticker_pipeline_leaves_an_index_name_alone() {
    for raw in ["日経平均", "TOPIX", "NASDAQ100"] {
        assert_eq!(xoksa::bootstrap::normalize_ticker_input(raw), raw);
    }
}

#[test]
fn ticker_pipeline_lowercase_us_stock() {
    let raw = "nvda";
    let normalized = xoksa::bootstrap::normalize_ticker_input(raw);
    let sanitized = xoksa::bootstrap::sanitize_ticker(&normalized).unwrap();
    assert_eq!(sanitized, "NVDA");
}

#[test]
fn ticker_pipeline_4digit_appends_t_and_sanitizes() {
    let raw = "7203";
    let normalized = xoksa::bootstrap::normalize_ticker(raw);
    let sanitized = xoksa::bootstrap::sanitize_ticker(&normalized).unwrap();
    assert_eq!(sanitized, "7203.T");
}

// ─── インジェクション攻撃ブロック（全サニタイザー横断） ─────────────────

/// セミコロンは全 3 サニタイザーで拒否される
#[test]
fn injection_semicolon_blocked_at_all_entry_points() {
    assert!(xoksa::bootstrap::sanitize_ticker("AAPL;DROP TABLE").is_err());
    assert!(xoksa::bootstrap::sanitize_news_query("NVDA;rm -rf /").is_err());
    assert!(xoksa::bootstrap::sanitize_llm_note("note;evil").is_err());
}

/// パイプは全 3 サニタイザーで拒否される
#[test]
fn injection_pipe_blocked_at_all_entry_points() {
    assert!(xoksa::bootstrap::sanitize_ticker("AAPL|bash").is_err());
    assert!(xoksa::bootstrap::sanitize_news_query("query|cat /etc/passwd").is_err());
    assert!(xoksa::bootstrap::sanitize_llm_note("note|evil").is_err());
}

/// バッククォートは全 3 サニタイザーで拒否される
#[test]
fn injection_backtick_blocked_at_all_entry_points() {
    assert!(xoksa::bootstrap::sanitize_ticker("AAPL`cmd`").is_err());
    assert!(xoksa::bootstrap::sanitize_news_query("`whoami`").is_err());
    assert!(xoksa::bootstrap::sanitize_llm_note("use `rm -rf`").is_err());
}

// ─── jp_code_from_ticker ─────────────────────────────────────────────────

/// 4桁コードが .T 付きに正規化されたあと jp_code_from_ticker が Some を返す
#[test]
fn jp_code_from_normalized_ticker_returns_some() {
    let ticker = xoksa::bootstrap::normalize_ticker("9984");
    let result = xoksa::bootstrap::jp_code_from_ticker(&ticker);
    assert_eq!(result, Some("9984".to_string()));
}

/// US ティッカーは jp_code_from_ticker が None を返す
#[test]
fn jp_code_from_us_ticker_returns_none() {
    assert!(xoksa::bootstrap::jp_code_from_ticker("AAPL").is_none());
}

// ─── VWAP 計算 ─────────────────────────────────────────────────────────

fn market_row(
    date: &str,
    high: f64,
    low: f64,
    close: f64,
    volume: Option<f64>,
) -> xoksa::market::MarketData {
    xoksa::market::MarketData {
        date: date.to_string(),
        datetime: None,
        timestamp: None,
        timezone: None,
        high,
        low,
        close,
        volume,
        name: None,
    }
}

#[test]
fn calculate_period_vwap_weights_typical_price_by_volume() {
    let data = vec![
        market_row("2026-01-01", 100.0, 100.0, 100.0, Some(10_000.0)),
        market_row("2026-01-02", 12.0, 8.0, 10.0, Some(100.0)),
        market_row("2026-01-03", 24.0, 18.0, 18.0, Some(300.0)),
    ];

    let vwap = match xoksa::technical::indicators::calculate_period_vwap(&data, 2) {
        Ok(v) => v,
        Err(e) => panic!("VWAP should calculate, got: {e}"),
    };

    assert!((vwap - 17.5).abs() < f64::EPSILON);
}

#[test]
fn calculate_period_vwap_rejects_missing_volume() {
    let data = vec![
        market_row("2026-01-01", 12.0, 8.0, 10.0, Some(100.0)),
        market_row("2026-01-02", 24.0, 18.0, 18.0, None),
    ];

    assert!(xoksa::technical::indicators::calculate_period_vwap(&data, 2).is_err());
}
