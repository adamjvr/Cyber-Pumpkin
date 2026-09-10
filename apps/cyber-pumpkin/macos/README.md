# Cyber-Pumpkin macOS

Native AppKit frontend.

Major GUI Pass 1 uses `cpk` as a temporary process boundary for local filesystem
enumeration, so filesystem behavior remains in the shared Rust backend instead
of being reimplemented in Swift. The AppKit side owns presentation only.

Build from the repository root on macOS:

```sh
./scripts/build-native.sh
```

Run:

```sh
./scripts/run-macos.sh
```

The exact Transmit-style menu hierarchy is intentionally provisional until the
clean-room screenshot inventory is captured.
