#![forbid(unsafe_code)]
// XOKSA desktop app — a thin UI shell (Tauri) around a local or remote engine.
//
// It shows a small CONNECTION screen (which engine to talk to: host:port + an
// access token), then points the WebView at that engine's dashboard. When "start a
// local engine automatically" is on, it launches the `xoksa` engine binary shipped
// alongside it as a CHILD PROCESS (`xoksa serve --ui`) on loopback; otherwise it
// connects to the address entered (a remote engine reached over the network). The
// desktop itself runs no analysis and links no engine code. Configuration (API
// keys, indicators, notify, the access token, the settings password) lives in the
// SEPARATE settings app (`xoksa-setup`), which the "Settings" button/menu launches;
// keys never touch argv or the HTTP server. The CLI and `serve --ui` are
// unaffected. See docs/dev-prog/security-design.md §6.
// GUI app: no console window (even in debug — a stray console window just looks broken).
#![windows_subsystem = "windows"]

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager};

// Desktop-only top inset. A browser gives the shared web UI chrome (tab/URL bar)
// above its top toolbar; the bare window does not, so without this the toolbar
// sits flush against the native title bar. Applied via the DOM `style` property
// (not an inline <style>), so it is not blocked by the server's strict CSP, the
// browser UI is untouched, and no dist rebuild is needed. The percentage-based
// layout (calc(100% - header)) absorbs it with no scrollbar.
const TOP_INSET_JS: &str = "document.body.style.paddingTop='16px';";

const HOST: &str = "127.0.0.1";

// Read the canonical `xoksa.env` (`xoksa_paths::env_file` — §6's single location).
//
// The app chdirs into the config dir at startup, so a bare `"xoksa.env"` lands on
// the same file today. That is an implicit dependency on the cwd, which is exactly
// the shape §6 rules out ("no implicit ./xoksa.env fallback"): the chdir result is
// discarded, so a failed one would leave a relative read pointing at whatever
// directory the process was launched from — silently answering "no LAN, default
// port" from a file that is not the user's. Resolving the path explicitly keeps
// the answer tied to the one file the engine, the CLI and this app share.
fn read_canonical_env() -> Option<String> {
    std::fs::read_to_string(xoksa_paths::env_file()?).ok()
}

// Bind address for the in-process server. Loopback by default; `0.0.0.0` (LAN)
// only when the user explicitly enabled it in the setup form (DESKTOP_LAN_ACCESS).
// The WebView always connects to 127.0.0.1 (HOST) — a 0.0.0.0 bind still accepts
// loopback — so LAN exposure is opt-in without changing the local connection.
fn bind_host() -> String {
    let lan = read_canonical_env()
        .map(|s| {
            s.lines()
                .any(|l| l.trim().eq_ignore_ascii_case("DESKTOP_LAN_ACCESS=true"))
        })
        .unwrap_or(false);
    if lan {
        "0.0.0.0".to_string()
    } else {
        HOST.to_string()
    }
}

// The running engine child process, so it can be terminated when the app exits
// (a spawned process does not die with its parent on macOS/Linux).
struct EngineProcess(Mutex<Option<std::process::Child>>);

// Path to the `xoksa` engine binary. Shipped alongside the desktop app, so it
// sits next to this executable (`XOKSA.app/Contents/MacOS/xoksa` on macOS). Falls
// back to the bare name (PATH lookup) for `cargo run` during development.
fn engine_bin_path() -> std::path::PathBuf {
    let name = if cfg!(windows) { "xoksa.exe" } else { "xoksa" };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(name);
            if sibling.exists() {
                return sibling;
            }
        }
    }
    std::path::PathBuf::from(name)
}

// Build a Command for the engine binary. On Windows the engine is a console
// program, so a GUI app spawning it pops a black console window unless
// CREATE_NO_WINDOW is set. This helper suppresses that window for every engine
// child (serve / config-json / conn-token); no-op elsewhere.
fn engine_command() -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(engine_bin_path());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

