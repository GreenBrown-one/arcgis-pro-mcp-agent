using System.Text.Json;
using ArcGISProAgent.Bridge;

namespace ArcGISProAgent.Mcp.Tests;

public static class TestCollections
{
    public const string RuntimeEnvironment = "Runtime environment";
}

[CollectionDefinition(TestCollections.RuntimeEnvironment, DisableParallelization = true)]
public sealed class RuntimeEnvironmentCollectionDefinition;

[Collection(TestCollections.RuntimeEnvironment)]
public sealed class BridgeClientFactoryTests
{
    private const string RuntimeFileVariable = "ARCGIS_AGENT_RUNTIME_FILE";

    [Fact]
    public void Create_rejects_a_missing_runtime_file_variable()
    {
        using var environment = new RuntimeFileEnvironment(null);

        Assert.Throws<InvalidOperationException>(() => BridgeClientFactory.Create());
    }

    [Theory]
    [InlineData("")]
    [InlineData(" ")]
    [InlineData("\t")]
    public void Create_rejects_a_blank_runtime_file_variable(string value)
    {
        using var environment = new RuntimeFileEnvironment(value);

        Assert.Throws<InvalidOperationException>(() => BridgeClientFactory.Create());
    }

    [Fact]
    public void Create_rejects_a_missing_runtime_file()
    {
        using var temporaryDirectory = new TemporaryDirectory();
        var missingPath = Path.Combine(temporaryDirectory.DirectoryPath, "missing.json");
        using var environment = new RuntimeFileEnvironment(missingPath);

        Assert.Throws<FileNotFoundException>(() => BridgeClientFactory.Create());
    }

    [Fact]
    public void Create_rejects_invalid_runtime_json()
    {
        using var temporaryDirectory = new TemporaryDirectory();
        var runtimeFile = temporaryDirectory.WriteFile("invalid.json", "{not-json");
        using var environment = new RuntimeFileEnvironment(runtimeFile);

        Assert.Throws<JsonException>(() => BridgeClientFactory.Create());
    }

    [Fact]
    public void Create_rejects_blank_runtime_credentials()
    {
        using var temporaryDirectory = new TemporaryDirectory();
        var runtimeFile = temporaryDirectory.WriteFile(
            "blank.json",
            """{"pipeName":" ","token":" "}""");
        using var environment = new RuntimeFileEnvironment(runtimeFile);

        Assert.Throws<InvalidDataException>(() => BridgeClientFactory.Create());
    }

    [Fact]
    public void Create_builds_a_named_pipe_client_from_valid_runtime_credentials()
    {
        using var temporaryDirectory = new TemporaryDirectory();
        var runtimeFile = temporaryDirectory.WriteFile(
            "valid.json",
            """{"pipeName":"test-pipe","token":"test-token"}""");
        using var environment = new RuntimeFileEnvironment(runtimeFile);

        var client = BridgeClientFactory.Create();

        Assert.IsType<NamedPipeBridgeClient>(client);
    }

    [Fact]
    public void Create_uses_a_five_second_bridge_timeout()
    {
        Assert.Equal(TimeSpan.FromSeconds(5), BridgeClientFactory.BridgeTimeout);
    }

    private sealed class RuntimeFileEnvironment : IDisposable
    {
        private readonly string? _originalValue =
            Environment.GetEnvironmentVariable(RuntimeFileVariable);

        public RuntimeFileEnvironment(string? value) =>
            Environment.SetEnvironmentVariable(RuntimeFileVariable, value);

        public void Dispose() =>
            Environment.SetEnvironmentVariable(RuntimeFileVariable, _originalValue);
    }

    private sealed class TemporaryDirectory : IDisposable
    {
        public TemporaryDirectory()
        {
            DirectoryPath = Directory.CreateDirectory(
                Path.Combine(
                    Path.GetTempPath(),
                    $"ArcGISProAgent.Mcp.Tests.{Guid.NewGuid():N}"))
                .FullName;
        }

        public string DirectoryPath { get; }

        public string WriteFile(string fileName, string contents)
        {
            var path = Path.Combine(DirectoryPath, fileName);
            File.WriteAllText(path, contents);
            return path;
        }

        public void Dispose()
        {
            if (Directory.Exists(DirectoryPath))
            {
                Directory.Delete(DirectoryPath, recursive: true);
            }
        }
    }
}
