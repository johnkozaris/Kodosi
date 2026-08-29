using System.Reflection;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class SessionKeyGenerationClaimerTests
{
    [Fact]
    public async Task ClaimNextAsync_Locks_Advances_And_Commits()
    {
        var session = CreateSession();
        var sessions = new ClaimSessionRepository(session);
        var unitOfWork = new ClaimUnitOfWork();
        var claimer = new SessionKeyGenerationClaimer(sessions, unitOfWork);

        var result = await claimer.ClaimNextAsync(
            session.Id,
            session.OwnerUserId,
            session.IncarnationId,
            session.CurrentKeyGeneration,
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionKeyGenerationClaimState.Claimed, result.State);
        Assert.Equal(1, result.Generation);
        Assert.Equal(1, session.CurrentKeyGeneration);
        Assert.Equal(1, sessions.LockCalls);
        Assert.Equal(1, sessions.UpdateCalls);
        Assert.Equal(1, unitOfWork.SaveCalls);
        Assert.Equal(1, unitOfWork.CommitCalls);
    }

    [Fact]
    public async Task ClaimNextAsync_Collapses_Missing_And_NonOwner_To_NotFound()
    {
        var session = CreateSession();
        var missing = await ClaimAsync(session: null, session.Id, session.OwnerUserId);
        var nonOwner = await ClaimAsync(session, session.Id, UserId.New());

        Assert.Equal(SessionKeyGenerationClaimState.NotFound, missing.Result.State);
        Assert.Equal(SessionKeyGenerationClaimState.NotFound, nonOwner.Result.State);
        Assert.Equal(0, session.CurrentKeyGeneration);
        Assert.Equal(0, missing.Sessions.UpdateCalls);
        Assert.Equal(0, nonOwner.Sessions.UpdateCalls);
        Assert.Equal(0, missing.UnitOfWork.SaveCalls);
        Assert.Equal(0, nonOwner.UnitOfWork.SaveCalls);
        Assert.Equal(0, missing.UnitOfWork.CommitCalls);
        Assert.Equal(0, nonOwner.UnitOfWork.CommitCalls);
    }

    [Fact]
    public async Task ClaimNextAsync_Rejects_Ended_Session_Without_Mutation()
    {
        var session = CreateSession();
        session.End();

        var attempt = await ClaimAsync(session, session.Id, session.OwnerUserId);

        Assert.Equal(SessionKeyGenerationClaimState.Ended, attempt.Result.State);
        Assert.Equal(0, session.CurrentKeyGeneration);
        Assert.Equal(0, attempt.Sessions.UpdateCalls);
        Assert.Equal(0, attempt.UnitOfWork.SaveCalls);
        Assert.Equal(0, attempt.UnitOfWork.CommitCalls);
    }

    [Fact]
    public async Task ClaimNextAsync_Rejects_Delayed_Old_Incarnation_After_Republish()
    {
        var session = CreateSession();
        var oldIncarnationId = session.IncarnationId;
        session.End();
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            session.OwnerUserId,
            "republished",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "new-secret",
            roomId: null);
        Assert.NotEqual(oldIncarnationId, session.IncarnationId);

        var attempt = await ClaimAsync(
            session,
            session.Id,
            session.OwnerUserId,
            expectedIncarnationId: oldIncarnationId);

        Assert.Equal(SessionKeyGenerationClaimState.StaleIncarnation, attempt.Result.State);
        Assert.Equal(1, session.CurrentKeyGeneration);
        Assert.Equal(0, attempt.Sessions.UpdateCalls);
        Assert.Equal(0, attempt.UnitOfWork.SaveCalls);
        Assert.Equal(0, attempt.UnitOfWork.CommitCalls);
    }

    [Fact]
    public async Task ClaimNextAsync_Rejects_Changed_Expected_Generation()
    {
        var session = CreateSession();
        SetCurrentKeyGeneration(session, 4);

        var attempt = await ClaimAsync(
            session,
            session.Id,
            session.OwnerUserId,
            expectedGeneration: 3);

        Assert.Equal(SessionKeyGenerationClaimState.GenerationChanged, attempt.Result.State);
        Assert.Equal(4, attempt.Result.Generation);
        Assert.Equal(4, session.CurrentKeyGeneration);
        Assert.Equal(0, attempt.Sessions.UpdateCalls);
        Assert.Equal(0, attempt.UnitOfWork.SaveCalls);
        Assert.Equal(0, attempt.UnitOfWork.CommitCalls);
    }

    [Fact]
    public async Task ClaimNextAsync_Rejects_Exhausted_Generation_Without_Mutation()
    {
        var session = CreateSession();
        SetCurrentKeyGeneration(session, int.MaxValue);

        var attempt = await ClaimAsync(session, session.Id, session.OwnerUserId);

        Assert.Equal(SessionKeyGenerationClaimState.Exhausted, attempt.Result.State);
        Assert.Equal(int.MaxValue, session.CurrentKeyGeneration);
        Assert.Equal(0, attempt.Sessions.UpdateCalls);
        Assert.Equal(0, attempt.UnitOfWork.SaveCalls);
        Assert.Equal(0, attempt.UnitOfWork.CommitCalls);
    }

    private static async Task<ClaimAttempt> ClaimAsync(
        Session? session,
        SessionId sessionId,
        UserId requestorId,
        Guid? expectedIncarnationId = null,
        int? expectedGeneration = null)
    {
        var sessions = new ClaimSessionRepository(session);
        var unitOfWork = new ClaimUnitOfWork();
        var result = await new SessionKeyGenerationClaimer(sessions, unitOfWork)
            .ClaimNextAsync(
                sessionId,
                requestorId,
                expectedIncarnationId ?? session?.IncarnationId ?? Guid.CreateVersion7(),
                expectedGeneration ?? session?.CurrentKeyGeneration ?? 0,
                TestContext.Current.CancellationToken);
        return new ClaimAttempt(result, sessions, unitOfWork);
    }

    private static Session CreateSession() =>
        Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");

    private static void SetCurrentKeyGeneration(Session session, int generation)
    {
        typeof(Session)
            .GetProperty(
                nameof(Session.CurrentKeyGeneration),
                BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)!
            .SetValue(session, generation);
    }

    private sealed class ClaimSessionRepository(Session? session) : SessionRepositoryStub
    {
        public int LockCalls { get; private set; }
        public int UpdateCalls { get; private set; }

        public override Task<Session?> GetByIdForUpdateAsync(
            SessionId id,
            CancellationToken ct = default)
        {
            LockCalls++;
            return Task.FromResult(session?.Id == id ? session : null);
        }

        public override Task UpdateAsync(
            Session updated,
            CancellationToken ct = default)
        {
            UpdateCalls++;
            return Task.CompletedTask;
        }
    }

    private sealed class ClaimUnitOfWork : UnitOfWorkStub
    {
        public int SaveCalls { get; private set; }
        public int CommitCalls { get; private set; }

        public override Task SaveChangesAsync(CancellationToken ct = default)
        {
            SaveCalls++;
            return Task.CompletedTask;
        }

        public override Task<ITransactionScope> BeginTransactionAsync(
            CancellationToken ct = default) =>
            Task.FromResult<ITransactionScope>(new ClaimTransaction(this));

        private sealed class ClaimTransaction(ClaimUnitOfWork owner)
            : TransactionScopeStub
        {
            public override Task CommitAsync(CancellationToken ct = default)
            {
                owner.CommitCalls++;
                return Task.CompletedTask;
            }

            public override ValueTask DisposeAsync() => ValueTask.CompletedTask;
        }
    }

    private sealed record ClaimAttempt(
        SessionKeyGenerationClaimResult Result,
        ClaimSessionRepository Sessions,
        ClaimUnitOfWork UnitOfWork);
}