// Path to the standalone settings app (xoksa-setup), shipped beside this desktop
// executable. Falls back to the bare name (PATH) for `cargo run` in development.
fn setup_bin_path() -> std::path::PathBuf {
    let name = if cfg!(windows) {
        "xoksa-setup.exe"
    } else {
        "xoksa-setup"
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(name);
            if sibling.exists() {
                return sibling;
            }
        }
    }
    std::path::PathBuf::from(name)
}

// Launch the standalone settings app as its own process/window. Config + keychain
// live in that app (via the engine); the desktop only opens it. No shell — the
// path is an explicit argv[0], so nothing can be injected.
fn launch_setup_app() {
    if let Err(e) = std::process::Command::new(setup_bin_path()).spawn() {
        eprintln!("XOKSA: failed to launch the settings app: {e}");
    }
}

// ── Desktop connection config (which engine this UI connects to) ─────────────
// The non-secret target lives in a small desktop-owned file (NOT xoksa.env, which
// the engine rewrites wholesale on every settings save); the access token is a
// secret and lives in the OS keychain (via the engine's `conn-token` subcommand).

// serde default for `auto_start`: on by default (the common case is a local engine).
fn d_true() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
struct DesktopConn {
    #[serde(default)]
    host: String,
    #[serde(default)]
    port: String,
    #[serde(default = "d_true")]
    auto_start: bool,
}

fn desktop_conn_path() -> Option<std::path::PathBuf> {
    xoksa_paths::config_dir().map(|d| d.join("desktop.json"))
}

fn load_desktop_conn() -> DesktopConn {
    let raw = desktop_conn_path().and_then(|p| std::fs::read_to_string(p).ok());
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(DesktopConn {
            host: String::new(),
            port: String::new(),
            auto_start: true,
        })
}

fn save_desktop_conn(c: &DesktopConn) -> Result<(), String> {
    let p = desktop_conn_path().ok_or_else(|| "no config dir".to_string())?;
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let json = serde_json::to_string_pretty(c).map_err(|e| e.to_string())?;
    std::fs::write(&p, json).map_err(|e| e.to_string())
}

// The engine host:port the WebView is currently pointed at — used to build popup /
// manual URLs to the SAME engine. Loopback for a local auto-started engine; the
// entered address for a remote one.
fn current_target() -> (String, String) {
    let c = load_desktop_conn();
    let host = if c.host.trim().is_empty() {
        HOST.to_string()
    } else {
        c.host.trim().to_string()
    };
    let port = if c.port.trim().is_empty() {
        "8787".to_string()
    } else {
        c.port.trim().to_string()
    };
    (host, port)
}

// The connection token (secret), held so the page-load auth hook can present it.
struct ConnToken(Mutex<String>);

// Read the stored connection token from the keychain via the engine subcommand.
fn read_conn_token() -> String {
    engine_command()
        .arg("conn-token")
        .arg("--show")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout).lines().find_map(|l| {
                l.strip_prefix("DESKTOP_CONN_TOKEN: ")
                    .map(|t| t.trim().to_string())
            })
        })
        .unwrap_or_default()
}

// Store the connection token in the keychain via the engine subcommand (stdin).
fn write_conn_token(token: &str) -> Result<(), String> {
    use std::io::Write;
    let mut child = engine_command()
        .arg("conn-token")
        .arg("--set")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to launch engine: {e}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "engine stdin unavailable".to_string())
        .and_then(|mut s| s.write_all(token.as_bytes()).map_err(|e| e.to_string()))?;
    let st = child.wait().map_err(|e| e.to_string())?;
    if st.success() {
        Ok(())
    } else {
        Err("failed to store the connection token".to_string())
    }
}

