# Security

## Secrets

- macOS native app: Keychain.
- Linux native app: Secret Service/libsecret.
- Configuration stores only stable secret references.
- Never log passwords, private keys, bearer tokens, session cookies, or decrypted secret payloads.
- Never accept passwords or private-key passphrases as ordinary CLI arguments.

Phase 1A CLI authentication uses the SSH agent. The library supports private-key authentication for trusted callers, but secret acquisition remains outside the SFTP backend.

## SSH/SFTP Phase 1A

The SFTP backend is fail-closed on host identity:

1. complete SSH handshake;
2. read the presented host key;
3. load OpenSSH `known_hosts`;
4. require an exact host + port match;
5. reject unknown, mismatched, or unverifiable keys;
6. authenticate only after host verification;
7. open SFTP only after successful authentication.

Trust-on-first-use is **not** implemented in Phase 1A. If added later, it must be an explicit user-visible decision that displays enough fingerprint information for meaningful verification.

Future SSH work includes keyboard-interactive/password flows through secure native prompts, modern hardware-backed keys, ProxyJump/ProxyCommand behavior, and richer connection diagnostics.

## Filesystem safety

- Normalize only according to backend rules; do not reinterpret remote paths as host paths.
- Reject unrepresentable paths rather than silently changing them.
- Treat symlinks deliberately during recursive copy/sync.
- Detect path traversal at archive/import boundaries.
- Never delete outside a confirmed synchronization root.
- Dry-run/preview must use the same planner as execution.
- Atomic staging/finalization becomes mandatory where the backend advertises support once Phase 1B introduces the staging API.

## Dependencies

Network, crypto, parser, compression, and credential dependencies receive elevated review. `ssh2`/libssh2 and its OpenSSL path are security-sensitive dependencies. Cargo lockfile changes must be reviewed like source changes.

Automated dependency auditing will be added before the first public binary release; it should not silently mutate the lockfile.
