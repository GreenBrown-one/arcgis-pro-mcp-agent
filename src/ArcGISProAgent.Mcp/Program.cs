using ArcGISProAgent.Bridge;
using ArcGISProAgent.Mcp;
using ArcGISProAgent.Mcp.Tools;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;

var builder = Host.CreateApplicationBuilder(args);
builder.Logging.ClearProviders(); // stdout is reserved for MCP JSON-RPC
builder.Logging.AddConsole(options => options.LogToStandardErrorThreshold = LogLevel.Trace);
builder.Services.AddSingleton<IBridgeClient>(_ => BridgeClientFactory.Create());
builder.Services.AddMcpServer(options =>
    {
        options.ServerInfo = new() { Name = "arcgis-pro-agent", Version = "0.2.0-preview.1" };
    })
    .WithStdioServerTransport()
    .WithTools<ConnectionTools>()
    .WithTools<ContextTools>()
    .WithTools<LayerTools>()
    .WithTools<QuerySelectionTools>()
    .WithTools<MapViewTools>();
await builder.Build().RunAsync();
