using Kodosi.Domain;

namespace Kodosi.Application;



public sealed class SessionAccessOverrideRevoker(
    ISessionRepository sessionRepository,
    IAccessOverrideRepository overrides,
    ISessionKeyBlobRepository keyBlobs)
{
    private readonly ISessionRepository _sessionRepository = sessionRepository;
    private readonly IAccessOverrideRepository _overrides = overrides;
    private readonly ISessionKeyBlobRepository _keyBlobs = keyBlobs;

    internal Task<IReadOnlyList<SessionId>> GetNonEndedRoomSessionIdsAsync(
        RoomId roomId,
        CancellationToken ct = default) =>
        _sessionRepository.GetAllNonEndedByRoomIdsAsync(roomId, ct);

    internal async Task<IReadOnlyList<RoomLiveSession>> RevokeRoomMemberOverridesAsync(
        RoomId roomId,
        UserId removedUserId,
        IReadOnlyCollection<SessionId> sessionIds,
        CancellationToken ct = default)
    {
        var roomSessions = await _sessionRepository.GetNonEndedByRoomIdsForUpdateAsync(
            roomId,
            sessionIds,
            ct);
        if (roomSessions.Count == 0)
        {
            return [];
        }

        var lockedSessionIds = roomSessions.Select(session => session.Id).ToList();
        await RevokeActiveOverridesForActorAsync(
            removedUserId,
            lockedSessionIds,
            ct);

        foreach (var session in roomSessions)
        {


            if (session.OwnerUserId == removedUserId)
            {
                session.End();
            }
            else
            {
                session.FenceKeyPublication();
            }

            await _sessionRepository.UpdateAsync(session, ct);
            await _keyBlobs.DeleteForSessionAsync(session.Id, ct);
        }

        return roomSessions
            .Select(session => new RoomLiveSession(
                session.Id,
                session.OwnerUserId,
                session.StartedAt,
                session.IncarnationId,
                session.OwnerUserId == removedUserId
                    ? new LiveSessionTransitionResult(
                        SessionTransitionOutcome.Applied,
                        session.ToDiscoveryTarget(),
                        session.StartedAt)
                    : null))
            .ToList();
    }

    internal async Task<IReadOnlyList<SessionAccessFanoutTarget>> RevokeFriendshipOverridesAsync(
        UserId ownerId,
        UserId formerFriendId,
        CancellationToken ct = default)
    {
        var sessions = await _sessionRepository.GetAllNonEndedFriendSessionsByOwnerAsync(ownerId, ct);
        if (sessions.Count == 0)
        {
            return [];
        }

        var sessionIds = sessions.Select(s => SessionId.From(s.Id)).ToList();
        await RevokeActiveOverridesForActorAsync(formerFriendId, sessionIds, ct);
        return sessions
            .Select(session => new SessionAccessFanoutTarget(
                SessionId.From(session.Id),
                session.StartedAt,
                session.IncarnationId))
            .ToList();
    }

    internal async Task<IReadOnlyList<SessionId>> GetNonEndedFriendSessionIdsByOwnerAsync(
        UserId ownerId,
        CancellationToken ct = default)
    {
        var sessions =
            await _sessionRepository.GetAllNonEndedFriendSessionsByOwnerAsync(
                ownerId,
                ct);
        return sessions
            .Select(session => SessionId.From(session.Id))
            .Distinct()
            .OrderBy(sessionId => sessionId.Value)
            .ToList();
    }

    internal async Task RevokeSessionOverridesAsync(
        SessionId sessionId,
        CancellationToken ct = default)
    {
        var unrevoked = await _overrides.GetUnrevokedBySessionAsync(sessionId, ct);
        if (unrevoked.Count == 0)
        {
            return;
        }
        foreach (var accessOverride in unrevoked)
        {
            accessOverride.Revoke();
        }
        await _overrides.UpdateRangeAsync(unrevoked, ct);
    }

    private async Task RevokeActiveOverridesForActorAsync(
        UserId actorUserId,
        IReadOnlyList<SessionId> sessionIds,
        CancellationToken ct)
    {
        var orphanedOverrides =
            await _overrides.GetActiveForActorAsync(actorUserId, sessionIds, ct);
        if (orphanedOverrides.Count == 0)
        {
            return;
        }

        foreach (var accessOverride in orphanedOverrides)
        {
            accessOverride.Revoke();
        }

        await _overrides.UpdateRangeAsync(orphanedOverrides, ct);
    }
}

public readonly record struct SessionAccessFanoutTarget(
    SessionId SessionId,
    DateTimeOffset StartedAt,
    Guid IncarnationId);

public readonly record struct RoomLiveSession(
    SessionId SessionId,
    UserId OwnerId,
    DateTimeOffset StartedAt,
    Guid IncarnationId = default,
    LiveSessionTransitionResult? EndTransition = null)
{
    public bool EndedByRemoval =>
        EndTransition?.Outcome == SessionTransitionOutcome.Applied;
}
