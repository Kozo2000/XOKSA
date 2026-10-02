//! Local HTTP server for the XOKSA Web UI (`xoksa serve --ui`).
//!
//! This is the transport/wiring layer only: CLI parsing for the `serve`
//! subcommand, the axum router, binding, and static-asset serving. All request
//! handling lives in [`api`]. The server is intentionally **local-first** — it
//! binds to `127.0.0.1` by default and exposes a small JSON API plus a static
//! dashboard.
//!
//! ## Architecture boundary (do not cross)
//! The browser/WASM UI is presentation only. Analysis, indicator math,
//! fundamental/market fetching, the LLM call, and backtests stay on this native
//! side and are reached only through the HTTP API. Nothing in the WASM UI
//! performs those computations.
//!
//! ## Assets
//! The dashboard is served from the Leptos/WASM build output on disk (default:
//! `webui-leptos/dist`, overridable with `--web-dir`).
//!
//! The built UI can be **embedded into the binary** for a self-contained release
//! (the `embedded-ui` cargo feature, via `include_dir!`). `read_asset` serves from
//! `--web-dir` on disk first, then falls back to the embedded assets — so a dev
//! build uses `dist/` on disk and a release binary needs no external files.

mod api;
pub(crate) mod monitor;

use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{ConnectInfo, DefaultBodyLimit, Form, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Extension, Router,
};
use clap::Parser;
use std::net::{IpAddr, SocketAddr};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use zeroize::Zeroizing;

/// Default dashboard port — the single definition shared by the `serve` CLI flag,
/// the settings form (`SERVE_PORT`), and the clients that must know where to
/// connect. Remote devices rely on this number being stable.
pub const DEFAULT_SERVE_PORT: u16 = 8787;

