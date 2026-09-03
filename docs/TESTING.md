# Testing

Testing is layered around behavior and invariants, not implementation trivia.

## Required gates

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Test families

### Unit

- domain validation
- capability decisions
- transfer state machine
- retry/backoff math
- path handling
- conflict policy

### Contract

Every backend must pass the same filesystem contract suite: list/stat/read/write/rename/delete, errors, Unicode names, empty files, large files, timestamps, cancellation, and declared capabilities.

### Integration

Containerized or disposable servers for SFTP/FTP/WebDAV/S3-compatible testing. Tests must control server versions and configuration.

### Fault injection

- disconnect during upload/download
- short read/write
- permission change mid-transfer
- destination fills up
- rename failure
- checksum mismatch
- server restart
- cancellation at every lifecycle phase

### Cross-platform

Behavioral fixtures are shared. Native shells additionally test platform interactions, drag/drop, keyboard navigation, secret storage, and lifecycle behavior.

## Performance

Benchmarks measure throughput, CPU, memory, enumeration latency, cancellation latency, and scaling across worker counts. Performance changes never weaken correctness invariants just to improve a benchmark.
