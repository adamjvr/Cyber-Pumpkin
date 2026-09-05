# Phase 1A — Local + SFTP Core Vertical Slice

Base: `dd00af5` (`Harden Phase 0 verification and capability model`)

## Purpose

Turn the Phase 0 architecture into the first real file-transfer path without handing protocol or transfer ownership to a UI framework.

## Delivered

- common backend I/O trait and contextual error model
- native local filesystem backend
- strict-known-host SFTP backend
- SSH-agent authentication plus private-key API
- regular-file transfer execution through common backend streams
- destination-size verification and failed-state handling
- CLI local list/stat/copy
- CLI SFTP list/put/get/remove
- automated local executor coverage
- opt-in live SFTP list and round-trip harness
- architecture, security, testing, roadmap, and backend documentation updates

## Explicitly deferred

- recursive directory transfers
- temporary-name atomic commit
- progress event stream
- cancellation/retry/resume
- queue concurrency
- persistent server profiles/secrets
- native dual-pane UI

Those are Phase 1B/1C work and must extend this core rather than reimplement it in platform shells.
