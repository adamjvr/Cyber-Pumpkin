# Security

## Secrets

- macOS: Keychain.
- Linux: Secret Service/libsecret.
- Configuration stores only stable secret references.
- Never log passwords, private keys, bearer tokens, session cookies, or decrypted secret payloads.

## SSH/SFTP target

First-class SSH behavior will include:

- host-key verification and known-hosts policy
- modern key types
- ssh-agent integration
- password and keyboard-interactive authentication where required
- explicit changed-host-key failure
- proxy/jump configuration in a later milestone

Trust-on-first-use, if offered, must be an explicit user-visible decision.

## Filesystem safety

- Normalize only according to backend rules; do not reinterpret remote paths as host paths.
- Treat symlinks deliberately during recursive copy/sync.
- Detect path traversal at archive/import boundaries.
- Never delete outside a confirmed synchronization root.
- Dry-run/preview must use the same planner as execution.

## Dependencies

Network, crypto, parser, compression, and credential dependencies receive elevated review. CI should add dependency auditing once the first protocol implementation lands.
