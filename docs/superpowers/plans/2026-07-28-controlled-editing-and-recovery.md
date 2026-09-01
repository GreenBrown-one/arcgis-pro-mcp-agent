# ArcGIS Pro Agent Controlled Editing and Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development`, `superpowers:test-driven-development`, and `superpowers:systematic-debugging` for any ArcGIS transaction failure.

**Goal:** Complete design-spec Phase 4 with preview-first R3 create/update/delete operations, snapshot revalidation, backup capability detection, native ArcGIS undo integration, and fail-closed recovery semantics.

**Prerequisite:** Phase 3 approval infrastructure has passed and cannot be bypassed.

## Global Constraints

- Every R3 call is two-step: preview returns an immutable snapshot token; execute requires both that token and a fresh user approval bound to the same canonical request.
- Snapshot tokens bind project path/fingerprint, map/layer URI, data-source fingerprint, schema fingerprint, operation kind, typed predicate or selected object IDs, matched count, sampled before-values, and expiry. Any change yields `snapshot_stale` and executes nothing.
- R3 is never auto-approved, remembered, retried, replayed after reconnect, or executed from a stale session.
- Use `EditOperation` for editable sources and keep changes on the ArcGIS undo stack. Saving edits/project is a separate R2/R3 action, not implicit.
- Before destructive changes, detect workspace/version/lock/transaction/undo/backup capability. File-backed data uses an approved, verified timestamped backup; enterprise/service sources without a demonstrable backup route require explicit `noRecovery` acknowledgement in both preview and approval.
- Delete/overwrite is never silent. Installation/uninstall code never owns GIS data, backups or exports.
- Public update values are typed scalars and field-validated. Geometry creation accepts only bounded GeoJSON converted by a dedicated parser; no WKT/SQL/script evaluation.

## Phase-4 Public Tools

- `arcgis_preview_create_features`, `arcgis_create_features` (R0 preview / R3 execute).
- `arcgis_preview_update_features`, `arcgis_update_features` (R0 preview / R3 execute).
- `arcgis_preview_delete_features`, `arcgis_delete_features` (R0 preview / R3 execute).
- `arcgis_edit_status` (R0): editability, lock/version, undo and backup capability.
- `arcgis_preview_data_repair`, `arcgis_run_data_repair` (R0 preview / R3 execute): only registered `append`, `define_projection`, and `repair_geometry` operations, with the same snapshot/backup rules as feature edits.
- `arcgis_preview_repair_data_source`, `arcgis_repair_data_source` (R0 preview / R3 execute): replace a broken connection only after source/target inspection, snapshot, backup capability check and approval.
- `arcgis_preview_undo_last_edit`, `arcgis_undo_last_edit` (R0 preview / R3 execute): only the application-owned most recent edit operation.
- `arcgis_preview_save_edits`, `arcgis_save_edits` and `arcgis_preview_discard_edits`, `arcgis_discard_edits` (R0 preview / R3 execute): explicit project edit boundaries bound to the current pending-edit set.

---

### Task 1: Snapshot, Backup, and Edit Capability Contracts

**Files:**
- Create: `src/ArcGISProAgent.Contracts/EditingContracts.cs`
- Create: `src/ArcGISProAgent.Mcp/Editing/EditSnapshotService.cs`
- Create: `src/ArcGISProAgent.Mcp/Editing/BackupPolicy.cs`
- Create: `tests/ArcGISProAgent.Mcp.Tests/EditSnapshotServiceTests.cs`
- Create: `tests/ArcGISProAgent.Mcp.Tests/BackupPolicyTests.cs`

- [ ] Write failing tests for canonical tokens, one-minute preview expiry, count/schema/source/selection drift, replay, typed values, geometry size/coordinate limits, backup-required decisions and enterprise no-recovery acknowledgement.
- [ ] Implement HMAC-protected opaque snapshot tokens using an application runtime secret distinct from the pipe token. Store only bounded redacted preview state in memory; restart invalidates all tokens.
- [ ] Define exact source capability outcomes: `nativeUndo`, `copyBackup`, `exportBackup`, `versionedEnterprise`, `noVerifiedRecovery`, with public rationale and required acknowledgement.
- [ ] Run focused and full .NET suites; commit Task 1.

### Task 2: Edit Status and Preview Operations

**Files:**
- Create: `src/ArcGISProAgent.Mcp/Tools/EditingTools.cs`
- Create: `src/ArcGISProAgent.AddIn/Operations/EditingPreviewOperations.cs`
- Modify: `src/ArcGISProAgent.AddIn/ArcGisOperationDispatcher.cs`
- Create: `tests/ArcGISProAgent.Mcp.Tests/EditingToolsTests.cs`

- [ ] Write failing MCP/catalog tests for R0 status/preview versus R3 execution metadata and exact bridge calls.
- [ ] Implement Add-In status inspection for layer editability, workspace type, version/lock indicators, pending project edits, undo availability and supported backup route.
- [ ] Preview create/update/delete using bounded counts and samples. For update/delete, resolve the typed predicate or current selection to a stable object-ID set and fingerprint it; never return all IDs/values to the model.
- [ ] Preview the three registered destructive geoprocessing operations (`append`, `define_projection`, `repair_geometry`) with exact inputs, affected target, count/schema fingerprint, lock/version state, backup route, and no-recovery warning. Reject every other tool ID.
- [ ] Preview broken-data-source repair with the old redacted connection summary, new canonical target, compatibility probe, layer/schema fingerprint, affected layers, backup route and exact rollback limits.
- [ ] Reject zero-match destructive previews by default, over-limit edits, unsupported geometry, noneditable fields, OID/global-ID edits and schema/system fields.
- [ ] Run focused/full suites and ArcGIS compile; commit Task 2.

