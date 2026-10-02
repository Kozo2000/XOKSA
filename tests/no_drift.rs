//! Drift guards: the classes of defect that a human review keeps missing.
//!
//! Every problem these tests catch was found by hand in 2.9.4, one at a time,
//! long after it shipped: a setting the engine accepted but no screen offered
//! (`SERVE_PORT`), a manual served over HTTP that nothing linked to, a Tauri
//! command with no caller, dictionary strings for a UI element that had been
//! deleted, and a language read from four different places. None of them are
//! visible in a diff — each is an *absence* of a connection between two files.
//!
//! So they are asserted here instead. These are source-inspection tests: they
//! read the repository's own files and check that the pieces still refer to each
//! other. They run under `cargo test` on every platform (no shell), which is why
//! they are not a script.
//!
//! Paths are resolved from `CARGO_MANIFEST_DIR` — the engine crate is the
//! workspace root, so the sibling crates are one level down.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn read(rel: &str) -> String {
    let p = repo(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

/// Field names of a `struct` — the lines that look like `name: Type,`.
fn struct_fields(source: &str, struct_name: &str) -> BTreeSet<String> {
    let start = source
        .find(&format!("struct {struct_name}"))
        .unwrap_or_else(|| panic!("struct {struct_name} not found"));
    let body = &source[start..];
    let end = body.find("\n}").expect("struct end");
    body[..end]
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let (name, rest) = l.split_once(':')?;
            let name = name.strip_prefix("pub ").unwrap_or(name).trim();
            let plausible = !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                && !rest.trim().is_empty();
            plausible.then(|| name.to_string())
        })
        .collect()
}

/// Every setting the engine accepts must be offered by the settings app, and the
/// app must not send fields the engine drops. `SERVE_PORT` was readable from
/// `xoksa.env` but had no field for two releases — only someone hand-editing the
/// file could move the engine off port 8787.
#[test]
fn every_engine_setting_is_reachable_from_the_settings_app() {
    let engine = struct_fields(&read("src/setup.rs"), "ApplyConfigInput");
    let app = struct_fields(&read("xoksa-setup/src/main.rs"), "OnboardingPayload");

    let hidden: Vec<_> = engine.difference(&app).collect();
    let dropped: Vec<_> = app.difference(&engine).collect();
    assert!(
        hidden.is_empty(),
        "the engine accepts settings the app cannot set (hidden settings): {hidden:?}"
    );
    assert!(
        dropped.is_empty(),
        "the app sends fields the engine ignores (silently discarded input): {dropped:?}"
    );
}

/// Localized strings whose element no longer exists. Harmless on their own, but
/// they are the fingerprint of a UI element that was removed while its wiring
/// stayed — which is how the dead "Setup Guide" and "Indicator Guide" links
/// survived: their text lived on in both dictionaries with nothing rendering it.
#[test]
fn no_orphan_ui_strings() {
    // The Rust side counts as a reference: the desktop's failures arrive as codes
    // (`err_spawn|…`) that the page looks up dynamically, so the string that
    // proves the key is alive lives in the Rust source.
    for (file, rust) in [
        ("xoksa-setup/frontend/index.html", "xoksa-setup/src/main.rs"),
        (
            "xoksa-desktop/frontend/index.html",
            "xoksa-desktop/src/main.rs",
        ),
    ] {
        let s = format!("{}{}", read(file), read(rust));
        // Only the I18N dictionaries — elsewhere `name: value` is ordinary JS.
        let dict = {
            let from = s.find("const I18N").expect("I18N table");
            let rest = &s[from..];
            let to = rest.find("\n      };").map(|i| from + i).unwrap_or(s.len());
            &s[from..to]
        };
        let mut orphans: Vec<String> = Vec::new();
        for part in dict.split(',') {
            let Some((key, rest)) = part.split_once(':') else {
                continue;
            };
            let key = key.trim();
            if key.len() < 3
                || !key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                || !rest.trim_start().starts_with('"')
            {
                continue;
            }
            // A key is alive if it appears anywhere outside the dictionary itself:
            // in markup (`data-i18n`), as a property (`d.key`), by name, or inside
            // a Rust format string (the desktop's failure codes, `err_spawn|{e}`).
            // Counting past the dictionary's own two entries catches every form
            // without enumerating them.
            let referenced = s.matches(key).count() > dict.matches(key).count();
            if !referenced && !orphans.contains(&key.to_string()) {
                orphans.push(key.to_string());
            }
        }
        assert!(
            orphans.is_empty(),
            "{file}: dictionary strings nothing renders: {orphans:?}"
        );
    }
}

