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
Paused -> Failed when an underlying operation becomes unrecoverable
```

Terminal states never transition.

## Phase 1A executor

`execute_file` is the first actual executor behind the lifecycle contract. It is deliberately synchronous and single-file so correctness is established before queue concurrency is introduced.

Current invariants:

- backend identifiers must match the immutable transfer specification;
- source must be a regular file;
- bytes stream through `Read`/`Write` handles rather than a whole-file buffer;
- destination is flushed before verification;
- destination size must equal bytes copied when the backend reports size;
- any post-start execution error moves the lifecycle to `Failed`;
- success ends in `Completed`.

## Correctness rules for later Phase 1

1. Never expose a partial write under the final destination name when an atomic staging strategy is available.
2. Completion means all required data, metadata, and verification policy succeeded.
3. Cancellation stops new work first, then unwinds active I/O cooperatively.
4. Retry policy is bounded and observable.
5. Progress has explicit known/unknown semantics; do not fake percentages before enumeration knows total work.
6. Directory transfers aggregate child state deterministically.
7. Resume is capability-driven and must validate that source/destination still match the recovery record.

## Concurrency

Concurrency is intentionally **not** part of Phase 1A. It will be controlled at multiple levels:

- global worker limit
- per-backend/host connection limit
- per-transfer subtask limit
- optional bandwidth limit

Defaults favor responsiveness and server politeness rather than synthetic benchmark scores.

## Verification

Phase 1A performs destination-size verification. Later policy can select:

- size + metadata baseline
- local checksum when appropriate
- server checksum when trustworthy and supported
- re-read hash for strict mode

Checksums are never described as authenticity guarantees unless the transport and threat model justify that statement.
