using Kodosi.Application;
using Kodosi.Domain;
using static Kodosi.HostTests.SecurityAuthorizationTestFactory;

namespace Kodosi.HostTests;

public sealed class LiveSessionTerminatorTests
{
    [Fact]
    public async Task EndOrphanedSessionAsync_Rejects_Changed_HostRelease_Marker()
    {
        var session = CreateSession(UserId.New(), SessionScope.Friends);
        session.ActivateHost("host-1");
        session.ReleaseHostSlot("host-1");
        var observedReleasedAt = session.HostReleasedAt;
        Assert.NotNull(observedReleasedAt);
        var candidate = new OrphanedSessionCandidate(
            session.Id,
            session.StartedAt,
            observedReleasedAt,
            session.LastHeartbeatAt);
        do
        {
            await Task.Delay(1, TestContext.Current.CancellationToken);
            session.ActivateHost("host-2");
            session.ReleaseHostSlot("host-2");
        }
        while (session.HostReleasedAt == observedReleasedAt);
        var service = CreateTerminator(new FakeSessionRepository(session));

        var transition = await service.EndOrphanedSessionAsync(
            candidate,
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionTransitionOutcome.Rejected, transition.Outcome);
        Assert.NotEqual(SessionStatus.Ended, session.Status);
    }

    [Fact]
    public async Task EndOrphanedSessionAsync_Rejects_Changed_Heartbeat_Marker()
    {
        var session = CreateSession(UserId.New(), SessionScope.Friends);
        var observedHeartbeat = session.LastHeartbeatAt;
        var candidate = new OrphanedSessionCandidate(
            session.Id,
            session.StartedAt,
            ObservedHostReleasedAt: null,
            observedHeartbeat);
        do
        {
            await Task.Delay(1, TestContext.Current.CancellationToken);
            session.RecordHostHeartbeat();
        }
        while (session.LastHeartbeatAt == observedHeartbeat);
        var service = CreateTerminator(new FakeSessionRepository(session));

        var transition = await service.EndOrphanedSessionAsync(
            candidate,
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionTransitionOutcome.Rejected, transition.Outcome);
        Assert.NotEqual(SessionStatus.Ended, session.Status);
    }

    [Fact]
    public async Task IdempotentOwnerEnd_RetryRetainsFirstAttemptAndDoesNotReapply()
    {
        var session = CreateSession(UserId.New(), SessionScope.Friends);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var sessions = new FakeSessionRepository(session);
        var receipts = new FakeSessionEndMutationRepository();
        var terminator = new LiveSessionTerminator(
            sessions,
            new FakeSessionKeyBlobRepository(),
            new FakeUnitOfWork(),
            receipts);
        var mutationId = Guid.CreateVersion7();
        var firstAttemptId = Guid.CreateVersion7();

        var first = await terminator.EndOwnedSessionIdempotentlyAsync(
            session.Id,
            session.OwnerUserId,
            session.IncarnationId,
            mutationId,
            firstAttemptId,
            TestContext.Current.CancellationToken);
        var retry = await terminator.EndOwnedSessionIdempotentlyAsync(
            session.Id,
            session.OwnerUserId,
            session.IncarnationId,
            mutationId,
            Guid.CreateVersion7(),
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionTransitionOutcome.Applied, first.Outcome);
        Assert.Equal(SessionTransitionOutcome.AlreadyInTargetState, retry.Outcome);
        var receipt = Assert.Single(receipts.Mutations);
        Assert.Equal(firstAttemptId, receipt.FirstAttemptId);
        Assert.True(receipt.Matches(session.Id, session.IncarnationId));
    }

    [Fact]
    public async Task IdempotentOwnerEnd_RejectsMutationReuseForDifferentTarget()
    {
        var session = CreateSession(UserId.New(), SessionScope.Friends);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var receipts = new FakeSessionEndMutationRepository();
        var terminator = new LiveSessionTerminator(
            new FakeSessionRepository(session),
            new FakeSessionKeyBlobRepository(),
            new FakeUnitOfWork(),
            receipts);
        var mutationId = Guid.CreateVersion7();

        await terminator.EndOwnedSessionIdempotentlyAsync(
            session.Id,
            session.OwnerUserId,
            session.IncarnationId,
            mutationId,
            Guid.CreateVersion7(),
            TestContext.Current.CancellationToken);

        var error = await Assert.ThrowsAsync<SessionEndMutationTargetConflictException>(() =>
            terminator.EndOwnedSessionIdempotentlyAsync(
                session.Id,
                session.OwnerUserId,
                Guid.CreateVersion7(),
                mutationId,
                Guid.CreateVersion7(),
                TestContext.Current.CancellationToken));
        Assert.Equal("SESSION_END_MUTATION_TARGET_CONFLICT", error.Code);
    }

