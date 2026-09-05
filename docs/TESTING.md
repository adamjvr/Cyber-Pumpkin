# Testing

Testing is layered around behavior and invariants, not implementation trivia.

## Required gate

```bash
./scripts/verify.sh
```

The verifier runs formatting, strict Clippy, all workspace tests, CLI smoke tests, and diff checks. It is fail-fast; a non-zero result means the milestone is not eligible to commit or push.

## Phase 1A automated coverage

- backend/domain validation
- local list/stat behavior
- transfer state machine
- local regular-file copy through the real backend contract
- destination-size verification path
- SFTP configuration validation without requiring network access
- local CLI browse/stat smoke tests

## SFTP live test

Network tests are opt-in because normal CI must not depend on a private SSH server or user credentials.

```bash
export CPK_SFTP_HOST=server.example.com
export CPK_SFTP_USER=adam
export CPK_SFTP_LIST_PATH=/tmp
./scripts/test-sftp-live.sh
```

For a writable round trip:

```bash
export CPK_SFTP_ROUNDTRIP_DIR=/tmp
./scripts/test-sftp-live.sh
```

The live harness lists the server, uploads a unique temporary object, downloads it, verifies exact bytes with `cmp`, and removes the remote test object. It uses the user's SSH agent and existing `known_hosts` file.

## Contract suite target

Every backend must eventually pass the same filesystem contract suite: list/stat/read/write/rename/delete, errors, Unicode names, empty files, large files, timestamps, cancellation, and declared capabilities.

## Fault injection target

- disconnect during upload/download
- short read/write
- permission change mid-transfer
- destination fills up
- rename failure
- checksum mismatch
- server restart
- cancellation at every lifecycle phase

## Cross-platform

Behavioral fixtures are shared. Native shells additionally test platform interactions, drag/drop, keyboard navigation, secret storage, and lifecycle behavior.

## Performance

Benchmarks measure throughput, CPU, memory, enumeration latency, cancellation latency, and scaling across worker counts. Performance changes never weaken correctness invariants just to improve a benchmark.
