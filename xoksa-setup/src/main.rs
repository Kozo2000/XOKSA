#![forbid(unsafe_code)]
// XOKSA settings app — a standalone Tauri app for engine setup.
//
// It drives the `xoksa` engine binary as a CHILD PROCESS; it links no
// engine/analysis code (SOT — config + keychain live in the engine). It:
//   * gates on a password (no ID; default `XOKSA_password`, user-changeable),
//   * reads/writes config + keys via `config-json` / `apply-config` (keys over
//     stdin → OS keychain, never argv/HTTP),
//   * generates / regenerates the serve access token
//     (`serve --show-token` / `serve --rotate-serve-token`),
//   * restarts the engine (`serve`) — one button that starts it if nothing runs;
//     spawned DETACHED so it persists for the UIs / remote to connect.
// The token is copied by the user into each UI (desktop IP+token field, browser
// /login). The engine listens on a known port (SERVE_PORT, default 8787), set on
// the Connection card. See docs/dev-prog/security-design.md §6.
// GUI app: no console window (even in debug — the settings app is not run from a
// terminal, so a stray console window would just look broken).
#![windows_subsystem = "windows"]

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::process::Stdio;
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use zeroize::Zeroize;

const DEFAULT_PORT: u16 = 8787;
const LOOPBACK: &str = "127.0.0.1";

// The running engine child. Spawned detached — NOT killed on app exit, so the
// engine persists for the UIs / remote to connect (design decision ②). Tracked
// only so a restart WITHIN this session can stop it before respawning.
struct EngineProcess(Mutex<Option<std::process::Child>>);

// Path to the `xoksa` engine binary, shipped alongside this app (sibling of this
// executable). Falls back to the bare name (PATH) for `cargo run` in development.
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

// Build a Command for the engine. On Windows a GUI app spawning a console program
// pops a black console window unless CREATE_NO_WINDOW is set; suppress it.
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

// Read the canonical `xoksa.env` (`xoksa_paths::env_file` — §6's single location).
//
// The app chdirs into the config dir at startup, so a bare `"xoksa.env"` lands on
// the same file today. That is an implicit dependency on the cwd, which is exactly
// the shape §6 rules out ("no implicit ./xoksa.env fallback"): the chdir result is
// discarded, so a failed one would leave a relative read pointing at whatever
// directory the process was launched from — silently answering "no LAN, default
// port" from a file that is not the user's.
fn read_canonical_env() -> Option<String> {
    std::fs::read_to_string(xoksa_paths::env_file()?).ok()
}

// Engine bind host: 0.0.0.0 only when LAN access is enabled in the config,
// otherwise loopback. Read from the canonical xoksa.env.
fn serve_host() -> String {
    let lan = read_canonical_env()
        .map(|s| {
            s.lines()
                .any(|l| l.trim().eq_ignore_ascii_case("DESKTOP_LAN_ACCESS=true"))
        })
        .unwrap_or(false);
    if lan {
        "0.0.0.0".to_string()
    } else {
        LOOPBACK.to_string()
    }
}

