using System.Text.Json;

namespace ArcGISProAgent.Contracts;

public enum AttributeComparisonOperator
{
    Equal,
    NotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    StartsWith,
    Contains,
    IsNull,
    IsNotNull,
}

public sealed record AttributePredicate(
    string Field,
    AttributeComparisonOperator Operator,
    JsonElement? Value);

public sealed record FeatureCountArguments(
    string LayerUri,
    AttributePredicate? Predicate = null);

public sealed record FeatureCountResult(string LayerUri, long Count);

public sealed record FeatureQueryArguments(
    string LayerUri,
    IReadOnlyList<string> Fields,
    AttributePredicate? Predicate = null,
    int Offset = 0,
    int Limit = 20);

public sealed record FeatureRecord(
    long ObjectId,
    IReadOnlyDictionary<string, JsonElement?> Values);

public sealed record FeatureQueryResult(
    string LayerUri,
    int Offset,
    int Limit,
    long? TotalCount,
    IReadOnlyList<FeatureRecord> Features,
    bool HasMore);

public enum SpatialRelation
{
    Intersects,
    Within,
    Contains,
    Touches,
    Crosses,
    Overlaps,
}

public enum SpatialQuerySourceKind
{
    Layer,
    Extent,
    CurrentView,
}

public sealed record SpatialQuerySource(
    SpatialQuerySourceKind Kind,
    string? LayerUri,
    MapExtent? Extent);

public sealed record SpatialQueryArguments(
    string LayerUri,
    SpatialQuerySource Source,
    SpatialRelation Relation,
    IReadOnlyList<string> Fields,
    int Offset = 0,
    int Limit = 20);

public enum SelectionCombinationMode
{
    Replace,
    Add,
    Remove,
    Toggle,
}

public sealed record SelectionDescribeArguments(
    string? LayerUri = null,
    int ObjectIdLimit = 20);

public sealed record LayerSelectionSummary(
    string LayerUri,
    long Count,
    IReadOnlyList<long> ObjectIds,
    bool Truncated);

public sealed record SelectionDescription(
    IReadOnlyList<LayerSelectionSummary> Layers);

public sealed record SelectByAttributeArguments(
    string LayerUri,
    AttributePredicate Predicate,
    SelectionCombinationMode Mode = SelectionCombinationMode.Replace);

public sealed record SelectByLocationArguments(
    string LayerUri,
    SpatialQuerySource Source,
    SpatialRelation Relation,
    SelectionCombinationMode Mode = SelectionCombinationMode.Replace);

public sealed record SelectionResult(string LayerUri, long SelectedCount);

public sealed record ClearSelectionArguments(string? LayerUri = null);

public sealed record ClearSelectionResult(long LayersCleared, long FeaturesCleared);

public sealed record ZoomToLayerArguments(
    string LayerUri,
    bool SelectedOnly = false);

public static class GisContractGuards
{
    public const int MaximumPublicStringLength = 2000;
    public const int MaximumResultLimit = 100;
    public const int MaximumObjectIdCount = 100;

    public static void Validate(ContextDescribeArguments arguments) =>
        ArgumentNullException.ThrowIfNull(arguments);

    public static void Validate(ListLayersArguments arguments) =>
        ArgumentNullException.ThrowIfNull(arguments);

