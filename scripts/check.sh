#!/usr/bin/env bash
# Local mirror of .github/workflows/ci.yml — runs the same gates on your own
# machine at $0, so quality checks do not depend on GitHub-hosted Actions
# minutes. Run before every push.
#
#   ./scripts/check.sh
#
# Core gates (fmt / clippy / test) always run. The supply-chain gates
# (audit / deny / machete) run only if their tool is installed; a missing tool
# is reported and skipped, not treated as a pass. Exit code is non-zero if any
# gate that actually ran failed.
set -uo pipefail

cd "$(dirname "$0")/.."

fail=0

run() { # run "<label>" <cmd...>
    local label="$1"
    shift
    printf '\n=== %s ===\n' "$label"
    if "$@"; then
        printf '✅ %s\n' "$label"
    else
        printf '❌ %s\n' "$label"
        fail=1
    fi
}

optional() { # optional "<label>" <tool-binary> <cmd...>
    local label="$1" tool="$2"
    shift 2
    if command -v "$tool" >/dev/null 2>&1; then
        run "$label" "$@"
    else
        printf '\n=== %s ===\n' "$label"
        printf '⚠️  %s not installed — skipped (install: cargo install %s --locked)\n' "$tool" "$tool"
    fi
}

# 1) Format / Lint / Test  (ci.yml job: check)
run "cargo fmt --check" cargo fmt -- --check
run "cargo clippy -D warnings" cargo clippy -- -D warnings
run "cargo test -q" cargo test -q

# 1b) The webui-leptos (WASM frontend) and xoksa-desktop (Tauri shell) are their
# own workspaces — root fmt/clippy do NOT cover them. Gate them explicitly so a
# format or lint regression there cannot slip through (as it did before 2.6.4).
run "cargo fmt --check (webui-leptos)" \
    cargo fmt --manifest-path webui-leptos/Cargo.toml -- --check
run "cargo clippy -D warnings (webui-leptos)" \
    cargo clippy --manifest-path webui-leptos/Cargo.toml --target wasm32-unknown-unknown -- -D warnings
run "cargo fmt --check (xoksa-desktop)" \
    cargo fmt --manifest-path xoksa-desktop/Cargo.toml -- --check
run "cargo clippy -D warnings (xoksa-desktop)" \
    cargo clippy --manifest-path xoksa-desktop/Cargo.toml -- -D warnings

# 1c) xoksa-paths is a tiny shared crate (single canonical config path), excluded
# from the root workspace — gate its format, lint, and table-driven tests too
# (mirrors the same steps added to ci.yml).
run "cargo fmt --check (xoksa-paths)" \
    cargo fmt --manifest-path xoksa-paths/Cargo.toml -- --check
run "cargo clippy -D warnings (xoksa-paths)" \
    cargo clippy --manifest-path xoksa-paths/Cargo.toml -- -D warnings
run "cargo test (xoksa-paths)" \
    cargo test --manifest-path xoksa-paths/Cargo.toml

# 2) Supply-chain: advisories  (ci.yml job: audit)
optional "cargo audit (native)" cargo-audit cargo audit
optional "cargo audit (webui-leptos / WASM frontend)" cargo-audit \
    cargo audit -f webui-leptos/Cargo.lock

# 3) Supply-chain: licenses + advisories + bans + sources  (ci.yml job: deny)
optional "cargo deny check (native)" cargo-deny cargo deny check
optional "cargo deny check (webui-leptos / WASM frontend)" cargo-deny \
    cargo deny --manifest-path webui-leptos/Cargo.toml --config deny.toml check

# 4) Hygiene: unused dependencies  (ci.yml job: machete)
optional "cargo machete (both workspaces)" cargo-machete cargo machete

printf '\n'
if [ "$fail" -eq 0 ]; then
    printf '✅ all executed checks passed\n'
else
    printf '❌ some checks failed (see above)\n'
fi
exit "$fail"
