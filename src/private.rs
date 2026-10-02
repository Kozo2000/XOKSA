//! Process-wide private-mode flag and local home-path helpers.
//!
//! `--private` is a no-trace session: nothing is written to disk. The log file
//! (`logging.rs`), the saved-strategies file (`strategies.rs`), and the alert
//! rules the dashboard would append to `xoksa.env` (`server::monitor`) all
//! consult `is_private`. `home_dir` resolves the user's home directory (used for
//! the strategies / history dotfiles); the error log lives under `logs/` in the
//! working directory (`logging.rs`).

use std::sync::atomic::{AtomicBool, Ordering};

/// When set, nothing is persisted to disk (no-trace session).
static PRIVATE: AtomicBool = AtomicBool::new(false);

/// Mark this process as private (no disk writes) — set once at startup.
pub fn set_private(private: bool) {
    PRIVATE.store(private, Ordering::Relaxed);
}

/// Whether private mode is active.
pub fn is_private() -> bool {
    PRIVATE.load(Ordering::Relaxed)
}

/// Serializes the tests that set [`set_private`], and restores the flag when the
/// guard drops.
///
/// The flag is process-wide and cargo runs tests on parallel threads, so a test
/// that sets it is not isolated from one that reads it — and a per-module gate
/// only serializes that module. This one lives beside the flag so every test
/// that touches it shares the same gate, and none has to remember to put the
/// flag back.
#[cfg(test)]
pub struct PrivateFlagGuard {
    /// Held for the guard's lifetime; the serialization is the point, not a read.
    _gate: std::sync::MutexGuard<'static, ()>,
    restore: bool,
}

#[cfg(test)]
impl Drop for PrivateFlagGuard {
    fn drop(&mut self) {
        PRIVATE.store(self.restore, Ordering::Relaxed);
    }
}

/// Take the gate, remembering the flag to restore on drop.
#[cfg(test)]
pub fn test_gate() -> PrivateFlagGuard {
    static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let g = GATE.lock().unwrap_or_else(|e| e.into_inner());
    PrivateFlagGuard {
        _gate: g,
        restore: is_private(),
    }
}

/// User home directory (`HOME`, or `USERPROFILE` on Windows).
pub fn home_dir() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
        .map(|s| s.trim_end_matches(['/', '\\']).to_string())
        .filter(|s| !s.is_empty())
}
