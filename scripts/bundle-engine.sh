#!/usr/bin/env bash
# Copy the freshly built engine AND settings app into the Tauri sidecar location
# so `cargo tauri build` bundles them via `bundle.externalBin`
# ("binaries/xoksa", "binaries/xoksa-setup").
#
# Tauri expects each sidecar as <name>-<target-triple>[.exe]; at bundle time it
# strips the triple and places the binary next to the app binary — which is
# exactly what xoksa-desktop::engine_bin_path() looks for. Same mechanism on
# macOS and Windows (unified; replaces the old manual cp into XOKSA.app).
#
# Order:  trunk build --release                     (in webui-leptos)
#         cargo build --release --bin xoksa         (engine)
#         cargo build --release                     (in xoksa-setup)
#         scripts/bundle-engine.sh                  (this script)
#         cargo tauri build                         (in xoksa-desktop)
# Runs on macOS and on Windows via Git Bash / MSYS.
#
# Both sidecars are staged here on purpose. Staging only the engine and leaving
# the settings app to a remembered manual step is how v2.9.9 nearly shipped a
# 2.9.8 settings binary inside a 2.9.9 installer, so the version gate below
# refuses to stage a binary that does not report the release version.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

triple="$(rustc -vV | sed -n 's/^host: //p')"
[ -n "$triple" ] || { echo "bundle-engine: could not determine host target triple (is rustc on PATH?)" >&2; exit 1; }

case "$triple" in
  *windows*) ext=".exe" ;;
  *)         ext="" ;;
esac

# The release version every staged binary must report. The engine's manifest is
# the single source; the other two must agree, or the bundle would ship a
# mismatched set of binaries that are meant to be a matched pair (§6).
# Pure bash on purpose. A pipeline here would be fragile twice over: MSYS `sed`
# silently strips CR on Windows while BSD `sed` on macOS does not, so relying on
# it would hide a CRLF manifest; and `… | head -1` under `set -o pipefail` can
# report failure when head closes the pipe on a file that matched. So read the
# file directly and strip CR explicitly.
read_version() {                      # $1 = manifest path
  local line
  while IFS= read -r line || [ -n "$line" ]; do
    line="${line%$'\r'}"
    case "$line" in
      'version = "'*'"')
        line="${line#version = \"}"
        printf '%s\n' "${line%\"}"
        return 0
        ;;
    esac
  done < "$1"
  return 1
}

# `|| true` so a manifest with no version line reaches the message below
# instead of `set -e` killing the script without one.
version="$(read_version Cargo.toml || true)"
[ -n "$version" ] || { echo "bundle-engine: could not read version from Cargo.toml" >&2; exit 1; }
for manifest in xoksa-setup/Cargo.toml xoksa-desktop/Cargo.toml; do
  other="$(read_version "$manifest" || true)"
  [ "$other" = "$version" ] || {
    echo "bundle-engine: $manifest is $other but Cargo.toml is $version — bump them together" >&2
    exit 1
  }
done

# Resolve cargo's target dir for one workspace. `.cargo/config.toml` (which may
# set target-dir = "target.nosync" on macOS) is machine-local (gitignored), so
# the path is not fixed across machines — probe CARGO_TARGET_DIR, then the two
# known layouts. CARGO_TARGET_DIR may be absolute and shared by every
# workspace, so try it unprefixed as well as under the workspace.
find_built() {                        # $1 = workspace dir, $2 = binary file name
  local ws="$1" bin="$2" base candidate
  for base in "${CARGO_TARGET_DIR:-}" "target.nosync" "target"; do
    [ -n "$base" ] || continue
    for candidate in "$ws/$base/release/$bin" "$base/release/$bin"; do
      if [ -f "$candidate" ]; then printf '%s\n' "$candidate"; return 0; fi
    done
  done
  return 1
}

# Rust embeds CARGO_PKG_VERSION as ASCII, and on Windows the PE version resource
# holds it as UTF-16LE. The settings app carries ONLY the UTF-16LE copy, so a
# check that looks for ASCII alone would always fail on it. Dropping NUL bytes
# first lets one search find either encoding. `grep -c` (not `-q`) is deliberate:
# `-q` exits on the first match, and the SIGPIPE it sends to `tr` would surface
# through `set -o pipefail` as a failure on a file that actually matched.
reports_version() {                   # $1 = file, $2 = expected version
  local count
  count="$(LC_ALL=C tr -d '\000' < "$1" | LC_ALL=C grep -acF -- "$2" || true)"
  [ "${count:-0}" -gt 0 ]
}

# The sidecars, each as (workspace directory, base name) — the two entries of
# tauri.conf.json's bundle.externalBin.
names=("xoksa" "xoksa-setup")
workspaces=("." "xoksa-setup")

# Resolve and check every sidecar BEFORE copying any of them, so a failure
# cannot leave binaries/ half staged — one fresh binary beside one stale one is
# exactly the state that would still bundle.
sources=()
for i in "${!names[@]}"; do
  name="${names[$i]}"
  bin="$name$ext"
  if ! src="$(find_built "${workspaces[$i]}" "$bin")"; then
    echo "bundle-engine: $name not built (looked under ${workspaces[$i]} and \$CARGO_TARGET_DIR / target.nosync / target for release/$bin)" >&2
    exit 1
  fi
  if ! reports_version "$src" "$version"; then
    echo "bundle-engine: $src does not report $version — rebuild it before bundling (a stale binary staged here is shipped inside the installer)" >&2
    exit 1
  fi
  sources+=("$src")
done

dest_dir="xoksa-desktop/binaries"
mkdir -p "$dest_dir"
for i in "${!names[@]}"; do
  dest="$dest_dir/${names[$i]}-$triple$ext"
  cp "${sources[$i]}" "$dest"
  chmod +x "$dest" 2>/dev/null || true
  echo "bundle-engine: copied ${sources[$i]} -> $dest ($version)"
done
