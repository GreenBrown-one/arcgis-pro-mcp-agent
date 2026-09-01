using System.Text.Json;
using ArcGISProAgent.Contracts;

namespace ArcGISProAgent.Contracts.Tests;

public sealed class GisContractTests
{
    public static IEnumerable<object[]> RequestAndResultContracts()
    {
        var extent = new MapExtent(1, 2, 3, 4, 3857);
        var predicate = new AttributePredicate(
            "STATUS",
            AttributeComparisonOperator.Equal,
            Scalar("Open"));
        var spatialSource = new SpatialQuerySource(
            SpatialQuerySourceKind.Extent,
            null,
            extent);
        var feature = new FeatureRecord(
            7,
            new Dictionary<string, JsonElement?>
            {
                ["NAME"] = Scalar("Main"),
            });

        object[] contracts =
        [
            new ContextDescribeArguments(),
            new ContextDescription(
                new ProjectSummary(
                    "Demo",
                    "C:\\projects\\demo.aprx",
                    false,
                    [new ProjectItemSummary("item://map/1", "Map", ProjectItemKind.Map, true)]),
                new ActiveViewSummary("item://map/1", "Map", ProjectItemKind.Map, extent)),
            new ListLayersArguments(),
            new LayerListResult(
                [new LayerSummary(
                    "layer://roads",
                    "Roads",
                    "Group\\Roads",
                    "FeatureLayer",
                    "layer://group",
                    1,
                    true,
                    true)]),
            new DescribeLayerArguments("layer://roads"),
            new LayerDescription(
                "layer://roads",
                "Roads",
                "FeatureLayer",
                "FileGeodatabase",
                "C:\\data\\roads.gdb",
                "Polyline",
                new SpatialReferenceSummary(3857, "WGS 84 / Pseudo-Mercator"),
                "Connected",
                123L),
            new ListFieldsArguments("layer://roads"),
            new LayerFieldsResult(
                "layer://roads",
                [new FieldSummary("NAME", "Name", "String", true, true, null)]),
            new FeatureCountArguments("layer://roads", predicate),
            new FeatureCountResult("layer://roads", 123L),
            new FeatureQueryArguments("layer://roads", ["NAME"], predicate),
            new FeatureQueryResult("layer://roads", 0, 20, 123L, [feature], true),
            new SpatialQueryArguments(
                "layer://roads",
                spatialSource,
                SpatialRelation.Intersects,
                ["NAME"]),
            new SelectionDescribeArguments(),
            new SelectionDescription(
                [new LayerSelectionSummary("layer://roads", 3L, [1L, 2L, 3L], false)]),
            new SelectByAttributeArguments("layer://roads", predicate),
            new SelectByLocationArguments(
                "layer://roads",
                spatialSource,
                SpatialRelation.Intersects),
            new SelectionResult("layer://roads", 3L),
            new ClearSelectionArguments(),
            new ClearSelectionResult(2, 15L),
            new ActivateViewArguments("item://map/1"),
            new ActivateViewResult("item://map/1", true),
            new ZoomToLayerArguments("layer://roads"),
            new ZoomToExtentArguments(extent),
            new ZoomResult(true),
            new FlashFeaturesArguments("layer://roads", [1L, 2L]),
            new FlashFeaturesResult(true, 2),
        ];

        return contracts.Select(contract => new[] { contract });
    }

    [Theory]
    [MemberData(nameof(RequestAndResultContracts))]
    public void Request_and_result_contracts_round_trip_as_JSON(object contract)
    {
        var json = JsonSerializer.Serialize(contract, contract.GetType(), BridgeJson.Options);
        var restored = JsonSerializer.Deserialize(json, contract.GetType(), BridgeJson.Options);

        Assert.NotNull(restored);
        Assert.Equal(
            json,
            JsonSerializer.Serialize(restored, restored.GetType(), BridgeJson.Options));
    }