// Engine port: fixed/known so the UIs can connect (design decision ①). Read from
// SERVE_PORT in xoksa.env, default 8787.
fn serve_port() -> u16 {
    read_canonical_env()
        .and_then(|s| {
            s.lines().find_map(|l| {
                l.trim()
                    .strip_prefix("SERVE_PORT=")
                    .map(|v| v.trim().to_string())
            })
        })
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

// ── Config payload (same shape as the engine's apply-config / the form) ──────
#[derive(Deserialize, Serialize)]
struct OllamaInput {
    alias: String,
    model: String,
    host: String,
    port: String,
}

#[derive(Deserialize, Serialize)]
struct NotifyInput {
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    to: String,
    #[serde(default)]
    secret: String,
}

fn d_true() -> bool {
    true
}
fn d_serve_port() -> u16 {
    DEFAULT_PORT
}
fn d_buy_rsi() -> f64 {
    30.0
}
fn d_sell_rsi() -> f64 {
    70.0
}
fn d_macd_low() -> f64 {
    2.0
}
fn d_macd_mid() -> f64 {
    10.0
}
fn d_macd_extreme() -> f64 {
    100.0
}
fn d_w_basic() -> f64 {
    2.0
}
fn d_w_one() -> f64 {
    1.0
}
fn d_u5() -> usize {
    5
}
fn d_u20() -> usize {
    20
}
fn d_u14() -> usize {
    14
}
fn d_u10() -> usize {
    10
}
fn d_u9() -> usize {
    9
}
fn d_u26() -> usize {
    26
}
fn d_stddev() -> f64 {
    2.0
}
fn d_bbpct() -> f64 {
    8.0
}
fn d_fibratio() -> f64 {
    0.05
}

#[derive(Deserialize, Serialize)]
struct OnboardingPayload {
    lang: String,
    #[serde(default)]
    openai_key: String,
    #[serde(default)]
    gemini_key: String,
    #[serde(default)]
    claude_key: String,
    #[serde(default)]
    brave_key: String,
    #[serde(default)]
    jquants_key: String,
    #[serde(default)]
    openai_model: String,
    #[serde(default)]
    gemini_model: String,
    #[serde(default)]
    claude_model: String,
    #[serde(default)]
    no_llm: bool,
    primary_provider: u8,
    #[serde(default)]
    ollama: Vec<OllamaInput>,
    #[serde(default)]
    notify: Vec<NotifyInput>,
    #[serde(default)]
    sec_user_agent: String,
    #[serde(default)]
    alias_csv: String,
    #[serde(default)]
    news_enabled: bool,
    #[serde(default)]
    fundamental_enabled: bool,
    #[serde(default)]
    lan_access: bool,
    #[serde(default = "d_serve_port")]
    serve_port: u16,
    #[serde(default = "d_true")]
    ind_ema: bool,
    #[serde(default = "d_true")]
    ind_sma: bool,
    #[serde(default = "d_true")]
    ind_fibonacci: bool,
    #[serde(default = "d_true")]
    ind_stochastics: bool,
    #[serde(default = "d_true")]
    ind_adx: bool,
    #[serde(default = "d_true")]
    ind_roc: bool,
    #[serde(default = "d_true")]
    ind_bollinger: bool,
    #[serde(default = "d_true")]
    ind_vwap: bool,
    #[serde(default = "d_true")]
    ind_ichimoku: bool,
    #[serde(default)]
    macd_minus_ok: bool,
    #[serde(default = "d_buy_rsi")]
    buy_rsi: f64,
    #[serde(default = "d_sell_rsi")]
    sell_rsi: f64,
    #[serde(default = "d_macd_low")]
    macd_diff_low: f64,
    #[serde(default = "d_macd_mid")]
    macd_diff_mid: f64,
    #[serde(default = "d_macd_extreme")]
    macd_diff_extreme: f64,
    #[serde(default = "d_w_basic")]
    weight_basic: f64,
    #[serde(default = "d_w_one")]
    weight_ema: f64,
    #[serde(default = "d_w_one")]
    weight_sma: f64,
    #[serde(default = "d_w_one")]
    weight_bollinger: f64,
    #[serde(default = "d_w_one")]
    weight_roc: f64,
    #[serde(default = "d_w_one")]
    weight_adx: f64,
    #[serde(default = "d_w_one")]
    weight_stochastics: f64,
    #[serde(default = "d_w_one")]
    weight_fibonacci: f64,
    #[serde(default = "d_w_one")]
    weight_vwap: f64,
    #[serde(default = "d_w_one")]
    weight_ichimoku: f64,
    #[serde(default = "d_u5")]
    ema_short_period: usize,
    #[serde(default = "d_u20")]
    ema_long_period: usize,
    #[serde(default = "d_u5")]
    sma_short_period: usize,
    #[serde(default = "d_u20")]
    sma_long_period: usize,
    #[serde(default = "d_u14")]
    adx_period: usize,
    #[serde(default = "d_u10")]
    roc_period: usize,
    #[serde(default = "d_u14")]
    stochastics_period: usize,
    #[serde(default = "d_u20")]
    bollinger_period: usize,
    #[serde(default = "d_stddev")]
    bollinger_stddev_multiplier: f64,
    #[serde(default = "d_bbpct")]
    bb_bandwidth_squeeze_pct: f64,
    #[serde(default = "d_u9")]
    ichimoku_tenkan_period: usize,
    #[serde(default = "d_u26")]
    ichimoku_kijun_period: usize,
    #[serde(default = "d_u14")]
    vwap_period: usize,
    #[serde(default = "d_fibratio")]
    fibonacci_neutral_ratio: f64,
}

// ── Password gate (engine owns the keychain; verify/set over stdin) ──────────
#[tauri::command]
fn verify_password(mut password: String) -> Result<bool, String> {
    let out = engine_with_stdin("settings-password", &["--verify"], &password);
    password.zeroize();
    let out = out?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).contains("\"ok\":true"))
}