/// Baseline security response headers applied to every response (OWASP API8).
/// `nosniff` stops MIME confusion, `DENY` blocks clickjacking via framing, and a
/// no-referrer policy avoids leaking the local URL outbound. The CSP is strict —
/// **no `'unsafe-inline'`**: the single inline module script (trunk's WASM
/// bootstrap) carries a per-request `nonce` (stamped by `serve_index`), and every
/// former inline handler/style was de-inlined (event delegation, SVG presentation
/// attributes, CSSOM-adopted popup stylesheets). `'wasm-unsafe-eval'` is required
/// by the WASM; `connect-src 'self'` blocks exfiltration; `form-action 'self'`
/// (it does not fall back to `default-src`), `object-src`/`base-uri 'none'`, and
/// `frame-ancestors 'none'` round it out. All resources are same-origin.
async fn security_headers(mut req: Request, next: Next) -> Response {
    // Mint the per-request nonce and expose it to handlers (serve_index) before
    // running them, so the served HTML and this CSP header share the same value.
    let nonce = random_hex_token();
    req.extensions_mut().insert(CspNonce(nonce.clone()));
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert(
        "X-Content-Type-Options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert("X-Frame-Options", HeaderValue::from_static("DENY"));
    h.insert("Referrer-Policy", HeaderValue::from_static("no-referrer"));
    // Deny browser features the dashboard never uses (defense-in-depth).
    h.insert(
        "Permissions-Policy",
        HeaderValue::from_static(
            "geolocation=(), camera=(), microphone=(), payment=(), usb=(), \
             accelerometer=(), gyroscope=(), magnetometer=()",
        ),
    );
    let csp = format!(
        "default-src 'self'; \
         script-src 'self' 'wasm-unsafe-eval' 'nonce-{nonce}'; \
         style-src 'self'; \
         img-src 'self' data:; \
         connect-src 'self'; \
         object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"
    );
    if let Ok(v) = HeaderValue::from_str(&csp) {
        h.insert("Content-Security-Policy", v);
    }
    resp
}

/// CSRF defence-in-depth (OWASP API6): reject any **state-changing** (POST)
/// request whose `Origin`/`Referer` does not match the server's `Host`. GET is
/// left alone (the SSE chat endpoint carries its own check). This does not rely
/// on the browser's CORS preflight — it is a server-side control.
async fn reject_cross_origin_mutations(req: Request, next: Next) -> Response {
    // The login POST (`/auth`) is exempt from the origin check: it is gated by the
    // unguessable 256-bit token itself (a cross-site POST without it just gets 401),
    // and the strict `Referrer-Policy: no-referrer` makes browsers send `Origin: null`
    // on that same-origin form POST — which this check would otherwise reject,
    // breaking a legitimate login. Every other state-changing POST keeps the check.
    let is_login_post = req.method() == Method::POST && req.uri().path() == "/auth";
    if !is_login_post && req.method() == Method::POST && api::cross_origin_rejected(req.headers()) {
        return (StatusCode::FORBIDDEN, "cross-origin request rejected").into_response();
    }
    next.run(req).await
}

/// Hard ceiling on requests admitted concurrently (OWASP API4 — unrestricted
/// resource consumption).
///
/// This was `u16::MAX`, which bounded nothing a host would reach before its own
/// RAM and file descriptors did; §4 promises to shed over capacity with `503`,
/// and 65,535 permits meant something else ran out first.
///
/// 256 against what a real session asks for: HTTP/1.1 holds a browser to six
/// connections per host, so this is some forty tabs' worth, and the alert
/// monitor runs inside the process without passing through this layer at all.
/// Measured at 6, 30 and 100 concurrent requests — no `503`.
///
/// **A live SSE stream does not hold a permit.** The permit is released when the
/// handler returns its `Response`, and an SSE body streams after that point —
/// measured: with the ceiling at 2 and two streams open, two ordinary requests
/// still succeeded and only the third was shed. (The comment here used to say
/// the opposite, and a long chat was expected to occupy the pool. It does not.)
///
/// What a permit *is* held for is the whole handler, and nothing on this side
/// cuts a handler short — so the hold is bounded by the outbound client timeout,
/// capped at [`crate::config::LLM_TIMEOUT_SECS_MAX`].
const MAX_CONCURRENT_CONNECTIONS: usize = 256;

/// Process-wide admission permits, sized once to [`MAX_CONCURRENT_CONNECTIONS`].
static CONNECTION_PERMITS: std::sync::OnceLock<tokio::sync::Semaphore> = std::sync::OnceLock::new();

/// Admission control (OWASP API4): bound the number of requests in flight. A
/// request that cannot get a permit is shed immediately with `503` rather than
/// queued — saturation can never become an unbounded wait (which would reintroduce
/// the hang the per-session lock removed). The permit is released when the handler
/// returns its `Response`; for a streaming response that is before the body is
/// sent, so a live SSE stream holds nothing.
async fn limit_concurrency(req: Request, next: Next) -> Response {
    let permits =
        CONNECTION_PERMITS.get_or_init(|| tokio::sync::Semaphore::new(MAX_CONCURRENT_CONNECTIONS));
    match permits.try_acquire() {
        Ok(_permit) => next.run(req).await,
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "server at capacity").into_response(),
    }
}

/// Per-client Web session identifier, carried in the `xoksa_sid` cookie and
/// injected into request extensions by [`ensure_session_cookie`]. Session-stateful
/// handlers key their per-client chat session on this — never on `symbol|timeframe`
/// alone, which would let any client read or mutate another client's session
/// (history, LLM selection, context). See `chat::exec` session store.
#[derive(Clone)]
pub(crate) struct SessionId(pub String);

/// Per-request CSP nonce (random hex), threaded into request extensions so the
/// index handler can stamp the bootstrap `<script nonce=…>` and `security_headers`
/// emits the matching `script-src 'nonce-…'` — letting the CSP drop `'unsafe-inline'`.
#[derive(Clone)]
pub(crate) struct CspNonce(pub String);

