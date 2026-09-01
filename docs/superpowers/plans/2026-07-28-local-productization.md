# ArcGIS Pro Agent Local Productization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development`, `superpowers:test-driven-development`, `superpowers:requesting-code-review`, and `superpowers:verification-before-completion`.

**Goal:** Complete design-spec Phase 5 and the local first-release acceptance gate: maintenance/settings/history/diagnostics, repair and safe uninstall, reproducible packaging, accessibility/performance/crash recovery, full ArcGIS Pro 3.7 evidence, and release documentation.

**Prerequisite:** Phases 2–4 have passed their acceptance gates.

## Global Constraints

- Local-only first release; no auto-update, telemetry, cloud storage, API-key authentication or public distribution signing in this milestone.
- Install/repair/uninstall may touch only manifest-owned application/Add-In files. Config, logs, history and backups require separate explicit choices; `.aprx`, geodatabases, shapefiles, exports and arbitrary GIS sidecars are never installation-owned.
- All path operations retain the existing fail-closed entity/reparse/hard-link/topology checks, exclusive lock, durable journal, crash recovery and hash verification.
- Diagnostic bundles redact account identifiers, tokens, conversation text, feature values, connection strings, credentials and private path segments by default.
- Completion requires fresh command output and real-machine evidence. A pending manual step prevents the release tag and any claim of completion.

---

### Task 1: Settings, History, Logs, and Maintenance UI

**Files:**
- Create: `apps/desktop/src/components/SettingsView.tsx`
- Create: `apps/desktop/src/components/HistoryView.tsx`
- Create: `apps/desktop/src/components/MaintenanceView.tsx`
- Modify: `apps/desktop/src/domain.ts`
- Modify: `apps/desktop/src/desktopApi.ts`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/components/Sidebar.tsx`
- Modify: `apps/desktop/src/app.css`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/paths.rs`
- Modify: `apps/desktop/src-tauri/src/history.rs`
- Create: `apps/desktop/src-tauri/src/settings.rs`
- Create: `apps/desktop/src-tauri/src/logging.rs`

- [ ] Write failing Rust/frontend tests for schema-versioned atomic settings, R1 ask toggle, history filter/clear, retention limits, path reveal allowlist, log rotation and redaction.
- [ ] Add Chinese settings/history/maintenance screens with keyboard navigation and visible focus. Display install, Add-In, config, logs, source, backups and outputs locations; reveal only predefined canonical directories.
- [ ] Keep ChatGPT credentials under official Codex ownership. Store no API key/token/account email in application settings/history/logs.
- [ ] Add separate clear-history/reset-settings actions with confirmation; neither action touches GIS data, outputs or backups.
- [ ] Run focused/full frontend and Rust suites; commit Task 1.

### Task 2: Redacted Diagnostic Bundle

**Files:**
- Create: `apps/desktop/src-tauri/src/diagnostics.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src/components/MaintenanceView.tsx`
- Create: `apps/desktop/src-tauri/tests/diagnostics.rs`

- [ ] Write failing tests with seeded tokens, emails, conversation text, database credentials, UNC/local paths and feature values; assert none appear in the archive.
- [ ] Export a ZIP containing app/Add-In/MCP/protocol versions, capability manifest, redacted recent logs, settings schema, install-manifest verification, and test timestamps. Never include runtime secret files or raw history.
- [ ] Use a user-selected output path with canonicalization/collision confirmation; verify archive entries and size before success.
- [ ] Run focused/full suites; commit Task 2.

### Task 3: Repair and Safe Uninstall

**Files:**
- Create: `scripts/Repair-Install.ps1`
- Create: `scripts/Uninstall.ps1`
- Create: `scripts/Uninstall.Core.psm1`
- Create: `scripts/Test-Uninstall.ps1`
- Modify: `scripts/Install-Dev.Core.psm1`
- Modify: `apps/desktop/src/components/MaintenanceView.tsx`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `scripts/Test-Foundation.ps1`

- [ ] Write failing PowerShell tests for valid/tampered manifests, hard links, junction races, locked files, partial crashes, idempotence, rollback, and preservation of GIS/config/log/history/backup/output sentinels.
- [ ] Repair verifies ownership/hashes and transactionally reinstalls missing/corrupt owned files without claiming ownership of pre-existing unowned targets.
- [ ] Uninstall first stops only application-owned processes, then removes only hash/identity-verified manifest entries and the manifest. Preserve suspicious files and return a manual-review report.
- [ ] Offer separate opt-in removal of config/log/history after executable uninstall; backups/outputs/GIS data have no delete option.
- [ ] Run PowerShell and aggregate suites; commit Task 3.

