# Roadmap

## Phase 0 — Foundation

- [x] Architecture boundaries
- [x] Coding standards
- [x] Transfer lifecycle model
- [x] Test/security specifications
- [x] CLI skeleton
- [ ] Native shells compile on both target platforms
- [ ] FFI boundary design ADR

## Phase 1 — Local + SFTP vertical slice

- local backend contract
- SFTP backend
- known-hosts and authentication flow
- dual-pane browser
- file/directory upload + download
- transfer queue UI
- cancellation/retry
- temporary destination + atomic finalization
- integration/fault tests

**Exit:** daily-use-quality SFTP transfer between local and remote panes on macOS and Linux.

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

- macOS file-provider exploration
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