    [Fact]
    public void Request_defaults_are_bounded_and_conservative()
    {
        Assert.True(new ListLayersArguments().IncludeNested);
        Assert.Equal(20, new FeatureQueryArguments("layer://roads", ["NAME"]).Limit);
        Assert.Equal(0, new FeatureQueryArguments("layer://roads", ["NAME"]).Offset);
        Assert.Equal(20, new SpatialQueryArguments(
            "layer://roads",
            new SpatialQuerySource(SpatialQuerySourceKind.CurrentView, null, null),
            SpatialRelation.Intersects,
            ["NAME"]).Limit);
        Assert.Equal(20, new SelectionDescribeArguments().ObjectIdLimit);
        Assert.Equal(SelectionCombinationMode.Replace,
            new SelectByAttributeArguments(
                "layer://roads",
                new AttributePredicate(
                    "STATUS",
                    AttributeComparisonOperator.IsNotNull,
                    null)).Mode);
        Assert.Equal(1000,
            new FlashFeaturesArguments("layer://roads", [1L]).DurationMilliseconds);
    }

    [Theory]
    [InlineData(AttributeComparisonOperator.Equal)]
    [InlineData(AttributeComparisonOperator.NotEqual)]
    [InlineData(AttributeComparisonOperator.GreaterThan)]
    [InlineData(AttributeComparisonOperator.GreaterThanOrEqual)]
    [InlineData(AttributeComparisonOperator.LessThan)]
    [InlineData(AttributeComparisonOperator.LessThanOrEqual)]
    [InlineData(AttributeComparisonOperator.StartsWith)]
    [InlineData(AttributeComparisonOperator.Contains)]
    [InlineData(AttributeComparisonOperator.IsNull)]
    [InlineData(AttributeComparisonOperator.IsNotNull)]
    public void Every_attribute_operator_round_trips(AttributeComparisonOperator value) =>
        AssertEnumRoundTrip(value);

    [Theory]
    [InlineData(SpatialRelation.Intersects)]
    [InlineData(SpatialRelation.Within)]
    [InlineData(SpatialRelation.Contains)]
    [InlineData(SpatialRelation.Touches)]
    [InlineData(SpatialRelation.Crosses)]
    [InlineData(SpatialRelation.Overlaps)]
    public void Every_spatial_relation_round_trips(SpatialRelation value) =>
        AssertEnumRoundTrip(value);

    [Theory]
    [InlineData(SpatialQuerySourceKind.Layer)]
    [InlineData(SpatialQuerySourceKind.Extent)]
    [InlineData(SpatialQuerySourceKind.CurrentView)]
    public void Every_spatial_source_kind_round_trips(SpatialQuerySourceKind value) =>
        AssertEnumRoundTrip(value);

    [Theory]
    [InlineData(SelectionCombinationMode.Replace)]
    [InlineData(SelectionCombinationMode.Add)]
    [InlineData(SelectionCombinationMode.Remove)]
    [InlineData(SelectionCombinationMode.Toggle)]
    public void Every_selection_combination_mode_round_trips(SelectionCombinationMode value) =>
        AssertEnumRoundTrip(value);

    [Fact]
    public void Public_contracts_do_not_expose_raw_SQL_or_expression_properties()
    {
        var publicProperties = typeof(AttributePredicate).Assembly
            .GetExportedTypes()
            .SelectMany(type => type.GetProperties())
            .Select(property => property.Name)
            .ToArray();

        Assert.DoesNotContain(publicProperties,
            name => name.Contains("Sql", StringComparison.OrdinalIgnoreCase)
                || name.Contains("Expression", StringComparison.OrdinalIgnoreCase)
                || name.Contains("Wkt", StringComparison.OrdinalIgnoreCase)
                || name.Contains("Script", StringComparison.OrdinalIgnoreCase));
    }

