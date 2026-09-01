using System.Runtime.CompilerServices;
using ArcGISProAgent.Bridge;

[assembly: InternalsVisibleTo("ArcGISProAgent.Mcp.Tests")]

namespace ArcGISProAgent.Mcp;

public static class BridgeClientFactory
{
    private const string RuntimeFileEnvironmentVariable = "ARCGIS_AGENT_RUNTIME_FILE";
    internal static readonly TimeSpan BridgeTimeout = TimeSpan.FromSeconds(5);

    public static IBridgeClient Create()
    {
        var runtimeFile = Environment.GetEnvironmentVariable(RuntimeFileEnvironmentVariable);
        if (string.IsNullOrWhiteSpace(runtimeFile))
        {
            throw new InvalidOperationException(
                $"{RuntimeFileEnvironmentVariable} must point to the ArcGIS Pro Agent runtime credential file.");
        }

        var credentials = RuntimeCredentials.Load(runtimeFile);
        return new NamedPipeBridgeClient(
            credentials.PipeName,
            () => credentials.Token,
            BridgeTimeout);
    }
}
