# Major GUI Pass 7 — Pane-Local Behavior Reconstruction

Base commit: `83e3624` (`Rebuild native browser UI with inspector and server workspaces`).

This pass follows a second frame-by-frame review of the macOS reference screenshots
and screen recording. The goal is behavior parity in the clean-room architecture,
not visual asset copying.

## Primary correction

Browser, Pumpkin Patch, and Quick Connect are **pane-local states**. The left pane
and right pane can now independently show different content modes.

Each pane owns:

- Browser
- Pumpkin Patch
- Quick Connect

A successful saved or quick connection returns only that pane to Browser mode,
backed by SFTP. Home / Downloads / Desktop return only that pane to a local
browser.

## Linux interaction reconstruction

- removes the permanent Pumpkin Patch sidebar
- gives each pane a compact local-location strip
- gives each pane independent Pumpkin Patch and Quick Connect controls
- Pumpkin Patch rows use the reference-style name-left / address-right layout
- Pumpkin Patch has Shared Connections and History foundation rows
- bottom Pumpkin Patch controls provide add / group / edit affordances
- Add opens that pane's inline Quick Connect workspace
- Quick Connect includes protocol, display name, server, port, user, remote path,
  authentication status, Add to Pumpkin Patch, Cancel, and Connect
- saved connections open in the pane that activated them
- global Activity becomes an anchored popover instead of a persistent browser row
- global Inspector button toggles the right inspector pane
- global Sync button replaces browser content with a focused Sync Files workspace
- search dispatches to the currently active pane and filters Pumpkin Patch profiles
  when that pane is in Pumpkin Patch mode
- window title and search placeholder follow the active pane mode
- file dates use absolute timestamps instead of relative `7mo ago` text
- copy progress UI is hidden until a transfer starts; copy commands remain available
  through menus/context actions/keyboard

## Sync Files

The Sync Files screen is deliberately non-destructive in this milestone. It models
both endpoints and the main sync options observed in the reference UI, but
`Synchronize` remains disabled until the shared `cp-sync-plan` implementation can
produce a deterministic preview/execution plan.

## Preserved architecture

- Rust still owns filesystem behavior and transfer execution
- native GTK owns Linux presentation
- saved profiles remain non-secret JSON state
- SFTP still uses strict `known_hosts` verification and SSH-agent/OpenSSH auth
- no password is stored in connection profiles

## Deferred

- persistent connection history
- editable Pumpkin Patch groups
- saved-profile editing
- real sync planner/executor
- Inspector POSIX permission editing and ownership metadata
- exact local-time rendering instead of UTC-derived file timestamp formatting
- equivalent AppKit pane-local workspace reconstruction after this Linux pass is
  compiled and behavior-validated
