# Cyber-Pumpkin Completion Roadmap

## Code-complete Local + SFTP release candidate

The current `0.9.0-rc.1` scope treats the reusable Rust SDK as the behavior owner and the GTK/AppKit applications as native presentation layers.

### Complete for RC

- Local and SFTP backends
- strict SSH host verification and agent authentication
- recursive transfer and recursive delete
- conflict policy and safe replacement
- progress, cancellation, bounded retries, scheduler
- durable recovery journals and local startup replay
- decisions, history, preferences, profiles, trust, and secrets abstractions
- sync planner/executor and reusable rules
- Linux Remote Edit lifecycle
- Linux Inspector permissions and async metadata
- Linux dual-pane daily-use shell
- macOS dual-pane daily-use shell through the `cpk` Rust boundary
- Local↔Local, Local↔SFTP, and SFTP↔SFTP macOS tree transfer
- macOS recursive delete and shared conflict/delete/keepalive preferences

### Promotion gates

`docs/RELEASE-CANDIDATE.md` is canonical. `1.0.0` requires live target-machine acceptance and packaging/signing.

### Post-v1 architecture work

- longer-lived connection pooling
- native filesystem notifications for Remote Edit
- reconnect-assisted remote journal recovery
- richer macOS queue/history/sync/Remote Edit parity
- drag/drop and tabs
- FTP/FTPS, WebDAV, and object providers
- File Provider/FUSE experiments
- updater/migration infrastructure

The invariant remains unchanged: Rust owns behavior; native code owns presentation and OS integration.
