using System.ComponentModel;
using ArcGISProAgent.Bridge;
using ArcGISProAgent.Contracts;
using ModelContextProtocol.Server;

namespace ArcGISProAgent.Mcp.Tools;

[McpServerToolType]
public sealed class LayerTools(IBridgeClient bridge)
{
    [McpServerTool(Name = "arcgis_list_layers", ReadOnly = true, Destructive = false, Idempotent = true)]
    [Description("List layers in the active view with stable layer URIs for later calls.")]
    public Task<LayerListResult> ListLayersAsync(
        bool includeNested = true,
        CancellationToken cancellationToken = default)
    {
        var arguments = new ListLayersArguments(includeNested);
        GisContractGuards.Validate(arguments);
        return bridge.InvokeAsync<LayerListResult>(
            "layers.list",
            arguments,
            cancellationToken);
    }

    [McpServerTool(Name = "arcgis_describe_layer", ReadOnly = true, Destructive = false, Idempotent = true)]
    [Description("Describe one layer addressed by its stable layer URI.")]
    public Task<LayerDescription> DescribeLayerAsync(
        string layerUri,
        CancellationToken cancellationToken)
    {
        var arguments = new DescribeLayerArguments(layerUri);
        GisContractGuards.Validate(arguments);
        return bridge.InvokeAsync<LayerDescription>(
            "layers.describe",
            arguments,
            cancellationToken);
    }

    [McpServerTool(Name = "arcgis_list_fields", ReadOnly = true, Destructive = false, Idempotent = true)]
    [Description("List fields for a layer addressed by its stable layer URI.")]
    public Task<LayerFieldsResult> ListFieldsAsync(
        string layerUri,
        CancellationToken cancellationToken)
    {
        var arguments = new ListFieldsArguments(layerUri);
        GisContractGuards.Validate(arguments);
        return bridge.InvokeAsync<LayerFieldsResult>(
            "layers.fields",
            arguments,
            cancellationToken);
    }
}
