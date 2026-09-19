# Roadmap

## Local + SFTP release candidate

### Core behavior — implemented

- [x] backend-neutral filesystem contract
- [x] native local backend
- [x] SFTP backend with strict host verification
- [x] recursive copy and recursive delete
- [x] safe replacement / atomic finalization
- [x] progress and cooperative cancellation
- [x] bounded retry/backoff
- [x] metadata / POSIX permission hooks
- [x] conflict decisions: Replace / Skip / Keep Both
- [x] operation scheduler and Activity/history model
- [x] durable replacement journals and local startup recovery
- [x] sync planner/executor and reusable rules
- [x] Remote Edit lifecycle and remote-change protection
- [x] persistent non-secret connection profiles
- [x] platform secret-store/trust abstractions

### Native daily-use shells — implemented

- [x] Linux dual-pane browser
- [x] macOS dual-pane browser
- [x] Local↔SFTP recursive transfers on both platforms
- [x] SFTP↔SFTP transfer through the shared Rust companion boundary
- [x] saved Pumpkin Patch connections
- [x] create / rename / recursive delete
- [x] native menus and keyboard commands
- [x] shared conflict preferences
- [x] Inspector / Activity surfaces
- [x] asynchronous Linux destination preflight
- [x] cancellation-responsive retry waiting

### Release signoff — must be run on target machines

- [ ] Linux live SFTP upload/download/tree-copy acceptance
- [ ] macOS live SFTP upload/download/tree-copy acceptance
- [ ] conflict matrix: Replace / Skip / Keep Both on files and folders
- [ ] cancellation and crash-recovery acceptance
- [ ] Remote Edit conflict/cancel acceptance on Linux
- [ ] accessibility / HiDPI visual pass
- [ ] packaging, signing, and notarization artifacts

## Post-v1

- FTP / FTPS
- WebDAV
- S3-compatible object storage
- richer long-lived connection pooling
- native filesystem-event Remote Edit watcher
- reconnect-assisted remote recovery
- tabs and additional desktop drag/drop integrations
- File Provider / FUSE exploration
- updater and migration tooling
