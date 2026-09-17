# Metadata + Sync Safety Mega-Pass 17

- Backend Unix mode read/write primitives.
- Local chmod/stat mode support.
- SFTP mode read/write through lstat/setstat.
- `cpk local-mode` and `cpk local-chmod`.
- Sync type-conflict replacement now uses shared reliable tree replacement.
- Old delete-first conflict replacement removed.

This pass does not yet claim fully crash-journaled tree replacement; Pass 16 recovery is the foundation for that wiring.
