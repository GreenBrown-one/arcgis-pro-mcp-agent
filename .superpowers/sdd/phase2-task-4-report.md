# Phase 2 Task 4 Report: R1 Selection and Navigation

## Status

`DONE_WITH_CONCERNS`

Verified implementation commit: `309980c7c486990a131f17cc3ea304567db10f30` (`feat(addin): implement R1 selection and navigation`).

The implementation and all non-GUI foundation checks are complete. The concern is an accidental ArcGIS SDK Add-In registration attempt during the first standalone package build, documented below. No ArcGIS Pro GUI, project, map, source data, or live GIS operation was opened or invoked.

## RED evidence

The source allowlist was advanced before production changes and run with:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\Test-Foundation.ps1 -SourceAssertionsOnly
```

Expected result: exit `1`; the assertion expected the exact 17-tool R1 allowlist and found the existing 10-tool R0 allowlist.

Focused MCP/reflection tests were then added before MCP production changes and run with:

```powershell
dotnet test tests\ArcGISProAgent.Mcp.Tests\ArcGISProAgent.Mcp.Tests.csproj --configuration Release --no-restore --filter "FullyQualifiedName~GisReadToolsTests|FullyQualifiedName~McpToolMetadataTests" --logger "console;verbosity=normal"
```

Expected result: 21 total, 10 passed, 11 failed. Failures were the absent R1 tool type/methods, exact 17-tool allowlist, signatures, mappings, and exception-propagation surfaces. There were no test compilation errors.

A subsequent metadata RED exposed that ModelContextProtocol 1.4.1 defaults `Destructive=true`: 20 of 21 focused tests passed, while the all-tools non-destructive assertion failed for the ten existing R0 tools. The binding brief was updated to authorize metadata-only changes, and all 17 tools now explicitly set `Destructive=false`.

## Exact public MCP signatures

```csharp
Task<SelectionResult> SelectByAttributeAsync(
    string layerUri,
    AttributePredicate predicate,
    SelectionCombinationMode mode = SelectionCombinationMode.Replace,
    CancellationToken cancellationToken = default)

Task<SelectionResult> SelectByLocationAsync(
    string layerUri,
    SpatialQuerySource source,
    SpatialRelation relation,
    SelectionCombinationMode mode = SelectionCombinationMode.Replace,
    CancellationToken cancellationToken = default)

Task<ClearSelectionResult> ClearSelectionAsync(
    string? layerUri = null,
    CancellationToken cancellationToken = default)

Task<ActivateViewResult> ActivateViewAsync(
    string itemUri,
    CancellationToken cancellationToken = default)

Task<ZoomResult> ZoomToLayerAsync(
    string layerUri,
    bool selectedOnly = false,
    CancellationToken cancellationToken = default)

Task<ZoomResult> ZoomToExtentAsync(
    MapExtent extent,
    CancellationToken cancellationToken = default)

Task<FlashFeaturesResult> FlashFeaturesAsync(
    string layerUri,
    IReadOnlyList<long> objectIds,
    int durationMilliseconds = 1000,
    CancellationToken cancellationToken = default)
