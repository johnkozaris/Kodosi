using Kodosi.Application;
using Kodosi.Domain;

using static Kodosi.HostTests.SecurityAuthorizationTestFactory;

namespace Kodosi.HostTests;

public sealed class SessionUpdaterAuthorizationTests
{
    [Fact]
    public async Task SessionUpdate_Allows_Owner_To_Rename_Title()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.JustMe);
        var service = CreateSessionUpdater(session);

        var result = await service.UpdateOwnedAsync(
            session.Id,
            ownerId,
            new UpdateSessionRequest(
                session.IncarnationId,
                "Renamed Session",
                null,
                null,
                null),
            TestContext.Current.CancellationToken);

        Assert.Equal("Renamed Session", result.Response.Title);
        Assert.Equal("Renamed Session", session.Title);
    }

    [Fact]
    public async Task SessionUpdate_Rejects_Stale_Incarnation_After_Republish()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var staleIncarnationId = session.IncarnationId;
        session.End();
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            ownerId,
            "replacement",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "replacement-secret",
            roomId: null);
        var service = CreateSessionUpdater(session);

        await Assert.ThrowsAsync<NotFoundException>(() =>
            service.UpdateOwnedAsync(
                session.Id,
                ownerId,
                new UpdateSessionRequest(
                    staleIncarnationId,
                    "stale rename",
                    null,
                    null,
                    null),
                TestContext.Current.CancellationToken));

        Assert.Equal("replacement", session.Title);
    }

    [Fact]
    public async Task ScopeChange_Returns_Lifecycle_Lease_For_PostCommit_Fanout()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var authority = new FakeSessionEndAuthority();
        var service = CreateSessionUpdater(
            session,
            sessionEndAuthority: authority);

        var result = await service.UpdateOwnedAsync(
            session.Id,
            ownerId,
            new UpdateSessionRequest(
                session.IncarnationId,
                null,
                SessionScope.JustMe,
                null,
                null),
            TestContext.Current.CancellationToken);

        Assert.True(authority.LeaseHeld);
        await result.DisposeAsync();
        Assert.False(authority.LeaseHeld);
    }

    [Fact]
    public async Task SessionUpdate_Rejects_RoomScope_For_NonMember()
    {
        var ownerId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = CreateSession(ownerId, SessionScope.JustMe);
        var service = CreateSessionUpdater(
            session,
            roomMemberRepository: new FakeRoomMemberRepository());

        await Assert.ThrowsAsync<PolicyViolationException>(() => service.UpdateOwnedAsync(
            session.Id,
            ownerId,
            new UpdateSessionRequest(
                session.IncarnationId,
                null,
                SessionScope.Room,
                null,
                roomId.Value),
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task SessionUpdate_Clears_RoomId_When_Leaving_Room()
    {
        var ownerId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = CreateSession(ownerId, SessionScope.Room, roomId);
        var service = CreateSessionUpdater(
            session,
            roomMemberRepository: new FakeRoomMemberRepository((roomId, ownerId)));

        var result = await service.UpdateOwnedAsync(
            session.Id,
            ownerId,
            new UpdateSessionRequest(
                session.IncarnationId,
                null,
                SessionScope.Friends,
                null,
                roomId.Value),
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionScope.Friends, result.Response.Scope);
        Assert.Null(result.Response.RoomId);
        Assert.Null(session.RoomId);
    }

    [Fact]
    public async Task SessionUpdate_Rejects_Inject_DefaultAccess()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var service = CreateSessionUpdater(session);

        await Assert.ThrowsAsync<DomainException>(() => service.UpdateOwnedAsync(
            session.Id,
            ownerId,
            new UpdateSessionRequest(
                session.IncarnationId,
                null,
                null,
                (DefaultAudienceAccess)2,
                null),
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task SessionUpdate_Revokes_Orphaned_Override_When_Scope_Narrows_Friends_To_Private()
    {
        var ownerId = UserId.New();
        var eveId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var existingOverride = SessionAccessOverride.Create(
            session.Id,
            eveId,
            AccessLevel.Inject,
            ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new FakeAccessOverrideRepository(existingOverride);
        var service = CreateSessionUpdater(
            session,
            accessOverrideRepository: overrides);

        await service.UpdateOwnedAsync(
            session.Id,
            ownerId,
            new UpdateSessionRequest(
                session.IncarnationId,
                null,
                SessionScope.JustMe,
                null,
                null),
            TestContext.Current.CancellationToken);

        var afterUpdate = await overrides.GetAsync(
            session.Id,
            eveId,
            TestContext.Current.CancellationToken);
        Assert.NotNull(afterUpdate);
        Assert.False(afterUpdate!.IsActiveAt(DateTimeOffset.UtcNow));
        Assert.NotNull(afterUpdate.RevokedAt);
    }

    [Fact]
    public async Task SessionUpdate_Keeps_Override_When_Grantee_Still_Admissible_Under_New_Scope()
    {
        var ownerId = UserId.New();
        var charlieId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = CreateSession(ownerId, SessionScope.Room, roomId);
        var existingOverride = SessionAccessOverride.Create(
            session.Id,
            charlieId,
            AccessLevel.Inject,
            ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new FakeAccessOverrideRepository(existingOverride);
        var service = CreateSessionUpdater(
            session,
            friendshipRepository: new FakeFriendshipRepository(friendIds: [charlieId]),
            roomMemberRepository: new FakeRoomMemberRepository((roomId, ownerId)),
            accessOverrideRepository: overrides);

        await service.UpdateOwnedAsync(
            session.Id,
            ownerId,
            new UpdateSessionRequest(
                session.IncarnationId,
                null,
                SessionScope.Friends,
                null,
                null),
            TestContext.Current.CancellationToken);

        var afterUpdate = await overrides.GetAsync(
            session.Id,
            charlieId,
            TestContext.Current.CancellationToken);
        Assert.NotNull(afterUpdate);
        Assert.True(afterUpdate!.IsActiveAt(DateTimeOffset.UtcNow));
    }

    [Fact]
    public async Task SessionUpdate_Revokes_Former_Friend_Override_On_Friends_To_Room()
    {
        var ownerId = UserId.New();
        var bobId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = CreateSession(ownerId, SessionScope.Friends);
        var existingOverride = SessionAccessOverride.Create(
            session.Id,
            bobId,
            AccessLevel.Suggest,
            ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new FakeAccessOverrideRepository(existingOverride);
        var roomMembers = new FakeRoomMemberRepository((roomId, ownerId));
        var service = CreateSessionUpdater(
            session,
            friendshipRepository: new FakeFriendshipRepository(friendIds: [bobId]),
            roomMemberRepository: roomMembers,
            accessOverrideRepository: overrides);

        await service.UpdateOwnedAsync(
            session.Id,
            ownerId,
            new UpdateSessionRequest(
                session.IncarnationId,
                null,
                SessionScope.Room,
                null,
                roomId.Value),
            TestContext.Current.CancellationToken);

        var afterUpdate = await overrides.GetAsync(
            session.Id,
            bobId,
            TestContext.Current.CancellationToken);
        Assert.NotNull(afterUpdate);
        Assert.False(afterUpdate!.IsActiveAt(DateTimeOffset.UtcNow));
    }

    [Fact]
    public async Task SessionUpdate_Keeps_Override_On_Friends_To_Room_When_Grantee_Is_Member()
    {
        var ownerId = UserId.New();
        var charlieId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = CreateSession(ownerId, SessionScope.Friends);
        var existingOverride = SessionAccessOverride.Create(
            session.Id,
            charlieId,
            AccessLevel.Inject,
            ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new FakeAccessOverrideRepository(existingOverride);
        var roomMembers = new FakeRoomMemberRepository(
            (roomId, ownerId),
            (roomId, charlieId));
        var service = CreateSessionUpdater(
            session,
            roomMemberRepository: roomMembers,
            accessOverrideRepository: overrides);

        await service.UpdateOwnedAsync(
            session.Id,
            ownerId,
            new UpdateSessionRequest(
                session.IncarnationId,
                null,
                SessionScope.Room,
                null,
                roomId.Value),
            TestContext.Current.CancellationToken);

        var afterUpdate = await overrides.GetAsync(
            session.Id,
            charlieId,
            TestContext.Current.CancellationToken);
        Assert.NotNull(afterUpdate);
        Assert.True(afterUpdate!.IsActiveAt(DateTimeOffset.UtcNow));
    }

    [Fact]
    public async Task SessionUpdate_Leaves_Overrides_Alone_On_Scope_Widening()
    {
        var ownerId = UserId.New();
        var granteeId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.JustMe);
        var existingOverride = SessionAccessOverride.Create(
            session.Id,
            granteeId,
            AccessLevel.Suggest,
            ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new FakeAccessOverrideRepository(existingOverride);
        var service = CreateSessionUpdater(session, accessOverrideRepository: overrides);

        await service.UpdateOwnedAsync(
            session.Id,
            ownerId,
            new UpdateSessionRequest(
                session.IncarnationId,
                null,
                SessionScope.Friends,
                null,
                null),
            TestContext.Current.CancellationToken);

        var afterUpdate = await overrides.GetAsync(
            session.Id,
            granteeId,
            TestContext.Current.CancellationToken);
        Assert.NotNull(afterUpdate);
        Assert.True(afterUpdate!.IsActiveAt(DateTimeOffset.UtcNow));
    }

    [Fact]
    public async Task SessionUpdate_Revokes_Override_From_Old_Room_On_Room_To_Room_Move()
    {





        var ownerId = UserId.New();
        var danaId = UserId.New();
        var roomA = RoomId.From(Guid.NewGuid());
        var roomB = RoomId.From(Guid.NewGuid());
        var session = CreateSession(ownerId, SessionScope.Room, roomA);
        var existingOverride = SessionAccessOverride.Create(
            session.Id,
            danaId,
            AccessLevel.Inject,
            ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new FakeAccessOverrideRepository(existingOverride);
        var roomMembers = new FakeRoomMemberRepository(
            (roomA, ownerId),
            (roomA, danaId),
            (roomB, ownerId));
        var service = CreateSessionUpdater(
            session,
            roomMemberRepository: roomMembers,
            accessOverrideRepository: overrides);

        await service.UpdateOwnedAsync(
            session.Id,
            ownerId,
            new UpdateSessionRequest(
                session.IncarnationId,
                null,
                SessionScope.Room,
                null,
                roomB.Value),
            TestContext.Current.CancellationToken);

        var afterUpdate = await overrides.GetAsync(
            session.Id,
            danaId,
            TestContext.Current.CancellationToken);
        Assert.NotNull(afterUpdate);
        Assert.False(afterUpdate!.IsActiveAt(DateTimeOffset.UtcNow));
        Assert.NotNull(afterUpdate.RevokedAt);
    }

    [Fact]
    public async Task SessionUpdate_Keeps_Override_On_Room_Move_When_Grantee_Is_In_Both_Rooms()
    {
        var ownerId = UserId.New();
        var erinId = UserId.New();
        var roomA = RoomId.From(Guid.NewGuid());
        var roomB = RoomId.From(Guid.NewGuid());
        var session = CreateSession(ownerId, SessionScope.Room, roomA);
        var existingOverride = SessionAccessOverride.Create(
            session.Id,
            erinId,
            AccessLevel.Inject,
            ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new FakeAccessOverrideRepository(existingOverride);
        var roomMembers = new FakeRoomMemberRepository(
            (roomA, ownerId),
            (roomA, erinId),
            (roomB, ownerId),
            (roomB, erinId));
        var service = CreateSessionUpdater(
            session,
            roomMemberRepository: roomMembers,
            accessOverrideRepository: overrides);

        await service.UpdateOwnedAsync(
            session.Id,
            ownerId,
            new UpdateSessionRequest(
                session.IncarnationId,
                null,
                SessionScope.Room,
                null,
                roomB.Value),
            TestContext.Current.CancellationToken);

        var afterUpdate = await overrides.GetAsync(
            session.Id,
            erinId,
            TestContext.Current.CancellationToken);
        Assert.NotNull(afterUpdate);
        Assert.True(afterUpdate!.IsActiveAt(DateTimeOffset.UtcNow));
    }

    [Fact]
    public async Task SessionUpdate_Leaves_Overrides_Alone_When_Room_Is_Unchanged()
    {


        var ownerId = UserId.New();
        var frankId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = CreateSession(ownerId, SessionScope.Room, roomId);
        var existingOverride = SessionAccessOverride.Create(
            session.Id,
            frankId,
            AccessLevel.Inject,
            ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new FakeAccessOverrideRepository(existingOverride);
        var roomMembers = new FakeRoomMemberRepository(
            (roomId, ownerId),
            (roomId, frankId));
        var service = CreateSessionUpdater(
            session,
            roomMemberRepository: roomMembers,
            accessOverrideRepository: overrides);

        await service.UpdateOwnedAsync(
            session.Id,
            ownerId,
            new UpdateSessionRequest(
                session.IncarnationId,
                "renamed",
                SessionScope.Room,
                null,
                roomId.Value),
            TestContext.Current.CancellationToken);

        var afterUpdate = await overrides.GetAsync(
            session.Id,
            frankId,
            TestContext.Current.CancellationToken);
        Assert.NotNull(afterUpdate);
        Assert.True(afterUpdate!.IsActiveAt(DateTimeOffset.UtcNow));
    }

}
