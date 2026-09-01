using ArcGIS.Core.Geometry;
using ArcGIS.Desktop.Core;
using ArcGIS.Desktop.Layouts;
using ArcGIS.Desktop.Mapping;
using ArcGISProAgent.Contracts;

namespace ArcGISProAgent.AddIn.Operations;

internal static class ContextOperations
{
    internal static ContextDescription Describe()
    {
        var project = ArcGisObjectResolver.RequireProject();
        var activeMapView = MapView.Active;
        var activeMap = activeMapView?.Map;
        var activeLayout = LayoutView.Active?.Layout;
        var items = new List<ProjectItemSummary>();

        foreach (var item in project.GetItems<MapProjectItem>())
        {
            var map = item.GetMap();
            var kind = IsMap(map) ? ProjectItemKind.Map : ProjectItemKind.Scene;
            items.Add(new ProjectItemSummary(
                map.URI,
                map.Name,
                kind,
                string.Equals(activeMap?.URI, map.URI, StringComparison.Ordinal)));
        }

        foreach (var item in project.GetItems<LayoutProjectItem>())
        {
            var layout = item.GetLayout();
            items.Add(new ProjectItemSummary(
                layout.URI,
                layout.Name,
                ProjectItemKind.Layout,
                string.Equals(activeLayout?.URI, layout.URI, StringComparison.Ordinal)));
        }

        var activeView = activeLayout is not null
            ? new ActiveViewSummary(
                activeLayout.URI,
                activeLayout.Name,
                ProjectItemKind.Layout,
                Extent: null)
            : CreateActiveMapSummary(activeMapView);

        return new ContextDescription(
            new ProjectSummary(
                project.Name,
                project.Path,
                project.IsDirty,
                items.OrderBy(item => item.Uri, StringComparer.Ordinal).ToArray()),
            activeView);
    }

    private static ActiveViewSummary? CreateActiveMapSummary(MapView? view)
    {
        if (view?.Map is not { } map)
        {
            return null;
        }

        return new ActiveViewSummary(
            map.URI,
            map.Name,
            IsMap(map) ? ProjectItemKind.Map : ProjectItemKind.Scene,
            ToMapExtent(view.Extent));
    }

    internal static MapExtent? ToMapExtent(Envelope? envelope)
    {
        if (envelope is null || envelope.IsEmpty)
        {
            return null;
        }

        var wkid = envelope.SpatialReference?.Wkid;
        return new MapExtent(
            envelope.XMin,
            envelope.YMin,
            envelope.XMax,
            envelope.YMax,
            wkid is > 0 ? wkid : null);
    }

    private static bool IsMap(Map map) =>
        string.Equals(map.MapType.ToString(), "Map", StringComparison.Ordinal);
}
