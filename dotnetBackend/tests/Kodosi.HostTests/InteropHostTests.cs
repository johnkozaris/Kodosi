using System.Net;
using System.Text.Json;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class InteropHostTests(PostgresFixture postgres)
{
    [Fact(Explicit = true)]
    public async Task ServeLocalRustInterop()
    {
        var output = Environment.GetEnvironmentVariable("KODOSI_INTEROP_OUTPUT")
            ?? throw new InvalidOperationException("Set KODOSI_INTEROP_OUTPUT to a fresh temporary descriptor path.");
        Assert.False(File.Exists(output));
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        app.UseKestrel(options => options.Listen(IPAddress.Loopback, 0));
        using var client = app.CreateClient();
        var descriptor = new
        {
            baseUrl = client.BaseAddress!.ToString().TrimEnd('/'),
            ownerToken = app.Token("rust-owner"),
            friendToken = app.Token("rust-friend"),
            ownerHandle = "rust-owner",
            friendHandle = "rust-friend"
        };
        await File.WriteAllTextAsync(output, JsonSerializer.Serialize(descriptor), TestContext.Current.CancellationToken);
        if (!OperatingSystem.IsWindows()) File.SetUnixFileMode(output, UnixFileMode.UserRead | UnixFileMode.UserWrite);
        using var deadline = CancellationTokenSource.CreateLinkedTokenSource(TestContext.Current.CancellationToken);
        deadline.CancelAfter(TimeSpan.FromMinutes(9));
        while (!File.Exists(output + ".done")) await Task.Delay(250, deadline.Token);
    }
}
