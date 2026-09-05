# Roadmap

## Phase 0 — Foundation

- [x] Architecture boundaries
- [x] Coding standards
- [x] Transfer lifecycle model
- [x] Test/security specifications
- [x] CLI skeleton
- [x] Fail-fast repository verifier
- [ ] Native shells compile on both target platforms
- [ ] FFI boundary design ADR

## Phase 1 — Local + SFTP vertical slice

### Phase 1A — Core I/O and real file transfer

- [x] backend-neutral filesystem I/O contract
- [x] native local backend
- [x] SFTP backend
- [x] strict `known_hosts` verification
- [x] SSH-agent authentication
- [x] trusted-caller private-key authentication API
- [x] local directory listing/stat
- [x] SFTP directory listing/stat
- [x] regular-file local copy
- [x] regular-file SFTP upload/download
- [x] destination-size verification
- [x] local integration test
- [x] opt-in live SFTP round-trip harness

### Phase 1B — Reliable transfer mechanics

- [ ] recursive directory planning/copy
- [ ] temporary destination + atomic finalization
- [ ] byte/progress event stream
- [ ] cooperative cancellation
- [ ] bounded retry/backoff
- [ ] resume API and capability validation
- [ ] metadata/permission preservation policy
- [ ] controlled SFTP integration server for CI
- [ ] disconnect/short-write/rename-failure tests

### Phase 1C — Native daily-use shell

- [ ] macOS dual-pane browser
- [ ] Linux dual-pane browser
- [ ] saved server model without secrets
- [ ] transfer queue UI
- [ ] drag/drop between panes and desktop file managers
- [ ] keyboard navigation and commands

**Phase 1 exit:** daily-use-quality SFTP transfer between local and remote panes on macOS and Linux.

## Phase 2 — Production transfer engine

- durable queue/recovery
- pause/resume
- checksum policies
- bandwidth limits
- per-host concurrency controls
- conflict handling
- history and detailed diagnostics
- folder compare

## Phase 3 — Protocol expansion

- FTP/FTPS
- WebDAV/HTTPS
- S3-compatible storage
- Cloudflare R2/MinIO compatibility validation

## Phase 4 — Synchronization

- one-way mirror
- bidirectional planning model
- dry-run operation graph
- ignore rules (`.cyberpumpkinignore` plus optional `.gitignore` awareness)
- destructive-operation guardrails

## Phase 5 — Desktop integration

- macOS File Provider exploration
- Linux FUSE3 mount exploration
- remote editing
- terminal handoff
- notifications

## Phase 6 — Power-user layer

- `cpk` full CLI
- scripting/automation interface
- exportable JSON profiles without secrets
- connection doctor
- benchmark suite and telemetry-free diagnostics bundle
