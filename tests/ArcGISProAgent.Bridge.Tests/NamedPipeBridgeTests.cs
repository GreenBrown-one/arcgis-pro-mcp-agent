using System.IO.Pipes;
using System.Text;
using System.Text.Json;
using ArcGISProAgent.Bridge;
using ArcGISProAgent.Contracts;

namespace ArcGISProAgent.Bridge.Tests;

public sealed class NamedPipeBridgeTests
{
    private static readonly TimeSpan TestTimeout = TimeSpan.FromSeconds(10);

    [Fact]
    public async Task Valid_token_returns_health_response()
    {
        var pipe = NewPipeName();
        using var cts = new CancellationTokenSource(TestTimeout);
        var server = new NamedPipeBridgeServer(pipe, () => "token");
        var run = server.RunAsync(HealthHandler, cts.Token);
        var client = CreateClient(pipe);

        await AssertHealthyAsync(client, cts.Token);

        await StopServerAsync(cts, run);
    }

    [Theory]
    [InlineData("creation")]
    [InlineData("wait")]
    public async Task Listener_failure_is_propagated_without_retry(
        string scenario)
    {
        var expected = new InvalidOperationException("Listener configuration is invalid.");
        var retried = NewSignal();
        var pending = new TaskCompletionSource<NamedPipeServerStream>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        using var cts = new CancellationTokenSource();
        var attempts = 0;

        Task<NamedPipeServerStream> CreateListener(CancellationToken _)
        {
            if (Interlocked.Increment(ref attempts) > 1)
            {
                retried.TrySetResult(true);
                return pending.Task;
            }

            return scenario == "creation"
                ? throw expected
                : Task.FromException<NamedPipeServerStream>(expected);
        }

        var server = new NamedPipeBridgeServer(
            NewPipeName(),
            () => "token",
            CreateListener);
        var run = server.RunAsync(HealthHandler, cts.Token);

        try
        {
            var completed = await Task.WhenAny(run, retried.Task)
                .WaitAsync(TestTimeout);
            Assert.Same(run, completed);

            var actual = await Assert.ThrowsAsync<InvalidOperationException>(() =>
                run.WaitAsync(TestTimeout));
            Assert.Same(expected, actual);
            Assert.Equal(1, attempts);
        }
        finally
        {
            cts.Cancel();
            pending.TrySetCanceled(cts.Token);
            try
            {
                await run.WaitAsync(TestTimeout);
            }
            catch (Exception)
            {
                // The assertion above verifies the production exception.
            }
        }
    }

    [Fact]
    public async Task Invalid_token_is_rejected_before_handler_runs_and_loop_survives()
    {
        var pipe = NewPipeName();
        using var cts = new CancellationTokenSource(TestTimeout);
        var called = false;
        var server = new NamedPipeBridgeServer(pipe, () => "server-token");
        var run = server.RunAsync((request, _) =>
        {
            called = true;
            return Task.FromResult(BridgeResponse.Success(request.RequestId, new { connected = true }));
        }, cts.Token);

        try
        {
            var invalidClient = CreateClient(pipe, "wrong-token");
            var error = await Assert.ThrowsAsync<BridgeCallException>(() =>
                invalidClient.InvokeAsync<object>("connection.health", new { }, cts.Token));

            Assert.Equal("unauthorized", error.Code);
            Assert.False(called);

            await AssertHealthyAsync(CreateClient(pipe, "server-token"), cts.Token);
            Assert.True(called);
        }
        finally
        {
            await StopServerAsync(cts, run);
        }
    }

    [Fact]
    public async Task Protocol_mismatch_is_rejected_before_handler_runs_and_loop_survives()
    {
        var pipe = NewPipeName();
        using var cts = new CancellationTokenSource(TestTimeout);
        var called = false;
        var server = new NamedPipeBridgeServer(pipe, () => "token");
        var run = server.RunAsync((request, _) =>
        {
            called = true;
            return Task.FromResult(BridgeResponse.Success(request.RequestId, new { connected = true }));
        }, cts.Token);

        try
        {
            var request = CreateRequest(protocolVersion: "0.9");
            var response = await SendRawRequestAsync(pipe, request, cts.Token);

            Assert.Equal("protocol_mismatch", response.Error!.Code);
            Assert.False(called);

            await AssertHealthyAsync(CreateClient(pipe), cts.Token);
            Assert.True(called);
        }
        finally
        {
            await StopServerAsync(cts, run);
        }
    }