```

Every wrapper constructs the existing typed DTO, calls `GisContractGuards.Validate`, forwards the exact operation and cancellation token through `IBridgeClient`, returns the typed result object unchanged, and does not catch or retry bridge failures.

## Exact R1 mappings and metadata

| MCP tool | Bridge operation | ReadOnly | Idempotent | Destructive |
|---|---|---:|---:|---:|
| `arcgis_select_by_attribute` | `selection.by_attribute` | false | false | false |
| `arcgis_select_by_location` | `selection.by_location` | false | false | false |
| `arcgis_clear_selection` | `selection.clear` | false | true | false |
| `arcgis_activate_view` | `map_view.activate` | false | true | false |
| `arcgis_zoom_to_layer` | `map_view.zoom_to_layer` | false | true | false |
| `arcgis_zoom_to_extent` | `map_view.zoom_to_extent` | false | true | false |
| `arcgis_flash_features` | `map_view.flash_features` | false | false | false |

All exact 17 compiled MCP tools explicitly set `Destructive=false`. R1 capability descriptors have `Risk=R1`, `SupportsCancellation=false`, `SupportsPreview=false`, `SupportsUndo=false`, `SupportsBackup=false`, `ModifiesProject=false`, `ModifiesData=false`, `ModifiesFileSystem=false`, and minimum ArcGIS Pro version `3.7`. No operation saves a project.

## Exact compiled tool allowlist (17)

1. `arcgis_activate_view`
2. `arcgis_capabilities`
3. `arcgis_clear_selection`
4. `arcgis_connection_status`
5. `arcgis_count_features`
6. `arcgis_describe_context`
7. `arcgis_describe_layer`
8. `arcgis_flash_features`
9. `arcgis_get_selection`
10. `arcgis_list_fields`
11. `arcgis_list_layers`
12. `arcgis_query_features`
13. `arcgis_query_spatial`
14. `arcgis_select_by_attribute`
15. `arcgis_select_by_location`
16. `arcgis_zoom_to_extent`
17. `arcgis_zoom_to_layer`

## Exact Add-In runtime capability allowlist (16)

1. `connection.health`
2. `context.describe`
3. `layers.list`
4. `layers.describe`
5. `layers.fields`
6. `query.feature_count`
7. `query.features`
8. `query.spatial`
9. `selection.describe`
10. `selection.by_attribute`
11. `selection.by_location`
12. `selection.clear`
13. `map_view.activate`
14. `map_view.zoom_to_layer`
15. `map_view.zoom_to_extent`
16. `map_view.flash_features`

No R2/R3 operation, raw SQL, user-controlled `WhereClause`, raw WKT/CIM/script, layer-name resolver, or generic MCP dispatcher is exposed.

## Handler and safety behavior

- Attribute selection reuses Task-3 `ResolvePredicateField`, `ReadPageObjectIds`, `OpenTable`, compatibility, and managed evaluator code. It scans at most 10,000 rows and returns fixed `request_too_large` beyond the bound. It selects only the matched OIDs through `BasicFeatureLayer.Select`, so ArcGIS layer definition queries and joins remain enforced. Matched IDs and feature values are not public results.
- Spatial selection reuses the typed spatial source, target projection, and spatial filter helpers. Selection modes map exactly to SDK `New`, `Add`, `Subtract`, and `XOR`.
- Every returned selection is disposed. Selection results count the actual final layer selection. Clear counts only layers/features that had selections and clears either one exact feature-layer URI or feature layers in the active map; map-wide clear without an active map returns `no_active_map`.
- Activation resolves only an existing map/scene/layout object by its stable URI. It GUI-activates an existing pane when present; otherwise it awaits `Map.OpenViewAsync` outside MCT for map/scene or GUI-dispatched `CreateLayoutPaneAsync` for layout. It never creates a project item.
- Zoom resolves/prepares compatible active-map targets on MCT, awaits `ZoomToAsync` outside MCT, and maps a false SDK completion value to fixed `navigation_interrupted`.
- Flash validates no more than 100 unique positive OIDs and an Add-In maximum duration of 10,000 ms, filters nonexistent OIDs without materializing public row data, GUI-invokes ArcGIS Pro 3.7 `MapView.FlashFeature` once per existing OID, then awaits the observation-window `Task.Delay` outside MCT. It does not mutate selection.
- Existing fixed public failures, unexpected-exception redaction to `arcgis_operation_failed`, and the final serialized 1 MiB response-size gate cover both synchronous and new asynchronous dispatcher branches.

## GREEN evidence and totals

Focused MCP/reflection tests:

```text
21 passed, 0 failed, 0 skipped
```

Source assertions:

```text
Task 3 source assertions passed.
```

Guarded Add-In Release compile/package against `D:\arcgis_pro`:

```text
0 errors; package produced at
src\ArcGISProAgent.AddIn\bin\Release\net10.0-windows\ArcGISProAgent.AddIn.esriAddinX
```

The SDK-generated packaging targets emitted two existing `CS0162` unreachable-code warnings from temporary generated files. No project source warning was emitted.

Exact full verification command:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\Test-Foundation.ps1 -ArcGISProInstallDir D:\arcgis_pro
```

The first sandboxed run passed installer tests (29/29) and then failed only because NuGet access to `https://api.nuget.org/v3/index.json` returned `NU1301`/`NU1900` TLS credential errors. The single authorized, narrowly scoped rerun of the exact command exited `0`:

- Installer safety: 29/29.
- .NET: Contracts 77/77, Bridge 25/25, MCP 32/32; 134/134 total.
- Frontend Vitest: 29/29 across four files.
- Rust: 43 passed, 0 failed, 1 intentionally ignored fake-process helper.
- TypeScript/Vite build, Tauri debug build, Cargo tests/doc tests, exact source allowlists, version, secret, API-key, runtime, and tracked-output guards all passed.
- Final line: `Foundation non-GUI verification passed.`

## Threading and runtime limitations

