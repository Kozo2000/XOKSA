//! File-backed saved backtest strategies (the rule editor's save / reuse).
//!
//! Replaces the former DB table `saved_strategies`: a tiny JSON file under the
//! user's home (`~/.xoksa.strategies.json`). No database — this is the only
//! user-authored data that needs to survive across sessions, and a flat file is
//! enough for it.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// One saved rule set: a name and its `StrategyRules` JSON (stored verbatim, the
/// same `spec_json` string the rule editor sends).
#[derive(Clone, Serialize, Deserialize)]
pub struct SavedStrategy {
    pub name: String,
    pub spec_json: String,
}

/// Cap on how many saved rules are kept on disk: the most-recent are retained,
/// older ones evicted. Bounds the file so it cannot grow without limit (see the
/// security assessment).
const MAX_SAVED_STRATEGIES: usize = 200;

/// Path to the strategies file (`~/.xoksa.strategies.json`; falls back to the
/// working directory if the home directory can't be resolved).
fn store_path() -> String {
    match crate::private::home_dir() {
        Some(h) => format!("{h}/.xoksa.strategies.json"),
        None => ".xoksa.strategies.json".to_string(),
    }
}

/// All saved strategies, most-recently-saved first. A missing or unreadable file
/// yields an empty list (best-effort, never an error to the caller).
pub fn load() -> Vec<SavedStrategy> {
    load_from(&store_path())
}

/// Upsert a strategy by name (moved to the front). No-op in private mode.
pub fn save(name: &str, spec_json: &str) -> Result<()> {
    if crate::private::is_private() {
        return Ok(());
    }
    save_to(&store_path(), name, spec_json)
}

/// Read + parse the strategies file at `path` (empty on missing/invalid).
fn load_from(path: &str) -> Vec<SavedStrategy> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// Upsert `name` into the file at `path`, moved to the front (newest first).
fn save_to(path: &str, name: &str, spec_json: &str) -> Result<()> {
    let mut list = load_from(path);
    list.retain(|s| s.name != name);
    list.insert(
        0,
        SavedStrategy {
            name: name.to_string(),
            spec_json: spec_json.to_string(),
        },
    );
    // Keep only the most-recent MAX_SAVED_STRATEGIES (evict oldest).
    list.truncate(MAX_SAVED_STRATEGIES);
    let text = serde_json::to_string_pretty(&list).context("serializing strategies")?;
    std::fs::write(path, text).with_context(|| format!("writing {path}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_orders_newest_first_and_upserts_by_name() {
        // A unique temp path so the test never touches the real strategies file.
        let path = std::env::temp_dir()
            .join(format!("xoksa_strat_test_{}.json", std::process::id()))
            .to_string_lossy()
            .to_string();
        let _ = std::fs::remove_file(&path);

        // Missing file → empty list.
        assert!(load_from(&path).is_empty());

        // Two rules; the most recently saved comes first.
        save_to(&path, "alpha", r#"{"a":1}"#).unwrap();
        save_to(&path, "beta", r#"{"b":2}"#).unwrap();
        let list = load_from(&path);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "beta");
        assert_eq!(list[1].name, "alpha");

        // Saving an existing name upserts (no duplicate, moved to front, new spec).
        save_to(&path, "alpha", r#"{"a":9}"#).unwrap();
        let list = load_from(&path);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "alpha");
        assert_eq!(list[0].spec_json, r#"{"a":9}"#);

        let _ = std::fs::remove_file(&path);
    }
}
