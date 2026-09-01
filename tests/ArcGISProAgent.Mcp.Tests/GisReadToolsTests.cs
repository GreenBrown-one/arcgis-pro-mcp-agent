using System.Reflection;
using System.Text.Json;
using ArcGISProAgent.Bridge;
using ArcGISProAgent.Contracts;
using ArcGISProAgent.Mcp.Tools;
using ModelContextProtocol.Server;

namespace ArcGISProAgent.Mcp.Tests;

public sealed class GisReadToolsTests
{
    [Fact]
    public async Task Describe_context_forwards_the_typed_request()
    {
        var expected = new ContextDescription(null, null);
        var bridge = new RecordingBridgeClient(expected);
        var tools = new ContextTools(bridge);
        using var cancellation = new CancellationTokenSource();

        var actual = await tools.DescribeContextAsync(cancellation.Token);

        AssertForwarded<ContextDescribeArguments, ContextDescription>(
            bridge,
            "context.describe",
            new ContextDescribeArguments(),
            cancellation.Token);
        Assert.Same(expected, actual);
    }

    [Fact]
    public async Task List_layers_forwards_the_typed_request()
    {
        var expected = new LayerListResult([]);
        var bridge = new RecordingBridgeClient(expected);
        var tools = new LayerTools(bridge);
        using var cancellation = new CancellationTokenSource();

        var actual = await tools.ListLayersAsync(false, cancellation.Token);

        AssertForwarded<ListLayersArguments, LayerListResult>(
            bridge,
            "layers.list",
            new ListLayersArguments(false),
            cancellation.Token);
        Assert.Same(expected, actual);
    }

    [Fact]
    public async Task Describe_layer_forwards_the_typed_request()
    {
        const string layerUri = "map://project/layers/parcels";
        var expected = new LayerDescription(
            layerUri,
            "Parcels",
            "FeatureLayer",
            "FileGeodatabase",
            "C:\\Data\\Land.gdb\\Parcels",
            "Polygon",
            new SpatialReferenceSummary(3857, "WGS 1984 Web Mercator Auxiliary Sphere"),
            "Connected",
            12);
        var bridge = new RecordingBridgeClient(expected);
        var tools = new LayerTools(bridge);
        using var cancellation = new CancellationTokenSource();

        var actual = await tools.DescribeLayerAsync(layerUri, cancellation.Token);

        AssertForwarded<DescribeLayerArguments, LayerDescription>(
            bridge,
            "layers.describe",
            new DescribeLayerArguments(layerUri),
            cancellation.Token);
        Assert.Same(expected, actual);
    }

    [Fact]
    public async Task List_fields_forwards_the_typed_request()
    {
        const string layerUri = "map://project/layers/parcels";
        var expected = new LayerFieldsResult(layerUri, []);
        var bridge = new RecordingBridgeClient(expected);
        var tools = new LayerTools(bridge);
        using var cancellation = new CancellationTokenSource();

        var actual = await tools.ListFieldsAsync(layerUri, cancellation.Token);

        AssertForwarded<ListFieldsArguments, LayerFieldsResult>(
            bridge,
            "layers.fields",
            new ListFieldsArguments(layerUri),
            cancellation.Token);
        Assert.Same(expected, actual);
    }

    [Fact]
    public async Task Count_features_forwards_the_typed_request()
    {
        const string layerUri = "map://project/layers/parcels";
        var predicate = new AttributePredicate(
            "STATUS",
            AttributeComparisonOperator.Equal,
            JsonSerializer.SerializeToElement("Active"));
        var expected = new FeatureCountResult(layerUri, 12);
        var bridge = new RecordingBridgeClient(expected);
        var tools = new QuerySelectionTools(bridge);
        using var cancellation = new CancellationTokenSource();

        var actual = await tools.CountFeaturesAsync(
            layerUri,
            predicate,
            cancellation.Token);

        AssertForwarded<FeatureCountArguments, FeatureCountResult>(
            bridge,
            "query.feature_count",
            new FeatureCountArguments(layerUri, predicate),
            cancellation.Token);
        Assert.Same(expected, actual);
    }

