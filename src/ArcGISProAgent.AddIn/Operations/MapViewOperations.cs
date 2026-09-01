using ArcGIS.Core.Data;
using ArcGIS.Core.Geometry;
using ArcGIS.Desktop.Core;
using ArcGIS.Desktop.Framework;
using ArcGIS.Desktop.Framework.Contracts;
using ArcGIS.Desktop.Framework.Threading.Tasks;
using ArcGIS.Desktop.Layouts;
using ArcGIS.Desktop.Mapping;
using ArcGISProAgent.Contracts;

namespace ArcGISProAgent.AddIn.Operations;

internal static class MapViewOperations
{
    private const int MaximumFlashDurationMilliseconds = 10_000;

    internal static async Task<ActivateViewResult> ActivateAsync(
        ActivateViewArguments arguments)
    {
        var target = await QueuedTask.Run(() => ResolveActivationTarget(arguments.ItemUri));
        var activated = await RunOnGuiAsync(async () =>
        {
            if (target.Map is not null)
            {
                var pane = FindExistingMapPane(arguments.ItemUri);
                if (pane is not null)
                {
                    pane.Activate();
                }
                else
                {
                    await target.Map.OpenViewAsync();
                }

                return string.Equals(
                    MapView.Active?.Map?.URI,
                    arguments.ItemUri,
                    StringComparison.Ordinal);
            }

            var layout = target.Layout!;
            var layoutPane = FindExistingLayoutPane(arguments.ItemUri);
            if (layoutPane is not null)
            {
                layoutPane.Activate();
            }
            else
            {
                await FrameworkApplication.Panes.CreateLayoutPaneAsync(layout);
            }

            return string.Equals(
                LayoutView.Active?.Layout?.URI,
                arguments.ItemUri,
                StringComparison.Ordinal);
        });

        return new ActivateViewResult(arguments.ItemUri, activated);
    }

    private static Pane? FindExistingMapPane(string itemUri) =>
        FrameworkApplication.Panes
            .OfType<Pane>()
            .FirstOrDefault(pane =>
                pane is IMapPane mapPane
                && string.Equals(
                    mapPane.MapView.Map.URI,
                    itemUri,
                    StringComparison.Ordinal));

    private static Pane? FindExistingLayoutPane(string itemUri) =>
        FrameworkApplication.Panes
            .OfType<Pane>()
            .FirstOrDefault(pane =>
                pane is ILayoutPane layoutPane
                && string.Equals(
                    layoutPane.LayoutView.Layout.URI,
                    itemUri,
                    StringComparison.Ordinal));

    internal static async Task<ZoomResult> ZoomToLayerAsync(
        ZoomToLayerArguments arguments)
    {
        var target = await QueuedTask.Run(() => ResolveZoomLayerTarget(arguments.LayerUri));
        var completed = await target.View.ZoomToAsync(
            target.Layer,
            arguments.SelectedOnly);
        return RequireCompletedNavigation(completed);
    }

    internal static async Task<ZoomResult> ZoomToExtentAsync(
        ZoomToExtentArguments arguments)
    {
        var target = await QueuedTask.Run(() => CreateZoomExtentTarget(arguments.Extent));
        var completed = await target.View.ZoomToAsync(target.Extent);
        return RequireCompletedNavigation(completed);
    }

    internal static void ValidateFlashArguments(FlashFeaturesArguments arguments)
    {
        GisContractGuards.Validate(arguments);
        if (arguments.DurationMilliseconds > MaximumFlashDurationMilliseconds)
        {
            throw new ArgumentOutOfRangeException(
                nameof(arguments.DurationMilliseconds),
                $"Flash duration cannot exceed {MaximumFlashDurationMilliseconds} milliseconds.");
        }
    }

    internal static async Task<FlashFeaturesResult> FlashFeaturesAsync(
        FlashFeaturesArguments arguments)
    {
        var target = await QueuedTask.Run(() => ResolveFlashTarget(arguments));
        await RunOnGuiAsync(() =>
        {
            foreach (var objectId in target.ObjectIds)
            {
                target.View.FlashFeature(target.Layer, objectId);
            }

            return Task.CompletedTask;
        });
        await Task.Delay(arguments.DurationMilliseconds);
        return new FlashFeaturesResult(
            Completed: true,
            FlashedCount: target.ObjectIds.Count);
    }

