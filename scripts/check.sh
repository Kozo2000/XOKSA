#!/usr/bin/env bash
# The checks run inside CI and before a local push. Every required tool must
# exist; absence or any failed check makes the command fail.
set -uo pipefail
cd "$(dirname "$0")/.."

missing=0
for tool in cargo cargo-audit cargo-deny cargo-machete; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'Required tool missing: %s\n' "$tool" >&2
        missing=1
    fi
done
[ "$missing" -eq 0 ] || exit 1

fail=0
run() {
    local label="$1"
    shift
    printf '\n=== %s ===\n' "$label"
    if "$@"; then
        printf 'PASS %s\n' "$label"
    else
        printf 'FAIL %s\n' "$label" >&2
        fail=1
    fi
}

run "version labels" bash scripts/check-versions.sh

for workspace in . xoksa-desktop xoksa-setup xoksa-paths; do
    manifest="$workspace/Cargo.toml"
    run "fmt ($workspace)" cargo fmt --manifest-path "$manifest" -- --check
    run "clippy ($workspace)" cargo clippy --locked --manifest-path "$manifest" -- -D warnings
    run "test ($workspace)" cargo test --locked --manifest-path "$manifest" -q
done
run "fmt (webui-leptos)" cargo fmt --manifest-path webui-leptos/Cargo.toml -- --check
run "clippy (webui-leptos)" cargo clippy --locked --manifest-path webui-leptos/Cargo.toml --target wasm32-unknown-unknown -- -D warnings

host="$(rustc -vV | sed -n 's/^host: //p')"
[ -n "$host" ] || { printf 'Cannot determine host target\n' >&2; exit 1; }
for workspace in . webui-leptos xoksa-desktop xoksa-setup xoksa-paths; do
    run "audit ($workspace)" cargo audit --file "$workspace/Cargo.lock"
    target="$host"
    [ "$workspace" != webui-leptos ] || target=wasm32-unknown-unknown
    run "deny ($workspace)" cargo deny --locked --manifest-path "$workspace/Cargo.toml" --target "$target" --config deny.toml check
done
run "unused dependencies" cargo machete

if [ "$fail" -eq 0 ]; then
    printf '\nAll checks passed\n'
else
    printf '\nChecks failed\n' >&2
fi
exit "$fail"