/// A fresh 128-bit random token, lowercase-hex encoded (32 chars), from the OS
/// CSPRNG. Used for both the session id and the per-request CSP nonce.
fn random_hex_token() -> String {
    let mut buf = [0u8; 16];
    getrandom::getrandom(&mut buf).expect("OS CSPRNG unavailable");
    let mut s = String::with_capacity(32);
    for b in buf {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Pull a well-formed `xoksa_sid` (exactly 32 hex chars) out of a raw `Cookie`
/// header. Anything else is rejected so a client cannot inject an arbitrary map
/// key; a rejected/absent cookie simply mints a fresh session id.
fn parse_session_cookie(cookie_header: &str) -> Option<String> {
    cookie_header
        .split(';')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| k.trim() == "xoksa_sid")
        .map(|(_, v)| v.trim().to_string())
        .filter(|v| v.len() == 32 && v.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Session isolation (OWASP API1 — broken object-level authorization): give each
/// client a random `xoksa_sid` cookie and thread it into request extensions, so
/// every browser gets its own chat-session set instead of sharing one keyed by
/// `symbol|timeframe`. `HttpOnly` keeps it out of JS (XSS can't read it),
/// `SameSite=Strict` blocks cross-site sends (CSRF). No `Secure` — the server is
/// local-first HTTP; add it when TLS is introduced.
async fn ensure_session_cookie(mut req: Request, next: Next) -> Response {
    let existing = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_session_cookie);
    let (sid, is_new) = match existing {
        Some(s) => (s, false),
        None => (random_hex_token(), true),
    };
    req.extensions_mut().insert(SessionId(sid.clone()));
    let mut resp = next.run(req).await;
    if is_new {
        if let Ok(v) = HeaderValue::from_str(&format!(
            "xoksa_sid={sid}; Path=/; HttpOnly; SameSite=Strict"
        )) {
            resp.headers_mut().append(header::SET_COOKIE, v);
        }
    }
    resp
}

/// True for hosts inside the OS trust boundary (loopback): `localhost`, `127.0.0.0/8`,
/// `::1`. Everything else (`0.0.0.0`, a LAN address) is non-loopback and
/// requires the serve auth token.
fn host_is_loopback(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// Constant-time byte comparison for equal-length inputs (avoids leaking how many
/// leading bytes of a guessed token matched).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Pull the `xoksa_auth` value out of a raw `Cookie` header.
fn parse_auth_cookie(cookie_header: &str) -> Option<String> {
    cookie_header
        .split(';')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| k.trim() == "xoksa_auth")
        .map(|(_, v)| v.trim().to_string())
}

/// A request is authenticated when it carries `Authorization: Bearer <token>` or an
/// `xoksa_auth` cookie equal to the serve token (constant-time compare).
fn request_has_valid_token(headers: &HeaderMap, expected: &Zeroizing<String>) -> bool {
    if let Some(bearer) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        if ct_eq(bearer.trim().as_bytes(), expected.as_bytes()) {
            return true;
        }
    }
    if let Some(cookie) = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_auth_cookie)
    {
        if ct_eq(cookie.as_bytes(), expected.as_bytes()) {
            return true;
        }
    }
    false
}

/// Auth gate. On a loopback bind the token is `None` and every request passes. On a
/// non-loopback bind, only the login flow (`GET /login`, `POST /auth`) and requests
/// carrying a valid token are allowed; an unauthenticated browser navigation is
/// redirected to `/login`, everything else gets `401`.
async fn require_auth(
    State(expected): State<Arc<Option<Zeroizing<String>>>>,
    req: Request,
    next: Next,
) -> Response {
    let Some(token) = expected.as_ref() else {
        return next.run(req).await;
    };
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let is_login_flow =
        (method == Method::GET && path == "/login") || (method == Method::POST && path == "/auth");
    if is_login_flow || request_has_valid_token(req.headers(), token) {
        return next.run(req).await;
    }
    let wants_html = req
        .headers()
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|a| a.contains("text/html"))
        .unwrap_or(false);
    if method == Method::GET && wants_html {
        return Redirect::to("/login").into_response();
    }
    (StatusCode::UNAUTHORIZED, "authentication required").into_response()
}

