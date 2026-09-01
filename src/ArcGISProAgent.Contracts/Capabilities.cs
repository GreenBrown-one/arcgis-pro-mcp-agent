namespace ArcGISProAgent.Contracts;

public enum RiskLevel { R0, R1, R2, R3 }

public sealed record CapabilityDescriptor(
    string Id,
    string Version,
    RiskLevel Risk,
    bool SupportsCancellation,
    bool SupportsPreview,
    bool SupportsUndo,
    bool SupportsBackup,
    string DisplayName = "",
    string Module = "",
    bool ModifiesProject = false,
    bool ModifiesData = false,
    bool ModifiesFileSystem = false,
    string MinimumArcGisProVersion = "3.7",
    bool IsIdempotent = true);

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

public static class OperationCatalog
{
    private static readonly IReadOnlyList<CapabilityDescriptor> FoundationOperations =
        Array.AsReadOnly(
        [
            new CapabilityDescriptor(
                "connection.health",
                BridgeProtocol.Current,
                RiskLevel.R0,
                SupportsCancellation: true,
                SupportsPreview: false,
                SupportsUndo: false,
                SupportsBackup: false,
                DisplayName: "Connection health",
                Module: "Connection"),
        ]);

    private static readonly IReadOnlyList<CapabilityDescriptor> Phase2Operations =
        Array.AsReadOnly(
        [
            Create("context.describe", "Describe context", "Context", RiskLevel.R0),
            Create("layers.list", "List layers", "Layers", RiskLevel.R0),
            Create("layers.describe", "Describe layer", "Layers", RiskLevel.R0),
            Create("layers.fields", "List layer fields", "Layers", RiskLevel.R0),
            Create("query.feature_count", "Count features", "Query", RiskLevel.R0),
            Create("query.features", "Query features", "Query", RiskLevel.R0),
            Create("query.spatial", "Query features spatially", "Query", RiskLevel.R0),
            Create("selection.describe", "Describe selection", "Selection", RiskLevel.R0),
            Create(
                "selection.by_attribute",
                "Select by attribute",
                "Selection",
                RiskLevel.R1,
                isIdempotent: false),
            Create(
                "selection.by_location",
                "Select by location",
                "Selection",
                RiskLevel.R1,
                isIdempotent: false),
            Create("selection.clear", "Clear selection", "Selection", RiskLevel.R1),
            Create("map_view.activate", "Activate map view", "Map view", RiskLevel.R1),
            Create("map_view.zoom_to_layer", "Zoom to layer", "Map view", RiskLevel.R1),
            Create("map_view.zoom_to_extent", "Zoom to extent", "Map view", RiskLevel.R1),
            Create(
                "map_view.flash_features",
                "Flash features",
                "Map view",
                RiskLevel.R1,
                isIdempotent: false),
        ]);

    private static readonly IReadOnlyList<CapabilityDescriptor> AllOperations =
        Array.AsReadOnly(FoundationOperations.Concat(Phase2Operations).ToArray());

    public static IReadOnlyList<CapabilityDescriptor> Foundation => FoundationOperations;

    public static IReadOnlyList<CapabilityDescriptor> Phase2 => Phase2Operations;

    public static IReadOnlyList<CapabilityDescriptor> All => AllOperations;

    private static CapabilityDescriptor Create(
        string id,
        string displayName,
        string module,
        RiskLevel risk,
        bool isIdempotent = true) =>
        new(
            id,
            BridgeProtocol.Current,
            risk,
            SupportsCancellation: false,
            SupportsPreview: false,
            SupportsUndo: false,
            SupportsBackup: false,
            DisplayName: displayName,
            Module: module,
            ModifiesProject: false,
            ModifiesData: false,
            ModifiesFileSystem: false,
            MinimumArcGisProVersion: "3.7",
            IsIdempotent: isIdempotent);
}