#[tauri::command]
fn change_password(mut new_password: String) -> Result<(), String> {
    let out = engine_with_stdin("settings-password", &["--set"], &new_password);
    new_password.zeroize();
    let out = out?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

// Run an engine subcommand, streaming `stdin_data` on its stdin (so secrets never
// touch argv), and return its captured output.
fn engine_with_stdin(
    subcmd: &str,
    args: &[&str],
    stdin_data: &str,
) -> Result<std::process::Output, String> {
    let mut child = engine_command()
        .arg(subcmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to launch engine: {e}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "engine stdin unavailable".to_string())
        .and_then(|mut s| {
            s.write_all(stdin_data.as_bytes())
                .map_err(|e| e.to_string())
        })?;
    child.wait_with_output().map_err(|e| e.to_string())
}

// ── Config read/write (SOT: config + keychain live in the engine) ────────────
#[tauri::command]
fn load_config() -> Result<serde_json::Value, String> {
    let out = engine_command()
        .arg("config-json")
        .output()
        .map_err(|e| format!("failed to run engine config-json: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "engine config-json failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("engine config-json returned invalid JSON: {e}"))
}

#[tauri::command]
fn save_config(payload: OnboardingPayload) -> Result<(), String> {
    let mut json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    let mut child = engine_command()
        .arg("apply-config")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to launch engine: {e}"))?;
    let write_res = child
        .stdin
        .take()
        .ok_or_else(|| "engine stdin unavailable".to_string())
        .and_then(|mut s| s.write_all(json.as_bytes()).map_err(|e| e.to_string()));
    json.zeroize();
    write_res?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if msg.is_empty() {
            "Failed to write the configuration. Check OS keychain access.".to_string()
        } else {
            msg
        })
    }
}

