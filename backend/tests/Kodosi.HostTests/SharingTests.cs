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
    public async Task ASignedFriendListIsReplacedOnlyFromTheRevisionThatTheDeviceReadAndTheFriendListGivesTheIdentityOfEachFriend()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner"); var friend = await store.UserAsync("friend");
        await Assert.ThrowsAsync<ApiException>(() => store.Friends.SignedListAsync(owner.User.Id, ct));
        var signature = Convert.ToBase64String(new byte[IdentityWireFormat.MlDsa65SignatureLength]);
        string Body(byte value) => Convert.ToBase64String([value]);
        await store.Friends.ReplaceSignedListAsync(owner.User.Id, new(0, 1, Body(1), signature), ct);
        var stale = await Assert.ThrowsAsync<ApiException>(() => store.Friends.ReplaceSignedListAsync(owner.User.Id, new(0, 2, Body(2), signature), ct));
        Assert.Equal(409, stale.Status);
        await Assert.ThrowsAsync<ApiException>(() => store.Friends.ReplaceSignedListAsync(owner.User.Id, new(1, 1, Body(2), signature), ct));
        await store.Friends.ReplaceSignedListAsync(owner.User.Id, new(1, 3, Body(3), signature), ct);
        var list = JsonSerializer.SerializeToElement(await store.Friends.SignedListAsync(owner.User.Id, ct), Wire.Json);
        Assert.Equal(3, list.GetProperty("revision").GetInt64());
        Assert.Equal(Body(3), list.GetProperty("body").GetString());
        await Assert.ThrowsAsync<ApiException>(() => store.Friends.SignedListAsync(friend.User.Id, ct));
        await store.Friends.MutateAsync(owner.User.Id, "friend", "send", ct);
        await store.Friends.MutateAsync(friend.User.Id, "owner", "accept", ct);
        var listed = Assert.Single(JsonSerializer.SerializeToElement(await store.Friends.ListAsync(owner.User.Id, ct), Wire.Json).EnumerateArray());
        Assert.Equal(friend.User.IdentityIncarnationId, listed.GetProperty("identityIncarnationId").GetGuid());
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
        Assert.Equal(2, shared.AuthorizationRevision);
        await store.Sessions.ShareAsync(id, owner.User.Id, owner.Device.Id, new(incarnation, shared.AuthorizationRevision, []), TestContext.Current.CancellationToken);
        var error = await Assert.ThrowsAsync<ApiException>(() => store.Sessions.ShareAsync(id, owner.User.Id, owner.Device.Id, new(incarnation, 1, [friend.User.Id]), TestContext.Current.CancellationToken));
        Assert.Equal(409, error.Status); Assert.Empty(await store.Sessions.ListAsync(friend.User.Id, TestContext.Current.CancellationToken));
        await store.Sessions.EndAsync(id, owner.User.Id, owner.Device.Id, incarnation, TestContext.Current.CancellationToken);
        await Assert.ThrowsAsync<ApiException>(() => store.Sessions.CreateAsync(owner.User.Id, owner.Device, new(id, Guid.CreateVersion7(), "Other", owner.Device.Id, "Host", null), TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task SecondApprovedOwnerDeviceCanEditMetadataButNotSharingAndFriendsCannotEditEither()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner"); var friend = await store.UserAsync("friend");
        var second = new DeviceFixture(owner.User.Id, "second-device");
        var init = JsonSerializer.SerializeToElement(await store.Devices.StartLinkAsync(owner.User.Id,
            new(second.DeviceId, "Second", Convert.ToBase64String(second.KemKey), Convert.ToBase64String(second.SigningKey), DeviceFixture.LinkNonce, DeviceFixture.LinkProof), TestContext.Current.CancellationToken), Wire.Json);
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var cert = second.CertificateBody(owner.Device.Id, now);
        var list = DeviceFixture.ListBody(owner.User.Id, 2, [(owner.Device.Id, owner.Device.Id), (second.DeviceId, owner.Device.Id)], owner.Device.Id, now);
        await store.Devices.ApproveLinkAsync(owner.User.Id, owner.Device, new(init.GetProperty("requestId").GetGuid(), Convert.ToBase64String(cert),
            owner.Fixture.SignedCertificate(cert), Convert.ToBase64String(list), owner.Fixture.SignedList(list), DeviceFixture.LinkProof), TestContext.Current.CancellationToken);
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
        Assert.Equal(3, (await store.Db.Sessions.SingleAsync(TestContext.Current.CancellationToken)).AuthorizationRevision);
    }
}
