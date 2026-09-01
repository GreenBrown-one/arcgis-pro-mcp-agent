using System.ComponentModel;
using ArcGISProAgent.Bridge;
using ArcGISProAgent.Contracts;
using ModelContextProtocol.Server;

namespace ArcGISProAgent.Mcp.Tools;

[McpServerToolType]
public sealed class QuerySelectionTools(IBridgeClient bridge)
{
    [McpServerTool(Name = "arcgis_count_features", ReadOnly = true, Destructive = false, Idempotent = true)]
    [Description("Count features in a layer addressed by its stable layer URI.")]
    public Task<FeatureCountResult> CountFeaturesAsync(
        string layerUri,
        AttributePredicate? predicate = null,
        CancellationToken cancellationToken = default)
    {
        var arguments = new FeatureCountArguments(layerUri, predicate);
        GisContractGuards.Validate(arguments);
        return bridge.InvokeAsync<FeatureCountResult>(
            "query.feature_count",
            arguments,
            cancellationToken);
    }

    [McpServerTool(Name = "arcgis_query_features", ReadOnly = true, Destructive = false, Idempotent = true)]
    [Description("Query fields from a layer addressed by its stable layer URI.")]
    public Task<FeatureQueryResult> QueryFeaturesAsync(
        string layerUri,
        IReadOnlyList<string> fields,
        AttributePredicate? predicate = null,
        int offset = 0,
        int limit = 20,
        CancellationToken cancellationToken = default)
    {
        var arguments = new FeatureQueryArguments(
            layerUri,
            fields,
            predicate,
            offset,
            limit);
        GisContractGuards.Validate(arguments);
        return bridge.InvokeAsync<FeatureQueryResult>(
            "query.features",
            arguments,
            cancellationToken);
    }

    [McpServerTool(Name = "arcgis_query_spatial", ReadOnly = true, Destructive = false, Idempotent = true)]
    [Description("Run a scoped spatial query against a stable target layer URI.")]
    public Task<FeatureQueryResult> QuerySpatialAsync(
        string layerUri,
        SpatialQuerySource source,
        SpatialRelation relation,
        IReadOnlyList<string> fields,
        int offset = 0,
        int limit = 20,
        CancellationToken cancellationToken = default)
    {
        var arguments = new SpatialQueryArguments(
            layerUri,
            source,
            relation,
            fields,
            offset,
            limit);
        GisContractGuards.Validate(arguments);
        return bridge.InvokeAsync<FeatureQueryResult>(
            "query.spatial",
            arguments,
            cancellationToken);
    }

    [McpServerTool(Name = "arcgis_get_selection", ReadOnly = true, Destructive = false, Idempotent = true)]
    [Description("Describe selection state, optionally scoped by a stable layer URI.")]
    public Task<SelectionDescription> GetSelectionAsync(
        string? layerUri = null,
        int objectIdLimit = 20,
        CancellationToken cancellationToken = default)
    {
        var arguments = new SelectionDescribeArguments(layerUri, objectIdLimit);
        GisContractGuards.Validate(arguments);
        return bridge.InvokeAsync<SelectionDescription>(
            "selection.describe",
            arguments,
            cancellationToken);
    }

    [McpServerTool(
        Name = "arcgis_select_by_attribute",
        ReadOnly = false,
        Destructive = false,
        Idempotent = false)]
    [Description("Select safely matched features in a layer addressed by its stable layer URI.")]
    public Task<SelectionResult> SelectByAttributeAsync(
        string layerUri,
        AttributePredicate predicate,
        SelectionCombinationMode mode = SelectionCombinationMode.Replace,
        CancellationToken cancellationToken = default)
    {
        var arguments = new SelectByAttributeArguments(layerUri, predicate, mode);
        GisContractGuards.Validate(arguments);
        return bridge.InvokeAsync<SelectionResult>(
            "selection.by_attribute",
            arguments,
            cancellationToken);
    }

    [McpServerTool(
        Name = "arcgis_select_by_location",
        ReadOnly = false,
        Destructive = false,
        Idempotent = false)]
    [Description("Select spatially matched features in a layer addressed by its stable layer URI.")]
    public Task<SelectionResult> SelectByLocationAsync(
        string layerUri,
        SpatialQuerySource source,
        SpatialRelation relation,
        SelectionCombinationMode mode = SelectionCombinationMode.Replace,
        CancellationToken cancellationToken = default)
    {
        var arguments = new SelectByLocationArguments(layerUri, source, relation, mode);
        GisContractGuards.Validate(arguments);
        return bridge.InvokeAsync<SelectionResult>(
            "selection.by_location",
            arguments,
            cancellationToken);
    }

    [McpServerTool(
        Name = "arcgis_clear_selection",
        ReadOnly = false,
        Destructive = false,
        Idempotent = true)]
    [Description("Clear feature selection, optionally scoped by a stable layer URI.")]
    public Task<ClearSelectionResult> ClearSelectionAsync(
        string? layerUri = null,
        CancellationToken cancellationToken = default)
    {
        var arguments = new ClearSelectionArguments(layerUri);
        GisContractGuards.Validate(arguments);
        return bridge.InvokeAsync<ClearSelectionResult>(
            "selection.clear",
            arguments,
            cancellationToken);
    }
}
