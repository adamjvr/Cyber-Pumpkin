# Security Storage Pass 14A

This pass establishes the credential/trust substrate before native SSH trust dialogs are wired.

- Adds `cyber-pumpkin-secrets`.
- Linux uses Secret Service through `secret-tool`.
- macOS uses Keychain through `security`.
- Adds a deterministic memory secret store for tests.
- Adds versioned application-scoped SSH trust records.
- Keeps saved profiles non-secret: no password fields are added to JSON profiles.

Next pass wires password/private-key authentication, host-key probing, Trust Once / Trust Always, and Quick Connect into these stores.
