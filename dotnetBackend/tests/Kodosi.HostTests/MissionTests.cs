using System.Text.Json;
using Kodosi.Data;
using Kodosi.Realtime;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Infrastructure;
using Microsoft.EntityFrameworkCore.Migrations;
using Npgsql;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class MissionTests(PostgresFixture postgres)
{
    [Fact]
    public async Task RenameAndDeletionNotifyMembersInviteesAndTerminalViewers()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner");
        var invitee = await store.UserAsync("invitee");
        await store.Friends.MutateAsync(owner.User.Id, "invitee", "send", TestContext.Current.CancellationToken);
        await store.Friends.MutateAsync(invitee.User.Id, "owner", "accept", TestContext.Current.CancellationToken);
        var roomId = Guid.CreateVersion7();
        await store.Rooms.CreateAsync(owner.User.Id, new(roomId, "Before"), TestContext.Current.CancellationToken);
        await store.Rooms.InviteAsync(roomId, owner.User.Id, new(Guid.CreateVersion7(), invitee.User.Id), TestContext.Current.CancellationToken);
        var sessionId = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7();
        await store.Sessions.CreateAsync(owner.User.Id, owner.Device, new(sessionId, incarnation, "Terminal", owner.Device.Id, "Host", roomId), TestContext.Current.CancellationToken);
        await store.Sessions.ShareAsync(sessionId, owner.User.Id, owner.Device.Id, new(incarnation, 1, [invitee.User.Id]), TestContext.Current.CancellationToken);
        var socket = new EventSocket();
        await using var events = new SocketPeer(socket, invitee.User.Id, invitee.Device.Id, Guid.CreateVersion7().ToString());
        Assert.True(store.Relay.RegisterEvents(events));
        await store.Rooms.RenameAsync(roomId, owner.User.Id, "After", TestContext.Current.CancellationToken);
        Assert.Equal("sessions", await socket.NextSurfaceAsync());
        Assert.Equal("After", (await store.Sessions.ListAsync(invitee.User.Id, TestContext.Current.CancellationToken)).Single().RoomName);
        await store.Rooms.DeleteAsync(roomId, owner.User.Id, TestContext.Current.CancellationToken);
        var surfaces = new[] { await socket.NextSurfaceAsync(), await socket.NextSurfaceAsync() };
        Assert.Contains("rooms", surfaces); Assert.Contains("sessions", surfaces);
        var listed = JsonSerializer.SerializeToElement(await store.Rooms.ListAsync(invitee.User.Id, TestContext.Current.CancellationToken), Wire.Json);
        Assert.Empty(listed.GetProperty("invitations").EnumerateArray());
        Assert.Null((await store.Sessions.ListAsync(invitee.User.Id, TestContext.Current.CancellationToken)).Single().RoomId);
        Assert.Single(await store.Db.Sessions.ToListAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task MissionCapacityIsEnforcedBeforeAdmissionAndOverflowIsExplicit()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var viewer = await store.UserAsync("viewer");
        var owners = new List<Guid>();
        for (var i = 0; i < 5; i++) owners.Add((await store.UserAsync($"owner{i}")).User.Id);
        for (var i = 0; i < Limits.MaxVisibleRooms; i++)
        {
            var id = Guid.CreateVersion7();
            store.Db.Rooms.Add(new Room { Id = id, OwnerUserId = owners[i % owners.Count], Name = $"Project {i}" });
            store.Db.RoomMembers.Add(new RoomMember { RoomId = id, UserId = viewer.User.Id });
        }
        await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        await Assert.ThrowsAsync<ApiException>(() => store.Rooms.CreateAsync(viewer.User.Id, new(Guid.CreateVersion7(), "One more"), TestContext.Current.CancellationToken));
        var result = JsonSerializer.SerializeToElement(await store.Rooms.ListAsync(viewer.User.Id, TestContext.Current.CancellationToken), Wire.Json);
        Assert.Equal(Limits.MaxVisibleRooms, result.GetProperty("rooms").GetArrayLength());
        var extra = new Room { Id = Guid.CreateVersion7(), OwnerUserId = owners[0], Name = "Legacy excess" };
        store.Db.Rooms.Add(extra); store.Db.RoomMembers.Add(new RoomMember { RoomId = extra.Id, UserId = viewer.User.Id });
        await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        result = JsonSerializer.SerializeToElement(await store.Rooms.ListAsync(viewer.User.Id, TestContext.Current.CancellationToken), Wire.Json);
        Assert.Equal(Limits.MaxVisibleRooms, result.GetProperty("rooms").GetArrayLength());
        Assert.True(result.GetProperty("roomsTruncated").GetBoolean());
        var visibleId = result.GetProperty("rooms")[0].GetProperty("id").GetGuid();
        await store.Rooms.RemoveMemberAsync(visibleId, viewer.User.Id, viewer.User.Id, TestContext.Current.CancellationToken);
        result = JsonSerializer.SerializeToElement(await store.Rooms.ListAsync(viewer.User.Id, TestContext.Current.CancellationToken), Wire.Json);
        Assert.False(result.GetProperty("roomsTruncated").GetBoolean());
        Assert.Equal(Limits.MaxVisibleRooms, await store.Db.RoomMembers.CountAsync(x => x.UserId == viewer.User.Id, TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task InvitationLimitPreservesExistingRequestsAndAllowsDeclining()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner"); var invitee = await store.UserAsync("invitee");
        await store.Friends.MutateAsync(owner.User.Id, "invitee", "send", TestContext.Current.CancellationToken);
        await store.Friends.MutateAsync(invitee.User.Id, "owner", "accept", TestContext.Current.CancellationToken);
        var otherOwner = await store.UserAsync("other-owner");
        var invitations = new List<RoomInvitation>();
        for (var i = 0; i < Limits.MaxPendingRoomInvitations; i++)
        {
            var room = new Room { Id = Guid.CreateVersion7(), OwnerUserId = i < 64 ? owner.User.Id : otherOwner.User.Id, Name = "Project" };
            store.Db.Rooms.Add(room);
            var invitation = new RoomInvitation { Id = Guid.CreateVersion7(), RoomId = room.Id, UserId = invitee.User.Id, InviterUserId = room.OwnerUserId, CreatedAt = DateTimeOffset.UtcNow };
            invitations.Add(invitation); store.Db.RoomInvitations.Add(invitation);
        }
        var extra = new Room { Id = Guid.CreateVersion7(), OwnerUserId = owner.User.Id, Name = "Another" };
        store.Db.Rooms.Add(extra);
        await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        await Assert.ThrowsAsync<ApiException>(() => store.Rooms.InviteAsync(extra.Id, owner.User.Id, new(Guid.CreateVersion7(), invitee.User.Id), TestContext.Current.CancellationToken));
        Assert.Equal(Limits.MaxPendingRoomInvitations, await store.Db.RoomInvitations.CountAsync(TestContext.Current.CancellationToken));
        await store.Rooms.ResolveInvitationAsync(invitations[0].Id, invitee.User.Id, false, TestContext.Current.CancellationToken);
        await store.Rooms.InviteAsync(extra.Id, owner.User.Id, new(Guid.CreateVersion7(), invitee.User.Id), TestContext.Current.CancellationToken);
        Assert.Equal(Limits.MaxPendingRoomInvitations, await store.Db.RoomInvitations.CountAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task SlugRetirementPreservesExistingDataAndNewMissionsNeedOnlyNames()
    {
        var connection = await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken);
        await using var db = PostgresFixture.Context(connection);
        await db.GetService<IMigrator>().MigrateAsync("20260911165358_TerminalFirst", TestContext.Current.CancellationToken);
        var owner = Guid.CreateVersion7(); var oldRoom = Guid.CreateVersion7();
        await using (var sql = new NpgsqlConnection(connection))
        {
            await sql.OpenAsync(TestContext.Current.CancellationToken);
            await using var command = new NpgsqlCommand("""
                INSERT INTO users ("Id","Issuer","Subject","Handle","DisplayName","Email","AvatarUrl","IdentityRevision")
                VALUES (@owner,'https://fixture.invalid/','owner','owner','Owner','saved@example.invalid','https://saved.invalid/avatar',0);
                INSERT INTO rooms ("Id","Name","Slug","OwnerUserId") VALUES (@room,'Existing','keep-this-slug',@owner);
                """, sql);
            command.Parameters.AddWithValue("owner", owner); command.Parameters.AddWithValue("room", oldRoom);
            await command.ExecuteNonQueryAsync(TestContext.Current.CancellationToken);
        }
        await DatabaseSetup.InitializeAsync(db, TestContext.Current.CancellationToken);
        Assert.Equal("keep-this-slug", await db.Rooms.Where(x => x.Id == oldRoom).Select(x => EF.Property<string>(x, "Slug")).SingleAsync(TestContext.Current.CancellationToken));
        var retained = await db.Users.SingleAsync(TestContext.Current.CancellationToken);
        Assert.Equal("saved@example.invalid", retained.Email); Assert.Equal("https://saved.invalid/avatar", retained.AvatarUrl);
        db.Rooms.Add(new Room { Id = Guid.CreateVersion7(), OwnerUserId = owner, Name = "Existing" });
        db.Rooms.Add(new Room { Id = Guid.CreateVersion7(), OwnerUserId = owner, Name = "Existing" });
        await db.SaveChangesAsync(TestContext.Current.CancellationToken);
        Assert.Equal(3, await db.Rooms.CountAsync(TestContext.Current.CancellationToken));
        await Assert.ThrowsAsync<PostgresException>(() => db.GetService<IMigrator>().MigrateAsync("20260911165358_TerminalFirst", TestContext.Current.CancellationToken));
        db.ChangeTracker.Clear();
        Assert.Equal(3, await db.Rooms.CountAsync(TestContext.Current.CancellationToken));
        Assert.Equal("keep-this-slug", await db.Rooms.Where(x => x.Id == oldRoom).Select(x => EF.Property<string>(x, "Slug")).SingleAsync(TestContext.Current.CancellationToken));
    }

    private sealed class EventSocket : FakeSocket
    {
        private readonly System.Threading.Channels.Channel<byte[]> frames = System.Threading.Channels.Channel.CreateUnbounded<byte[]>();
        public override Task SendAsync(ArraySegment<byte> buffer, System.Net.WebSockets.WebSocketMessageType type, bool endOfMessage, CancellationToken ct)
        { frames.Writer.TryWrite(buffer.ToArray()); return Task.CompletedTask; }
        public async Task<string?> NextSurfaceAsync()
        {
            var bytes = await frames.Reader.ReadAsync(TestContext.Current.CancellationToken).AsTask().WaitAsync(TimeSpan.FromSeconds(2), TestContext.Current.CancellationToken);
            using var json = JsonDocument.Parse(bytes);
            return json.RootElement.GetProperty("surface").GetString();
        }
    }
}