// ── Access token: generate in this app, user copies it into each UI ──────────
#[tauri::command]
fn get_token() -> Result<String, String> {
    let out = engine_command()
        .arg("serve")
        .arg("--show-token")
        .output()
        .map_err(|e| format!("failed to run engine: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let s = String::from_utf8_lossy(&out.stdout);
    for line in s.lines() {
        if let Some(t) = line.strip_prefix("SERVE_AUTH_TOKEN: ") {
            return Ok(t.trim().to_string());
        }
    }
    Ok(String::new()) // not generated yet
}

#[tauri::command]
fn generate_token() -> Result<String, String> {
    let out = engine_command()
        .arg("serve")
        .arg("--rotate-serve-token")
        .output()
        .map_err(|e| format!("failed to run engine: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let s = String::from_utf8_lossy(&out.stdout);
    for line in s.lines() {
        if let Some(t) = line.split("New token: ").nth(1) {
            return Ok(t.trim().to_string());
        }
    }
    Err("could not read the generated token".to_string())
}

// ── Ollama model list + channel test (same as the desktop) ───────────────────
#[tauri::command]
fn list_ollama_models(host: String, port: String) -> Vec<String> {
    let h = if host.trim().is_empty() {
        LOOPBACK.to_string()
    } else {
        host.trim().to_string()
    };
    let p: u16 = port.trim().parse().unwrap_or(11434);
    match engine_command()
        .arg("ollama-models")
        .arg("--host")
        .arg(&h)
        .arg("--port")
        .arg(p.to_string())
        .output()
    {
        Ok(o) if o.status.success() => serde_json::from_slice(&o.stdout).unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[tauri::command]
fn test_notify(kind: String, to: String, secret: String, n: u8) -> Result<(), String> {
    #[derive(Serialize)]
    struct TestInput {
        kind: String,
        to: String,
        secret: String,
        n: u8,
    }
    let mut json = serde_json::to_string(&TestInput {
        kind,
        to,
        secret,
        n,
    })
    .map_err(|e| e.to_string())?;
    let mut child = engine_command()
        .arg("test-notify")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to launch engine: {e}"))?;
    let write_res = child
        .stdin
        .take()
        .ok_or_else(|| "engine stdin unavailable".to_string())
        .and_then(|mut s| s.write_all(json.as_bytes()).map_err(|e| e.to_string()));
    json.zeroize();
    write_res?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if msg.is_empty() {
            "Test notification failed.".to_string()
        } else {
            msg
        })
    }
}

// ── Engine lifecycle (start / restart; spawned detached — persists) ──────────
fn spawn_engine() -> Result<std::process::Child, String> {
    engine_command()
        .arg("serve")
        .arg("--ui")
        .arg("--host")
        .arg(serve_host())
        .arg("--port")
        .arg(serve_port().to_string())
        .spawn()
        .map_err(|e| format!("failed to start engine: {e}"))
}

// The local URL the engine serves at (returned as the command result for display).
// The engine may bind 0.0.0.0 (LAN), but the local access point is always loopback.
fn serve_url() -> String {
    format!("http://{}:{}", LOOPBACK, serve_port())
}

// Is anything listening on the port we are about to bind? Only this app's own
// child can be stopped from here; a foreign listener (a `xoksa serve` started
// elsewhere, or an unrelated program) must be reported, never killed.
fn port_is_taken(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::time::Duration::from_millis(300),
    )
    .is_ok()
}

// Wait for the freshly spawned engine to answer, so "restarted" means the engine
// is actually up — not merely that a process was created. An immediate exit
// (a bind failure, say) is reported with its status instead of a bare timeout.
fn wait_until_ready(child: &mut std::process::Child, port: u16) -> Result<(), String> {
    for _ in 0..50 {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("engine exited right after starting ({status})"));
        }
        if port_is_taken(port) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err("engine did not start listening within 5 seconds".to_string())
}

// Restart the engine: stop the child this session started (if any), then start a
// fresh one — the only way a new port or access token takes effect. One button:
// with nothing running it simply starts the engine, so the user never has to
// know which case they are in.
#[tauri::command]
fn restart_engine(state: tauri::State<EngineProcess>) -> Result<String, String> {
    let mut guard = state
        .0
        .lock()
        .map_err(|_| "engine state lock".to_string())?;
    if let Some(mut child) = guard.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let port = serve_port();
    // Someone else holds the port. Killing a process this app did not start is
    // not ours to do, so name the situation instead of spawning a doomed child.
    if port_is_taken(port) {
        return Err(format!(
            "port {port} is already in use by another program (an engine started \
             outside this app, perhaps). Stop it, or change the port above, then \
             restart."
        ));
    }
    let mut child = spawn_engine()?;
    wait_until_ready(&mut child, port)?;
    *guard = Some(child);
    Ok(serve_url())
}

// Whether the form holds edits the user has not saved. The WebView owns the
// truth (it knows what was typed) and mirrors it here so the native close button
// can be intercepted — a window closed by the title bar must warn just like the
// Quit button does.
struct UnsavedChanges(std::sync::atomic::AtomicBool);

#[tauri::command]
fn set_unsaved(state: tauri::State<UnsavedChanges>, unsaved: bool) {
    state.0.store(unsaved, std::sync::atomic::Ordering::Relaxed);
}

// Quit after the WebView has confirmed (or found nothing to confirm). The engine
// this app started is left running on purpose — the settings app is transient,
// and killing the engine would drop the connection just configured (§6).
#[tauri::command]
fn quit_app(app: tauri::AppHandle, state: tauri::State<UnsavedChanges>) {
    state.0.store(false, std::sync::atomic::Ordering::Relaxed);
    app.exit(0);
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(EngineProcess(Mutex::new(None)))
        .manage(UnsavedChanges(std::sync::atomic::AtomicBool::new(false)))
        .invoke_handler(tauri::generate_handler![
            verify_password,
            change_password,
            load_config,
            save_config,
            get_token,
            generate_token,
            list_ollama_models,
            test_notify,
            restart_engine,
            set_unsaved,
            quit_app
        ])
        .on_window_event(|window, event| {
            // Native close button: hold the window open and let the WebView ask,
            // so both exit paths give the same warning.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let unsaved = window
                    .state::<UnsavedChanges>()
                    .0
                    .load(std::sync::atomic::Ordering::Relaxed);
                if unsaved {
                    api.prevent_close();
                    let _ = window.emit("confirm-quit", ());
                }
            }
        })
        .setup(|_app| {
            // Stable working directory: the SINGLE canonical config dir shared with
            // the engine, CLI, and desktop (SOT — xoksa_paths), so this app reads/
            // writes the same xoksa.env regardless of where it is launched from.
            if let Some(dir) = xoksa_paths::config_dir() {
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::env::set_current_dir(&dir);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the XOKSA settings app");
}
