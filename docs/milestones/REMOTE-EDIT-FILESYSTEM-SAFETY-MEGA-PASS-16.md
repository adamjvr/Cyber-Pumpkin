# Remote Edit + Filesystem Safety Mega-Pass 16

Large vertical pass following Transfer + Connection Runtime Pass 15.

## Filesystem operations
- Adds `cyber-pumpkin-file-ops`.
- Backend-neutral depth-first recursive delete.
- Missing-root-safe recursive cleanup helper.
- Linux Delete now removes non-empty directory trees instead of failing on them.
- CLI gets `local-rm-tree`.

## Remote Edit
- Adds `cyber-pumpkin-remote-edit`.
- Deterministic per-session workspace paths.
- Initial remote download through the shared transfer runtime.
- Change detection uses size + mtime + full FNV-1a content fingerprint, so same-size/coarse-timestamp saves are detected.
- Changed working copies upload through `execute_file_reliable`.
- Linux gets `Edit Remote File`: download, launch with `xdg-open`, 1-second polling, 1-second debounce, reconnect and safe upload.
- CLI gets a local remote-edit probe.

## Recovery
- Adds `cyber-pumpkin-recovery`.
- Versioned atomic JSON recovery journal.
- Recovery phases: Staging, BackupMoved, Finalized.
- Conservative interrupted-replacement repair:
  - remove abandoned stages,
  - restore backups when destination vanished,
  - clean stale backup/stage sidecars after successful finalization.
- Recovery tests exercise journal persistence and backup restoration.

## Product behavior
- Remote edit remains restricted to regular files.
- Host trust/auth rules continue to come from pane connection state.
- Remote-edit uploads reuse the existing reliable replacement transaction.
- Linux recursive delete remains behind the existing confirmation/DecisionCenter flow.
