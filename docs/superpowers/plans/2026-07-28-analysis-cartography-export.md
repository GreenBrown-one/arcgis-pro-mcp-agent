# ArcGIS Pro Agent Analysis, Cartography, and Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` and `superpowers:test-driven-development`; every task needs a clean task review.

**Goal:** Complete design-spec Phase 3 with an approval-bound R2 path, a strict geoprocessing registry, controlled output management, basic symbology/labeling, layout inspection/text updates, and map/layout export.

**Prerequisite:** `2026-07-28-read-only-and-navigation.md` has passed its acceptance gate.

## Global Constraints

- R2 execution is impossible until the desktop user accepts an MCP form elicitation. Decline, cancel, timeout, app restart, thread change, or parameter change executes nothing.
- Approval binds a canonical operation ID, normalized arguments, input identities, output path, overwrite policy, active project/session generation, and a SHA-256 digest. A digest is single-use and expires after five minutes.
- Every write has a caller-generated operation ID. The same normalized ID is bound into the approval, used as the bridge request ID, and recorded by the Add-In before SDK execution so a lost response can be reconciled without replay.
- R2 tools never accept arbitrary ArcGIS tool IDs, Python, ModelBuilder, shell commands, environment expansion, or unvalidated parameter arrays.
- Output defaults below `%LOCALAPPDATA%\ArcGISProAgent\outputs`; an explicit user path is allowed only after canonicalization and approval. Existing outputs require a separate overwrite confirmation and are never auto-retried.
- The geoprocessing allowlist is code-owned and versioned. Each entry declares exact ArcGIS tool ID, typed parameter schema, risk, timeout, output indices, cancellation support, result parser, and ArcGIS Pro minimum version.
- Failure/diagnostic output is redacted; logs store digests, tool IDs, timing, public paths chosen by the user, result locations, and public error codes, not credentials or full feature values.

## Phase-3 Public Tools

- `arcgis_preview_operation` (R0): canonical impact/target preview for any declared R2/R3 tool.
- `arcgis_run_analysis` (R2): one of `buffer`, `clip`, `intersect`, `union`, `erase`, `dissolve`, `merge`, `spatial_join`, `summary_statistics`, `frequency`, `project`, or `check_geometry`; every operation creates a new output and exact schemas come from the registry.
- `arcgis_create_map`, `arcgis_save_project`, `arcgis_save_project_copy`, `arcgis_set_map_spatial_reference` (R2): explicit project/map changes with target and impact confirmation.
- `arcgis_add_layer`, `arcgis_remove_layer`, `arcgis_update_layer` (R2): controlled map membership/name/order/visibility/transparency.
- `arcgis_set_single_symbol`, `arcgis_set_unique_value_renderer`, `arcgis_set_graduated_renderer`, `arcgis_apply_layer_style`, `arcgis_configure_labels` (R2).
- `arcgis_list_layouts` (R0), `arcgis_describe_layout` (R0), `arcgis_update_layout_text` (R2).
- `arcgis_create_layout` (R2): create a bounded basic layout or import an approved local layout template.
- `arcgis_export_map`, `arcgis_export_layout` (R2): PDF/PNG/JPEG only in first release.
- `arcgis_get_operation_status` (R0): reconcile long/uncertain operations by application operation ID without replay.

---

### Task 1: Approval Envelope and Interactive Desktop Elicitation

