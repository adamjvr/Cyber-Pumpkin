# Decision + History Pass 12

This pass moves more application behavior out of native shells.

Implemented:
- `cyber-pumpkin-decisions`: frontend-neutral pending decisions, allowed choices,
  this-item vs all-matching scope, and session-remembered policies.
- `cyber-pumpkin-history`: bounded versioned JSON activity history with atomic save.
- Linux Activity now restores persisted history on launch.
- copy and sync completion/cancellation/failure events are persisted through the
  shared history library when Keep Activity is enabled.
- Linux gets Clear Activity.
- `cpk history-list` and `cpk history-clear` inspect the same history file.

The next decision pass should connect existing-item preferences, host-trust
prompts, delete confirmation, and sync type conflicts to `DecisionCenter`
instead of implementing those choices independently in each frontend.
