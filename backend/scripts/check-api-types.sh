#!/usr/bin/env bash
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
