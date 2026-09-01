using System.Text.Json;
using ArcGISProAgent.Contracts;

namespace ArcGISProAgent.Bridge;

public sealed record RuntimeCredentials(string PipeName, string Token)
{
    public static RuntimeCredentials Load(string path)
    {
        var json = File.ReadAllText(path);
        var credentials = JsonSerializer.Deserialize<RuntimeCredentials>(
                json,
                BridgeJson.Options)
            ?? throw new InvalidDataException("Runtime credential file is invalid.");

        if (string.IsNullOrWhiteSpace(credentials.PipeName)
            || string.IsNullOrWhiteSpace(credentials.Token))
        {
            throw new InvalidDataException("Runtime credential file is invalid.");
        }

        return credentials;
    }
}
