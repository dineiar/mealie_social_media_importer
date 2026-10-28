#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
types=frontend/src/api/types
depth=$(awk -F/ '{ print NF }' <<<"$types")

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir "$work/fresh" "$work/committed"

TS_RS_EXPORT_DIR="$work/fresh" cargo test --locked --lib export_bindings

git -C .. archive HEAD -- "$types" | tar -x -C "$work/committed" --strip-components="$depth"

if ! diff -ru "$work/committed" "$work/fresh" >&2; then
	echo "$types differs from a fresh export of the Rust types." >&2
	echo "Delete the folder, run 'cargo test --lib export_bindings' in backend/, and commit the result." >&2
	exit 1
fi