/// True when `ip` is private (RFC1918 / IPv6 unique-local / link-local) or loopback
/// — reachable only from the same machine or a private network, never routable on
/// the public internet.
fn is_private_or_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            if v6.is_loopback() {
                return true;
            }
            if let Some(v4) = v6.to_ipv4_mapped() {
                return v4.is_loopback() || v4.is_private() || v4.is_link_local();
            }
            let head = v6.segments()[0];
            (head & 0xfe00) == 0xfc00 // fc00::/7 unique local
                || (head & 0xffc0) == 0xfe80 // fe80::/10 link local
        }
    }
}

/// Source-address restriction: on a non-loopback bind, admit only private/loopback
/// peers, so the engine is never reachable directly from the public internet. A
/// no-op on a loopback bind (peers are loopback anyway). Runs before `require_auth`.
async fn restrict_to_private(
    State(expected): State<Arc<Option<Zeroizing<String>>>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    if expected.is_some() && !is_private_or_loopback(peer.ip()) {
        return (StatusCode::FORBIDDEN, "source address not permitted").into_response();
    }
    next.run(req).await
}

/// Minimal server-rendered login page (no inline script/style, so it renders under
/// the strict CSP). Its form POSTs the token to `/auth`.
async fn serve_login() -> Html<&'static str> {
    Html(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>XOKSA — Sign in</title></head><body>\
<h1>XOKSA</h1><p>This server requires an access token.</p>\
<form method=\"post\" action=\"/auth\">\
<label>Access token<br><input type=\"password\" name=\"token\" autocomplete=\"off\" autofocus></label>\
<p><button type=\"submit\">Sign in</button></p></form></body></html>",
    )
}

/// Form body for `POST /auth`.
#[derive(serde::Deserialize)]
struct AuthForm {
    token: String,
}

/// Validate the posted token; on success set the `xoksa_auth` cookie
/// (`HttpOnly`, `SameSite=Strict`) and redirect to the dashboard.
async fn handle_auth(
    Extension(expected): Extension<Arc<Option<Zeroizing<String>>>>,
    Form(form): Form<AuthForm>,
) -> Response {
    let submitted = Zeroizing::new(form.token);
    if let Some(token) = expected.as_ref() {
        if ct_eq(submitted.as_bytes(), token.as_bytes()) {
            let mut resp = Redirect::to("/").into_response();
            if let Ok(v) = HeaderValue::from_str(&format!(
                "xoksa_auth={}; Path=/; HttpOnly; SameSite=Strict",
                token.as_str()
            )) {
                resp.headers_mut().append(header::SET_COOKIE, v);
            }
            return resp;
        }
    }
    (StatusCode::UNAUTHORIZED, "invalid token").into_response()
}

/// CLI arguments for `xoksa serve`.
#[derive(Parser, Debug)]
#[command(
    name = "xoksa serve",
    about = "Run the XOKSA local Web UI / HTTP API server"
)]
pub struct ServeArgs {
    /// Serve the bundled Web UI (dashboard). Reserved as an explicit opt-in; the
    /// UI is served regardless for now, but the flag pins the intended UX.
    #[arg(long)]
    pub ui: bool,

    /// Port to listen on.
    #[arg(long, default_value_t = DEFAULT_SERVE_PORT)]
    pub port: u16,

    /// Host/interface to bind. Defaults to loopback (local-first).
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Directory containing the Web UI static assets. Defaults to the Leptos/WASM
    /// build output (`trunk build` in `webui-leptos/`).
    #[arg(long, default_value = "webui-leptos/dist")]
    pub web_dir: String,

    /// Private mode: do not save anything to disk (no log or saved-strategies file).
    /// Use this for a no-trace session.
    #[arg(long)]
    pub private: bool,

    /// Log output format: `text` (human-readable, default) or `json` (one JSON
    /// object per line / NDJSON, for log aggregation).
    #[arg(long, value_enum, default_value_t = LogFormat::Text)]
    pub log_format: LogFormat,

    /// Use this xoksa.env instead of the default (canonical app-data path). Same
    /// single-source resolution as the CLI.
    #[arg(long, value_name = "PATH")]
    pub env_file: Option<String>,

    /// Print the serve auth token and exit. The token gates a non-loopback bind;
    /// share it with remote clients (`Authorization: Bearer <token>`). §4.
    #[arg(long)]
    pub show_token: bool,

