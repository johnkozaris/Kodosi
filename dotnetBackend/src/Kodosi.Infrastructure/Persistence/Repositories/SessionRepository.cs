using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class SessionRepository(KodosiDbContext context) : ISessionRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task<Session?> GetByIdAsync(SessionId id, CancellationToken ct = default)


        => await _context.Sessions.AsNoTracking().FirstOrDefaultAsync(s => s.Id == id, ct);

    public async Task<Session?> GetByIdForUpdateAsync(
        SessionId id,
        CancellationToken ct = default)
    {
        if (_context.Database.CurrentTransaction is null)
        {
            throw new InvalidOperationException(
                "A session row lock requires an active database transaction.");
        }

        _ = await _context.Database.ExecuteSqlInterpolatedAsync(
            $"SELECT 1 FROM sessions WHERE id = {id.Value} FOR UPDATE",
            ct);
        return await GetByIdAsync(id, ct);
    }

    public async Task<IReadOnlyList<Session>> GetByIdsForUpdateAsync(
        IReadOnlyCollection<SessionId> ids,
        CancellationToken ct = default)
    {
        if (_context.Database.CurrentTransaction is null)
        {
            throw new InvalidOperationException(
                "Session row locks require an active database transaction.");
        }
        if (ids.Count == 0)
        {
            return [];
        }

        var sessionIds = ids
            .Select(sessionId => sessionId.Value)
            .Distinct()
            .Order()
            .ToArray();
        return await _context.Sessions
            .FromSqlInterpolated($"""
                SELECT *, xmin FROM sessions
                WHERE id = ANY ({sessionIds})
                ORDER BY id
                FOR UPDATE
                """)
            .ToListAsync(ct);
    }

    public async Task AddAsync(Session session, CancellationToken ct = default)
        => await _context.Sessions.AddAsync(session, ct);

    public Task UpdateAsync(Session session, CancellationToken ct = default)
    {
        _context.Sessions.Update(session);
        return Task.CompletedTask;
    }

    private static readonly SessionStatus[] LiveStatuses =
        [SessionStatus.Live, SessionStatus.Reconnecting];


    private const int MySessionsMaxResults = 500;

    public async Task<IReadOnlyList<SessionCardProjection>> GetByOwnerProjectedAsync(
        UserId ownerUserId, CancellationToken ct = default)
    {
        return await _context.Sessions
            .Where(s => s.OwnerUserId == ownerUserId)
            .Join(_context.Users, s => s.OwnerUserId, u => u.Id, (s, u) => new
            {
                s.Id,
                s.Title,
                s.Scope,
                s.DefaultAccess,
                s.Status,
                s.OwnerUserId,
                u.DisplayName,
                u.AvatarUrl,
                s.StartedAt,
                s.IncarnationId,
                s.ToolKind,
                s.RoomId
            })
            .OrderByDescending(x => x.StartedAt)
            .ThenByDescending(x => x.Id)
            .Take(MySessionsMaxResults)
            .Select(x => new SessionCardProjection(
                x.Id.Value, x.Title, x.Scope, x.DefaultAccess, x.Status,
                x.OwnerUserId.Value, x.DisplayName, x.AvatarUrl,
                x.StartedAt, x.IncarnationId, x.ToolKind,
                x.RoomId != null ? x.RoomId.Value.Value : (Guid?)null))
            .ToListAsync(ct);
    }

    public Task<IReadOnlyList<SessionCardProjection>> GetLiveByRoomProjectedAsync(
        RoomId roomId,
        FeedCursor? cursor = null,
        int limit = 20,
        ToolKind? toolKindFilter = null,
        DateTimeOffset? since = null,
        CancellationToken ct = default)
    {
        IQueryable<Session> query;
        if (cursor is null)
        {
            query = _context.Sessions
                .Where(s => s.RoomId == roomId
                    && s.Scope == SessionScope.Room
                    && LiveStatuses.Contains(s.Status));
        }
        else
        {
            query = _context.Sessions.FromSqlInterpolated($"""
                SELECT *
                FROM sessions
                WHERE room_id = {roomId.Value}
                  AND scope = {SessionScope.Room.ToString()}
                  AND status IN ({SessionStatus.Live.ToString()}, {SessionStatus.Reconnecting.ToString()})
                  AND (
                      started_at < {cursor.StartedAt}
                      OR (started_at = {cursor.StartedAt} AND id < {cursor.Id})
                  )
                """);
        }

        return ProjectFeedAsync(query, limit, toolKindFilter, since, ct);
    }

    public async Task<IReadOnlyList<SessionId>> GetAllNonEndedByRoomIdsAsync(
        RoomId roomId,
        CancellationToken ct = default)
    {
        return await _context.Sessions
            .Where(session =>
                session.RoomId == roomId
                && session.Scope == SessionScope.Room
                && session.Status != SessionStatus.Ended)
            .OrderBy(session => session.Id)
            .Select(session => session.Id)
            .ToListAsync(ct);
    }

    public async Task<IReadOnlyList<Session>> GetNonEndedByRoomIdsForUpdateAsync(
        RoomId roomId,
        IReadOnlyCollection<SessionId> sessionIds,
        CancellationToken ct = default)
    {
        if (_context.Database.CurrentTransaction is null)
        {
            throw new InvalidOperationException(
                "Room session row locks require an active database transaction.");
        }
        if (sessionIds.Count == 0)
        {
            return [];
        }

        var ids = sessionIds.Select(static sessionId => sessionId.Value).ToArray();
        return await _context.Sessions
            .FromSqlInterpolated($"""
                SELECT *, xmin FROM sessions
                WHERE room_id = {roomId.Value}
                  AND id = ANY ({ids})
                  AND scope = {SessionScope.Room.ToString()}
                  AND status <> {SessionStatus.Ended.ToString()}
                ORDER BY id
                FOR UPDATE
                """)
            .ToListAsync(ct);
    }

    public Task<IReadOnlyList<SessionCardProjection>> GetAllNonEndedFriendSessionsByOwnerAsync(
        UserId ownerUserId,
        CancellationToken ct = default) =>
        ProjectAllNonEndedAsync(
            _context.Sessions.Where(session =>
                session.OwnerUserId == ownerUserId
                && session.Scope == SessionScope.Friends
                && session.Status != SessionStatus.Ended),
            ct);

    public async Task<IReadOnlyList<SessionId>> GetActiveOwnedSessionIdsAsync(
        UserId ownerUserId,
        CancellationToken ct = default)
    {
        return await _context.Sessions
            .Where(s => s.OwnerUserId == ownerUserId
                && s.Status != SessionStatus.Ended)
            .OrderBy(s => s.Id)
            .Select(s => s.Id)
            .ToListAsync(ct);
    }

    public Task ReleaseHostSlotAsync(
        SessionId sessionId,
        string? connectionId,
        CancellationToken ct = default)
    {
        var now = DateTimeOffset.UtcNow;
        var query = _context.Sessions
            .Where(s => s.Id == sessionId && s.HostConnectionSlot != null);
        if (connectionId is not null)
        {
            query = query.Where(s => s.HostConnectionSlot == connectionId);
        }

        return query.ExecuteUpdateAsync(
            setters => setters
                .SetProperty(s => s.HostConnectionSlot, (string?)null)
                .SetProperty(s => s.HostClaimedAt, (DateTimeOffset?)null)
                .SetProperty(s => s.HostReleasedAt, (DateTimeOffset?)now),
            ct);
    }

    public async Task<bool> TryIncrementParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        int maxParticipants,
        CancellationToken ct = default)
    {
        var updated = await _context.Sessions
            .Where(session => session.Id == sessionId
                && session.StartedAt == expectedStartedAt
                && session.Status != SessionStatus.Ended
                && session.LiveParticipantCount < maxParticipants)
            .ExecuteUpdateAsync(
                setters => setters.SetProperty(
                    session => session.LiveParticipantCount,
                    session => session.LiveParticipantCount + 1),
                ct);
        return updated > 0;
    }

    public async Task<bool> TryDecrementParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        CancellationToken ct = default)
    {
        var updated = await _context.Sessions
                .Where(session => session.Id == sessionId
                    && session.StartedAt == expectedStartedAt
                    && session.LiveParticipantCount > 0)
                .ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        session => session.LiveParticipantCount,
                        session => session.LiveParticipantCount - 1),
                    ct);
        return updated > 0;
    }

    public Task ClearParticipantCountAsync(
        SessionId sessionId,
        CancellationToken ct = default)
    {
        return _context.Sessions
            .Where(s => s.Id == sessionId && s.LiveParticipantCount > 0)
            .ExecuteUpdateAsync(
                setters => setters.SetProperty(s => s.LiveParticipantCount, 0),
                ct);
    }

    public async Task<bool> TryClearParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        CancellationToken ct = default)
    {
        var updated = await _context.Sessions
                .Where(session => session.Id == sessionId
                    && session.StartedAt == expectedStartedAt
                    && session.LiveParticipantCount > 0)
                .ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        session => session.LiveParticipantCount,
                        0),
                    ct);
        return updated > 0;
    }

    public async Task<bool> TrySetParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        int participantCount,
        CancellationToken ct = default)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(participantCount);
        var updated = await _context.Sessions
                .Where(session => session.Id == sessionId
                    && session.StartedAt == expectedStartedAt)
                .ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        session => session.LiveParticipantCount,
                        participantCount),
                    ct);
        return updated > 0;
    }

    public async Task ResetRealtimeCountersAsync(CancellationToken ct = default)
    {

        var now = DateTimeOffset.UtcNow;
        await _context.Sessions
            .Where(s => s.HostConnectionSlot != null || s.LiveParticipantCount > 0)
            .ExecuteUpdateAsync(
                setters => setters
                    .SetProperty(s => s.HostConnectionSlot, (string?)null)
                    .SetProperty(s => s.HostClaimedAt, (DateTimeOffset?)null)
                    .SetProperty(s => s.HostReleasedAt, (DateTimeOffset?)now)
                    .SetProperty(s => s.LiveParticipantCount, 0),
                ct);
    }

    public async Task<IReadOnlyList<OrphanedSessionCandidate>> GetOrphanedLiveSessionsAsync(
        DateTimeOffset noHostSince,
        CancellationToken ct = default)
    {




        return await _context.Sessions
            .AsNoTracking()
            .Where(s => s.HostConnectionSlot == null
                && (s.Status == SessionStatus.Pending
                    ? s.StartedAt < noHostSince && s.LastHeartbeatAt < noHostSince
                    : (s.Status == SessionStatus.Live || s.Status == SessionStatus.Reconnecting)
                        && (s.HostReleasedAt != null
                            ? s.HostReleasedAt < noHostSince
                            : s.StartedAt < noHostSince && s.LastHeartbeatAt < noHostSince)))
            .Select(s => new OrphanedSessionCandidate(
                s.Id,
                s.StartedAt,
                s.HostReleasedAt,
                s.LastHeartbeatAt))
            .ToListAsync(ct);
    }

    private async Task<IReadOnlyList<SessionCardProjection>> ProjectFeedAsync(
        IQueryable<Session> query,
        int limit,
        ToolKind? toolKindFilter,
        DateTimeOffset? since,
        CancellationToken ct)
    {
        if (toolKindFilter is not null)
            query = query.Where(s => s.ToolKind == toolKindFilter.Value);
        if (since is not null)
            query = query.Where(s => s.StartedAt > since.Value || s.LastHeartbeatAt > since.Value);

        return await query
            .Join(_context.Users, s => s.OwnerUserId, u => u.Id, (s, u) => new
            {
                s.Id,
                s.Title,
                s.Scope,
                s.DefaultAccess,
                s.Status,
                s.OwnerUserId,
                u.DisplayName,
                u.AvatarUrl,
                s.StartedAt,
                s.IncarnationId,
                s.ToolKind,
                s.RoomId
            })
            .OrderByDescending(x => x.StartedAt)
            .ThenByDescending(x => x.Id)
            .Take(limit + 1)
            .Select(x => new SessionCardProjection(
                x.Id.Value, x.Title, x.Scope, x.DefaultAccess, x.Status,
                x.OwnerUserId.Value, x.DisplayName, x.AvatarUrl,
                x.StartedAt, x.IncarnationId, x.ToolKind,
                x.RoomId != null ? x.RoomId.Value.Value : (Guid?)null))
            .ToListAsync(ct);
    }

    private async Task<IReadOnlyList<SessionCardProjection>> ProjectAllNonEndedAsync(
        IQueryable<Session> query,
        CancellationToken ct)
    {
        return await query
            .Join(_context.Users, session => session.OwnerUserId, user => user.Id, (session, user) => new
            {
                session.Id,
                session.Title,
                session.Scope,
                session.DefaultAccess,
                session.Status,
                session.OwnerUserId,
                user.DisplayName,
                user.AvatarUrl,
                session.StartedAt,
                session.IncarnationId,
                session.ToolKind,
                session.RoomId,
            })
            .OrderByDescending(item => item.StartedAt)
            .ThenByDescending(item => item.Id)
            .Select(item => new SessionCardProjection(
                item.Id.Value,
                item.Title,
                item.Scope,
                item.DefaultAccess,
                item.Status,
                item.OwnerUserId.Value,
                item.DisplayName,
                item.AvatarUrl,
                item.StartedAt,
                item.IncarnationId,
                item.ToolKind,
                item.RoomId != null ? item.RoomId.Value.Value : (Guid?)null))
            .ToListAsync(ct);
    }
}
