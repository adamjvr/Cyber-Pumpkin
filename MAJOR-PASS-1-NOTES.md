# Cyber-Pumpkin Major GUI Pass 1

Base: local HEAD c9f8566 with the uncommitted progress/cancellation milestone already applied.

Linux:
- modular GTK source split
- Places sidebar targeting the active pane
- active-pane tracking
- Refresh
- New Folder
- Show Hidden toggle
- per-pane selection/item footer
- Activity drawer
- transfer progress/cancel preserved
- completed/cancelled/failed transfer rows

macOS:
- first real native AppKit dual-pane browser
- Places sidebar
- native table columns
- path entry, Up, Refresh, Show Hidden, Activity
- local listings come from the shared Rust `cpk` companion
- menu skeleton is intentionally provisional until Transmit screenshots are captured

Native build lanes:
- scripts/build-native.sh dispatches Linux vs macOS
- scripts/run-macos.sh builds `cpk` and launches the AppKit shell
- GitHub Actions workflow builds Linux GTK and macOS AppKit in parallel

This pass intentionally does not move macOS filesystem behavior into Swift.
The current AppKit shell reads directory listings from `cpk local-ls`.
