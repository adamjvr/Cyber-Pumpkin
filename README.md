# Cyber-Pumpkin

Cyber-Pumpkin is a native macOS and Linux file-transfer client built from scratch around a shared Rust core and native platform user interfaces.

The target experience is a fast, polished, keyboard-friendly dual-pane local/remote file browser. Reference applications may be studied for observable behavior and workflow, but Cyber-Pumpkin does **not** copy proprietary source, assets, icons, strings, or implementation details.

## Phase 0

The first vertical slice is deliberately narrow and real:

1. Launch the native application.
2. Browse local files in either pane.
3. Connect to SFTP.
4. Browse remote files.
5. Upload/download files and directories.
6. Expose deterministic queue/progress state.
7. Cancel safely.
8. Resume where backend semantics permit.
9. Recover from disconnects without corrupting destination data.
10. Verify completed transfers according to policy.

## Architecture

```text
macOS AppKit/SwiftUI ─┐
                      ├─ stable FFI boundary ─ Rust product core
Linux GTK4 ───────────┘                     ├─ filesystem model
                                            ├─ transfer engine
                                            ├─ sync planner
                                            ├─ protocol backends
                                            ├─ profiles/config
                                            └─ diagnostics
```

**Architectural rule:** UI frameworks are consumers. They never own protocol, transfer, synchronization, persistence, retry, or security behavior.

## Workspace

- `crates/cyber-pumpkin-core` — backend-neutral domain model and contracts.
- `crates/cyber-pumpkin-transfer` — transfer planning and lifecycle model.
- `crates/cpk` — CLI using the same product core.
- `platform/macos` — native macOS shell.
- `platform/linux` — native Linux shell.
- `docs` — architecture, standards, testing, security, roadmap, behavior matrix.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run -p cpk -- about
```

Start with [`docs/CODING_STANDARDS.md`](docs/CODING_STANDARDS.md) and [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## License

GPL-3.0-only. The GitHub repository already contains the canonical `LICENSE` file.
