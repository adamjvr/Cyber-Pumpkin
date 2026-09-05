# Documentation Index

- [Architecture](ARCHITECTURE.md) — ownership boundaries and major components.
- [Behavior Matrix](BEHAVIOR_MATRIX.md) — clean-room reference behavior tracking.
- [Coding Standards](CODING_STANDARDS.md) — mandatory engineering rules.
- [Local Backend](LOCAL_BACKEND.md) — native filesystem semantics and path policy.
- [Roadmap](ROADMAP.md) — milestone sequence.
- [Security](SECURITY.md) — threat model and secret-handling rules.
- [SFTP Backend](SFTP.md) — SSH/SFTP connection, host-key, and authentication policy.
- [Testing](TESTING.md) — validation layers and required gates.
- [Transfer Engine](TRANSFER_ENGINE.md) — lifecycle and execution invariants.

## Milestones

- [Phase 0.1 — Bootstrap Hardening](milestones/PHASE_0_1_HARDENING.md) — closes the first Linux format/lint/lockfile validation findings and establishes fail-fast verification.
- [Phase 1A — Local + SFTP Core](milestones/PHASE_1A_LOCAL_SFTP_CORE.md) — delivers the first real local/SFTP browse and regular-file transfer path through the shared core.
