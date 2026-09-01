using System.ComponentModel;
using ArcGISProAgent.Bridge;
using ArcGISProAgent.Contracts;
using ModelContextProtocol.Server;

namespace ArcGISProAgent.Mcp.Tools;

[McpServerToolType]
public sealed class MapViewTools(IBridgeClient bridge)
{
    [McpServerTool(
        Name = "arcgis_activate_view",
        ReadOnly = false,
        Destructive = false,
        Idempotent = true)]
    [Description("Activate an existing project item addressed by its stable URI.")]
    public Task<ActivateViewResult> ActivateViewAsync(
        string itemUri,
        CancellationToken cancellationToken = default)
    {
        var arguments = new ActivateViewArguments(itemUri);
        GisContractGuards.Validate(arguments);
        return bridge.InvokeAsync<ActivateViewResult>(
            "map_view.activate",
            arguments,
            cancellationToken);
    }

    [McpServerTool(
        Name = "arcgis_zoom_to_layer",
        ReadOnly = false,
        Destructive = false,
        Idempotent = true)]
    [Description("Zoom the active map view to a layer addressed by its stable layer URI.")]
    public Task<ZoomResult> ZoomToLayerAsync(
        string layerUri,
        bool selectedOnly = false,
        CancellationToken cancellationToken = default)
    {
        var arguments = new ZoomToLayerArguments(layerUri, selectedOnly);
        GisContractGuards.Validate(arguments);
        return bridge.InvokeAsync<ZoomResult>(
            "map_view.zoom_to_layer",
            arguments,
            cancellationToken);
    }

    [McpServerTool(
        Name = "arcgis_zoom_to_extent",
        ReadOnly = false,
        Destructive = false,
        Idempotent = true)]
    [Description("Zoom the active map view to a validated extent.")]
    public Task<ZoomResult> ZoomToExtentAsync(
        MapExtent extent,
        CancellationToken cancellationToken = default)
    {
        var arguments = new ZoomToExtentArguments(extent);
        GisContractGuards.Validate(arguments);
        return bridge.InvokeAsync<ZoomResult>(
            "map_view.zoom_to_extent",
            arguments,
            cancellationToken);
    }

    [McpServerTool(
        Name = "arcgis_flash_features",
        ReadOnly = false,
        Destructive = false,
        Idempotent = false)]
    [Description("Flash existing features in a layer addressed by its stable layer URI.")]
    public Task<FlashFeaturesResult> FlashFeaturesAsync(
        string layerUri,
        IReadOnlyList<long> objectIds,
        int durationMilliseconds = 1000,
        CancellationToken cancellationToken = default)
    {
        var arguments = new FlashFeaturesArguments(
            layerUri,
            objectIds,
            durationMilliseconds);
        GisContractGuards.Validate(arguments);
        return bridge.InvokeAsync<FlashFeaturesResult>(
            "map_view.flash_features",
            arguments,
            cancellationToken);
    }
}
