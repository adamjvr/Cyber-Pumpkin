# Inspector Permissions + Remote Edit Controls — Pass 20

Pass 20 moves two previously hidden backend capabilities into the Linux product UI.

## Inspector permissions

- The Inspector no longer displays disabled placeholder permission boxes.
- A selected file/folder can load its POSIX mode through the backend-neutral
  `Backend::unix_mode` contract.
- User/group/world read/write/execute checkboxes are live.
- An octal field accepts `0000` through `7777`, including setuid/setgid/sticky
  bits that are not representable by the nine basic checkboxes.
- Apply uses `Backend::set_unix_mode` and immediately reads the value back for
  verification.
- The same UI path works for both Local and SFTP backends; capability checks are
  performed at runtime rather than branching on backend type.

Permission load/apply is intentionally user-triggered. SFTP connection setup is
currently synchronous, so selection changes do not automatically initiate a
network round trip on the GTK main thread.

## Remote Edit lifecycle

- Active Remote Edit watchers are now registered by transfer/session id.
- The Transfer menu exposes **Remote Edit Sessions…**.
- Each active session has an individual Stop control.
- A Stop All control cancels every watcher.
- Application shutdown uses the same registry, replacing the previous one-way
  global shutdown token.
- Watchers unregister themselves when they exit.
- Stopping a watcher deliberately leaves its local working copy intact.

## Deferred

- Automatic filesystem notifications instead of polling/debounce.
- Background/asynchronous Inspector metadata reads for SFTP.
- Creation/birth time in the common backend metadata model.
- Rich Remote Edit status integration into the Activity list.