    [Fact]
    public async Task Query_features_forwards_the_typed_request()
    {
        const string layerUri = "map://project/layers/parcels";
        string[] fields = ["OBJECTID", "STATUS"];
        var predicate = new AttributePredicate(
            "STATUS",
            AttributeComparisonOperator.Equal,
            JsonSerializer.SerializeToElement("Active"));
        var expected = new FeatureQueryResult(layerUri, 10, 5, 12, [], true);
        var bridge = new RecordingBridgeClient(expected);
        var tools = new QuerySelectionTools(bridge);
        using var cancellation = new CancellationTokenSource();

        var actual = await tools.QueryFeaturesAsync(
            layerUri,
            fields,
            predicate,
            10,
            5,
            cancellation.Token);

        AssertForwarded<FeatureQueryArguments, FeatureQueryResult>(
            bridge,
            "query.features",
            new FeatureQueryArguments(layerUri, fields, predicate, 10, 5),
            cancellation.Token);
        Assert.Same(expected, actual);
    }

    [Fact]
    public async Task Spatial_query_forwards_only_the_scoped_source_and_relation()
    {
        const string targetLayerUri = "map://project/layers/parcels";
        var source = new SpatialQuerySource(
            SpatialQuerySourceKind.Layer,
            "map://project/layers/flood-zones",
            null);
        string[] fields = ["OBJECTID", "STATUS"];
        var expected = new FeatureQueryResult(targetLayerUri, 3, 7, 15, [], true);
        var bridge = new RecordingBridgeClient(expected);
        var tools = new QuerySelectionTools(bridge);
        using var cancellation = new CancellationTokenSource();

        var actual = await tools.QuerySpatialAsync(
            targetLayerUri,
            source,
            SpatialRelation.Intersects,
            fields,
            3,
            7,
            cancellation.Token);

        AssertForwarded<SpatialQueryArguments, FeatureQueryResult>(
            bridge,
            "query.spatial",
            new SpatialQueryArguments(
                targetLayerUri,
                source,
                SpatialRelation.Intersects,
                fields,
                3,
                7),
            cancellation.Token);
        Assert.Same(expected, actual);
    }

    [Fact]
    public async Task Get_selection_forwards_the_typed_request()
    {
        const string layerUri = "map://project/layers/parcels";
        var expected = new SelectionDescription([]);
        var bridge = new RecordingBridgeClient(expected);
        var tools = new QuerySelectionTools(bridge);
        using var cancellation = new CancellationTokenSource();

        var actual = await tools.GetSelectionAsync(layerUri, 9, cancellation.Token);

        AssertForwarded<SelectionDescribeArguments, SelectionDescription>(
            bridge,
            "selection.describe",
            new SelectionDescribeArguments(layerUri, 9),
            cancellation.Token);
        Assert.Same(expected, actual);
    }

    [Fact]
    public async Task Select_by_attribute_forwards_the_typed_request_and_defaults()
    {
        const string layerUri = "map://project/layers/parcels";
        var predicate = new AttributePredicate(
            "STATUS",
            AttributeComparisonOperator.Equal,
            JsonSerializer.SerializeToElement("Active"));
        var expected = new SelectionResult(layerUri, 7);
        var bridge = new RecordingBridgeClient(expected);
        using var cancellation = new CancellationTokenSource();

        var actual = await InvokeToolAsync(
            typeof(QuerySelectionTools).FullName!,
            "SelectByAttributeAsync",
            bridge,
            layerUri,
            predicate,
            Type.Missing,
            cancellation.Token);

        AssertForwarded<SelectByAttributeArguments, SelectionResult>(
            bridge,
            "selection.by_attribute",
            new SelectByAttributeArguments(layerUri, predicate),
            cancellation.Token);
        Assert.Same(expected, actual);
    }

    [Fact]
    public async Task Select_by_location_forwards_the_typed_request_and_defaults()
    {
        const string layerUri = "map://project/layers/parcels";
        var source = new SpatialQuerySource(
            SpatialQuerySourceKind.CurrentView,
            null,
            null);
        var expected = new SelectionResult(layerUri, 4);
        var bridge = new RecordingBridgeClient(expected);
        using var cancellation = new CancellationTokenSource();

        var actual = await InvokeToolAsync(
            typeof(QuerySelectionTools).FullName!,
            "SelectByLocationAsync",
            bridge,
            layerUri,
            source,
            SpatialRelation.Intersects,
            Type.Missing,
            cancellation.Token);

        AssertForwarded<SelectByLocationArguments, SelectionResult>(
            bridge,
            "selection.by_location",
            new SelectByLocationArguments(layerUri, source, SpatialRelation.Intersects),
            cancellation.Token);
        Assert.Same(expected, actual);
    }

