#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "=== CYBER-PUMPKIN VERIFY ==="

for tool in rustc cargo git; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "error: required tool not found: $tool" >&2
        exit 1
    fi
done

rustc --version
cargo --version

echo
echo "=== REPOSITORY SANITY ==="
git diff --check

if [[ ! -f Cargo.lock ]]; then
    echo "error: Cargo.lock is required for this application workspace" >&2
    exit 1
fi

echo
echo "=== FORMAT ==="
cargo fmt --all --check

echo
echo "=== CLIPPY ==="
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo
echo "=== TEST ==="
cargo test --workspace --all-features

echo
echo "=== CLI SMOKE ==="
cargo run --quiet -p cpk -- about
cargo run --quiet -p cpk -- local-ls . >/dev/null
cargo run --quiet -p cpk -- local-stat Cargo.toml >/dev/null

echo
echo "=== FINAL DIFF CHECK ==="
git diff --check

echo
echo "CYBER-PUMPKIN VERIFY: PASS"