    /// Generate a fresh serve auth token (invalidating the old one) and exit.
    #[arg(long)]
    pub rotate_serve_token: bool,
}

/// Log line format for `--log-format`.
#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum LogFormat {
    Text,
    Json,
}

/// Entry point invoked from `main` when the first CLI argument is `serve`.
///
/// `rest` is the argument slice *after* the `serve` token. Parsed in isolation so
/// the existing analysis CLI (`config::Args`) is never touched.
pub async fn run_serve_cli(rest: &[String]) -> Result<()> {
    let serve_args = ServeArgs::parse_from(
        std::iter::once(String::from("xoksa-serve")).chain(rest.iter().cloned()),
    );
    run_server(&serve_args).await
}

/// Build the router and serve until the process is interrupted.
pub async fn run_server(args: &ServeArgs) -> Result<()> {
    // SOT: honor --env-file for `serve` too (idempotent — main() already set it
    // before dispatch; this also covers direct run_server callers/tests).
    crate::utils::init_env_path(args.env_file.as_deref());

    // Serve-token maintenance actions print and exit before any server setup.
    if args.show_token {
        match crate::keystore::get_serve_token()? {
            Some(t) => println!("SERVE_AUTH_TOKEN: {}", t.as_str()),
            None => println!(
                "SERVE_AUTH_TOKEN is not set yet (auto-generated on first non-loopback `serve`)."
            ),
        }
        return Ok(());
    }
    if args.rotate_serve_token {
        let t = crate::keystore::rotate_serve_token()?;
        println!("SERVE_AUTH_TOKEN rotated. New token: {}", t.as_str());
        return Ok(());
    }
    // Select the log format up front so every subsequent notice honors it.
    crate::logging::set_format(match args.log_format {
        LogFormat::Text => crate::logging::Format::Text,
        LogFormat::Json => crate::logging::Format::Json,
    });

    // The engine colors terminal output via the `colored` crate. In server mode the
    // analysis text is returned as JSON to the browser, where ANSI escape codes
    // would render as literal garbage (e.g. the price-change line). Force color OFF
    // process-wide so every rendered string (technical display, market line, …) is
    // plain text. The CLI is a separate invocation and keeps its terminal colors.
    colored::control::set_override(false);

    let web_dir = PathBuf::from(&args.web_dir);

    // `--private` is a no-trace session: nothing is written to disk (the log file
    // and the saved-strategies file both honor it).
    crate::private::set_private(args.private);
    if args.private {
        println!("Private mode: nothing is saved to disk.");
    }

    // Fail-closed for non-loopback: a bind off the OS trust boundary must be
    // authenticated. Loopback needs no auth (the OS account is the boundary), so
    // the desktop/CLI local UX is unchanged.
    let auth_token: Option<Zeroizing<String>> = if host_is_loopback(&args.host) {
        None
    } else if args.private {
        anyhow::bail!(
            "Refusing to bind non-loopback host '{}' in --private mode: an auth token cannot be persisted. Use loopback, or drop --private.",
            args.host
        );
    } else {
        let (token, newly) = crate::keystore::ensure_serve_token()?;
        if newly {
            println!("🔑 Generated SERVE_AUTH_TOKEN for non-loopback access:");
            println!("      {}", token.as_str());
            println!("      Clients authenticate with:  Authorization: Bearer <token>");
            println!("      Re-show: `xoksa serve --show-token`   Rotate: `xoksa serve --rotate-serve-token`");
        } else {
            println!(
                "🔒 Non-loopback bind: auth required. Show token with `xoksa serve --show-token`."
            );
        }
        Some(token)
    };
    let auth_state = Arc::new(auth_token);

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/api/health", get(api::health))
        .route("/api/config", get(api::config))
        // `{symbol}`, not `:symbol` — axum 0.8 changed the path-parameter syntax
        // and the old spelling panics when the router is built, which neither the
        // compiler nor the test suite catches (no test starts the server).
        .route("/api/symbol/{symbol}/summary", get(api::symbol_summary))
        .route("/api/symbol/{symbol}/news", get(api::symbol_news))
        .route("/api/symbol/{symbol}/chart", get(api::symbol_chart))
        .route(
            "/api/analysis/multi-timeframe",
            post(api::analysis_multi_timeframe),
        )
        .route("/api/llm/options", get(api::llm_options))
        .route("/api/llm/select", post(api::llm_select))
        .route("/api/config/lang", post(api::set_lang))
        .route("/api/backtest", post(api::backtest))
        .route(
            "/api/backtest/rules",
            get(api::list_rules).post(api::save_rule),
        )
        .route("/api/alerts", get(api::alerts_list).post(api::alerts_add))
        .route("/api/alerts/delete", post(api::alerts_delete))
        .route("/api/alerts/toggle", post(api::alerts_toggle))
        .route("/api/alerts/test", post(api::alerts_test))
        .route("/api/chat/stream", get(api::chat_stream))
        .route("/api/chat/commands", get(api::chat_commands))
        .route("/login", get(serve_login))
        .route("/auth", post(handle_auth))
        .fallback(serve_static)
        // Cap request bodies (DoS): the JSON handlers never need more than this.
        .layer(DefaultBodyLimit::max(64 * 1024))
        // Server-side CSRF check on state-changing POSTs (defence-in-depth).
        .layer(middleware::from_fn(reject_cross_origin_mutations))
        // Per-client session cookie → request extensions (isolates chat sessions).
        .layer(middleware::from_fn(ensure_session_cookie))
        .layer(middleware::from_fn(security_headers))
        // Auth gate for a non-loopback bind (a no-op on loopback). Runs after
        // admission and before the rest; the `Extension` also hands the token to
        // the `/auth` handler.
        .layer(Extension(auth_state.clone()))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            require_auth,
        ))
        // Reject non-private/non-loopback source addresses on a non-loopback bind
        // (runs before auth, no-op on loopback).
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            restrict_to_private,
        ))
        // Outermost: admit (or shed with 503) before any other work is done.
        .layer(middleware::from_fn(limit_concurrency))
        .with_state(web_dir.clone());

    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    println!(
        "XOKSA Web UI is running at http://{}:{}",
        args.host, args.port
    );
    match crate::logging::current_log_path() {
        Some(path) => println!("Diagnostics log: {path}"),
        None => println!("Diagnostics log: disabled (private mode)"),
    }
    if !web_dir.join("index.html").exists() && read_embedded("index.html").is_none() {
        eprintln!(
            "⚠️  Web UI assets not found at '{}'. Run from the repository root, or pass --web-dir <path>.",
            web_dir.display()
        );
    }

    // Start the chat-notification alert monitor (a no-op when no ALERT_<n>_* rules
    // are defined). Runs as background tasks for the lifetime of the server.
    monitor::spawn_alert_monitor();

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .context("HTTP server error")?;
    Ok(())
}

