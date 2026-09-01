namespace ArcGISProAgent.Contracts;

public sealed record MapExtent(
    double XMin,
    double YMin,
    double XMax,
    double YMax,
    int? Wkid);

public enum ProjectItemKind
{
    Map,
    Scene,
    Layout,
}

public sealed record ProjectItemSummary(
    string Uri,
    string Name,
    ProjectItemKind Kind,
    bool IsActive);

public sealed record ProjectSummary(
    string Name,
    string? Path,
    bool HasUnsavedChanges,
    IReadOnlyList<ProjectItemSummary> Items);

public sealed record ActiveViewSummary(
    string Uri,
    string Name,
    ProjectItemKind Kind,
    MapExtent? Extent);

public sealed record ContextDescribeArguments;

public sealed record ContextDescription(
    ProjectSummary? Project,
    ActiveViewSummary? ActiveView);

public sealed record ActivateViewArguments(string ItemUri);

public sealed record ActivateViewResult(string ItemUri, bool Activated);

public sealed record ZoomToExtentArguments(MapExtent Extent);

public sealed record ZoomResult(bool Completed);

public sealed record FlashFeaturesArguments(
    string LayerUri,
    IReadOnlyList<long> ObjectIds,
    int DurationMilliseconds = 1000);

public sealed record FlashFeaturesResult(bool Completed, long FlashedCount);
