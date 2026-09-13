# Major GUI Pass 4 — Real SFTP Panes

This milestone turns the backend-neutral browser work into a real remote-file workflow.

## Linux

- active pane can switch between Local and SFTP
- Connect SFTP dialog: host, username, port and initial remote path
- strict `known_hosts` verification remains enforced
- SSH-agent authentication remains the first supported auth path
- browser navigation, refresh, New Folder, Rename, Delete, Get Info and sorting now use the active pane backend
- recursive copy engine now accepts Local→Local, Local→SFTP, SFTP→Local and SFTP→SFTP backend pairs
- transfer progress/cancellation remains shared Rust behavior
- Connect / Local controls are visible in the toolbar and File menu

## Shared backend reliability

SFTP status 2 is translated to the common `NotFound` error and status 3 to
`PermissionDenied`. This is required for safe destination preflight and
recursive transfer cleanup to work with SFTP rather than treating a missing
remote destination as a protocol failure.

## cpk companion

Adds:

- `sftp-copy-tree-put`
- `sftp-copy-tree-get`
- `sftp-mkdir`
- `sftp-rename`

These keep the AppKit process boundary backed by the same Rust filesystem and
recursive transfer behavior.

## macOS source lane

- pane model can switch Local/SFTP
- Connect / Local controls
- SFTP browse/new-folder/rename/delete through `cpk`
- recursive Local↔SFTP copy through `cpk`
- strict SSH-agent + known-host behavior is inherited from the Rust backend

The AppKit changes must still be compiled on macOS/CI before they are called green.

## Deliberately deferred

- persistent Pumpkin Patch saved connections
- Secret Service / Keychain credential integration
- connection pooling and keepalive
- async connection establishment
- host-key trust decision UI
- password authentication
- SFTP→SFTP transfer through the AppKit `cpk` process boundary