// Spawn the local engine (bind per DESKTOP_LAN_ACCESS) on `port`, tracked so it is
// stopped on app exit. Any previously tracked child is replaced.
fn spawn_local_engine(port: &str, state: &tauri::State<EngineProcess>) -> Result<(), String> {
    let child = engine_command()
        .arg("serve")
        .arg("--ui")
        .arg("--host")
        .arg(bind_host())
        .arg("--port")
        .arg(port)
        .spawn()
        .map_err(|e| format!("err_spawn|{e}"))?;
    if let Ok(mut g) = state.0.lock() {
        if let Some(mut old) = g.take() {
            let _ = old.kill();
        }
        *g = Some(child);
    }
    Ok(())
}

// What answered a health probe. Distinguishing these is what lets a connection
// failure name its cause (401 vs nothing listening vs a foreign process).
enum Probe {
    Ready,              // our engine: xoksa-web reporting this app's version
    OtherXoksa(String), // an xoksa engine, but a different version (a stale serve)
    AuthRequired,       // HTTP 401 — the engine wants a Bearer token
    Foreign,            // something answered, but it is not a xoksa engine
    Down,               // nothing accepted the TCP connection
}

// Health probe to host:port over a raw TCP request (Bearer when a token is given).
// Ready requires the reported version to equal this app's version — the engine
// ships beside the desktop as a matched pair, so a version mismatch means a
// stale or foreign engine, never ours (the v2.6.4 identity check, restored).
fn probe_engine(host: &str, port: &str, token: &str) -> Probe {
    use std::io::{Read, Write};
    let Ok(p) = port.parse::<u16>() else {
        return Probe::Down;
    };
    let Ok(mut s) = std::net::TcpStream::connect((host, p)) else {
        return Probe::Down;
    };
    let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(600)));
    let auth = if token.is_empty() {
        String::new()
    } else {
        format!("Authorization: Bearer {token}\r\n")
    };
    let req =
        format!("GET /api/health HTTP/1.0\r\nHost: {host}:{p}\r\n{auth}Connection: close\r\n\r\n");
    if s.write_all(req.as_bytes()).is_err() {
        return Probe::Down;
    }
    let mut buf = String::new();
    if s.read_to_string(&mut buf).is_err() {
        return Probe::Down;
    }
    if buf.split_whitespace().nth(1) == Some("401") {
        return Probe::AuthRequired;
    }
    let Some(body) = buf.split("\r\n\r\n").nth(1) else {
        return Probe::Foreign;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return Probe::Foreign;
    };
    if v.get("service").and_then(|x| x.as_str()) != Some("xoksa-web") {
        return Probe::Foreign;
    }
    match v.get("version").and_then(|x| x.as_str()) {
        Some(env!("CARGO_PKG_VERSION")) => Probe::Ready,
        Some(ver) => Probe::OtherXoksa(ver.to_string()),
        None => Probe::Foreign,
    }
}

