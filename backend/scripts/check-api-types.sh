#!/usr/bin/env bash
# Fails when the committed files in frontend/src/api/types differ from a fresh export.
# It deletes the directory first so the files of removed types show up too. A plain
# `cargo test` rewrites the same files, so this compares against git, not the working tree.
set -euo pipefail

cd "$(dirname "$0")/.."
types=../frontend/src/api/types

rm -rf "$types"
cargo test --locked --lib export_bindings

if [ -n "$(git status --porcelain --untracked-files=all -- "$types")" ]; then
	git status --short --untracked-files=all -- "$types" >&2
	git diff HEAD -- "$types" >&2
	echo "frontend/src/api/types differs from a fresh export of the Rust types." >&2
	echo "It now holds the fresh files. Review and commit them." >&2
	exit 1
fi
