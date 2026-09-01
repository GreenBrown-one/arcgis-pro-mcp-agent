using System.IO;

namespace ArcGISProAgent.AddIn;

internal static class RuntimeCredentialLocator
{
    private const string RuntimeFileEnvironmentVariable = "ARCGIS_AGENT_RUNTIME_FILE";

    internal static string GetPath()
    {
        var configured = Environment.GetEnvironmentVariable(RuntimeFileEnvironmentVariable);
        if (!string.IsNullOrWhiteSpace(configured))
        {
            return Path.GetFullPath(configured);
        }

        return Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "ArcGISProAgent",
            "runtime",
            "bridge.json");
    }
}
