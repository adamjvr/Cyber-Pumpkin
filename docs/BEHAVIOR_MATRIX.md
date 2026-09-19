# Observable Behavior Matrix

| Area | Release-candidate contract | Status |
|---|---|---|
| Browser | Two independent filesystem panes; either side may be Local or SFTP | Implemented Linux + macOS |
| Navigation | Path/up navigation, filtering, sorting, hidden-file control | Implemented |
| Local browse | Enumerate/stat native files | Implemented |
| Remote browse | Enumerate/stat SFTP paths with strict host verification | Implemented |
| Transfer | Recursive Local↔Local, Local↔SFTP, and SFTP↔SFTP copy | Implemented |
| Conflicts | Ask / Replace / Skip / Keep Both | Implemented |
| Queue/runtime | Shared lifecycle, progress, bounded concurrency/retry, cancellation | Implemented; richer macOS queue UI post-v1 |
| Recovery | Durable replacement journals; local startup replay; remote pending discovery | Implemented |
| Favorites | Pumpkin Patch saved connections without embedded secrets | Implemented |
| Sync | Shared preview/planner/executor with rules | Implemented core + Linux UI |
| Remote edit | Download/watch/debounce/re-upload/conflict protection | Implemented Linux; macOS post-v1 |
| Inspector | Metadata and permission editing | Implemented Linux; basic metadata macOS |
| Preferences | Shared persisted conflict/delete/connection preferences | Implemented |
| Diagnostics | Structured backend errors plus Activity/history | Implemented baseline |

## Clean-room rule

Cyber-Pumpkin implements independently specified observable behavior. Do not copy proprietary source, assets, icons, strings, private symbols, or implementation details from reference products.