// ── static asset serving ─────────────────────────────────────────────────────
//
// Static asset serving: `read_asset` tries `--web-dir` on disk first, then the
// assets embedded in the binary (the `embedded-ui` feature). Both paths are
// path-traversal-checked.

/// The configured UI language (`LANG` in `xoksa.env`) — `ja` only when set to
/// `ja`, English otherwise. One resolver, read by the page stamp and the JSON
/// config alike.
fn configured_lang() -> &'static str {
    match crate::bootstrap::load_env_map()
        .get("LANG")
        .map(|v| v.trim().to_ascii_lowercase())
    {
        Some(v) if v == "ja" => "ja",
        _ => "en",
    }
}

async fn serve_index(
    Extension(CspNonce(nonce)): Extension<CspNonce>,
    axum::extract::State(web_dir): axum::extract::State<PathBuf>,
) -> Response {
    match read_asset(&web_dir, "index.html") {
        Some((bytes, content_type)) => {
            // Stamp the per-request CSP nonce onto trunk's WASM-bootstrap module
            // script so it runs under `script-src 'nonce-…'` (no 'unsafe-inline').
            let html = String::from_utf8_lossy(&bytes).replacen(
                "<script type=\"module\">",
                &format!("<script type=\"module\" nonce=\"{nonce}\">"),
                1,
            );
            // Hand the dashboard the configured language (`LANG`) with the page
            // itself, so the UI renders in it from the first paint and no surface
            // has to keep its own copy of the setting (SOT).
            let lang = configured_lang();
            let private = if crate::private::is_private() {
                "1"
            } else {
                "0"
            };
            let html = html.replacen(
                "<head>",
                &format!(
                    "<head><meta name=\"xoksa-lang\" content=\"{lang}\">\
                     <meta name=\"xoksa-private\" content=\"{private}\">"
                ),
                1,
            );
            // The document's own language attribute must agree with that stamp.
            // `index.html` can only carry a static value (trunk builds it once), so
            // it ships `ja` and an English session was served a page that still
            // declared Japanese — which is what a screen reader announces and what
            // the browser's translate prompt reads, neither of them our meta tag.
            // The language selector reloads the page after saving, so stamping here
            // is enough; nothing has to touch the attribute at runtime.
            let html = html.replacen("<html lang=\"ja\"", &format!("<html lang=\"{lang}\""), 1);
            // The HTML entry point must never be cached (see respond_with_asset).
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, content_type),
                    (header::CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
                ],
                Body::from(html),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
    }
}

