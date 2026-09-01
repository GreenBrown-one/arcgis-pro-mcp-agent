using System.ComponentModel;
using ArcGISProAgent.Bridge;
using ArcGISProAgent.Contracts;
using ModelContextProtocol.Server;

namespace ArcGISProAgent.Mcp.Tools;

[McpServerToolType]
public sealed class ContextTools(IBridgeClient bridge)
{
    [McpServerTool(Name = "arcgis_describe_context", ReadOnly = true, Destructive = false, Idempotent = true)]
    [Description("Describe the current project and active view using stable project item URIs.")]
    public Task<ContextDescription> DescribeContextAsync(
        CancellationToken cancellationToken)
    {
        var arguments = new ContextDescribeArguments();
        GisContractGuards.Validate(arguments);
        return bridge.InvokeAsync<ContextDescription>(
            "context.describe",
            arguments,
            cancellationToken);
    }
}
