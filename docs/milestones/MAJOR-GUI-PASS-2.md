# Major GUI Pass 2 — File Operations and Menus

This pass assumes Major GUI Pass 1 has been committed and the working tree is clean.

Linux adds Rename/Delete toolbar controls and dialogs, a native GTK app menu with provisional File/View/Go/Help submenus, and keyboard accelerators.

macOS adds New Folder, Rename, Delete, safe Copy, Pumpkin Patch navigation, and expanded native Cyber-Pumpkin/File/View/Go/Transfer/Help menus.

The `cpk` companion gains `local-copy-safe`, `local-mkdir`, `local-rename`, and `local-rm` so AppKit continues to delegate filesystem behavior to Rust.

The menu hierarchy remains provisional until the clean-room Transmit screenshot inventory is captured.
