# Observable Behavior Matrix

This document converts reference-workflow observations into independent Cyber-Pumpkin requirements. It records **what users can observe**, not proprietary implementation details.

| Area | Observable requirement | Cyber-Pumpkin contract | Status |
|---|---|---|---|
| Browser | Two filesystem panes | Either pane can represent any configured backend | Planned |
| Navigation | Back/forward/path navigation | Shared navigation model, native rendering | Planned |
| Local browse | Enumerate/stat native files | Common backend contract + local implementation | Phase 1A core |
| Remote browse | Enumerate/stat SFTP paths | Common backend contract + strict SFTP session | Phase 1A core |
| Transfer | Copy between local/remote locations | Immutable transfer spec + shared executor | Phase 1A regular files |
| Queue | Progress and cancellation | Core-owned lifecycle snapshot | Foundation |
| Tabs | Multiple locations/connections | Platform-native tabs mapped to independent sessions | Planned |
| Favorites | Saved server/location access | Profile + location model | Planned |
| Sync | Preview before execution | Shared sync planner produces explicit operation set | Planned |
| Remote edit | Edit and upload changed file | Staged download/watch/re-upload workflow | Planned |
| Diagnostics | Actionable connection errors | Structured backend error categories; full doctor later | Partial |

## Clean-room rule

Do not copy reference source code, binaries, private symbols, proprietary assets, iconography, or text. Behavior observations must be documented in neutral product terms and implemented from Cyber-Pumpkin's own architecture.