    [Fact]
    public async Task Runtime_not_ready_recovers_when_credentials_become_available()
    {
        var pipe = NewPipeName();
        using var cts = new CancellationTokenSource(TestTimeout);
        string? runtimeToken = null;
        var server = new NamedPipeBridgeServer(
            pipe,
            () => runtimeToken ?? throw new FileNotFoundException("Runtime file is not ready."));
        var run = server.RunAsync(HealthHandler, cts.Token);

        try
        {
            var response = await SendRawRequestAsync(pipe, CreateRequest(), cts.Token);
            Assert.Equal("runtime_not_ready", response.Error!.Code);

            runtimeToken = "token";
            await AssertHealthyAsync(CreateClient(pipe), cts.Token);
        }
        finally
        {
            await StopServerAsync(cts, run);
        }
    }

    [Theory]
    [InlineData("malformed", "invalid_request")]
    [InlineData("too-large", "request_too_large")]
    [InlineData("invalid-utf8", "invalid_request")]
    public async Task Invalid_wire_request_returns_stable_error_and_loop_survives(
        string scenario,
        string expectedCode)
    {
        var pipe = NewPipeName();
        using var cts = new CancellationTokenSource(TestTimeout);
        var server = new NamedPipeBridgeServer(pipe, () => "token");
        var run = server.RunAsync(HealthHandler, cts.Token);

        try
        {
            var response = await SendRawLineAsync(
                pipe,
                CreateInvalidLine(scenario),
                cts.Token);

            Assert.Equal(expectedCode, response.Error!.Code);
            await AssertHealthyAsync(CreateClient(pipe), cts.Token);
        }
        finally
        {
            await StopServerAsync(cts, run);
        }
    }

    [Fact]
    public async Task Handler_exception_returns_internal_error_and_loop_survives()
    {
        var pipe = NewPipeName();
        using var cts = new CancellationTokenSource(TestTimeout);
        var calls = 0;
        var server = new NamedPipeBridgeServer(pipe, () => "token");
        var run = server.RunAsync((request, _) =>
        {
            if (Interlocked.Increment(ref calls) == 1)
            {
                throw new InvalidOperationException("Handler failed.");
            }

            return Task.FromResult(BridgeResponse.Success(
                request.RequestId,
                new { connected = true }));
        }, cts.Token);

        try
        {
            var client = CreateClient(pipe);
            var error = await Assert.ThrowsAsync<BridgeCallException>(() =>
                client.InvokeAsync<object>("connection.health", new { }, cts.Token));

            Assert.Equal("bridge_internal_error", error.Code);
            await AssertHealthyAsync(client, cts.Token);
        }
        finally
        {
            await StopServerAsync(cts, run);
        }
    }

    [Fact]
    public async Task Server_cancellation_after_handler_enters_is_propagated()
    {
        var pipe = NewPipeName();
        using var testCts = new CancellationTokenSource(TestTimeout);
        using var serverCts = new CancellationTokenSource();
        var entered = NewSignal();
        var gate = NewSignal();
        var server = new NamedPipeBridgeServer(pipe, () => "token");
        var run = server.RunAsync(async (request, ct) =>
        {
            entered.TrySetResult(true);
            await gate.Task.WaitAsync(ct);
            return BridgeResponse.Success(request.RequestId, new { connected = true });
        }, serverCts.Token);

        await using var client = CreateRawClient(pipe);
        await client.ConnectAsync(2000, testCts.Token);
        await WriteLineAsync(
            client,
            JsonSerializer.Serialize(CreateRequest(), BridgeJson.Options),
            testCts.Token);
        Assert.True(await entered.Task.WaitAsync(TestTimeout));

        serverCts.Cancel();

        try
        {
            await Assert.ThrowsAnyAsync<OperationCanceledException>(() =>
                run.WaitAsync(TestTimeout));
        }
        finally
        {
            gate.TrySetResult(true);
            serverCts.Cancel();
        }
    }

