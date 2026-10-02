#!/usr/bin/env bash
# Package the manuals as a release asset.
#
#   ./scripts/make-manual-zip.sh            # -> dist/xoksa-manuals-<version>.zip
#
# The manuals used to be compiled into the engine and served at /manual/:slug,
# which meant editing a document changed the shipped binary's hash — and nothing
# in any UI linked to that surface anyway (removed in 2.9.4). They ship beside
# the app instead: one zip, readable offline, no binary involved.
#
# Contents: every manual (Markdown) plus the images they reference, keeping the
# `../images/<file>` relative paths intact so the links resolve after unzipping.
set -euo pipefail

cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[ -n "$version" ] || { echo "cannot read version from Cargo.toml" >&2; exit 1; }

out="dist/xoksa-manuals-${version}.zip"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

root="$stage/xoksa-manuals-${version}"
mkdir -p "$root/docs/manual" "$root/docs/images"
cp docs/manual/*.md "$root/docs/manual/"

# Only the images the manuals actually reference — docs/images also holds
# material for the design docs, which are not part of this package.
grep -ho '\.\./images/[A-Za-z0-9._-]*' docs/manual/*.md | sed 's|.*/||' | sort -u |
    while read -r img; do
        [ -f "docs/images/$img" ] && cp "docs/images/$img" "$root/docs/images/"
    done

mkdir -p dist
rm -f "$out"
(cd "$stage" && zip -qr "xoksa-manuals-${version}.zip" "xoksa-manuals-${version}")
mv "$stage/xoksa-manuals-${version}.zip" "$out"

printf 'make-manual-zip: %s (%s manuals, %s images, %s)\n' \
    "$out" \
    "$(ls docs/manual/*.md | wc -l | tr -d ' ')" \
    "$(ls "$root/docs/images" | wc -l | tr -d ' ')" \
    "$(du -h "$out" | cut -f1 | tr -d ' ')"