    [Fact]
    public void Attribute_predicates_accept_only_allowlisted_operators_and_scalar_values()
    {
        GisContractGuards.Validate(new AttributePredicate(
            "STATUS",
            AttributeComparisonOperator.Equal,
            Scalar("Open")));
        GisContractGuards.Validate(new AttributePredicate(
            "COUNT",
            AttributeComparisonOperator.GreaterThan,
            Scalar(2)));
        GisContractGuards.Validate(new AttributePredicate(
            "ACTIVE",
            AttributeComparisonOperator.Equal,
            Scalar(true)));
        GisContractGuards.Validate(new AttributePredicate(
            "STATUS",
            AttributeComparisonOperator.IsNull,
            null));

        Assert.Throws<ArgumentOutOfRangeException>(() => GisContractGuards.Validate(
            new AttributePredicate("STATUS", (AttributeComparisonOperator)999, Scalar("Open"))));
        Assert.Throws<ArgumentException>(() => GisContractGuards.Validate(
            new AttributePredicate("STATUS", AttributeComparisonOperator.Equal, null)));
        Assert.Throws<ArgumentException>(() => GisContractGuards.Validate(
            new AttributePredicate("STATUS", AttributeComparisonOperator.IsNull, Scalar("Open"))));
        Assert.Throws<ArgumentException>(() => GisContractGuards.Validate(
            new AttributePredicate("STATUS", AttributeComparisonOperator.Equal, Json("{}"))));
        Assert.Throws<ArgumentException>(() => GisContractGuards.Validate(
            new AttributePredicate("STATUS", AttributeComparisonOperator.Equal, Json("[1]"))));
    }

    [Fact]
    public void Public_strings_are_required_and_capped_at_2000_characters()
    {
        GisContractGuards.Validate(new DescribeLayerArguments(new string('u', 2000)));

        Assert.Throws<ArgumentException>(() =>
            GisContractGuards.Validate(new DescribeLayerArguments(" ")));
        Assert.Throws<ArgumentOutOfRangeException>(() =>
            GisContractGuards.Validate(new DescribeLayerArguments(new string('u', 2001))));
        Assert.Throws<ArgumentOutOfRangeException>(() =>
            GisContractGuards.Validate(new AttributePredicate(
                "NAME",
                AttributeComparisonOperator.Equal,
                Scalar(new string('v', 2001)))));
    }

    [Fact]
    public void Stable_item_and_layer_URI_fields_are_required()
    {
        Assert.Throws<ArgumentException>(() =>
            GisContractGuards.Validate(new ActivateViewArguments("")));
        Assert.Throws<ArgumentException>(() =>
            GisContractGuards.Validate(new ListFieldsArguments(" ")));
        Assert.Throws<ArgumentException>(() =>
            GisContractGuards.Validate(new ZoomToLayerArguments(null!)));
    }

    [Theory]
    [InlineData(double.NaN, 0, 1, 1)]
    [InlineData(0, double.PositiveInfinity, 1, 1)]
    [InlineData(2, 0, 1, 1)]
    [InlineData(0, 2, 1, 1)]
    [InlineData(1, 0, 1, 1)]
    [InlineData(0, 1, 1, 1)]
    public void Extents_require_finite_normalized_coordinates(
        double xMin,
        double yMin,
        double xMax,
        double yMax)
    {
        Assert.Throws<ArgumentException>(() =>
            GisContractGuards.Validate(new MapExtent(xMin, yMin, xMax, yMax, null)));
    }

