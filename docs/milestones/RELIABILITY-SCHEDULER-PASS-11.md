# Reliability + Scheduler Pass 11

This milestone moves Cyber-Pumpkin closer to the internal-library separation
recovered during clean-room architecture research.

## Implemented

- `cyber-pumpkin-reliability`
  - operation-owned stage path
  - payload transfer through the controlled transfer engine
  - size verification before finalization
  - cancellation cleanup before finalization
  - backup of an existing destination
  - atomic stage-to-final rename
  - original-destination restore when finalization fails
  - backup cleanup diagnostics
- `cyber-pumpkin-scheduler`
  - simultaneous operation range 1..=20
  - default simultaneous ceiling 5
  - dependency-aware admission from `cyber-pumpkin-operations`
  - completion/failure/cancellation slot release
- `cyber-pumpkin-sync`
  - every file copy/replacement now uses the reliability layer
  - existing destination files are no longer overwritten in place
- `cpk local-replace-safe`
  - direct reliability-layer probe for local files

## Transaction invariant

For file replacement:

1. validate destination atomic-rename capability
2. reserve operation-owned stage and backup sibling paths
3. transfer source into stage
4. flush/close and verify stage size through transfer layer
5. honor cancellation before finalization
6. rename old destination to backup when present
7. rename stage to final destination
8. restore backup if finalization fails
9. remove backup after successful finalization

Cancellation never interrupts the rename transaction after step 6 begins.