    [Fact]
    public async Task Clear_selection_forwards_the_typed_request_and_defaults()
    {
        var expected = new ClearSelectionResult(2, 9);
        var bridge = new RecordingBridgeClient(expected);
        using var cancellation = new CancellationTokenSource();

        var actual = await InvokeToolAsync(
            typeof(QuerySelectionTools).FullName!,
            "ClearSelectionAsync",
            bridge,
            Type.Missing,
            cancellation.Token);

        AssertForwarded<ClearSelectionArguments, ClearSelectionResult>(
            bridge,
            "selection.clear",
            new ClearSelectionArguments(),
            cancellation.Token);
        Assert.Same(expected, actual);
    }

    [Fact]
    public async Task Activate_view_forwards_the_typed_request()
    {
        const string itemUri = "map://project/maps/operations";
        var expected = new ActivateViewResult(itemUri, true);
        var bridge = new RecordingBridgeClient(expected);
        using var cancellation = new CancellationTokenSource();

        var actual = await InvokeToolAsync(
            "ArcGISProAgent.Mcp.Tools.MapViewTools",
            "ActivateViewAsync",
            bridge,
            itemUri,
            cancellation.Token);

        AssertForwarded<ActivateViewArguments, ActivateViewResult>(
            bridge,
            "map_view.activate",
            new ActivateViewArguments(itemUri),
            cancellation.Token);
        Assert.Same(expected, actual);
    }

    [Fact]
    public async Task Zoom_to_layer_forwards_the_typed_request_and_defaults()
    {
        const string layerUri = "map://project/layers/parcels";
        var expected = new ZoomResult(true);
        var bridge = new RecordingBridgeClient(expected);
        using var cancellation = new CancellationTokenSource();

        var actual = await InvokeToolAsync(
            "ArcGISProAgent.Mcp.Tools.MapViewTools",
            "ZoomToLayerAsync",
            bridge,
            layerUri,
            Type.Missing,
            cancellation.Token);

        AssertForwarded<ZoomToLayerArguments, ZoomResult>(
            bridge,
            "map_view.zoom_to_layer",
            new ZoomToLayerArguments(layerUri),
            cancellation.Token);
        Assert.Same(expected, actual);
    }

    [Fact]
    public async Task Zoom_to_extent_forwards_the_typed_request()
    {
        var extent = new MapExtent(-123, 37, -121, 39, 4326);
        var expected = new ZoomResult(true);
        var bridge = new RecordingBridgeClient(expected);
        using var cancellation = new CancellationTokenSource();

        var actual = await InvokeToolAsync(
            "ArcGISProAgent.Mcp.Tools.MapViewTools",
            "ZoomToExtentAsync",
            bridge,
            extent,
            cancellation.Token);

        AssertForwarded<ZoomToExtentArguments, ZoomResult>(
            bridge,
            "map_view.zoom_to_extent",
            new ZoomToExtentArguments(extent),
            cancellation.Token);
        Assert.Same(expected, actual);
    }

    [Fact]
    public async Task Flash_features_forwards_the_typed_request_and_defaults()
    {
        const string layerUri = "map://project/layers/parcels";
        long[] objectIds = [3, 8];
        var expected = new FlashFeaturesResult(true, 2);
        var bridge = new RecordingBridgeClient(expected);
        using var cancellation = new CancellationTokenSource();

        var actual = await InvokeToolAsync(
            "ArcGISProAgent.Mcp.Tools.MapViewTools",
            "FlashFeaturesAsync",
            bridge,
            layerUri,
            objectIds,
            Type.Missing,
            cancellation.Token);

        AssertForwarded<FlashFeaturesArguments, FlashFeaturesResult>(
            bridge,
            "map_view.flash_features",
            new FlashFeaturesArguments(layerUri, objectIds),
            cancellation.Token);
        Assert.Same(expected, actual);
    }

