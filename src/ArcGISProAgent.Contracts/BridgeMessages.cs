using System.Text.Json;

namespace ArcGISProAgent.Contracts;

public sealed record BridgeRequest(
    string ProtocolVersion,
    string RequestId,
    string Operation,
    string AuthToken,
    JsonElement Arguments)
{
    public static BridgeRequest Create(
        string operation,
        string authToken,
        object? arguments = null,
        string? requestId = null) =>
        new(
            BridgeProtocol.Current,
            requestId ?? Guid.NewGuid().ToString("N"),
            operation,
            authToken,
            JsonSerializer.SerializeToElement(arguments ?? new { }, BridgeJson.Options));
}

public sealed record BridgeError(string Code, string Message, string? Detail = null);

public static class BridgeErrorCodes
{
    public const string RequestTooLarge = "request_too_large";
    public const string InvalidRequest = "invalid_request";
    public const string BridgeInternalError = "bridge_internal_error";
    public const string ProtocolMismatch = "protocol_mismatch";
    public const string RuntimeNotReady = "runtime_not_ready";
    public const string Unauthorized = "unauthorized";
    public const string OperationNotFound = "operation_not_found";
    public const string InvalidArguments = "invalid_arguments";
    public const string NoActiveProject = "no_active_project";
    public const string NoActiveView = "no_active_view";
    public const string NoActiveMap = "no_active_map";
    public const string ProjectItemNotFound = "project_item_not_found";
    public const string LayerNotFound = "layer_not_found";
    public const string AmbiguousLayer = "ambiguous_layer";
    public const string UnsupportedLayerType = "unsupported_layer_type";
    public const string FieldNotFound = "field_not_found";
    public const string InvalidPredicate = "invalid_predicate";
    public const string InvalidExtent = "invalid_extent";
    public const string InvalidSpatialSource = "invalid_spatial_source";
    public const string DataSourceUnavailable = "data_source_unavailable";
    public const string NavigationInterrupted = "navigation_interrupted";
    public const string ArcGisOperationFailed = "arcgis_operation_failed";
}

public sealed record BridgeResponse(
    string ProtocolVersion,
    string RequestId,
    bool Ok,
    JsonElement? Data,
    BridgeError? Error)
{
    public static BridgeResponse Success(string requestId, object data) =>
        new(BridgeProtocol.Current, requestId, true,
            JsonSerializer.SerializeToElement(data, BridgeJson.Options), null);

    public static BridgeResponse Failure(
        string requestId, string code, string message, string? detail = null) =>
        new(BridgeProtocol.Current, requestId, false, null,
            new BridgeError(
                code,
                message,
                string.Equals(
                    code,
                    BridgeErrorCodes.ArcGisOperationFailed,
                    StringComparison.Ordinal)
                    ? null
                    : detail));
}
