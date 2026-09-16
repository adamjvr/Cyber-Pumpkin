# Pass 11 Internal SDK Delta

```text
native shells
    |
application
    |
operations ---- scheduler
    |
sync-plan ---- sync
                 |
            reliability
                 |
              transfer
                 |
              backend
              /    \
           local   sftp
                    |
                   ssh
```

The reliability layer is intentionally not embedded in SFTP, local filesystem,
or the Linux/macOS shells. It is a backend-neutral transaction layer above the
filesystem contract and transfer engine.

The scheduler likewise does not own operation semantics. It only admits ready
operations according to the configured simultaneous-work ceiling while the
operation queue remains the source of lifecycle and dependency truth.