    [Fact]
    public void Spatial_inputs_accept_only_allowlisted_relations_and_well_formed_sources()
    {
        var currentView = new SpatialQuerySource(
            SpatialQuerySourceKind.CurrentView,
            null,
            null);
        GisContractGuards.Validate(currentView);

        Assert.Throws<ArgumentOutOfRangeException>(() => GisContractGuards.Validate(
            (SpatialRelation)999));
        Assert.Throws<ArgumentOutOfRangeException>(() => GisContractGuards.Validate(
            new SpatialQuerySource((SpatialQuerySourceKind)999, null, null)));
        Assert.Throws<ArgumentException>(() => GisContractGuards.Validate(
            new SpatialQuerySource(SpatialQuerySourceKind.Layer, null, null)));
        Assert.Throws<ArgumentException>(() => GisContractGuards.Validate(
            new SpatialQuerySource(
                SpatialQuerySourceKind.Extent,
                "layer://roads",
                new MapExtent(0, 0, 1, 1, null))));
        Assert.Throws<ArgumentException>(() => GisContractGuards.Validate(
            new SpatialQuerySource(
                SpatialQuerySourceKind.CurrentView,
                "layer://roads",
                null)));
    }

    [Fact]
    public void Selection_modes_accept_only_allowlisted_values()
    {
        GisContractGuards.Validate(SelectionCombinationMode.Toggle);

        Assert.Throws<ArgumentOutOfRangeException>(() =>
            GisContractGuards.Validate((SelectionCombinationMode)999));
    }

    [Theory]
    [InlineData(0)]
    [InlineData(101)]
    public void Query_and_selection_limits_stay_within_one_and_one_hundred(int limit)
    {
        Assert.Throws<ArgumentOutOfRangeException>(() => GisContractGuards.Validate(
            new FeatureQueryArguments("layer://roads", ["NAME"], Limit: limit)));
        Assert.Throws<ArgumentOutOfRangeException>(() => GisContractGuards.Validate(
            new SelectionDescribeArguments(ObjectIdLimit: limit)));
    }

    [Fact]
    public void Object_ID_collections_are_required_unique_positive_and_capped_at_one_hundred()
    {
        GisContractGuards.Validate(new FlashFeaturesArguments(
            "layer://roads",
            Enumerable.Range(1, 100).Select(value => (long)value).ToArray()));

        Assert.Throws<ArgumentException>(() => GisContractGuards.Validate(
            new FlashFeaturesArguments("layer://roads", [])));
        Assert.Throws<ArgumentOutOfRangeException>(() => GisContractGuards.Validate(
            new FlashFeaturesArguments(
                "layer://roads",
                Enumerable.Range(1, 101).Select(value => (long)value).ToArray())));
        Assert.Throws<ArgumentOutOfRangeException>(() => GisContractGuards.Validate(
            new FlashFeaturesArguments("layer://roads", [0L])));
        Assert.Throws<ArgumentException>(() => GisContractGuards.Validate(
            new FlashFeaturesArguments("layer://roads", [1L, 1L])));
    }

    [Fact]
    public void Requested_fields_are_required_and_cannot_contain_duplicates()
    {
        Assert.Throws<ArgumentException>(() => GisContractGuards.Validate(
            new FeatureQueryArguments("layer://roads", [])));
        Assert.Throws<ArgumentException>(() => GisContractGuards.Validate(
            new FeatureQueryArguments("layer://roads", ["NAME", "name"])));
        Assert.Throws<ArgumentException>(() => GisContractGuards.Validate(
            new SpatialQueryArguments(
                "layer://roads",
                new SpatialQuerySource(SpatialQuerySourceKind.CurrentView, null, null),
                SpatialRelation.Intersects,
                ["NAME", "NAME"])));
    }

