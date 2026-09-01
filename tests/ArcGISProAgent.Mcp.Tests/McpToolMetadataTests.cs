using System.Reflection;
using System.ComponentModel;
using ArcGISProAgent.Mcp.Tools;
using ModelContextProtocol.Server;

namespace ArcGISProAgent.Mcp.Tests;

public sealed class McpToolMetadataTests
{
    [Fact]
    public void Compiled_mcp_assembly_exposes_only_the_exact_R0_and_R1_operations()
    {
        var tools = typeof(ConnectionTools).Assembly
            .GetTypes()
            .SelectMany(type => type.GetMethods(
                BindingFlags.Instance |
                BindingFlags.Static |
                BindingFlags.Public |
                BindingFlags.NonPublic))
            .Select(method => method.GetCustomAttribute<McpServerToolAttribute>())
            .Where(attribute => attribute is not null)
            .OrderBy(attribute => attribute!.Name)
            .ToArray();

        string[] expectedNames =
        [
            "arcgis_activate_view",
            "arcgis_capabilities",
            "arcgis_clear_selection",
            "arcgis_connection_status",
            "arcgis_count_features",
            "arcgis_describe_context",
            "arcgis_describe_layer",
            "arcgis_flash_features",
            "arcgis_get_selection",
            "arcgis_list_fields",
            "arcgis_list_layers",
            "arcgis_query_features",
            "arcgis_query_spatial",
            "arcgis_select_by_attribute",
            "arcgis_select_by_location",
            "arcgis_zoom_to_extent",
            "arcgis_zoom_to_layer",
        ];

        Assert.Equal(expectedNames, tools.Select(attribute => attribute!.Name));
        var nonIdempotent = new HashSet<string>(StringComparer.Ordinal)
        {
            "arcgis_select_by_attribute",
            "arcgis_select_by_location",
            "arcgis_flash_features",
        };
        var R1 = new HashSet<string>(StringComparer.Ordinal)
        {
            "arcgis_select_by_attribute",
            "arcgis_select_by_location",
            "arcgis_clear_selection",
            "arcgis_activate_view",
            "arcgis_zoom_to_layer",
            "arcgis_zoom_to_extent",
            "arcgis_flash_features",
        };
        Assert.All(tools, attribute => AssertMetadata(
            attribute!,
            readOnly: !R1.Contains(attribute!.Name!),
            idempotent: !nonIdempotent.Contains(attribute.Name!),
            destructive: false));
        Assert.DoesNotContain(tools, attribute =>
            attribute!.Name is "Ping" or "Echo"
            || (attribute.Name?.Contains("sql", StringComparison.OrdinalIgnoreCase) ?? false)
            || (attribute.Name?.Contains("operation", StringComparison.OrdinalIgnoreCase) ?? false)
            || (attribute.Name?.Contains("dispatch", StringComparison.OrdinalIgnoreCase) ?? false));
    }

    [Fact]
    public void New_read_tools_describe_stable_project_or_layer_uris()
    {
        var descriptions = new[]
            {
                typeof(ContextTools),
                typeof(LayerTools),
                typeof(QuerySelectionTools),
            }
            .SelectMany(type => type.GetMethods(BindingFlags.Instance | BindingFlags.Public))
            .Where(method => method.GetCustomAttribute<McpServerToolAttribute>() is { ReadOnly: true })
            .Select(method => method.GetCustomAttribute<DescriptionAttribute>()?.Description)
            .ToArray();

        Assert.Equal(8, descriptions.Length);
        Assert.All(descriptions, description =>
        {
            Assert.NotNull(description);
            Assert.Contains("stable", description, StringComparison.OrdinalIgnoreCase);
            Assert.Contains("URI", description, StringComparison.OrdinalIgnoreCase);
        });
    }

    private static void AssertMetadata(
        McpServerToolAttribute attribute,
        bool readOnly,
        bool idempotent,
        bool destructive)
    {
        Assert.False(string.IsNullOrWhiteSpace(attribute.Name));
        Assert.Equal(readOnly, attribute.ReadOnly);
        Assert.Equal(idempotent, attribute.Idempotent);
        Assert.Equal(destructive, attribute.Destructive);
    }
}
