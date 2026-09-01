using System.Text.Json;
using System.Text.Json.Serialization;

namespace ArcGISProAgent.Contracts;

public static class BridgeProtocol
{
    public const string Current = "1.0";
    public const string DefaultPipeName = "ArcGISProAgent.Bridge.v1";
}

public static class BridgeJson
{
    public static JsonSerializerOptions Options { get; } = new(JsonSerializerDefaults.Web)
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
    };
}