    [Fact]
    public void Operation_catalog_contains_every_phase_two_operation_once()
    {
        var expected = new Dictionary<string, RiskLevel>
        {
            ["context.describe"] = RiskLevel.R0,
            ["layers.list"] = RiskLevel.R0,
            ["layers.describe"] = RiskLevel.R0,
            ["layers.fields"] = RiskLevel.R0,
            ["query.feature_count"] = RiskLevel.R0,
            ["query.features"] = RiskLevel.R0,
            ["query.spatial"] = RiskLevel.R0,
            ["selection.describe"] = RiskLevel.R0,
            ["selection.by_attribute"] = RiskLevel.R1,
            ["selection.by_location"] = RiskLevel.R1,
            ["selection.clear"] = RiskLevel.R1,
            ["map_view.activate"] = RiskLevel.R1,
            ["map_view.zoom_to_layer"] = RiskLevel.R1,
            ["map_view.zoom_to_extent"] = RiskLevel.R1,
            ["map_view.flash_features"] = RiskLevel.R1,
        };

        Assert.Equal(expected.Count, OperationCatalog.Phase2.Count);
        Assert.Equal(expected.Keys.Order(), OperationCatalog.Phase2.Select(item => item.Id).Order());
        Assert.All(OperationCatalog.Phase2, item => Assert.Equal(expected[item.Id], item.Risk));
        Assert.Equal(OperationCatalog.Phase2.Count,
            OperationCatalog.Phase2.Select(item => item.Id).Distinct(StringComparer.Ordinal).Count());
    }

    [Fact]
    public void Operation_catalog_reports_truthful_support_and_mutation_metadata()
    {
        var nonIdempotent = new HashSet<string>(StringComparer.Ordinal)
        {
            "selection.by_attribute",
            "selection.by_location",
            "map_view.flash_features",
        };

        Assert.All(OperationCatalog.Phase2, item =>
        {
            Assert.False(item.SupportsCancellation);
            Assert.False(item.SupportsPreview);
            Assert.False(item.SupportsUndo);
            Assert.False(item.SupportsBackup);
            Assert.False(item.ModifiesProject);
            Assert.False(item.ModifiesData);
            Assert.False(item.ModifiesFileSystem);
            Assert.Equal("3.7", item.MinimumArcGisProVersion);
            Assert.False(string.IsNullOrWhiteSpace(item.DisplayName));
            Assert.False(string.IsNullOrWhiteSpace(item.Module));
            Assert.Equal(!nonIdempotent.Contains(item.Id), item.IsIdempotent);
        });
    }

    [Fact]
    public void Foundation_and_complete_catalogs_are_exposed_without_duplicates()
    {
        var health = Assert.Single(OperationCatalog.Foundation);

        Assert.Equal("connection.health", health.Id);
        Assert.True(health.SupportsCancellation);
        Assert.Equal(1 + OperationCatalog.Phase2.Count, OperationCatalog.All.Count);
        Assert.Equal(OperationCatalog.All.Count,
            OperationCatalog.All.Select(item => item.Id).Distinct(StringComparer.Ordinal).Count());
    }

    [Fact]
    public void Capability_descriptor_new_metadata_has_backward_compatible_defaults()
    {
        var descriptor = new CapabilityDescriptor(
            "connection.health",
            "1.0",
            RiskLevel.R0,
            true,
            false,
            false,
            false);

        Assert.Equal(string.Empty, descriptor.DisplayName);
        Assert.Equal(string.Empty, descriptor.Module);
        Assert.False(descriptor.ModifiesProject);
        Assert.False(descriptor.ModifiesData);
        Assert.False(descriptor.ModifiesFileSystem);
        Assert.Equal("3.7", descriptor.MinimumArcGisProVersion);
        Assert.True(descriptor.IsIdempotent);
    }

