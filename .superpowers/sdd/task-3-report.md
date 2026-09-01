# Task 3 Report: ArcGIS Pro discovery, launch, and official Add-In handoff

## Status

Implemented Task 3 only. The desktop runtime discovers and validates ArcGIS Pro 3.7, persists the canonical selected root, supports validated manual selection, and directly launches the validated executable without a shell. Add-In installation or upgrade is handed to Esri's registered `.esriAddInX` handler using only the fixed bundled package. Uninstall returns Add-In Manager guidance; the desktop runtime never writes or deletes files below the ArcGIS Add-Ins directory.

The original direct-mutation work and its formal-review follow-up remain documented below as superseded implementation history. The final authoritative behavior is the Option A amendment at the end of this report.

## RED / GREEN evidence

1. Discovery priority RED: `cargo test --test arcgis_install` failed with E0432 because `arcgis_install` did not exist. GREEN: saved, registry, standard, and compatibility candidates are ordered and tested.
2. Required files RED: compilation failed because `validate_installation` did not exist. GREEN: roots are canonicalized and both `bin\ArcGISPro.exe` and `bin\ArcGIS.Core.dll` must be files.
3. Ownership boundary RED: compilation failed because `addin_install` did not exist. GREEN: an ownership destination outside the exact GUID-root package path is refused.
4. Changed hash RED: compilation failed because `cleanup_owned_addin` did not exist. GREEN: a changed target and its ownership record are preserved.
5. ArcGIS 3.7 RED: compilation failed because `validate_installation_with` did not exist. GREEN: 3.7.x file versions are accepted; 3.6 and unknown versions are rejected.
6. Repair/restart RED: compilation failed because `repair_addin` did not exist. GREEN: a new owned package is installed atomically and a running ArcGIS Pro produces `requiresRestart: true` only when package content changes.
7. Exact cleanup RED: the test observed `Refuse` instead of `Delete`. GREEN: only the unchanged manifest-owned package and ownership record are deleted; an unrelated Add-In and the containing directory remain.
8. Safe launch RED: compilation failed because `arcgis_launch_command` did not exist. GREEN: the command program is the validated executable and has no shell or concatenated arguments.
9. Manual selection RED: the validated manual-selection seam and `Manual` source were absent. GREEN: only an exact `bin\ArcGISPro.exe` from a valid 3.7 root is accepted.
10. Absolute ownership RED: lexically matching relative paths incorrectly planned deletion. GREEN: cleanup refuses relative roots and destinations.

## Files

- Added `apps/desktop/src-tauri/src/arcgis_install.rs`.
- Added `apps/desktop/src-tauri/src/addin_install.rs`.
- Added `apps/desktop/src-tauri/tests/arcgis_install.rs`.
- Updated Rust dependencies, state snapshot, settings persistence, command handlers, and Tauri registration.
- Updated `apps/desktop/src/domain.ts` and `apps/desktop/src/desktopApi.ts` with the Task 3 contracts.

## Verification

- `cargo test --test arcgis_install`: 10 passed.
- `cargo test`: all suites passed (3 unit; 10 Task 3; 20 passed/1 ignored; 29; 31; 3; 14; doc tests passed).
- `cargo fmt --all -- --check`: passed.
- `npm.cmd run build`: TypeScript and Vite production build passed.
- `powershell -ExecutionPolicy Bypass -File scripts\Test-InstallDev.ps1`: 36 installer ownership tests passed. The script writes only below its checked `%TEMP%\ArcGISProAgent-InstallerTests-{GUID}` root and temporary `USERPROFILE` fixtures.
- Current-machine layout check: `D:\arcgis_pro` contains both required files and `ArcGISPro.exe` reports product version `3.7.0.1901`.
- Static ownership/launch scan: production Task 3 code contains no recursive directory deletion, shell command, or ArcGIS termination; the only launch constructor is `Command::new(&installation.executable)`. Production deletion calls target only the manifest destination, ownership record, or sibling temporary files.
- Scoped production-library Clippy passed with the two pre-existing `commands.rs` lint categories allowed. Strict all-target Clippy remains blocked by pre-existing warnings in unrelated command/test lines.

