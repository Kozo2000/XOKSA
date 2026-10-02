//! Lightweight structured logging — levels + stable error codes, with a
//! **deduplicated** console and an append-only local file. No external crates
//! (auditable). Native-only (the WASM UI is a separate crate).
//!
//! Why: emitting a warning is a notification that something should be improved —
//! not a "print and forget". So each entry carries a stable code (searchable /
//! documentable), repeats are collapsed on the console, and the full record goes
//! to a file for later review.
//!
//! SECURITY: never pass credentials (Class A — API keys) as `code`/`msg`. Callers
//! must keep keys out of log text, exactly as for any other error string.

use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
    Info,
}

/// Output format for log lines (console + file).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Human-readable: `⚠️ [CODE] msg` / `<ts> WARN [CODE] msg`.
    Text,
    /// One JSON object per line (NDJSON) — for log aggregation / the REST server.
    Json,
}

/// Global format, set once at startup (default text). Lock-free: read on every
/// log call. 0 = text, 1 = json.
static FORMAT: AtomicU8 = AtomicU8::new(0);

/// Select the log output format (e.g. `xoksa serve --log-format json`). Applies to
/// both the console and the file. Call once during startup, before serving.
pub fn set_format(f: Format) {
    FORMAT.store(u8::from(f == Format::Json), Ordering::Relaxed);
}

fn current_format() -> Format {
    if FORMAT.load(Ordering::Relaxed) == 1 {
        Format::Json
    } else {
        Format::Text
    }
}

/// Escape a string for embedding in a JSON string literal (dependency-free, so we
/// hand-roll the minimal set the JSON spec requires: quotes, backslash, and
/// control characters < 0x20).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// One line as a JSON object: `{"ts":..,"level":..,"code":..,"msg":..}`.
fn json_line(ts: &str, level: Level, code: &str, msg: &str) -> String {
    format!(
        "{{\"ts\":\"{}\",\"level\":\"{}\",\"code\":\"{}\",\"msg\":\"{}\"}}",
        json_escape(ts),
        level.tag(),
        json_escape(code),
        json_escape(msg),
    )
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
        }
    }
    fn console_prefix(self) -> &'static str {
        match self {
            Level::Error => "❌",
            Level::Warn => "⚠️",
            Level::Info => "",
        }
    }
}

/// Rotate the log file once it exceeds this size (keeps 2 archived generations).
const MAX_LOG_BYTES: u64 = 5_000_000;
/// Bound the in-memory dedup set so distinct messages can't grow it forever.
const MAX_SEEN: usize = 2_000;

struct State {
    /// (code|msg) already shown on the console — for one-time console output.
    seen: HashSet<String>,
    /// Resolved log file path (lazy).
    path: Option<String>,
    path_resolved: bool,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Emit a coded log entry. WARN/ERROR print to stderr **once** per unique
/// (code, msg) so repeats don't flood; INFO is file-only. The full record is
/// appended to the local log file unless private mode is active (no-trace).
pub fn log(level: Level, code: &str, msg: &str) {
    let st = STATE.get_or_init(|| {
        Mutex::new(State {
            seen: HashSet::new(),
            path: None,
            path_resolved: false,
        })
    });
    let mut g = st.lock().unwrap_or_else(|e| e.into_inner());

    let fmt = current_format();
    let ts = now();

    // Console: show each unique (code|msg) once.
    let key = format!("{code}|{msg}");
    let first_time = g.seen.insert(key);
    if g.seen.len() > MAX_SEEN {
        g.seen.clear();
    }
    if first_time && level != Level::Info {
        match fmt {
            Format::Json => eprintln!("{}", json_line(&ts, level, code, msg)),
            Format::Text => eprintln!("{} [{}] {}", level.console_prefix(), code, msg),
        }
    }

    // File: full record (skipped in private mode = no disk trace).
    if crate::private::is_private() {
        return;
    }
    if !g.path_resolved {
        g.path = log_file_path();
        g.path_resolved = true;
    }
    if let Some(path) = g.path.clone() {
        let line = match fmt {
            Format::Json => json_line(&ts, level, code, msg),
            Format::Text => format!("{} {} [{}] {}", ts, level.tag(), code, msg),
        };
        append_line(&path, &line);
    }
}

/// Log file path: `<cwd>/logs/xoksa-error.log` by default. If the working directory
/// isn't writable (a backend launched from a locked-down or system directory), fall
/// back to a machine-wide **system** location — never the user's home. Windows:
/// `%PROGRAMDATA%\xoksa\logs`; Unix: the system temp dir (`/tmp/xoksa`). The chosen
/// directory is created here (so writability is proven and startup can print it).
fn log_file_path() -> Option<String> {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("logs"));
    }
    #[cfg(windows)]
    if let Some(pd) = std::env::var_os("PROGRAMDATA") {
        dirs.push(std::path::PathBuf::from(pd).join("xoksa").join("logs"));
    }
    #[cfg(not(windows))]
    dirs.push(std::env::temp_dir().join("xoksa"));

    for dir in dirs {
        if std::fs::create_dir_all(&dir).is_ok() {
            return Some(dir.join("xoksa-error.log").to_string_lossy().into_owned());
        }
    }
    None
}

