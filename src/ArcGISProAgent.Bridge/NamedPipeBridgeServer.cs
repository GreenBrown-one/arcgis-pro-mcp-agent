using System.IO.Pipes;
using System.Runtime.CompilerServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ArcGISProAgent.Contracts;

[assembly: InternalsVisibleTo("ArcGISProAgent.Bridge.Tests")]

namespace ArcGISProAgent.Bridge;

public sealed class NamedPipeBridgeServer
{
    private readonly string _pipeName;
    private readonly Func<string> _tokenProvider;
    private readonly Func<CancellationToken, Task<NamedPipeServerStream>> _listenerFactory;

    public NamedPipeBridgeServer(
        string pipeName,
        Func<string> tokenProvider)
    {
        _pipeName = !string.IsNullOrWhiteSpace(pipeName)
            ? pipeName
            : throw new ArgumentException("Pipe name is required.", nameof(pipeName));
        _tokenProvider = tokenProvider
            ?? throw new ArgumentNullException(nameof(tokenProvider));
        _listenerFactory = AcceptConnectionAsync;
    }

    internal NamedPipeBridgeServer(
        string pipeName,
        Func<string> tokenProvider,
        Func<CancellationToken, Task<NamedPipeServerStream>> listenerFactory)
        : this(pipeName, tokenProvider)
    {
        _listenerFactory = listenerFactory
            ?? throw new ArgumentNullException(nameof(listenerFactory));
    }

    public async Task RunAsync(
        Func<BridgeRequest, CancellationToken, Task<BridgeResponse>> handler,
        CancellationToken ct)
    {
        ArgumentNullException.ThrowIfNull(handler);

        while (true)
        {
            ct.ThrowIfCancellationRequested();
            await using var server = await _listenerFactory(ct).ConfigureAwait(false);
            try
            {
                await ProcessConnectionAsync(server, handler, ct)
                    .ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (ct.IsCancellationRequested)
            {
                throw;
            }
            catch (Exception)
            {
                // A connection failure must not terminate the accept loop.
            }
        }
    }

    private async Task<NamedPipeServerStream> AcceptConnectionAsync(
        CancellationToken ct)
    {
        var server = new NamedPipeServerStream(
            _pipeName,
            PipeDirection.InOut,
            1,
            PipeTransmissionMode.Byte,
            PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly);
        try
        {
            await server.WaitForConnectionAsync(ct).ConfigureAwait(false);
            return server;
        }
        catch
        {
            await server.DisposeAsync().ConfigureAwait(false);
            throw;
        }
    }

    private async Task ProcessConnectionAsync(
        Stream stream,
        Func<BridgeRequest, CancellationToken, Task<BridgeResponse>> handler,
        CancellationToken ct)
    {
        BridgeResponse response;

        try
        {
            var request = await ReadRequestAsync(stream, ct).ConfigureAwait(false);
            response = await DispatchAsync(request, handler, ct).ConfigureAwait(false);
        }
        catch (JsonLineTooLongException)
        {
            response = BridgeResponse.Failure(
                string.Empty,
                "request_too_large",
                "Bridge request exceeds 1 MiB.");
        }
        catch (JsonException)
        {
            response = BridgeResponse.Failure(
                string.Empty,
                "invalid_request",
                "Bridge request is invalid.");
        }
        catch (DecoderFallbackException)
        {
            response = BridgeResponse.Failure(
                string.Empty,
                "invalid_request",
                "Bridge request is invalid.");
        }
        catch (InvalidDataException)
        {
            response = BridgeResponse.Failure(
                string.Empty,
                "invalid_request",
                "Bridge request is invalid.");
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }

        try
        {
            await WriteResponseAsync(stream, response, ct).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception)
        {
            var fallback = BridgeResponse.Failure(
                response?.RequestId ?? string.Empty,
                "bridge_internal_error",
                "Bridge response could not be serialized.");
            await WriteResponseAsync(stream, fallback, ct).ConfigureAwait(false);
        }
    }

    private async Task<BridgeResponse> DispatchAsync(
        BridgeRequest request,
        Func<BridgeRequest, CancellationToken, Task<BridgeResponse>> handler,
        CancellationToken ct)
    {
        if (!string.Equals(
                request.ProtocolVersion,
                BridgeProtocol.Current,
                StringComparison.Ordinal))
        {
            return BridgeResponse.Failure(
                request.RequestId,
                "protocol_mismatch",
                $"Expected protocol {BridgeProtocol.Current}");
        }

        string expectedToken;
        try
        {
            expectedToken = _tokenProvider();
            if (string.IsNullOrEmpty(expectedToken))
            {
                throw new InvalidDataException("Runtime token is unavailable.");
            }
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception)
        {
            return BridgeResponse.Failure(
                request.RequestId,
                "runtime_not_ready",
                "Bridge runtime credentials are not ready.");
        }

        if (!TokensMatch(request.AuthToken, expectedToken))
        {
            return BridgeResponse.Failure(
                request.RequestId,
                "unauthorized",
                "Bridge token rejected");
        }

        try
        {
            return await handler(request, ct).ConfigureAwait(false)
                ?? BridgeResponse.Failure(
                    request.RequestId,
                    "bridge_internal_error",
                    "Bridge request handler returned no response.");
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception)
        {
            return BridgeResponse.Failure(
                request.RequestId,
                "bridge_internal_error",
                "Bridge request handler failed.");
        }
    }

    private static bool TokensMatch(string? suppliedToken, string expectedToken)
    {
        if (suppliedToken is null)
        {
            return false;
        }

        var suppliedBytes = Encoding.UTF8.GetBytes(suppliedToken);
        var expectedBytes = Encoding.UTF8.GetBytes(expectedToken);
        return CryptographicOperations.FixedTimeEquals(
            suppliedBytes,
            expectedBytes);
    }

    private static async Task<BridgeRequest> ReadRequestAsync(
        Stream stream,
        CancellationToken ct)
    {
        var json = await BridgeJsonLine.ReadAsync(
                stream,
                BridgeJsonLine.MaximumLineBytes,
                ct)
            .ConfigureAwait(false);
        var request = JsonSerializer.Deserialize<BridgeRequest>(
                json,
                BridgeJson.Options)
            ?? throw new InvalidDataException("Bridge request is empty.");

        if (string.IsNullOrEmpty(request.ProtocolVersion)
            || string.IsNullOrEmpty(request.RequestId)
            || string.IsNullOrEmpty(request.Operation)
            || request.AuthToken is null)
        {
            throw new InvalidDataException("Bridge request fields are invalid.");
        }

        return request;
    }

    private static Task WriteResponseAsync(
        Stream stream,
        BridgeResponse response,
        CancellationToken ct)
    {
        var json = JsonSerializer.Serialize(response, BridgeJson.Options);
        return BridgeJsonLine.WriteAsync(stream, json, ct);
    }
}

internal static class BridgeJsonLine
{
    public const int MaximumLineBytes = 1024 * 1024;

