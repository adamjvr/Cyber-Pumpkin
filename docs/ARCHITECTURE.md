# Architecture

## Goals

Cyber-Pumpkin must be native-feeling on macOS and Linux while sharing the difficult product logic: filesystem modeling, transfers, synchronization, retries, recovery, profiles, diagnostics, and protocol semantics.

## Layer model

```text
Presentation
  macOS AppKit/SwiftUI       Linux GTK4       CLI
            \                  |              /
             \          stable application API
              +----------------+----------------+
                               |
Domain                         v
                  backend-neutral core model
                         transfer engine
                           sync planner
                          profile model
                               |
Infrastructure                 v
                  local / SFTP / FTP / WebDAV / S3 ...
                               |
Platform services              v
             Keychain/libsecret, notifications, file integration
```

Dependencies point downward. Infrastructure implements domain contracts; the domain does not import UI frameworks or concrete protocol packages.

## Backend model

A backend represents a filesystem-like namespace with declared capabilities. The UI asks what a backend can do; it does not switch on protocol names.

Initial capability examples:

- resumable read/write
- atomic rename
- server-side copy
- checksum
- Unix permissions
- symlink support
- recursive delete
- timestamp preservation

## Transfer model

The transfer engine owns:

- planning and enumeration
- queue ordering
- bounded global and per-host concurrency
- cancellation
- retry/backoff
- temporary destination naming
- atomic commit into final name where supported
- progress accounting
- verification
- durable recovery metadata

No UI implementation may duplicate these rules.

## FFI

The shared core will eventually expose a deliberately small stable boundary. Native UI layers receive immutable snapshots/events and submit commands. FFI types must be boring: fixed-width scalars, explicit ownership, UTF-8 strings, byte buffers, opaque handles, and versioned structs where needed.

## Persistence

Human-readable project/user configuration should use JSON where practical. Secrets never live in JSON; profiles store references to platform secret providers.

## ADR rule

Major irreversible decisions get a short Architecture Decision Record under `docs/adr/` before broad implementation.
