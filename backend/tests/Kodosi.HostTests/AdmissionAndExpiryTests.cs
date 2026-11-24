using System.Net;
using System.Text.Json;
using Kodosi.Admission;
using Kodosi.Data;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Microsoft.Extensions.Logging.Abstractions;
using Xunit;
using static Kodosi.HostTests.BackendApplication;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class AdmissionAndExpiryTests(PostgresFixture postgres)
{
    [Fact]
    public async Task AChangeWaitsOnlyForAnotherChangeOfTheSameAccount()
    {
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        using var owner = await app.EnrollAsync("owner");
        using var other = await app.EnrollAsync("other");
        using var read = await SignedAsync(owner, HttpMethod.Get, "/api/sessions");
        using var change = await SignedAsync(owner, HttpMethod.Post, "/api/projects", new { id = Guid.CreateVersion7(), name = "Project" });
        using var separate = await SignedAsync(other, HttpMethod.Post, "/api/projects", new { id = Guid.CreateVersion7(), name = "Other" });
        var changing = await app.Services.GetRequiredService<AccountGate>().EnterAsync(owner.Fixture.UserId, TestContext.Current.CancellationToken);
        Task<HttpResponseMessage> pending;
        try
        {
            using var listed = await owner.Client.SendAsync(read, TestContext.Current.CancellationToken)
                .WaitAsync(TimeSpan.FromSeconds(5), TestContext.Current.CancellationToken);
            Assert.Equal(HttpStatusCode.OK, listed.StatusCode);
            pending = owner.Client.SendAsync(change, TestContext.Current.CancellationToken);
            using var made = await other.Client.SendAsync(separate, TestContext.Current.CancellationToken)
                .WaitAsync(TimeSpan.FromSeconds(5), TestContext.Current.CancellationToken);
            Assert.Equal(HttpStatusCode.NoContent, made.StatusCode);
            Assert.False(pending.IsCompleted);
        }
        finally { changing.Dispose(); }
        using var changed = await pending;
        Assert.Equal(HttpStatusCode.NoContent, changed.StatusCode);
    }

    [Fact]
    public async Task OfflineTerminalsKeepTheirRecordAndSharingAndOnlyEndedOnesAreRemoved()
    {
        var clock = new ManualClock();
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken), services =>
        {
            services.RemoveAll<TimeProvider>(); services.AddSingleton<TimeProvider>(clock);
        });
        using var owner = await app.EnrollAsync("owner");
        using var friend = await app.EnrollAsync("friend");
        var scopes = app.Services.GetRequiredService<IServiceScopeFactory>();
        using var cleanup = new DataCleanup(scopes, clock, NullLogger<DataCleanup>.Instance);
        Session Terminal(string name, bool ended) => new()
        {
            Id = Guid.CreateVersion7(),
            IncarnationId = Guid.CreateVersion7(),
            OwnerUserId = owner.Fixture.UserId,
            HostDeviceId = owner.Fixture.DeviceId,
            HostName = "Host",
            Name = name,
            Ended = ended,
            ExpiresAt = clock.GetUtcNow().AddMinutes(2),
            CreatedAt = clock.GetUtcNow()
        };
        var offline = Terminal("Offline", ended: false);
        var ended = Terminal("Ended", ended: true);
        await using (var scope = scopes.CreateAsyncScope())
        {
            var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            db.Sessions.AddRange(offline, ended);
            db.SessionMembers.Add(new SessionMember { SessionId = offline.Id, UserId = friend.Fixture.UserId });
            await db.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        await cleanup.RunAsync(TestContext.Current.CancellationToken);
        await using (var scope = scopes.CreateAsyncScope())
            Assert.Equal(2, await scope.ServiceProvider.GetRequiredService<KodosiDbContext>().Sessions.CountAsync(TestContext.Current.CancellationToken));
        clock.Advance(TimeSpan.FromDays(30));
        await cleanup.RunAsync(TestContext.Current.CancellationToken);
        await using (var scope = scopes.CreateAsyncScope())
        {
            var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            Assert.Equal(offline.Id, (await db.Sessions.SingleAsync(TestContext.Current.CancellationToken)).Id);
            Assert.Equal(friend.Fixture.UserId, (await db.SessionMembers.SingleAsync(TestContext.Current.CancellationToken)).UserId);
        }
    }

    [Fact]
    public async Task RequestsWithNoTokenCannotBlockTheHealthCheck()
    {
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        using var client = app.CreateClient();
        for (var i = 0; i < 1250; i++)
        {
            using var probe = await client.GetAsync("/health/ready", TestContext.Current.CancellationToken);
            Assert.Equal(System.Net.HttpStatusCode.OK, probe.StatusCode);
        }
    }

    [Fact]
    public async Task SocketEndsWithItsTokenUnlessTheDeviceRenewsItWithANewerOne()
    {
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        using var renewing = await app.EnrollAsync("renewing");
        using var silent = await app.EnrollAsync("silent");
        var brief = TimeSpan.FromSeconds(5);
        var kept = await app.ConnectAsync(renewing with { Token = app.Token("renewing", lifetime: brief) });
        var ended = await app.ConnectAsync(silent with { Token = app.Token("silent", lifetime: brief) });
        await CallAsync(renewing, HttpMethod.Post, "/api/me/connections/renew");
        await CallAsync(silent, HttpMethod.Get, "/api/sessions");
        await Task.Delay(brief + TimeSpan.FromSeconds(1), TestContext.Current.CancellationToken);
        await SendAsync(kept, new { type = "ping" });
        Assert.Equal("pong", (await JsonAsync(kept)).GetProperty("type").GetString());
        await Assert.ThrowsAnyAsync<Exception>(() => JsonAsync(ended));
    }

    [Fact]
    public async Task DeviceConnectionRefusesAProtocolOutsideItsRangeAndReportsTheRange()
    {
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        using var device = await app.EnrollAsync("older");
        var client = app.Server.CreateWebSocketClient(); client.ConfigureRequest = request => request.Headers.Authorization = "Bearer " + device.Token;
        using var socket = await client.ConnectAsync(new Uri("ws://localhost/ws/device"), TestContext.Current.CancellationToken);
        await SendAsync(socket, new { type = "hello", protocolVersion = ContractVersions.OldestTerminalConnection - 1, deviceId = device.Fixture.DeviceId, deviceSession = device.Session });
        var refused = await JsonAsync(socket);
        Assert.Equal("refused", refused.GetProperty("type").GetString());
        Assert.Equal("version", refused.GetProperty("reason").GetString());
        Assert.Equal(ContractVersions.OldestTerminalConnection, refused.GetProperty("oldest").GetInt32());
        Assert.Equal(ContractVersions.TerminalConnection, refused.GetProperty("current").GetInt32());
    }

    [Fact]
    public async Task ADeviceConnectionThatSendsTooManyControlMessagesIsClosed()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(ct));
        using var device = await app.EnrollAsync("chatty");
        using var socket = await app.ConnectAsync(device);
        var ping = JsonSerializer.SerializeToUtf8Bytes(new { type = "ping" });
        var sent = 0;
        try
        {
            for (; sent < 6000; sent++) await socket.SendAsync(ping, System.Net.WebSockets.WebSocketMessageType.Text, true, ct);
        }
        catch (Exception error) when (error is System.Net.WebSockets.WebSocketException or IOException or ObjectDisposedException)
        {
        }
        var pongs = 0;
        using var deadline = CancellationTokenSource.CreateLinkedTokenSource(ct);
        deadline.CancelAfter(TimeSpan.FromSeconds(20));
        using var reader = new Kodosi.TerminalConnections.MessageReader(socket);
        try
        {
            while (await reader.ReadAsync(Kodosi.TerminalConnections.ConnectionEndpoints.ControlBytes, deadline.Token) is not null) pongs++;
        }
        catch (Exception error) when (error is System.Net.WebSockets.WebSocketException or IOException or ObjectDisposedException)
        {
        }
        Assert.True(pongs < sent, $"{pongs} answers for {sent} messages");
        Assert.True(app.Services.GetRequiredService<ServerMetrics>().Read().Refused >= 1);
    }
}
