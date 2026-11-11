using System.Text.Json;
using Kodosi.Data;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class ProjectTests(PostgresFixture postgres)
{
    [Fact]
    public async Task RenameAndDeletionNotifyMembersInviteesAndTerminalViewers()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner");
        var invitee = await store.UserAsync("invitee");
        await store.Friends.MutateAsync(owner.User.Id, "invitee", "send", TestContext.Current.CancellationToken);
        await store.Friends.MutateAsync(invitee.User.Id, "owner", "accept", TestContext.Current.CancellationToken);
        var projectId = Guid.CreateVersion7();
        await store.Projects.CreateAsync(owner.User.Id, new(projectId, "Before"), TestContext.Current.CancellationToken);
        var invitation = Guid.CreateVersion7();
        await store.Projects.InviteAsync(projectId, owner.User.Id, new(invitation, invitee.User.Id), TestContext.Current.CancellationToken);
        await store.Projects.ResolveInvitationAsync(invitation, invitee.User.Id, true, TestContext.Current.CancellationToken);
        var sessionId = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7();
        await store.Sessions.CreateAsync(owner.User.Id, owner.Approved, new(sessionId, incarnation, "Terminal", owner.Device.Id, "Host", projectId), TestContext.Current.CancellationToken);
        var socket = new EventSocket();
        await using var events = new DeviceConnection(socket, invitee.User.Id, invitee.Device.Id, store.Connections.Relays, TimeProvider.System);
        store.Connections.Register(events);
        await store.Projects.RenameAsync(projectId, owner.User.Id, "After", TestContext.Current.CancellationToken);
        var renamedSurfaces = new[] { await socket.NextSurfaceAsync(), await socket.NextSurfaceAsync() };
        Assert.Contains("projects", renamedSurfaces); Assert.Contains("sessions", renamedSurfaces);
        Assert.Equal("After", (await store.Sessions.ListAsync(invitee.User.Id, invitee.Device.Id, TestContext.Current.CancellationToken)).Single().ProjectName);
        await store.Projects.DeleteAsync(projectId, owner.User.Id, TestContext.Current.CancellationToken);
        var surfaces = new[] { await socket.NextSurfaceAsync(), await socket.NextSurfaceAsync() };
        Assert.Contains("projects", surfaces); Assert.Contains("sessions", surfaces);
        var listed = JsonSerializer.SerializeToElement(await store.Projects.ListAsync(invitee.User.Id, TestContext.Current.CancellationToken), TestJson.Options);
        Assert.Empty(listed.GetProperty("invitations").EnumerateArray());
        Assert.Empty(await store.Sessions.ListAsync(invitee.User.Id, invitee.Device.Id, TestContext.Current.CancellationToken));
        Assert.Null((await store.Sessions.ListAsync(owner.User.Id, owner.Device.Id, TestContext.Current.CancellationToken)).Single().ProjectId);
        Assert.Single(await store.Db.Sessions.ToListAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task DosiHasOneHiddenProjectThatOnlyItsOwnerUses()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner");
        var friend = await store.UserAsync("friend");
        await store.Friends.MutateAsync(owner.User.Id, "friend", "send", ct);
        await store.Friends.MutateAsync(friend.User.Id, "owner", "accept", ct);
        var dosi = Guid.CreateVersion7();
        await store.Projects.CreateAsync(owner.User.Id, new(dosi, "Dosi", Dosi: true), ct);
        await store.Projects.CreateAsync(owner.User.Id, new(dosi, "Dosi", Dosi: true), ct);
        await Assert.ThrowsAsync<ApiException>(() => store.Projects.CreateAsync(owner.User.Id, new(Guid.CreateVersion7(), "Dosi", Dosi: true), ct));
        var listed = JsonSerializer.SerializeToElement(await store.Projects.ListAsync(owner.User.Id, ct), TestJson.Options);
        Assert.Empty(listed.GetProperty("projects").EnumerateArray());
        Assert.Equal(dosi, listed.GetProperty("dosi").GetGuid());
        var other = JsonSerializer.SerializeToElement(await store.Projects.ListAsync(friend.User.Id, ct), TestJson.Options);
        Assert.Equal(JsonValueKind.Null, other.GetProperty("dosi").ValueKind);
        await Assert.ThrowsAsync<ApiException>(() => store.Projects.InviteAsync(dosi, owner.User.Id, new(Guid.CreateVersion7(), friend.User.Id), ct));
        await Assert.ThrowsAsync<ApiException>(() => store.Projects.RenameAsync(dosi, owner.User.Id, "Mine", ct));
        await Assert.ThrowsAsync<ApiException>(() => store.Sessions.CreateAsync(owner.User.Id, owner.Approved,
            new(Guid.CreateVersion7(), Guid.CreateVersion7(), "Terminal", owner.Device.Id, "Host", dosi), ct));
        var project = Guid.CreateVersion7();
        await store.Projects.CreateAsync(owner.User.Id, new(project, "Launch week"), ct);
        listed = JsonSerializer.SerializeToElement(await store.Projects.ListAsync(owner.User.Id, ct), TestJson.Options);
        Assert.Equal(project, listed.GetProperty("projects").EnumerateArray().Single().GetProperty("id").GetGuid());
    }

    [Fact]
    public async Task ProjectCapacityIsEnforcedBeforeAdmissionAndOverflowIsExplicit()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var viewer = await store.UserAsync("viewer");
        var owners = new List<Guid>();
        for (var i = 0; i < 5; i++) owners.Add((await store.UserAsync($"owner{i}")).User.Id);
        for (var i = 0; i < Limits.MaxVisibleProjects; i++)
        {
            var id = Guid.CreateVersion7();
            store.Db.Projects.Add(new Project { Id = id, OwnerUserId = owners[i % owners.Count], Name = $"Project {i}" });
            store.Db.ProjectMembers.Add(new ProjectMember { ProjectId = id, UserId = viewer.User.Id });
        }
        await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        await Assert.ThrowsAsync<ApiException>(() => store.Projects.CreateAsync(viewer.User.Id, new(Guid.CreateVersion7(), "One more"), TestContext.Current.CancellationToken));
        var result = JsonSerializer.SerializeToElement(await store.Projects.ListAsync(viewer.User.Id, TestContext.Current.CancellationToken), TestJson.Options);
        Assert.Equal(Limits.MaxVisibleProjects, result.GetProperty("projects").GetArrayLength());
        var extra = new Project { Id = Guid.CreateVersion7(), OwnerUserId = owners[0], Name = "Over the limit" };
        store.Db.Projects.Add(extra); store.Db.ProjectMembers.Add(new ProjectMember { ProjectId = extra.Id, UserId = viewer.User.Id });
        await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        result = JsonSerializer.SerializeToElement(await store.Projects.ListAsync(viewer.User.Id, TestContext.Current.CancellationToken), TestJson.Options);
        Assert.Equal(Limits.MaxVisibleProjects, result.GetProperty("projects").GetArrayLength());
        Assert.True(result.GetProperty("projectsTruncated").GetBoolean());
        var visibleId = result.GetProperty("projects")[0].GetProperty("id").GetGuid();
        await store.Projects.RemoveMemberAsync(visibleId, viewer.User.Id, viewer.User.Id, TestContext.Current.CancellationToken);
        result = JsonSerializer.SerializeToElement(await store.Projects.ListAsync(viewer.User.Id, TestContext.Current.CancellationToken), TestJson.Options);
        Assert.False(result.GetProperty("projectsTruncated").GetBoolean());
        Assert.Equal(Limits.MaxVisibleProjects, await store.Db.ProjectMembers.CountAsync(x => x.UserId == viewer.User.Id, TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task InvitationLimitPreservesExistingRequestsAndAllowsDeclining()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner"); var invitee = await store.UserAsync("invitee");
        await store.Friends.MutateAsync(owner.User.Id, "invitee", "send", TestContext.Current.CancellationToken);
        await store.Friends.MutateAsync(invitee.User.Id, "owner", "accept", TestContext.Current.CancellationToken);
        var otherOwner = await store.UserAsync("other-owner");
        var invitations = new List<ProjectInvitation>();
        for (var i = 0; i < Limits.MaxPendingProjectInvitations; i++)
        {
            var project = new Project { Id = Guid.CreateVersion7(), OwnerUserId = i < 64 ? owner.User.Id : otherOwner.User.Id, Name = "Project" };
            store.Db.Projects.Add(project);
            var invitation = new ProjectInvitation { Id = Guid.CreateVersion7(), ProjectId = project.Id, UserId = invitee.User.Id, InviterUserId = project.OwnerUserId, CreatedAt = DateTimeOffset.UtcNow };
            invitations.Add(invitation); store.Db.ProjectInvitations.Add(invitation);
        }
        var extra = new Project { Id = Guid.CreateVersion7(), OwnerUserId = owner.User.Id, Name = "Another" };
        store.Db.Projects.Add(extra);
        await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        await Assert.ThrowsAsync<ApiException>(() => store.Projects.InviteAsync(extra.Id, owner.User.Id, new(Guid.CreateVersion7(), invitee.User.Id), TestContext.Current.CancellationToken));
        Assert.Equal(Limits.MaxPendingProjectInvitations, await store.Db.ProjectInvitations.CountAsync(TestContext.Current.CancellationToken));
        await store.Projects.ResolveInvitationAsync(invitations[0].Id, invitee.User.Id, false, TestContext.Current.CancellationToken);
        await store.Projects.InviteAsync(extra.Id, owner.User.Id, new(Guid.CreateVersion7(), invitee.User.Id), TestContext.Current.CancellationToken);
        Assert.Equal(Limits.MaxPendingProjectInvitations, await store.Db.ProjectInvitations.CountAsync(TestContext.Current.CancellationToken));
    }
}

internal sealed class EventSocket : FakeSocket
{
    private readonly System.Threading.Channels.Channel<byte[]> frames = System.Threading.Channels.Channel.CreateUnbounded<byte[]>();
    public override Task SendAsync(ArraySegment<byte> buffer, System.Net.WebSockets.WebSocketMessageType type, bool endOfMessage, CancellationToken ct)
    { frames.Writer.TryWrite(buffer.ToArray()); return Task.CompletedTask; }
    public async Task<JsonElement> NextAsync()
    {
        var bytes = await frames.Reader.ReadAsync(TestContext.Current.CancellationToken).AsTask().WaitAsync(TimeSpan.FromSeconds(2), TestContext.Current.CancellationToken);
        using var json = JsonDocument.Parse(bytes);
        return json.RootElement.Clone();
    }
    public async Task<string?> NextSurfaceAsync() => (await NextAsync()).GetProperty("surface").GetString();
}