**Files:**
- Create: `src/ArcGISProAgent.Contracts/ApprovalContracts.cs`
- Create: `src/ArcGISProAgent.Contracts/OperationStatusContracts.cs`
- Create: `src/ArcGISProAgent.Mcp/Approval/OperationApprovalService.cs`
- Create: `tests/ArcGISProAgent.Mcp.Tests/OperationApprovalServiceTests.cs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/app_state.rs`
- Modify: `apps/desktop/src/domain.ts`
- Create: `apps/desktop/src/components/ApprovalCard.tsx`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/components/ConversationPane.tsx`
- Modify: `apps/desktop/tests/conversationFlow.test.tsx`
- Modify: `apps/desktop/src-tauri/tests/desktop_commands.rs`

- [ ] Write failing .NET/Rust/frontend tests for normalized operation IDs, canonical digests, accept/decline/cancel, five-minute expiry, single use, stale thread/session rejection, redacted display, and response payloads.
- [ ] Inject the concrete ModelContextProtocol 1.4.1 `McpServer` and call `McpServer.ElicitAsync` with a form schema containing an explicit confirmation boolean and immutable impact text. Add a compile/protocol test first that proves the concrete API is available, Codex advertises form elicitation capability, cancellation propagates, and accept/decline response shapes are parsed exactly. The MCP tool revalidates the normalized request after acceptance before bridge dispatch.
- [ ] Replace the foundation's fail-closed auto-decline with a bounded pending-approval state and Chinese approval card. Keep unknown server requests and non-ArcGIS elicitations rejected.
- [ ] Ensure window close, logout, Codex exit, ArcGIS disconnect, and turn interruption decline pending approvals and wake every waiter.
- [ ] Run focused .NET/Rust/frontend suites and security scans; commit Task 1.

### Task 2: Registered Geoprocessing and Output Policy

**Files:**
- Create: `src/ArcGISProAgent.Contracts/GeoprocessingContracts.cs`
- Create: `src/ArcGISProAgent.Mcp/Tools/GeoprocessingTools.cs`
- Create: `src/ArcGISProAgent.Mcp/Geoprocessing/GeoprocessingRegistry.cs`
- Create: `src/ArcGISProAgent.Mcp/Geoprocessing/OutputPathPolicy.cs`
- Create: `src/ArcGISProAgent.AddIn/Operations/GeoprocessingOperations.cs`
- Create: `src/ArcGISProAgent.AddIn/Operations/OperationResultLedger.cs`
- Create: `src/ArcGISProAgent.AddIn/Operations/OperationStatusOperations.cs`
- Modify: `src/ArcGISProAgent.AddIn/ArcGISProAgent.AddIn.csproj`
- Modify: `src/ArcGISProAgent.AddIn/ArcGisOperationDispatcher.cs`
- Create: `tests/ArcGISProAgent.Mcp.Tests/GeoprocessingRegistryTests.cs`
- Create: `tests/ArcGISProAgent.Mcp.Tests/OutputPathPolicyTests.cs`

- [ ] Write failing tests for the exact non-destructive allowlist, explicit rejection of `append`, `define_projection`, `repair_geometry`, and unknown IDs, per-tool parameter types/ranges, traversal/device/UNC/reparse rejection, output collision policy, digest binding, timeout, cancellation and no automatic write retry.
- [ ] Implement immutable registry entries that convert only validated DTOs to `Geoprocessing.MakeValueArray` arguments. Do not expose registry internals as user-editable configuration in first release.
- [ ] Run `Geoprocessing.ExecuteToolAsync` with declared flags, bounded timeout and cancelable progressor; parse messages/results into structured public DTOs and add successful outputs only when the registry says so. Spatial selection remains the Phase-2 R1 tool and is not routed through the R2 analysis entry point.
- [ ] Revalidate project, input layer URIs, output nonexistence/approved overwrite, and approval digest immediately before execution.
- [ ] Extend the bridge call path so the approved operation ID is the `BridgeRequest.RequestId`. In the Add-In, persist a bounded in-memory ledger entry before SDK execution, then atomically transition it through `accepted`, `running`, and `succeeded` or `failed` with a redacted structured result. Expose R0 bridge operation `operation.status` and register it in `CapabilityCatalog`/contracts/MCP as `arcgis_get_operation_status`.
- [ ] Test operation status for accepted/running/succeeded/failed/unknown, reconnect, bounded TTL/LRU eviction, duplicate operation-ID rejection, and a dropped response after successful execution. If the pipe drops after dispatch, return `result_unknown` with the operation ID; only status lookup may reconcile it, and every test must prove zero write replay.
- [ ] Compile against ArcGIS Pro 3.7 and run all suites; commit Task 2.

### Task 3: Controlled Layer, Symbology, and Labeling Changes

**Files:**
- Create: `src/ArcGISProAgent.Contracts/CartographyContracts.cs`
- Create: `src/ArcGISProAgent.Mcp/Tools/LayerManagementTools.cs`
- Create: `src/ArcGISProAgent.Mcp/Tools/ProjectMapTools.cs`
- Create: `src/ArcGISProAgent.Mcp/Tools/CartographyTools.cs`
- Create: `src/ArcGISProAgent.AddIn/Operations/LayerManagementOperations.cs`
- Create: `src/ArcGISProAgent.AddIn/Operations/ProjectMapOperations.cs`
- Create: `src/ArcGISProAgent.AddIn/Operations/CartographyOperations.cs`
- Create: `tests/ArcGISProAgent.Mcp.Tests/CartographyToolsTests.cs`

- [ ] Write failing tests for exact R2 metadata, color/size/transparency/class-count limits, field compatibility, label expression restrictions, and approval binding.
- [ ] Implement add/remove/rename/order/visible/transparency without deleting source data. Return before/after summaries and the target layer URI.
- [ ] Implement creating maps, setting a map spatial reference, saving the project, and saving a project-file copy as separate approved operations. Save-copy must disclose that it copies only the project file and still references the same source data. Existing-map/layout activation remains the Phase-2 R1 tool.
- [ ] Implement single symbol plus unique-value/graduated renderers using SDK renderer factories; cap generated classes at 50. Implement basic label enable/disable, one verified field/expression template, font, size and color.
- [ ] Apply only an approved canonical local `.lyrx` path after inspecting the target layer/type compatibility and showing it in the approval. Never accept raw CIM as a style template.
- [ ] Reject arbitrary CIM JSON and Arcade/Python expressions. Complex CIM remains unavailable unless later added as a named tested capability.
- [ ] Compile/package and run focused/full suites; commit Task 3.

### Task 4: Layout Inspection, Text Update, and Export

**Files:**
- Create: `src/ArcGISProAgent.Contracts/LayoutExportContracts.cs`
- Create: `src/ArcGISProAgent.Mcp/Tools/LayoutExportTools.cs`
- Create: `src/ArcGISProAgent.AddIn/Operations/LayoutExportOperations.cs`
- Create: `tests/ArcGISProAgent.Mcp.Tests/LayoutExportToolsTests.cs`

- [ ] Write failing tests for layout/element stable IDs, element type filtering, basic/template layout limits, canonical `.pagx` paths, text length, PDF/PNG/JPEG extension-format agreement, DPI `72..600`, pixel limits, overwrite policy and approval digest.
- [ ] Implement R0 layout/element summaries, R2 update of existing text elements, and R2 creation of a bounded basic layout or import from an approved compatible local `.pagx` template. Do not accept arbitrary CIM or create unsupported free-form elements.
- [ ] Export active map or named layout with SDK `PDFFormat`, `PNGFormat`, or `JPEGFormat`; validate `ExportFormat.ValidateOutputFilePath`, canonical path, free space, and final file existence/size.
- [ ] Return a revealable result path but never open it automatically. Run focused/full suites and commit Task 4.

### Task 5: Progress, Result Cards, History, and Phase-3 Acceptance

**Files:**
- Modify: `apps/desktop/src/domain.ts`
- Modify: `apps/desktop/src/components/ConversationPane.tsx`
- Modify: `apps/desktop/src/app.css`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Create: `apps/desktop/src-tauri/src/history.rs`
- Modify: `apps/desktop/tests/conversationFlow.test.tsx`
- Create: `docs/development/phase-3-smoke.md`
- Modify: `README.md`
- Modify: `scripts/Test-Foundation.ps1`

- [ ] Write failing tests for pending/running/cancelled/succeeded/failed tool cards, bounded GP messages, result reveal, approval audit records, redaction, and crash-safe JSONL history rotation.
- [ ] Stream available progress and preserve operation ID across approval, execution and result. Cancellation must produce a terminal card and never trigger a retry.
- [ ] Persist a local bounded redacted execution record; never persist elicitation content containing credentials, full rows, or connection strings.
- [ ] Run aggregate verification, transactional install and disposable ArcGIS Pro smoke for every new public operation: each registered non-destructive analysis family with representative compatible inputs; add/remove/update layer; create map; save project and save project copy; set map spatial reference; single/unique/graduated symbol; apply compatible `.lyrx`; label toggle; list/describe/update layout; create basic layout; import compatible `.pagx`; PDF/PNG/JPEG map/layout export; operation-status lookup; decline; cancel; and disconnect during approval. Use disposable project copies for save/SR tests.
- [ ] Commit after evidence is recorded; mark unobserved manual checks `pending`.

## Acceptance Gate

- Every R2 tool is impossible to execute without a fresh matching acceptance.
- At least one spatial analysis, one style change, and one map/layout export pass end to end on ArcGIS Pro 3.7.
- Registry and output-policy tests prove there is no arbitrary tool/script/path execution channel.
- Automated and documented real-machine verification are green.
