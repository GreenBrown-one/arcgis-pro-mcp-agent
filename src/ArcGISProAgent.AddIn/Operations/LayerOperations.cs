using System.Text.RegularExpressions;
using ArcGIS.Core.Data;
using ArcGIS.Desktop.Mapping;
using ArcGISProAgent.Contracts;

namespace ArcGISProAgent.AddIn.Operations;

internal static partial class LayerOperations
{
    internal static LayerListResult List(ListLayersArguments arguments)
    {
        var map = ArcGisObjectResolver.RequireActiveMap();
        var layers = arguments.IncludeNested
            ? map.GetLayersAsFlattenedList()
            : map.Layers;

        return new LayerListResult(layers.Select(CreateSummary).ToArray());
    }

    internal static LayerDescription Describe(DescribeLayerArguments arguments)
    {
        var resolved = ArcGisObjectResolver.ResolveLayer(arguments.LayerUri);
        if (resolved.Layer is not BasicFeatureLayer basicLayer)
        {
            return new LayerDescription(
                resolved.Layer.URI,
                resolved.Layer.Name,
                resolved.Layer.GetType().Name,
                SourceType: null,
                SourcePath: null,
                GeometryType: null,
                SpatialReference: null,
                ConnectionStatus: "not_applicable",
                FeatureCount: null);
        }

        try
        {
            using var table = basicLayer.GetTable();
            if (table is null)
            {
                throw DataSourceUnavailable();
            }

            using var definition = table.GetDefinition();
            using var datastore = table.GetDatastore();
            var featureDefinition = definition as FeatureClassDefinition;
            var spatialReference = featureDefinition?.GetSpatialReference();
            var datasetPath = table.GetPath();
            var sourcePath = datasetPath is null
                ? SanitizeSourcePath(datastore.GetPath())
                : SanitizeSourcePath(datasetPath);

            return new LayerDescription(
                resolved.Layer.URI,
                resolved.Layer.Name,
                resolved.Layer.GetType().Name,
                datastore.GetType().Name,
                sourcePath,
                featureDefinition?.GetShapeType().ToString(),
                spatialReference is null
                    ? null
                    : new SpatialReferenceSummary(
                        spatialReference.Wkid > 0 ? spatialReference.Wkid : null,
                        spatialReference.Name),
                "available",
                table.GetCount());
        }
        catch (ArcGisOperationException)
        {
            throw;
        }
        catch
        {
            throw DataSourceUnavailable();
        }
    }

    internal static LayerFieldsResult ListFields(ListFieldsArguments arguments)
    {
        var basicLayer = ArcGisObjectResolver.RequireBasicFeatureLayer(
            ArcGisObjectResolver.ResolveLayer(arguments.LayerUri));

        try
        {
            using var table = basicLayer.GetTable();
            if (table is null)
            {
                throw DataSourceUnavailable();
            }

            using var definition = table.GetDefinition();
            var fields = new List<FieldSummary>();
            foreach (var field in definition.GetFields())
            {
                using var domain = field.GetDomain(null);
                fields.Add(new FieldSummary(
                    field.Name,
                    field.AliasName ?? field.Name,
                    field.FieldType.ToString(),
                    field.IsNullable,
                    field.IsEditable,
                    domain?.GetName()));
            }

            return new LayerFieldsResult(arguments.LayerUri, fields);
        }
        catch (ArcGisOperationException)
        {
            throw;
        }
        catch
        {
            throw DataSourceUnavailable();
        }
    }

    internal static LayerSummary CreateSummary(Layer layer)
    {
        var ancestors = new Stack<Layer>();
        var current = layer.Parent as Layer;
        while (current is not null)
        {
            ancestors.Push(current);
            current = current.Parent as Layer;
        }

        var longNameParts = ancestors.Select(parent => parent.Name).Append(layer.Name);
        var parent = layer.Parent as Layer;
        return new LayerSummary(
            layer.URI,
            layer.Name,
            string.Join("\\", longNameParts),
            layer.GetType().Name,
            parent?.URI,
            ancestors.Count,
            layer.IsVisible,
            layer is FeatureLayer);
    }

    internal static string? SanitizeSourcePath(Uri? path)
    {
        if (path is null || !path.IsAbsoluteUri || !string.IsNullOrEmpty(path.UserInfo))
        {
            return null;
        }

        var source = path.IsFile
            ? path.LocalPath
            : path.GetComponents(
                UriComponents.SchemeAndServer | UriComponents.Path,
                UriFormat.SafeUnescaped);
        if (CredentialPattern().IsMatch(source))
        {
            return null;
        }

        return source.Length <= GisContractGuards.MaximumPublicStringLength
            ? source
            : source[..GisContractGuards.MaximumPublicStringLength];
    }

    private static ArcGisOperationException DataSourceUnavailable() =>
        new(
            BridgeErrorCodes.DataSourceUnavailable,
            "The layer data source is unavailable.");

    [GeneratedRegex(
        "(?i)(password|pwd|token|secret|credential|user\\s*id|uid)\\s*=",
        RegexOptions.CultureInvariant)]
    private static partial Regex CredentialPattern();
}
