using System.Security.Cryptography;
using System.Text.Json;
using Kodosi.Data;
using Kodosi.Projects;
using Kodosi.Plans;
using Kodosi.Security;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class ProjectContentTests(PostgresFixture postgres)
{
    [Fact]
    public async Task SignedContentKeepsOneOrderedHistoryAndRejectsStaleTaskUpdates()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var alice = await store.UserAsync("alice");
        var project = Guid.CreateVersion7(); var task = Guid.CreateVersion7();
        await store.Projects.CreateAsync(alice.User.Id, new(project, "Workspace"), ct);
        var state = JsonSerializer.SerializeToUtf8Bytes(new {
            roomId=project,ownerUserId=alice.User.Id,authorId=alice.User.Id,deviceId=alice.Device.Id,
            version=1,epoch=1,createdAtMs=DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),previousHash="",
            members=new Dictionary<Guid,object> { [alice.User.Id] = new {} },
            recipients=new[] { new { userId=alice.User.Id,deviceId=alice.Device.Id,kemCiphertext="AA==",@sealed=new { nonce="AA==",ciphertext="AA==" } } },
            previousKey=(object?)null
        }, TestJson.Options);
        await store.Projects.StoreKeysAsync(project, alice.User.Id, new(alice.Device.Id,Convert.ToBase64String(state),
            Convert.ToBase64String(alice.Fixture.Sign(Proofs.Tagged("kodosi-room-state-v1"u8,state)))),ct);
        ProjectService.ContentWrite Content(long expected, string kind="task") {
            var bytes = JsonSerializer.SerializeToUtf8Bytes(new { roomId=project,id=task,kind,version=expected+1,
                keyVersion=1,epoch=1,authorId=alice.User.Id,deviceId=alice.Device.Id,
                nonce=Convert.ToBase64String(new byte[12]),ciphertext=Convert.ToBase64String(new byte[20]) },TestJson.Options);
            return new(expected,alice.Device.Id,Convert.ToBase64String(bytes),
                Convert.ToBase64String(alice.Fixture.Sign(Proofs.Tagged("kodosi-room-content-v1"u8,bytes))));
        }
        var first = Content(0);
        await store.Projects.PutContentAsync(project,task,alice.User.Id,first,ct);
        await store.Projects.PutContentAsync(project,task,alice.User.Id,first,ct);
        Assert.Equal(1,(await store.Db.ProjectItems.SingleAsync(ct)).Sequence);
        await store.Projects.PutContentAsync(project,task,alice.User.Id,Content(1),ct);
        var stale = await Assert.ThrowsAsync<ApiException>(() => store.Projects.PutContentAsync(project,task,alice.User.Id,first,ct));
        Assert.Equal(409,stale.Status);
        var wrongType = await Assert.ThrowsAsync<ApiException>(() => store.Projects.PutContentAsync(project,task,alice.User.Id,Content(2,"message"),ct));
        Assert.Equal(409,wrongType.Status);
        var page = JsonSerializer.SerializeToElement(await store.Projects.ContentAsync(project,alice.User.Id,"task",0,null,1,ct),TestJson.Options);
        Assert.Equal(2,page.GetProperty("sequence").GetInt64());
        Assert.Single(page.GetProperty("items").EnumerateArray());
        Assert.False(page.GetProperty("hasMore").GetBoolean());
        var badSignature = first with { Signature=Convert.ToBase64String(new byte[IdentityWireFormat.MlDsa65SignatureLength]) };
        await Assert.ThrowsAsync<ApiException>(() => store.Projects.PutContentAsync(project,task,alice.User.Id,badSignature,ct));
    }

    [Fact]
    public async Task TheKeyHistoryComesInPagesOfABoundedSize()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var alice = await store.UserAsync("alice");
        var project = new Project { Id = Guid.CreateVersion7(), OwnerUserId = alice.User.Id, Name = "Workspace", KeyVersion = 10 };
        store.Db.Projects.Add(project);
        for (var version = 1; version <= 10; version++)
            store.Db.ProjectKeyStates.Add(new ProjectKeyState { ProjectId = project.Id, Version = version, Epoch = 1, Body = new byte[700_000],
                Signature = new byte[IdentityWireFormat.MlDsa65SignatureLength] });
        await store.Db.SaveChangesAsync(ct);
        async Task<long[]> PageAsync(long after)
        {
            var page = JsonSerializer.SerializeToElement(await store.Projects.KeyHistoryAsync(project.Id, alice.User.Id, after, ct), TestJson.Options);
            Assert.Equal(10, page.GetProperty("version").GetInt64());
            return page.GetProperty("states").EnumerateArray().Select(x => x.GetProperty("version").GetInt64()).ToArray();
        }
        Assert.Equal(new long[] { 1, 2, 3, 4, 5 }, await PageAsync(0));
        Assert.Equal(new long[] { 6, 7, 8, 9, 10 }, await PageAsync(5));
        Assert.Empty(await PageAsync(10));
        var stranger = await store.UserAsync("stranger");
        await Assert.ThrowsAsync<ApiException>(() => store.Projects.KeyHistoryAsync(project.Id, stranger.User.Id, 0, ct));
    }

    [Fact]
    public async Task MembersHearWhichProjectChangedAndItsNewContentSequence()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var alice = await store.UserAsync("alice");
        var project = Guid.CreateVersion7(); var message = Guid.CreateVersion7();
        await store.Projects.CreateAsync(alice.User.Id, new(project, "Workspace"), ct);
        var socket = new EventSocket();
        await using var link = new Kodosi.TerminalConnections.DeviceConnection(socket, alice.User.Id, alice.Device.Id, store.Connections.Relays, TimeProvider.System);
        store.Connections.Register(link);
        var state = JsonSerializer.SerializeToUtf8Bytes(new {
            roomId=project,ownerUserId=alice.User.Id,authorId=alice.User.Id,deviceId=alice.Device.Id,
            version=1,epoch=1,createdAtMs=DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),previousHash="",
            members=new Dictionary<Guid,object> { [alice.User.Id] = new {} },
            recipients=new[] { new { userId=alice.User.Id,deviceId=alice.Device.Id,kemCiphertext="AA==",@sealed=new { nonce="AA==",ciphertext="AA==" } } },
            previousKey=(object?)null
        }, TestJson.Options);
        await store.Projects.StoreKeysAsync(project, alice.User.Id, new(alice.Device.Id,Convert.ToBase64String(state),
            Convert.ToBase64String(alice.Fixture.Sign(Proofs.Tagged("kodosi-room-state-v1"u8,state)))),ct);
        var keys = await socket.NextAsync();
        Assert.Equal("projectContent", keys.GetProperty("surface").GetString());
        Assert.Equal(project, keys.GetProperty("projectId").GetGuid());
        Assert.False(keys.TryGetProperty("sequence", out _));
        var content = JsonSerializer.SerializeToUtf8Bytes(new { roomId=project,id=message,kind="message",version=1,
            keyVersion=1,epoch=1,authorId=alice.User.Id,deviceId=alice.Device.Id,
            nonce=Convert.ToBase64String(new byte[12]),ciphertext=Convert.ToBase64String(new byte[20]) },TestJson.Options);
        await store.Projects.PutContentAsync(project, message, alice.User.Id, new(0, alice.Device.Id, Convert.ToBase64String(content),
            Convert.ToBase64String(alice.Fixture.Sign(Proofs.Tagged("kodosi-room-content-v1"u8,content)))), ct);
        var posted = await socket.NextAsync();
        Assert.Equal("projectContent", posted.GetProperty("surface").GetString());
        Assert.Equal(project, posted.GetProperty("projectId").GetGuid());
        Assert.Equal(1, posted.GetProperty("sequence").GetInt64());
    }

    [Fact]
    public async Task AConcurrentTerminalShareCannotRejoinARoomAfterMemberRemoval()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var alice = await store.UserAsync("alice"); var bob = await store.UserAsync("bob");
        var project = Guid.CreateVersion7(); var terminal = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7();
        await store.Projects.CreateAsync(alice.User.Id,new(project,"Workspace"),ct);
        var history = await store.KeyProjectAsync(project);
        store.Db.ProjectMembers.Add(new ProjectMember { ProjectId=project,UserId=bob.User.Id });
        await store.Db.SaveChangesAsync(ct);
        await store.Sessions.CreateAsync(bob.User.Id,bob.Approved,new(terminal,incarnation,"Bob's terminal",bob.Device.Id,"Bob",null),ct);
        var pause = new PauseSave();
        await using var removal = store.Fork(pause); await using var attachment = store.Fork();
        var removing = removal.Projects.RemoveMemberAsync(project,alice.User.Id,bob.User.Id,ct);
        await pause.Reached.Task.WaitAsync(TimeSpan.FromSeconds(5),ct);
        var attaching = attachment.Sessions.AttachAsync(terminal, bob.User.Id, bob.Approved,
            new(incarnation, project, bob.ProjectChoice(terminal, incarnation, project, 1, history)), ct);
        try {
            Assert.NotSame(attaching,await Task.WhenAny(attaching,Task.Delay(150,ct)));
        } finally { pause.Resume.TrySetResult(); }
        await removing;
        await Assert.ThrowsAsync<ApiException>(async () => await attaching);
        store.Db.ChangeTracker.Clear();
        Assert.Null((await store.Db.Sessions.SingleAsync(ct)).ProjectId);
        Assert.Single(await store.Sessions.ListAsync(bob.User.Id, bob.Device.Id, ct));
    }

    private sealed class PauseSave : SaveChangesInterceptor
    {
        public TaskCompletionSource Reached { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Resume { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public override async ValueTask<InterceptionResult<int>> SavingChangesAsync(DbContextEventData data, InterceptionResult<int> result, CancellationToken ct=default)
        { Reached.TrySetResult(); await Resume.Task.WaitAsync(ct); return result; }
    }

    [Fact]
    public async Task ProjectRecipientKeysOfAnAbsentMemberStayAvailableWhileTheIdentityIsRefused()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var alice = await store.UserAsync("alice"); var bob = await store.UserAsync("bob"); var carol = await store.UserAsync("carol");
        var (first, second) = Kodosi.Friends.FriendService.Pair(alice.User.Id, bob.User.Id);
        store.Db.Friendships.Add(new Friendship { FirstUserId = first, SecondUserId = second, RequestedBy = alice.User.Id, Accepted = true, CreatedAt = DateTimeOffset.UtcNow });
        await store.Db.SaveChangesAsync(ct);
        var key = new byte[ProjectService.ProjectPublicKeyLength];
        await store.Projects.RegisterProjectKeyAsync(bob.User.Id, bob.Approved,
            new(Convert.ToBase64String(key), Convert.ToBase64String(bob.Fixture.Sign(Proofs.ProjectRecipient(bob.User.Id, bob.Device.Id, key)))), ct);
        var list = await store.Db.DeviceLists.SingleAsync(x => x.UserId == bob.User.Id, ct);
        list.ExpiresAtMs = DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds();
        await store.Db.SaveChangesAsync(ct);
        var refused = await Assert.ThrowsAsync<ApiException>(() => store.Devices.IdentityAsync(alice.User.Id, bob.User.Id, null, ct));
        Assert.Equal(400, refused.Status);
        var keys = JsonSerializer.SerializeToElement(await store.Projects.RecipientKeysAsync(alice.User.Id, bob.User.Id, ct), TestJson.Options);
        Assert.Equal(bob.Device.Id, Assert.Single(keys.EnumerateArray()).GetProperty("deviceId").GetString());
        var stranger = await Assert.ThrowsAsync<ApiException>(() => store.Projects.RecipientKeysAsync(carol.User.Id, bob.User.Id, ct));
        Assert.Equal(404, stranger.Status);
    }

    [Fact]
    public async Task ADeviceKeepsTheProjectKeyItRegisteredFirst()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var bob = await store.UserAsync("bob");
        ProjectService.RecipientKeyWrite Key(byte fill)
        {
            var key = Enumerable.Repeat(fill, ProjectService.ProjectPublicKeyLength).ToArray();
            return new(Convert.ToBase64String(key), Convert.ToBase64String(bob.Fixture.Sign(Proofs.ProjectRecipient(bob.User.Id, bob.Device.Id, key))));
        }
        await store.Projects.RegisterProjectKeyAsync(bob.User.Id, bob.Approved, Key(1), ct);
        await store.Projects.RegisterProjectKeyAsync(bob.User.Id, bob.Approved, Key(1), ct);
        var changed = await Assert.ThrowsAsync<ApiException>(() => store.Projects.RegisterProjectKeyAsync(bob.User.Id, bob.Approved, Key(2), ct));
        Assert.Equal(409, changed.Status);
        Assert.Equal(1, (await store.Db.ProjectRecipientKeys.SingleAsync(x => x.DeviceId == bob.Device.Id, ct)).PublicKey[0]);
    }

    [Fact]
    public async Task TheNextKeysOfARoomKeepAMemberWhoseAccountWasDeletedButNoOtherPerson()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner"); var bob = await store.UserAsync("bob"); var carol = await store.UserAsync("carol");
        var dave = await store.UserAsync("dave");
        var project = Guid.CreateVersion7();
        await store.Projects.CreateAsync(owner.User.Id, new(project, "Workspace"), ct);
        store.Db.ProjectMembers.AddRange(new ProjectMember { ProjectId = project, UserId = bob.User.Id }, new ProjectMember { ProjectId = project, UserId = carol.User.Id });
        await store.Db.SaveChangesAsync(ct);
        Guid[] members = [owner.User.Id, bob.User.Id, carol.User.Id];
        async Task<string> StoreAsync(TestUser author, long version, long epoch, string previous, Guid[] named, TestUser[] recipients)
        {
            var state = KeyState(project, owner, author, version, epoch, previous, named, recipients);
            await store.Projects.StoreKeysAsync(project, author.User.Id, Signed(author, state), ct);
            return Convert.ToHexStringLower(SHA256.HashData(state));
        }
        var history = await StoreAsync(owner, 1, 1, "", members, [owner, bob, carol]);
        await store.Accounts.DeleteSignedInAsync(carol.User, DateTimeOffset.UtcNow, ct);

        history = await StoreAsync(owner, 2, 2, history, members, [owner, bob]);
        foreach (var other in new[] { dave.User.Id, Guid.CreateVersion7() })
        {
            var refused = await Assert.ThrowsAsync<ApiException>(() => StoreAsync(owner, 3, 2, history, [.. members, other], [owner, bob]));
            Assert.Equal(409, refused.Status);
        }
        await StoreAsync(bob, 3, 2, history, members, [owner, bob]);
    }

    [Fact]
    public async Task ARoomKeyStateNamesNoMorePeopleThanARoomCanHave()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner"); var bob = await store.UserAsync("bob");
        await store.Friends.MutateAsync(owner.User.Id, "bob", "send", ct);
        await store.Friends.MutateAsync(bob.User.Id, "owner", "accept", ct);
        var project = Guid.CreateVersion7();
        await store.Projects.CreateAsync(owner.User.Id, new(project, "Workspace"), ct);
        Guid[] members = [owner.User.Id, .. Enumerable.Range(1, PlanLimits.Safety.ProjectPeople - 1).Select(_ => Guid.CreateVersion7())];
        var first = KeyState(project, owner, owner, 1, 1, "", members, [owner]);
        store.Db.ProjectKeyStates.Add(new ProjectKeyState { ProjectId = project, Version = 1, Epoch = 1, Body = first, Signature = new byte[IdentityWireFormat.MlDsa65SignatureLength] });
        (await store.Db.Projects.SingleAsync(x => x.Id == project, ct)).KeyVersion = 1;
        await store.Db.SaveChangesAsync(ct);

        var next = KeyState(project, owner, owner, 2, 1, Convert.ToHexStringLower(SHA256.HashData(first)), [.. members, bob.User.Id], [owner, bob]);
        var refused = await Assert.ThrowsAsync<ApiException>(() =>
            store.Projects.InviteAsync(project, owner.User.Id, new(Guid.CreateVersion7(), bob.User.Id, Signed(owner, next)), ct));
        Assert.Equal(("projectPeople", PlanLimits.Safety.ProjectPeople), (refused.Limit, refused.Most));
        store.Db.ChangeTracker.Clear();
        Assert.Empty(await store.Db.ProjectInvitations.ToListAsync(ct));
    }

    private static byte[] KeyState(Guid project, TestUser owner, TestUser author, long version, long epoch, string previousHash, Guid[] members,
        TestUser[] recipients) => JsonSerializer.SerializeToUtf8Bytes(new
    {
        roomId = project, ownerUserId = owner.User.Id, authorId = author.User.Id, deviceId = author.Device.Id,
        version, epoch, createdAtMs = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(), previousHash,
        members = members.ToDictionary(x => x, _ => new { }),
        recipients = recipients.Select(x => new { userId = x.User.Id, deviceId = x.Device.Id, kemCiphertext = "AA==", @sealed = new { nonce = "AA==", ciphertext = "AA==" } }),
        previousKey = (object?)null
    }, TestJson.Options);

    private static ProjectService.ProjectKeyWrite Signed(TestUser author, byte[] state) =>
        new(author.Device.Id, Convert.ToBase64String(state), Convert.ToBase64String(author.Fixture.Sign(Proofs.Tagged("kodosi-room-state-v1"u8, state))));
}
