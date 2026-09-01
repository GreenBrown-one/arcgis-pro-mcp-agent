using System.Text.Json;
using ArcGISProAgent.Contracts;

namespace ArcGISProAgent.Contracts.Tests;

public sealed class BridgeProtocolTests
{
    [Fact]
    public void Request_round_trips_without_losing_operation_identity()
    {
        var request = BridgeRequest.Create(
            operation: "connection.health",
            authToken: "secret",
            arguments: new { includeCapabilities = true },
            requestId: "op-123");

        var json = JsonSerializer.Serialize(request, BridgeJson.Options);
        var restored = JsonSerializer.Deserialize<BridgeRequest>(json, BridgeJson.Options)!;

        Assert.Equal("1.0", restored.ProtocolVersion);
        Assert.Equal("op-123", restored.RequestId);
        Assert.Equal("connection.health", restored.Operation);
        Assert.True(restored.Arguments.GetProperty("includeCapabilities").GetBoolean());
    }

    [Fact]
    public void Failure_contains_stable_code_and_no_success_data()
    {
        var response = BridgeResponse.Failure(
            "op-123", "protocol_mismatch", "Expected protocol 1.0");

        Assert.False(response.Ok);
        Assert.Null(response.Data);
        Assert.Equal("protocol_mismatch", response.Error!.Code);
    }

    [Fact]
    public void Redacted_ArcGIS_failure_uses_the_stable_public_error_code()
    {
        const string sensitiveDetail = "C:\\private\\data.gdb; SELECT secret";
        var response = BridgeResponse.Failure(
            "op-456",
            BridgeErrorCodes.ArcGisOperationFailed,
            "ArcGIS Pro operation failed.",
            sensitiveDetail);

        var json = JsonSerializer.Serialize(response, BridgeJson.Options);

        Assert.Equal("arcgis_operation_failed", response.Error!.Code);
        Assert.Null(response.Error.Detail);
        Assert.DoesNotContain(sensitiveDetail, json, StringComparison.Ordinal);
    }

    [Fact]
    public void Bridge_error_codes_include_existing_and_phase_two_codes()
    {
        Assert.Equal("request_too_large", BridgeErrorCodes.RequestTooLarge);
        Assert.Equal("invalid_request", BridgeErrorCodes.InvalidRequest);
        Assert.Equal("bridge_internal_error", BridgeErrorCodes.BridgeInternalError);
        Assert.Equal("protocol_mismatch", BridgeErrorCodes.ProtocolMismatch);
        Assert.Equal("runtime_not_ready", BridgeErrorCodes.RuntimeNotReady);
        Assert.Equal("unauthorized", BridgeErrorCodes.Unauthorized);
        Assert.Equal("operation_not_found", BridgeErrorCodes.OperationNotFound);
        Assert.Equal("invalid_arguments", BridgeErrorCodes.InvalidArguments);
        Assert.Equal("no_active_project", BridgeErrorCodes.NoActiveProject);
        Assert.Equal("no_active_view", BridgeErrorCodes.NoActiveView);
        Assert.Equal("no_active_map", BridgeErrorCodes.NoActiveMap);
        Assert.Equal("project_item_not_found", BridgeErrorCodes.ProjectItemNotFound);
        Assert.Equal("layer_not_found", BridgeErrorCodes.LayerNotFound);
        Assert.Equal("ambiguous_layer", BridgeErrorCodes.AmbiguousLayer);
        Assert.Equal("unsupported_layer_type", BridgeErrorCodes.UnsupportedLayerType);
        Assert.Equal("field_not_found", BridgeErrorCodes.FieldNotFound);
        Assert.Equal("invalid_predicate", BridgeErrorCodes.InvalidPredicate);
        Assert.Equal("invalid_extent", BridgeErrorCodes.InvalidExtent);
        Assert.Equal("invalid_spatial_source", BridgeErrorCodes.InvalidSpatialSource);
        Assert.Equal("data_source_unavailable", BridgeErrorCodes.DataSourceUnavailable);
        Assert.Equal("navigation_interrupted", BridgeErrorCodes.NavigationInterrupted);
        Assert.Equal("arcgis_operation_failed", BridgeErrorCodes.ArcGisOperationFailed);
    }
}
