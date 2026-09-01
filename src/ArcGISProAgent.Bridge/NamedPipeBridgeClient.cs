using System.IO.Pipes;
using System.Text;
using System.Text.Json;
using ArcGISProAgent.Contracts;

namespace ArcGISProAgent.Bridge;

public sealed class NamedPipeBridgeClient(
    string pipeName,
    Func<string> tokenProvider,
    TimeSpan timeout) : IBridgeClient
{
    private readonly string _pipeName = !string.IsNullOrWhiteSpace(pipeName)
        ? pipeName
        : throw new ArgumentException("Pipe name is required.", nameof(pipeName));
    private readonly Func<string> _tokenProvider = tokenProvider
        ?? throw new ArgumentNullException(nameof(tokenProvider));
    private readonly TimeSpan _timeout = timeout > TimeSpan.Zero
        ? timeout
        : throw new ArgumentOutOfRangeException(nameof(timeout));

    public async Task<T> InvokeAsync<T>(
        string operation,
        object? arguments,
        CancellationToken ct)
    {
        ct.ThrowIfCancellationRequested();

        var request = BridgeRequest.Create(operation, _tokenProvider(), arguments);
        await using var pipe = new NamedPipeClientStream(
            ".",
            _pipeName,
            PipeDirection.InOut,
            PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly);

        try
        {
            await pipe.ConnectAsync(GetTimeoutMilliseconds(), ct).ConfigureAwait(false);
        }
        catch (TimeoutException)
        {
            throw new BridgeCallException(
                "arcgis_not_connected",
                "ArcGIS Pro bridge is not connected.");
        }
        catch (IOException)
        {
            throw new BridgeCallException(
                "arcgis_not_connected",
                "ArcGIS Pro bridge is not connected.");
        }

        using var timeoutCts = new CancellationTokenSource(_timeout);
        using var linkedCts = CancellationTokenSource.CreateLinkedTokenSource(
            ct,
            timeoutCts.Token);

        try
        {
            var requestJson = JsonSerializer.Serialize(request, BridgeJson.Options);
            await BridgeJsonLine.WriteAsync(pipe, requestJson, linkedCts.Token)
                .ConfigureAwait(false);
            var responseJson = await BridgeJsonLine.ReadAsync(
                    pipe,
                    BridgeJsonLine.MaximumLineBytes,
                    linkedCts.Token)
                .ConfigureAwait(false);
            var response = JsonSerializer.Deserialize<BridgeResponse>(
                    responseJson,
                    BridgeJson.Options)
                ?? throw InvalidResponse("Bridge returned an empty response.");

            ValidateResponse(request, response);
            if (!response.Ok)
            {
                var error = response.Error
                    ?? throw InvalidResponse("Bridge returned an error without details.");
                throw new BridgeCallException(error.Code, error.Message);
            }

            if (response.Data is not { } data)
            {
                throw InvalidResponse("Bridge returned no result data.");
            }

            var value = data.Deserialize<T>(BridgeJson.Options);
            return value is not null
                ? value
                : throw InvalidResponse("Bridge result data is invalid.");
        }
        catch (OperationCanceledException) when (!ct.IsCancellationRequested)
        {
            throw new BridgeCallException(
                "bridge_timeout",
                "ArcGIS Pro bridge request timed out.");
        }
        catch (JsonException)
        {
            throw InvalidResponse("Bridge returned invalid JSON.");
        }
        catch (DecoderFallbackException)
        {
            throw InvalidResponse("Bridge returned invalid UTF-8.");
        }
        catch (JsonLineTooLongException)
        {
            throw InvalidResponse("Bridge response is too large.");
        }
        catch (IOException)
        {
            throw new BridgeCallException(
                "arcgis_not_connected",
                "ArcGIS Pro bridge disconnected.");
        }
    }

    private int GetTimeoutMilliseconds()
    {
        var milliseconds = _timeout.TotalMilliseconds;
        return milliseconds >= int.MaxValue
            ? int.MaxValue
            : Math.Max(1, (int)Math.Ceiling(milliseconds));
    }

    private static void ValidateResponse(
        BridgeRequest request,
        BridgeResponse response)
    {
        if (!string.Equals(
                response.ProtocolVersion,
                BridgeProtocol.Current,
                StringComparison.Ordinal))
        {
            throw new BridgeCallException(
                "protocol_mismatch",
                $"Expected protocol {BridgeProtocol.Current}.");
        }

        if (!string.Equals(
                response.RequestId,
                request.RequestId,
                StringComparison.Ordinal))
        {
            throw InvalidResponse("Bridge response request ID does not match.");
        }
    }

    private static BridgeCallException InvalidResponse(string message) =>
        new("invalid_response", message);
}
