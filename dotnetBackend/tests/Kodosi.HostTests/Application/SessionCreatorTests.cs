using Kodosi.Application;
using Kodosi.Domain;

using static Kodosi.HostTests.SecurityAuthorizationTestFactory;

namespace Kodosi.HostTests;

public sealed class SessionCreatorTests
{
    [Fact]
    public async Task SessionCreate_Rejects_RoomScope_For_NonMember()
    {
        var ownerId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var service = CreateSessionCreator(roomMemberRepository: new FakeRoomMemberRepository());

        await Assert.ThrowsAsync<PolicyViolationException>(() => service.CreateOwnedAsync(
            ownerId,
            new CreateSessionRequest(
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
                "Room Session",
                SessionScope.Room,
                ToolKind.Terminal,
                DefaultAudienceAccess.Suggest,
                "owner-secret",
                roomId.Value),
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task SessionCreate_Allows_RoomScope_For_Member()
    {
        var ownerId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var requestedSessionId = Guid.CreateVersion7();
        var service = CreateSessionCreator(
            roomMemberRepository: new FakeRoomMemberRepository((roomId, ownerId)));

        var result = await service.CreateOwnedAsync(
            ownerId,
            new CreateSessionRequest(
                requestedSessionId,
                Guid.CreateVersion7(),
                "Room Session",
                SessionScope.Room,
                ToolKind.Terminal,
                DefaultAudienceAccess.Suggest,
                "owner-secret",
                roomId.Value),
            TestContext.Current.CancellationToken);

        Assert.Equal(requestedSessionId, result.Response.Id);
        Assert.Equal(SessionScope.Room, result.Response.Scope);
        Assert.Equal(roomId.Value, result.Response.RoomId);
    }

    [Fact]
    public async Task SessionCreate_Retry_With_Same_Idempotency_Key_Returns_Server_Incarnation()
    {
        var ownerId = UserId.New();
        var requestedSessionId = Guid.CreateVersion7();
        var idempotencyKey = Guid.CreateVersion7();
        var service = CreateSessionCreator();
        var request = new CreateSessionRequest(
            requestedSessionId,
            idempotencyKey,
            "Idempotent Session",
            SessionScope.MyDevices,
            ToolKind.Terminal,
            DefaultAudienceAccess.Suggest,
            "owner-secret",
            null);

        var first = await service.CreateOwnedAsync(
            ownerId,
            request,
            TestContext.Current.CancellationToken);
        var retry = await service.CreateOwnedAsync(
            ownerId,
            request,
            TestContext.Current.CancellationToken);

        Assert.NotEqual(idempotencyKey, first.Response.IncarnationId);
        Assert.Equal(first.Response, retry.Response);
    }

    [Fact]
    public async Task SessionCreate_NonRoom_Retry_Ignores_Supplied_RoomId()
    {
        var ownerId = UserId.New();
        var request = new CreateSessionRequest(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            "Non-room idempotent session",
            SessionScope.MyDevices,
            ToolKind.Terminal,
            DefaultAudienceAccess.Suggest,
            "owner-secret",
            Guid.CreateVersion7());
        var service = CreateSessionCreator();

        var first = await service.CreateOwnedAsync(
            ownerId,
            request,
            TestContext.Current.CancellationToken);
        var retry = await service.CreateOwnedAsync(
            ownerId,
            request,
            TestContext.Current.CancellationToken);

        Assert.Null(first.Response.RoomId);
        Assert.Equal(first.Response, retry.Response);
    }

    [Fact]
    public async Task SessionCreate_Rejects_Existing_RuntimeSessionId()
    {
        var ownerId = UserId.New();
        var existing = CreateSession(ownerId, SessionScope.Friends);
        var service = CreateSessionCreator(existing);

        await Assert.ThrowsAsync<ConflictException>(() => service.CreateOwnedAsync(
            ownerId,
            new CreateSessionRequest(
                existing.Id.Value,
                Guid.CreateVersion7(),
                "Duplicate",
                SessionScope.Friends,
                ToolKind.Terminal,
                DefaultAudienceAccess.Suggest,
                "owner-secret",
                null),
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task SessionCreate_Republishes_Ended_RuntimeSessionId()
    {
        var ownerId = UserId.New();
        var existing = CreateSession(ownerId, SessionScope.Friends);
        existing.End();
        var endedStartedAt = existing.StartedAt;
        var endedIncarnationId = existing.IncarnationId;
        var authority = new FakeSessionEndAuthority();
        var service = CreateSessionCreator(
            existing,
            sessionEndAuthority: authority);

        var result = await service.CreateOwnedAsync(
            ownerId,
            new CreateSessionRequest(
                existing.Id.Value,
                Guid.CreateVersion7(),
                "Republished",
                SessionScope.Friends,
                ToolKind.Terminal,
                DefaultAudienceAccess.Suggest,
                "new-owner-secret",
                null),
            TestContext.Current.CancellationToken);

        Assert.Equal(existing.Id.Value, result.Response.Id);
        Assert.Equal("Republished", result.Response.Title);
        Assert.Equal(SessionStatus.Pending, result.Response.Status);
        Assert.Null(existing.EndedAt);
        Assert.NotEqual(endedIncarnationId, result.Response.IncarnationId);
        Assert.Equal(existing.IncarnationId, result.Response.IncarnationId);
        Assert.Equal(2, result.Response.IncarnationGeneration);
        Assert.Equal(1, existing.CurrentKeyGeneration);
        var retired = Assert.Single(authority.RetiredIncarnations);
        Assert.Equal(existing.Id, retired.Session.SessionId);
        Assert.Equal(endedStartedAt, retired.StartedAt);
    }

    [Fact]
    public async Task SessionCreate_Republish_Revokes_Expired_Unrevoked_Override()
    {
        var now = DateTimeOffset.UtcNow;
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var existing = CreateSession(ownerId, SessionScope.Friends);
        var accessOverride = SessionAccessOverride.Create(
            existing.Id,
            viewerId,
            AccessLevel.Inject,
            ownerId,
            now.AddMinutes(-1),
            now.AddMinutes(-2));
        var overrides = new FakeAccessOverrideRepository(accessOverride);
        existing.End();
        var service = CreateSessionCreator(
            existing,
            accessOverrideRepository: overrides);

        await service.CreateOwnedAsync(
            ownerId,
            new CreateSessionRequest(
                existing.Id.Value,
                Guid.CreateVersion7(),
                "Republished",
                SessionScope.Friends,
                ToolKind.Terminal,
                DefaultAudienceAccess.Suggest,
                "new-owner-secret",
                null),
            TestContext.Current.CancellationToken);

        Assert.NotNull(accessOverride.RevokedAt);
    }

    [Fact]
    public async Task SessionCreate_Republish_Retry_Is_Idempotent()
    {
        var ownerId = UserId.New();
        var existing = CreateSession(ownerId, SessionScope.Friends);
        existing.End();
        var authority = new FakeSessionEndAuthority();
        var service = CreateSessionCreator(
            existing,
            sessionEndAuthority: authority);
        var request = new CreateSessionRequest(
            existing.Id.Value,
            Guid.CreateVersion7(),
            "Republished",
            SessionScope.Friends,
            ToolKind.Terminal,
            DefaultAudienceAccess.Suggest,
            "new-owner-secret",
            null);

        var first = await service.CreateOwnedAsync(
            ownerId,
            request,
            TestContext.Current.CancellationToken);
        var retry = await service.CreateOwnedAsync(
            ownerId,
            request,
            TestContext.Current.CancellationToken);

        Assert.Equal(first.Response, retry.Response);
        Assert.Single(authority.RetiredIncarnations);
    }

    [Fact]
    public async Task SessionCreate_Rejects_Reusing_Legacy_Incarnation_As_Idempotency_Key()
    {
        var ownerId = UserId.New();
        var existing = CreateSession(ownerId, SessionScope.Friends);
        existing.End();
        var authority = new FakeSessionEndAuthority();
        var service = CreateSessionCreator(
            existing,
            sessionEndAuthority: authority);

        await Assert.ThrowsAsync<ConflictException>(() =>
            service.CreateOwnedAsync(
                ownerId,
                new CreateSessionRequest(
                    existing.Id.Value,
                    existing.IncarnationId,
                    "Reused incarnation",
                    SessionScope.Friends,
                    ToolKind.Terminal,
                    DefaultAudienceAccess.Suggest,
                    "new-owner-secret",
                    null),
                TestContext.Current.CancellationToken));

        Assert.Equal(SessionStatus.Ended, existing.Status);
        Assert.Empty(authority.RetiredIncarnations);
    }

    [Fact]
    public async Task SessionCreate_Republish_Clears_Prior_Incarnation_Dismissals()
    {
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var existing = CreateSession(ownerId, SessionScope.Friends);
        var dismissals = new FakeSessionViewerDismissalRepository(
            SessionViewerDismissal.Create(
                existing.Id,
                viewerId));
        existing.End();
        var service = CreateSessionCreator(
            existing,
            dismissalRepository: dismissals);

        await service.CreateOwnedAsync(
            ownerId,
            new CreateSessionRequest(
                existing.Id.Value,
                Guid.CreateVersion7(),
                "Republished",
                SessionScope.Friends,
                ToolKind.Terminal,
                DefaultAudienceAccess.Suggest,
                "new-owner-secret",
                null),
            TestContext.Current.CancellationToken);

        Assert.Equal(0, dismissals.Count);
    }

    [Theory]
    [InlineData(SessionScope.JustMe)]
    [InlineData(SessionScope.MyDevices)]
    [InlineData(SessionScope.Friends)]
    public async Task SessionCreate_Rejects_Inject_DefaultAccess_For_Every_Scope(SessionScope scope)
    {
        var ownerId = UserId.New();
        var service = CreateSessionCreator();

        await Assert.ThrowsAsync<DomainException>(() => service.CreateOwnedAsync(
            ownerId,
            new CreateSessionRequest(
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
                "Session",
                scope,
                ToolKind.Terminal,
                (DefaultAudienceAccess)2,
                "owner-secret",
                null),
            TestContext.Current.CancellationToken));
    }

    [Theory]
    [InlineData(SessionScope.JustMe)]
    [InlineData(SessionScope.MyDevices)]
    [InlineData(SessionScope.Friends)]
    public async Task SessionCreate_Accepts_Suggest_DefaultAccess_For_Every_Scope(SessionScope scope)
    {
        var ownerId = UserId.New();
        var requestedSessionId = Guid.CreateVersion7();
        var service = CreateSessionCreator();

        var result = await service.CreateOwnedAsync(
            ownerId,
            new CreateSessionRequest(
                requestedSessionId,
                Guid.CreateVersion7(),
                "Session",
                scope,
                ToolKind.Terminal,
                DefaultAudienceAccess.Suggest,
                "owner-secret",
                null),
            TestContext.Current.CancellationToken);

        Assert.Equal(requestedSessionId, result.Response.Id);
        Assert.Equal(scope, result.Response.Scope);
        Assert.Equal(AccessLevel.Suggest, result.Response.DefaultAccess);

        Assert.Equal(AccessLevel.Inject, result.Response.EffectiveAccess);
    }
}