    [Fact]
    public async Task IdempotentOwnerEnd_OldReceiptDoesNotProjectRepublishedIncarnation()
    {
        var session = CreateSession(UserId.New(), SessionScope.Friends);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var oldIncarnationId = session.IncarnationId;
        var receipts = new FakeSessionEndMutationRepository();
        var terminator = new LiveSessionTerminator(
            new FakeSessionRepository(session),
            new FakeSessionKeyBlobRepository(),
            new FakeUnitOfWork(),
            receipts);
        var mutationId = Guid.CreateVersion7();

        await terminator.EndOwnedSessionIdempotentlyAsync(
            session.Id,
            session.OwnerUserId,
            oldIncarnationId,
            mutationId,
            Guid.CreateVersion7(),
            TestContext.Current.CancellationToken);
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            session.OwnerUserId,
            "republished",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "replacement-secret",
            roomId: null);

        var retry = await terminator.EndOwnedSessionIdempotentlyAsync(
            session.Id,
            session.OwnerUserId,
            oldIncarnationId,
            mutationId,
            Guid.CreateVersion7(),
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionTransitionOutcome.AlreadyInTargetState, retry.Outcome);
        Assert.Null(retry.SharingState);
        Assert.Equal(SessionStatus.Pending, session.Status);
        Assert.NotEqual(oldIncarnationId, session.IncarnationId);
    }

    [Fact]
    public async Task IdempotentOwnerEnd_FirstStaleTargetCommitsBoundNoOpReceipt()
    {
        var session = CreateSession(UserId.New(), SessionScope.Friends);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var currentIncarnationId = session.IncarnationId;
        var staleIncarnationId = Guid.CreateVersion7();
        var receipts = new FakeSessionEndMutationRepository();
        var terminator = new LiveSessionTerminator(
            new FakeSessionRepository(session),
            new FakeSessionKeyBlobRepository(),
            new FakeUnitOfWork(),
            receipts);
        var mutationId = Guid.CreateVersion7();
        var firstAttemptId = Guid.CreateVersion7();

        var result = await terminator.EndOwnedSessionIdempotentlyAsync(
            session.Id,
            session.OwnerUserId,
            staleIncarnationId,
            mutationId,
            firstAttemptId,
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionTransitionOutcome.AlreadyInTargetState, result.Outcome);
        Assert.Null(result.SharingState);
        Assert.Equal(SessionStatus.Live, session.Status);
        Assert.Equal(currentIncarnationId, session.IncarnationId);
        var receipt = Assert.Single(receipts.Mutations);
        Assert.True(receipt.Matches(session.Id, staleIncarnationId));
        Assert.Equal(firstAttemptId, receipt.FirstAttemptId);

        var conflict = await Assert.ThrowsAsync<SessionEndMutationTargetConflictException>(() =>
            terminator.EndOwnedSessionIdempotentlyAsync(
                session.Id,
                session.OwnerUserId,
                currentIncarnationId,
                mutationId,
                Guid.CreateVersion7(),
                TestContext.Current.CancellationToken));
        Assert.Equal("SESSION_END_MUTATION_TARGET_CONFLICT", conflict.Code);
    }

    [Fact]
    public void SessionEndMutation_RequiresUuidV7MutationAndAttemptIds()
    {
        var session = CreateSession(UserId.New(), SessionScope.Friends);

        Assert.Throws<DomainException>(() => SessionEndMutation.Create(
            session.OwnerUserId,
            Guid.NewGuid(),
            session.Id,
            session.IncarnationId,
            Guid.CreateVersion7(),
            DateTimeOffset.UtcNow));
        Assert.Throws<DomainException>(() => SessionEndMutation.Create(
            session.OwnerUserId,
            Guid.CreateVersion7(),
            session.Id,
            session.IncarnationId,
            Guid.NewGuid(),
            DateTimeOffset.UtcNow));
    }

    [Fact]
    public async Task EndOwnedSessionIdempotentlyAsync_Ends_Owned_Session()
    {
        var session = CreateSession(UserId.New(), SessionScope.Friends);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var repository = new FakeSessionRepository(session);
        var service = CreateTerminator(repository);

        var transition = await service.EndOwnedSessionIdempotentlyAsync(
            session.Id,
            session.OwnerUserId,
            session.IncarnationId,
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionTransitionOutcome.Applied, transition.Outcome);
        Assert.Equal(SessionStatus.Ended, session.Status);
        Assert.Equal(session.OwnerUserId, transition.SharingState?.OwnerUserId);
    }

    [Fact]
    public async Task EndOwnedSessionIdempotentlyAsync_Hides_Session_From_NonOwner()
    {
        var session = CreateSession(UserId.New(), SessionScope.Friends);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var repository = new FakeSessionRepository(session);
        var service = CreateTerminator(repository);

        await Assert.ThrowsAsync<NotFoundException>(() =>
            service.EndOwnedSessionIdempotentlyAsync(
                session.Id,
                UserId.New(),
                session.IncarnationId,
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
                TestContext.Current.CancellationToken));

        Assert.Equal(SessionStatus.Live, session.Status);
    }

    private static LiveSessionTerminator CreateTerminator(FakeSessionRepository repository)
        => new(
            repository,
            new FakeSessionKeyBlobRepository(),
            new FakeUnitOfWork(),
            new FakeSessionEndMutationRepository());
}
