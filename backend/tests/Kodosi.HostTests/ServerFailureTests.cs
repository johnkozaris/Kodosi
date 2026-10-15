using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Kodosi.TerminalConnections;
using Microsoft.AspNetCore.Http;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging.Abstractions;
using Npgsql;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class ServerFailureTests(PostgresFixture postgres)
{
    private static ServerMetrics Metrics()
    {
        var meters = new ServiceCollection().AddMetrics().BuildServiceProvider().GetRequiredService<System.Diagnostics.Metrics.IMeterFactory>();
        return new ServerMetrics(meters, new ConnectionDirectory(), new Kodosi.Security.DeviceSessions(TimeProvider.System));
    }

    private static async Task<(int Status, JsonElement? Body)> AnswerAsync(Exception error, ServerMetrics metrics, bool aborted = false)
    {
        using var request = new CancellationTokenSource();
        if (aborted) await request.CancelAsync();
        var context = new DefaultHttpContext { RequestAborted = request.Token };
        context.Response.Body = new MemoryStream();
        await new ApiErrors(_ => throw error, metrics, NullLogger<ApiErrors>.Instance).InvokeAsync(context);
        if (context.Response.Body.Length == 0) return (context.Response.StatusCode, null);
        context.Response.Body.Position = 0;
        using var json = await JsonDocument.ParseAsync(context.Response.Body, cancellationToken: TestContext.Current.CancellationToken);
        return (context.Response.StatusCode, json.RootElement.Clone());
    }

    [Fact]
    public async Task EachFailureGetsTheAnswerOfTheContractAndOnlyServerFaultsCount()
    {
        var metrics = Metrics();
        var (status, body) = await AnswerAsync(ApiException.Limited("devices", "Remove a device."), metrics);
        Assert.Equal(403, status);
        Assert.Equal("devices", body!.Value.GetProperty("limit").GetString());
        Assert.Equal("Remove a device.", body.Value.GetProperty("error").GetString());

        Assert.Equal(400, (await AnswerAsync(new JsonException("bad"), metrics)).Status);
        Assert.Equal(409, (await AnswerAsync(new DbUpdateConcurrencyException("raced"), metrics)).Status);
        var unique = new DbUpdateException("duplicate", new PostgresException("duplicate", "ERROR", "ERROR", PostgresErrorCodes.UniqueViolation));
        Assert.Equal(409, (await AnswerAsync(unique, metrics)).Status);
        Assert.Equal(0, metrics.Read().Failed);

        var (unavailable, why) = await AnswerAsync(new InvalidOperationException("retry", new NpgsqlException("down", new TimeoutException())), metrics);
        Assert.Equal(503, unavailable);
        Assert.Equal("The Kodosi server could not reach its database. Try again in a moment.", why!.Value.GetProperty("error").GetString());
        var (fault, text) = await AnswerAsync(new InvalidOperationException("a private detail"), metrics);
        Assert.Equal(500, fault);
        Assert.DoesNotContain("private", text!.Value.GetProperty("error").GetString());
        Assert.Equal(2, metrics.Read().Failed);

        var (cancelled, nothing) = await AnswerAsync(new OperationCanceledException(), metrics, aborted: true);
        Assert.Equal(200, cancelled);
        Assert.Null(nothing);
    }

    [Fact]
    public async Task AFailedPassIsLoggedAndTheNextPassRuns()
    {
        var work = new Flaky();
        await work.StartAsync(TestContext.Current.CancellationToken);
        for (var wait = 0; wait < 200 && Volatile.Read(ref work.Passes) < 3; wait++) await Task.Delay(10, TestContext.Current.CancellationToken);
        await work.StopAsync(TestContext.Current.CancellationToken);
        Assert.True(work.Passes >= 3);
    }

    [Fact]
    public async Task ARateLimitAnswers429WithRetryAfter()
    {
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        using var owner = await app.EnrollAsync("owner");
        HttpResponseMessage? refused = null;
        for (var attempt = 0; attempt < 40 && refused is null; attempt++)
        {
            var response = await owner.Client.GetAsync("/api/me/recovery-key", TestContext.Current.CancellationToken);
            if (response.StatusCode == HttpStatusCode.TooManyRequests) refused = response;
            else response.Dispose();
        }
        Assert.NotNull(refused);
        using (refused)
        {
            Assert.True(refused.Headers.RetryAfter?.Delta >= TimeSpan.FromSeconds(1));
            var body = await refused.Content.ReadFromJsonAsync<JsonElement>(TestContext.Current.CancellationToken);
            Assert.Equal("Too many requests. Wait a moment and try again.", body.GetProperty("error").GetString());
        }
        Assert.True(app.Services.GetRequiredService<ServerMetrics>().Read().Limited >= 1);
        using var other = await owner.Client.GetAsync("/api/me", TestContext.Current.CancellationToken);
        Assert.Equal(HttpStatusCode.OK, other.StatusCode);
    }

    private sealed class Flaky() : PeriodicWork(TimeSpan.FromMilliseconds(10), TimeProvider.System, NullLogger.Instance)
    {
        public int Passes;

        protected internal override Task RunAsync(CancellationToken ct) =>
            Interlocked.Increment(ref Passes) == 1 ? throw new InvalidOperationException("The first pass fails.") : Task.CompletedTask;
    }
}
