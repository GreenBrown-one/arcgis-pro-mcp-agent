namespace ArcGISProAgent.Bridge;

public interface IBridgeClient
{
    Task<T> InvokeAsync<T>(string operation, object? arguments, CancellationToken ct);
}

public sealed class BridgeCallException(string code, string message) : Exception(message)
{
    public string Code { get; } = code;
}
