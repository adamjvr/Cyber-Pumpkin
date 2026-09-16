# Runtime Integration Pass 10

This milestone turns the synchronization screen from a placeholder into a real
consumer of the internal SDK.

## Implemented

- new `cyber-pumpkin-sync` execution library
- immutable `cyber-pumpkin-sync-plan` preview is executed without re-planning
- execution supports create-directory, copy/replace-file, orphan removal,
  explicit skip actions, conflict rejection, cancellation, progress, and reports
- Linux Sync Files screen now:
  - selects Left → Right or Right → Left
  - previews the exact plan
  - displays action-by-action changes
  - blocks execution when conflicts exist
  - executes the plan through `cyber-pumpkin-sync`
  - drives `cyber-pumpkin-operations` state/progress
  - publishes completed/cancelled/failed syncs into Activity
  - refreshes the destination pane after execution
- `cpk` gains local sync planning and execution probes
- sync execution has local integration tests

## Reliability boundary

New destination files use the controlled transfer path and are cancellable
mid-file with partial cleanup. Replacing an existing file still uses the
transfer layer's direct replacement path. The next reliability milestone will
replace this with staging + verification + atomic finalize so cancellation or
failure never destroys an existing destination.

## Architectural invariant

The GTK frontend does not implement synchronization semantics. It asks the
planner for an immutable plan, renders that plan, and hands the exact same plan
to the sync execution library. Operation state belongs to the operations SDK.
