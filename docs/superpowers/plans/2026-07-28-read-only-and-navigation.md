# ArcGIS Pro Agent Read-Only and Navigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` and `superpowers:test-driven-development`; execute one task at a time and obtain a clean task review before continuing.

**Goal:** Complete design-spec Phase 2 by migrating the useful nicogis operations into the hardened bridge and adding typed project, layer, field, count, selection, and navigation tools with a live desktop context pane.

**Architecture:** Public MCP tools accept stable typed inputs and call versioned bridge operations. The Add-In resolves ArcGIS objects on `QueuedTask`, identifies layers by `Layer.URI`, returns structured errors, and never exposes raw exceptions. Pure validation and DTO behavior live in .NET 8 projects; ArcGIS Pro 3.7 SDK code remains in the .NET 10 Add-In.

**Baseline:** commit `42da100`; 29 installer, 39 .NET, 29 frontend, and 43 Rust tests pass. The old nicogis code at `c7f3d03` is reference material only.

## Global Constraints

- Preserve the authenticated current-user named pipe, 1 MiB frame limit, startup token, protocol `1.0`, timeout, cancellation, and structured bridge errors.
- Keep Contracts, Bridge, and MCP on `net8.0`; keep the ArcGIS Pro 3.7 Add-In on `net10.0-windows` with portable `ArcGISProInstallDir` resolution.
- Never use layer name as the authoritative identity. Return and accept `Layer.URI`; a convenience name must fail with `ambiguous_layer` when it is not unique.
- Public query inputs use a bounded typed predicate (`field`, allowed operator, typed scalar value), not arbitrary SQL, scripts, or expression strings.
- Results are bounded: feature samples default to 20, maximum 100; strings are truncated to 2,000 characters; counts use `long`.
- R0 tools are read-only and automatic. R1 selection/navigation tools are temporary, idempotent where specified, and do not save the project.
- SDK operations that cannot actually be interrupted must declare `SupportsCancellation: false` even though bridge I/O is cancellable.
- Every new tool is represented in Contracts, `CapabilityCatalog`, MCP metadata tests, user documentation, and the ArcGIS real-machine checklist.
- Do not edit, overwrite, delete, save, export, or create GIS data in this phase.

## Phase-2 Tool Manifest

| MCP tool | Bridge operation | Risk | Purpose |
|---|---|---:|---|
| `arcgis_describe_context` | `context.describe` | R0 | Project, maps/layouts, active view/map and extent summary |
| `arcgis_list_layers` | `layers.list` | R0 | Flattened layer tree with URI, hierarchy, type and visibility |
| `arcgis_describe_layer` | `layers.describe` | R0 | Source, spatial reference, geometry, connection and count summary |
| `arcgis_list_fields` | `layers.fields` | R0 | Field schema, alias, type, nullable/editable/domain summary |
| `arcgis_count_features` | `query.feature_count` | R0 | `long` count with optional typed predicate |
| `arcgis_query_features` | `query.features` | R0 | Bounded page of requested fields and stable object IDs |
| `arcgis_query_spatial` | `query.spatial` | R0 | Bounded query by source layer, explicit extent, or current view and an allowlisted spatial relation |
| `arcgis_get_selection` | `selection.describe` | R0 | Counts and bounded object-ID samples by layer URI |
| `arcgis_select_by_attribute` | `selection.by_attribute` | R1 | Replace/add/remove/toggle selection from a typed predicate |
| `arcgis_select_by_location` | `selection.by_location` | R1 | Replace/add/remove/toggle selection from a bounded spatial relation |
| `arcgis_clear_selection` | `selection.clear` | R1 | Clear one layer or the active map selection |
| `arcgis_activate_view` | `map_view.activate` | R1 | Open/activate an existing map, scene, or layout by stable project-item URI |
| `arcgis_zoom_to_layer` | `map_view.zoom_to_layer` | R1 | Zoom to layer or selected features and return real completion state |
| `arcgis_zoom_to_extent` | `map_view.zoom_to_extent` | R1 | Zoom/pan to an explicit bounded extent |
| `arcgis_flash_features` | `map_view.flash_features` | R1 | Temporarily flash bounded object IDs without changing data |

Existing `arcgis_connection_status` and `arcgis_capabilities` remain unchanged.

---

