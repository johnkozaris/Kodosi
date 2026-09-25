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
        Assert.Equal(12, db.Model.GetEntityTypes().Count());
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
        Assert.Equal(16, health.GetProperty("apiContractVersion").GetInt32());
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
        var revision = shared.GetProperty("authorizationRevision").GetInt64(); var generation = shared.GetProperty("keyGeneration").GetInt32();
        var blobs = new[] { owner, friend }.Select(actor =>
        {
            var bytes = new byte[1200]; var issued = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
            return new SessionService.KeyBlob(actor.Fixture.UserId, actor.Fixture.DeviceId, Convert.ToBase64String(bytes), owner.Fixture.DeviceId,
                Convert.ToBase64String(owner.Fixture.Sign(Proofs.SessionKey(id, incarnation, actor.Fixture.DeviceId, bytes, (uint)generation, (ulong)issued))), 2, issued);
        }).ToArray();
        await CallAsync(owner, HttpMethod.Post, path + "/keys", new { incarnationId = incarnation, authorizationRevision = revision, keyGeneration = generation, blobs });
        await host.SendAsync(new ArraySegment<byte>(TerminalConnectionTests.Frame(3, generation, 0, 1, 0)), WebSocketMessageType.Binary, true, TestContext.Current.CancellationToken);
        var participant = await app.ConnectAsync(friend, "participant", id, incarnation); using var viewer = participant.Socket;
        JsonElement checkpointRequest;
        do { checkpointRequest = await JsonAsync(host); } while (checkpointRequest.GetProperty("type").GetString() == "accessChanged");
        Assert.Equal("checkpointRequested", checkpointRequest.GetProperty("type").GetString());
        var checkpoint = TerminalConnectionTests.Frame(3, generation, 1, 2, 0);
        await SendAsync(host, new
        {
            type = "checkpoint",
            requestId = checkpointRequest.GetProperty("requestId").GetGuid(),
            frame = Convert.ToBase64String(checkpoint),
            signature = Convert.ToBase64String(new byte[3309])
        });
        var checkpointProof = await JsonAsync(viewer);
        Assert.Equal("checkpointProof", checkpointProof.GetProperty("type").GetString());
        Assert.Equal(checkpointRequest.GetProperty("challenge").GetString(), checkpointProof.GetProperty("challenge").GetString());
        Assert.Equal(friend.Fixture.UserId, checkpointProof.GetProperty("recipientUserId").GetGuid());
        Assert.Equal(Convert.ToBase64String(System.Security.Cryptography.SHA256.HashData(checkpoint)), checkpointProof.GetProperty("frameSha256").GetString());
        Assert.Equal(checkpoint, (await FrameAsync(viewer)).Bytes);
        var raw = TerminalConnectionTests.Frame(4, generation, 0, 0, 1);
        await host.SendAsync(new ArraySegment<byte>(raw), WebSocketMessageType.Binary, true, TestContext.Current.CancellationToken);
        Assert.Equal(raw, (await FrameAsync(viewer)).Bytes);
        var requestId = Guid.CreateVersion7();
        await SendAsync(viewer, new
        {
            type = "control",
            sequence = 1,
            requestId,
            keyGeneration = generation,
            nonce = Convert.ToBase64String(new byte[12]),
            ciphertext = Convert.ToBase64String(new byte[32]),
            signature = Convert.ToBase64String(new byte[3309])
        });
        var presence = await JsonAsync(host); Assert.Equal("participantConnected", presence.GetProperty("type").GetString());
        var control = await JsonAsync(host); Assert.Equal("control", control.GetProperty("type").GetString());
        Assert.Equal(friend.Fixture.UserId, control.GetProperty("senderUserId").GetGuid());
        Assert.Equal(participant.Ready.GetProperty("connectionId").GetString(), control.GetProperty("connectionId").GetString());
        await SendAsync(host, new
        {
            type = "controlResult",
            connectionId = control.GetProperty("connectionId").GetString(),
            requestId,
            sequence = 1,
            accepted = true,
            signature = Convert.ToBase64String(new byte[3309])
        });
        var result = await JsonAsync(viewer); Assert.True(result.GetProperty("accepted").GetBoolean());
        Assert.Equal(requestId, result.GetProperty("requestId").GetGuid());
        await CallAsync(owner, HttpMethod.Put, path + "/members", new { incarnationId = incarnation, expectedRevision = revision, userIds = Array.Empty<Guid>() });
        Assert.Empty((await CallAsync(friend, HttpMethod.Get, "/api/sessions")).EnumerateArray());
        using var revoked = await SignedAsync(friend, HttpMethod.Get, path + "/keys/mine");
        using var noKey = await friend.Client.SendAsync(revoked, TestContext.Current.CancellationToken); Assert.Equal(HttpStatusCode.NotFound, noKey.StatusCode);
        await Assert.ThrowsAnyAsync<Exception>(() => FrameAsync(viewer));
        Assert.Equal("changed", (await JsonAsync(ownerEvents)).GetProperty("type").GetString());
    }
}
