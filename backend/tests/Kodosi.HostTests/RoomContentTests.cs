using System.Text.Json;
using Kodosi.Data;
using Kodosi.Missions;
using Kodosi.Security;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class RoomContentTests(PostgresFixture postgres)
{
    [Fact]
    public async Task SignedContentKeepsOneOrderedHistoryAndRejectsStaleTaskUpdates()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var alice = await store.UserAsync("alice");
        var room = Guid.CreateVersion7(); var task = Guid.CreateVersion7();
        await store.Missions.CreateAsync(alice.User.Id, new(room, "Workspace"), ct);
        var state = JsonSerializer.SerializeToUtf8Bytes(new {
            roomId=room,ownerUserId=alice.User.Id,authorId=alice.User.Id,deviceId=alice.Device.Id,
            version=1,epoch=1,createdAtMs=DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),previousHash="",
            members=new Dictionary<Guid,object> { [alice.User.Id] = new {} },
            recipients=new[] { new { userId=alice.User.Id,deviceId=alice.Device.Id,kemCiphertext="AA==",@sealed=new { nonce="AA==",ciphertext="AA==" } } },
            previousKey=(object?)null
        }, Wire.Json);
        await store.Missions.StoreKeysAsync(room, alice.User.Id, new(alice.Device.Id,Convert.ToBase64String(state),
            Convert.ToBase64String(alice.Fixture.Sign(Proofs.Tagged("kodosi-room-state-v1"u8,state)))),ct);
        MissionService.ContentWrite Content(long expected, string kind="task") {
            var bytes = JsonSerializer.SerializeToUtf8Bytes(new { roomId=room,id=task,kind,version=expected+1,
                keyVersion=1,epoch=1,authorId=alice.User.Id,deviceId=alice.Device.Id,
                nonce=Convert.ToBase64String(new byte[12]),ciphertext=Convert.ToBase64String(new byte[20]) },Wire.Json);
            return new(expected,alice.Device.Id,Convert.ToBase64String(bytes),
                Convert.ToBase64String(alice.Fixture.Sign(Proofs.Tagged("kodosi-room-content-v1"u8,bytes))));
        }
        var first = Content(0);
        await store.Missions.PutContentAsync(room,task,alice.User.Id,first,ct);
        await store.Missions.PutContentAsync(room,task,alice.User.Id,first,ct);
        Assert.Equal(1,(await store.Db.MissionItems.SingleAsync(ct)).Sequence);
        await store.Missions.PutContentAsync(room,task,alice.User.Id,Content(1),ct);
        var stale = await Assert.ThrowsAsync<ApiException>(() => store.Missions.PutContentAsync(room,task,alice.User.Id,first,ct));
        Assert.Equal(409,stale.Status);
        var wrongType = await Assert.ThrowsAsync<ApiException>(() => store.Missions.PutContentAsync(room,task,alice.User.Id,Content(2,"message"),ct));
        Assert.Equal(409,wrongType.Status);
        var page = JsonSerializer.SerializeToElement(await store.Missions.ContentAsync(room,alice.User.Id,"task",0,null,1,ct),Wire.Json);
        Assert.Equal(2,page.GetProperty("sequence").GetInt64());
        Assert.Single(page.GetProperty("items").EnumerateArray());
        Assert.False(page.GetProperty("hasMore").GetBoolean());
        var badSignature = first with { Signature=Convert.ToBase64String(new byte[3309]) };
        await Assert.ThrowsAsync<ApiException>(() => store.Missions.PutContentAsync(room,task,alice.User.Id,badSignature,ct));
    }

    [Fact]
    public async Task AConcurrentTerminalShareCannotRejoinARoomAfterMemberRemoval()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var alice = await store.UserAsync("alice"); var bob = await store.UserAsync("bob");
        var room = Guid.CreateVersion7(); var terminal = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7();
        await store.Missions.CreateAsync(alice.User.Id,new(room,"Workspace"),ct);
        store.Db.MissionMembers.Add(new MissionMember { MissionId=room,UserId=bob.User.Id });
        await store.Db.SaveChangesAsync(ct);
        await store.Sessions.CreateAsync(bob.User.Id,bob.Device,new(terminal,incarnation,"Bob's terminal",bob.Device.Id,"Bob",null),ct);
        var pause = new PauseSave();
        await using var removal = store.Fork(pause); await using var attachment = store.Fork();
        var removing = removal.Missions.RemoveMemberAsync(room,alice.User.Id,bob.User.Id,ct);
        await pause.Reached.Task.WaitAsync(TimeSpan.FromSeconds(5),ct);
        var attaching = attachment.Sessions.AttachAsync(terminal,bob.User.Id,bob.Device.Id,new(incarnation,room),ct);
        try {
            Assert.NotSame(attaching,await Task.WhenAny(attaching,Task.Delay(150,ct)));
        } finally { pause.Resume.TrySetResult(); }
        await removing;
        await Assert.ThrowsAsync<ApiException>(async () => await attaching);
        store.Db.ChangeTracker.Clear();
        Assert.Null((await store.Db.Sessions.SingleAsync(ct)).MissionId);
        Assert.Single(await store.Sessions.ListAsync(bob.User.Id,ct));
    }

    private sealed class PauseSave : SaveChangesInterceptor
    {
        public TaskCompletionSource Reached { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Resume { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public override async ValueTask<InterceptionResult<int>> SavingChangesAsync(DbContextEventData data, InterceptionResult<int> result, CancellationToken ct=default)
        { Reached.TrySetResult(); await Resume.Task.WaitAsync(ct); return result; }
    }

    [Fact]
    public async Task RoomRecipientKeysOfAnAbsentMemberStayAvailableWhileTheIdentityIsRefused()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var alice = await store.UserAsync("alice"); var bob = await store.UserAsync("bob"); var carol = await store.UserAsync("carol");
        var (first, second) = Kodosi.Friends.FriendService.Pair(alice.User.Id, bob.User.Id);
        store.Db.Friendships.Add(new Friendship { FirstUserId = first, SecondUserId = second, RequestedBy = alice.User.Id, Accepted = true, CreatedAt = DateTimeOffset.UtcNow });
        await store.Db.SaveChangesAsync(ct);
        var key = new byte[MissionService.RoomPublicKeyLength];
        await store.Missions.RegisterRoomKeyAsync(bob.User.Id, bob.Device,
            new(Convert.ToBase64String(key), Convert.ToBase64String(bob.Fixture.Sign(MissionService.RoomKeyProof(bob.User.Id, bob.Device.Id, key)))), ct);
        var list = await store.Db.DeviceLists.SingleAsync(x => x.UserId == bob.User.Id, ct);
        list.ExpiresAtMs = DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds();
        await store.Db.SaveChangesAsync(ct);
        var refused = await Assert.ThrowsAsync<ApiException>(() => store.Devices.IdentityAsync(alice.User.Id, bob.User.Id, null, ct));
        Assert.Equal(400, refused.Status);
        var keys = JsonSerializer.SerializeToElement(await store.Missions.RecipientKeysAsync(alice.User.Id, bob.User.Id, ct), Wire.Json);
        Assert.Equal(bob.Device.Id, Assert.Single(keys.EnumerateArray()).GetProperty("deviceId").GetString());
        var stranger = await Assert.ThrowsAsync<ApiException>(() => store.Missions.RecipientKeysAsync(carol.User.Id, bob.User.Id, ct));
        Assert.Equal(404, stranger.Status);
    }
}