/// An HTTP route no client calls is a surface nobody reviews. `/manual/:slug`
/// served two compiled-in documents for four releases with nothing linking to
/// it; `POST /api/analysis/context-pack` was never called at all.
#[test]
fn every_http_route_has_a_caller() {
    let router = read("src/server/mod.rs");
    let clients = [
        read("webui-leptos/src/main.rs"),
        read("xoksa-desktop/src/main.rs"),
        read("xoksa-desktop/frontend/index.html"),
        read("xoksa-setup/src/main.rs"),
        read("xoksa-setup/frontend/index.html"),
    ]
    .concat();

    let mut unused: Vec<String> = Vec::new();
    for line in router.lines() {
        let Some(rest) = line.trim().strip_prefix(".route(\"") else {
            continue;
        };
        let Some((path, _)) = rest.split_once('"') else {
            continue;
        };
        if !path.starts_with("/api/") {
            continue; // `/`, `/login`, `/auth` are browser navigations
        }
        // Match up to the first path parameter (`/api/symbol/:symbol/news`).
        let probe = path.split(":").next().unwrap_or(path);
        if !clients.contains(probe) {
            unused.push(path.to_string());
        }
    }
    assert!(
        unused.is_empty(),
        "routes no client calls (an unreviewed surface): {unused:?}"
    );
}

/// A registered Tauri command with no caller is reachable from the WebView's IPC
/// while nothing in the product uses it — `open_manual` shipped that way.
#[test]
fn every_tauri_command_has_a_caller() {
    for (app, callers) in [
        (
            "xoksa-desktop",
            vec![
                "xoksa-desktop/frontend/index.html",
                "webui-leptos/src/main.rs",
            ],
        ),
        ("xoksa-setup", vec!["xoksa-setup/frontend/index.html"]),
    ] {
        let src = read(&format!("{app}/src/main.rs"));
        let callers: String = callers.iter().map(|f| read(f)).collect();
        let mut uncalled: Vec<String> = Vec::new();
        let mut lines = src.lines().peekable();
        while let Some(l) = lines.next() {
            if l.trim() != "#[tauri::command]" {
                continue;
            }
            // The signature is the next line that declares the function.
            let sig = lines
                .by_ref()
                .find(|l| l.contains("fn "))
                .unwrap_or_default();
            let Some(after) = sig.split("fn ").nth(1) else {
                continue;
            };
            let name = after.split('(').next().unwrap_or("").trim();
            if !name.is_empty() && !callers.contains(&format!("\"{name}\"")) {
                uncalled.push(name.to_string());
            }
        }
        assert!(
            uncalled.is_empty(),
            "{app}: Tauri commands nothing invokes: {uncalled:?}"
        );
    }
}

/// The UI language is a setting (`LANG`), not a per-surface guess. Each surface
/// used to decide for itself — the dashboard from `localStorage`, the connection
/// screen from the OS locale — so choosing Japanese in the settings app could
/// leave another screen in English. The OS locale may only be a fallback, which
/// is why these two files are allowed exactly one mention each.
#[test]
fn the_ui_language_has_one_source() {
    let dashboard = read("webui-leptos/src/main.rs");
    assert!(
        !dashboard.contains("xoksa.lang"),
        "the dashboard keeps its own copy of the language again (localStorage)"
    );
    assert!(
        dashboard.contains("xoksa-lang"),
        "the dashboard no longer reads the language the engine stamps into the page"
    );

    for (file, allowed) in [
        ("webui-leptos/src/main.rs", 0usize),
        ("xoksa-desktop/frontend/index.html", 1usize),
        ("xoksa-setup/frontend/index.html", 1usize),
    ] {
        let n = read(file).matches("navigator.language").count();
        assert_eq!(
            n, allowed,
            "{file}: the OS locale is a first-run fallback only — expected {allowed} use(s), found {n}"
        );
    }
}

/// iCloud's sync-conflict copies — `no_drift 2.rs`, `Cargo 2.toml`, `icon 3.png`.
///
/// The repository lives under `~/Documents`, so iCloud duplicates files it thinks
/// diverged, appending ` 2` before the extension. They are untracked, invisible in
/// `git status`, and mostly harmless — until one lands in `tests/`, where Cargo
/// turns the filename into a crate name and the whole suite stops compiling:
/// `invalid character ' ' in crate name: 'no_drift 2'`. That is how 453 tests went
/// dark in 2.9.8, with nothing in the diff to explain it.
///
/// Finding them takes one directory walk, so the walk happens here rather than in
/// someone's afternoon. Build outputs and `.git` are skipped; a hit prints the
/// paths, since the fix is to delete them (they are copies of files already in the
/// tree) and, eventually, to keep the repository out of iCloud altogether.
#[test]
fn no_icloud_conflict_copies() {
    fn walk(dir: &std::path::Path, found: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                // Build trees and git internals carry their own noise and are not
                // synced source.
                if name.starts_with('.') || name.starts_with("target") || name == "dist" {
                    continue;
                }
                walk(&path, found);
            } else if let Some((stem, _)) = name.rsplit_once('.') {
                // A conflict copy ends in " 2", " 3", … before the extension.
                if stem
                    .rsplit_once(' ')
                    .map(|(head, tail)| {
                        !head.is_empty() && tail.chars().all(|c| c.is_ascii_digit())
                    })
                    .unwrap_or(false)
                {
                    found.push(path.display().to_string());
                }
            }
        }
    }

    let mut found = Vec::new();
    walk(&repo("."), &mut found);
    found.sort();
    assert!(
        found.is_empty(),
        "iCloud conflict copies in the tree — delete them (they duplicate files that \
         are already here); one inside tests/ breaks the build outright:\n  {}",
        found.join("\n  ")
    );
}
