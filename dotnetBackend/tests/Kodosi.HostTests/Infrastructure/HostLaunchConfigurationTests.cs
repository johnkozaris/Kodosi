using System.Text.Json;

namespace Kodosi.HostTests;

public sealed class HostLaunchConfigurationTests
{
    [Fact]
    public void Development_Launch_Uses_Port_5180_Without_Opening_A_Browser()
    {
        using var launchSettings = JsonDocument.Parse(File.ReadAllText(BackendPath(
            "src", "Kodosi.Host", "Properties", "launchSettings.json")));
        var profile = launchSettings.RootElement.GetProperty("profiles").GetProperty("http");
        Assert.False(profile.GetProperty("launchBrowser").GetBoolean());
        Assert.Equal(
            "http://localhost:5180",
            profile.GetProperty("applicationUrl").GetString());
        Assert.Contains(
            "\"127.0.0.1:5180:8080\"",
            File.ReadAllText(RepoPath("compose.yaml")));
    }

    private static string BackendPath(params string[] parts)
    {
        var root = Path.GetFullPath(Path.Combine(
            AppContext.BaseDirectory,
            "..",
            "..",
            "..",
            "..",
            ".."));
        return Path.Combine([root, .. parts]);
    }

    private static string RepoPath(params string[] parts)
    {
        var root = Path.GetFullPath(Path.Combine(
            AppContext.BaseDirectory,
            "..",
            "..",
            "..",
            "..",
            "..",
            ".."));
        return Path.Combine([root, .. parts]);
    }
}