### Task 3: Verified Backups and R3 Execute Path

**Files:**
- Create: `src/ArcGISProAgent.AddIn/Operations/BackupOperations.cs`
- Create: `src/ArcGISProAgent.AddIn/Operations/EditingExecuteOperations.cs`
- Modify: `src/ArcGISProAgent.Mcp/Tools/EditingTools.cs`
- Modify: `src/ArcGISProAgent.AddIn/ArcGisOperationDispatcher.cs`
- Modify: `src/ArcGISProAgent.AddIn/ArcGISProAgent.AddIn.csproj`

- [ ] Write failing tests for preview-token and approval-digest enforcement, backup-before-edit ordering, backup validation, mid-operation failure, and no automatic replay.
- [ ] Revalidate project/source/schema/object IDs immediately before execution. Create a timestamped backup under the approved backup root using only a registered copy/export route and verify its existence, count and source fingerprint before editing.
- [ ] Execute create/update/delete through one named `EditOperation` with an application event token. On failure return the SDK error and backup location; do not claim rollback unless ArcGIS confirms it.
- [ ] Execute a registered destructive geoprocessing operation only after identical snapshot, backup and R3 approval gates. It is not eligible for automatic retry or application-owned ArcGIS undo unless the SDK explicitly creates a matching undo entry.
- [ ] Repair a broken data-source connection only after rechecking the old/new connection summaries, target schema compatibility, affected-layer set, backup/no-recovery decision, snapshot token and R3 approval. Return before/after connection health and rollback limits; never expose or log credentials.
- [ ] Return operation ID, affected count, native undo availability, backup result and unsaved-edit state. Never auto-save.
- [ ] Compile/package and run all suites; commit Task 3.

### Task 4: Application-Owned Undo, Save, and Discard

**Files:**
- Create: `src/ArcGISProAgent.AddIn/Operations/EditRecoveryOperations.cs`
- Modify: `src/ArcGISProAgent.Mcp/Tools/EditingTools.cs`
- Modify: `src/ArcGISProAgent.AddIn/ArcGisOperationDispatcher.cs`
- Modify: `tests/ArcGISProAgent.Mcp.Tests/EditingToolsTests.cs`

- [ ] Write failing tests for the three preview tokens, ownership token, intervening user edit, empty undo stack, pending-edit fingerprint, save/discard approval, disconnect and stale session. Every stale/intervening/unowned case must assert zero ArcGIS SDK execute/save/discard calls.
- [ ] Allow undo only after `arcgis_preview_undo_last_edit` binds the current top undo entry, application event token, project/source fingerprint and fresh R3 approval. Otherwise return `undo_not_owned` without invoking ArcGIS undo.
- [ ] Preview save with the current pending-edit fingerprint and whether edits not owned by this application would also be persisted; require a separate explicit acknowledgement when ownership is mixed. Preview discard only when all pending edits can be proven application-owned; otherwise return `unowned_edits_present` and provide no executable token.
- [ ] Implement explicit approved save/discard using ArcGIS project APIs only after the preview token is revalidated. Surface conflicts/locks; never silently choose discard or save.
- [ ] Clear application recovery state after undo/save/discard and on project/session change. Run focused/full suites; commit Task 4.

### Task 5: R3 UI, Recovery Evidence, and Phase-4 Acceptance

**Files:**
- Modify: `apps/desktop/src/components/ApprovalCard.tsx`
- Modify: `apps/desktop/src/components/ConversationPane.tsx`
- Modify: `apps/desktop/src/domain.ts`
- Modify: `apps/desktop/src-tauri/src/history.rs`
- Modify: `apps/desktop/tests/conversationFlow.test.tsx`
- Create: `docs/development/phase-4-smoke.md`
- Modify: `README.md`
- Modify: `scripts/Test-Foundation.ps1`

- [ ] Write failing UI/history tests for affected count, sample-before values, backup/no-recovery warning, unsaved state, undo ownership, accept/decline and stale-preview presentation.
- [ ] Render R3 distinctly from R2 and require the user to acknowledge backup/no-recovery and exact target. Never offer “always allow” for R3.
- [ ] Record backup and undo identifiers without full feature data. Provide reveal buttons only for verified backup paths.
- [ ] Use a disposable file geodatabase test project to smoke-test one update and undo, create and delete, registered repair, broken-data-source repair, backup creation, stale preview rejection, approval decline, intervening edit rejection, save, mixed-ownership save warning, owned-only discard, and unowned discard rejection. Do not run against the user's real datasets.
- [ ] Run aggregate verification and commit after the real-machine record is complete.

## Acceptance Gate

- A demonstrated ArcGIS Pro 3.7 edit can be previewed, approved, executed, verified and natively undone.
- Backup-required operations cannot edit before a verified backup; no-recovery cases are unmistakable and require explicit acknowledgement.
- Stale/replayed/unowned operations fail closed, and all automated plus disposable-project tests pass.
