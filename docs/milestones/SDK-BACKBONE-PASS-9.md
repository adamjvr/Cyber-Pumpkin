# SDK Backbone Pass 9

Implemented:
- shared SSH transport/auth/trust library
- SFTP refactored to consume shared SSH
- dependency-aware generalized operation queue
- reusable file-rule evaluator
- deterministic one-way synchronization planner
- create/copy/remove/conflict/skip plan actions
- safe deepest-first orphan directory removal ordering
- local sync-plan example
- unit tests for the new libraries
- documented internal SDK dependency map

Deliberate boundary:
sync planning does not execute changes. The next layer consumes the immutable
plan through the operations and transfer libraries so preview and execution use
the exact same plan.
