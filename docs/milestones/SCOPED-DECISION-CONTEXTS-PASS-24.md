# Scoped Decision Contexts — Pass 24

Pass 24 migrates user-visible copy and sync conflict decisions away from the
legacy decision-kind-only compatibility path.

## Behavior

- Copy conflict memory is scoped by:
  - decision kind,
  - upload/download/local/neutral direction,
  - source backend family,
  - destination backend family,
  - file-vs-directory object scope.
- Sync type-conflict memory uses the same endpoint/direction dimensions with a
  dedicated `sync:type-conflict` scope.
- Pane identity is deliberately not used as backend identity. Stable backend
  families (`local` and `sftp`) prevent remembered choices from being coupled
  to left/right pane assignment.
- A remembered Upload/Replace choice therefore cannot silently resolve a
  Download conflict or a directory conflict.
- Non-conflict compatibility callers may continue to use kind-only request
  semantics until their policy model requires richer context.

## Regression coverage

The decisions crate verifies that an `AllMatching` answer is reused for the
exact same context while remaining isolated from an opposite transfer
direction and a different object scope.