    [Theory]
    [InlineData("null")]
    [InlineData("disposed-json")]
    [InlineData("undefined-json")]
    public async Task Unserializable_handler_response_returns_internal_error_and_loop_survives(
        string scenario)
    {
        var pipe = NewPipeName();
        using var cts = new CancellationTokenSource(TestTimeout);
        var calls = 0;
        var server = new NamedPipeBridgeServer(pipe, () => "token");
        var run = server.RunAsync((request, _) =>
        {
            if (Interlocked.Increment(ref calls) == 1)
            {
                return Task.FromResult(CreateInvalidHandlerResponse(
                    scenario,
                    request.RequestId));
            }

            return Task.FromResult(BridgeResponse.Success(
                request.RequestId,
                new { connected = true }));
        }, cts.Token);

        try
        {
            var client = CreateClient(pipe);
            var error = await Assert.ThrowsAsync<BridgeCallException>(() =>
                client.InvokeAsync<object>("connection.health", new { }, cts.Token));

            Assert.Equal("bridge_internal_error", error.Code);
            await AssertHealthyAsync(client, cts.Token);
        }
        finally
        {
            await StopServerAsync(cts, run);
        }
    }

    [Fact]
    public async Task Missing_pipe_returns_arcgis_not_connected()
    {
        using var cts = new CancellationTokenSource(TestTimeout);
        var client = new NamedPipeBridgeClient(
            NewPipeName(),
            () => "token",
            TimeSpan.FromMilliseconds(100));

        var error = await Assert.ThrowsAsync<BridgeCallException>(() =>
            client.InvokeAsync<object>("connection.health", new { }, cts.Token));

        Assert.Equal("arcgis_not_connected", error.Code);
    }

    [Fact]
    public async Task Silent_server_returns_bridge_timeout()
    {
        var pipe = NewPipeName();
        using var testCts = new CancellationTokenSource(TestTimeout);
        using var serverCts = new CancellationTokenSource(TestTimeout);
        var requestReceived = NewSignal();
        var serverRun = RunSilentServerAsync(pipe, requestReceived, serverCts.Token);
        var client = new NamedPipeBridgeClient(
            pipe,
            () => "token",
            TimeSpan.FromSeconds(1));

        try
        {
            var error = await Assert.ThrowsAsync<BridgeCallException>(() =>
                client.InvokeAsync<object>("connection.health", new { }, testCts.Token));

            Assert.Equal("bridge_timeout", error.Code);
            Assert.True(await requestReceived.Task.WaitAsync(testCts.Token));
        }
        finally
        {
            serverCts.Cancel();
            await Assert.ThrowsAnyAsync<OperationCanceledException>(() =>
                serverRun.WaitAsync(TestTimeout));
        }
    }

    [Fact]
    public async Task Caller_cancellation_is_not_mapped_to_bridge_timeout()
    {
        var pipe = NewPipeName();
        using var testCts = new CancellationTokenSource(TestTimeout);
        using var serverCts = new CancellationTokenSource(TestTimeout);
        using var callerCts = new CancellationTokenSource();
        var requestReceived = NewSignal();
        var serverRun = RunSilentServerAsync(pipe, requestReceived, serverCts.Token);
        var client = new NamedPipeBridgeClient(
            pipe,
            () => "token",
            TimeSpan.FromSeconds(5));
        var invoke = client.InvokeAsync<object>(
            "connection.health",
            new { },
            callerCts.Token);

        try
        {
            Assert.True(await requestReceived.Task.WaitAsync(testCts.Token));
            callerCts.Cancel();

            await Assert.ThrowsAnyAsync<OperationCanceledException>(() => invoke);
        }
        finally
        {
            serverCts.Cancel();
            await Assert.ThrowsAnyAsync<OperationCanceledException>(() =>
                serverRun.WaitAsync(TestTimeout));
        }
    }

