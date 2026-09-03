# Transfer Engine

The transfer engine is the product's reliability core.

## State machine

```text
Queued
  -> Connecting
  -> Enumerating (when needed)
  -> Transferring
  -> Verifying
  -> Completed

Connecting / Enumerating / Transferring / Verifying
  -> RetryWaiting -> Connecting
  -> Failed
  -> Cancelled

Transferring -> Paused -> Connecting or Transferring
```

Terminal states never transition.

## Correctness rules

1. Never expose a partial write under the final destination name when an atomic staging strategy is available.
2. Completion means all required data, metadata, and verification policy succeeded.
3. Cancellation stops new work first, then unwinds active I/O cooperatively.
4. Retry policy is bounded and observable.
5. Progress has explicit known/unknown semantics; do not fake percentages before enumeration knows total work.
6. Directory transfers aggregate child state deterministically.
7. Resume is capability-driven and must validate that source/destination still match the recovery record.

## Concurrency

Concurrency is controlled at multiple levels:

- global worker limit
- per-backend/host connection limit
- per-transfer subtask limit
- optional bandwidth limit

Defaults favor responsiveness and server politeness rather than synthetic benchmark scores.

## Verification

Verification policy is configurable by backend capability:

- size + metadata baseline
- local checksum when appropriate
- server checksum when trustworthy and supported
- re-read hash for strict mode

Checksums are never described as authenticity guarantees unless the transport and threat model justify that statement.
