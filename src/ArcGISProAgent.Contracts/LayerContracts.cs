namespace ArcGISProAgent.Contracts;

public sealed record ListLayersArguments(bool IncludeNested = true);

public sealed record LayerSummary(
    string Uri,
    string Name,
    string LongName,
    string LayerType,
    string? ParentUri,
    int Depth,
    bool Visible,
    bool IsFeatureLayer);

public sealed record LayerListResult(IReadOnlyList<LayerSummary> Layers);

public sealed record DescribeLayerArguments(string LayerUri);

public sealed record SpatialReferenceSummary(int? Wkid, string Name);

public sealed record LayerDescription(
    string Uri,
    string Name,
    string LayerType,
    string? SourceType,
    string? SourcePath,
    string? GeometryType,
    SpatialReferenceSummary? SpatialReference,
    string ConnectionStatus,
    long? FeatureCount);

public sealed record ListFieldsArguments(string LayerUri);

public sealed record FieldSummary(
    string Name,
    string Alias,
    string FieldType,
    bool IsNullable,
    bool IsEditable,
    string? DomainName);

public sealed record LayerFieldsResult(
    string LayerUri,
    IReadOnlyList<FieldSummary> Fields);
