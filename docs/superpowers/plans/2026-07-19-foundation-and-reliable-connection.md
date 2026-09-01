# ArcGIS Pro Agent Foundation and Reliable Connection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first independently testable product slice: a Tauri desktop app that uses the installed Codex App Server for ChatGPT subscription login, exposes a safe three-pane shell, and reports a versioned, authenticated connection through the .NET MCP server to the ArcGIS Pro 3.7 Add-In.

**Architecture:** The React UI calls typed Tauri commands only. Rust owns Codex App Server process lifecycle and normalizes its JSONL protocol; Codex starts the .NET MCP server, which talks through a current-user-only named pipe to the ArcGIS Pro Add-In. Shared .NET contracts define requests, responses, errors, versions, capabilities, and health snapshots.

**Tech Stack:** Windows; ArcGIS Pro 3.7; .NET 8 for MCP/shared libraries; .NET 10 for the ArcGIS Pro 3.7 Add-In; ModelContextProtocol 1.4.1; Tauri 2.11; React 19.2.7; TypeScript 7.0.2; Vite 8.1.5; Vitest 4.1.10; Rust stable 1.96; Codex CLI/App Server 0.144.5.

## Global Constraints

- First release is Windows local-only, single-user, and controls one ArcGIS Pro instance and one active project at a time.
- Authentication is ChatGPT subscription browser login through Codex App Server; the application must not accept API keys or copy OpenAI credentials.
- Codex App Server starts with a read-only sandbox, approval policy `never`, an empty application-owned working directory, and ArcGIS-only developer instructions; the UI never exposes shell or patch approvals.
- R0 read operations are automatic; R1 temporary navigation/selection is automatic but configurable; R2 creates/exports require target and parameter confirmation; R3 edits/overwrite/delete require confirmation every time and backup when possible.
- No arbitrary script, shell, unknown geoprocessing tool, or system-command MCP tool is registered.
- The named pipe uses `PipeOptions.CurrentUserOnly`; every request also carries a startup token read from `%LOCALAPPDATA%\ArcGISProAgent\runtime\bridge.json`.
- The protocol version for this slice is exactly `1.0`; incompatible clients receive `protocol_mismatch` and do not execute an operation.
- ArcGIS SDK references must resolve from `ArcGISProInstallDir`/`ARCGIS_PRO_HOME` or a detected install location; no build assumes ArcGIS Pro is on `C:`.
- Application logs, runtime state, and non-sensitive configuration live below `%LOCALAPPDATA%\ArcGISProAgent`; GIS project/data files are never installation-owned.
- All implementation steps follow TDD, use `apply_patch` for file edits, run the named verification command, and commit only the files listed by that task.

## Subproject Boundaries

This plan implements design-spec Phase 1 only. The remaining independently testable plans will be written and executed in this order after this plan passes its acceptance gate:

1. Read-only ArcGIS context, layer tree, query, selection, and map navigation.
2. Geoprocessing, output management, symbology, labeling, layout, and export.
3. Editing transactions, snapshot validation, backup, undo, and destructive-operation approvals.
4. Maintenance UI, installer/repair/uninstaller, diagnostics, packaging, and full ArcGIS Pro acceptance testing.

The approved design maps to this plan as follows: architecture and versioned contracts are Tasks 1-4; the three-pane UI is Task 5; official ChatGPT authentication and Codex lifecycle are Tasks 6-7; connection security and error recovery are Tasks 2, 6, and 7; local paths, maintainability, installation ownership, documentation, and the Phase 1 test matrix are Task 8. GIS capabilities beyond connection health remain outside this independently testable slice and are assigned to the four named plans above.

## Target File Map

```text
MCP-Server-ArcGIS-Pro-AddIn/
├─ apps/desktop/
│  ├─ package.json                         # locked frontend commands and dependencies
│  ├─ package-lock.json                    # exact npm dependency graph
│  ├─ vite.config.ts                       # Vite/Vitest configuration
│  ├─ src/
│  │  ├─ App.tsx                           # top-level login/app routing
│  │  ├─ app.css                           # approved three-pane visual system
│  │  ├─ domain.ts                         # normalized UI-facing types
│  │  ├─ desktopApi.ts                     # typed Tauri invoke/listen boundary
│  │  ├─ appStore.ts                       # reducer and immutable UI state
│  │  └─ components/
│  │     ├─ LoginView.tsx                  # ChatGPT login status and browser action
│  │     ├─ Sidebar.tsx                    # sessions and bridge health
│  │     ├─ ConversationPane.tsx           # empty conversation shell for this slice
│  │     └─ ArcGisContextPane.tsx          # connection/version snapshot
│  ├─ tests/                               # Vitest component/store tests
│  └─ src-tauri/
│     ├─ Cargo.toml                        # Tauri/Rust dependencies
│     ├─ tauri.conf.json                   # app identity/window/bundle config
│     ├─ capabilities/default.json         # minimum Tauri permissions
│     └─ src/
│        ├─ lib.rs                         # Tauri setup and managed state
│        ├─ main.rs                        # Windows entry point
│        ├─ commands.rs                    # frontend command surface
│        ├─ paths.rs                       # application-owned paths
│        ├─ runtime_secret.rs              # startup token generation/write
│        └─ codex/
│           ├─ mod.rs                      # normalized runtime interface
│           ├─ protocol.rs                 # minimal app-server wire types
│           └─ client.rs                   # JSONL process client and request routing
├─ src/
│  ├─ ArcGISProAgent.Contracts/            # protocol DTOs and constants
│  ├─ ArcGISProAgent.Bridge/               # pipe client/server and credential loading
│  ├─ ArcGISProAgent.Mcp/                  # MCP host and R0 connection tools
│  └─ ArcGISProAgent.AddIn/                # ArcGIS SDK dispatcher and module lifecycle
├─ tests/
│  ├─ ArcGISProAgent.Contracts.Tests/
│  ├─ ArcGISProAgent.Bridge.Tests/
│  └─ ArcGISProAgent.Mcp.Tests/
├─ scripts/
│  ├─ Resolve-ArcGISProInstall.ps1         # deterministic SDK discovery
│  ├─ Install-Dev.ps1                      # development add-in/app configuration
│  └─ Test-Foundation.ps1                  # aggregate non-GUI verification
├─ docs/development/foundation.md          # build/run/diagnostic instructions
└─ McpServer.sln                           # all .NET production and test projects
```

---

### Task 1: Versioned Shared Contracts

**Files:**
- Create: `src/ArcGISProAgent.Contracts/ArcGISProAgent.Contracts.csproj`
- Create: `src/ArcGISProAgent.Contracts/BridgeProtocol.cs`
- Create: `src/ArcGISProAgent.Contracts/BridgeMessages.cs`
- Create: `src/ArcGISProAgent.Contracts/Capabilities.cs`
- Create: `tests/ArcGISProAgent.Contracts.Tests/ArcGISProAgent.Contracts.Tests.csproj`
- Create: `tests/ArcGISProAgent.Contracts.Tests/BridgeProtocolTests.cs`
- Modify: `McpServer.sln`

**Interfaces:**
- Consumes: JSON serialization from `System.Text.Json` in .NET 8.
- Produces: `BridgeProtocol.Current`, `BridgeRequest`, `BridgeResponse`, `BridgeError`, `BridgeHealth`, `CapabilityManifest`, `CapabilityDescriptor`, and `RiskLevel`.

