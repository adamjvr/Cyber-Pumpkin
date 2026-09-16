# Transfer + Connection Runtime Mega-Pass 15

This is a large vertical integration pass rather than a single-subsystem patch.

## Transfer runtime
- Adds `cyber-pumpkin-transfer-runtime`.
- Ordinary file and directory copies now share explicit conflict policy:
  Fail, Replace, Skip, Keep Both.
- Keep Both deterministically finds `name copy`, `name copy 2`, etc.
- File **and directory-tree** replacement use the reliability layer.
- Existing directory trees are moved aside before replacement and restored on
  cancellation/failure.
- Replacement cleanup recursively removes operation-owned backups.

## Linux operation scheduling
- Copy operations enter the shared `OperationQueue`.
- `OperationScheduler` uses the user's simultaneous-transfer preference (1..=20).
- Multiple transfers can execute concurrently.
- Excess copies remain queued and are admitted when slots free.
- Cancel All cancels active jobs and removes queued jobs.
- Activity entries use actual operation IDs and terminal scheduler state.
- Progress updates flow into the shared operation queue.

## Existing-item decisions
- Upload/download file/folder preferences are now active behavior.
- Ask invokes shared `DecisionCenter`.
- Replace / Skip / Keep Both / Cancel are supported.
- Matching decisions can be remembered for the current session.

## SSH connection resilience
- SSH keepalive defaults to 60 seconds.
- Keepalive obeys the Advanced preference.
- Connection setup performs up to two bounded redials for transport/protocol
  failures.
- Authentication and host-key failures remain terminal and are never retried.
- SFTP exposes keepalive/redial configuration.

## Sync rule correctness
- Destination objects excluded by a rule are protected from
  delete-orphans cleanup.
- This closes the source-rule/orphan-deletion semantic hole.

## CLI / validation
- Adds `cpk copy-runtime-local ... --conflicts=fail|replace|skip|keep-both`.
- New runtime tests cover reliable directory replacement, Keep Both, and Skip.
- Existing full verify gate remains the release gate.