/// The resolved absolute log-file path, for a startup notice. `None` in private
/// mode (no file is written) or if the working directory cannot be resolved.
pub fn current_log_path() -> Option<String> {
    if crate::private::is_private() {
        return None;
    }
    log_file_path()
}

/// Generational rotation at the size cap: `log` → `log.1` → `log.2` (oldest
/// dropped), keeping the two most recent full files. After rotation `path` no
/// longer exists, so the caller's `create(true)` opens a fresh current file (with
/// permissions re-applied). Renames preserve the `0600` mode on the archived files.
fn rotate_log(path: &str) {
    let gen1 = format!("{path}.1");
    let gen2 = format!("{path}.2");
    let _ = std::fs::remove_file(&gen2); // drop the oldest
    let _ = std::fs::rename(&gen1, &gen2); // .1 → .2
    let _ = std::fs::rename(path, &gen1); // current → .1
}

fn append_line(path: &str, line: &str) {
    // Create the logs/ directory on first write.
    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Generational rotation once the file grows past the cap.
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > MAX_LOG_BYTES {
            rotate_log(path);
        }
    }
    let existed = std::fs::metadata(path).is_ok();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{line}");
        // Restrict permissions on first creation (may contain symbols/analysis text).
        #[cfg(unix)]
        if !existed {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        #[cfg(not(unix))]
        let _ = existed;
    }
}

/// Log a warning with a stable code.
pub fn warn(code: &str, msg: &str) {
    log(Level::Warn, code, msg);
}

/// Log an error with a stable code.
pub fn error(code: &str, msg: &str) {
    log(Level::Error, code, msg);
}

/// Log an informational entry (file-only) with a stable code.
pub fn info(code: &str, msg: &str) {
    log(Level::Info, code, msg);
}

#[cfg(test)]
mod tests {
    use super::{json_escape, json_line, Level};

    #[test]
    fn escapes_json_special_chars() {
        assert_eq!(json_escape("plain"), "plain");
        assert_eq!(json_escape("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(json_escape("line1\nline2\t."), "line1\\nline2\\t.");
        // Control char below 0x20 → \u escape.
        assert_eq!(json_escape("\u{0001}"), "\\u0001");
    }

    #[test]
    fn json_line_is_wellformed() {
        let line = json_line(
            "2026-07-03T00:00:00Z",
            Level::Warn,
            "XK-TEST",
            "hi \"there\"",
        );
        assert_eq!(
            line,
            r#"{"ts":"2026-07-03T00:00:00Z","level":"WARN","code":"XK-TEST","msg":"hi \"there\""}"#
        );
    }
}
