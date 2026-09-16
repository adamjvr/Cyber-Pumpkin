# Cyber-Pumpkin Completion Roadmap

Cyber-Pumpkin is being completed as a family of reusable internal libraries
with thin native frontends, rather than as one monolithic GUI application.

## P10 — Runtime integration

- real sync preview and execution
- operations-backed sync activity
- CLI sync probes

## P11 — Reliability and decisions

Internal libraries:

- transfer staging + verification + atomic finalize
- destination conflict decision requests
- bounded retry policy
- scheduler capacity / connection-aware concurrency
- pause/resume where backend capabilities permit it
- exact cleanup ownership on failure/cancel

Native integration:

- conflict sheets/dialogs
- queue controls
- retry / pause / resume / cancel

## P12 — Accounts, secrets, trust, and history

Internal libraries:

- persistent transfer/connection history
- secret-store abstraction
- trust-decision model
- connection pool / keepalive / bounded redial

Platform adapters:

- macOS Keychain
- Linux Secret Service
- native trust and credential prompts

## P13 — Full synchronization system

- persisted sync presets
- reusable rules editor backed by `cyber-pumpkin-rules`
- preview filters and conflict resolution
- scheduled/manual execution model
- profile-specific delete/orphan policy
- equivalent preview/execution plan guarantees

## P14 — File-management depth

- multi-selection
- batch rename
- drag/drop
- richer metadata and POSIX permissions
- recursive delete decision flow
- tags/labels where supported
- checksums
- trash adapters

## P15 — Remote editing

Internal library owns:

- temporary workspace
- download/open/watch/debounce/upload lifecycle
- conflict detection
- cleanup and recovery

Native shells own editor selection and OS launch integration.

## P16 — Provider expansion

Providers remain independent crates behind `cyber-pumpkin-backend`.
Candidate order after Local/SFTP reliability is sealed:

1. FTP / FTPS
2. WebDAV
3. object/cloud providers required by the product scope

Provider-specific behavior must not leak into pane/application state.

## P17 — Native parity

- AppKit consumes the same sync/operations/history/secrets behavior
- Linux GTK4/libadwaita feature parity
- native keyboard/menu/context-menu parity
- accessibility and high-DPI validation

## P18 — Ship gates

- deterministic stress harnesses
- network loss and reconnect tests
- cancellation/crash cleanup tests
- multi-instance tests
- packaging/signing/notarization on macOS
- Linux packaging
- updater abstraction
- migration/versioning guarantees for profiles/preferences/history

The invariant remains: Rust owns behavior; native code owns presentation and
OS integration.