- Data access, layer/project-object resolution, selection access/mutation, OID filtering, and geometry preparation run on `QueuedTask`/MCT.
- Pane activation and layout pane creation run on the ArcGIS GUI scheduler. `CreateLayoutPaneAsync` never runs inside `QueuedTask`.
- Map/scene `OpenViewAsync`, async zoom completion, and flash observation delay are awaited outside MCT. There is no `.Result`, `.Wait()`, or thread sleep.
- The Add-In SDK actions used here do not accept cancellation; capability metadata therefore reports `SupportsCancellation=false`. MCP cancellation tokens are still forwarded exactly to bridge waiting.
- ArcGIS Pro was not launched and no live GIS/project/data behavior was exercised. Runtime confidence is based on typed wrapper tests, source assertions, local ArcGIS Pro 3.7 XML/API inspection, Release compilation, packaging, and the non-GUI foundation suite.

## Concern: accidental SDK registration attempt

The first standalone Add-In build omitted the foundation script's nonexistent `ArcGISFolder` no-register override. After producing:

```text
E:\Ai_Project\codex\MCP-Server-ArcGIS-Pro-AddIn\.worktrees\arcgis-pro-agent-foundation\src\ArcGISProAgent.AddIn\bin\Release\net10.0-windows\ArcGISProAgent.AddIn.esriAddinX
```

the Esri SDK target logged this invocation:

```text
RegisterAddIn.exe "E:\Ai_Project\codex\MCP-Server-ArcGIS-Pro-AddIn\.worktrees\arcgis-pro-agent-foundation\src\ArcGISProAgent.AddIn\bin\Release\net10.0-windows\ArcGISProAgent.AddIn.esriAddinX" /s
```

Likely effect: the SDK silently registered the developer Add-In package for discovery by the local ArcGIS Pro environment. It did not launch ArcGIS Pro or execute a GIS operation. Per parent direction, no unregister, deletion, or cleanup mutation was attempted. Every subsequent Add-In and full verification used the exact nonexistent guard path `ArcGISProAgent-foundation-no-register`; guarded output logged `Unable to execute RegisterAddIn.exe. ArcGIS Pro is not installed.` while compile/package still exited `0`.

## Root-review focused fixes after `9ca5774`

The root review required two behavioral corrections:

- Empty attribute or spatial matches no longer call `BasicFeatureLayer.Select` with an empty `ObjectIDs` filter. `Replace` clears the target layer selection; `Add`, `Remove`, and `Toggle` perform no mutation. Every branch reads and returns the actual final selection count.
- Existing map, scene, and layout panes are now enumerated from `FrameworkApplication.Panes.OfType<Pane>()`. Map/scene panes match the requested stable URI through `IMapPane.MapView.Map.URI`; layout panes match through `ILayoutPane.LayoutView.Layout.URI`. A match is activated, and a new view is opened or created only when no match exists.

Strict RED was established by adding source assertions before changing production code and running:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\Test-Foundation.ps1 -SourceAssertionsOnly
```

The run exited `1` with both expected failures:

```text
Empty attribute/spatial matches must clear only Replace, no-op Add/Remove/Toggle, and return the actual count without Select(empty ObjectIDs).
Existing panes must be enumerated from FrameworkApplication.Panes and matched through documented IMapPane/ILayoutPane view URIs.
```

After the production changes, the same command exited `0` with `Task 3 source assertions passed.`

Focused and full GREEN verification after these fixes:

- Focused MCP tests: 21/21 passed.
- Guarded Add-In Release build/package: 0 errors, with the same two Esri-generated `CS0162` warnings; the `.esriAddinX` package was produced.
- Cached .NET solution tests: 134/134 passed.
- Exact full foundation command: installer 29/29; .NET 134/134; frontend 29/29; Rust 43 passed, 1 intentionally ignored, 0 failed; TypeScript/Vite, Tauri debug, Cargo/doc tests, and all foundation guards passed. Final line: `Foundation non-GUI verification passed.`

An initial attempt to run the focused MCP tests and guarded Add-In build concurrently caused only a shared compiler-output collision (`CS2012` on `ArcGISProAgent.Contracts.dll`). Sequential reruns of both commands passed; no source change was needed for that orchestration-only failure.

The first exact full run inside the sandbox again reached the NuGet TLS credential restriction after installer 29/29. The one permitted narrowly scoped escalation reran the exact command and passed with the totals above.

`MapView.FlashFeature` remains GUI-dispatched. The local ArcGIS Pro 3.7 XML describes the method signature and behavior but does not specify a required calling context, so it is not evidence that GUI dispatch is required. Confirming its exact calling-context behavior remains pending a live ArcGIS Pro smoke test. No ArcGIS Pro GUI, live project, or GIS data operation was executed during this task.

The accidental SDK registration attempt documented above remains unchanged. Per parent direction, no unregister, deletion, or other cleanup mutation was performed.