    [Theory]
    [InlineData("protocol-version", "protocol_mismatch")]
    [InlineData("request-id", "invalid_response")]
    public async Task Client_rejects_mismatched_response_identity(
        string scenario,
        string expectedCode)
    {
        var pipe = NewPipeName();
        using var cts = new CancellationTokenSource(TestTimeout);
        var serverRun = RunSingleResponseServerAsync(pipe, scenario, cts.Token);
        var client = CreateClient(pipe);

        var error = await Assert.ThrowsAsync<BridgeCallException>(() =>
            client.InvokeAsync<object>("connection.health", new { }, cts.Token));

        Assert.Equal(expectedCode, error.Code);
        await serverRun.WaitAsync(TestTimeout);
    }

    private static string NewPipeName() =>
        $"ArcGISProAgent.Tests.{Guid.NewGuid():N}";

    private static NamedPipeBridgeClient CreateClient(
        string pipe,
        string token = "token") =>
        new(pipe, () => token, TimeSpan.FromSeconds(2));

    private static BridgeRequest CreateRequest(
        string protocolVersion = BridgeProtocol.Current) =>
        new(
            protocolVersion,
            Guid.NewGuid().ToString("N"),
            "connection.health",
            "token",
            JsonSerializer.SerializeToElement(new { }, BridgeJson.Options));

    private static Task<BridgeResponse> HealthHandler(
        BridgeRequest request,
        CancellationToken _) =>
        Task.FromResult(BridgeResponse.Success(
            request.RequestId,
            new { connected = true }));

    private static async Task AssertHealthyAsync(
        NamedPipeBridgeClient client,
        CancellationToken ct)
    {
        var health = await client.InvokeAsync<Dictionary<string, bool>>(
            "connection.health",
            new { },
            ct);

        Assert.True(health["connected"]);
    }