## Self-review

- Production discovery preserves priority even when optional saved/registry candidates are absent and never treats a directory name alone as an installation.
- File-version metadata, not a path label, determines 3.7 compatibility.
- Repair refuses an existing destination without a valid ownership record and refuses a manifest-owned destination whose current SHA-256 changed.
- Cleanup requires absolute exact destination ownership, refuses links/non-files, preserves hash changes, and never removes the Add-In root.
- ArcGIS process detection is read-only; repair does not terminate ArcGIS Pro and reports the restart requirement.
- Resource bytes are read once and the installed manifest hash is derived from those exact bytes.

## Concerns

- The preview Add-In resource itself is staged by the later packaging task; this work resolves both the direct resource path and the planned `generated/preview` resource path but cannot run packaged-resource repair end to end until that artifact exists.
- The real ArcGIS GUI was not launched during automated verification to avoid changing external application state. Discovery inputs and version were verified on the current machine, while safe process construction is covered by an automated test.
- Strict repository-wide Clippy is not clean because of pre-existing warnings outside the Task 3 diff; the Task 3 production library passes the scoped check described above.

## Formal review fixes

The follow-up fix keeps Task 3's scope and closes every item raised by formal review:

1. Windows Add-In ownership now rejects every reparse component, anchors the final directories with no-share-delete handles, verifies final object paths against those handles, and holds the exact package/manifest file objects exclusively from validation through mutation or deletion.
2. A real directory-junction test proves repair fails closed without writing through the junction. Controllable cleanup and repair race tests prove a competing remove/replace cannot intervene between hash validation and handle-bound mutation/deletion.
3. Repair updates the package and ownership record as a rollback transaction. A failure injected after the package sync and before manifest commit restores the previous package through the same exclusive handle and leaves the previous manifest byte-for-byte unchanged; rollback failure maps to `Unavailable` and never reports success.
4. Temporary siblings use 128 bits from `getrandom`, `create_new`, and cleanup only after this invocation successfully created the candidate. A collision test proves a pre-existing candidate is preserved and a fresh candidate is selected.
5. Production discovery now feeds saved, registry, and standard candidates through the sourced injectable selector; missing optional candidates preserve both priority and source labels in tests.
6. ArcGIS process enumeration has explicit `Running`, `NotRunning`, and `Unknown` states. Enumeration failure is `Unknown` and conservatively requires restart whenever repair changed the package.
7. All settings load-modify-save commands, including ArcGIS root persistence, share one mutation mutex. The existing barrier test now overlaps provider, credential, snapshot, and ArcGIS-root writes and proves no update is lost.
8. Filesystem tests use `tempfile::TempDir`, giving every test an exclusively created, strongly unique owned root.

### Follow-up RED / GREEN evidence

- Real junction RED: repair followed a junction at the Add-In root and wrote outside the owned tree. GREEN: the same test now returns an error and the external sentinel remains untouched.
- Cleanup race RED: the handle-bound cleanup seam was absent. GREEN: a competing `remove_file` gets a sharing violation while the verified object is held, and cleanup deletes that exact object through its handle.
- Repair race RED: the handle-bound repair seam was absent. GREEN: a competing removal is blocked while the already-hashed package is updated through the same handle.
- Transaction RED: `repair_addin_with_hook` was absent. GREEN: injected manifest-commit failure restores v1 package bytes and preserves the original manifest bytes.
- Temp collision RED: the candidate seam was absent. GREEN: a pre-existing collision sentinel is unchanged and the next exclusive candidate receives the staged bytes.
- Sourced discovery RED: the sourced selector was absent. GREEN: saved/standard source labels remain correct when optional candidates are missing.
- Process failure RED: the explicit process state seam was absent. GREEN: enumeration error maps to `Unknown` and requires restart for a changed package.
- Shared settings lock RED: ArcGIS-root persistence could complete while a provider mutation was paused. GREEN: it remains blocked at the barrier and both settings survive after release.

### Follow-up verification