// A free loopback port from the OS (bind :0, read the assigned port, release it).
// Restores the v2.6.4 approach for the auto-started local engine: it never
// contends with a stale serve or a foreign process on a fixed port. Chosen per
// connection attempt, and never persisted (the saved host:port is the form's).
fn pick_local_port() -> Result<u16, String> {
    std::net::TcpListener::bind((HOST, 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|_| "err_no_port".to_string())
}

// Prefill for the connection screen (values + whether a token is stored).
#[tauri::command]
fn load_connection() -> serde_json::Value {
    let c = load_desktop_conn();
    serde_json::json!({
        "host": if c.host.trim().is_empty() { HOST } else { c.host.trim() },
        "port": if c.port.trim().is_empty() { "8787" } else { c.port.trim() },
        "auto_start": c.auto_start,
        "token_set": !read_conn_token().is_empty(),
        // The configured language (`LANG`), asked of the engine — the same source
        // the dashboard, the settings app, and the CLI read. Empty when the engine
        // cannot answer (first run, no config yet); the screen then falls back to
        // the OS locale.
        "lang": engine_lang(),
    })
}

// Open the standalone settings app from the connection screen.
#[tauri::command]
fn open_settings_app() {
    launch_setup_app();
}

// Store the connection, start a local engine if requested, and return the URL the
// WebView should navigate to. A blank token keeps the stored one; a token-gated
// engine is authenticated by the page-load hook (which presents this token).
#[tauri::command]
fn connect(
    host: String,
    port: String,
    token: String,
    auto_start: bool,
    state: tauri::State<EngineProcess>,
    tok_state: tauri::State<ConnToken>,
) -> Result<String, String> {
    let host = if host.trim().is_empty() {
        HOST.to_string()
    } else {
        host.trim().to_string()
    };
    let port = if port.trim().is_empty() {
        "8787".to_string()
    } else {
        port.trim().to_string()
    };
    save_desktop_conn(&DesktopConn {
        host: host.clone(),
        port: port.clone(),
        auto_start,
    })?;
    if !token.is_empty() {
        write_conn_token(&token)?;
    }
    let effective_token = if token.is_empty() {
        read_conn_token()
    } else {
        token
    };
    if let Ok(mut g) = tok_state.0.lock() {
        *g = effective_token.clone();
    }
    if auto_start {
        // Loopback (the default): an ephemeral port — no contention, no stale-
        // engine adoption. LAN mode: the fixed, user-known port other devices
        // rely on, so refuse up front — naming the occupant — if it is taken.
        let engine_port = if bind_host() == "0.0.0.0" {
            match probe_engine(HOST, &port, &effective_token) {
                Probe::Down => {}
                Probe::AuthRequired => return Err(format!("err_busy_auth|{port}")),
                Probe::Ready => {
                    return Err(format!(
                        "err_busy_engine|{port}|{}",
                        env!("CARGO_PKG_VERSION")
                    ))
                }
                Probe::OtherXoksa(v) => return Err(format!("err_busy_engine|{port}|{v}")),
                Probe::Foreign => return Err(format!("err_busy_other|{port}")),
            }
            port.clone()
        } else {
            pick_local_port()?.to_string()
        };
        spawn_local_engine(&engine_port, &state)?;
        let mut last = String::from("no response");
        for _ in 0..100 {
            // A child that died at startup (bind failure, broken config) would
            // otherwise read as a silent timeout — report its exit instead.
            if let Ok(mut g) = state.0.lock() {
                if let Some(child) = g.as_mut() {
                    if let Ok(Some(st)) = child.try_wait() {
                        return Err(format!("err_exit|{st}"));
                    }
                }
            }
            match probe_engine(HOST, &engine_port, &effective_token) {
                Probe::Ready => return Ok(format!("http://{HOST}:{engine_port}")),
                Probe::AuthRequired => return Err("err_auth".to_string()),
                Probe::OtherXoksa(v) => last = format!("different xoksa version v{v}"),
                Probe::Foreign => last = String::from("not a xoksa engine"),
                Probe::Down => last = String::from("no response"),
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        return Err(format!("err_timeout|{last}"));
    }
    Ok(format!("http://{host}:{port}"))
}

// Ask the engine (single source) whether the configured language is Japanese —
// used only to localize the native menu at startup. English on any failure.
/// The configured UI language (`LANG`), read from the engine — `"ja"` / `"en"`,
/// or empty when the engine cannot be asked. Single source; the desktop keeps no
/// language setting of its own.
fn engine_lang() -> String {
    engine_command()
        .arg("config-json")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
        .and_then(|v| v.get("lang").and_then(|l| l.as_str()).map(str::to_string))
        .unwrap_or_default()
}

fn engine_lang_is_ja() -> bool {
    engine_command()
        .arg("config-json")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
        .and_then(|v| {
            v.get("lang")
                .and_then(|l| l.as_str())
                .map(|l| l.eq_ignore_ascii_case("ja"))
        })
        .unwrap_or(false)
}

// Open a dashboard "popup" (chart / help) as a real native window — the Web UI's
// `window.open()` popups don't work in a WebView, so on the desktop the button
// asks Rust to open a Tauri window pointed at the same server in popup mode.
// MUST be `async`. A synchronous #[tauri::command] runs on the main/UI thread that
// owns the event loop and the WebView2 COM message pump. On Windows,
// `WebviewWindowBuilder::build()` there deadlocks: it needs that thread's message
// loop to pump WebView2's CreateCoreWebView2Controller completion callback, but the
// sync command is occupying the thread, so the controller never initializes and the
// popup's first navigation aborts (net::ERR_ABORTED) — a blank window that never
// gets `__TAURI__`. `async` dispatches the command to the async runtime, freeing the
// event loop to pump that callback, so the popup loads the loopback URL and
// self-renders. macOS/WKWebView is not affected, so ONE path serves both platforms.
// (docs.rs WebviewWindowBuilder: build() "deadlocks when used in a synchronous
// command … use async commands"; wry#583, tauri#3597.) Do NOT revert to `fn`, and do
// NOT wrap build() in `run_on_main_thread` — both reintroduce the deadlock.
#[tauri::command]
async fn open_popup_window(
    app: AppHandle,
    kind: String,
    symbol: String,
    tf: String,
    lang: String,
) -> Result<(), String> {
    let safe: String = kind.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let label = format!("popup-{safe}");
    let (w, h) = if safe == "chart" {
        (900.0, 620.0)
    } else {
        (560.0, 760.0)
    };
    let title = if safe == "chart" {
        "XOKSA — Chart"
    } else {
        "XOKSA — Help"
    };
    // Percent-encode so index symbols (e.g. `^N225`) / reserved chars don't break parse.
    let (chost, cport) = current_target();
    let mut url =
        tauri::Url::parse(&format!("http://{chost}:{cport}/")).map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("popup", &safe)
        .append_pair("symbol", &symbol)
        .append_pair("tf", &tf)
        .append_pair("lang", &lang);
    // Reuse an already-open popup: re-point it and focus.
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.navigate(url);
        let _ = win.set_focus();
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::External(url))
        .title(title)
        .inner_size(w, h)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

// Open a web URL in the OS default browser. The dashboard WebView cannot follow
// `target="_blank"`/`window.open`, so news and other external links are routed here.
// Restricted to http(s) so no `file://` or custom-scheme URL can be launched, and
// the URL is passed as a single process argument (no shell) to avoid injection.
#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("only http(s) URLs may be opened".into());
    }
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("open").arg(&url).spawn();
    // Windows: NOT `cmd /C start` — cmd treats `&` `|` `^` etc. as shell
    // metacharacters even in an argument (and Rust does not quote a space-less
    // URL), so a URL query string like `?a=1&b=2` would break or inject a
    // command. `rundll32 url.dll,FileProtocolHandler <url>` opens the URL in the
    // default browser with no shell parsing (the URL is a single CreateProcess arg).
    #[cfg(target_os = "windows")]
    let spawned = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", &url])
        .spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let spawned = std::process::Command::new("xdg-open").arg(&url).spawn();
    spawned.map(|_| ()).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_connection,
            connect,
            open_settings_app,
            open_popup_window,
            open_external
        ])
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Finished {
                let _ = webview.eval(TOP_INSET_JS);
                // Auto-authenticate a token-gated engine: on the login page, submit
                // the stored connection token to /auth (sets the cookie), then load
                // the dashboard. A loopback engine never shows /login → no-op.
                if payload.url().path() == "/login" {
                    if let Some(tok) = webview.app_handle().try_state::<ConnToken>() {
                        let token = tok.0.lock().map(|g| g.clone()).unwrap_or_default();
                        if !token.is_empty() {
                            let js = format!(
                                "fetch('/auth',{{method:'POST',headers:{{'Content-Type':'application/x-www-form-urlencoded'}},body:'token='+encodeURIComponent('{}')}}).then(function(){{location.replace('/');}});",
                                token
                            );
                            let _ = webview.eval(&js);
                        }
                    }
                }
            }
        })
        .setup(|app| {
            // Stable working directory: launched from Finder/Explorer the process
            // cwd is not the project dir. Use the SINGLE canonical config dir from
            // the shared `xoksa-paths` crate — the SAME definition the engine uses
            // (xoksa_paths::env_file), so the desktop, its engine child, the CLI and
            // serve all share one xoksa.env and one logs dir (SOT — §6). One source,
            // no drift.
            if let Some(dir) = xoksa_paths::config_dir() {
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::env::set_current_dir(&dir);
            }
            // One-time import of a pre-2.7.1 desktop config (Tauri app_data_dir) or a
            // working-dir xoksa.env into the canonical location, BEFORE any config
            // read below, so an upgrading user keeps their non-secret settings (LLM,
            // indicators, notify, alerts, DESKTOP_LAN_ACCESS, alias CSV) instead of
            // losing them. Non-destructive; never overwrites.
            // Surface a failure (not swallowed) so lost-looking settings are
            // explained rather than silent.
            if let Err(e) = xoksa_paths::migrate_env_if_needed() {
                eprintln!(
                    "XOKSA: config migration failed ({e}); existing settings may not be applied — check permissions on the config directory"
                );
            }
            // No engine is started here: the frontend shows the connection screen,
            // and the `connect` command starts a local engine (or attaches to a
            // remote one) on the user's action. Track the (initially none) engine
            // child so it is stopped on exit, and hold the connection token so the
            // page-load hook can authenticate a token-gated engine.
            app.manage(EngineProcess(Mutex::new(None)));
            app.manage(ConnToken(Mutex::new(String::new())));
            // Native menu: Settings (⌘,) and Restart under the app menu, plus a
            // standard Edit menu so text fields (e.g. pasting a token) have
            // copy/paste. "Settings" launches the standalone settings app;
            // "Restart" relaunches to reconnect cleanly.
            let handle = app.handle().clone();
            // Localize the native menu to the configured language (ja/en). Built at
            // startup; a language change takes effect on the next launch.
            let ja = engine_lang_is_ja();
            let t = |ja_s: &'static str, en_s: &'static str| if ja { ja_s } else { en_s };
            let settings_item = MenuItemBuilder::with_id("settings", t("設定…", "Settings…"))
                .accelerator("CmdOrCtrl+,")
                .build(&handle)?;
            let restart_item =
                MenuItemBuilder::with_id("restart", t("再起動", "Restart")).build(&handle)?;
            let app_menu = SubmenuBuilder::new(&handle, "XOKSA")
                .item(&settings_item)
                .item(&restart_item)
                .separator()
                .item(&PredefinedMenuItem::quit(
                    &handle,
                    Some(t("XOKSA を終了", "Quit XOKSA")),
                )?)
                .build()?;
            let edit_menu = SubmenuBuilder::new(&handle, t("編集", "Edit"))
                .item(&PredefinedMenuItem::undo(
                    &handle,
                    Some(t("取り消す", "Undo")),
                )?)
                .item(&PredefinedMenuItem::redo(
                    &handle,
                    Some(t("やり直す", "Redo")),
                )?)
                .separator()
                .item(&PredefinedMenuItem::cut(&handle, Some(t("カット", "Cut")))?)
                .item(&PredefinedMenuItem::copy(
                    &handle,
                    Some(t("コピー", "Copy")),
                )?)
                .item(&PredefinedMenuItem::paste(
                    &handle,
                    Some(t("ペースト", "Paste")),
                )?)
                .build()?;
            let menu = MenuBuilder::new(&handle)
                .item(&app_menu)
                .item(&edit_menu)
                .build()?;
            app.set_menu(menu)?;
            app.on_menu_event(|app, event| match event.id().as_ref() {
                "settings" => {
                    launch_setup_app();
                }
                "restart" => {
                    app.restart();
                }
                _ => {}
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the XOKSA desktop app")
        .run(|app, event| {
            // Terminate the engine child process when the app exits, so it does
            // not linger holding the loopback port.
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<EngineProcess>() {
                    if let Ok(mut guard) = state.0.lock() {
                        if let Some(mut child) = guard.take() {
                            let _ = child.kill();
                        }
                    }
                }
            }
        });
}
