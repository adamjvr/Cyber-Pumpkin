#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

./scripts/verify.sh

echo
echo "=== NATIVE RELEASE BUILD ==="
case "$(uname -s)" in
  Linux)
    cargo build -p cyber-pumpkin-linux
    ;;
  Darwin)
    cargo build -p cpk
    swift build --package-path apps/cyber-pumpkin/macos
    ;;
  *)
    echo "unsupported release host: $(uname -s)" >&2
    exit 2
    ;;
esac

echo
echo "=== RELEASE DOCUMENTATION ==="
test -f docs/RELEASE-CANDIDATE.md
grep -q '0.9.0-rc.1' docs/RELEASE-CANDIDATE.md
grep -q 'Local + SFTP release candidate' README.md

echo
echo "CYBER-PUMPKIN RELEASE VERIFY: PASS"
