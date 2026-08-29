using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Endpoints;

using static Kodosi.HostTests.SecurityAuthorizationTestFactory;

namespace Kodosi.HostTests;

public sealed class SessionCreationReceiptLookupTests
{
    [Fact]
    public async Task GetOwnedAsync_Returns_Exact_Historical_Receipt_After_Republish()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var historicalKey = Guid.CreateVersion7();
        var historical = new SessionIncarnationRecord(
            session.Id,
            session.IncarnationGeneration,
            session.IncarnationId,
            session.IncarnationProtocolVersion,
            historicalKey,
            session.StartedAt);
        session.End();
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            ownerId,
            "Republished",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            "replacement-secret",
            roomId: null);
        var incarnations = new FakeSessionIncarnationRepository();
        await incarnations.AddAsync(historical, TestContext.Current.CancellationToken);
        var lookup = new SessionCreationReceiptLookup(
            new FakeSessionRepository(session),
            incarnations,
            new FakeUserLifecycleLock(),
            new FakeSessionEndAuthority(),
            new FakeUnitOfWork());

        var receipt = await lookup.GetOwnedAsync(
            session.Id,
            historicalKey,
            ownerId,
            TestContext.Current.CancellationToken);

        Assert.Equal(historical.SessionId.Value, receipt.SessionId);
        Assert.Equal(historical.IdempotencyKey, receipt.CreateIdempotencyKey);
        Assert.Equal(historical.IncarnationId, receipt.IncarnationId);
        Assert.Equal(historical.Generation, receipt.Generation);
        Assert.Equal(historical.ProtocolVersion, receipt.ProtocolVersion);
        Assert.NotEqual(session.IncarnationId, receipt.IncarnationId);
    }

    [Fact]
    public async Task GetOwnedAsync_Conceals_Receipt_From_NonOwner()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var key = Guid.CreateVersion7();
        var incarnations = new FakeSessionIncarnationRepository();
        await incarnations.AddAsync(
            ReceiptFor(session, key),
            TestContext.Current.CancellationToken);
        var lookup = new SessionCreationReceiptLookup(
            new FakeSessionRepository(session),
            incarnations,
            new FakeUserLifecycleLock(),
            new FakeSessionEndAuthority(),
            new FakeUnitOfWork());

        await Assert.ThrowsAsync<NotFoundException>(() => lookup.GetOwnedAsync(
            session.Id,
            key,
            UserId.New(),
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task GetOwnedAsync_Uses_NotFound_For_Missing_Key_And_Missing_Session()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var lookup = new SessionCreationReceiptLookup(
            new FakeSessionRepository(session),
            new FakeSessionIncarnationRepository(),
            new FakeUserLifecycleLock(),
            new FakeSessionEndAuthority(),
            new FakeUnitOfWork());

        await Assert.ThrowsAsync<NotFoundException>(() => lookup.GetOwnedAsync(
            session.Id,
            Guid.CreateVersion7(),
            ownerId,
            TestContext.Current.CancellationToken));
        await Assert.ThrowsAsync<NotFoundException>(() => lookup.GetOwnedAsync(
            SessionId.New(),
            Guid.CreateVersion7(),
            ownerId,
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task GetOwnedAsync_Acquires_User_Then_Session_Lifecycle_Authority()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var key = Guid.CreateVersion7();
        var incarnations = new FakeSessionIncarnationRepository();
        await incarnations.AddAsync(
            ReceiptFor(session, key),
            TestContext.Current.CancellationToken);
        var order = new List<string>();
        var userLock = new OrderedUserLifecycleLock(
            new FakeUserLifecycleLock(),
            order);
        var sessionAuthority = new OrderedSessionEndAuthority(
            new FakeSessionEndAuthority(),
            order);
        var lookup = new SessionCreationReceiptLookup(
            new FakeSessionRepository(session),
            incarnations,
            userLock,
            sessionAuthority,
            new FakeUnitOfWork());

        _ = await lookup.GetOwnedAsync(
            session.Id,
            key,
            ownerId,
            TestContext.Current.CancellationToken);

        Assert.Equal(["user", "session"], order);
        Assert.Equal([ownerId], userLock.Inner.AcquiredUserIds);
        Assert.Equal([session.Id], sessionAuthority.Inner.AcquiredSessionIds);
        Assert.False(sessionAuthority.Inner.LeaseHeld);
    }

    [Theory]
    [InlineData("not-a-uuid")]
    [InlineData("33333333-3333-4333-8333-333333333333")]
    public void ParseUuidV7_Rejects_Malformed_And_NonV7_Values(string raw)
    {
        var error = Assert.Throws<InvalidParameterException>(() =>
            SessionEndpoints.ParseUuidV7(raw, "createIdempotencyKey"));

        Assert.Equal("INVALID_PARAMETER", error.Code);
    }

    private sealed class OrderedUserLifecycleLock(
        FakeUserLifecycleLock inner,
        List<string> order) : IUserLifecycleLock
    {
        public FakeUserLifecycleLock Inner { get; } = inner;

        public Task AcquireAsync(UserId userId, CancellationToken ct = default)
        {
            order.Add("user");
            return Inner.AcquireAsync(userId, ct);
        }
    }

    private sealed class OrderedSessionEndAuthority(
        FakeSessionEndAuthority inner,
        List<string> order) : ISessionEndAuthority
    {
        public FakeSessionEndAuthority Inner { get; } = inner;

        public ValueTask<IAsyncDisposable> AcquireAsync(
            IReadOnlyCollection<SessionId> sessionIds,
            CancellationToken ct = default)
        {
            order.Add("session");
            return Inner.AcquireAsync(sessionIds, ct);
        }

        public Task ProjectCommittedAsync(
            IReadOnlyCollection<CommittedSessionEnd> sessionEnds,
            CancellationToken ct = default) =>
            Inner.ProjectCommittedAsync(sessionEnds, ct);

        public Task RetireEndedIncarnationAsync(
            SessionDiscoveryTarget endedSession,
            DateTimeOffset startedAt,
            CancellationToken ct = default) =>
            Inner.RetireEndedIncarnationAsync(endedSession, startedAt, ct);
    }

    private static SessionIncarnationRecord ReceiptFor(Session session, Guid key) =>
        new(
            session.Id,
            session.IncarnationGeneration,
            session.IncarnationId,
            session.IncarnationProtocolVersion,
            key,
            session.StartedAt);
}
