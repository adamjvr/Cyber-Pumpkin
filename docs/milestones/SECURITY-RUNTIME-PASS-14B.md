# Security Runtime Pass 14B

Implemented:
- SSH password authentication.
- SHA-256 host-key probing before authentication.
- app-scoped exact-fingerprint trust for unknown hosts only.
- known_hosts mismatches remain fatal.
- SFTP host-probe exports and trusted-fingerprint config.
- saved profiles v2: Agent / Password / Private Key.
- password/passphrase values stay in Secret Service / Keychain.
- Linux Quick Connect exposes Agent / Password / Private Key.
- Unknown-host Trust Once / Trust Always / Cancel.
- Pumpkin Patch restores saved authentication through the OS secret store.