    private static readonly Encoding Utf8 = new UTF8Encoding(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    public static async Task<string> ReadAsync(
        Stream stream,
        int maximumBytes,
        CancellationToken ct)
    {
        using var line = new MemoryStream();
        var buffer = new byte[8192];

        while (true)
        {
            var bytesRead = await stream.ReadAsync(buffer, ct).ConfigureAwait(false);
            if (bytesRead == 0)
            {
                throw new InvalidDataException("JSONL stream ended before a newline.");
            }

            var newline = Array.IndexOf(buffer, (byte)'\n', 0, bytesRead);
            var contentBytes = newline >= 0 ? newline : bytesRead;
            if (line.Length + contentBytes > maximumBytes)
            {
                throw new JsonLineTooLongException();
            }

            line.Write(buffer, 0, contentBytes);
            if (newline >= 0)
            {
                var bytes = line.ToArray();
                var length = bytes.Length;
                if (length > 0 && bytes[length - 1] == (byte)'\r')
                {
                    length--;
                }

                return Utf8.GetString(bytes, 0, length);
            }
        }
    }

    public static async Task WriteAsync(
        Stream stream,
        string json,
        CancellationToken ct)
    {
        var payload = Utf8.GetBytes(json);
        await stream.WriteAsync(payload, ct).ConfigureAwait(false);
        await stream.WriteAsync("\n"u8.ToArray(), ct).ConfigureAwait(false);
        await stream.FlushAsync(ct).ConfigureAwait(false);
    }
}

internal sealed class JsonLineTooLongException : Exception;
