using System.Globalization;
using System.IO;
using System.Text.Json;
using ArcGIS.Core.Data;
using ArcGIS.Core.Geometry;
using ArcGIS.Desktop.Mapping;
using ArcGISProAgent.Contracts;

namespace ArcGISProAgent.AddIn.Operations;

internal static class QueryOperations
{
    private const int MaximumFallbackRows = 10_000;
    private const int MaximumSpatialSourceFeatures = 1_000;
    private const int MaximumPublicResultBytes = 900 * 1024;

    internal static FeatureCountResult Count(FeatureCountArguments arguments)
    {
        var layer = ArcGisObjectResolver.RequireBasicFeatureLayer(
            ArcGisObjectResolver.ResolveLayer(arguments.LayerUri));
        using var table = OpenTable(layer);

        if (arguments.Predicate is null)
        {
            return new FeatureCountResult(arguments.LayerUri, table.GetCount());
        }

        using var definition = table.GetDefinition();
        var predicateField = ResolvePredicateField(definition, arguments.Predicate);
        var filter = new QueryFilter { SubFields = predicateField.Name };
        long count = 0;
        using var cursor = table.Search(filter, false);
        while (cursor.MoveNext())
        {
            using var row = cursor.Current;
            if (EvaluatePredicate(row[predicateField.Name], predicateField, arguments.Predicate))
            {
                count++;
            }
        }

        return new FeatureCountResult(arguments.LayerUri, count);
    }

    internal static FeatureQueryResult Query(FeatureQueryArguments arguments)
    {
        var layer = ArcGisObjectResolver.RequireBasicFeatureLayer(
            ArcGisObjectResolver.ResolveLayer(arguments.LayerUri));
        using var table = OpenTable(layer);
        using var definition = table.GetDefinition();
        var fields = ResolveRequestedFields(definition, arguments.Fields);
        var predicateField = arguments.Predicate is null
            ? null
            : ResolvePredicateField(definition, arguments.Predicate);
        long? totalCount = arguments.Predicate is null ? table.GetCount() : null;

        return QueryPage(
            table,
            definition,
            arguments.LayerUri,
            fields,
            arguments.Predicate,
            predicateField,
            new QueryFilter(),
            arguments.Offset,
            arguments.Limit,
            totalCount,
            allowSdkPagination: arguments.Predicate is null);
    }