    [Fact]
    public void Legacy_capability_descriptor_JSON_uses_new_metadata_defaults()
    {
        const string legacyJson = """
            {
              "id": "connection.health",
              "version": "1.0",
              "risk": 0,
              "supportsCancellation": true,
              "supportsPreview": false,
              "supportsUndo": false,
              "supportsBackup": false
            }
            """;

        var descriptor = JsonSerializer.Deserialize<CapabilityDescriptor>(
            legacyJson,
            BridgeJson.Options)!;

        Assert.Equal("connection.health", descriptor.Id);
        Assert.Equal("1.0", descriptor.Version);
        Assert.Equal(RiskLevel.R0, descriptor.Risk);
        Assert.True(descriptor.SupportsCancellation);
        Assert.False(descriptor.SupportsPreview);
        Assert.False(descriptor.SupportsUndo);
        Assert.False(descriptor.SupportsBackup);
        Assert.Equal(string.Empty, descriptor.DisplayName);
        Assert.Equal(string.Empty, descriptor.Module);
        Assert.False(descriptor.ModifiesProject);
        Assert.False(descriptor.ModifiesData);
        Assert.False(descriptor.ModifiesFileSystem);
        Assert.Equal("3.7", descriptor.MinimumArcGisProVersion);
        Assert.True(descriptor.IsIdempotent);
    }

    [Fact]
    public void Every_count_bearing_public_result_property_uses_a_64_bit_integer()
    {
        Type[] publicResultDtos =
        [
            typeof(ProjectItemSummary),
            typeof(ProjectSummary),
            typeof(ActiveViewSummary),
            typeof(ContextDescription),
            typeof(ActivateViewResult),
            typeof(ZoomResult),
            typeof(FlashFeaturesResult),
            typeof(LayerSummary),
            typeof(LayerListResult),
            typeof(SpatialReferenceSummary),
            typeof(LayerDescription),
            typeof(FieldSummary),
            typeof(LayerFieldsResult),
            typeof(FeatureCountResult),
            typeof(FeatureRecord),
            typeof(FeatureQueryResult),
            typeof(LayerSelectionSummary),
            typeof(SelectionDescription),
            typeof(SelectionResult),
            typeof(ClearSelectionResult),
        ];
        var expected = new Dictionary<(Type Type, string Property), Type>
        {
            [(typeof(FlashFeaturesResult), nameof(FlashFeaturesResult.FlashedCount))] =
                typeof(long),
            [(typeof(LayerDescription), nameof(LayerDescription.FeatureCount))] =
                typeof(long?),
            [(typeof(FeatureCountResult), nameof(FeatureCountResult.Count))] =
                typeof(long),
            [(typeof(FeatureQueryResult), nameof(FeatureQueryResult.TotalCount))] =
                typeof(long?),
            [(typeof(LayerSelectionSummary), nameof(LayerSelectionSummary.Count))] =
                typeof(long),
            [(typeof(SelectionResult), nameof(SelectionResult.SelectedCount))] =
                typeof(long),
            [(typeof(ClearSelectionResult), nameof(ClearSelectionResult.LayersCleared))] =
                typeof(long),
            [(typeof(ClearSelectionResult), nameof(ClearSelectionResult.FeaturesCleared))] =
                typeof(long),
        };

        var actual = publicResultDtos
            .SelectMany(type => type.GetProperties().Select(property => (Type: type, Property: property)))
            .Where(item => item.Property.Name.Contains("Count", StringComparison.Ordinal)
                || item.Property.Name.EndsWith("Cleared", StringComparison.Ordinal))
            .ToDictionary(
                item => (Type: item.Type, Property: item.Property.Name),
                item => item.Property.PropertyType);

        Assert.Equal(
            expected.Keys.OrderBy(item => item.Type.Name).ThenBy(item => item.Property),
            actual.Keys.OrderBy(item => item.Type.Name).ThenBy(item => item.Property));
        Assert.All(expected, item => Assert.Equal(item.Value, actual[item.Key]));
    }

    private static JsonElement Scalar<T>(T value) =>
        JsonSerializer.SerializeToElement(value, BridgeJson.Options);

    private static JsonElement Json(string value) =>
        JsonDocument.Parse(value).RootElement.Clone();

    private static void AssertEnumRoundTrip<T>(T value) where T : struct, Enum
    {
        var json = JsonSerializer.Serialize(value, BridgeJson.Options);
        var restored = JsonSerializer.Deserialize<T>(json, BridgeJson.Options);

        Assert.Equal(value, restored);
    }
}