- `cargo test --test arcgis_install`: 17 passed.
- `cargo test`: every suite passed; the pre-existing fake app-server test remains ignored (20 passed/1 ignored in that suite).
- `cargo fmt --all -- --check`: passed.
- `npm.cmd run build`: TypeScript and Vite production build passed.
- `powershell -ExecutionPolicy Bypass -File scripts\Test-InstallDev.ps1`: all 36 installer ownership tests passed.
- `git diff --check`: passed.

## Option A amendment: official Esri installation utility

The user selected Option A after the direct-mutation design exposed an unresolved Windows handle-relative atomic-rename boundary. Option A supersedes every earlier report statement about desktop repair, ownership manifests, hash ownership, cleanup, or automatic uninstall.

### Final behavior

1. ArcGIS Pro 3.7 discovery, sourced priority, saved/manual selection, snapshot persistence, process-state detection, and direct shell-free ArcGIS launch remain unchanged.
2. The desktop resolves only `ArcGISProAgent.AddIn.esriAddInX` at one of the two fixed application-resource layouts used by development and planned packaging. Missing resources, directories, symlinks, and differently named packages are rejected.
3. `open_addin_installer` accepts no user path. It passes the exact resolved bundled package to `tauri_plugin_opener::open_path` with no selected program; on Windows the enabled `open/shellexecute-on-windows` feature invokes the registered file association without `cmd.exe` or command-string concatenation.
4. `addin_uninstall_guidance` returns instructions to open ArcGIS Pro and use Project/Settings > Add-In Manager > Delete this Add-In. It performs no filesystem operation.
5. Installer-open results expose only the fixed package name and conservative `requiresRestart` guidance. `Running` and process-enumeration `Unknown` both require restart guidance; `NotRunning` does not.
6. Production desktop code contains no Add-Ins root, ownership manifest, SHA-256 ownership, package copying/replacement, cleanup deletion, `SetFileInformationByHandle`, or `NtSetInformationFile` implementation.

### Option A RED / GREEN evidence

- RED: `cargo test --test arcgis_install -- --nocapture` failed with E0432 because `ADDIN_UNINSTALL_GUIDANCE`, `open_packaged_addin_with`, and `uninstall_guidance` did not exist.
- GREEN: the focused suite passes 12 tests covering discovery/launch retention, exact bundled-resource resolution, missing/non-file/unexpected rejection, the registered-association opener seam, opener failure, conservative unknown-process restart guidance, uninstall guidance, and a production-source mutation scan.

### Final verification

- `cargo test --test arcgis_install -- --nocapture`: 12 passed.
- `cargo test`: every suite passed; the pre-existing fake app-server test remains ignored (20 passed/1 ignored in that suite).
- `cargo fmt --all -- --check`: passed.
- `npm.cmd run build`: TypeScript and Vite production build passed.
- `powershell -ExecutionPolicy Bypass -File scripts\Test-InstallDev.ps1`: all 36 independent foundation installer ownership tests passed without script changes.
- Production static scan returned no matches for the retired repair, cleanup, ownership, Add-Ins mutation, SHA-256, `SetFileInformationByHandle`, or `NtSetInformationFile` tokens.

## Option A resolver hardening follow-up

The final review tightened the bundled-resource trust boundary without changing the Esri handoff or any foundation-installer behavior.

### Final resolver behavior

1. The application resource root and the selected package are canonicalized before the package is opened.
2. The selected package must be a regular file, must not itself be a symbolic link, and its canonical parent must be exactly one of the two canonical application-owned layouts: the resource root or `generated/preview` below it.
3. Every accepted package remains within the canonical resource root. A symlink or junction in the allowed ancestor layout is rejected rather than followed outside that root.
4. The opener receives only the verified canonical package path. Every resolution failure retains the fixed redacted `Bundled ArcGIS Pro Add-In is unavailable` error.
5. The architecture regression test now recursively scans every production `.rs` file below `apps/desktop/src-tauri/src`, instead of inspecting only the Add-In and command modules.

### Follow-up RED / GREEN evidence

