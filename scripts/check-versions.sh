#!/usr/bin/env bash
# Every place that states the current version must state the one in Cargo.toml.
# A version bump that misses one of them fails here, before it reaches main:
# 2.9.12 shipped with README and the command reference still titled v2.9.11,
# and with CHANGELOG still calling 2.9.12 unreleased after it was published.
#
#   ./scripts/check-versions.sh
set -uo pipefail

cd "$(dirname "$0")/.."

# Read without CR, so a CRLF checkout gives the same result as an LF one.
rd() { tr -d '\r' < "$1"; }

want=$(rd Cargo.toml | sed -n 's/^version = "\(.*\)"$/\1/p' | head -1)
[ -n "$want" ] || { echo "cannot read version from Cargo.toml" >&2; exit 1; }
printf 'Cargo.toml: %s\n' "$want"

bad=0
expect() { # expect "<where>" "<found>"
    if [ "$2" = "$want" ]; then
        printf '  ok   %s\n' "$1"
    else
        printf '  FAIL %s: %s (expected %s)\n' "$1" "${2:-<missing>}" "$want"
        bad=1
    fi
}
expect_count() { # expect_count "<where>" <expected> <found>
    if [ "$3" -ne "$2" ]; then
        printf '  FAIL %s: %s found, expected %s\n' "$1" "$3" "$2"
        bad=1
    fi
}

for f in webui-leptos/Cargo.toml xoksa-desktop/Cargo.toml xoksa-setup/Cargo.toml; do
    expect "$f" "$(rd "$f" | sed -n 's/^version = "\(.*\)"$/\1/p' | head -1)"
done
for f in xoksa-desktop/tauri.conf.json xoksa-setup/tauri.conf.json; do
    expect "$f" "$(rd "$f" | sed -n 's/^  "version": "\(.*\)",$/\1/p' | head -1)"
done

lock() { # lock <Cargo.lock> <crate>: the version recorded for this project's own crate
    rd "$1" | awk -v n="$2" '$0 == "name = \"" n "\"" { getline; sub(/^version = "/, ""); sub(/"$/, ""); print; exit }'
}
expect "Cargo.lock (xoksa)" "$(lock Cargo.lock xoksa)"
expect "webui-leptos/Cargo.lock (xoksa-webui)" "$(lock webui-leptos/Cargo.lock xoksa-webui)"
expect "xoksa-desktop/Cargo.lock (xoksa-desktop)" "$(lock xoksa-desktop/Cargo.lock xoksa-desktop)"
expect "xoksa-setup/Cargo.lock (xoksa-setup)" "$(lock xoksa-setup/Cargo.lock xoksa-setup)"

# Each document has one title in English and one in Japanese. Both must be
# present, and both must name the current version.
titles() { # titles <file> <count> <title regex before " vX.Y.Z">
    local found v
    found=$(rd "$1" | grep -E "^($3) v[0-9]+\.[0-9]+\.[0-9]+$" | sed 's/.* v//')
    expect_count "$1 titles" "$2" "$(printf '%s' "$found" | grep -c .)"
    while read -r v; do
        [ -n "$v" ] && expect "$1 title" "$v"
    done <<< "$found"
}
titles README.md 2 '# xoksa'
titles docs/manual/command-reference.md 2 '# xoksa Command-Line Reference|# xoksa コマンドラインリファレンス'

# CHANGELOG has an entry for the current version in each language. Once the
# version is tagged, neither heading may still call it unreleased.
heads=$(rd CHANGELOG.md | grep -F "## [$want]")
expect_count "CHANGELOG.md [$want] headings" 2 "$(printf '%s' "$heads" | grep -c .)"
if git rev-parse -q --verify "refs/tags/v$want" >/dev/null 2>&1; then
    if printf '%s\n' "$heads" | grep -q -E 'unreleased|未リリース'; then
        printf '  FAIL CHANGELOG.md: v%s is tagged, but its heading still says unreleased\n' "$want"
        bad=1
    else
        printf '  ok   CHANGELOG.md headings (v%s is tagged)\n' "$want"
    fi
else
    printf '  note tag v%s is not present in this checkout; the released-heading check did not run\n' "$want"
fi

exit "$bad"
