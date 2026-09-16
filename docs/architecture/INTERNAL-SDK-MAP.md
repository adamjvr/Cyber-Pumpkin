# Cyber-Pumpkin Internal SDK Map

Cyber-Pumpkin deliberately keeps reusable behavior in internal libraries.

| Library | Responsibility |
| --- | --- |
| `cyber-pumpkin-core` | IDs, backend paths, file metadata, capabilities |
| `cyber-pumpkin-backend` | backend-neutral filesystem contract |
| `cyber-pumpkin-local` | local provider |
| `cyber-pumpkin-ssh` | TCP/SSH handshake, host trust, authentication |
| `cyber-pumpkin-sftp` | SFTP filesystem provider consuming shared SSH |
| `cyber-pumpkin-transfer` | streaming, verify, cancellation, recursive transfer |
| `cyber-pumpkin-reliability` | staged, verified, recoverable destination replacement |
| `cyber-pumpkin-scheduler` | dependency-aware simultaneous operation admission |
| `cyber-pumpkin-operations` | generalized operation lifecycle and dependencies |
| `cyber-pumpkin-rules` | reusable include/skip rules |
| `cyber-pumpkin-sync-plan` | deterministic backend-neutral sync planning |
| `cyber-pumpkin-sync` | execution of immutable sync plans through backend/transfer primitives |
| `cyber-pumpkin-application` | pane/session/profile/preferences state |

## Dependency direction

```text
native shells
    |
cyber-pumpkin-application
    |
operations / sync-plan / transfer
    |          |          |
rules      filesystem ----+
               |
       backend-neutral contract
          /            \
       local           sftp
                        |
                       ssh
```

macOS AppKit and Linux GTK4/libadwaita own presentation and OS integration.
Rust libraries own behavior.

## Next libraries

- `cyber-pumpkin-history`
- `cyber-pumpkin-secrets` with Keychain / Secret Service adapters
- `cyber-pumpkin-platform` for notifications, clipboard, dialogs, filesystem watch, trash, terminal, and updates
- provider crates such as FTP only when actually implemented

Pass 13 rule: native shells render decisions; shared decision and sync libraries own allowed choices and conflict semantics.