- [ ] **Step 1: Write the failing contract tests**

```csharp
using System.Text.Json;
using ArcGISProAgent.Contracts;

namespace ArcGISProAgent.Contracts.Tests;

public sealed class BridgeProtocolTests
{
    [Fact]
    public void Request_round_trips_without_losing_operation_identity()
    {
        var request = BridgeRequest.Create(
            operation: "connection.health",
            authToken: "secret",
            arguments: new { includeCapabilities = true },
            requestId: "op-123");

        var json = JsonSerializer.Serialize(request, BridgeJson.Options);
        var restored = JsonSerializer.Deserialize<BridgeRequest>(json, BridgeJson.Options)!;

        Assert.Equal("1.0", restored.ProtocolVersion);
        Assert.Equal("op-123", restored.RequestId);
        Assert.Equal("connection.health", restored.Operation);
        Assert.True(restored.Arguments.GetProperty("includeCapabilities").GetBoolean());
    }

    [Fact]
    public void Failure_contains_stable_code_and_no_success_data()
    {
        var response = BridgeResponse.Failure(
            "op-123", "protocol_mismatch", "Expected protocol 1.0");

        Assert.False(response.Ok);
        Assert.Null(response.Data);
        Assert.Equal("protocol_mismatch", response.Error!.Code);
    }
}
```

- [ ] **Step 2: Run the test to prove the contracts do not exist**

Run: `dotnet test tests/ArcGISProAgent.Contracts.Tests/ArcGISProAgent.Contracts.Tests.csproj`

Expected: FAIL because the test project or `ArcGISProAgent.Contracts` types do not exist.

- [ ] **Step 3: Add the contract project and records**

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
    <TreatWarningsAsErrors>true</TreatWarningsAsErrors>
  </PropertyGroup>
</Project>
```

```csharp
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
```

```csharp
using System.Text.Json;

namespace ArcGISProAgent.Contracts;

public sealed record BridgeRequest(
    string ProtocolVersion,
    string RequestId,
    string Operation,
    string AuthToken,
    JsonElement Arguments)
{
    public static BridgeRequest Create(
        string operation,
        string authToken,
        object? arguments = null,
        string? requestId = null) =>
        new(
            BridgeProtocol.Current,
            requestId ?? Guid.NewGuid().ToString("N"),
            operation,
            authToken,
            JsonSerializer.SerializeToElement(arguments ?? new { }, BridgeJson.Options));
}

public sealed record BridgeError(string Code, string Message, string? Detail = null);

public sealed record BridgeResponse(
    string ProtocolVersion,
    string RequestId,
    bool Ok,
    JsonElement? Data,
    BridgeError? Error)
{
    public static BridgeResponse Success(string requestId, object data) =>
        new(BridgeProtocol.Current, requestId, true,
            JsonSerializer.SerializeToElement(data, BridgeJson.Options), null);

    public static BridgeResponse Failure(
        string requestId, string code, string message, string? detail = null) =>
        new(BridgeProtocol.Current, requestId, false, null,
            new BridgeError(code, message, detail));
}
```

```csharp
namespace ArcGISProAgent.Contracts;

public enum RiskLevel { R0, R1, R2, R3 }

public sealed record CapabilityDescriptor(
    string Id,
    string Version,
    RiskLevel Risk,
    bool SupportsCancellation,
    bool SupportsPreview,
    bool SupportsUndo,
    bool SupportsBackup);

public sealed record CapabilityManifest(
    string ProtocolVersion,
    string AddInVersion,
    string ArcGisProVersion,
    IReadOnlyList<CapabilityDescriptor> Capabilities);

public sealed record BridgeHealth(
    bool Connected,
    string ProtocolVersion,
    string AddInVersion,
    string ArcGisProVersion,
    string? ProjectName,
    string? ActiveMapName,
    IReadOnlyList<CapabilityDescriptor> Capabilities);
```

- [ ] **Step 4: Add the production and test projects to `McpServer.sln`**

Use `apply_patch` to add SDK-style C# project entries and Debug/Release configuration mappings. The test project references the contract project and pins:

```xml
<ItemGroup>
  <PackageReference Include="Microsoft.NET.Test.Sdk" Version="17.14.1" />
  <PackageReference Include="xunit" Version="2.9.3" />
  <PackageReference Include="xunit.runner.visualstudio" Version="3.1.4" />
  <ProjectReference Include="..\..\src\ArcGISProAgent.Contracts\ArcGISProAgent.Contracts.csproj" />
</ItemGroup>
```

- [ ] **Step 5: Run contract tests**

Run: `dotnet test tests/ArcGISProAgent.Contracts.Tests/ArcGISProAgent.Contracts.Tests.csproj --no-restore`

Expected: PASS, 2 tests.

- [ ] **Step 6: Commit the contract boundary**

```powershell
git add McpServer.sln src/ArcGISProAgent.Contracts tests/ArcGISProAgent.Contracts.Tests
git commit -m "feat: add versioned ArcGIS bridge contracts"
```

---

### Task 2: Authenticated Current-User Named-Pipe Transport

**Files:**
- Create: `src/ArcGISProAgent.Bridge/ArcGISProAgent.Bridge.csproj`
- Create: `src/ArcGISProAgent.Bridge/RuntimeCredentials.cs`
- Create: `src/ArcGISProAgent.Bridge/IBridgeClient.cs`
- Create: `src/ArcGISProAgent.Bridge/NamedPipeBridgeClient.cs`
- Create: `src/ArcGISProAgent.Bridge/NamedPipeBridgeServer.cs`
- Create: `tests/ArcGISProAgent.Bridge.Tests/ArcGISProAgent.Bridge.Tests.csproj`
- Create: `tests/ArcGISProAgent.Bridge.Tests/NamedPipeBridgeTests.cs`
- Modify: `McpServer.sln`

**Interfaces:**
- Consumes: Task 1 `BridgeRequest`, `BridgeResponse`, `BridgeJson`, and `BridgeProtocol.Current`.
- Produces: `RuntimeCredentials.Load(path)`, `IBridgeClient.InvokeAsync<T>()`, and `NamedPipeBridgeServer.RunAsync(handler, ct)`.

- [ ] **Step 1: Write failing pipe authentication and protocol tests**

```csharp
using ArcGISProAgent.Bridge;
using ArcGISProAgent.Contracts;

namespace ArcGISProAgent.Bridge.Tests;

