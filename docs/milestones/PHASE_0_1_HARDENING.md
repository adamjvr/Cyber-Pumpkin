# Phase 0.1 — Bootstrap Hardening

Status: delivery milestone

## Why this exists

The first real Linux validation of Phase 0 proved the core tests and CLI smoke path, but exposed three bootstrap defects before Phase 1:

- `cargo fmt --all --check` reported formatting drift in the transfer state machine.
- strict Clippy rejected missing `# Errors` API documentation and the boolean-heavy capability structure.
- `Cargo.lock` was generated locally but was not included in the Phase 0 commit.

The original delivery command also continued to commit and push after validation failures. That behavior is unacceptable for future milestones.

## Changes

- format the transfer crate to Rustfmt output;
- document all public fallible APIs and error fields required by the configured lints;
- replace boolean capability flags with explicit `CapabilitySupport` values;
- add and track the application workspace `Cargo.lock`;
- add `scripts/verify.sh` as the canonical fail-fast validation entry point;
- make CI call the same verifier used locally;
- document lockfile and fail-fast milestone policy.

## Acceptance gate

```bash
./scripts/verify.sh
git status --short
```

`verify.sh` must report `CYBER-PUMPKIN VERIFY: PASS`. After the milestone commit, `git status --short` must be empty.

Phase 1 does not begin from a red or dirty Phase 0 baseline.
