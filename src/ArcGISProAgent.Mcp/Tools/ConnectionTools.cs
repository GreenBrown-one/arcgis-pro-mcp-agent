using System.ComponentModel;
using ArcGISProAgent.Bridge;
using ArcGISProAgent.Contracts;
using ModelContextProtocol.Server;

namespace ArcGISProAgent.Mcp.Tools;

[McpServerToolType]
public sealed class ConnectionTools(IBridgeClient bridge)
{
    [McpServerTool(Name = "arcgis_connection_status", ReadOnly = true, Destructive = false, Idempotent = true)]
    [Description("Return the live ArcGIS Pro Add-In connection, version, project, map, and capability status.")]
    public Task<BridgeHealth> StatusAsync(CancellationToken cancellationToken) =>
        bridge.InvokeAsync<BridgeHealth>("connection.health", new { }, cancellationToken);

    [McpServerTool(Name = "arcgis_capabilities", ReadOnly = true, Destructive = false, Idempotent = true)]
    [Description("List the ArcGIS Pro operations currently supported by the connected Add-In.")]
    public async Task<IReadOnlyList<CapabilityDescriptor>> CapabilitiesAsync(
        CancellationToken cancellationToken) =>
        (await StatusAsync(cancellationToken)).Capabilities;
}