public sealed class NamedPipeBridgeTests
{
    [Fact]
    public async Task Valid_token_returns_health_response()
    {
        var pipe = $"ArcGISProAgent.Tests.{Guid.NewGuid():N}";
        using var cts = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        var server = new NamedPipeBridgeServer(pipe, () => "token");
        var run = server.RunAsync(
            (request, _) => Task.FromResult(BridgeResponse.Success(
                request.RequestId, new { connected = true })), cts.Token);
        var client = new NamedPipeBridgeClient(pipe, () => "token", TimeSpan.FromSeconds(2));

        var health = await client.InvokeAsync<Dictionary<string, bool>>(
            "connection.health", new { }, cts.Token);

        Assert.True(health["connected"]);
        cts.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => run);
    }

    [Fact]
    public async Task Invalid_token_is_rejected_before_handler_runs()
    {
        var pipe = $"ArcGISProAgent.Tests.{Guid.NewGuid():N}";
        using var cts = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        var called = false;
        var server = new NamedPipeBridgeServer(pipe, () => "server-token");
        var run = server.RunAsync((request, _) =>
        {
            called = true;
            return Task.FromResult(BridgeResponse.Success(request.RequestId, new { }));
        }, cts.Token);
        var client = new NamedPipeBridgeClient(pipe, () => "wrong-token", TimeSpan.FromSeconds(2));

        var error = await Assert.ThrowsAsync<BridgeCallException>(() =>
            client.InvokeAsync<object>("connection.health", new { }, cts.Token));

        Assert.Equal("unauthorized", error.Code);
        Assert.False(called);
        cts.Cancel();
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `dotnet test tests/ArcGISProAgent.Bridge.Tests/ArcGISProAgent.Bridge.Tests.csproj`

Expected: FAIL because the bridge project and named-pipe types do not exist.

- [ ] **Step 3: Implement credential loading and the bridge client interface**

```csharp
namespace ArcGISProAgent.Bridge;

public sealed record RuntimeCredentials(string PipeName, string Token)
{
    public static RuntimeCredentials Load(string path)
    {
        var json = File.ReadAllText(path);
        return System.Text.Json.JsonSerializer.Deserialize<RuntimeCredentials>(json, BridgeJson.Options)
            ?? throw new InvalidDataException("Runtime credential file is invalid.");
    }
}

public interface IBridgeClient
{
    Task<T> InvokeAsync<T>(string operation, object? arguments, CancellationToken ct);
}

public sealed class BridgeCallException(string code, string message) : Exception(message)
{
    public string Code { get; } = code;
}
```

Implement `NamedPipeBridgeClient` with one request per connection, UTF-8 JSONL, `PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly`, a linked timeout token, exact response ID/version validation, and deserialization through `BridgeJson.Options`. On pipe absence throw `BridgeCallException("arcgis_not_connected", ...)`; on timeout throw `BridgeCallException("bridge_timeout", ...)`.

- [ ] **Step 4: Implement the pipe server authorization gate**

```csharp
public async Task RunAsync(
    Func<BridgeRequest, CancellationToken, Task<BridgeResponse>> handler,
    CancellationToken ct)
{
    while (!ct.IsCancellationRequested)
    {
        await using var server = new NamedPipeServerStream(
            _pipeName, PipeDirection.InOut, 1, PipeTransmissionMode.Byte,
            PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly);
        await server.WaitForConnectionAsync(ct);
        var request = await ReadRequestAsync(server, ct);
        var response = request.ProtocolVersion != BridgeProtocol.Current
            ? BridgeResponse.Failure(request.RequestId, "protocol_mismatch",
                $"Expected protocol {BridgeProtocol.Current}")
            : !CryptographicOperations.FixedTimeEquals(
                Encoding.UTF8.GetBytes(request.AuthToken),
                Encoding.UTF8.GetBytes(_tokenProvider()))
                ? BridgeResponse.Failure(request.RequestId, "unauthorized", "Bridge token rejected")
                : await handler(request, ct);
        await WriteResponseAsync(server, response, ct);
    }
}
```

`ReadRequestAsync` rejects lines larger than 1 MiB with `request_too_large`; `WriteResponseAsync` appends exactly one newline. The server catches per-connection parse/handler exceptions and returns `invalid_request` or `bridge_internal_error` without terminating its accept loop. If the runtime credential file does not exist yet, token-provider failure maps to `runtime_not_ready` and the accept loop remains available, which allows ArcGIS Pro to start before the desktop app.

- [ ] **Step 5: Run bridge tests and the solution build**

Run: `dotnet test tests/ArcGISProAgent.Bridge.Tests/ArcGISProAgent.Bridge.Tests.csproj`

Expected: PASS, 2 tests.

Run: `dotnet build src/ArcGISProAgent.Bridge/ArcGISProAgent.Bridge.csproj --no-restore`

Expected: Build succeeded with 0 warnings and 0 errors.

- [ ] **Step 6: Commit the transport**

```powershell
git add McpServer.sln src/ArcGISProAgent.Bridge tests/ArcGISProAgent.Bridge.Tests
git commit -m "feat: secure the local ArcGIS named-pipe bridge"
```

---

### Task 3: Stable MCP Host and R0 Connection Tools

**Files:**
- Create: `src/ArcGISProAgent.Mcp/ArcGISProAgent.Mcp.csproj`
- Create: `src/ArcGISProAgent.Mcp/Program.cs`
- Create: `src/ArcGISProAgent.Mcp/BridgeClientFactory.cs`
- Create: `src/ArcGISProAgent.Mcp/Tools/ConnectionTools.cs`
- Create: `tests/ArcGISProAgent.Mcp.Tests/ArcGISProAgent.Mcp.Tests.csproj`
- Create: `tests/ArcGISProAgent.Mcp.Tests/ConnectionToolsTests.cs`
- Modify: `.mcp.json`
- Modify: `McpServer.sln`

**Interfaces:**
- Consumes: Task 2 `IBridgeClient`, `NamedPipeBridgeClient`, and `RuntimeCredentials`.
- Produces: MCP tools `arcgis_connection_status` and `arcgis_capabilities`; environment variable `ARCGIS_AGENT_RUNTIME_FILE` selects the runtime credential file.

- [ ] **Step 1: Write the failing MCP tool unit tests**

```csharp
using ArcGISProAgent.Bridge;
using ArcGISProAgent.Contracts;
using ArcGISProAgent.Mcp.Tools;

namespace ArcGISProAgent.Mcp.Tests;

public sealed class ConnectionToolsTests
{
    [Fact]
    public async Task Status_returns_structured_bridge_health()
    {
        var expected = new BridgeHealth(true, "1.0", "0.1.0", "3.7", "Demo", "Map", []);
        var tools = new ConnectionTools(new StubBridgeClient(expected));

        var actual = await tools.StatusAsync(CancellationToken.None);

        Assert.Equal(expected, actual);
    }

    private sealed class StubBridgeClient(BridgeHealth value) : IBridgeClient
    {
        public Task<T> InvokeAsync<T>(string operation, object? arguments, CancellationToken ct)
        {
            Assert.Equal("connection.health", operation);
            return Task.FromResult((T)(object)value);
        }
    }
}
```

- [ ] **Step 2: Run the MCP tests to verify failure**

Run: `dotnet test tests/ArcGISProAgent.Mcp.Tests/ArcGISProAgent.Mcp.Tests.csproj`

Expected: FAIL because `ArcGISProAgent.Mcp` and `ConnectionTools` do not exist.

- [ ] **Step 3: Create the MCP host pinned to the stable SDK**

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
    <TreatWarningsAsErrors>true</TreatWarningsAsErrors>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Microsoft.Extensions.Hosting" Version="8.0.1" />
    <PackageReference Include="ModelContextProtocol" Version="1.4.1" />
    <ProjectReference Include="..\ArcGISProAgent.Bridge\ArcGISProAgent.Bridge.csproj" />
    <ProjectReference Include="..\ArcGISProAgent.Contracts\ArcGISProAgent.Contracts.csproj" />
  </ItemGroup>
</Project>
```

```csharp
using ArcGISProAgent.Bridge;
using ArcGISProAgent.Mcp.Tools;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;

var builder = Host.CreateApplicationBuilder(args);
builder.Logging.ClearProviders(); // stdout is reserved for MCP JSON-RPC
builder.Logging.AddConsole(options => options.LogToStandardErrorThreshold = LogLevel.Trace);
builder.Services.AddSingleton<IBridgeClient>(_ => BridgeClientFactory.Create());
builder.Services.AddMcpServer(options =>
    {
        options.ServerInfo = new() { Name = "arcgis-pro-agent", Version = "0.1.0" };
    })
    .WithStdioServerTransport()
    .WithTools<ConnectionTools>();
await builder.Build().RunAsync();
```

- [ ] **Step 4: Implement only the declared R0 tools**

```csharp
using ArcGISProAgent.Bridge;
using ArcGISProAgent.Contracts;
using ModelContextProtocol.Server;
using System.ComponentModel;

namespace ArcGISProAgent.Mcp.Tools;

[McpServerToolType]
public sealed class ConnectionTools(IBridgeClient bridge)
{
    [McpServerTool(Name = "arcgis_connection_status", ReadOnly = true, Idempotent = true)]
    [Description("Return the live ArcGIS Pro Add-In connection, version, project, map, and capability status.")]
    public Task<BridgeHealth> StatusAsync(CancellationToken cancellationToken) =>
        bridge.InvokeAsync<BridgeHealth>("connection.health", new { }, cancellationToken);

    [McpServerTool(Name = "arcgis_capabilities", ReadOnly = true, Idempotent = true)]
    [Description("List the ArcGIS Pro operations currently supported by the connected Add-In.")]
    public async Task<IReadOnlyList<CapabilityDescriptor>> CapabilitiesAsync(
        CancellationToken cancellationToken) =>
        (await StatusAsync(cancellationToken)).Capabilities;
}
```

`BridgeClientFactory.Create()` must require `ARCGIS_AGENT_RUNTIME_FILE`, load `RuntimeCredentials`, and construct `NamedPipeBridgeClient` with a five-second timeout. It never falls back to an unauthenticated fixed pipe.

- [ ] **Step 5: Point the developer MCP manifest to the new host**

```json
{
  "inputs": [
    {
      "id": "arcgisRuntimeFile",
      "type": "promptString",
      "description": "Absolute path to %LOCALAPPDATA%\\ArcGISProAgent\\runtime\\bridge.json"
    }
  ],
  "servers": {
    "arcgis": {
      "type": "stdio",
      "command": "dotnet",
      "args": ["run", "--project", "src/ArcGISProAgent.Mcp/ArcGISProAgent.Mcp.csproj"],
      "env": {
        "ARCGIS_AGENT_RUNTIME_FILE": "${input:arcgisRuntimeFile}"
      }
    }
  }
}
```

If this client does not support input interpolation, the development documentation instructs launching through the desktop app; it must not commit a token or user-specific absolute runtime path.

- [ ] **Step 6: Run tests and verify stdout remains protocol-only**

Run: `dotnet test tests/ArcGISProAgent.Mcp.Tests/ArcGISProAgent.Mcp.Tests.csproj`

Expected: PASS, 1 test.

Run: `dotnet build src/ArcGISProAgent.Mcp/ArcGISProAgent.Mcp.csproj --no-restore`

Expected: Build succeeded with 0 warnings and 0 errors. Inspect `Program.cs` with `rg -n "ClearProviders|LogToStandardErrorThreshold" src/ArcGISProAgent.Mcp/Program.cs`; both guards must be present so logs cannot contaminate MCP stdout.

- [ ] **Step 7: Commit the MCP host**

```powershell
git add .mcp.json McpServer.sln src/ArcGISProAgent.Mcp tests/ArcGISProAgent.Mcp.Tests
git commit -m "feat: expose ArcGIS bridge health through MCP"
```

---

### Task 4: ArcGIS Pro 3.7 Add-In Lifecycle and Health Dispatcher

**Files:**
- Create: `src/ArcGISProAgent.AddIn/ArcGISProAgent.AddIn.csproj`
- Create: `src/ArcGISProAgent.AddIn/Config.daml`
- Create: `src/ArcGISProAgent.AddIn/AgentModule.cs`
- Create: `src/ArcGISProAgent.AddIn/ArcGisOperationDispatcher.cs`
- Create: `src/ArcGISProAgent.AddIn/RuntimeCredentialLocator.cs`
- Reuse: `AddIn/APBridgeAddIn/Images/*`
- Create: `scripts/Resolve-ArcGISProInstall.ps1`
- Modify: `McpServer.sln`

**Interfaces:**
- Consumes: Task 2 `NamedPipeBridgeServer` and Task 1 contracts.
- Produces: auto-started Add-In pipe service and `connection.health` response populated from ArcGIS Pro SDK.

- [ ] **Step 1: Write the SDK path resolver smoke assertion before changing the project**

Run:

```powershell
$resolved = & scripts/Resolve-ArcGISProInstall.ps1 -Candidate 'D:\arcgis_pro'
if ($resolved -ne 'D:\arcgis_pro') { throw "Unexpected ArcGIS path: $resolved" }
```

Expected: FAIL because `Resolve-ArcGISProInstall.ps1` does not exist.

- [ ] **Step 2: Implement deterministic SDK path resolution**

```powershell
param([string]$Candidate)

$locations = @(
    $Candidate,
    $env:ARCGIS_PRO_HOME,
    (Get-ItemProperty -Path 'HKLM:\SOFTWARE\ESRI\ArcGISPro' -ErrorAction SilentlyContinue).InstallDir,
    'C:\Program Files\ArcGIS\Pro'
) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }

foreach ($location in $locations) {
    $full = [IO.Path]::GetFullPath($location).TrimEnd('\')
    if (Test-Path -LiteralPath (Join-Path $full 'bin\ArcGIS.Core.dll')) {
        $full
        exit 0
    }
}

throw 'ArcGIS Pro SDK assemblies were not found. Set ARCGIS_PRO_HOME or pass -Candidate.'
```

Run the Step 1 assertion again. Expected: PASS and output `D:\arcgis_pro`.

- [ ] **Step 3: Create the Add-In project with property-based references**

Use `$(ArcGISProInstallDir)` in every `HintPath` and SDK target import:

```xml
<PropertyGroup>
  <TargetFramework>net10.0-windows</TargetFramework>
  <UseWPF>true</UseWPF>
  <RuntimeIdentifier>win-x64</RuntimeIdentifier>
  <ArcGISProInstallDir Condition="'$(ArcGISProInstallDir)' == '' and '$(ARCGIS_PRO_HOME)' != ''">$(ARCGIS_PRO_HOME)</ArcGISProInstallDir>
  <ArcGISProInstallDir Condition="'$(ArcGISProInstallDir)' == ''">C:\Program Files\ArcGIS\Pro</ArcGISProInstallDir>
</PropertyGroup>
<Target Name="ValidateArcGISProInstall" BeforeTargets="ResolveAssemblyReferences">
  <Error Condition="!Exists('$(ArcGISProInstallDir)\bin\ArcGIS.Core.dll')"
         Text="ArcGIS Pro SDK not found at '$(ArcGISProInstallDir)'. Set ArcGISProInstallDir or ARCGIS_PRO_HOME." />
</Target>
```

The project sets `<Version>0.1.0</Version>`, references `ArcGISProAgent.Contracts` and `ArcGISProAgent.Bridge`, copies the existing icons, and imports `$(ArcGISProInstallDir)\bin\Esri.ProApp.SDK.Desktop.targets`.

- [ ] **Step 4: Auto-start and dispose the bridge in the ArcGIS module**

```csharp
internal sealed class AgentModule : Module
{
    private NamedPipeBridgeServer? _server;
    private CancellationTokenSource? _cts;
    private Task? _runTask;

    protected override bool Initialize()
    {
        _cts = new CancellationTokenSource();
        _server = new NamedPipeBridgeServer(
            BridgeProtocol.DefaultPipeName,
            () => RuntimeCredentials.Load(RuntimeCredentialLocator.GetPath()).Token);
        var dispatcher = new ArcGisOperationDispatcher();
        _runTask = _server.RunAsync(dispatcher.DispatchAsync, _cts.Token);
        return true;
    }

    protected override bool CanUnload()
    {
        _cts?.Cancel();
        return true;
    }
}
```

The run task must observe and log cancellation/failure through ArcGIS diagnostics without showing modal message boxes. `Config.daml` sets `autoLoad="true"`, `desktopVersion="3.7.0.1901"`, Chinese name `ArcGIS Pro 智能助手桥接`, and one status button whose click only displays current connection/version details.

- [ ] **Step 5: Implement `connection.health` on the correct ArcGIS thread**

```csharp
public Task<BridgeResponse> DispatchAsync(BridgeRequest request, CancellationToken ct) =>
    request.Operation switch
    {
        "connection.health" => QueuedTask.Run(() =>
        {
            var project = Project.Current;
            var map = MapView.Active?.Map;
            var health = new BridgeHealth(
                true,
                BridgeProtocol.Current,
                typeof(AgentModule).Assembly.GetName().Version?.ToString() ?? "0.1.0",
                FileVersionInfo.GetVersionInfo(Environment.ProcessPath!).ProductVersion ?? "unknown",
                project?.Name,
                map?.Name,
                CapabilityCatalog.All);
            return BridgeResponse.Success(request.RequestId, health);
        }, ct),
        _ => Task.FromResult(BridgeResponse.Failure(
            request.RequestId, "operation_not_found", $"Unknown operation: {request.Operation}"))
    };
```

`CapabilityCatalog.All` contains exactly `connection.health` at R0 in this slice.

- [ ] **Step 6: Build on the actual machine path**

Run: `dotnet build src/ArcGISProAgent.AddIn/ArcGISProAgent.AddIn.csproj -p:ArcGISProInstallDir=D:\arcgis_pro`

Expected: Build succeeded with 0 errors and an `.esriAddInX` package produced by the ArcGIS SDK targets.

- [ ] **Step 7: Commit the Add-In foundation**

```powershell
git add McpServer.sln scripts/Resolve-ArcGISProInstall.ps1 src/ArcGISProAgent.AddIn
git commit -m "feat: add the ArcGIS Pro bridge add-in lifecycle"
```

---

### Task 5: Tauri Shell and Three-Pane Connection UI

**Files:**
- Create: `apps/desktop/package.json`
- Create: `apps/desktop/package-lock.json`
- Create: `apps/desktop/index.html`
- Create: `apps/desktop/tsconfig.json`
- Create: `apps/desktop/vite.config.ts`
- Create: `apps/desktop/src/main.tsx`
- Create: `apps/desktop/src/App.tsx`
- Create: `apps/desktop/src/app.css`
- Create: `apps/desktop/src/domain.ts`
- Create: `apps/desktop/src/appStore.ts`
- Create: `apps/desktop/src/desktopApi.ts`
- Create: `apps/desktop/src/components/LoginView.tsx`
- Create: `apps/desktop/src/components/Sidebar.tsx`
- Create: `apps/desktop/src/components/ConversationPane.tsx`
- Create: `apps/desktop/src/components/ArcGisContextPane.tsx`
- Create: `apps/desktop/tests/appStore.test.ts`
- Create: `apps/desktop/tests/App.test.tsx`
- Create: `apps/desktop/src-tauri/Cargo.toml`
- Create: `apps/desktop/src-tauri/tauri.conf.json`
- Create: `apps/desktop/src-tauri/capabilities/default.json`
- Create: `apps/desktop/src-tauri/src/main.rs`
- Create: `apps/desktop/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: no live backend yet; `desktopApi` is mocked in tests.
- Produces: `AppState`, `AppAction`, `DesktopSnapshot`, `AccountState`, `BridgeSnapshot`, and the approved three-pane UI.

- [ ] **Step 1: Write failing reducer and rendering tests**

```ts
import { describe, expect, it } from "vitest";
import { initialState, reduceAppState } from "../src/appStore";

describe("app state", () => {
  it("marks a disconnected snapshot as non-live", () => {
    const state = reduceAppState(initialState, {
      type: "snapshot/received",
      snapshot: {
        account: { status: "signedOut" },
        arcgis: { status: "disconnected", lastUpdated: "2026-07-19T00:00:00Z" },
        codex: { status: "ready", version: "0.144.5" },
      },
    });
    expect(state.arcgis.isLive).toBe(false);
  });
});
```

```tsx
import { render, screen } from "@testing-library/react";
import { vi } from "vitest";
import { App } from "../src/App";

vi.mock("../src/desktopApi", () => ({
  getSnapshot: vi.fn().mockResolvedValue({
    account: { status: "signedOut" },
    arcgis: { status: "disconnected", lastUpdated: null },
    codex: { status: "ready", version: "0.144.5" },
  }),
  startChatGptLogin: vi.fn(),
  subscribeDesktopEvents: vi.fn().mockResolvedValue(() => undefined),
}));

it("shows ChatGPT login without API-key controls", async () => {
  render(<App />);
  expect(await screen.findByRole("button", { name: "使用 ChatGPT 账号登录" })).toBeVisible();
  expect(screen.queryByText(/API Key/i)).toBeNull();
});
```

- [ ] **Step 2: Create the package manifest and prove the tests fail**

```json
{
  "name": "arcgis-pro-agent-desktop",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "test": "vitest run",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tauri-apps/api": "2.11.1",
    "react": "19.2.7",
    "react-dom": "19.2.7"
  },
  "devDependencies": {
    "@tauri-apps/cli": "2.11.4",
    "@testing-library/jest-dom": "6.9.1",
    "@testing-library/react": "16.3.2",
    "@types/react": "19.2.17",
    "@types/react-dom": "19.2.3",
    "@vitejs/plugin-react": "6.0.3",
    "jsdom": "29.1.1",
    "typescript": "7.0.2",
    "vite": "8.1.5",
    "vitest": "4.1.10"
  }
}
```

Run: `cd apps/desktop; npm.cmd install; npm.cmd test`

Expected: FAIL because `App`, `appStore`, and `desktopApi` do not exist. Commit the generated `package-lock.json`, never `node_modules`.

- [ ] **Step 3: Implement normalized domain types and reducer**

```ts
export type AccountState =
  | { status: "checking" }
  | { status: "signedOut" }
  | { status: "loginPending"; loginId: string }
  | { status: "signedIn"; email: string | null; planType: string };

export type BridgeSnapshot = {
  status: "connected" | "disconnected" | "error";
  isLive: boolean;
  protocolVersion?: string;
  addInVersion?: string;
  arcGisProVersion?: string;
  projectName?: string | null;
  activeMapName?: string | null;
  lastUpdated: string | null;
  error?: string;
};

export type DesktopSnapshot = {
  account: AccountState;
  arcgis: Omit<BridgeSnapshot, "isLive">;
  codex: { status: "starting" | "ready" | "error"; version?: string; error?: string };
};
```

`reduceAppState` is a pure exhaustive reducer. It derives `arcgis.isLive` only when status is `connected`, so a stale snapshot cannot appear live.

- [ ] **Step 4: Implement the three-pane shell**

`App.tsx` routes signed-out users to `LoginView`; signed-in users see `Sidebar`, `ConversationPane`, and `ArcGisContextPane`. CSS uses a 264 px left rail, fluid center, 320 px inspector, minimum 1180×720 window, visible keyboard focus, Chinese system-font stack, dark neutral background, and cyan/green status accents from the approved visual. At widths below 980 px the inspector becomes a toggled drawer rather than compressing the conversation to unreadable width.

The conversation pane in this slice renders a welcome message and disabled prompt with text `连接闭环完成后即可发送 ArcGIS 指令`; sending messages is Task 7.

- [ ] **Step 5: Add the minimum Tauri shell**

```toml
[package]
name = "arcgis-pro-agent-desktop"
version = "0.1.0"
edition = "2024"

[lib]
name = "arcgis_pro_agent_desktop_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tauri = { version = "2.11", features = [] }
tokio = { version = "1", features = ["process", "io-util", "sync", "time"] }
```

`tauri.conf.json` uses identifier `com.arcgisproagent.desktop`, product name `ArcGIS Pro 智能助手`, dev URL `http://localhost:1420`, frontend dist `../dist`, and one 1280×800 window. `capabilities/default.json` grants only `core:default`; filesystem, shell, opener, and process plugins are absent.

- [ ] **Step 6: Run frontend and Rust checks**

Run: `cd apps/desktop; npm.cmd test`

Expected: PASS, 2 tests.

Run: `cd apps/desktop; npm.cmd run build`

Expected: TypeScript and Vite build succeed.

Run: `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`

Expected: Rust check succeeds.

- [ ] **Step 7: Commit the desktop shell**

```powershell
git add apps/desktop
git commit -m "feat: add the ArcGIS Pro Agent desktop shell"
```

---

### Task 6: Codex App Server Process and Protocol Adapter

**Files:**
- Create: `apps/desktop/src-tauri/src/paths.rs`
- Create: `apps/desktop/src-tauri/src/runtime_secret.rs`
- Create: `apps/desktop/src-tauri/src/codex/mod.rs`
- Create: `apps/desktop/src-tauri/src/codex/protocol.rs`
- Create: `apps/desktop/src-tauri/src/codex/client.rs`
- Create: `apps/desktop/src-tauri/tests/codex_client.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: installed `codex.cmd` 0.144.5 and the Task 3 MCP executable/project.
- Produces: `CodexRuntime::start`, `request`, `subscribe`, `shutdown`; startup `RuntimeFile`; normalized `CodexEvent` values.

- [ ] **Step 1: Write a fake JSONL server integration test**

```rust
#[tokio::test]
async fn initializes_once_and_routes_responses_by_id() {
    let fake = FakeAppServer::start(vec![
        response(1, json!({"userAgent":"fake","platformFamily":"windows","platformOs":"windows"})),
        response(2, json!({"account":null,"requiresOpenaiAuth":true})),
    ]).await;
    let runtime = CodexRuntime::start_with_command(fake.command(), test_options()).await.unwrap();

    let account = runtime.request("account/read", json!({"refreshToken": false})).await.unwrap();

    assert!(fake.received_method("initialize").await);
    assert!(fake.received_notification("initialized").await);
    assert_eq!(account["account"], Value::Null);
}

#[tokio::test]
async fn malformed_stdout_line_becomes_diagnostic_not_a_crash() {
    let fake = FakeAppServer::with_stdout_lines(["not-json"]).await;
    let runtime = CodexRuntime::start_with_command(fake.command(), test_options()).await.unwrap();
    let event = runtime.next_event().await.unwrap();
    assert!(matches!(event, CodexEvent::ProtocolError { .. }));
}
```

- [ ] **Step 2: Run Rust tests to verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml codex_client`

Expected: FAIL because `CodexRuntime` and the fake-server test support do not exist.

- [ ] **Step 3: Implement application paths and startup credentials**

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeFile {
    pub pipe_name: String,
    pub token: String,
}

pub fn create_runtime_file(local_app_data: &Path) -> Result<PathBuf, RuntimeError> {
    let runtime_dir = local_app_data.join("ArcGISProAgent").join("runtime");
    std::fs::create_dir_all(&runtime_dir)?;
    let value = RuntimeFile {
        pipe_name: "ArcGISProAgent.Bridge.v1".to_owned(),
        token: generate_32_byte_base64url_token(),
    };
    let target = runtime_dir.join("bridge.json");
    atomic_write_current_user_only(&target, &serde_json::to_vec(&value)?)?;
    Ok(target)
}
```

On Windows `atomic_write_current_user_only` creates a temporary file in the same directory, applies an ACL for the current user and SYSTEM, flushes it, then renames it over `bridge.json`. Tests use a temporary directory and assert no token appears in logs or error display values.

- [ ] **Step 4: Implement the JSONL request router**

```rust
#[derive(Debug, Serialize)]
struct WireRequest {
    method: String,
    id: u64,
    params: Value,
}

#[derive(Debug, Clone)]
pub enum CodexEvent {
    Notification { method: String, params: Value },
    ServerRequest { id: Value, method: String, params: Value },
    ProtocolError { message: String },
    ProcessExited { code: Option<i32> },
}
```

`CodexRuntime` owns child stdin/stdout/stderr, an `AtomicU64` request ID, a `HashMap<u64, oneshot::Sender<Result<Value, CodexError>>>`, and a broadcast event channel. It sends:

```json
{"method":"initialize","id":1,"params":{"clientInfo":{"name":"arcgis_pro_agent","title":"ArcGIS Pro Agent","version":"0.1.0"},"capabilities":{"mcpServerOpenaiFormElicitation":true}}}
{"method":"initialized","params":{}}
```

Unknown notifications are preserved as normalized events; malformed lines never satisfy a pending request. Stderr is kept in a capped 200-line diagnostic ring and never parsed as protocol.

- [ ] **Step 5: Launch Codex with an ArcGIS-only MCP configuration**

The production command is created with `std::process::Command`, never a shell string:

```rust
Command::new(codex_cmd)
    .arg("app-server")
    .arg("--stdio")
    .arg("-c")
    .arg(format!("mcp_servers.arcgis.command={}", toml_string(dotnet_path)))
    .arg("-c")
    .arg(format!("mcp_servers.arcgis.args={}", toml_array(mcp_args)))
    .arg("-c")
    .arg(format!("mcp_servers.arcgis.env={}", toml_table([
        ("ARCGIS_AGENT_RUNTIME_FILE", runtime_file.display().to_string())
    ])))
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
```

Thread creation in Task 7 passes `sandbox: "read-only"`, `approvalPolicy: "never"`, `approvalsReviewer: "user"`, and an empty application-owned `cwd`. Developer instructions say the agent may use only tools from MCP server `arcgis`, must not use shell/file-change tools, and must surface R2/R3 elicitation to the user.

- [ ] **Step 6: Run adapter tests and protocol-version smoke check**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`

Expected: PASS, including initialization, response routing, malformed output, child exit, and token-redaction tests.

Run: `codex.cmd app-server generate-json-schema --out .superpowers/app-server-json-schema`

Expected: exit code 0. Verify `account/login/start`, `account/read`, `thread/start`, `turn/start`, `mcpServer/elicitation/request`, `item/mcpToolCall/progress`, and `turn/completed` exist in the generated schema for installed version 0.144.5.

- [ ] **Step 7: Commit the Codex runtime adapter**

```powershell
git add apps/desktop/src-tauri
git commit -m "feat: embed the Codex app-server runtime"
```

---

### Task 7: ChatGPT Login, Conversation Loop, and Live Connection Snapshot

**Files:**
- Create: `apps/desktop/src-tauri/src/commands.rs`
- Create: `apps/desktop/src-tauri/src/app_state.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/capabilities/default.json`
- Modify: `apps/desktop/package.json`
- Modify: `apps/desktop/package-lock.json`
- Modify: `apps/desktop/src/desktopApi.ts`
- Modify: `apps/desktop/src/appStore.ts`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/components/LoginView.tsx`
- Modify: `apps/desktop/src/components/ConversationPane.tsx`
- Modify: `apps/desktop/src/components/ArcGisContextPane.tsx`
- Create: `apps/desktop/tests/loginFlow.test.tsx`
- Create: `apps/desktop/tests/conversationFlow.test.tsx`

**Interfaces:**
- Consumes: Task 6 `CodexRuntime`; Codex methods `account/read`, `account/login/start`, `account/logout`, `thread/start`, `turn/start`, `turn/interrupt`, and `mcpServer/tool/call`.
- Produces: Tauri commands `desktop_snapshot`, `chatgpt_login_start`, `chatgpt_logout`, `conversation_start`, `turn_start`, `turn_interrupt`, and event `desktop://event`.

- [ ] **Step 1: Write failing login and turn-flow tests**

```tsx
it("opens only the official ChatGPT auth URL", async () => {
  api.startChatGptLogin.mockResolvedValue({
    loginId: "login-1",
    authUrl: "https://auth.openai.com/oauth/authorize?...",
  });
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "使用 ChatGPT 账号登录" }));
  expect(api.openExternalUrl).toHaveBeenCalledWith(expect.stringMatching(/^https:\/\/(auth\.openai\.com|chatgpt\.com)\//));
});

it("renders streamed agent text and ArcGIS MCP event cards", async () => {
  render(<App initialSnapshot={signedInConnectedSnapshot} />);
  await user.type(screen.getByRole("textbox"), "检查 ArcGIS 连接");
  await user.click(screen.getByRole("button", { name: "发送" }));
  emit({ type: "item/agentMessage/delta", text: "连接正常" });
  emit({ type: "item/completed", item: { type: "mcpToolCall", server: "arcgis", tool: "arcgis_connection_status", status: "completed" } });
  expect(await screen.findByText("连接正常")).toBeVisible();
  expect(screen.getByText("arcgis_connection_status")).toBeVisible();
});
```

- [ ] **Step 2: Run frontend tests to verify failure**

Run: `cd apps/desktop; npm.cmd test -- loginFlow conversationFlow`

Expected: FAIL because login, URL validation, sending, and event rendering are not implemented.

- [ ] **Step 3: Implement Rust account commands without token access**

Add `@tauri-apps/plugin-opener` 2.5.4 and Rust `tauri-plugin-opener = "2"`; register the plugin in `lib.rs`. Grant `opener:allow-open-url` only for `https://auth.openai.com/*`, `https://chatgpt.com/*`, and `https://openai.com/*`; do not grant `opener:default` or any file-opening permission.

`chatgpt_login_start` sends:

```json
{"method":"account/login/start","params":{"type":"chatgpt","codexStreamlinedLogin":true,"useHostedLoginSuccessPage":true,"appBrand":"codex"}}
```

It returns only `{ loginId, authUrl }`, validates that the parsed HTTPS host is in `{auth.openai.com, chatgpt.com, openai.com}`, and asks the operating system to open it through a tightly scoped Tauri opener capability. `account/read` maps only ChatGPT `email` and `planType`; if App Server reports API-key auth, the UI displays `当前登录方式不受首版支持，请退出后使用 ChatGPT 登录` and does not start a thread.

`account/login/completed` and `account/updated` notifications refresh `DesktopSnapshot`. `chatgpt_logout` calls `account/logout` and clears UI state, not runtime/log directories.

- [ ] **Step 4: Implement safe thread and turn commands**

`conversation_start` sends this normalized payload:

```json
{
  "cwd": "%LOCALAPPDATA%\\ArcGISProAgent\\workspace",
  "sandbox": "read-only",
  "approvalPolicy": "never",
  "approvalsReviewer": "user",
  "serviceName": "ArcGIS Pro Agent",
  "developerInstructions": "You operate ArcGIS Pro only through tools from MCP server arcgis. Never use shell, command execution, file changes, arbitrary scripts, or unregistered geoprocessing. Treat MCP elicitation as mandatory user approval and accurately report structured tool results."
}
```

`turn_start` sends `input: [{"type":"text","text":message,"text_elements":[]}]`. The command rejects empty text and messages over 20,000 UTF-8 bytes. `turn_interrupt` forwards the active thread/turn IDs. Server requests for command, file-change, permissions, dynamic-tool, token refresh, and attestation are denied or returned as unsupported; only `mcpServer/elicitation/request` is forwarded to the approval UI in later phases.

- [ ] **Step 5: Refresh ArcGIS health through App Server**

After App Server reports MCP server `arcgis` ready, and every 10 seconds while the window is visible, call:

```rust
json!({"method":"mcpServer/tool/call","params":{"threadId":active_thread_id,"server":"arcgis","tool":"arcgis_connection_status","arguments":{}}})
```

Map `structuredContent` or the first JSON text content to `BridgeSnapshot`. A failed call produces `disconnected` plus a redacted error; the last successful project/map remains visible with `isLive=false`. Stop polling when the app exits or Codex process stops.

- [ ] **Step 6: Render login, conversation, tool, and status events**

`desktopApi.ts` is the only file importing Tauri `invoke`, `listen`, and opener APIs. `ConversationPane` accumulates agent deltas by item ID and renders MCP call cards by `server/tool/status/duration`. It never renders raw HTML from model or tool output. `ArcGisContextPane` shows protocol/Add-In/ArcGIS versions, project, map, last update time, and explicit disconnected/stale states.

- [ ] **Step 7: Run frontend, Rust, and production build checks**

Run: `cd apps/desktop; npm.cmd test`

Expected: all frontend tests PASS.

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`

Expected: all Rust tests PASS.

Run: `cd apps/desktop; npm.cmd run tauri build -- --debug`

Expected: debug desktop executable builds and contains no API-key input string.

- [ ] **Step 8: Commit the first end-to-end desktop loop**

```powershell
git add apps/desktop
git commit -m "feat: connect ChatGPT conversations to ArcGIS MCP"
```

---

### Task 8: Development Install, Aggregate Verification, and Foundation Documentation

**Files:**
- Create: `scripts/Install-Dev.ps1`
- Create: `scripts/Test-Foundation.ps1`
- Create: `docs/development/foundation.md`
- Modify: `.gitignore`
- Modify: `README.md`
- Remove after parity verification: `McpServer/ArcGisMcpServer/*`
- Remove after parity verification: `AddIn/APBridgeAddIn/*.cs`, `AddIn/APBridgeAddIn/*.csproj`, `AddIn/APBridgeAddIn/Config.daml`
- Modify: `McpServer.sln`

**Interfaces:**
- Consumes: build outputs from Tasks 1-7.
- Produces: repeatable local development installation, non-GUI verification script, explicit file ownership manifest, and migration away from the original sample projects.

- [ ] **Step 1: Write the aggregate verification script before the install script**

```powershell
param([string]$ArcGISProInstallDir = (& "$PSScriptRoot\Resolve-ArcGISProInstall.ps1" -Candidate 'D:\arcgis_pro'))

$ErrorActionPreference = 'Stop'
dotnet test "$PSScriptRoot\..\McpServer.sln" -p:ArcGISProInstallDir=$ArcGISProInstallDir
if ($LASTEXITCODE -ne 0) { throw 'dotnet test failed' }

Push-Location "$PSScriptRoot\..\apps\desktop"
try {
    npm.cmd test
    if ($LASTEXITCODE -ne 0) { throw 'frontend tests failed' }
    npm.cmd run build
    if ($LASTEXITCODE -ne 0) { throw 'frontend build failed' }
} finally { Pop-Location }

cargo test --manifest-path "$PSScriptRoot\..\apps\desktop\src-tauri\Cargo.toml"
if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
```

Run: `powershell -ExecutionPolicy Bypass -File scripts/Test-Foundation.ps1 -ArcGISProInstallDir D:\arcgis_pro`

Expected: FAIL because the new aggregate script/install assumptions are not complete.

- [ ] **Step 2: Implement a manifest-owned development installation**

`Install-Dev.ps1` accepts `-ArcGISProInstallDir`, `-InstallRoot` (default `%LOCALAPPDATA%\ArcGISProAgent\dev`), and `-AddInRoot` (default `%USERPROFILE%\Documents\ArcGIS\AddIns\ArcGISProAgent`). It:

1. builds MCP and Add-In Release outputs;
2. builds the Tauri debug app;
3. copies only those outputs into versioned subdirectories;
4. writes `install-manifest.json` containing every copied absolute path, version, SHA-256, and owner `ArcGISProAgent`;
5. never copies, moves, or deletes `.aprx`, `.gdb`, `.sde`, shapefile, raster, or export files;
6. prints the source, install, Add-In, config, log, and runtime locations.

No uninstall action is included in this slice; the packaging plan will implement removal by consuming the ownership manifest and testing preserve/delete-local-data choices.

- [ ] **Step 3: Replace the sample projects only after parity checks**

Before removing the old sample code, run:

```powershell
dotnet build src/ArcGISProAgent.Mcp/ArcGISProAgent.Mcp.csproj
dotnet build src/ArcGISProAgent.AddIn/ArcGISProAgent.AddIn.csproj -p:ArcGISProInstallDir=D:\arcgis_pro
rg -n "pro\.getActiveMapName|pro\.listLayers|pro\.countFeatures|pro\.zoomToLayer|pro\.selectByAttribute" McpServer AddIn
```

Record the five original operations in `docs/development/foundation.md` as migration inputs for the read-only/navigation plan. Then use `apply_patch` to remove the superseded source and project files and remove their solution entries. Keep the original icon assets until copied into the new Add-In. Git history remains the recovery path.

- [ ] **Step 4: Document exact developer startup and recovery**

`docs/development/foundation.md` includes:

- prerequisites and detected local versions;
- `npm.cmd`/`codex.cmd` Windows command names because PowerShell blocks the `.ps1` shims;
- SDK resolution and the actual `D:\arcgis_pro` verification command;
- build/test commands from this plan;
- ChatGPT login flow and the fact that credentials remain Codex-owned;
- runtime, config, logs, install, Add-In, source, and manifest paths;
- how to open ArcGIS Pro, confirm the Add-In loaded, start the desktop app, and read connection status;
- how to stop processes and remove only development installation files listed in the manifest;
- failure diagnosis for missing Codex, missing runtime file, pipe rejection, protocol mismatch, Add-In not loaded, and MCP startup failure.

Update the root `README.md` to describe the product rather than the original Copilot sample and link the approved spec, this plan, and the development guide.

- [ ] **Step 5: Run the complete non-GUI gate**

Run: `powershell -ExecutionPolicy Bypass -File scripts/Test-Foundation.ps1 -ArcGISProInstallDir D:\arcgis_pro`

Expected: all .NET, frontend, and Rust tests/builds PASS with no errors.

Run: `git status --short`

Expected: only Task 8 documentation/script/removal changes are present before commit; no `bin`, `obj`, `node_modules`, `dist`, runtime secret, token, or user-specific installation output is tracked.

- [ ] **Step 6: Perform the ArcGIS Pro manual smoke test**

Use a blank or disposable ArcGIS Pro project:

1. run `Install-Dev.ps1`;
2. start ArcGIS Pro 3.7 and confirm the Add-In status control shows protocol `1.0`;
3. start the desktop debug executable;
4. complete ChatGPT browser login if signed out;
5. confirm Codex, MCP, Add-In, ArcGIS Pro, project, and active map appear in the right pane;
6. send `检查 ArcGIS Pro 连接状态` and verify one completed `arcgis_connection_status` MCP event card;
7. close ArcGIS Pro and verify the right pane becomes disconnected/stale within 15 seconds without the desktop app crashing;
8. reopen ArcGIS Pro and verify recovery without restarting the desktop app.

Expected: all eight observations pass. Save only a redacted text record in `docs/development/foundation-smoke-2026-07-19.md`; do not commit tokens, full account email, or private GIS paths.

- [ ] **Step 7: Commit and tag the foundation slice**

```powershell
git add .gitignore README.md McpServer.sln scripts docs/development McpServer AddIn
git commit -m "docs: complete the local foundation workflow"
git tag arcgis-pro-agent-foundation-v0.1.0
```

## Plan Acceptance Gate

Before writing the read-only/navigation plan, verify all of the following:

- `scripts/Test-Foundation.ps1` passes on `D:\arcgis_pro`.
- The Add-In package builds without any `C:\Program Files\ArcGIS\Pro` hardcoded reference in source or project files.
- ChatGPT login uses `account/login/start` with type `chatgpt`; no API-key input exists.
- Codex App Server initializes once and uses JSONL over stdio.
- The Codex thread is read-only, never requests shell/file-change approvals, and receives ArcGIS MCP configuration without writing secrets to Git.
- The MCP server exposes only `arcgis_connection_status` and `arcgis_capabilities` in this slice.
- Named-pipe requests require current-user access, protocol `1.0`, matching request IDs, and the startup token.
- The desktop UI shows live and stale/disconnected states distinctly.
- Closing and reopening ArcGIS Pro recovers without restarting the desktop app.
- The development install manifest owns only application/Add-In files and excludes all GIS data.
- Git working tree is clean and the foundation tag points to the verified commit.
