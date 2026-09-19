# Cyber-Pumpkin macOS

Native AppKit frontend backed by the shared Rust `cpk` companion.

The Local + SFTP release-candidate shell includes dual panes, Local/SFTP browsing, Pumpkin Patch saved connections, recursive file operations, safe conflict-aware tree copy including SFTP→SFTP, shared preferences, Inspector basics, native menus, and keyboard commands.

Build from the repository root:

```sh
./scripts/build-native.sh
./scripts/verify-release.sh
```

The AppKit layer owns presentation. Filesystem/protocol/transfer behavior remains in Rust.
