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
        await DatabaseSetup.InitializeAsync(db, TestContext.Current.CancellationToken);
        Assert.Equal(db.Database.GetMigrations(), await db.Database.GetAppliedMigrationsAsync(TestContext.Current.CancellationToken));
        Assert.Equal(11, db.Model.GetEntityTypes().Count());
        Assert.DoesNotContain(db.Model.GetEntityTypes(), x => x.Name.Contains("Audit") || x.Name.Contains("Task") || x.Name.Contains("Message"));
        await DatabaseSetup.InitializeAsync(db, TestContext.Current.CancellationToken);
        var oldConnection = await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken);
        await using var old = PostgresFixture.Context(oldConnection);
        await old.Database.ExecuteSqlRawAsync("CREATE TABLE legacy_marker (id integer); INSERT INTO legacy_marker VALUES (17)", TestContext.Current.CancellationToken);
        await Assert.ThrowsAsync<InvalidOperationException>(() => DatabaseSetup.InitializeAsync(old, TestContext.Current.CancellationToken));
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
        Assert.Equal(17, health.GetProperty("apiContractVersion").GetInt32());
        anonymous.DefaultRequestHeaders.Authorization = new AuthenticationHeaderValue("Bearer", app.Token("bad", "wrong-audience"));
        using var audience = await anonymous.GetAsync("/api/me", TestContext.Current.CancellationToken); Assert.Equal(HttpStatusCode.Unauthorized, audience.StatusCode);
        anonymous.DefaultRequestHeaders.Authorization = new AuthenticationHeaderValue("Bearer", app.Token("unenrolled"));
        using var unenrolled = await anonymous.GetAsync("/api/sessions", TestContext.Current.CancellationToken); Assert.Equal(HttpStatusCode.Forbidden, unenrolled.StatusCode);
        using var owner = await app.EnrollAsync("owner");
        using var proof = await SignedAsync(owner, HttpMethod.Get, "/api/sessions");
        using var first = await owner.Client.SendAsync(proof, TestContext.Current.CancellationToken); await SuccessAsync(first);
        using var replay = new HttpRequestMessage(HttpMethod.Get, "/api/sessions");
        foreach (var header in proof.Headers) replay.Headers.TryAddWithoutValidation(header.Key, header.Value);
        using var duplicate = await owner.Client.SendAsync(replay, TestContext.Current.CancellationToken); Assert.Equal(HttpStatusCode.Forbidden, duplicate.StatusCode);
        using var altered = await SignedAsync(owner, HttpMethod.Post, "/api/missions", new { id = Guid.CreateVersion7(), name = "A" });
        altered.Content = JsonContent.Create(new { id = Guid.CreateVersion7(), name = "B" });
        using var modified = await owner.Client.SendAsync(altered, TestContext.Current.CancellationToken); Assert.Equal(HttpStatusCode.Forbidden, modified.StatusCode);
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
        var gate = scope.ServiceProvider.GetRequiredService<AdmissionGate>();
        var held = await gate.EnterAsync(TestContext.Current.CancellationToken);
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
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        using var owner = await app.EnrollAsync("owner"); using var friend = await app.EnrollAsync("friend");
        await CallAsync(owner, HttpMethod.Post, "/api/friends/requests", new { username = "friend" });
        await CallAsync(friend, HttpMethod.Post, "/api/friends/requests/owner/accept");
        var mission = Guid.CreateVersion7();
        await CallAsync(owner, HttpMethod.Post, "/api/missions", new { id = mission, name = "Project" });
        var invite = Guid.CreateVersion7();
        await CallAsync(owner, HttpMethod.Post, $"/api/missions/{mission}/invitations", new { id = invite, userId = friend.Fixture.UserId });
        await CallAsync(friend, HttpMethod.Post, $"/api/missions/invitations/{invite}/accept");
        var id = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7(); var path = $"/api/sessions/{id}";
        await CallAsync(owner, HttpMethod.Post, "/api/sessions", new { id, incarnationId = incarnation, name = "Shell", hostDeviceId = owner.Fixture.DeviceId, hostName = "Host", missionId = mission });
        Assert.Empty((await CallAsync(friend, HttpMethod.Get, "/api/sessions")).EnumerateArray());
        Assert.False((await CallAsync(friend, HttpMethod.Get, $"/api/missions/{mission}")).TryGetProperty("sessionIds", out _));
        using var ownerEvents = (await app.ConnectAsync(owner, "events")).Socket;
        using var host = (await app.ConnectAsync(owner, "host", id, incarnation)).Socket;
        var shared = await CallAsync(owner, HttpMethod.Put, path + "/members", new { incarnationId = incarnation, expectedRevision = 1, userIds = new[] { friend.Fixture.UserId } });
        var revision = shared.GetProperty("authorizationRevision").GetInt64();
        Assert.False(shared.TryGetProperty("keyGeneration", out _));
        var participant = app.ConnectAsync(friend, "participant", id, incarnation);
        JsonElement notice;
        do { notice = await JsonAsync(host); } while (notice.GetProperty("type").GetString() == "accessChanged");
        Assert.Equal("viewer", notice.GetProperty("type").GetString());
        Assert.Equal(friend.Fixture.UserId, notice.GetProperty("userId").GetGuid());
        Assert.Equal(friend.Fixture.DeviceId, notice.GetProperty("deviceId").GetString());
        var channel = notice.GetProperty("channelId").GetGuid();
        await Assert.ThrowsAnyAsync<Exception>(() => app.ConnectAsync(friend, "relay", id, incarnation, channel));
        using var relay = (await app.ConnectAsync(owner, "relay", id, incarnation, channel)).Socket;
        using var viewer = (await participant).Socket;
        var toHost = new byte[70_000]; Random.Shared.NextBytes(toHost);
        await viewer.SendAsync(toHost, WebSocketMessageType.Binary, true, TestContext.Current.CancellationToken);
        var arrived = await FrameAsync(relay); Assert.Equal(WebSocketMessageType.Binary, arrived.Type); Assert.Equal(toHost, arrived.Bytes);
        var toViewer = new byte[] { 1, 2, 3 };
        await relay.SendAsync(toViewer, WebSocketMessageType.Binary, true, TestContext.Current.CancellationToken);
        Assert.Equal(toViewer, (await FrameAsync(viewer)).Bytes);
        await CallAsync(owner, HttpMethod.Put, path + "/members", new { incarnationId = incarnation, expectedRevision = revision, userIds = Array.Empty<Guid>() });
        Assert.Empty((await CallAsync(friend, HttpMethod.Get, "/api/sessions")).EnumerateArray());
        await Assert.ThrowsAnyAsync<Exception>(() => FrameAsync(viewer));
        await Assert.ThrowsAnyAsync<Exception>(() => FrameAsync(relay));
        await Assert.ThrowsAnyAsync<Exception>(() => app.ConnectAsync(friend, "participant", id, incarnation));
        Assert.Equal("accessChanged", (await JsonAsync(host)).GetProperty("type").GetString());
        Assert.Equal("changed", (await JsonAsync(ownerEvents)).GetProperty("type").GetString());
    }
}
