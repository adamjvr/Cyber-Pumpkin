# Local Backend

The local backend maps the common filesystem contract onto `std::fs` for macOS and Linux.

## Phase 1A operations

- directory listing
- symlink-aware metadata
- sequential read
- create/truncate sequential write
- single-directory create
- rename/move within the host filesystem
- remove file/link/empty directory

Directory listings are sorted by display name so the same backend call is deterministic for CLI/tests. The GUI may apply its own user-selected sort policy to snapshots later.

## Path policy

The current common domain path is UTF-8. Local paths that cannot be represented as UTF-8 are rejected rather than lossily rewritten. This protects mutations from targeting a path different from the one the user selected.

A later path-model ADR can introduce opaque native path bytes if full non-UTF-8 Unix filename support is required.

## Capabilities

Phase 1A advertises atomic rename and Unix permissions. Resume is intentionally not advertised until the common backend API includes positioned read/write semantics and tests.
