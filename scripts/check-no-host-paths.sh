#!/usr/bin/env bash
# Fail if a shipped artifact discloses who built it.
#
# rustc records the source path of every crate it compiles so a panic can name a
# file. On a developer machine those paths run through the home directory, so a
# release binary ends up carrying the builder's account name — 689 occurrences of
# C:\Users\<name> in the 2.9.9 engine, and 85 in the WASM the browser downloads.
# `--remap-path-prefix` removes most but not all of them: the paths baked into the
# pre-built `std` rlibs rustup ships, and the ones a C toolchain (`aws-lc-sys`)
# embeds through cc-rs, are outside rustc's reach. The reliable answer is to build
# where the account name discloses nothing, which is what CI is for.
#
# So this gate does not ask "are there paths?" — there always are. It asks
# "whose?", and accepts only the known build accounts.
#
# Usage: scripts/check-no-host-paths.sh <artifact> [artifact...]
# Exit 0 when every artifact is clean, 1 otherwise (naming what it found).
set -uo pipefail

# Build accounts that identify nobody. GitHub's Windows runners use
# `runneradmin`; its Linux/macOS runners and containers use the others.
#
# RUNNER~1 is the same account under its 8.3 short name. It has to be listed
# separately because a C toolchain writes paths in that form: the first CI
# build reached this gate with the engine clean except for 45 short-name paths
# from aws-lc-sys, while the other four artifacts had none. Short names appear
# only where Windows generates them, so listing the known ones is exact; a
# pattern for `*~1` would accept any account whose name happens to collide.
ALLOWED="runneradmin RUNNER~1 RUNNERA~1 runner ContainerAdministrator ci build"

# One home-directory path, in the three forms a build can leave behind. The
# backslash sits inside a bracket expression on purpose: POSIX ERE takes it
# literally there, while `\\` outside one is read inconsistently across greps.
PATH_RE='([A-Za-z]:[/\]Users[/\]|/Users/|/home/)[A-Za-z0-9_.~-]+'

fail=0
scanned=0

for f in "$@"; do
    if [ ! -f "$f" ]; then
        printf 'check-no-host-paths: no such file: %s\n' "$f" >&2
        fail=1
        continue
    fi
    scanned=$((scanned + 1))

    # -a treats the binary as text. Dropping NUL bytes first turns a UTF-16LE
    # copy of the same path into the ASCII run this pattern matches, so the
    # Windows resource strings are covered by the one scan.
    hits="$(LC_ALL=C tr -d '\000' < "$f" | LC_ALL=C grep -aoE "$PATH_RE")"
    names="$(printf '%s\n' "$hits" | LC_ALL=C sed -E 's#^.*[/\]##' \
             | LC_ALL=C sort -u | LC_ALL=C sed '/^$/d')"

    bad=""
    for n in $names; do
        allowed=0
        for a in $ALLOWED; do
            if [ "$n" = "$a" ]; then allowed=1; break; fi
        done
        if [ "$allowed" -eq 0 ]; then bad="$bad $n"; fi
    done

    if [ -n "$bad" ]; then
        printf 'NG  %s discloses the build account:%s\n' "$f" "$bad" >&2
        for n in $bad; do
            c="$(printf '%s\n' "$hits" | LC_ALL=C grep -c "[/\]${n}\$" || true)"
            printf '      %-20s %s occurrence(s)\n' "$n" "${c:-0}" >&2
        done
        fail=1
    else
        printf 'OK  %s carries no identifying build path\n' "$f"
    fi
done

if [ "$scanned" -eq 0 ]; then
    printf 'check-no-host-paths: nothing to scan\n' >&2
    exit 1
fi

if [ "$fail" -ne 0 ]; then
    printf '\ncheck-no-host-paths: build it on CI (see .github/workflows/release-windows.yml).\n' >&2
    exit 1
fi
exit 0
