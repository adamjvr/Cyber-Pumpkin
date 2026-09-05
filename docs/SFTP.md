# SFTP Backend

## Phase 1A security model

SFTP is built on the Rust `ssh2` bindings to libssh2. The backend performs operations in this order:

1. TCP connect with bounded read/write timeout.
2. SSH handshake.
3. Read the server host key.
4. Load the configured OpenSSH `known_hosts` file.
5. Require an exact host-key match for hostname + port.
6. Authenticate.
7. Open the SFTP subsystem.

Unknown hosts, mismatched host keys, and host-key verification failures are fatal. Phase 1A does not implement trust-on-first-use.

## Authentication

The CLI uses `ssh-agent` authentication. The Rust API also supports private-key files with an optional passphrase for callers that obtain the passphrase through a trusted secret path.

Do **not** add password/passphrase command-line flags. Command arguments are routinely exposed through process listings and shell history.

Later native apps will resolve credentials through macOS Keychain and Linux Secret Service/libsecret.

## Known hosts

Default:

```text
~/.ssh/known_hosts
```

A programmatic caller may override the path through `SftpConfig::with_known_hosts_file`.

Users should populate/verify host keys using normal OpenSSH administration workflows before Cyber-Pumpkin connects. Cyber-Pumpkin must never silently weaken host identity checks to make a connection succeed.

## Current operations

- list
- stat/lstat
- sequential read
- create/truncate sequential write
- mkdir
- rename
- remove file/link/empty directory

The CLI currently exposes list, upload, download, and remove. More file-management commands arrive with the browser shell.

## Deliberate Phase 1A limits

- no password or keyboard-interactive CLI flow
- no ProxyJump/ProxyCommand yet
- no resume/seek API yet
- no recursive copy yet
- no atomic temporary-name commit yet
- no per-host connection pool yet
- UTF-8 paths only in the common model
