# Runtime Decisions Pass 13

Pass 13 connects the shared decision model to real destructive/conflict behavior.

Implemented:
- Sync type conflicts can be resolved as Replace All, Skip All, or Cancel from Linux Sync.
- Shared sync execution exposes ConflictPolicy::{Fail, Skip, Replace}.
- Replace removes a conflicting destination tree before reconstructing source shape.
- Sync conflicts are ordered before descendant create/copy actions.
- Orphan removals tolerate paths already removed by conflict replacement.
- Delete confirmation routes through DecisionCenter and honors confirm_delete.
- Delete decisions can be remembered for the current app session.
- Successful and failed deletes are persisted to Activity history.
- cpk sync-run-local gains --conflicts=fail|skip|replace.

Deferred to Pass 14:
- ordinary Copy overwrite decisions
- SSH host-trust retry/persistence UI
- Secret Service / Keychain secret storage
