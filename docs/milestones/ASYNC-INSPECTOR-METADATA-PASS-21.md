# Async Inspector Metadata — Pass 21

Pass 21 removes synchronous backend metadata work from the Linux Inspector UI
path and adds backend-neutral creation/birth-time reporting.

## Backend contract

- `Backend::created_time` returns an optional Unix timestamp.
- The default implementation returns `None`; absence is a supported state, not
  an error and must not be synthesized from modification time.
- `LocalBackend` uses native filesystem creation/birth time when the host
  filesystem exposes it.
- SFTP currently inherits `None` because the SSH2/SFTP `FileStat` used by this
  implementation does not expose a portable creation/birth timestamp.

## Inspector

- Adds a **Created** metadata row.
- Selection immediately starts an asynchronous metadata request.
- Backend connection, creation-time lookup, Unix-mode lookup, chmod, and chmod
  verification execute on worker threads rather than GTK's main thread.
- The UI polls worker result channels from the GTK main loop, so GTK objects
  never cross thread boundaries.
- Every selection increments a generation counter. Results from an older
  selection are discarded instead of repainting the Inspector with stale SFTP
  metadata.
- **Load Mode** remains available as an explicit refresh but is now asynchronous.
- **Apply Mode** is asynchronous and still verifies the mode by reading it back.

## Safety invariants

- No GTK object is moved into a worker thread.
- No worker mutates Inspector state directly.
- Missing creation time is shown as unavailable rather than guessed.
- A selection change invalidates in-flight metadata and permission responses.

## Deferred

- Remote Edit lifecycle events in the Activity surface.
- Filesystem notifications/inotify for Remote Edit instead of polling.
- UID/GID, owner/group names, ACLs and extended attributes.
