# Cyber-Pumpkin Local + SFTP Release Candidate

The code-complete release-candidate target is `0.9.0-rc.1`. Promotion to `1.0.0` requires the target-machine acceptance checks below.

## Automated gates

- `./scripts/verify.sh`
- `./scripts/verify-release.sh`
- GitHub Linux native build
- GitHub macOS AppKit build

## Linux acceptance

1. Local→Local file and nested-folder copy.
2. Local→SFTP and SFTP→Local nested-folder copy.
3. Replace, Skip, and Keep Both for files and folders.
4. Cancel active transfer and confirm cleanup.
5. Kill/relaunch during a journaled local-destination replacement and confirm startup recovery.
6. Remote Edit: upload, remote-change conflict, stop one, stop all, and Activity lifecycle.
7. Inspector permission edit and async metadata refresh.
8. Sync preview and execution with delete-orphan guardrails.

## macOS acceptance

1. Local/SFTP browsing in either pane.
2. Local↔Local, Local↔SFTP, and SFTP↔SFTP nested-folder copy.
3. Replace, Skip, and Keep Both behavior.
4. Recursive local and SFTP folder deletion.
5. Pumpkin Patch save/open/remove flow.
6. Shared delete/conflict/keepalive preferences persist through `cpk`.
7. Build/run from a clean checkout using `scripts/build-native.sh`.

## Promotion rule

Do not tag `1.0.0` until both platform acceptance lists are green and packaging/signing artifacts have been produced.
