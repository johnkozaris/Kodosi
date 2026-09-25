using System.Text.Json;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.TerminalConnections;
using Kodosi.Security;
using Kodosi.Sessions;
using Microsoft.EntityFrameworkCore;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class SharingTests(PostgresFixture postgres)
{
    [Fact]
    public async Task CatalogDoesNotSilentlyDropSessionsBeyond512()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var viewer = await store.UserAsync("viewer");
        for (var ownerIndex = 0; ownerIndex < 5; ownerIndex++)
        {
            var owner = await store.UserAsync($"owner{ownerIndex}");
            for (var index = 0; index < 110; index++)
            {
                var id = Guid.CreateVersion7();
                store.Db.Sessions.Add(new Session { Id = id, IncarnationId = Guid.CreateVersion7(),
                    OwnerUserId = owner.User.Id, HostDeviceId = owner.Device.Id, HostName = "Host", Name = "Terminal",
                    CreatedAt = DateTimeOffset.UtcNow, ExpiresAt = DateTimeOffset.UtcNow.AddMinutes(2) });
                store.Db.SessionMembers.Add(new SessionMember { SessionId = id, UserId = viewer.User.Id });
            }
        }
        await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        var catalog = await store.Sessions.ListAsync(viewer.User.Id, TestContext.Current.CancellationToken);
        Assert.Equal(550, catalog.Count);
        Assert.All(catalog, session => Assert.Equal(new[] { viewer.User.Id }, session.SharedWith));
    }

    [Fact]
    public async Task FriendshipAndMissionMembershipDoNotGrantTerminals()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner"); var friend = await store.UserAsync("friend");
        await store.Friends.MutateAsync(owner.User.Id, "friend", "send", TestContext.Current.CancellationToken);
        await store.Friends.MutateAsync(friend.User.Id, "owner", "accept", TestContext.Current.CancellationToken);
        var mission = new Mission { Id = Guid.CreateVersion7(), Name = "Project", OwnerUserId = owner.User.Id };
        store.Db.Missions.Add(mission); store.Db.MissionMembers.Add(new MissionMember { MissionId = mission.Id, UserId = friend.User.Id }); await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        var id = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7();
        await store.Sessions.CreateAsync(owner.User.Id, owner.Device, new(id, incarnation, "Terminal", owner.Device.Id, "Host", mission.Id), TestContext.Current.CancellationToken);
        Assert.Empty(await store.Sessions.ListAsync(friend.User.Id, TestContext.Current.CancellationToken));
        await Assert.ThrowsAsync<ApiException>(() => store.Sessions.AuthorizedAsync(id, friend.User.Id, TestContext.Current.CancellationToken));
        var detail = JsonSerializer.SerializeToElement(await store.Missions.OpenAsync(mission.Id, friend.User.Id, TestContext.Current.CancellationToken), Wire.Json);
        Assert.False(detail.TryGetProperty("sessionIds", out _));
        await store.Sessions.ShareAsync(id, owner.User.Id, owner.Device.Id, new(incarnation, 1, [friend.User.Id]), TestContext.Current.CancellationToken);
        Assert.Single(await store.Sessions.ListAsync(friend.User.Id, TestContext.Current.CancellationToken));
        await Assert.ThrowsAsync<ApiException>(() => store.Sessions.ShareAsync(id, friend.User.Id, friend.Device.Id, new(incarnation, 2, []), TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task RevocationIsCurrentStateAndOldGrantCannotResurrectIt()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner"); var friend = await store.UserAsync("friend");
        await store.Friends.MutateAsync(owner.User.Id, "friend", "send", TestContext.Current.CancellationToken); await store.Friends.MutateAsync(friend.User.Id, "owner", "accept", TestContext.Current.CancellationToken);
        var id = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7();
        await store.Sessions.CreateAsync(owner.User.Id, owner.Device, new(id, incarnation, "Terminal", owner.Device.Id, "Host", null), TestContext.Current.CancellationToken);
        var shared = await store.Sessions.ShareAsync(id, owner.User.Id, owner.Device.Id, new(incarnation, 1, [friend.User.Id]), TestContext.Current.CancellationToken);
        Assert.False(shared.Ready); Assert.Equal(1, shared.KeyGeneration);
        await store.Sessions.ShareAsync(id, owner.User.Id, owner.Device.Id, new(incarnation, shared.AuthorizationRevision, []), TestContext.Current.CancellationToken);
        var error = await Assert.ThrowsAsync<ApiException>(() => store.Sessions.ShareAsync(id, owner.User.Id, owner.Device.Id, new(incarnation, 1, [friend.User.Id]), TestContext.Current.CancellationToken));
        Assert.Equal(409, error.Status); Assert.Empty(await store.Sessions.ListAsync(friend.User.Id, TestContext.Current.CancellationToken));
        await store.Sessions.EndAsync(id, owner.User.Id, owner.Device.Id, incarnation, TestContext.Current.CancellationToken);
        await Assert.ThrowsAsync<ApiException>(() => store.Sessions.CreateAsync(owner.User.Id, owner.Device, new(id, Guid.CreateVersion7(), "Other", owner.Device.Id, "Host", null), TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task KeyPublicationRequiresTheExactRecipientSetAndHostSignature()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner"); var stranger = await store.UserAsync("stranger");
        var id = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7();
        var session = await store.Sessions.CreateAsync(owner.User.Id, owner.Device, new(id, incarnation, "Terminal", owner.Device.Id, "Host", null), TestContext.Current.CancellationToken);
        session = await store.Sessions.RotateAsync(id, owner.User.Id, owner.Device.Id, new(incarnation, session.AuthorizationRevision, 0), TestContext.Current.CancellationToken);
        var bytes = new byte[1200]; var issued = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var signature = owner.Fixture.Sign(Proofs.SessionKey(id, incarnation, owner.Device.Id, bytes, 1, (ulong)issued));
        var blob = new SessionService.KeyBlob(owner.User.Id, owner.Device.Id, Convert.ToBase64String(bytes), owner.Device.Id, Convert.ToBase64String(signature), 2, issued);
        await Assert.ThrowsAsync<ApiException>(() => store.Sessions.PublishKeysAsync(id, owner.User.Id, owner.Device.Id,
            new(incarnation, session.AuthorizationRevision, 1, [blob with { RecipientUserId = stranger.User.Id, RecipientDeviceId = stranger.Device.Id }]), TestContext.Current.CancellationToken));
        await store.Sessions.PublishKeysAsync(id, owner.User.Id, owner.Device.Id, new(incarnation, session.AuthorizationRevision, 1, [blob]), TestContext.Current.CancellationToken);
        var own = JsonSerializer.SerializeToElement(await store.Sessions.MyKeyAsync(id, owner.User.Id, owner.Device.Id, TestContext.Current.CancellationToken), Wire.Json);
        Assert.Equal("ready", own.GetProperty("state").GetString());
        await Assert.ThrowsAsync<ApiException>(() => store.Sessions.MyKeyAsync(id, stranger.User.Id, stranger.Device.Id, TestContext.Current.CancellationToken));
        Assert.Single(await store.Db.SessionKeys.ToListAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task HostMayWithholdUntrustedFriendKeysButMustCoverOwnerDevices()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner"); var friend = await store.UserAsync("friend");
        await store.Friends.MutateAsync(owner.User.Id, "friend", "send", TestContext.Current.CancellationToken);
        await store.Friends.MutateAsync(friend.User.Id, "owner", "accept", TestContext.Current.CancellationToken);
        var id = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7();
        await store.Sessions.CreateAsync(owner.User.Id, owner.Device, new(id, incarnation, "Terminal", owner.Device.Id, "Host", null), TestContext.Current.CancellationToken);
        var shared = await store.Sessions.ShareAsync(id, owner.User.Id, owner.Device.Id, new(incarnation, 1, [friend.User.Id]), TestContext.Current.CancellationToken);
        Assert.Equal(2, (await store.Sessions.RecipientsAsync(await store.Sessions.AuthorizedAsync(id, owner.User.Id, TestContext.Current.CancellationToken), TestContext.Current.CancellationToken)).Count);
        var bytes = new byte[1200]; var issued = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var blob = new SessionService.KeyBlob(owner.User.Id, owner.Device.Id, Convert.ToBase64String(bytes), owner.Device.Id,
            Convert.ToBase64String(owner.Fixture.Sign(Proofs.SessionKey(id, incarnation, owner.Device.Id, bytes, (uint)shared.KeyGeneration, (ulong)issued))), 2, issued);
        await Assert.ThrowsAsync<ApiException>(() => store.Sessions.PublishKeysAsync(id, owner.User.Id, owner.Device.Id,
            new(incarnation, shared.AuthorizationRevision, shared.KeyGeneration, []), TestContext.Current.CancellationToken));
        await store.Sessions.PublishKeysAsync(id, owner.User.Id, owner.Device.Id,
            new(incarnation, shared.AuthorizationRevision, shared.KeyGeneration, [blob]), TestContext.Current.CancellationToken);
        Assert.Equal("ready", JsonSerializer.SerializeToElement(await store.Sessions.MyKeyAsync(id, owner.User.Id, owner.Device.Id, TestContext.Current.CancellationToken), Wire.Json).GetProperty("state").GetString());
        var state = await store.Sessions.AuthorizedAsync(id, owner.User.Id, TestContext.Current.CancellationToken);
        var socket = new KeyRequestSocket();
        await using var host = new SocketPeer(socket, owner.User.Id, owner.Device.Id, Guid.CreateVersion7().ToString());
        var live = store.Connections.RegisterHost(state, host); store.Connections.MarkReady(state);
        var generation = live.Generation;
        Assert.Equal(403, (await Assert.ThrowsAsync<ApiException>(() => store.Sessions.MyKeyAsync(id, friend.User.Id, friend.Device.Id, TestContext.Current.CancellationToken))).Status);
        Assert.Equal(generation, live.Generation); Assert.True(live.Ready); Assert.True(host.IsOpen);
        Assert.Equal(0, socket.Messages);
        var expired = await store.Db.DeviceLists.SingleAsync(x => x.UserId == friend.User.Id, TestContext.Current.CancellationToken);
        expired.ExpiresAtMs = issued - 1; await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        Assert.Single(await store.Sessions.RecipientsAsync(await store.Sessions.AuthorizedAsync(id, owner.User.Id, TestContext.Current.CancellationToken), TestContext.Current.CancellationToken));
    }

    private sealed class KeyRequestSocket : FakeSocket
    {
        public int Messages;
        public override Task SendAsync(ArraySegment<byte> buffer, System.Net.WebSockets.WebSocketMessageType type, bool endOfMessage, CancellationToken ct)
        { Interlocked.Increment(ref Messages); return Task.CompletedTask; }
    }

    [Fact]
    public async Task SecondApprovedOwnerDeviceCanEditMetadataButNotSharingAndFriendsCannotEditEither()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner"); var friend = await store.UserAsync("friend");
        var second = new DeviceFixture(owner.User.Id, "second-device");
        var init = JsonSerializer.SerializeToElement(await store.Devices.StartLinkAsync(owner.User.Id,
            new(second.DeviceId, "Second", Convert.ToBase64String(second.KemKey), Convert.ToBase64String(second.SigningKey)), TestContext.Current.CancellationToken), Wire.Json);
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var cert = second.CertificateBody(owner.Device.Id, now);
        var list = DeviceFixture.ListBody(owner.User.Id, 2, [(owner.Device.Id, owner.Device.Id), (second.DeviceId, owner.Device.Id)], owner.Device.Id, now);
        await store.Devices.ApproveLinkAsync(owner.User.Id, owner.Device, new(init.GetProperty("userCode").GetString()!, Convert.ToBase64String(cert),
            owner.Fixture.SignedCertificate(cert), Convert.ToBase64String(list), owner.Fixture.SignedList(list)), TestContext.Current.CancellationToken);
        await store.Friends.MutateAsync(owner.User.Id, "friend", "send", TestContext.Current.CancellationToken);
        await store.Friends.MutateAsync(friend.User.Id, "owner", "accept", TestContext.Current.CancellationToken);
        var id = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7(); var mission = Guid.CreateVersion7();
        await store.Missions.CreateAsync(owner.User.Id, new(mission, "Project"), TestContext.Current.CancellationToken);
        await store.Sessions.CreateAsync(owner.User.Id, owner.Device, new(id, incarnation, "Original", owner.Device.Id, "Host", null), TestContext.Current.CancellationToken);
        var shared = await store.Sessions.ShareAsync(id, owner.User.Id, owner.Device.Id, new(incarnation, 1, [friend.User.Id]), TestContext.Current.CancellationToken);
        var renamed = await store.Sessions.RenameAsync(id, owner.User.Id, second.DeviceId, new(incarnation, shared.AuthorizationRevision, "Renamed"), TestContext.Current.CancellationToken);
        Assert.Equal("Renamed", renamed.Name);
        Assert.Equal(mission, (await store.Sessions.AttachAsync(id, owner.User.Id, second.DeviceId, new(incarnation, mission), TestContext.Current.CancellationToken)).MissionId);
        Assert.Null((await store.Sessions.AttachAsync(id, owner.User.Id, second.DeviceId, new(incarnation, null), TestContext.Current.CancellationToken)).MissionId);
        Assert.Equal(403, (await Assert.ThrowsAsync<ApiException>(() => store.Sessions.ShareAsync(id, owner.User.Id, second.DeviceId,
            new(incarnation, shared.AuthorizationRevision, []), TestContext.Current.CancellationToken))).Status);
        Assert.Equal(403, (await Assert.ThrowsAsync<ApiException>(() => store.Sessions.RenameAsync(id, friend.User.Id, friend.Device.Id,
            new(incarnation, shared.AuthorizationRevision, "Not allowed"), TestContext.Current.CancellationToken))).Status);
        Assert.Equal(403, (await Assert.ThrowsAsync<ApiException>(() => store.Sessions.AttachAsync(id, friend.User.Id, friend.Device.Id,
            new(incarnation, mission), TestContext.Current.CancellationToken))).Status);
    }

    [Fact]
    public async Task UnfriendRemovesExistingSharesWithoutDeletingSessions()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner"); var friend = await store.UserAsync("friend");
        await store.Friends.MutateAsync(owner.User.Id, "friend", "send", TestContext.Current.CancellationToken); await store.Friends.MutateAsync(friend.User.Id, "owner", "accept", TestContext.Current.CancellationToken);
        var id = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7();
        await store.Sessions.CreateAsync(owner.User.Id, owner.Device, new(id, incarnation, "Terminal", owner.Device.Id, "Host", null), TestContext.Current.CancellationToken);
        await store.Sessions.ShareAsync(id, owner.User.Id, owner.Device.Id, new(incarnation, 1, [friend.User.Id]), TestContext.Current.CancellationToken);
        await store.Friends.MutateAsync(friend.User.Id, "owner", "remove", TestContext.Current.CancellationToken);
        Assert.Empty(await store.Sessions.ListAsync(friend.User.Id, TestContext.Current.CancellationToken));
        Assert.Single(await store.Sessions.ListAsync(owner.User.Id, TestContext.Current.CancellationToken));
        Assert.False((await store.Db.Sessions.SingleAsync(TestContext.Current.CancellationToken)).Ready);
    }
}