    [Fact]
    public async Task R1_tools_propagate_every_bridge_failure_unchanged()
    {
        const string layerUri = "map://project/layers/parcels";
        var predicate = new AttributePredicate(
            "STATUS",
            AttributeComparisonOperator.Equal,
            JsonSerializer.SerializeToElement("Active"));
        var source = new SpatialQuerySource(SpatialQuerySourceKind.CurrentView, null, null);
        var extent = new MapExtent(-123, 37, -121, 39, 4326);
        var calls = new (string TypeName, string MethodName, object?[] Arguments)[]
        {
            (typeof(QuerySelectionTools).FullName!, "SelectByAttributeAsync", [layerUri, predicate, SelectionCombinationMode.Replace, CancellationToken.None]),
            (typeof(QuerySelectionTools).FullName!, "SelectByLocationAsync", [layerUri, source, SpatialRelation.Intersects, SelectionCombinationMode.Replace, CancellationToken.None]),
            (typeof(QuerySelectionTools).FullName!, "ClearSelectionAsync", [layerUri, CancellationToken.None]),
            ("ArcGISProAgent.Mcp.Tools.MapViewTools", "ActivateViewAsync", ["map://project/maps/operations", CancellationToken.None]),
            ("ArcGISProAgent.Mcp.Tools.MapViewTools", "ZoomToLayerAsync", [layerUri, false, CancellationToken.None]),
            ("ArcGISProAgent.Mcp.Tools.MapViewTools", "ZoomToExtentAsync", [extent, CancellationToken.None]),
            ("ArcGISProAgent.Mcp.Tools.MapViewTools", "FlashFeaturesAsync", [layerUri, new long[] { 3 }, 1000, CancellationToken.None]),
        };

        foreach (var call in calls)
        {
            var expected = new BridgeSentinelException();
            var bridge = new RecordingBridgeClient(expected);

            var actual = await Assert.ThrowsAsync<BridgeSentinelException>(() =>
                InvokeToolAsync(call.TypeName, call.MethodName, bridge, call.Arguments));

            Assert.Same(expected, actual);
            Assert.Single(bridge.Calls);
        }
    }

    [Fact]
    public async Task Read_tools_validate_task_one_dtos_before_invoking_the_bridge()
    {
        var bridge = new RecordingBridgeClient(new object());
        var layerTools = new LayerTools(bridge);
        var queryTools = new QuerySelectionTools(bridge);
        string[] validFields = ["OBJECTID"];

        Func<Task>[] invalidCalls =
        [
            () => layerTools.DescribeLayerAsync(" ", CancellationToken.None),
            () => layerTools.ListFieldsAsync(" ", CancellationToken.None),
            () => queryTools.CountFeaturesAsync(" ", null, CancellationToken.None),
            () => queryTools.QueryFeaturesAsync(
                "map://project/layers/parcels",
                [],
                null,
                0,
                20,
                CancellationToken.None),
            () => queryTools.QuerySpatialAsync(
                "map://project/layers/parcels",
                new SpatialQuerySource(SpatialQuerySourceKind.Layer, null, null),
                SpatialRelation.Intersects,
                validFields,
                0,
                20,
                CancellationToken.None),
            () => queryTools.GetSelectionAsync(null, 0, CancellationToken.None),
        ];

        foreach (var invalidCall in invalidCalls)
        {
            await Assert.ThrowsAnyAsync<ArgumentException>(invalidCall);
        }

        Assert.Empty(bridge.Calls);
    }

