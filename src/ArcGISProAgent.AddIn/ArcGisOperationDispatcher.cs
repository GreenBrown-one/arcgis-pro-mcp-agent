using System.Diagnostics;
using System.Text.Json;
using ArcGIS.Desktop.Core;
using ArcGIS.Desktop.Framework.Threading.Tasks;
using ArcGIS.Desktop.Mapping;
using ArcGISProAgent.AddIn.Operations;
using ArcGISProAgent.Contracts;

namespace ArcGISProAgent.AddIn;

internal sealed class ArcGisOperationDispatcher
{
    private const int MaximumBridgeResponseBytes = 1024 * 1024;

    public async Task<BridgeResponse> DispatchAsync(
        BridgeRequest request,
        CancellationToken ct)
    {
        ct.ThrowIfCancellationRequested();
        BridgeResponse response;
        try
        {
            response = await (request.Operation switch
            {
                "map_view.activate" => ExecuteAsync<ActivateViewArguments, ActivateViewResult>(
                    request,
                    GisContractGuards.Validate,
                    MapViewOperations.ActivateAsync),
                "map_view.zoom_to_layer" => ExecuteAsync<ZoomToLayerArguments, ZoomResult>(
                    request,
                    GisContractGuards.Validate,
                    MapViewOperations.ZoomToLayerAsync),
                "map_view.zoom_to_extent" => ExecuteAsync<ZoomToExtentArguments, ZoomResult>(
                    request,
                    GisContractGuards.Validate,
                    MapViewOperations.ZoomToExtentAsync),
                "map_view.flash_features" => ExecuteAsync<FlashFeaturesArguments, FlashFeaturesResult>(
                    request,
                    MapViewOperations.ValidateFlashArguments,
                    MapViewOperations.FlashFeaturesAsync),
                _ => QueuedTask.Run((Func<BridgeResponse>)(() =>
                {
                    ct.ThrowIfCancellationRequested();
                    return request.Operation switch
                    {
                        "connection.health" => Health(request),
                        "context.describe" => Execute<ContextDescribeArguments, ContextDescription>(
                            request,
                            GisContractGuards.Validate,
                            _ => ContextOperations.Describe()),
                        "layers.list" => Execute<ListLayersArguments, LayerListResult>(
                            request,
                            GisContractGuards.Validate,
                            LayerOperations.List),
                        "layers.describe" => Execute<DescribeLayerArguments, LayerDescription>(
                            request,
                            GisContractGuards.Validate,
                            LayerOperations.Describe),
                        "layers.fields" => Execute<ListFieldsArguments, LayerFieldsResult>(
                            request,
                            GisContractGuards.Validate,
                            LayerOperations.ListFields),
                        "query.feature_count" => Execute<FeatureCountArguments, FeatureCountResult>(
                            request,
                            GisContractGuards.Validate,
                            QueryOperations.Count),
                        "query.features" => Execute<FeatureQueryArguments, FeatureQueryResult>(
                            request,
                            GisContractGuards.Validate,
                            QueryOperations.Query),
                        "query.spatial" => Execute<SpatialQueryArguments, FeatureQueryResult>(
                            request,
                            GisContractGuards.Validate,
                            QueryOperations.QuerySpatial),
                        "selection.describe" => Execute<SelectionDescribeArguments, SelectionDescription>(
                            request,
                            GisContractGuards.Validate,
                            QueryOperations.DescribeSelection),
                        "selection.by_attribute" => Execute<SelectByAttributeArguments, SelectionResult>(
                            request,
                            GisContractGuards.Validate,
                            SelectionOperations.SelectByAttribute),
                        "selection.by_location" => Execute<SelectByLocationArguments, SelectionResult>(
                            request,
                            GisContractGuards.Validate,
                            SelectionOperations.SelectByLocation),
                        "selection.clear" => Execute<ClearSelectionArguments, ClearSelectionResult>(
                            request,
                            GisContractGuards.Validate,
                            SelectionOperations.Clear),
                        _ => BridgeResponse.Failure(
                            request.RequestId,
                            BridgeErrorCodes.OperationNotFound,
                            "The requested operation is not available."),
                    };
                })),
            });
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (ArcGisOperationException ex)
        {
            response = BridgeResponse.Failure(
                request.RequestId,
                ex.Code,
                ex.PublicMessage);
        }
        catch
        {
            response = BridgeResponse.Failure(
                request.RequestId,
                BridgeErrorCodes.ArcGisOperationFailed,
                "ArcGIS Pro could not complete the operation.");
        }

        return EnsureResponseFitsFrame(response);
    }

    private static BridgeResponse EnsureResponseFitsFrame(BridgeResponse response)
    {
        try
        {
            if (JsonSerializer.SerializeToUtf8Bytes(response, BridgeJson.Options).Length
                <= MaximumBridgeResponseBytes)
            {
                return response;
            }

            return CreateBoundedFailure(
                response.RequestId,
                BridgeErrorCodes.RequestTooLarge,
                "The operation response exceeds the 1 MiB bridge limit.");
        }
        catch
        {
            return CreateBoundedFailure(
                response.RequestId,
                BridgeErrorCodes.ArcGisOperationFailed,
                "ArcGIS Pro could not complete the operation.");
        }
    }

    private static BridgeResponse CreateBoundedFailure(
        string requestId,
        string code,
        string message)
    {
        var failure = BridgeResponse.Failure(requestId, code, message);
        return JsonSerializer.SerializeToUtf8Bytes(failure, BridgeJson.Options).Length
                <= MaximumBridgeResponseBytes
            ? failure
            : BridgeResponse.Failure(string.Empty, code, message);
    }

    private static BridgeResponse Execute<TArguments, TResult>(
        BridgeRequest request,
        Action<TArguments> validate,
        Func<TArguments, TResult> operation)
    {
        TArguments arguments;
        try
        {
            arguments = request.Arguments.Deserialize<TArguments>(BridgeJson.Options)
                ?? throw new JsonException("Arguments are required.");
            validate(arguments);
        }
        catch (Exception ex) when (ex is JsonException or ArgumentException)
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.InvalidArguments,
                "The operation arguments are invalid.");
        }

