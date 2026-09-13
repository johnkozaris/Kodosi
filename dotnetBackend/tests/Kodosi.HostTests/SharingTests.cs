using System.Text.Json;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Realtime;
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
        var room = new Room { Id = Guid.CreateVersion7(), Name = "Project", Slug = "project", OwnerUserId = owner.User.Id };
        store.Db.Rooms.Add(room); store.Db.RoomMembers.Add(new RoomMember { RoomId = room.Id, UserId = friend.User.Id }); await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        var id = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7();
        await store.Sessions.CreateAsync(owner.User.Id, owner.Device, new(id, incarnation, "Terminal", owner.Device.Id, "Host", room.Id), TestContext.Current.CancellationToken);
        Assert.Empty(await store.Sessions.ListAsync(friend.User.Id, TestContext.Current.CancellationToken));
        await Assert.ThrowsAsync<ApiException>(() => store.Sessions.AuthorizedAsync(id, friend.User.Id, TestContext.Current.CancellationToken));
        var detail = JsonSerializer.SerializeToElement(await store.Rooms.OpenAsync(room.Id, friend.User.Id, TestContext.Current.CancellationToken), Wire.Json);
        Assert.Empty(detail.GetProperty("sessionIds").EnumerateArray());
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
        Assert.Equal("pendingDistribution", JsonSerializer.SerializeToElement(await store.Sessions.MyKeyAsync(id, friend.User.Id, friend.Device.Id, TestContext.Current.CancellationToken), Wire.Json).GetProperty("state").GetString());
        var expired = await store.Db.DeviceLists.SingleAsync(x => x.UserId == friend.User.Id, TestContext.Current.CancellationToken);
        expired.ExpiresAtMs = issued - 1; await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        Assert.Single(await store.Sessions.RecipientsAsync(await store.Sessions.AuthorizedAsync(id, owner.User.Id, TestContext.Current.CancellationToken), TestContext.Current.CancellationToken));
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
        var id = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7(); var room = Guid.CreateVersion7();
        await store.Rooms.CreateAsync(owner.User.Id, new(room, "Project", "project"), TestContext.Current.CancellationToken);
        await store.Sessions.CreateAsync(owner.User.Id, owner.Device, new(id, incarnation, "Original", owner.Device.Id, "Host", null), TestContext.Current.CancellationToken);
        var shared = await store.Sessions.ShareAsync(id, owner.User.Id, owner.Device.Id, new(incarnation, 1, [friend.User.Id]), TestContext.Current.CancellationToken);
        var renamed = await store.Sessions.RenameAsync(id, owner.User.Id, second.DeviceId, new(incarnation, shared.AuthorizationRevision, "Renamed"), TestContext.Current.CancellationToken);
        Assert.Equal("Renamed", renamed.Name);
        Assert.Equal(room, (await store.Sessions.AttachAsync(id, owner.User.Id, second.DeviceId, new(incarnation, room), TestContext.Current.CancellationToken)).RoomId);
        Assert.Null((await store.Sessions.AttachAsync(id, owner.User.Id, second.DeviceId, new(incarnation, null), TestContext.Current.CancellationToken)).RoomId);
        Assert.Equal(403, (await Assert.ThrowsAsync<ApiException>(() => store.Sessions.ShareAsync(id, owner.User.Id, second.DeviceId,
            new(incarnation, shared.AuthorizationRevision, []), TestContext.Current.CancellationToken))).Status);
        Assert.Equal(403, (await Assert.ThrowsAsync<ApiException>(() => store.Sessions.RenameAsync(id, friend.User.Id, friend.Device.Id,
            new(incarnation, shared.AuthorizationRevision, "Not allowed"), TestContext.Current.CancellationToken))).Status);
        Assert.Equal(403, (await Assert.ThrowsAsync<ApiException>(() => store.Sessions.AttachAsync(id, friend.User.Id, friend.Device.Id,
            new(incarnation, room), TestContext.Current.CancellationToken))).Status);
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