    private static async Task StopServerAsync(
        CancellationTokenSource cts,
        Task run)
    {
        cts.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() =>
            run.WaitAsync(TestTimeout));
    }

    private static byte[] CreateInvalidLine(string scenario) =>
        scenario switch
        {
            "malformed" => "{"u8.ToArray(),
            "too-large" => Enumerable.Repeat(
                (byte)'x',
                (1024 * 1024) + 1).ToArray(),
            "invalid-utf8" => [0xc3, 0x28],
            _ => throw new ArgumentOutOfRangeException(nameof(scenario))
        };

    private static BridgeResponse CreateInvalidHandlerResponse(
        string scenario,
        string requestId)
    {
        if (scenario == "null")
        {
            return null!;
        }

        if (scenario == "disposed-json")
        {
            var document = JsonDocument.Parse("{\"connected\":true}");
            var disposedElement = document.RootElement;
            document.Dispose();
            return new BridgeResponse(
                BridgeProtocol.Current,
                requestId,
                true,
                disposedElement,
                null);
        }

        if (scenario == "undefined-json")
        {
            return new BridgeResponse(
                BridgeProtocol.Current,
                requestId,
                true,
                (JsonElement?)default(JsonElement),
                null);
        }

        throw new ArgumentOutOfRangeException(nameof(scenario));
    }

    private static async Task<BridgeResponse> SendRawRequestAsync(
        string pipe,
        BridgeRequest request,
        CancellationToken ct) =>
        await SendRawLineAsync(
            pipe,
            JsonSerializer.SerializeToUtf8Bytes(request, BridgeJson.Options),
            ct);

    private static async Task<BridgeResponse> SendRawLineAsync(
        string pipe,
        byte[] line,
        CancellationToken ct)
    {
        await using var client = CreateRawClient(pipe);
        await client.ConnectAsync(2000, ct);
        for (var offset = 0; offset < line.Length; offset += 4096)
        {
            var count = Math.Min(4096, line.Length - offset);
            await client.WriteAsync(line.AsMemory(offset, count), ct);
        }

        if (line.Length <= 1024 * 1024)
        {
            await client.WriteAsync("\n"u8.ToArray(), ct);
        }

        await client.FlushAsync(ct);

        var responseLine = await ReadLineAsync(client, ct);
        return JsonSerializer.Deserialize<BridgeResponse>(
                responseLine,
                BridgeJson.Options)
            ?? throw new InvalidDataException("Server returned an empty response.");
    }

    private static async Task RunSilentServerAsync(
        string pipe,
        TaskCompletionSource<bool> requestReceived,
        CancellationToken ct)
    {
        await using var server = CreateRawServer(pipe);
        await server.WaitForConnectionAsync(ct);
        await ReadLineAsync(server, ct);
        requestReceived.TrySetResult(true);
        await Task.Delay(Timeout.InfiniteTimeSpan, ct);
    }

    private static async Task RunSingleResponseServerAsync(
        string pipe,
        string scenario,
        CancellationToken ct)
    {
        await using var server = CreateRawServer(pipe);
        await server.WaitForConnectionAsync(ct);
        var requestLine = await ReadLineAsync(server, ct);
        var request = JsonSerializer.Deserialize<BridgeRequest>(
                requestLine,
                BridgeJson.Options)
            ?? throw new InvalidDataException("Client returned an empty request.");
        var data = JsonSerializer.SerializeToElement(
            new { connected = true },
            BridgeJson.Options);
        var response = scenario switch
        {
            "protocol-version" => new BridgeResponse(
                "0.9",
                request.RequestId,
                true,
                data,
                null),
            "request-id" => new BridgeResponse(
                BridgeProtocol.Current,
                "other-request",
                true,
                data,
                null),
            _ => throw new ArgumentOutOfRangeException(nameof(scenario))
        };

        await WriteLineAsync(
            server,
            JsonSerializer.Serialize(response, BridgeJson.Options),
            ct);
    }

    private static NamedPipeClientStream CreateRawClient(string pipe) =>
        new(
            ".",
            pipe,
            PipeDirection.InOut,
            PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly);

    private static NamedPipeServerStream CreateRawServer(string pipe) =>
        new(
            pipe,
            PipeDirection.InOut,
            1,
            PipeTransmissionMode.Byte,
            PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly);

    private static async Task<string> ReadLineAsync(
        Stream stream,
        CancellationToken ct)
    {
        using var reader = new StreamReader(
            stream,
            new UTF8Encoding(false, true),
            detectEncodingFromByteOrderMarks: false,
            bufferSize: 1024,
            leaveOpen: true);
        return await reader.ReadLineAsync(ct)
            ?? throw new InvalidDataException("JSONL stream ended before a newline.");
    }

    private static async Task WriteLineAsync(
        Stream stream,
        string json,
        CancellationToken ct)
    {
        var payload = Encoding.UTF8.GetBytes(json);
        await stream.WriteAsync(payload, ct);
        await stream.WriteAsync("\n"u8.ToArray(), ct);
        await stream.FlushAsync(ct);
    }

    private static TaskCompletionSource<bool> NewSignal() =>
        new(TaskCreationOptions.RunContinuationsAsynchronously);
}

public sealed class RuntimeCredentialsTests
{
    [Theory]
    [InlineData(null, "token")]
    [InlineData("", "token")]
    [InlineData(" ", "token")]
    [InlineData("pipe", null)]
    [InlineData("pipe", "")]
    [InlineData("pipe", " ")]
    public void Load_rejects_blank_pipe_name_or_token(
        string? pipeName,
        string? token)
    {
        var path = Path.Combine(
            Path.GetTempPath(),
            $"ArcGISProAgent.Tests.{Guid.NewGuid():N}.json");

        try
        {
            File.WriteAllText(
                path,
                JsonSerializer.Serialize(
                    new { pipeName, token },
                    BridgeJson.Options));

            Assert.Throws<InvalidDataException>(() => RuntimeCredentials.Load(path));
        }
        finally
        {
            File.Delete(path);
        }
    }
}