- Canonical opener RED: the opener received the original lexical package path instead of the canonical path. GREEN: the opener assertion now receives the canonical package path.
- Ancestor junction RED: a real `resources/generated` junction targeting an outside tree resolved successfully. GREEN: the same real junction is rejected and the outside package remains unchanged.
- Direct symlink GREEN: an elevated Windows test creates a real package-file symbolic link and proves it is rejected with the fixed redacted error. The test is permission-gated in the default suite and is run explicitly with `--ignored` under a token that can create symbolic links.
- Architecture coverage GREEN: the regression test enumerates the entire production Rust source tree recursively and rejects the retired mutation/ownership protocol tokens in any module.

### Follow-up verification

- `cargo test --test arcgis_install -- --nocapture`: 13 passed; the permission-gated direct-file symlink test was ignored in this default-token run.
- `cargo test --test arcgis_install bundled_addin_resolution_rejects_a_direct_file_symlink_with_a_redacted_error -- --ignored --nocapture`: 1 passed under the elevated Windows token.
- `cargo test`: every suite passed; Task 3 reported 13 passed/1 permission-gated ignored, and the pre-existing fake app-server test remains ignored (20 passed/1 ignored in that suite).
- `cargo fmt --all -- --check`: passed.
- `npm.cmd run build`: TypeScript and Vite production build passed.
- Full production-Rust static scan returned no matches for the retired Add-Ins mutation, ownership, repair, cleanup, SHA-256, `USERPROFILE`, `SetFileInformationByHandle`, or `NtSetInformationFile` tokens.
- The independent foundation installer suite was not rerun because this follow-up changes only desktop Rust resource resolution and its tests. The immediately preceding Option A verification remains 36/36 passed with no foundation script changes.

## Resource-root reparse-point follow-up

### RED / GREEN evidence

- RED: `cargo test --test arcgis_install bundled_addin_resolution_rejects_a_resource_root_directory_junction -- --nocapture` failed because a lexical application resource root implemented as a Windows directory junction was canonicalized before trust validation. The resolver accepted the fixed package in the outside target tree and the opener returned success instead of the required fixed `Unavailable` error.
- GREEN: before canonicalization, `packaged_addin_path` now validates the lexical resource root using `symlink_metadata`: it must exist and be a directory, must not be a symbolic link, and on Windows must not have `FILE_ATTRIBUTE_REPARSE_POINT`. The same real junction test passes: it returns `Unavailable`, does not call the opener, and leaves the outside fixed-name package byte-for-byte unchanged. The existing canonical allowed-parent and candidate checks remain in place.

### Follow-up verification

- Focused resource-root regression: `cargo test --test arcgis_install bundled_addin_resolution_rejects_a_resource_root_directory_junction -- --nocapture` passed (1 passed).
- Full Task 3 focused suite: `cargo test --test arcgis_install -- --nocapture` passed (14 passed, 1 existing permission-gated direct-file-symlink test ignored).
- `cargo fmt --all -- --check` passed after formatting.
- `npm.cmd run build` passed (TypeScript and Vite production build).
- Full `cargo test` executed all suites through `settings_credentials`; Task 3 and all preceding suites passed, but two pre-existing Windows Credential Manager tests failed because the current Windows session lacks a logon session (`0x80070520` / `Unavailable`). This follow-up does not touch credential code.
- Full production-Rust static scan returned no retired Add-Ins mutation or ownership tokens. The independent foundation installer suite was not rerun because this desktop-only resolver change does not modify foundation scripts; the preceding Option A evidence remains 36/36.

### Controller verification of the Credential Manager environment failure

- The first post-fix full Rust run executed under the sandbox identity `codexsandboxonline`, while the active Windows console session belonged to `Administrator`; the two real Windows Credential Manager tests failed with `0x80070520` because the sandbox identity had no interactive logon session.
- Re-running `cargo test --test settings_credentials -- --nocapture` in the active Administrator logon session passed 14/14.
- Re-running the full `cargo test` suite in that same session passed every active test: Task 3 passed 14 with one permission-gated direct-file-symlink test ignored, the Codex fake app-server helper remained intentionally ignored, and all 14 settings/credential tests passed.
