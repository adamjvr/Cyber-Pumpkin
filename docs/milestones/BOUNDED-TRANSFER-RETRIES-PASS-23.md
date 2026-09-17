# Bounded Transfer Retries — Pass 23

Pass 23 hardens partial-file cleanup and adds reconnecting, bounded retries for transient interactive copy failures.

## Runtime policy

- Default is three total attempts, not three retries.
- Backoff is capped exponential: 250 ms, then 500 ms with the default policy.
- Product safety bound rejects more than five total attempts.
- Mid-stream and verification failures remove the operation-owned partial file before returning.
- Cleanup failure is a distinct non-retryable execution error because destination state is ambiguous.
- Source and destination backends are recreated for every retry so SFTP retries actually reconnect.
- Cancellation is checked before each attempt and after every backoff.

## Retry classification

Retryable: transport, protocol, generic I/O backend failures, and stream failures after successful cleanup/rollback.

Never retried: authentication, host-key, permission, input/configuration, destination conflicts, unsupported entry types, sidecar collisions, cleanup failures, recovery failures, backup-cleanup failures, lifecycle errors, endpoint mismatches, and verification size mismatches.

The critical invariant is that an error is retried only when the lower reliability layer returned a state that is safe for a fresh attempt. Ambiguous recovery/cleanup failures terminate immediately.
