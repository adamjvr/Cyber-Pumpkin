#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

case "$(uname -s)" in
  Linux)
    cargo build -p cyber-pumpkin-linux
    ;;
  Darwin)
    cargo build -p cpk
    swift build --package-path apps/cyber-pumpkin/macos
    ;;
  *)
    echo "unsupported host: $(uname -s)" >&2
    exit 2
    ;;
esac
