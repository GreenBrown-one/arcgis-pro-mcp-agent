using ArcGIS.Desktop.Core;
using ArcGIS.Desktop.Mapping;
using ArcGISProAgent.Contracts;

namespace ArcGISProAgent.AddIn.Operations;

internal sealed class ArcGisOperationException : Exception
{
    internal ArcGisOperationException(string code, string publicMessage)
        : base(publicMessage)
    {
        Code = code;
        PublicMessage = publicMessage;
    }

    internal string Code { get; }

    internal string PublicMessage { get; }
}

internal sealed record ResolvedLayer(Map Map, Layer Layer);

internal static class ArcGisObjectResolver
{
    internal static Project RequireProject() =>
        Project.Current ?? throw new ArcGisOperationException(
            BridgeErrorCodes.NoActiveProject,
            "No ArcGIS Pro project is open.");

    internal static Map RequireActiveMap()
    {
        RequireProject();
        return MapView.Active?.Map ?? throw new ArcGisOperationException(
            BridgeErrorCodes.NoActiveMap,
            "No active map or scene is available.");
    }

    internal static ResolvedLayer ResolveLayer(string layerUri)
    {
        var project = RequireProject();
        ResolvedLayer? match = null;

        foreach (var item in project.GetItems<MapProjectItem>())
        {
            var map = item.GetMap();
            foreach (var layer in map.GetLayersAsFlattenedList())
            {
                if (!string.Equals(layer.URI, layerUri, StringComparison.Ordinal))
                {
                    continue;
                }

                if (match is not null)
                {
                    throw new ArcGisOperationException(
                        BridgeErrorCodes.AmbiguousLayer,
                        "The layer URI is not unique in the current project.");
                }

                match = new ResolvedLayer(map, layer);
            }
        }

        return match ?? throw new ArcGisOperationException(
            BridgeErrorCodes.LayerNotFound,
            "The requested layer was not found.");
    }

    internal static BasicFeatureLayer RequireBasicFeatureLayer(ResolvedLayer resolved) =>
        resolved.Layer as BasicFeatureLayer
        ?? throw new ArcGisOperationException(
            BridgeErrorCodes.UnsupportedLayerType,
            "This operation requires a feature-backed layer.");

    internal static FeatureLayer RequireFeatureLayer(ResolvedLayer resolved) =>
        resolved.Layer as FeatureLayer
        ?? throw new ArcGisOperationException(
            BridgeErrorCodes.UnsupportedLayerType,
            "This operation requires a feature layer.");
}
