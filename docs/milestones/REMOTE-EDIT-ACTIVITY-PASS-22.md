# Remote Edit Activity — Pass 22

Pass 22 turns Remote Edit from a mostly console-observed background watcher into a first-class Activity subsystem.

## Added

- `HistoryKind::RemoteEdit` and `HistoryState::Active`.
- Active Remote Edit session rows are UI-transient so a restart cannot claim a dead watcher is still active.
- Worker-thread lifecycle events cross to GTK through a bounded, mutex-protected queue drained on the GTK main thread.
- Successful automatic uploads are persisted as completed Remote Edit activity.
- Manual Stop / Stop All actions are persisted as cancelled Remote Edit activity.
- Stale-remote detection is now a visible terminal conflict event; the watcher stops and the local working copy is retained.
- Other watcher/upload failures become visible terminal Activity failures rather than endless retry/eprintln loops.

## Safety properties

GTK objects never cross into watcher threads. The event queue contains only Send-friendly value data. The queue is bounded to 256 pending events. Remote-change conflicts remain fail-closed: no overwrite is attempted after the remote baseline diverges.
