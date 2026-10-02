#![forbid(unsafe_code)]
//! Single source of truth for xoksa's canonical, user-scoped filesystem paths.
//!
//! Depended on by BOTH the engine (`xoksa`) and the desktop (`xoksa-desktop`) so
//! the config directory is defined in exactly ONE place — it cannot drift between
//! the two separate crates (the desktop deliberately does not link the engine; see
//! docs/dev-prog/security-design.md §6). The config *contents* live in one
//! `xoksa.env`; this crate keeps its *location* single-sourced too, and performs a
//! one-time import of an existing config into the canonical location so upgrades
//! never lose non-secret settings and there is no split by launch directory.

use std::path::PathBuf;

/// Canonical, user-scoped config directory: `<config_dir>/xoksa`
/// (Windows `%APPDATA%\xoksa`, macOS `~/Library/Application Support/xoksa`,
/// Linux `~/.config/xoksa`). `None` if the OS config dir cannot be determined.
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("xoksa"))
}

/// Canonical `xoksa.env` path: `<config_dir>/xoksa/xoksa.env`.
pub fn env_file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("xoksa.env"))
}

/// The pre-2.7.1 desktop config location — Tauri `app_data_dir()` = the OS data
/// dir plus the bundle identifier `app.xoksa.desktop` (Windows `%APPDATA%`, macOS
/// `~/Library/Application Support`, Linux `~/.local/share`). Kept only as the
/// one-time migration SOURCE for existing desktop installs; never written to.
pub fn legacy_desktop_env_file() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("app.xoksa.desktop").join("xoksa.env"))
}

/// Where a one-time migration should import the config from, given which files
/// exist. Pure (no I/O) so the policy is table-testable. The canonical file, if it
/// already exists, is authoritative and never overwritten. Otherwise prefer an
/// existing legacy desktop config over a working-directory `./xoksa.env`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationSource {
    LegacyDesktop,
    Cwd,
    None,
}

/// Pure migration-source policy (see [`MigrationSource`]).
pub fn choose_migration_source(
    canonical_exists: bool,
    legacy_exists: bool,
    cwd_exists: bool,
) -> MigrationSource {
    if canonical_exists {
        MigrationSource::None
    } else if legacy_exists {
        MigrationSource::LegacyDesktop
    } else if cwd_exists {
        MigrationSource::Cwd
    } else {
        MigrationSource::None
    }
}

/// One-time import of an existing config into the canonical location. Idempotent
/// and non-destructive: it copies (never moves), and never overwrites an existing
/// canonical file. Returns `Ok(Some(source))` for the path it imported from,
/// `Ok(None)` when there was nothing to migrate, or `Err` if the mkdir/copy failed
/// — the caller MUST surface that, because a silently-failed migration looks to the
/// user like lost settings. After a successful run the canonical file is the single
/// authoritative config regardless of the working directory.
pub fn migrate_env_if_needed() -> std::io::Result<Option<PathBuf>> {
    let Some(canonical) = env_file() else {
        return Ok(None);
    };
    let legacy = legacy_desktop_env_file();
    let cwd = PathBuf::from("xoksa.env");
    let source = match choose_migration_source(
        canonical.exists(),
        legacy.as_ref().is_some_and(|p| p.exists()),
        cwd.exists(),
    ) {
        MigrationSource::None => return Ok(None),
        // choose_migration_source returns LegacyDesktop only when legacy exists, so
        // this is Some; fall back to no-op rather than panic if that ever changes.
        MigrationSource::LegacyDesktop => match legacy {
            Some(l) => l,
            None => return Ok(None),
        },
        MigrationSource::Cwd => cwd,
    };
    if let Some(dir) = canonical.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::copy(&source, &canonical)?;
    Ok(Some(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_source_table() {
        // canonical present → never migrate (authoritative), regardless of others.
        assert_eq!(
            choose_migration_source(true, true, true),
            MigrationSource::None
        );
        assert_eq!(
            choose_migration_source(true, false, false),
            MigrationSource::None
        );
        // canonical absent → legacy desktop wins over cwd.
        assert_eq!(
            choose_migration_source(false, true, true),
            MigrationSource::LegacyDesktop
        );
        assert_eq!(
            choose_migration_source(false, true, false),
            MigrationSource::LegacyDesktop
        );
        // canonical absent, no legacy → cwd.
        assert_eq!(
            choose_migration_source(false, false, true),
            MigrationSource::Cwd
        );
        // nothing anywhere → nothing to migrate (canonical is a fresh write target).
        assert_eq!(
            choose_migration_source(false, false, false),
            MigrationSource::None
        );
    }
}
