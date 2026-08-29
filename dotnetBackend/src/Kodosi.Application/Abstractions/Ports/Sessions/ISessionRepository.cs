using Kodosi.Domain;

namespace Kodosi.Application;

public interface ISessionRepository
{
    Task<Session?> GetByIdAsync(SessionId id, CancellationToken ct = default);
    Task<Session?> GetByIdForUpdateAsync(
        SessionId id,
        CancellationToken ct = default);
    Task<IReadOnlyList<Session>> GetByIdsForUpdateAsync(
        IReadOnlyCollection<SessionId> ids,
        CancellationToken ct = default);
    Task AddAsync(Session session, CancellationToken ct = default);
    Task UpdateAsync(Session session, CancellationToken ct = default);

    Task<IReadOnlyList<SessionCardProjection>> GetByOwnerProjectedAsync(
        UserId ownerUserId,
        CancellationToken ct = default);

    Task<IReadOnlyList<SessionCardProjection>> GetLiveByRoomProjectedAsync(
        RoomId roomId,
        FeedCursor? cursor = null,
        int limit = 20,
        ToolKind? toolKindFilter = null,
        DateTimeOffset? since = null,
        CancellationToken ct = default);

    Task<IReadOnlyList<SessionId>> GetAllNonEndedByRoomIdsAsync(
        RoomId roomId,
        CancellationToken ct = default);

    Task<IReadOnlyList<Session>> GetNonEndedByRoomIdsForUpdateAsync(
        RoomId roomId,
        IReadOnlyCollection<SessionId> sessionIds,
        CancellationToken ct = default);

    Task<IReadOnlyList<SessionCardProjection>> GetAllNonEndedFriendSessionsByOwnerAsync(
        UserId ownerUserId,
        CancellationToken ct = default);

    Task<IReadOnlyList<SessionId>> GetActiveOwnedSessionIdsAsync(
        UserId ownerUserId,
        CancellationToken ct = default);

    Task ReleaseHostSlotAsync(
    SessionId sessionId,
    string? connectionId,
    CancellationToken ct = default);

    Task<bool> TryIncrementParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        int maxParticipants,
        CancellationToken ct = default);

    Task<bool> TryDecrementParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        CancellationToken ct = default);

    Task ClearParticipantCountAsync(
        SessionId sessionId,
        CancellationToken ct = default);

    Task<bool> TryClearParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        CancellationToken ct = default);

    Task<bool> TrySetParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        int participantCount,
        CancellationToken ct = default);

    Task ResetRealtimeCountersAsync(CancellationToken ct = default);

    Task<IReadOnlyList<OrphanedSessionCandidate>> GetOrphanedLiveSessionsAsync(
        DateTimeOffset noHostSince,
        CancellationToken ct = default);
}

public sealed record OrphanedSessionCandidate(
    SessionId SessionId,
    DateTimeOffset StartedAt,
    DateTimeOffset? ObservedHostReleasedAt,
    DateTimeOffset ObservedLastHeartbeatAt);
