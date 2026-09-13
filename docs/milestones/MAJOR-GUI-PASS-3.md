# Major GUI Pass 3 — Recursive Transfer, Context Menus, Info and Sorting

Base commit: `c9d66fb`.

## Shared Rust transfer engine

- adds `recursive.rs`
- one API copies either a regular file or a directory tree
- destination root must not already exist
- pre-enumerates the first recursive plan to produce stable aggregate progress
- reports files, directories, bytes and overall permille
- reuses the controlled file executor for every leaf
- cancellation removes completed files and created directories from the tree
- unsupported symlinks/special entries fail explicitly
- unit tests cover nested-tree parity and cancellation cleanup

This is intentionally the first recursive implementation. The future large-tree
milestone replaces pre-enumeration with incremental spooling without changing
the frontend-facing contract.

## Linux GTK

- Copy buttons now accept folders as well as files
- aggregate recursive progress
- recursive cancellation cleanup
- context menu: Copy to Other Pane / Get Info / Rename / Delete
- Get Info dialog
- Sort by Name / Type / Size
- Reverse sort
- Transfer submenu
- Copy-to-other-pane accelerator and app action
- directories remain grouped before files in normal ascending sorts

## macOS AppKit

- recursive safe file/folder copy through `cpk`
- right-click context menus
- Get Info
- sort by Name / Type / Size
- Reverse sort
- expanded File / View / Transfer menu parity
- Pumpkin Patch remains canonical terminology

## cpk companion

Repairs the Pass 2 staging omission and exposes:

- `local-copy-safe`
- `local-copy-tree-safe`
- `local-mkdir`
- `local-rename`
- `local-rm`

The AppKit frontend continues to delegate filesystem and transfer behavior to
Rust through this temporary process boundary.

## Deferred

- recursive symlink policy
- non-empty recursive delete
- incremental recursive spooling
- overwrite/conflict decision UI
- transfer queue with multiple simultaneous jobs
- persistent Pumpkin Patch favorites
- exact Transmit menu ordering pending screenshot inventory