### Task 4: Reproducible Local Package and Version Consistency

**Files:**
- Modify: `apps/desktop/src-tauri/tauri.conf.json`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/package.json`
- Modify: `src/ArcGISProAgent.Contracts/ArcGISProAgent.Contracts.csproj`
- Modify: `src/ArcGISProAgent.Bridge/ArcGISProAgent.Bridge.csproj`
- Modify: `src/ArcGISProAgent.Mcp/ArcGISProAgent.Mcp.csproj`
- Modify: `src/ArcGISProAgent.AddIn/ArcGISProAgent.AddIn.csproj`
- Create: `scripts/Build-Release.ps1`
- Create: `scripts/Test-ReleasePackage.ps1`

- [ ] Write failing tests for one canonical release version, protocol compatibility, package contents, hashes, duplicate/missing files, absolute build paths, forbidden secrets and absence of GIS data.
- [ ] Build Release-mode MCP, `.esriAddInX`, Tauri installer/bundle and maintenance scripts into a staging directory, then emit a signed-by-hash release manifest. Code signing is documented as a future distribution prerequisite, not faked.
- [ ] Ensure a clean build resolves ArcGIS Pro from `ArcGISProInstallDir`/`ARCGIS_PRO_HOME`/registry and succeeds with `D:\arcgis_pro` without `C:\Program Files\ArcGIS\Pro` literals.
- [ ] Run release package verification twice and compare the declared file set/digests where deterministic; document unavoidable toolchain metadata.
- [ ] Commit Task 4.

### Task 5: Accessibility, Performance, and Crash-Recovery Hardening

**Files:**
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/app.css`
- Modify: `apps/desktop/src-tauri/src/app_state.rs`
- Modify: `apps/desktop/src-tauri/src/codex/client.rs`
- Modify: `apps/desktop/tests/App.test.tsx`
- Modify: `apps/desktop/src-tauri/tests/command_lifecycle.rs`
- Create: `docs/development/performance-and-accessibility.md`

- [ ] Write failing tests for focus order/trap/return, keyboard approval actions, live-region throttling, contrast classes, 200-card virtualization/bounds, restart with pending approval, corrupt state and child-process crash.
- [ ] Bound all UI/runtime queues and files; keep conversation interaction responsive during long ArcGIS operations. Restore only safe non-secret preferences and redacted history after restart; never restore pending R2/R3 execution.
- [ ] Measure and record startup, idle polling, context refresh and large-history rendering on the local machine with explicit thresholds; fix threshold breaches before acceptance.
- [ ] Run full suites; commit Task 5.

### Task 6: Full Real-Machine Acceptance, Documentation, and Release Gate

**Files:**
- Create: `docs/user-guide.md`
- Create: `docs/development/architecture.md`
- Create: `docs/development/extending-tools.md`
- Create: `docs/development/uninstall-and-recovery.md`
- Create: `docs/development/release-acceptance.md`
- Modify: `README.md`
- Modify: `.superpowers/sdd/progress.md`

- [ ] Run a fresh aggregate verification and release build from the real worktree; save non-sensitive command summaries and exact versions.
- [ ] Transactionally install the release candidate. In a disposable ArcGIS Pro 3.7 project verify login, connect, read, select, navigate, buffer, add output, style, label, export map/layout, preview/update/undo, decline R2/R3, disconnect/reconnect and app restart.
- [ ] Exercise explicit failure evidence: bridge timeout before dispatch; protocol-version mismatch; a locked disposable dataset; one registered tool failure; and a pipe drop after write dispatch. Verify the UI shows a traceable public error or `result_unknown` operation ID, reconciliation uses `arcgis_get_operation_status`, and no write is automatically replayed.
- [ ] Run repair, diagnostic export and uninstall; verify manifest-owned files are removed while disposable project/data, outputs, backups, config/log choices and unrelated Add-Ins remain as documented.
- [ ] Reinstall and repeat the connection smoke. Review the module manifest against registered MCP tools and `CapabilityCatalog`; every first-release tool must be documented.
- [ ] Dispatch an independent whole-branch code/security review. Fix all Critical/Important findings and re-review.
- [ ] Only after every checklist item is observed, create the final commit and local annotated release tag. Do not push or publish without a separate user request.

## Acceptance Gate

- All ten acceptance criteria in the approved design specification are backed by fresh automated and real-machine evidence.
- Repair/uninstall/diagnostics have fail-closed adversarial tests and preserve GIS data.
- No manual step remains `pending`, the worktree is clean, independent review is approved, and the local release artifact can be found, modified, repaired and safely removed.
