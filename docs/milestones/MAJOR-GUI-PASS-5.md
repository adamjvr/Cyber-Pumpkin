# Major GUI Pass 5 — Inspector, Pumpkin Patch Profiles, Preferences

Base commit: `26140aa` (`Add SFTP panes and remote transfer workflow`).

This pass is driven by the macOS reference screenshots and recording captured
before implementation. It adopts the useful interaction hierarchy without
copying source code or proprietary visual assets.

## Shared Rust application model

- versioned JSON `ConnectionProfiles`
- non-secret saved SFTP profile metadata
- atomic profile-store writes
- platform-appropriate default application-support/config path
- shared `AppPreferences`
- file preferences
- transfer conflict preferences
- simultaneous-transfer preference
- advanced keepalive/logging preferences
- unit tests for profile upsert/remove and conservative defaults

Secrets remain outside connection profiles. Current SFTP authentication still
uses the SSH agent and strict `known_hosts` verification.

## Linux

- persistent Inspector sidebar
- selection-driven file metadata
- Pumpkin Patch now has:
  - Local Locations
  - saved Connections
  - add/remove saved connection controls
  - History placeholder/foundation
- saved profiles can connect the active pane
- Preferences window with native pages:
  - General
  - Files
  - Transfers
  - Rules
  - Keys
  - Advanced
- preference controls save to the shared Rust JSON model
- existing SFTP and recursive transfer behavior remains unchanged

## macOS source lane

- persistent AppKit Inspector
- saved Pumpkin Patch connections via `cpk`
- optional Save in Pumpkin Patch during SFTP connection
- native Preferences window shell matching the same functional taxonomy
- selection changes update Inspector
- shared connection-profile storage remains owned by Rust through `cpk`

The macOS preferences controls are currently a native shell and do not yet
write the Rust preferences file. The profile sidebar does use the shared Rust
profile boundary. AppKit compilation still requires macOS/CI validation.

## `cpk`

Adds:

- `profile-list`
- `profile-add`
- `profile-rm`

This lets native non-Rust frontends consume the shared profile store without
reimplementing profile persistence.

## Deliberately deferred

- Servers / Quick Connect as full pane content mode
- connection History persistence
- expanded metadata model for modification dates / permissions / ownership
- conflict decision dialog wired to transfer preferences
- persistent AppKit preference editing through `cpk`
- connection pooling / keepalive implementation
- SSH key-management UI
- reusable transfer/sync rule evaluator
