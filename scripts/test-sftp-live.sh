#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

: "${CPK_SFTP_HOST:?set CPK_SFTP_HOST}"
: "${CPK_SFTP_USER:?set CPK_SFTP_USER}"

PORT="${CPK_SFTP_PORT:-22}"
LIST_PATH="${CPK_SFTP_LIST_PATH:-.}"
ROUNDTRIP_DIR="${CPK_SFTP_ROUNDTRIP_DIR:-}"

echo "=== SFTP LIST PROBE ==="
cargo run -p cpk -- sftp-ls \
    "$CPK_SFTP_HOST" \
    "$CPK_SFTP_USER" \
    "$LIST_PATH" \
    "$PORT"

if [[ -z "$ROUNDTRIP_DIR" ]]; then
    echo
    echo "SFTP list probe passed."
    echo "Set CPK_SFTP_ROUNDTRIP_DIR to a writable remote directory for put/get verification."
    exit 0
fi

TMP="$(mktemp -d)"
REMOTE_NAME="cyber-pumpkin-live-$RANDOM-$RANDOM.bin"
REMOTE_PATH="${ROUNDTRIP_DIR%/}/$REMOTE_NAME"
LOCAL_SOURCE="$TMP/source.bin"
LOCAL_DESTINATION="$TMP/destination.bin"

cleanup() {
    rm -rf "$TMP"
}
trap cleanup EXIT

printf 'Cyber-Pumpkin SFTP live round trip\n' > "$LOCAL_SOURCE"

echo
echo "=== SFTP PUT ==="
cargo run -p cpk -- sftp-put \
    "$LOCAL_SOURCE" \
    "$CPK_SFTP_HOST" \
    "$CPK_SFTP_USER" \
    "$REMOTE_PATH" \
    "$PORT"

echo
echo "=== SFTP GET ==="
cargo run -p cpk -- sftp-get \
    "$CPK_SFTP_HOST" \
    "$CPK_SFTP_USER" \
    "$REMOTE_PATH" \
    "$LOCAL_DESTINATION" \
    "$PORT"

cmp "$LOCAL_SOURCE" "$LOCAL_DESTINATION"

echo
echo "=== SFTP CLEANUP ==="
cargo run -p cpk -- sftp-rm \
    "$CPK_SFTP_HOST" \
    "$CPK_SFTP_USER" \
    "$REMOTE_PATH" \
    "$PORT"

echo
echo "CYBER-PUMPKIN SFTP LIVE ROUND TRIP: PASS"
