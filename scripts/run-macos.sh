#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo build -p cpk
export CPK_BIN="$ROOT/target/debug/cpk"
swift run --package-path apps/cyber-pumpkin/macos CyberPumpkinMac