    private static ActivationTarget ResolveActivationTarget(string itemUri)
    {
        var project = ArcGisObjectResolver.RequireProject();
        var maps = project.GetItems<MapProjectItem>()
            .Select(item => item.GetMap())
            .Where(map => string.Equals(map.URI, itemUri, StringComparison.Ordinal))
            .ToArray();
        var layouts = project.GetItems<LayoutProjectItem>()
            .Select(item => item.GetLayout())
            .Where(layout => string.Equals(layout.URI, itemUri, StringComparison.Ordinal))
            .ToArray();
        if (maps.Length + layouts.Length != 1)
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.ProjectItemNotFound,
                "The requested project item was not found.");
        }

        return maps.Length == 1
            ? new ActivationTarget(maps[0], null)
            : new ActivationTarget(null, layouts[0]);
    }

    private static ZoomLayerTarget ResolveZoomLayerTarget(string layerUri)
    {
        var view = RequireActiveMapView();
        var resolved = ArcGisObjectResolver.ResolveLayer(layerUri);
        if (!string.Equals(resolved.Map.URI, view.Map.URI, StringComparison.Ordinal))
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.LayerNotFound,
                "The requested layer was not found in the active map.");
        }

        return new ZoomLayerTarget(view, resolved.Layer);
    }

    private static ZoomExtentTarget CreateZoomExtentTarget(MapExtent extent)
    {
        var view = RequireActiveMapView();
        try
        {
            SpatialReference spatialReference;
            if (extent.Wkid is not null)
            {
                if (extent.Wkid <= 0)
                {
                    throw new ArgumentException();
                }

                spatialReference = SpatialReferenceBuilder.CreateSpatialReference(extent.Wkid.Value);
            }
            else
            {
                spatialReference = view.Map.SpatialReference;
            }

            if (spatialReference is null || spatialReference.IsUnknown)
            {
                throw new ArgumentException();
            }

            var envelope = EnvelopeBuilderEx.CreateEnvelope(
                extent.XMin,
                extent.YMin,
                extent.XMax,
                extent.YMax,
                spatialReference);
            return new ZoomExtentTarget(view, envelope);
        }
        catch (ArcGisOperationException)
        {
            throw;
        }
        catch
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.InvalidExtent,
                "The extent spatial reference is invalid.");
        }
    }

    private static FlashTarget ResolveFlashTarget(FlashFeaturesArguments arguments)
    {
        var view = RequireActiveMapView();
        var resolved = ArcGisObjectResolver.ResolveLayer(arguments.LayerUri);
        if (!string.Equals(resolved.Map.URI, view.Map.URI, StringComparison.Ordinal))
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.LayerNotFound,
                "The requested layer was not found in the active map.");
        }

        var layer = ArcGisObjectResolver.RequireBasicFeatureLayer(resolved);
        using var table = QueryOperations.OpenTable(layer);
        using var definition = table.GetDefinition();
        var objectIdField = definition.GetObjectIDField();
        if (string.IsNullOrWhiteSpace(objectIdField))
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.UnsupportedLayerType,
                "The layer does not provide stable object IDs.");
        }

        var filter = new QueryFilter
        {
            SubFields = objectIdField,
            ObjectIDs = arguments.ObjectIds,
        };
        var existingObjectIds = QueryOperations.ReadPageObjectIds(
            table,
            filter,
            predicate: null,
            predicateField: null,
            GisContractGuards.MaximumObjectIdCount,
            failWhenScanLimitExceeded: true)
            .ToHashSet();
        var orderedExistingObjectIds = arguments.ObjectIds
            .Where(existingObjectIds.Contains)
            .ToArray();
        return new FlashTarget(view, layer, orderedExistingObjectIds);
    }

    private static MapView RequireActiveMapView() =>
        MapView.Active?.Map is not null
            ? MapView.Active
            : throw new ArcGisOperationException(
                BridgeErrorCodes.NoActiveView,
                "No active map view is available.");

    private static ZoomResult RequireCompletedNavigation(bool completed)
    {
        if (!completed)
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.NavigationInterrupted,
                "ArcGIS Pro navigation was interrupted.");
        }

        return new ZoomResult(Completed: true);
    }

    private static Task<T> RunOnGuiAsync<T>(Func<Task<T>> operation)
    {
        if (QueuedTask.OnGUI)
        {
            return operation();
        }

        return Task.Factory.StartNew(
            operation,
            CancellationToken.None,
            TaskCreationOptions.DenyChildAttach,
            QueuedTask.UIScheduler).Unwrap();
    }

    private static Task RunOnGuiAsync(Func<Task> operation)
    {
        if (QueuedTask.OnGUI)
        {
            return operation();
        }

        return Task.Factory.StartNew(
            operation,
            CancellationToken.None,
            TaskCreationOptions.DenyChildAttach,
            QueuedTask.UIScheduler).Unwrap();
    }

    private sealed record ActivationTarget(Map? Map, Layout? Layout);

    private sealed record ZoomLayerTarget(MapView View, Layer Layer);

    private sealed record ZoomExtentTarget(MapView View, Envelope Extent);

    private sealed record FlashTarget(
        MapView View,
        BasicFeatureLayer Layer,
        IReadOnlyList<long> ObjectIds);
}
