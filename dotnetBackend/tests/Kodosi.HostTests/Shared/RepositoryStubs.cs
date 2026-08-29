using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

internal abstract class SessionRepositoryStub : ISessionRepository
{
    protected static NotSupportedException Unconfigured() =>
        new("This test repository operation was not configured.");

    public virtual Task<Session?> GetByIdAsync(
        SessionId id,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task<Session?> GetByIdForUpdateAsync(
        SessionId id,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task<IReadOnlyList<Session>> GetByIdsForUpdateAsync(
        IReadOnlyCollection<SessionId> ids,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task AddAsync(Session session, CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task UpdateAsync(Session session, CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task<IReadOnlyList<SessionCardProjection>> GetByOwnerProjectedAsync(
        UserId ownerUserId,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task<IReadOnlyList<SessionCardProjection>> GetLiveByRoomProjectedAsync(
        RoomId roomId,
        FeedCursor? cursor = null,
        int limit = 20,
        ToolKind? toolKindFilter = null,
        DateTimeOffset? since = null,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task<IReadOnlyList<SessionId>> GetAllNonEndedByRoomIdsAsync(
        RoomId roomId,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task<IReadOnlyList<Session>> GetNonEndedByRoomIdsForUpdateAsync(
        RoomId roomId,
        IReadOnlyCollection<SessionId> sessionIds,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task<IReadOnlyList<SessionCardProjection>>
        GetAllNonEndedFriendSessionsByOwnerAsync(
            UserId ownerUserId,
            CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task<IReadOnlyList<SessionId>> GetActiveOwnedSessionIdsAsync(
        UserId ownerUserId,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task ReleaseHostSlotAsync(
        SessionId sessionId,
        string? connectionId,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task<bool> TryIncrementParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        int maxParticipants,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task<bool> TryDecrementParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task ClearParticipantCountAsync(
        SessionId sessionId,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task<bool> TryClearParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task<bool> TrySetParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        int participantCount,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task ResetRealtimeCountersAsync(CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task<IReadOnlyList<OrphanedSessionCandidate>>
        GetOrphanedLiveSessionsAsync(
        DateTimeOffset noHostSince,
        CancellationToken ct = default) =>
        throw Unconfigured();
}

internal abstract class RoomMutationReceiptRepositoryStub : IRoomMutationReceiptRepository
{
    protected static NotSupportedException Unconfigured() =>
        new("This test room-mutation receipt operation was not configured.");

    public virtual Task AcquireAsync(
        UserId actorUserId,
        RoomMutationOperation operation,
        Guid requestId,
        CancellationToken ct = default) => throw Unconfigured();

    public virtual Task<RoomMutationReceipt?> GetAsync(
        UserId actorUserId,
        RoomMutationOperation operation,
        Guid requestId,
        CancellationToken ct = default) => throw Unconfigured();

    public virtual Task<RoomMutationReceipt?> FindAsync(
        UserId actorUserId,
        RoomMutationOperation operation,
        Guid requestId,
        CancellationToken ct = default) => throw Unconfigured();

    public virtual Task AddAsync(
        RoomMutationReceipt receipt,
        CancellationToken ct = default) => throw Unconfigured();

    public virtual Task AddSessionEffectsAsync(
        IReadOnlyCollection<RoomMutationSessionEffect> effects,
        CancellationToken ct = default) => throw Unconfigured();

    public virtual Task<IReadOnlyList<RoomMutationSessionEffect>> GetSessionEffectsAsync(
        UserId actorUserId,
        Guid requestId,
        CancellationToken ct = default) => throw Unconfigured();
}

internal sealed class UnconfiguredRoomMutationReceiptRepository
    : RoomMutationReceiptRepositoryStub;

internal abstract class UnitOfWorkStub : IUnitOfWork
{
    protected static NotSupportedException Unconfigured() =>
        new("This test unit-of-work operation was not configured.");

    public virtual Task SaveChangesAsync(CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task<ITransactionScope> BeginTransactionAsync(
        CancellationToken ct = default) =>
        throw Unconfigured();
}

internal abstract class TransactionScopeStub : ITransactionScope
{
    protected static NotSupportedException Unconfigured() =>
        new("This test transaction operation was not configured.");

    public virtual Task CommitAsync(CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task CreateSavepointAsync(
        string name,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual Task RollbackToSavepointAsync(
        string name,
        CancellationToken ct = default) =>
        throw Unconfigured();

    public virtual ValueTask DisposeAsync() =>
        throw Unconfigured();
}

internal sealed class CompletedTransactionScope : TransactionScopeStub
{
    public override Task CommitAsync(CancellationToken ct = default) =>
        Task.CompletedTask;

    public override ValueTask DisposeAsync() => ValueTask.CompletedTask;
}

internal sealed class CompletedUnitOfWork : UnitOfWorkStub
{
    public override Task SaveChangesAsync(CancellationToken ct = default) =>
        Task.CompletedTask;

    public override Task<ITransactionScope> BeginTransactionAsync(
        CancellationToken ct = default) =>
        Task.FromResult<ITransactionScope>(new CompletedTransactionScope());
}

internal sealed class RecordingRoomRosterTransitionRepository
    : IRoomRosterTransitionRepository
{
    public List<RoomRosterTransition> Added { get; } = [];

    public Task AddAsync(
        RoomRosterTransition transition,
        CancellationToken ct = default)
    {
        Added.Add(transition);
        return Task.CompletedTask;
    }

    public Task<IReadOnlyList<RoomRosterTransition>> GetPageAfterAsync(
        RoomId roomId,
        long afterGeneration,
        int limit,
        CancellationToken ct = default) =>
        Task.FromResult<IReadOnlyList<RoomRosterTransition>>(
            Added.Where(transition =>
                    transition.RoomId == roomId
                    && transition.Generation > afterGeneration)
                .OrderBy(transition => transition.Generation)
                .Take(limit)
                .ToList());
}
