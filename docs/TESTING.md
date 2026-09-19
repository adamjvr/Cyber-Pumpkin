# Testing

Cyber-Pumpkin uses a strict layered gate. A release candidate is not green because it merely compiles.

## Required automated gate

```bash
./scripts/verify.sh
```

This runs repository sanity checks, rustfmt, strict Clippy, the complete Rust workspace test suite, CLI smoke tests, and final diff checks.

## Native release gate

```bash
./scripts/verify-release.sh
```

On Linux this adds the GTK application build. On macOS it builds the Rust `cpk` companion and the AppKit Swift package.

## Covered invariants

- backend/domain validation
- local filesystem behavior and POSIX modes
- SFTP configuration and host-trust boundaries
- transfer state machine and recursive copy cleanup
- reliable replacement and recovery-journal phases
- bounded retry classification
- operation scheduling and dependency behavior
- sync planning/execution and destructive-operation guardrails
- Remote Edit local-change / conflict behavior
- persistent preferences, profiles, history, and trust models

## Required live acceptance

Automated CI intentionally does not depend on private SSH credentials. Before `1.0.0`, run the target-machine checks in `docs/RELEASE-CANDIDATE.md`, including live SFTP, conflict, cancellation, recovery, and GUI acceptance on both Linux and macOS.

## Fault-injection work after 1.0

The existing correctness model is designed for future systematic injection of disconnects, short I/O, destination-full conditions, server restarts, and cancellation at each lifecycle phase.