        return BridgeResponse.Success(request.RequestId, operation(arguments)!);
    }

    private static async Task<BridgeResponse> ExecuteAsync<TArguments, TResult>(
        BridgeRequest request,
        Action<TArguments> validate,
        Func<TArguments, Task<TResult>> operation)
    {
        TArguments arguments;
        try
        {
            arguments = request.Arguments.Deserialize<TArguments>(BridgeJson.Options)
                ?? throw new JsonException("Arguments are required.");
            validate(arguments);
        }
        catch (Exception ex) when (ex is JsonException or ArgumentException)
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.InvalidArguments,
                "The operation arguments are invalid.");
        }

        return BridgeResponse.Success(request.RequestId, (await operation(arguments))!);
    }

    private static BridgeResponse Health(BridgeRequest request)
    {
        var project = Project.Current;
        var map = MapView.Active?.Map;
        var processPath = Environment.ProcessPath;
        var arcGisVersion = processPath is null
            ? "unknown"
            : FileVersionInfo.GetVersionInfo(processPath).ProductVersion ?? "unknown";
        var health = new BridgeHealth(
            true,
            BridgeProtocol.Current,
            typeof(AgentModule).Assembly.GetName().Version?.ToString() ?? "0.1.0",
            arcGisVersion,
            project?.Name,
            map?.Name,
            CapabilityCatalog.All);
        return BridgeResponse.Success(request.RequestId, health);
    }
}

internal static class CapabilityCatalog
{
    private static readonly IReadOnlyList<string> RuntimeOperationIds =
        Array.AsReadOnly(
        [
            "connection.health",
            "context.describe",
            "layers.list",
            "layers.describe",
            "layers.fields",
            "query.feature_count",
            "query.features",
            "query.spatial",
            "selection.describe",
            "selection.by_attribute",
            "selection.by_location",
            "selection.clear",
            "map_view.activate",
            "map_view.zoom_to_layer",
            "map_view.zoom_to_extent",
            "map_view.flash_features",
        ]);

    internal static IReadOnlyList<CapabilityDescriptor> All { get; } = Build();

    private static IReadOnlyList<CapabilityDescriptor> Build()
    {
        var descriptors = OperationCatalog.Foundation
            .Concat(OperationCatalog.Phase2.Where(item =>
                item.Risk is RiskLevel.R0 or RiskLevel.R1))
            .ToDictionary(item => item.Id, StringComparer.Ordinal);
        return Array.AsReadOnly(
            RuntimeOperationIds.Select(id => descriptors[id]).ToArray());
    }
}
