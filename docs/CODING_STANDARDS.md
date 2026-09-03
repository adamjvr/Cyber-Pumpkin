# Coding Standards

These rules are project policy, not suggestions.

## 1. Architectural ownership

- Product behavior lives in shared core modules.
- Swift/AppKit/SwiftUI and GTK are adapters/consumers.
- Never hide transfer, retry, synchronization, persistence, or security behavior inside UI callbacks.
- Protocol-specific behavior remains behind backend interfaces.
- Do not add platform conditionals to common logic when a capability model can express the difference.

## 2. Correctness before convenience

- No silent fallbacks for corrupting or security-sensitive operations.
- Errors cross boundaries explicitly and with actionable context.
- Destructive operations must be modeled and previewable where practical.
- Cancellation is a state transition, not thread/process destruction.
- Partial destination files must never masquerade as completed output.

## 3. Rust

- Stable Rust only unless a documented ADR approves otherwise.
- `unsafe` is forbidden in workspace crates. Put unavoidable FFI `unsafe` in a tiny audited boundary crate later.
- `cargo fmt` is mandatory.
- `cargo clippy -- -D warnings` must pass.
- Production code may not use `unwrap`, `expect`, `panic!`, `todo!`, or `unimplemented!`.
- Prefer domain types over strings/integers for identifiers, paths, protocol state, and units.
- Public APIs document invariants and ownership.
- Keep dependency count deliberate; every network/security dependency needs rationale.

## 4. Swift / macOS

- AppKit owns desktop-native file-browser interaction where it is stronger than SwiftUI.
- SwiftUI may compose settings, inspectors, and appropriate shell surfaces.
- UI code maps core state to presentation; it does not recreate the core state machine.
- No force unwraps in production paths.
- Main-thread work must stay bounded; enumeration and transfer work never block the UI thread.

## 5. GTK / Linux

- GTK4 is the native shell; prefer standard desktop interactions and portals.
- No blocking network/filesystem work on the GTK main context.
- Platform integration stays in the platform tree unless generally reusable.

## 6. Naming

- Product: `Cyber-Pumpkin`.
- CLI: `cpk`.
- Rust crates: `cyber-pumpkin-*`.
- Types describe domain concepts, not implementation accidents.
- Avoid ambiguous names like `Manager`, `Helper`, `Util`, or `Thing` when a precise role exists.

## 7. Commits

- One logical change per commit.
- Commit messages are imperative and specific.
- Never mix broad formatting churn with behavioral changes.
- Do not rewrite shared history without an explicit reason.
- Push completed logical work; do not leave important project state only on one workstation.

## 8. Repository safety

Automation and instructions must not casually invoke destructive cleanup such as `git reset --hard`, `git clean -fdx`, or deleting unknown user files. Build scripts may remove only paths they created and own.

## 9. Documentation

A behavioral or architectural change is incomplete until the relevant document changes with it. Documentation is organized by purpose and must remain navigable from the root README.