### Task 1: Typed Read/Navigation Contracts and Registry

**Files:**
- Create: `src/ArcGISProAgent.Contracts/ArcGisContextContracts.cs`
- Create: `src/ArcGISProAgent.Contracts/LayerContracts.cs`
- Create: `src/ArcGISProAgent.Contracts/QuerySelectionContracts.cs`
- Modify: `src/ArcGISProAgent.Contracts/Capabilities.cs`
- Modify: `src/ArcGISProAgent.Contracts/BridgeMessages.cs`
- Modify: `tests/ArcGISProAgent.Contracts.Tests/BridgeProtocolTests.cs`
- Create: `tests/ArcGISProAgent.Contracts.Tests/GisContractTests.cs`

- [ ] Write failing serialization/validation tests for every request/result, predicate/spatial operator, selection-combination mode, extent/coordinate/ID limit, URI requirement, and redacted error code.
- [ ] Run `dotnet test tests/ArcGISProAgent.Contracts.Tests/ArcGISProAgent.Contracts.Tests.csproj --no-restore`; verify the new tests fail for missing contracts.
- [ ] Add immutable records/enums and central `OperationCatalog` descriptors for every Phase-2 operation. Extend capability metadata with display name, module, project/data/filesystem mutation flags, minimum ArcGIS Pro version, and actual cancellation/preview/undo/backup support.
- [ ] Implement shared input guards without ArcGIS SDK references. Reject empty/oversized strings, unsupported operators, missing values, limits outside `1..100`, and duplicate requested fields.
- [ ] Re-run the focused tests and the complete .NET suite; commit only Task 1 files.

### Task 2: R0 Context, Layer, Field, Count, and Query MCP Surface

**Files:**
- Create: `src/ArcGISProAgent.Mcp/Tools/ContextTools.cs`
- Create: `src/ArcGISProAgent.Mcp/Tools/LayerTools.cs`
- Create: `src/ArcGISProAgent.Mcp/Tools/QuerySelectionTools.cs`
- Modify: `src/ArcGISProAgent.Mcp/Program.cs`
- Create: `tests/ArcGISProAgent.Mcp.Tests/GisReadToolsTests.cs`
- Modify: `tests/ArcGISProAgent.Mcp.Tests/McpToolMetadataTests.cs`

- [ ] Write failing tests using a recording `IBridgeClient` for exact operation IDs/DTOs, cancellation forwarding, structured results, spatial-query scope, and MCP metadata.
- [ ] Verify every R0 method is `ReadOnly=true`, `Idempotent=true`; ensure the tool list is an exact allowlist and contains no legacy `Ping`, `Echo`, arbitrary SQL, or generic operation dispatcher.
- [ ] Implement thin DI-based tool classes and explicit `.WithTools<T>()` registration.
- [ ] Run focused MCP tests and all .NET tests; commit Task 2.

### Task 3: Add-In R0 Dispatcher and ArcGIS Object Resolution

**Files:**
- Create: `src/ArcGISProAgent.AddIn/Operations/ArcGisObjectResolver.cs`
- Create: `src/ArcGISProAgent.AddIn/Operations/ContextOperations.cs`
- Create: `src/ArcGISProAgent.AddIn/Operations/LayerOperations.cs`
- Create: `src/ArcGISProAgent.AddIn/Operations/QueryOperations.cs`
- Modify: `src/ArcGISProAgent.AddIn/ArcGisOperationDispatcher.cs`
- Modify: `src/ArcGISProAgent.AddIn/ArcGISProAgent.AddIn.csproj`
- Modify: `scripts/Test-Foundation.ps1`

- [ ] Add source-level/contract tests that first fail because dispatcher handlers and exact capability IDs are absent.
- [ ] Resolve maps/layers from the active project and `Map.GetLayersAsFlattenedList()`. Treat no project/map, missing URI, ambiguous convenience name, wrong layer type, broken source, invalid field, invalid typed predicate, and ArcGIS SDK failure as distinct public error codes.
- [ ] Run all ArcGIS SDK access on `QueuedTask`; dispose tables, cursors, rows, feature classes, and selections deterministically.
- [ ] Implement bounded paging using object-ID ordering when supported. Support an allowlisted spatial relation against one resolved source layer, a validated extent, or the current view. Return JSON-safe scalar values only and summarize unsupported/blob/geometry values instead of serializing SDK objects.
- [ ] Compile/package the Add-In against `D:\arcgis_pro`; run the full non-GUI foundation verification; commit Task 3.

