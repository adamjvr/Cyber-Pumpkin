# Architecture

## Goals

Cyber-Pumpkin must feel native on macOS and Linux while sharing the difficult product logic: filesystem modeling, transfers, synchronization, retries, recovery, profiles, diagnostics, and protocol semantics.

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
                    backend I/O contract
                      /              \
                 local               SFTP
                                      |
                          future FTP/WebDAV/S3
                               |
Platform services              v
             Keychain/libsecret, notifications, file integration
```

Dependencies point downward. Infrastructure implements common contracts; the domain does not import UI frameworks.

## Phase 1A concrete ownership

`cyber-pumpkin-backend` defines the file-like I/O surface. `cyber-pumpkin-local` and `cyber-pumpkin-sftp` implement it. `cyber-pumpkin-transfer` only sees the contract and backend-neutral identifiers/paths.

This means local-to-local, local-to-SFTP, and SFTP-to-local regular-file copies all execute through the same transfer path. The CLI is simply the first real consumer.

## Backend model

A backend represents a filesystem-like namespace with declared capabilities. Consumers ask what a backend can do; they do not switch on protocol names.

Phase 1A common operations:

- list
- stat/lstat-style metadata
- sequential read
- create/truncate sequential write
- create directory
- rename
- remove one file/link/empty directory

Capabilities remain explicit. A capability is not marked supported until the common API can actually exercise it. For example, resumable writes remain unsupported in Phase 1A even though both native filesystems and SFTP can eventually support seeking.

## Transfer model

The transfer engine owns lifecycle state and execution semantics. The first executor:

1. validates backend identity against the transfer specification;
2. validates the source is a regular file;
3. opens source/destination streams through the backend contract;
4. streams without loading the whole file into memory;
5. flushes the destination;
6. verifies destination size when available;
7. marks the job completed or failed.

Directory planning, progress events, cancellation, retries, temporary destination names, atomic finalization, durable recovery, and concurrency are subsequent Phase 1 milestones. They belong here, never in the GUI.

## FFI

The shared core will expose a deliberately small stable boundary. Native UI layers receive immutable snapshots/events and submit commands. FFI types must be boring: fixed-width scalars, explicit ownership, UTF-8 strings, byte buffers, opaque handles, and versioned structs where needed.

## Persistence

Human-readable project/user configuration should use JSON where practical. Secrets never live in JSON; profiles store references to platform secret providers.

## ADR rule

Major irreversible decisions get a short Architecture Decision Record under `docs/adr/` before broad implementation.