    public static void Validate(DescribeLayerArguments arguments)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        ValidateUri(arguments.LayerUri, nameof(arguments.LayerUri));
    }

    public static void Validate(ListFieldsArguments arguments)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        ValidateUri(arguments.LayerUri, nameof(arguments.LayerUri));
    }

    public static void Validate(FeatureCountArguments arguments)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        ValidateUri(arguments.LayerUri, nameof(arguments.LayerUri));
        if (arguments.Predicate is not null)
        {
            Validate(arguments.Predicate);
        }
    }

    public static void Validate(FeatureQueryArguments arguments)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        ValidateUri(arguments.LayerUri, nameof(arguments.LayerUri));
        ValidateFields(arguments.Fields, nameof(arguments.Fields));
        ValidateOffset(arguments.Offset, nameof(arguments.Offset));
        ValidateLimit(arguments.Limit, nameof(arguments.Limit));
        if (arguments.Predicate is not null)
        {
            Validate(arguments.Predicate);
        }
    }

    public static void Validate(SpatialQueryArguments arguments)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        ValidateUri(arguments.LayerUri, nameof(arguments.LayerUri));
        Validate(arguments.Source);
        Validate(arguments.Relation);
        ValidateFields(arguments.Fields, nameof(arguments.Fields));
        ValidateOffset(arguments.Offset, nameof(arguments.Offset));
        ValidateLimit(arguments.Limit, nameof(arguments.Limit));
    }

    public static void Validate(SelectionDescribeArguments arguments)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        ValidateOptionalUri(arguments.LayerUri, nameof(arguments.LayerUri));
        ValidateLimit(arguments.ObjectIdLimit, nameof(arguments.ObjectIdLimit));
    }

    public static void Validate(SelectByAttributeArguments arguments)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        ValidateUri(arguments.LayerUri, nameof(arguments.LayerUri));
        Validate(arguments.Predicate);
        Validate(arguments.Mode);
    }

    public static void Validate(SelectByLocationArguments arguments)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        ValidateUri(arguments.LayerUri, nameof(arguments.LayerUri));
        Validate(arguments.Source);
        Validate(arguments.Relation);
        Validate(arguments.Mode);
    }

    public static void Validate(ClearSelectionArguments arguments)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        ValidateOptionalUri(arguments.LayerUri, nameof(arguments.LayerUri));
    }

    public static void Validate(ActivateViewArguments arguments)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        ValidateUri(arguments.ItemUri, nameof(arguments.ItemUri));
    }

    public static void Validate(ZoomToLayerArguments arguments)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        ValidateUri(arguments.LayerUri, nameof(arguments.LayerUri));
    }

    public static void Validate(ZoomToExtentArguments arguments)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        Validate(arguments.Extent);
    }

    public static void Validate(FlashFeaturesArguments arguments)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        ValidateUri(arguments.LayerUri, nameof(arguments.LayerUri));
        ValidateObjectIds(arguments.ObjectIds, nameof(arguments.ObjectIds));
        if (arguments.DurationMilliseconds <= 0)
        {
            throw new ArgumentOutOfRangeException(
                nameof(arguments.DurationMilliseconds),
                "Flash duration must be positive.");
        }
    }

    public static void Validate(AttributePredicate predicate)
    {
        ArgumentNullException.ThrowIfNull(predicate);
        ValidateRequiredString(predicate.Field, nameof(predicate.Field));
        ValidateDefinedEnum(predicate.Operator, nameof(predicate.Operator));

        var isNullOperator = predicate.Operator is AttributeComparisonOperator.IsNull
            or AttributeComparisonOperator.IsNotNull;
        if (isNullOperator)
        {
            if (predicate.Value is not null)
            {
                throw new ArgumentException(
                    "Null predicates cannot include a value.",
                    nameof(predicate));
            }

            return;
        }

        if (predicate.Value is null)
        {
            throw new ArgumentException(
                "The predicate value is required.",
                nameof(predicate));
        }

        ValidateScalar(predicate.Value.Value, nameof(predicate.Value));
    }

    public static void Validate(MapExtent extent)
    {
        ArgumentNullException.ThrowIfNull(extent);
        if (!double.IsFinite(extent.XMin)
            || !double.IsFinite(extent.YMin)
            || !double.IsFinite(extent.XMax)
            || !double.IsFinite(extent.YMax)
            || extent.XMin >= extent.XMax
            || extent.YMin >= extent.YMax)
        {
            throw new ArgumentException(
                "Extent coordinates must be finite and normalized.",
                nameof(extent));
        }
    }

    public static void Validate(SpatialRelation relation) =>
        ValidateDefinedEnum(relation, nameof(relation));

    public static void Validate(SpatialQuerySource source)
    {
        ArgumentNullException.ThrowIfNull(source);
        ValidateDefinedEnum(source.Kind, nameof(source.Kind));

        switch (source.Kind)
        {
            case SpatialQuerySourceKind.Layer:
                ValidateUri(source.LayerUri!, nameof(source.LayerUri));
                if (source.Extent is not null)
                {
                    throw new ArgumentException(
                        "A layer source cannot include an extent.",
                        nameof(source));
                }

                break;
            case SpatialQuerySourceKind.Extent:
                if (source.LayerUri is not null || source.Extent is null)
                {
                    throw new ArgumentException(
                        "An extent source requires only an extent.",
                        nameof(source));
                }

                Validate(source.Extent);
                break;
            case SpatialQuerySourceKind.CurrentView:
                if (source.LayerUri is not null || source.Extent is not null)
                {
                    throw new ArgumentException(
                        "A current-view source cannot include layer or extent data.",
                        nameof(source));
                }

                break;
        }
    }

    public static void Validate(SelectionCombinationMode mode) =>
        ValidateDefinedEnum(mode, nameof(mode));

    private static void ValidateScalar(JsonElement value, string parameterName)
    {
        if (value.ValueKind is not (JsonValueKind.String
            or JsonValueKind.Number
            or JsonValueKind.True
            or JsonValueKind.False))
        {
            throw new ArgumentException(
                "Predicate values must be JSON scalars.",
                parameterName);
        }

        if (value.ValueKind is JsonValueKind.String)
        {
            ValidateRequiredString(value.GetString()!, parameterName);
        }
    }

    private static void ValidateFields(
        IReadOnlyList<string> fields,
        string parameterName)
    {
        ArgumentNullException.ThrowIfNull(fields, parameterName);
        if (fields.Count == 0)
        {
            throw new ArgumentException(
                "At least one field is required.",
                parameterName);
        }

        var uniqueFields = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var field in fields)
        {
            ValidateRequiredString(field, parameterName);
            if (!uniqueFields.Add(field))
            {
                throw new ArgumentException(
                    "Requested fields cannot contain duplicates.",
                    parameterName);
            }
        }
    }

    private static void ValidateObjectIds(
        IReadOnlyList<long> objectIds,
        string parameterName)
    {
        ArgumentNullException.ThrowIfNull(objectIds, parameterName);
        if (objectIds.Count == 0)
        {
            throw new ArgumentException(
                "At least one object ID is required.",
                parameterName);
        }

        if (objectIds.Count > MaximumObjectIdCount)
        {
            throw new ArgumentOutOfRangeException(
                parameterName,
                $"At most {MaximumObjectIdCount} object IDs may be requested.");
        }

        var uniqueObjectIds = new HashSet<long>();
        foreach (var objectId in objectIds)
        {
            if (objectId <= 0)
            {
                throw new ArgumentOutOfRangeException(
                    parameterName,
                    "Object IDs must be positive.");
            }

            if (!uniqueObjectIds.Add(objectId))
            {
                throw new ArgumentException(
                    "Object IDs cannot contain duplicates.",
                    parameterName);
            }
        }
    }

    private static void ValidateOptionalUri(string? value, string parameterName)
    {
        if (value is not null)
        {
            ValidateUri(value, parameterName);
        }
    }

    private static void ValidateUri(string value, string parameterName) =>
        ValidateRequiredString(value, parameterName);

    private static void ValidateRequiredString(string value, string parameterName)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            throw new ArgumentException("A non-empty value is required.", parameterName);
        }

        if (value.Length > MaximumPublicStringLength)
        {
            throw new ArgumentOutOfRangeException(
                parameterName,
                $"Values cannot exceed {MaximumPublicStringLength} characters.");
        }
    }

    private static void ValidateLimit(int value, string parameterName)
    {
        if (value is < 1 or > MaximumResultLimit)
        {
            throw new ArgumentOutOfRangeException(
                parameterName,
                $"Limits must be between 1 and {MaximumResultLimit}.");
        }
    }

    private static void ValidateOffset(int value, string parameterName)
    {
        if (value < 0)
        {
            throw new ArgumentOutOfRangeException(
                parameterName,
                "Offsets cannot be negative.");
        }
    }

    private static void ValidateDefinedEnum<T>(T value, string parameterName)
        where T : struct, Enum
    {
        if (!Enum.IsDefined(value))
        {
            throw new ArgumentOutOfRangeException(
                parameterName,
                value,
                "The value is not supported.");
        }
    }
}