    [Fact]
    public void Public_read_tool_parameters_are_explicitly_scoped_contract_values()
    {
        var actual = typeof(ContextTools).Assembly
            .GetTypes()
            .SelectMany(type => type.GetMethods(BindingFlags.Instance | BindingFlags.Public))
            .Where(method => method.GetCustomAttribute<McpServerToolAttribute>() is not null)
            .ToDictionary(
                method => method.GetCustomAttribute<McpServerToolAttribute>()!.Name!,
                method => method.GetParameters()
                    .Where(parameter => parameter.ParameterType != typeof(CancellationToken))
                    .Select(parameter => parameter.Name!)
                    .ToArray());

        Assert.Empty(actual["arcgis_describe_context"]);
        Assert.Equal(["includeNested"], actual["arcgis_list_layers"]);
        Assert.Equal(["layerUri"], actual["arcgis_describe_layer"]);
        Assert.Equal(["layerUri"], actual["arcgis_list_fields"]);
        Assert.Equal(["layerUri", "predicate"], actual["arcgis_count_features"]);
        Assert.Equal(
            ["layerUri", "fields", "predicate", "offset", "limit"],
            actual["arcgis_query_features"]);
        Assert.Equal(
            ["layerUri", "source", "relation", "fields", "offset", "limit"],
            actual["arcgis_query_spatial"]);
        Assert.Equal(["layerUri", "objectIdLimit"], actual["arcgis_get_selection"]);
        Assert.Equal(
            ["layerUri", "predicate", "mode"],
            actual["arcgis_select_by_attribute"]);
        Assert.Equal(
            ["layerUri", "source", "relation", "mode"],
            actual["arcgis_select_by_location"]);
        Assert.Equal(["layerUri"], actual["arcgis_clear_selection"]);
        Assert.Equal(["itemUri"], actual["arcgis_activate_view"]);
        Assert.Equal(["layerUri", "selectedOnly"], actual["arcgis_zoom_to_layer"]);
        Assert.Equal(["extent"], actual["arcgis_zoom_to_extent"]);
        Assert.Equal(
            ["layerUri", "objectIds", "durationMilliseconds"],
            actual["arcgis_flash_features"]);
    }

    [Fact]
    public void R1_public_signatures_have_exact_types_nullability_and_defaults()
    {
        var nullability = new NullabilityInfoContext();
        var methods = typeof(QuerySelectionTools).Assembly
            .GetTypes()
            .SelectMany(type => type.GetMethods(BindingFlags.Instance | BindingFlags.Public))
            .Where(method => method.GetCustomAttribute<McpServerToolAttribute>() is not null)
            .ToDictionary(
                method => method.GetCustomAttribute<McpServerToolAttribute>()!.Name!,
                StringComparer.Ordinal);

        AssertSignature(methods, nullability, "arcgis_select_by_attribute", typeof(Task<SelectionResult>),
            new ParameterSpec("layerUri", typeof(string), NullabilityState.NotNull),
            new ParameterSpec("predicate", typeof(AttributePredicate), NullabilityState.NotNull),
            new ParameterSpec("mode", typeof(SelectionCombinationMode), NullabilityState.NotNull, SelectionCombinationMode.Replace),
            new ParameterSpec("cancellationToken", typeof(CancellationToken), NullabilityState.NotNull, null));
        AssertSignature(methods, nullability, "arcgis_select_by_location", typeof(Task<SelectionResult>),
            new ParameterSpec("layerUri", typeof(string), NullabilityState.NotNull),
            new ParameterSpec("source", typeof(SpatialQuerySource), NullabilityState.NotNull),
            new ParameterSpec("relation", typeof(SpatialRelation), NullabilityState.NotNull),
            new ParameterSpec("mode", typeof(SelectionCombinationMode), NullabilityState.NotNull, SelectionCombinationMode.Replace),
            new ParameterSpec("cancellationToken", typeof(CancellationToken), NullabilityState.NotNull, null));
        AssertSignature(methods, nullability, "arcgis_clear_selection", typeof(Task<ClearSelectionResult>),
            new ParameterSpec("layerUri", typeof(string), NullabilityState.Nullable, null),
            new ParameterSpec("cancellationToken", typeof(CancellationToken), NullabilityState.NotNull, null));
        AssertSignature(methods, nullability, "arcgis_activate_view", typeof(Task<ActivateViewResult>),
            new ParameterSpec("itemUri", typeof(string), NullabilityState.NotNull),
            new ParameterSpec("cancellationToken", typeof(CancellationToken), NullabilityState.NotNull, null));
        AssertSignature(methods, nullability, "arcgis_zoom_to_layer", typeof(Task<ZoomResult>),
            new ParameterSpec("layerUri", typeof(string), NullabilityState.NotNull),
            new ParameterSpec("selectedOnly", typeof(bool), NullabilityState.NotNull, false),
            new ParameterSpec("cancellationToken", typeof(CancellationToken), NullabilityState.NotNull, null));
        AssertSignature(methods, nullability, "arcgis_zoom_to_extent", typeof(Task<ZoomResult>),
            new ParameterSpec("extent", typeof(MapExtent), NullabilityState.NotNull),
            new ParameterSpec("cancellationToken", typeof(CancellationToken), NullabilityState.NotNull, null));
        AssertSignature(methods, nullability, "arcgis_flash_features", typeof(Task<FlashFeaturesResult>),
            new ParameterSpec("layerUri", typeof(string), NullabilityState.NotNull),
            new ParameterSpec("objectIds", typeof(IReadOnlyList<long>), NullabilityState.NotNull),
            new ParameterSpec("durationMilliseconds", typeof(int), NullabilityState.NotNull, 1000),
            new ParameterSpec("cancellationToken", typeof(CancellationToken), NullabilityState.NotNull, null));
    }

