using Kodosi.Data;
using Kodosi.Security;
using Kodosi.Sessions;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class OfflineTerminalTests(PostgresFixture postgres)
{
    private static async Task<(TestUser Owner, TestUser Friend, Guid Id, Guid Incarnation)> SharedAsync(TestStore store, CancellationToken ct)
    {
        var owner = await store.UserAsync("owner"); var friend = await store.UserAsync("friend");
        await store.Friends.MutateAsync(owner.User.Id, "friend", "send", ct);
        await store.Friends.MutateAsync(friend.User.Id, "owner", "accept", ct);
        var id = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7();
        await store.Sessions.CreateAsync(owner.User.Id, owner.Approved, new(id, incarnation, "Terminal", owner.Device.Id, "Host", null), ct);
        await store.Sessions.ShareAsync(id, owner.User.Id, owner.Device.Id, new(incarnation, 1, [friend.User.Id]), ct);
        return (owner, friend, id, incarnation);
    }

    [Fact]
    public async Task AHiddenTerminalLeavesOnlyTheListOfItsPersonAndComesBackWithItsHost()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var (owner, friend, id, incarnation) = await SharedAsync(store, ct);
        async Task<bool> HiddenAsync(TestUser person) => (await store.Sessions.ListAsync(person.User.Id, person.Device.Id, ct)).Single().Hidden;

        Assert.Equal(409, (await Assert.ThrowsAsync<ApiException>(() => store.Sessions.HideAsync(id, friend.User.Id, new(Guid.CreateVersion7()), ct))).Status);
        await store.Sessions.HideAsync(id, friend.User.Id, new(incarnation), ct);
        await store.Sessions.HideAsync(id, friend.User.Id, new(incarnation), ct);
        Assert.True(await HiddenAsync(friend));
        Assert.False(await HiddenAsync(owner));

        var socket = new EventSocket();
        await using var host = new DeviceConnection(socket, owner.User.Id, owner.Device.Id, store.Connections.Relays, TimeProvider.System, true);
        store.Connections.Register(host);
        store.Connections.Host(host, await store.Db.Sessions.AsNoTracking().SingleAsync(x => x.Id == id, ct));
        Assert.False(await HiddenAsync(friend));
        await store.Sessions.ShowAgainAsync(id, ct);
        await store.Sessions.HideAsync(id, friend.User.Id, new(incarnation), ct);
        Assert.Empty(await store.Db.HiddenSessions.ToListAsync(ct));
        store.Connections.Remove(host);
        Assert.False(await HiddenAsync(friend));
    }

    [Fact]
    public async Task OnlyASignedCloseRequestTakesATerminalOutOfEveryListButThatOfItsHost()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var (owner, friend, id, incarnation) = await SharedAsync(store, ct);
        var now = DateTimeOffset.UtcNow;
        for (var index = 1; index < Limits.MaxSessionsPerUser; index++)
            store.Db.Sessions.Add(new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), OwnerUserId = owner.User.Id,
                HostDeviceId = owner.Device.Id, HostName = "Host", Name = "Terminal", CreatedAt = now, ExpiresAt = now.AddMinutes(2) });
        await store.Db.SaveChangesAsync(ct);
        Task CreateAsync() => store.Sessions.CreateAsync(owner.User.Id, owner.Approved,
            new(Guid.CreateVersion7(), Guid.CreateVersion7(), "Another", owner.Device.Id, "Host", null), ct);
        SessionService.RequestClose Request(TestUser person, DeviceFixture signer) => new(incarnation, person.Device.Id,
            Convert.ToBase64String(signer.Sign(Proofs.TerminalClose(owner.User.Id, id, incarnation, person.User.Id, person.Device.Id))));
        Task AskAsync(TestUser person, SessionService.RequestClose request) =>
            store.Sessions.RequestCloseAsync(id, person.User.Id, person.Approved, request, ct);
        var socket = new EventSocket();
        await using var host = new DeviceConnection(socket, owner.User.Id, owner.Device.Id, store.Connections.Relays, TimeProvider.System, true);
        store.Connections.Register(host);
        store.Connections.Host(host, await store.Db.Sessions.AsNoTracking().SingleAsync(x => x.Id == id, ct));

        Assert.Equal(409, (await Assert.ThrowsAsync<ApiException>(CreateAsync)).Status);
        var forged = new DeviceFixture(friend.User.Id, friend.Device.Id);
        Assert.Equal(403, (await Assert.ThrowsAsync<ApiException>(() => AskAsync(friend, Request(friend, forged)))).Status);
        Assert.Equal(403, (await Assert.ThrowsAsync<ApiException>(() => AskAsync(friend, Request(friend, friend.Fixture) with { DeviceId = "other" }))).Status);
        Assert.Equal(403, (await Assert.ThrowsAsync<ApiException>(() => AskAsync(friend, Request(owner, owner.Fixture)))).Status);
        await AskAsync(friend, Request(friend, friend.Fixture));
        Assert.Equal("sessions", await socket.NextSurfaceAsync());
        Assert.Equal("accessChanged", (await socket.NextAsync()).GetProperty("type").GetString());
        Assert.Empty(await store.Sessions.ListAsync(friend.User.Id, friend.Device.Id, ct));
        Assert.DoesNotContain(await store.Sessions.ListAsync(owner.User.Id, "other-device", ct), x => x.Id == id);
        var closing = Assert.Single(await store.Sessions.ListAsync(owner.User.Id, owner.Device.Id, ct), x => x.Id == id).CloseRequest!;
        Assert.Equal((friend.User.Id, friend.Device.Id), (closing.UserId, closing.DeviceId));
        await CreateAsync();

        await AskAsync(owner, Request(owner, owner.Fixture));
        store.Db.ChangeTracker.Clear();
        Assert.Equal(friend.User.Id, Assert.Single(await store.Sessions.ListAsync(owner.User.Id, owner.Device.Id, ct), x => x.Id == id).CloseRequest?.UserId);
        Assert.Equal(403, (await Assert.ThrowsAsync<ApiException>(() => store.Sessions.RefuseCloseAsync(id, owner.User.Id, "other-device", incarnation, ct))).Status);
        await store.Sessions.RefuseCloseAsync(id, owner.User.Id, owner.Device.Id, incarnation, ct);
        Assert.Null(Assert.Single(await store.Sessions.ListAsync(friend.User.Id, friend.Device.Id, ct)).CloseRequest);
    }
}