async fn serve_static(
    axum::extract::State(web_dir): axum::extract::State<PathBuf>,
    uri: Uri,
) -> Response {
    let rel = uri.path().trim_start_matches('/');
    respond_with_asset(&web_dir, rel)
}

fn respond_with_asset(web_dir: &Path, rel_path: &str) -> Response {
    match read_asset(web_dir, rel_path) {
        Some((bytes, content_type)) => {
            // Never let the HTML entry point go stale: it references the hashed
            // asset filenames, so it MUST always be re-fetched — otherwise a
            // rebuilt UI would keep loading the old .wasm and mislead the user.
            // Hashed assets (js/wasm/css: name-<hash>.ext) are immutable: a
            // rebuild changes the filename, so caching them forever is safe.
            let cache_control = if content_type.starts_with("text/html") {
                "no-store, no-cache, must-revalidate"
            } else {
                "public, max-age=31536000, immutable"
            };
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, content_type),
                    (header::CACHE_CONTROL, cache_control),
                ],
                Body::from(bytes),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
    }
}

/// Read a static asset, guarding against path traversal. Returns the bytes and a
/// content-type guessed from the extension, or `None` if not found / unsafe.
fn read_asset(web_dir: &Path, rel_path: &str) -> Option<(Vec<u8>, &'static str)> {
    let rel = if rel_path.is_empty() {
        "index.html"
    } else {
        rel_path
    };

    // Reject absolute paths and any `..` / prefix / root components.
    let candidate = Path::new(rel);
    if candidate
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return None;
    }

    // On-disk assets take precedence — dev builds and an explicit `--web-dir`.
    let full = web_dir.join(candidate);
    if let Ok(bytes) = std::fs::read(&full) {
        return Some((bytes, content_type_for(&full)));
    }

    // Fall back to assets embedded in the binary (release builds with the
    // `embedded-ui` feature). `rel` is already path-traversal-checked above.
    read_embedded(rel)
}

/// Assets baked into the binary at build time from `webui-leptos/dist` (the
/// `embedded-ui` feature) — this is what makes a distributed binary
/// self-contained, with no external `dist/` needed. `rel` must already be
/// path-traversal-checked by the caller.
#[cfg(feature = "embedded-ui")]
fn read_embedded(rel: &str) -> Option<(Vec<u8>, &'static str)> {
    static EMBEDDED_UI: include_dir::Dir<'static> =
        include_dir::include_dir!("$CARGO_MANIFEST_DIR/webui-leptos/dist");
    let file = EMBEDDED_UI.get_file(rel)?;
    Some((file.contents().to_vec(), content_type_for(Path::new(rel))))
}

