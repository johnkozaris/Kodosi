using System.Net;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Net.WebSockets;
using System.Text.Json;
using Kodosi.Admission;
using Kodosi.Data;
using Kodosi.TerminalConnections;
using Kodosi.Security;
using Kodosi.Sessions;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Npgsql;
using Xunit;
using static Kodosi.HostTests.BackendApplication;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class IntegrationTests(PostgresFixture postgres)
{
    [Fact]
    public async Task IdentityResetRequiresARecentSignInAndClearsTheDeviceList()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(ct));
        using var owner = await app.EnrollAsync("owner");
        using var stale = app.CreateClient();
        stale.DefaultRequestHeaders.Authorization = new AuthenticationHeaderValue("Bearer", app.Token("owner"));
        using var refused = await stale.PostAsync("/api/me/identity/reset", null, ct);
        Assert.Equal(HttpStatusCode.Forbidden, refused.StatusCode);
        using var old = app.CreateClient();
        old.DefaultRequestHeaders.Authorization = new AuthenticationHeaderValue("Bearer", app.Token("owner", authenticatedAt: DateTimeOffset.UtcNow.AddHours(-1)));
        using var refusedOld = await old.PostAsync("/api/me/identity/reset", null, ct);
        Assert.Equal(HttpStatusCode.Forbidden, refusedOld.StatusCode);
        using var recent = app.CreateClient();
        recent.DefaultRequestHeaders.Authorization = new AuthenticationHeaderValue("Bearer", app.Token("owner", authenticatedAt: DateTimeOffset.UtcNow));
        using var accepted = await recent.PostAsync("/api/me/identity/reset", null, ct);
        Assert.Equal(HttpStatusCode.NoContent, accepted.StatusCode);
        using var identity = await owner.Client.GetAsync("/api/me/identity", ct);
        Assert.Equal(HttpStatusCode.NotFound, identity.StatusCode);
        using var again = await recent.PostAsync("/api/me/identity/reset", null, ct);
        Assert.Equal(HttpStatusCode.NoContent, again.StatusCode);
    }

    [Fact]
    public async Task IssuerAndSubjectIdentifyTheAccount()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        static Microsoft.AspNetCore.Http.DefaultHttpContext Context(string iss, string sub)
        {
            var context = new Microsoft.AspNetCore.Http.DefaultHttpContext();
            context.User = new System.Security.Claims.ClaimsPrincipal(new System.Security.Claims.ClaimsIdentity([
                new System.Security.Claims.Claim("iss", iss), new System.Security.Claims.Claim("sub", sub)
            ], "test"));
            return context;
        }
        var user = await new Kodosi.Accounts.CurrentUser(store.Db).GetAsync(Context("https://auth.example/realms/kodosi", "alias-user"), TestContext.Current.CancellationToken);
        var again = await new Kodosi.Accounts.CurrentUser(store.Db).GetAsync(Context("https://auth.example/realms/kodosi", "alias-user"), TestContext.Current.CancellationToken);
        var other = await new Kodosi.Accounts.CurrentUser(store.Db).GetAsync(Context("https://other.example/realms/kodosi", "alias-user"), TestContext.Current.CancellationToken);
        Assert.Equal(user.Id, again.Id);
        Assert.NotEqual(user.Id, other.Id);
    }

    [Fact]
    public async Task BaselineHasOnlyCurrentStateAndRefusesAnUnknownDatabase()
    {
        var connection = await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken);
        await using var db = PostgresFixture.Context(connection);
        await DatabaseSetup.MigrateAsync(db, TestContext.Current.CancellationToken);
        Assert.Equal(db.Database.GetMigrations(), await db.Database.GetAppliedMigrationsAsync(TestContext.Current.CancellationToken));
        Assert.Equal(11, db.Model.GetEntityTypes().Count());
        Assert.DoesNotContain(db.Model.GetEntityTypes(), x => x.Name.Contains("Audit") || x.Name.Contains("Task") || x.Name.Contains("Message"));
        await DatabaseSetup.MigrateAsync(db, TestContext.Current.CancellationToken);
        var oldConnection = await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken);
        await using var old = PostgresFixture.Context(oldConnection);
        await old.Database.ExecuteSqlRawAsync("CREATE TABLE legacy_marker (id integer); INSERT INTO legacy_marker VALUES (17)", TestContext.Current.CancellationToken);
        await Assert.ThrowsAsync<InvalidOperationException>(() => DatabaseSetup.MigrateAsync(old, TestContext.Current.CancellationToken));
        await using var inspect = new NpgsqlConnection(oldConnection); await inspect.OpenAsync(TestContext.Current.CancellationToken);
        await using var command = new NpgsqlCommand("SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='public'", inspect);
        Assert.Equal(1L, await command.ExecuteScalarAsync(TestContext.Current.CancellationToken));
        command.CommandText = "SELECT id FROM legacy_marker";
        Assert.Equal(17, await command.ExecuteScalarAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task HttpBoundaryRejectsMissingIdentityWrongAudienceReplayAndRetiredRoutes()
    {
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        using var anonymous = app.CreateClient();
        using var denied = await anonymous.GetAsync("/api/me", TestContext.Current.CancellationToken); Assert.Equal(HttpStatusCode.Unauthorized, denied.StatusCode);
        var health = await anonymous.GetFromJsonAsync<JsonElement>("/health/live", TestContext.Current.CancellationToken);
        Assert.Equal(21, health.GetProperty("apiContractVersion").GetInt32());
        anonymous.DefaultRequestHeaders.Authorization = new AuthenticationHeaderValue("Bearer", app.Token("bad", "wrong-audience"));
        using var audience = await anonymous.GetAsync("/api/me", TestContext.Current.CancellationToken); Assert.Equal(HttpStatusCode.Unauthorized, audience.StatusCode);
        anonymous.DefaultRequestHeaders.Authorization = new AuthenticationHeaderValue("Bearer", app.Token("unenrolled"));
        using var unenrolled = await anonymous.GetAsync("/api/sessions", TestContext.Current.CancellationToken); Assert.Equal(HttpStatusCode.PreconditionRequired, unenrolled.StatusCode);
        using var owner = await app.EnrollAsync("owner");
        using var proof = await SignedAsync(owner, HttpMethod.Get, "/api/sessions");
        using var first = await owner.Client.SendAsync(proof, TestContext.Current.CancellationToken); await SuccessAsync(first);
        using var other = await app.EnrollAsync("other");
        foreach (var (device, session) in new[] { (owner.Fixture.DeviceId, "not-a-session"), (owner.Fixture.DeviceId, other.Session), (other.Fixture.DeviceId, owner.Session) })
        {
            using var wrong = new HttpRequestMessage(HttpMethod.Get, "/api/sessions");
            wrong.Headers.Add("X-Kodosi-Device-Id", device); wrong.Headers.Add("X-Kodosi-Device-Session", session);
            using var refused = await owner.Client.SendAsync(wrong, TestContext.Current.CancellationToken);
            Assert.Equal(HttpStatusCode.PreconditionRequired, refused.StatusCode);
        }
        var challenge = await owner.Client.PostAsync("/api/me/devices/challenge", null, TestContext.Current.CancellationToken);
        var challengeId = (await challenge.Content.ReadFromJsonAsync<JsonElement>(TestContext.Current.CancellationToken)).GetProperty("challengeId").GetGuid();
        using var forged = await owner.Client.PostAsJsonAsync("/api/me/device-sessions", new
        {
            deviceId = owner.Fixture.DeviceId, challengeId, signature = Convert.ToBase64String(other.Fixture.Sign(new byte[40])),
        }, TestContext.Current.CancellationToken);
        Assert.Equal(HttpStatusCode.Forbidden, forged.StatusCode);
        foreach (var path in new[] { "/api/missions/00000000-0000-0000-0000-000000000000/tasks", "/api/sessions/00000000-0000-0000-0000-000000000000/suggestions", "/api/session-history" })
        {
            using var retired = await owner.Client.GetAsync(path, TestContext.Current.CancellationToken); Assert.Equal(HttpStatusCode.NotFound, retired.StatusCode);
        }
    }

    [Fact]
    public async Task StalePreparedProofCannotMutateAfterDeviceRevocation()
    {
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        using var owner = await app.EnrollAsync("owner");
        using var stale = await SignedAsync(owner, HttpMethod.Post, "/api/missions", new { id = Guid.CreateVersion7(), name = "Forbidden" });
        await using var scope = app.Services.CreateAsyncScope();
        var gate = scope.ServiceProvider.GetRequiredService<AccountGate>();
        var held = await gate.EnterAsync(owner.Fixture.UserId, TestContext.Current.CancellationToken);
        var pending = owner.Client.SendAsync(stale, TestContext.Current.CancellationToken);
        try
        {
            var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            await db.Devices.Where(x => x.Id == owner.Fixture.DeviceId).ExecuteUpdateAsync(set => set.SetProperty(x => x.Revoked, true), TestContext.Current.CancellationToken);
            Assert.False(pending.IsCompleted);
        }
        finally { held.Dispose(); }
        using var response = await pending;
        Assert.Equal(HttpStatusCode.Forbidden, response.StatusCode);
        Assert.Empty(await scope.ServiceProvider.GetRequiredService<KodosiDbContext>().Missions.ToListAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task RealSocketJourneySharesOnlyChosenSessionAndRevocationEndsControl()
    {
        long refused = 0, relayed = 0;
        using var meter = new System.Diagnostics.Metrics.MeterListener
        {
            InstrumentPublished = (instrument, listener) => { if (instrument.Meter.Name == ServerMetrics.MeterName) listener.EnableMeasurementEvents(instrument); },
        };
        meter.SetMeasurementEventCallback<long>((instrument, value, _, _) =>
        {
            if (instrument.Name == "kodosi.connections.refused") Interlocked.Add(ref refused, value);
            if (instrument.Name == "kodosi.relay.bytes") Interlocked.Add(ref relayed, value);
        });
        meter.Start();
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        using var owner = await app.EnrollAsync("owner"); using var friend = await app.EnrollAsync("friend");
        await CallAsync(owner, HttpMethod.Post, "/api/friends/requests", new { username = "friend" });
        await CallAsync(friend, HttpMethod.Post, "/api/friends/requests/owner/accept");
        async Task<(HttpStatusCode Status, string? Tag)> IdentityAsync(ApiDevice actor, string identity, string? known = null)
        {
            using var request = await SignedAsync(actor, HttpMethod.Get, identity);
            if (known is not null) request.Headers.TryAddWithoutValidation("If-None-Match", known);
            using var response = await actor.Client.SendAsync(request, TestContext.Current.CancellationToken);
            return (response.StatusCode, response.Headers.ETag?.ToString());
        }
        var ownerIdentity = $"/api/users/{owner.Fixture.UserId}/identity";
        var (found, tag) = await IdentityAsync(friend, ownerIdentity);
        Assert.Equal(HttpStatusCode.OK, found); Assert.NotNull(tag);
        Assert.Equal(HttpStatusCode.NotModified, (await IdentityAsync(friend, ownerIdentity, tag)).Status);
        Assert.Equal(HttpStatusCode.OK, (await IdentityAsync(friend, ownerIdentity, "\"other\"")).Status);
        Assert.Equal((HttpStatusCode.NotModified, tag), await IdentityAsync(owner, "/api/me/identity", tag));
        var mission = Guid.CreateVersion7();
        await CallAsync(owner, HttpMethod.Post, "/api/missions", new { id = mission, name = "Project" });
        var invite = Guid.CreateVersion7();
        await CallAsync(owner, HttpMethod.Post, $"/api/missions/{mission}/invitations", new { id = invite, userId = friend.Fixture.UserId });
        await CallAsync(friend, HttpMethod.Post, $"/api/missions/invitations/{invite}/accept");
        var id = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7(); var path = $"/api/sessions/{id}";
        await CallAsync(owner, HttpMethod.Post, "/api/sessions", new { id, incarnationId = incarnation, name = "Shell", hostDeviceId = owner.Fixture.DeviceId, hostName = "Host", missionId = mission });
        Assert.Empty((await CallAsync(friend, HttpMethod.Get, "/api/sessions")).EnumerateArray());
        Assert.False((await CallAsync(friend, HttpMethod.Get, $"/api/missions/{mission}")).TryGetProperty("sessionIds", out _));
        await Assert.ThrowsAnyAsync<Exception>(() => app.ConnectAsync(owner with { Session = friend.Session }));
        using var host = await app.ConnectAsync(owner);
        using var viewer = await app.ConnectAsync(friend);
        async Task<JsonElement> NextAsync(WebSocket socket, string type)
        {
            while (true)
            {
                var message = await JsonAsync(socket);
                if (message.GetProperty("type").GetString() == type) return message;
            }
        }
        var early = Guid.CreateVersion7();
        await SendAsync(viewer, new { type = "open", pipe = early, sessionId = id, incarnationId = incarnation });
        Assert.Equal(404, (await NextAsync(viewer, "closed")).GetProperty("status").GetInt32());
        await SendAsync(host, new { type = "host", sessionId = id, incarnationId = incarnation });
        Assert.Equal(id, (await NextAsync(host, "hosted")).GetProperty("sessionId").GetGuid());
        await SendAsync(viewer, new { type = "host", sessionId = id, incarnationId = incarnation });
        Assert.NotEqual(0, (await NextAsync(viewer, "unhosted")).GetProperty("status").GetInt32());
        var shared = await CallAsync(owner, HttpMethod.Put, path + "/members", new { incarnationId = incarnation, expectedRevision = 1, userIds = new[] { friend.Fixture.UserId } });
        var revision = shared.GetProperty("authorizationRevision").GetInt64();
        Assert.False(shared.TryGetProperty("keyGeneration", out _));
        Assert.True((await CallAsync(friend, HttpMethod.Get, path)).GetProperty("hostOnline").GetBoolean());
        var pipe = Guid.CreateVersion7();
        await SendAsync(viewer, new { type = "open", pipe, sessionId = id, incarnationId = incarnation });
        var notice = await NextAsync(host, "viewer");
        Assert.Equal(pipe, notice.GetProperty("pipe").GetGuid());
        Assert.Equal(id, notice.GetProperty("sessionId").GetGuid());
        Assert.Equal(friend.Fixture.UserId, notice.GetProperty("userId").GetGuid());
        Assert.Equal(friend.Fixture.DeviceId, notice.GetProperty("deviceId").GetString());
        var toHost = new byte[70_000]; Random.Shared.NextBytes(toHost);
        await viewer.SendAsync(PipeFrame(pipe, toHost), WebSocketMessageType.Binary, true, TestContext.Current.CancellationToken);
        async Task<byte[]> BinaryAsync(WebSocket socket)
        {
            while (true)
            {
                var frame = await FrameAsync(socket);
                if (frame.Type == WebSocketMessageType.Binary) return frame.Bytes;
            }
        }
        Assert.Equal(PipeFrame(pipe, toHost), await BinaryAsync(host));
        var toViewer = new byte[] { 1, 2, 3 };
        await host.SendAsync(PipeFrame(pipe, toViewer), WebSocketMessageType.Binary, true, TestContext.Current.CancellationToken);
        Assert.Equal(PipeFrame(pipe, toViewer), await BinaryAsync(viewer));
        await host.SendAsync(PipeFrame(Guid.CreateVersion7(), toViewer), WebSocketMessageType.Binary, true, TestContext.Current.CancellationToken);
        await CallAsync(owner, HttpMethod.Put, path + "/members", new { incarnationId = incarnation, expectedRevision = revision, userIds = Array.Empty<Guid>() });
        Assert.Empty((await CallAsync(friend, HttpMethod.Get, "/api/sessions")).EnumerateArray());
        Assert.Equal(403, (await NextAsync(viewer, "closed")).GetProperty("status").GetInt32());
        Assert.Equal(pipe, (await NextAsync(host, "closed")).GetProperty("pipe").GetGuid());
        Assert.Equal(id, (await NextAsync(host, "accessChanged")).GetProperty("sessionId").GetGuid());
        await viewer.SendAsync(PipeFrame(pipe, toHost), WebSocketMessageType.Binary, true, TestContext.Current.CancellationToken);
        await SendAsync(viewer, new { type = "open", pipe = Guid.CreateVersion7(), sessionId = id, incarnationId = incarnation });
        Assert.Equal(404, (await NextAsync(viewer, "closed")).GetProperty("status").GetInt32());
        await SendAsync(host, new { type = "ping" });
        Assert.Equal("pong", (await NextAsync(host, "pong")).GetProperty("type").GetString());
        for (var wait = 0; wait < 100 && Interlocked.Read(ref refused) < 3; wait++) await Task.Delay(20, TestContext.Current.CancellationToken);
        Assert.True(Interlocked.Read(ref relayed) >= toHost.Length + toViewer.Length);
        Assert.True(Interlocked.Read(ref refused) >= 3);
    }
}
