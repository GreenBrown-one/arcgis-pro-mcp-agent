using ArcGIS.Core.Data;
using ArcGIS.Desktop.Mapping;
using ArcGISProAgent.Contracts;

namespace ArcGISProAgent.AddIn.Operations;

internal static class SelectionOperations
{
    private const int MaximumScannedRows = 10_000;

    internal static SelectionResult SelectByAttribute(
        SelectByAttributeArguments arguments)
    {
        var layer = ArcGisObjectResolver.RequireBasicFeatureLayer(
            ArcGisObjectResolver.ResolveLayer(arguments.LayerUri));
        using var table = QueryOperations.OpenTable(layer);
        using var definition = table.GetDefinition();
        var predicateField = QueryOperations.ResolvePredicateField(
            definition,
            arguments.Predicate);
        var objectIdField = definition.GetObjectIDField();
        if (string.IsNullOrWhiteSpace(objectIdField))
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.UnsupportedLayerType,
                "The layer does not provide stable object IDs.");
        }

        var scanFilter = new QueryFilter
        {
            SubFields = string.Join(
                ",",
                new[] { objectIdField, predicateField.Name }
                    .Distinct(StringComparer.OrdinalIgnoreCase)),
        };
        var matchedObjectIds = QueryOperations.ReadPageObjectIds(
            table,
            scanFilter,
            arguments.Predicate,
            predicateField,
            MaximumScannedRows,
            failWhenScanLimitExceeded: true);
        if (matchedObjectIds.Count == 0)
        {
            return HandleEmptySelection(arguments.LayerUri, layer, arguments.Mode);
        }

        var selectionFilter = new QueryFilter { ObjectIDs = matchedObjectIds };
        using (layer.Select(selectionFilter, ToSdkMode(arguments.Mode)))
        {
        }

        return ReadFinalSelection(arguments.LayerUri, layer);
    }

    internal static SelectionResult SelectByLocation(
        SelectByLocationArguments arguments)
    {
        var resolved = ArcGisObjectResolver.ResolveLayer(arguments.LayerUri);
        var layer = ArcGisObjectResolver.RequireBasicFeatureLayer(resolved);
        using var table = QueryOperations.OpenTable(layer);
        using var definition = table.GetDefinition();
        if (definition is not FeatureClassDefinition featureDefinition)
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.UnsupportedLayerType,
                "This operation requires a spatial feature class.");
        }

        var spatialReference = featureDefinition.GetSpatialReference();
        if (spatialReference is null || spatialReference.IsUnknown)
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.InvalidSpatialSource,
                "The target layer does not have a known spatial reference.");
        }

        var geometry = QueryOperations.CreateSpatialSourceGeometry(
            arguments.Source,
            resolved.Map,
            spatialReference);
        if (geometry is null || geometry.IsEmpty)
        {
            return HandleEmptySelection(arguments.LayerUri, layer, arguments.Mode);
        }

        var filter = QueryOperations.CreateSpatialFilter(geometry, arguments.Relation);
        using (layer.Select(filter, ToSdkMode(arguments.Mode)))
        {
        }

        return ReadFinalSelection(arguments.LayerUri, layer);
    }

    internal static ClearSelectionResult Clear(ClearSelectionArguments arguments)
    {
        IReadOnlyList<FeatureLayer> layers = arguments.LayerUri is not null
            ?
            [
                ArcGisObjectResolver.RequireFeatureLayer(
                    ArcGisObjectResolver.ResolveLayer(arguments.LayerUri))
            ]
            : ArcGisObjectResolver.RequireActiveMap()
                .GetLayersAsFlattenedList()
                .OfType<FeatureLayer>()
                .ToArray();

        long layersCleared = 0;
        long featuresCleared = 0;
        foreach (var layer in layers)
        {
            using var selection = layer.GetSelection();
            var count = selection.GetCount();
            if (count == 0)
            {
                continue;
            }

            layer.ClearSelection();
            layersCleared++;
            featuresCleared += count;
        }

        return new ClearSelectionResult(layersCleared, featuresCleared);
    }

    private static SelectionResult HandleEmptySelection(
        string layerUri,
        BasicFeatureLayer layer,
        SelectionCombinationMode mode)
    {
        if (mode is SelectionCombinationMode.Replace)
        {
            layer.ClearSelection();
        }

        return ReadFinalSelection(layerUri, layer);
    }

    private static SelectionResult ReadFinalSelection(
        string layerUri,
        BasicFeatureLayer layer)
    {
        using var selection = layer.GetSelection();
        return new SelectionResult(layerUri, selection.GetCount());
    }

    private static SelectionCombinationMethod ToSdkMode(
        SelectionCombinationMode mode) => mode switch
        {
            SelectionCombinationMode.Replace => SelectionCombinationMethod.New,
            SelectionCombinationMode.Add => SelectionCombinationMethod.Add,
            SelectionCombinationMode.Remove => SelectionCombinationMethod.Subtract,
            SelectionCombinationMode.Toggle => SelectionCombinationMethod.XOR,
            _ => throw new ArcGisOperationException(
                BridgeErrorCodes.InvalidArguments,
                "The selection mode is invalid."),
        };
}
