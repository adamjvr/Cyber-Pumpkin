# Cyber-Pumpkin

Cyber-Pumpkin is a native macOS and Linux file-transfer client built from scratch around a shared Rust core and native platform user interfaces.

The target experience is a fast, polished, keyboard-friendly dual-pane local/remote file browser. Reference applications may be studied for observable behavior and workflow, but Cyber-Pumpkin does **not** copy proprietary source, assets, icons, strings, or implementation details.

## Current milestone: Phase 1A

Phase 1A turns the Phase 0 contracts into a working Local + SFTP core vertical slice:

- backend-neutral filesystem I/O contract
- native local-filesystem backend
- SFTP backend using `ssh2`/libssh2
- strict OpenSSH `known_hosts` verification
- SSH-agent authentication by default
- private-key authentication API for trusted callers
- regular-file local copy, upload, and download through one transfer executor
- destination-size verification
- local and SFTP CLI operations through `cpk`
- deterministic local tests plus opt-in live SFTP round-trip testing

The GUI is intentionally not allowed to own any of this behavior.

## Architecture

```text
macOS AppKit/SwiftUI ─┐
                      ├─ application/FFI boundary ─ Rust product core
Linux GTK4 ───────────┘                         ├─ domain model
CLI (`cpk`) ────────────────────────────────────┤
                                                ├─ backend contract
                                                ├─ local backend
                                                ├─ SFTP backend
                                                └─ transfer engine
```

**Architectural rule:** UI frameworks are consumers. They never own protocol, transfer, synchronization, persistence, retry, or security behavior.

## Workspace

- `crates/cyber-pumpkin-core` — backend-neutral domain types.
- `crates/cyber-pumpkin-backend` — common filesystem-like I/O contract.
- `crates/cyber-pumpkin-local` — native local filesystem implementation.
- `crates/cyber-pumpkin-sftp` — SFTP connection and filesystem implementation.
- `crates/cyber-pumpkin-transfer` — lifecycle plus first regular-file executor.
- `crates/cpk` — CLI using the same product core.
- `platform/macos` — native macOS shell.
- `platform/linux` — native Linux shell.
- `docs` — architecture, standards, testing, security, roadmap, behavior matrix.

## CLI examples

```bash
cpk local-ls ~/Downloads
cpk local-stat ~/Downloads/example.zip
cpk local-copy ./input.bin ./copy.bin

cpk sftp-ls server.example.com adam /var/www
cpk sftp-put ./site.tar server.example.com adam /tmp/site.tar
cpk sftp-get server.example.com adam /tmp/site.tar ./site.tar
```

SFTP CLI commands use the SSH agent and require the server key to already match `~/.ssh/known_hosts`. Passwords are deliberately not accepted as command-line arguments.

## Verify

```bash
./scripts/verify.sh
```

Optional live SFTP validation is documented in [`docs/SFTP.md`](docs/SFTP.md).

Start with [`docs/CODING_STANDARDS.md`](docs/CODING_STANDARDS.md) and [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## License

GPL-3.0-only. The repository `LICENSE` file is canonical.
