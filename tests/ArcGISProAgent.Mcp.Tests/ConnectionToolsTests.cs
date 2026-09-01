using ArcGISProAgent.Bridge;
using ArcGISProAgent.Contracts;
using ArcGISProAgent.Mcp.Tools;

namespace ArcGISProAgent.Mcp.Tests;

public sealed class ConnectionToolsTests
{
    [Fact]
    public async Task Status_forwards_the_health_request()
    {
        var expected = new BridgeHealth(true, "1.0", "0.1.0", "3.7", "Demo", "Map", []);
        var bridge = new RecordingBridgeClient(expected);
        var tools = new ConnectionTools(bridge);
        using var cancellation = new CancellationTokenSource();
        var token = cancellation.Token;

        var actual = await tools.StatusAsync(token);

        Assert.Same(expected, actual);
        Assert.Equal("connection.health", bridge.Operation);
        Assert.NotNull(bridge.Arguments);
        Assert.Empty(bridge.Arguments.GetType().GetProperties());
        Assert.NotEqual(CancellationToken.None, token);
        Assert.Equal(token, bridge.CancellationToken);
    }

    [Fact]
    public async Task Capabilities_forwards_the_health_request()
    {
        CapabilityDescriptor[] expected =
        [
            new("connection.health", "1.0", RiskLevel.R0, true, false, false, false),
            new("project.describe", "1.0", RiskLevel.R0, true, false, false, false),
        ];
        var health = new BridgeHealth(true, "1.0", "0.1.0", "3.7", "Demo", "Map", expected);
        var bridge = new RecordingBridgeClient(health);
        var tools = new ConnectionTools(bridge);
        using var cancellation = new CancellationTokenSource();
        var token = cancellation.Token;

        var actual = await tools.CapabilitiesAsync(token);

        Assert.Same(expected, actual);
        Assert.Equal("connection.health", bridge.Operation);
        Assert.NotNull(bridge.Arguments);
        Assert.Empty(bridge.Arguments.GetType().GetProperties());
        Assert.NotEqual(CancellationToken.None, token);
        Assert.Equal(token, bridge.CancellationToken);
    }

    private sealed class RecordingBridgeClient(BridgeHealth value) : IBridgeClient
    {
        public string? Operation { get; private set; }

        public object? Arguments { get; private set; }

        public CancellationToken CancellationToken { get; private set; }

        public Task<T> InvokeAsync<T>(
            string operation,
            object? arguments,
            CancellationToken ct)
        {
            Operation = operation;
            Arguments = arguments;
            CancellationToken = ct;
            return Task.FromResult((T)(object)value);
        }
    }
}