    private static async Task<object?> InvokeToolAsync(
        string typeName,
        string methodName,
        IBridgeClient bridge,
        params object?[] arguments)
    {
        var toolType = typeof(QuerySelectionTools).Assembly.GetType(typeName);
        Assert.NotNull(toolType);
        var tool = Activator.CreateInstance(toolType, bridge);
        Assert.NotNull(tool);
        var method = toolType.GetMethod(methodName, BindingFlags.Instance | BindingFlags.Public);
        Assert.NotNull(method);
        var task = Assert.IsAssignableFrom<Task>(method.Invoke(tool, arguments));
        await task;
        return task.GetType().GetProperty("Result")?.GetValue(task);
    }

    private static void AssertSignature(
        IReadOnlyDictionary<string, MethodInfo> methods,
        NullabilityInfoContext nullability,
        string toolName,
        Type returnType,
        params ParameterSpec[] expectedParameters)
    {
        var method = methods[toolName];
        Assert.Equal(returnType, method.ReturnType);
        var parameters = method.GetParameters();
        Assert.Equal(expectedParameters.Length, parameters.Length);
        for (var index = 0; index < parameters.Length; index++)
        {
            var parameter = parameters[index];
            var expected = expectedParameters[index];
            Assert.Equal(expected.Name, parameter.Name);
            Assert.Equal(expected.ParameterType, parameter.ParameterType);
            Assert.Equal(expected.Nullability, nullability.Create(parameter).ReadState);
            Assert.Equal(expected.HasDefaultValue, parameter.HasDefaultValue);
            if (expected.HasDefaultValue)
            {
                Assert.Equal(expected.DefaultValue, parameter.DefaultValue);
            }
        }
    }

    private static void AssertForwarded<TArguments, TResult>(
        RecordingBridgeClient bridge,
        string expectedOperation,
        TArguments expectedArguments,
        CancellationToken expectedCancellationToken)
    {
        var call = Assert.Single(bridge.Calls);
        Assert.Equal(expectedOperation, call.Operation);
        Assert.IsType<TArguments>(call.Arguments);
        Assert.Equal(expectedArguments, call.Arguments);
        Assert.Equal(typeof(TResult), call.ResultType);
        Assert.NotEqual(CancellationToken.None, expectedCancellationToken);
        Assert.Equal(expectedCancellationToken, call.CancellationToken);
    }

    private sealed class RecordingBridgeClient(object response) : IBridgeClient
    {
        public List<BridgeCall> Calls { get; } = [];

        public Task<T> InvokeAsync<T>(
            string operation,
            object? arguments,
            CancellationToken ct)
        {
            Calls.Add(new BridgeCall(operation, arguments, typeof(T), ct));
            if (response is Exception exception)
            {
                return Task.FromException<T>(exception);
            }

            return Task.FromResult((T)response);
        }
    }

    private sealed record ParameterSpec(
        string Name,
        Type ParameterType,
        NullabilityState Nullability,
        bool HasDefaultValue,
        object? DefaultValue)
    {
        internal ParameterSpec(
            string name,
            Type parameterType,
            NullabilityState nullability)
            : this(name, parameterType, nullability, false, null)
        {
        }

        internal ParameterSpec(
            string name,
            Type parameterType,
            NullabilityState nullability,
            object? defaultValue)
            : this(name, parameterType, nullability, true, defaultValue)
        {
        }
    }

    private sealed class BridgeSentinelException : Exception;

    private sealed record BridgeCall(
        string Operation,
        object? Arguments,
        Type ResultType,
        CancellationToken CancellationToken);
}