    internal static FeatureQueryResult QuerySpatial(SpatialQueryArguments arguments)
    {
        var resolved = ArcGisObjectResolver.ResolveLayer(arguments.LayerUri);
        var targetLayer = ArcGisObjectResolver.RequireFeatureLayer(resolved);
        using var table = OpenTable(targetLayer);
        using var definition = table.GetDefinition();
        if (definition is not FeatureClassDefinition featureDefinition)
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.UnsupportedLayerType,
                "This operation requires a spatial feature class.");
        }

        var targetSpatialReference = featureDefinition.GetSpatialReference();
        if (targetSpatialReference is null || targetSpatialReference.IsUnknown)
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.InvalidSpatialSource,
                "The target layer does not have a known spatial reference.");
        }

        var fields = ResolveRequestedFields(definition, arguments.Fields);
        var geometry = CreateSpatialSourceGeometry(
            arguments.Source,
            resolved.Map,
            targetSpatialReference);
        if (geometry is null || geometry.IsEmpty)
        {
            return new FeatureQueryResult(
                arguments.LayerUri,
                arguments.Offset,
                arguments.Limit,
                0,
                Array.Empty<FeatureRecord>(),
                HasMore: false);
        }

        var filter = CreateSpatialFilter(geometry, arguments.Relation);
        var totalCount = table.GetCount(filter);
        return QueryPage(
            table,
            definition,
            arguments.LayerUri,
            fields,
            predicate: null,
            predicateField: null,
            filter,
            arguments.Offset,
            arguments.Limit,
            totalCount,
            allowSdkPagination: true);
    }

    internal static SelectionDescription DescribeSelection(
        SelectionDescribeArguments arguments)
    {
        IReadOnlyList<FeatureLayer> layers;
        if (arguments.LayerUri is not null)
        {
            layers =
            [
                ArcGisObjectResolver.RequireFeatureLayer(
                    ArcGisObjectResolver.ResolveLayer(arguments.LayerUri))
            ];
        }
        else
        {
            layers = ArcGisObjectResolver.RequireActiveMap()
                .GetLayersAsFlattenedList()
                .OfType<FeatureLayer>()
                .ToArray();
        }

        var summaries = new List<LayerSelectionSummary>(layers.Count);
        foreach (var layer in layers)
        {
            using var selection = layer.GetSelection();
            var count = selection.GetCount();
            var objectIds = selection.GetObjectIDs()
                .OrderBy(id => id)
                .Take(arguments.ObjectIdLimit)
                .ToArray();
            summaries.Add(new LayerSelectionSummary(
                layer.URI,
                count,
                objectIds,
                count > objectIds.LongLength));
        }

        return new SelectionDescription(summaries);
    }

    internal static FeatureQueryResult QueryPage(
        Table table,
        TableDefinition definition,
        string layerUri,
        IReadOnlyList<Field> requestedFields,
        AttributePredicate? predicate,
        Field? predicateField,
        QueryFilter filter,
        int offset,
        int limit,
        long? totalCount,
        bool allowSdkPagination)
    {
        var objectIdField = definition.GetObjectIDField();
        if (string.IsNullOrWhiteSpace(objectIdField))
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.UnsupportedLayerType,
                "The layer does not provide stable object IDs.");
        }

        var subfields = new[] { objectIdField }.AsEnumerable();
        if (predicateField is not null)
        {
            subfields = subfields.Append(predicateField.Name);
        }

        filter.SubFields = string.Join(",", subfields.Distinct(StringComparer.OrdinalIgnoreCase));
        IReadOnlyList<long> pageObjectIds;
        var effectiveTotalCount = totalCount;
        if (allowSdkPagination && SupportsPagination(table))
        {
            filter.PostfixClause = $"ORDER BY {objectIdField}";
            filter.Offset = offset;
            filter.RowCount = limit + 1;
            pageObjectIds = ReadPageObjectIds(
                table,
                filter,
                predicate,
                predicateField,
                maximumScannedRows: limit + 1,
                failWhenScanLimitExceeded: false);
        }
        else
        {
            var matchingObjectIds = ReadPageObjectIds(
                table,
                filter,
                predicate,
                predicateField,
                maximumScannedRows: MaximumFallbackRows,
                failWhenScanLimitExceeded: true);
            pageObjectIds = matchingObjectIds
                .OrderBy(objectId => objectId)
                .Skip(offset)
                .Take(limit + 1)
                .ToArray();
            effectiveTotalCount ??= matchingObjectIds.Count;
        }

        return CreatePage(
            table,
            filter,
            layerUri,
            objectIdField,
            requestedFields,
            offset,
            limit,
            effectiveTotalCount,
            pageObjectIds);
    }

    internal static bool EvaluatePredicate(
        object? actualValue,
        Field field,
        AttributePredicate predicate)
    {
        var actual = actualValue is null or DBNull ? null : actualValue;
        if (predicate.Operator is AttributeComparisonOperator.IsNull)
        {
            return actual is null;
        }

        if (predicate.Operator is AttributeComparisonOperator.IsNotNull)
        {
            return actual is not null;
        }

        if (actual is null || predicate.Value is not { } expected)
        {
            return false;
        }

        if (predicate.Operator is AttributeComparisonOperator.StartsWith
            or AttributeComparisonOperator.Contains)
        {
            if (actual is not string actualText || expected.ValueKind is not JsonValueKind.String)
            {
                throw InvalidPredicate();
            }

            var expectedText = expected.GetString()!;
            return predicate.Operator is AttributeComparisonOperator.StartsWith
                ? actualText.StartsWith(expectedText, StringComparison.OrdinalIgnoreCase)
                : actualText.Contains(expectedText, StringComparison.OrdinalIgnoreCase);
        }

        var isOrdering = predicate.Operator is AttributeComparisonOperator.GreaterThan
            or AttributeComparisonOperator.GreaterThanOrEqual
            or AttributeComparisonOperator.LessThan
            or AttributeComparisonOperator.LessThanOrEqual;
        if (isOrdering && actual is bool or Guid)
        {
            throw InvalidPredicate();
        }

        var comparison = CompareTyped(actual, field, expected);
        return predicate.Operator switch
        {
            AttributeComparisonOperator.Equal => comparison == 0,
            AttributeComparisonOperator.NotEqual => comparison != 0,
            AttributeComparisonOperator.GreaterThan => comparison > 0,
            AttributeComparisonOperator.GreaterThanOrEqual => comparison >= 0,
            AttributeComparisonOperator.LessThan => comparison < 0,
            AttributeComparisonOperator.LessThanOrEqual => comparison <= 0,
            _ => throw InvalidPredicate(),
        };
    }

    internal static Geometry? CreateSpatialSourceGeometry(
        SpatialQuerySource source,
        Map targetMap,
        SpatialReference targetSpatialReference)
    {
        Geometry? sourceGeometry;
        string projectionFailureCode;
        switch (source.Kind)
        {
            case SpatialQuerySourceKind.Extent:
            {
                var extent = source.Extent!;
                if (extent.Wkid is <= 0)
                {
                    throw new ArcGisOperationException(
                        BridgeErrorCodes.InvalidExtent,
                        "The extent spatial reference is invalid.");
                }

                try
                {
                    var spatialReference = extent.Wkid is > 0
                        ? SpatialReferenceBuilder.CreateSpatialReference(extent.Wkid.Value)
                        : targetSpatialReference;
                    sourceGeometry = EnvelopeBuilderEx.CreateEnvelope(
                        extent.XMin,
                        extent.YMin,
                        extent.XMax,
                        extent.YMax,
                        spatialReference);
                }
                catch
                {
                    throw new ArcGisOperationException(
                        BridgeErrorCodes.InvalidExtent,
                        "The extent spatial reference is invalid.");
                }

                projectionFailureCode = BridgeErrorCodes.InvalidExtent;
                break;
            }
            case SpatialQuerySourceKind.CurrentView:
            {
                var view = MapView.Active;
                if (view?.Map is null)
                {
                    throw new ArcGisOperationException(
                        BridgeErrorCodes.NoActiveView,
                        "No active map view is available.");
                }

                if (!string.Equals(view.Map.URI, targetMap.URI, StringComparison.Ordinal))
                {
                    throw new ArcGisOperationException(
                        BridgeErrorCodes.InvalidSpatialSource,
                        "The active view is not compatible with the target layer.");
                }

                sourceGeometry = view.Extent;
                projectionFailureCode = BridgeErrorCodes.InvalidSpatialSource;
                break;
            }
            case SpatialQuerySourceKind.Layer:
                sourceGeometry = ReadSourceLayerGeometry(source.LayerUri!);
                projectionFailureCode = BridgeErrorCodes.InvalidSpatialSource;
                break;
            default:
                throw new ArcGisOperationException(
                    BridgeErrorCodes.InvalidSpatialSource,
                    "The spatial source is not supported.");
        }

        return sourceGeometry is null
            ? null
            : ProjectToTargetSpatialReference(
                sourceGeometry,
                targetSpatialReference,
                projectionFailureCode);
    }

    internal static Geometry ProjectToTargetSpatialReference(
        Geometry geometry,
        SpatialReference targetSpatialReference,
        string failureCode)
    {
        if (targetSpatialReference.IsUnknown
            || geometry.SpatialReference is null
            || geometry.SpatialReference.IsUnknown)
        {
            throw ProjectionFailed(failureCode);
        }

        try
        {
            return GeometryEngine.Instance.Project(geometry, targetSpatialReference);
        }
        catch
        {
            throw ProjectionFailed(failureCode);
        }
    }

    internal static SpatialQueryFilter CreateSpatialFilter(
        Geometry geometry,
        SpatialRelation relation) =>
        new()
        {
            FilterGeometry = geometry,
            SpatialRelationship = relation switch
            {
                SpatialRelation.Intersects => SpatialRelationship.Intersects,
                SpatialRelation.Within => SpatialRelationship.Within,
                SpatialRelation.Contains => SpatialRelationship.Contains,
                SpatialRelation.Touches => SpatialRelationship.Touches,
                SpatialRelation.Crosses => SpatialRelationship.Crosses,
                SpatialRelation.Overlaps => SpatialRelationship.Overlaps,
                _ => throw new ArcGisOperationException(
                    BridgeErrorCodes.InvalidSpatialSource,
                    "The spatial relation is not supported."),
            },
        };

    private static Geometry? ReadSourceLayerGeometry(string layerUri)
    {
        var sourceLayer = ArcGisObjectResolver.RequireFeatureLayer(
            ArcGisObjectResolver.ResolveLayer(layerUri));
        using var table = OpenTable(sourceLayer);
        using var definition = table.GetDefinition();
        if (definition is not FeatureClassDefinition featureDefinition)
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.InvalidSpatialSource,
                "The source layer is not a spatial feature class.");
        }

        RequireKnownSourceSpatialReference(featureDefinition);
        var shapeField = featureDefinition.GetShapeField();
        var filter = new QueryFilter { SubFields = shapeField };
        var geometries = new List<Geometry>();
        using var cursor = table.Search(filter, false);
        var sourceFeatureCount = 0;
        while (cursor.MoveNext())
        {
            sourceFeatureCount++;
            if (sourceFeatureCount > MaximumSpatialSourceFeatures)
            {
                throw new ArcGisOperationException(
                    BridgeErrorCodes.InvalidSpatialSource,
                    "The spatial source layer exceeds the 1,000-feature limit.");
            }

            using var row = cursor.Current;
            if (row[shapeField] is Geometry geometry && !geometry.IsEmpty)
            {
                if (geometry.SpatialReference is null || geometry.SpatialReference.IsUnknown)
                {
                    throw ProjectionFailed(BridgeErrorCodes.InvalidSpatialSource);
                }

                geometries.Add(geometry);
            }
        }

        try
        {
            return geometries.Count switch
            {
                0 => null,
                1 => geometries[0],
                _ => GeometryEngine.Instance.Union(geometries),
            };
        }
        catch
        {
            throw ProjectionFailed(BridgeErrorCodes.InvalidSpatialSource);
        }
    }

    private static void RequireKnownSourceSpatialReference(
        FeatureClassDefinition featureDefinition)
    {
        try
        {
            var spatialReference = featureDefinition.GetSpatialReference();
            if (spatialReference is null || spatialReference.IsUnknown)
            {
                throw new InvalidOperationException();
            }
        }
        catch
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.InvalidSpatialSource,
                "The source layer does not have a known spatial reference.");
        }
    }

    internal static IReadOnlyList<long> ReadPageObjectIds(
        Table table,
        QueryFilter filter,
        AttributePredicate? predicate,
        Field? predicateField,
        int maximumScannedRows,
        bool failWhenScanLimitExceeded)
    {
        var objectIds = new List<long>(Math.Min(maximumScannedRows, 101));
        using var cursor = table.Search(filter, false);
        var scannedRows = 0;
        while (cursor.MoveNext())
        {
            scannedRows++;
            if (scannedRows > maximumScannedRows)
            {
                if (failWhenScanLimitExceeded)
                {
                    throw FallbackLimitExceeded();
                }

                break;
            }

            using var row = cursor.Current;
            if (predicate is not null
                && predicateField is not null
                && !EvaluatePredicate(row[predicateField.Name], predicateField, predicate))
            {
                continue;
            }

            objectIds.Add(row.GetObjectID());
        }

        return objectIds;
    }

    private static IReadOnlyList<FeatureRecord> ReadPublicRecords(
        Table table,
        QueryFilter filter,
        string objectIdField,
        IReadOnlyList<Field> requestedFields,
        IReadOnlyList<long> publicObjectIds)
    {
        filter.Offset = 0;
        filter.RowCount = 0;
        filter.PostfixClause = string.Empty;
        filter.ObjectIDs = publicObjectIds;
        if (publicObjectIds.Count == 0)
        {
            return Array.Empty<FeatureRecord>();
        }

        filter.SubFields = string.Join(",", requestedFields
            .Select(field => field.Name)
            .Append(objectIdField)
            .Distinct(StringComparer.OrdinalIgnoreCase));
        var allowedObjectIds = publicObjectIds.ToHashSet();
        var rows = new List<FeatureRecord>(publicObjectIds.Count);
        var serializedResultBytes = 4096;
        using var cursor = table.Search(filter, false);
        while (cursor.MoveNext())
        {
            using var row = cursor.Current;
            var objectId = row.GetObjectID();
            if (!allowedObjectIds.Remove(objectId))
            {
                continue;
            }

            var values = new Dictionary<string, JsonElement?>(StringComparer.OrdinalIgnoreCase);
            foreach (var field in requestedFields)
            {
                values[field.Name] = ToJsonScalar(row[field.Name]);
            }

            var featureRecord = new FeatureRecord(objectId, values);
            serializedResultBytes += JsonSerializer.SerializeToUtf8Bytes(
                featureRecord,
                BridgeJson.Options).Length;
            if (serializedResultBytes > MaximumPublicResultBytes)
            {
                throw ResultBudgetExceeded();
            }

            rows.Add(featureRecord);
        }

        return rows.OrderBy(row => row.ObjectId).ToArray();
    }

    private static FeatureQueryResult CreatePage(
        Table table,
        QueryFilter filter,
        string layerUri,
        string objectIdField,
        IReadOnlyList<Field> requestedFields,
        int offset,
        int limit,
        long? totalCount,
        IReadOnlyList<long> pageObjectIds)
    {
        var hasMore = pageObjectIds.Count > limit;
        var publicObjectIds = pageObjectIds.Take(limit).ToArray();
        var publicRecords = ReadPublicRecords(
            table,
            filter,
            objectIdField,
            requestedFields,
            publicObjectIds);
        return new FeatureQueryResult(
            layerUri,
            offset,
            limit,
            totalCount,
            publicRecords,
            hasMore);
    }

    private static IReadOnlyList<Field> ResolveRequestedFields(
        TableDefinition definition,
        IReadOnlyList<string> requestedNames)
    {
        var available = definition.GetFields()
            .ToDictionary(field => field.Name, StringComparer.OrdinalIgnoreCase);
        var resolved = new List<Field>(requestedNames.Count);
        foreach (var requestedName in requestedNames)
        {
            if (!available.TryGetValue(requestedName, out var field))
            {
                throw FieldNotFound();
            }

            resolved.Add(field);
        }

        return resolved;
    }

    internal static Field ResolvePredicateField(
        TableDefinition definition,
        AttributePredicate predicate)
    {
        var field = definition.GetFields().FirstOrDefault(candidate =>
            string.Equals(candidate.Name, predicate.Field, StringComparison.OrdinalIgnoreCase));
        if (field is null)
        {
            throw FieldNotFound();
        }

        ValidatePredicateCompatibility(field, predicate);
        return field;
    }

    internal static void ValidatePredicateCompatibility(
        Field field,
        AttributePredicate predicate)
    {
        if (field.FieldType is FieldType.Geometry or FieldType.Blob or FieldType.Raster or FieldType.XML)
        {
            throw InvalidPredicate();
        }

        if (predicate.Operator is AttributeComparisonOperator.IsNull
            or AttributeComparisonOperator.IsNotNull)
        {
            return;
        }

        if (predicate.Value is not { } expected)
        {
            throw InvalidPredicate();
        }

        var equalityOnly = predicate.Operator is AttributeComparisonOperator.Equal
            or AttributeComparisonOperator.NotEqual;
        if (expected.ValueKind is JsonValueKind.True or JsonValueKind.False)
        {
            if (field.FieldType is not FieldType.SmallInteger || !equalityOnly)
            {
                throw InvalidPredicate();
            }

            return;
        }

        switch (field.FieldType)
        {
            case FieldType.String:
                if (expected.ValueKind is not JsonValueKind.String)
                {
                    throw InvalidPredicate();
                }

                return;
            case FieldType.GUID:
            case FieldType.GlobalID:
                if (!equalityOnly
                    || expected.ValueKind is not JsonValueKind.String
                    || !Guid.TryParse(expected.GetString(), out _))
                {
                    throw InvalidPredicate();
                }

                return;
            case FieldType.SmallInteger:
            case FieldType.Integer:
            case FieldType.OID:
            case FieldType.BigInteger:
                if (IsStringOperator(predicate.Operator)
                    || expected.ValueKind is not JsonValueKind.Number
                    || !expected.TryGetInt64(out _))
                {
                    throw InvalidPredicate();
                }

                return;
            case FieldType.Single:
            case FieldType.Double:
                if (IsStringOperator(predicate.Operator)
                    || expected.ValueKind is not JsonValueKind.Number
                    || !expected.TryGetDouble(out var expectedNumber)
                    || !double.IsFinite(expectedNumber))
                {
                    throw InvalidPredicate();
                }

                return;
            case FieldType.Date:
            case FieldType.DateOnly:
            case FieldType.TimeOnly:
            case FieldType.TimestampOffset:
                if (IsStringOperator(predicate.Operator)
                    || expected.ValueKind is not JsonValueKind.String
                    || !IsValidTemporalLiteral(field.FieldType, expected.GetString()!))
                {
                    throw InvalidPredicate();
                }

                return;
            default:
                throw InvalidPredicate();
        }
    }

    private static int CompareTyped(object actual, Field field, JsonElement expected) =>
        field.FieldType switch
        {
            FieldType.String => string.Compare(
                actual as string ?? throw InvalidPredicate(),
                expected.GetString()!,
                StringComparison.OrdinalIgnoreCase),
            FieldType.GUID or FieldType.GlobalID => ReadGuid(actual).CompareTo(
                Guid.Parse(expected.GetString()!)),
            FieldType.SmallInteger when expected.ValueKind is JsonValueKind.True or JsonValueKind.False =>
                ReadBoolean(actual).CompareTo(expected.GetBoolean()),
            FieldType.SmallInteger or FieldType.Integer or FieldType.OID or FieldType.BigInteger =>
                ReadInteger(actual).CompareTo(expected.GetInt64()),
            FieldType.Single or FieldType.Double => ReadFloatingPoint(actual).CompareTo(
                expected.GetDouble()),
            FieldType.Date => ReadDateTime(actual).CompareTo(DateTime.Parse(
                expected.GetString()!,
                CultureInfo.InvariantCulture,
                DateTimeStyles.RoundtripKind)),
            FieldType.DateOnly => ReadDateOnly(actual).CompareTo(DateOnly.Parse(
                expected.GetString()!,
                CultureInfo.InvariantCulture,
                DateTimeStyles.None)),
            FieldType.TimeOnly => ReadTimeOnly(actual).CompareTo(TimeOnly.Parse(
                expected.GetString()!,
                CultureInfo.InvariantCulture,
                DateTimeStyles.None)),
            FieldType.TimestampOffset => ReadTimestampOffset(actual).CompareTo(DateTimeOffset.Parse(
                expected.GetString()!,
                CultureInfo.InvariantCulture,
                DateTimeStyles.RoundtripKind)),
            _ => throw InvalidPredicate(),
        };

    private static bool IsStringOperator(AttributeComparisonOperator comparisonOperator) =>
        comparisonOperator is AttributeComparisonOperator.StartsWith
            or AttributeComparisonOperator.Contains;

    private static bool IsValidTemporalLiteral(FieldType fieldType, string value) =>
        fieldType switch
        {
            FieldType.Date => DateTime.TryParse(
                value,
                CultureInfo.InvariantCulture,
                DateTimeStyles.RoundtripKind,
                out _),
            FieldType.DateOnly => DateOnly.TryParse(
                value,
                CultureInfo.InvariantCulture,
                DateTimeStyles.None,
                out _),
            FieldType.TimeOnly => TimeOnly.TryParse(
                value,
                CultureInfo.InvariantCulture,
                DateTimeStyles.None,
                out _),
            FieldType.TimestampOffset => DateTimeOffset.TryParse(
                value,
                CultureInfo.InvariantCulture,
                DateTimeStyles.RoundtripKind,
                out _),
            _ => false,
        };

    private static Guid ReadGuid(object actual)
    {
        if (actual is Guid guid)
        {
            return guid;
        }

        if (actual is string text && Guid.TryParse(text, out guid))
        {
            return guid;
        }

        throw InvalidPredicate();
    }

    private static bool ReadBoolean(object actual)
    {
        if (actual is bool boolean)
        {
            return boolean;
        }

        try
        {
            var value = Convert.ToInt64(actual, CultureInfo.InvariantCulture);
            return value switch
            {
                0 => false,
                1 => true,
                _ => throw InvalidPredicate(),
            };
        }
        catch (ArcGisOperationException)
        {
            throw;
        }
        catch
        {
            throw InvalidPredicate();
        }
    }

    private static long ReadInteger(object actual)
    {
        try
        {
            return Convert.ToInt64(actual, CultureInfo.InvariantCulture);
        }
        catch
        {
            throw InvalidPredicate();
        }
    }

    private static double ReadFloatingPoint(object actual)
    {
        try
        {
            var value = Convert.ToDouble(actual, CultureInfo.InvariantCulture);
            return double.IsFinite(value) ? value : throw InvalidPredicate();
        }
        catch (ArcGisOperationException)
        {
            throw;
        }
        catch
        {
            throw InvalidPredicate();
        }
    }

    private static DateTime ReadDateTime(object actual) => actual switch
    {
        DateTime value => value,
        string text when DateTime.TryParse(
            text,
            CultureInfo.InvariantCulture,
            DateTimeStyles.RoundtripKind,
            out var value) => value,
        _ => throw InvalidPredicate(),
    };

    private static DateOnly ReadDateOnly(object actual) => actual switch
    {
        DateOnly value => value,
        string text when DateOnly.TryParse(
            text,
            CultureInfo.InvariantCulture,
            DateTimeStyles.None,
            out var value) => value,
        _ => throw InvalidPredicate(),
    };

    private static TimeOnly ReadTimeOnly(object actual) => actual switch
    {
        TimeOnly value => value,
        string text when TimeOnly.TryParse(
            text,
            CultureInfo.InvariantCulture,
            DateTimeStyles.None,
            out var value) => value,
        _ => throw InvalidPredicate(),
    };

    private static DateTimeOffset ReadTimestampOffset(object actual) => actual switch
    {
        DateTimeOffset value => value,
        string text when DateTimeOffset.TryParse(
            text,
            CultureInfo.InvariantCulture,
            DateTimeStyles.RoundtripKind,
            out var value) => value,
        _ => throw InvalidPredicate(),
    };

    private static JsonElement? ToJsonScalar(object? value)
    {
        if (value is null or DBNull)
        {
            return null;
        }

        object scalar = value switch
        {
            string text => text.Length <= GisContractGuards.MaximumPublicStringLength
                ? text
                : text[..GisContractGuards.MaximumPublicStringLength],
            char character => character.ToString(),
            float single when !float.IsFinite(single) => "<unsupported:non-finite-number>",
            double number when !double.IsFinite(number) => "<unsupported:non-finite-number>",
            bool or byte or sbyte or short or ushort or int or uint or long or ulong
                or float or double or decimal => value,
            DateTime dateTime => dateTime.ToString("O", CultureInfo.InvariantCulture),
            DateTimeOffset dateTimeOffset => dateTimeOffset.ToString("O", CultureInfo.InvariantCulture),
            DateOnly date => date.ToString("O", CultureInfo.InvariantCulture),
            TimeOnly time => time.ToString("O", CultureInfo.InvariantCulture),
            Guid guid => guid.ToString("D"),
            Geometry => "<unsupported:geometry>",
            byte[] or Stream => "<unsupported:blob>",
            _ => "<unsupported:value>",
        };
        return JsonSerializer.SerializeToElement(scalar, BridgeJson.Options);
    }

    internal static Table OpenTable(BasicFeatureLayer layer)
    {
        try
        {
            return layer.GetTable() ?? throw new InvalidOperationException();
        }
        catch
        {
            throw new ArcGisOperationException(
                BridgeErrorCodes.DataSourceUnavailable,
                "The layer data source is unavailable.");
        }
    }

    private static bool SupportsPagination(Table table)
    {
        try
        {
            using var datastore = table.GetDatastore();
            return datastore.GetDatastoreProperties().SupportsQueryPagination;
        }
        catch
        {
            return false;
        }
    }

    private static ArcGisOperationException FieldNotFound() =>
        new(BridgeErrorCodes.FieldNotFound, "A requested field was not found.");

    private static ArcGisOperationException InvalidPredicate() =>
        new(BridgeErrorCodes.InvalidPredicate, "The typed predicate is not valid for this field.");

    private static ArcGisOperationException ProjectionFailed(string failureCode) =>
        new(
            failureCode,
            string.Equals(failureCode, BridgeErrorCodes.InvalidExtent, StringComparison.Ordinal)
                ? "The extent could not be projected to the target spatial reference."
                : "The spatial source could not be projected to the target spatial reference.");

    private static ArcGisOperationException ResultBudgetExceeded() =>
        new(
            BridgeErrorCodes.RequestTooLarge,
            "The query result exceeds the public response budget.");

    private static ArcGisOperationException FallbackLimitExceeded() =>
        new(
            BridgeErrorCodes.RequestTooLarge,
            "The data source cannot serve this page within the bounded fallback limit.");
}