### Task 4: R1 Selection and Navigation End-to-End

**Files:**
- Create: `src/ArcGISProAgent.Mcp/Tools/MapViewTools.cs`
- Modify: `src/ArcGISProAgent.Mcp/Tools/QuerySelectionTools.cs`
- Create: `src/ArcGISProAgent.AddIn/Operations/SelectionOperations.cs`
- Create: `src/ArcGISProAgent.AddIn/Operations/MapViewOperations.cs`
- Modify: `src/ArcGISProAgent.AddIn/ArcGisOperationDispatcher.cs`
- Modify: `tests/ArcGISProAgent.Mcp.Tests/GisReadToolsTests.cs`
- Modify: `tests/ArcGISProAgent.Mcp.Tests/McpToolMetadataTests.cs`

- [ ] Write failing MCP and catalog tests for attribute/spatial selection modes, non-idempotent add/remove/toggle metadata, clear-selection scope, existing view activation, zoom/pan result propagation, bounded flashing and no automatic retry of non-idempotent R1 calls.
- [ ] Implement typed-predicate and allowlisted spatial selection with `New`, `Add`, `Subtract`, or `Xor` as requested; dispose returned selections and report actual `long` counts. Mark the public selection tools conservatively non-idempotent because their mode is argument-dependent.
- [ ] Implement clear for a target layer or all active-map feature layers; activate only existing map/scene/layout items by stable URI; implement zoom by layer/selection/extent and pan using validated extents, returning the SDK boolean and `navigation_interrupted` when false.
- [ ] Implement flashing for at most 100 validated object IDs and a bounded duration. Register R1 capabilities with `SupportsUndo=false`, `SupportsBackup=false`, and truthful cancellation/idempotence flags. Run focused and full suites; commit Task 4.

### Task 5: Live Context Pane, Tool Results, and Phase-2 Acceptance

**Files:**
- Modify: `apps/desktop/src/domain.ts`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src/components/ArcGisContextPane.tsx`
- Modify: `apps/desktop/src/components/ConversationPane.tsx`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/app.css`
- Modify: `apps/desktop/tests/App.test.tsx`
- Modify: `apps/desktop/tests/conversationFlow.test.tsx`
- Modify: `apps/desktop/src-tauri/tests/desktop_commands.rs`
- Create: `docs/development/phase-2-smoke.md`
- Modify: `README.md`
- Modify: `scripts/Test-Foundation.ps1`

- [ ] Write failing Rust/frontend tests for a bounded `arcgis_describe_context` poll, retained stale context, nested layer rendering, structured tool status/error cards, and no display of sensitive source connection strings.
- [ ] Extend the right pane with project/map/extent and a collapsible layer summary; keep polling visibility/thread-generation guards and ensure a stale response cannot overwrite a newer session.
- [ ] Show tool name, risk, duration, outcome, bounded summary, and public error code. Do not persist feature values or raw data-source paths.
- [ ] Update aggregate tests to assert exact MCP/capability parity and forbidden legacy operation names.
- [ ] Run `scripts/Test-Foundation.ps1`, install the new build transactionally, and perform the documented ArcGIS Pro 3.7 smoke with a disposable test project for every new public operation: context; nested layers; layer description; fields; count; paged attribute query; current-view/extent/source-layer spatial query; selection describe; attribute and location selection in replace/add/remove/toggle modes; clear; activate existing map/scene/layout; zoom layer/selection/extent; pan; bounded flash; and disconnect/reconnect. Record only non-sensitive evidence.
- [ ] Commit Task 5 only after automation passes; leave real-machine items explicitly `pending` until observed.

## Acceptance Gate

- All Phase-2 MCP tools are discoverable and match the capability catalog exactly.
- The nicogis behaviors for active map, nested layer listing, count, select-by-attribute, and zoom work through the new authenticated typed bridge.
- No R2/R3 operation is exposed; no raw SQL or arbitrary operation ID reaches ArcGIS Pro.
- Automated verification is green and the ArcGIS Pro 3.7 phase-2 smoke record is complete.
