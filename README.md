# Cyber-Pumpkin

Cyber-Pumpkin is a native Linux and macOS Local + SFTP file-transfer client with a shared Rust behavior core and native platform user interfaces.

## Current milestone: Local + SFTP release candidate

The release-candidate scope is intentionally narrow and complete enough for daily Local/SFTP work:

- backend-neutral local and SFTP filesystem model
- strict OpenSSH `known_hosts` verification and SSH-agent authentication
- dual-pane Linux GTK4/libadwaita and macOS AppKit browsers
- Pumpkin Patch saved connections without embedded secrets
- recursive file/folder copy across Local↔Local, Local↔SFTP, and SFTP↔SFTP
- Replace / Skip / Keep Both conflict behavior
- bounded retry, cooperative cancellation, progress, and durable replacement journals
- startup recovery for local-destination transactions and pending-remote recovery reporting
- synchronization planner/executor and reusable rules in the Rust core
- Remote Edit lifecycle on Linux with conflict detection and crash-safe upload transactions
- Inspector metadata/permissions on Linux
- shared persistent preferences and native secret/trust adapters
- native menus, keyboard commands, file operations, and Activity surfaces

The GUI does not own protocol or transfer correctness. Rust owns behavior; native code owns presentation and OS integration.

## Verify

```bash
./scripts/verify.sh
./scripts/verify-release.sh
```

`verify-release.sh` adds the native host build to the normal strict Rust gate. Live SFTP and final manual GUI acceptance remain explicit release-signoff steps; see `docs/RELEASE-CANDIDATE.md`.

## Post-RC work

FTP/FTPS, WebDAV, object storage, filesystem mounts, updater infrastructure, richer connection pooling, and additional desktop integrations are post-v1 features. They are not required for the Local + SFTP release contract.

## License

GPL-3.0-only.
