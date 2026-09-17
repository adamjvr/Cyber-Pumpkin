# Durable Transactions Mega-Pass 19

Pass 19 turns the recovery substrate into an opt-in live transaction journal and
hardens the rollback semantics used by interrupted replacement operations.

## Included

- `BackupMoved` recovery now treats the original backup as authoritative until
  `Finalized` is durably recorded. A partial/new destination is removed and the
  original is restored.
- Reliable file transfers expose `ReliableFileRequest` with an optional durable
  recovery journal. Phase records are persisted around stage/final/backup rename
  boundaries.
- Reliable tree transfers expose `ReliableTreeRequest` with the same journal
  support. Interrupted tree replacement can roll back a partial destination.
- Journaled calls replay a leftover transaction at the supplied journal path
  before beginning a new transaction.
- Remote Edit uploads now use the journal-aware reliable-file API.
- Remote Edit workspace directories include the process id to avoid silently
  reusing another process's stale session directory.
- Linux remote-edit watchers share an application-shutdown cancellation token,
  and uploads receive that cancellation token instead of a fresh throwaway one.

## Compatibility

The existing `execute_file_reliable` and `execute_tree_reliable` functions remain
available and retain their old non-journaled call signatures. Existing callers
are not forced to allocate a journal path.

## Still intentionally deferred

- A global persisted recovery registry capable of reconstructing backend/profile
  identity and reconnecting every remote transaction automatically at startup.
- Per-session Remote Edit stop controls / Activity UI ownership of watcher
  handles. Pass 19 stops watchers on application shutdown but does not yet expose
  a user-facing stop button for one edit session.
- Native filesystem notification APIs; Remote Edit still polls/debounces.
- Inspector permission editing UI.

## Pass 19R2 lint refactor

The journal-aware tree executor is split into focused new-destination,
replacement-completion, and rollback helpers. `ReliableTreeRequest` is borrowed
by the public request API rather than passed by value. This keeps the strict
workspace Clippy gate clean without suppressing `too_many_lines` or
`needless_pass_by_value`.