/// No embedded assets in a plain build — the UI is served from `--web-dir` on disk.
#[cfg(not(feature = "embedded-ui"))]
fn read_embedded(_rel: &str) -> Option<(Vec<u8>, &'static str)> {
    None
}

fn content_type_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::read_asset;
    use std::path::PathBuf;

    #[test]
    fn read_asset_rejects_path_traversal() {
        let dir = PathBuf::from("webui");
        // `..` segments must never escape the asset root (rejected before any I/O).
        assert!(read_asset(&dir, "../Cargo.toml").is_none());
        assert!(read_asset(&dir, "../../src/main.rs").is_none());
        assert!(read_asset(&dir, "a/../../b").is_none());
    }

    #[test]
    fn read_asset_rejects_absolute_path() {
        let dir = PathBuf::from("webui");
        // A leading root component must not be honored as an absolute filesystem path.
        assert!(read_asset(&dir, "/etc/passwd").is_none());
    }

    #[test]
    fn parses_log_format() {
        use super::{LogFormat, ServeArgs};
        use clap::Parser;
        // Default is text.
        let a = ServeArgs::parse_from(["xoksa-serve"]);
        assert!(matches!(a.log_format, LogFormat::Text));
        // `--log-format json` selects JSON.
        let a = ServeArgs::parse_from(["xoksa-serve", "--log-format", "json"]);
        assert!(matches!(a.log_format, LogFormat::Json));
        // Unknown value is rejected.
        assert!(ServeArgs::try_parse_from(["xoksa-serve", "--log-format", "xml"]).is_err());
    }

    #[test]
    fn host_is_loopback_classifies_hosts() {
        use super::host_is_loopback;
        assert!(host_is_loopback("127.0.0.1"));
        assert!(host_is_loopback("::1"));
        assert!(host_is_loopback("localhost"));
        assert!(host_is_loopback("LocalHost"));
        assert!(host_is_loopback("127.0.0.5"));
        assert!(!host_is_loopback("0.0.0.0"));
        assert!(!host_is_loopback("192.168.1.10"));
        assert!(!host_is_loopback("100.64.0.1")); // a CGNAT-range (non-loopback) address
    }

    #[test]
    fn ct_eq_matches_only_identical_bytes() {
        use super::ct_eq;
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab")); // differing length
        assert!(ct_eq(b"", b""));
    }

    #[test]
    fn parse_auth_cookie_extracts_token() {
        use super::parse_auth_cookie;
        assert_eq!(
            parse_auth_cookie("xoksa_auth=abc123").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            parse_auth_cookie("xoksa_sid=zzz; xoksa_auth=tok").as_deref(),
            Some("tok")
        );
        assert_eq!(parse_auth_cookie("xoksa_sid=zzz"), None);
    }

    #[test]
    fn request_has_valid_token_accepts_bearer_and_cookie() {
        use super::request_has_valid_token;
        use axum::http::{header, HeaderMap, HeaderValue};
        use zeroize::Zeroizing;
        let token = Zeroizing::new("secrettoken".to_string());

        let mut bearer = HeaderMap::new();
        bearer.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secrettoken"),
        );
        assert!(request_has_valid_token(&bearer, &token));

        let mut cookie = HeaderMap::new();
        cookie.insert(
            header::COOKIE,
            HeaderValue::from_static("xoksa_auth=secrettoken"),
        );
        assert!(request_has_valid_token(&cookie, &token));

        let mut wrong = HeaderMap::new();
        wrong.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer nope"),
        );
        assert!(!request_has_valid_token(&wrong, &token));

        assert!(!request_has_valid_token(&HeaderMap::new(), &token));
    }

    #[test]
    fn parses_token_flags() {
        use super::ServeArgs;
        use clap::Parser;
        assert!(ServeArgs::parse_from(["xoksa-serve", "--show-token"]).show_token);
        assert!(ServeArgs::parse_from(["xoksa-serve", "--rotate-serve-token"]).rotate_serve_token);
        let none = ServeArgs::parse_from(["xoksa-serve"]);
        assert!(!none.show_token && !none.rotate_serve_token);
    }
}
