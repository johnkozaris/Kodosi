using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class SessionUpdater(
    ISessionRepository sessions,
    SessionRoomResolver roomResolver,
    IAccessOverrideRepository overrides,
    IAccessOverrideAuditRepository accessOverrideAudit,
    IFriendshipRepository friendships,
    IRoomMemberRepository roomMembers,
    IUserLifecycleLock userLifecycleLock,
    IRoomLifecycleLock roomLifecycleLock,
    ISessionEndAuthority sessionEndAuthority,
    IUnitOfWork unitOfWork)
{
    private readonly ISessionRepository _sessions = sessions;
    private readonly SessionRoomResolver _roomResolver = roomResolver;
    private readonly IAccessOverrideRepository _overrides = overrides;
    private readonly IAccessOverrideAuditRepository _accessOverrideAudit = accessOverrideAudit;
    private readonly IFriendshipRepository _friendships = friendships;
    private readonly IRoomMemberRepository _roomMembers = roomMembers;
    private readonly IUserLifecycleLock _userLifecycleLock = userLifecycleLock;
    private readonly IRoomLifecycleLock _roomLifecycleLock = roomLifecycleLock;
    private readonly ISessionEndAuthority _sessionEndAuthority = sessionEndAuthority;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;

    public async Task<OwnedSessionUpdateResult> UpdateOwnedAsync(
        SessionId id,
        UserId requestorId,
        UpdateSessionRequest request,
        CancellationToken ct = default,
        string? clientIp = null,
        string? userAgent = null)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _userLifecycleLock.AcquireAsync(requestorId, ct);
        var observed = await GetOwnedSessionAsync(id, requestorId, ct);
        if (request.Scope.HasValue)
        {
            var roomIds = new[]
                {
                    observed.RoomId,
                    request.Scope == SessionScope.Room && request.RoomId.HasValue
                        ? RoomId.From(request.RoomId.Value)
                        : (RoomId?)null,
                }
                .Where(roomId => roomId.HasValue)
                .Select(roomId => roomId!.Value)
                .Distinct()
                .OrderBy(roomId => roomId.Value)
                .ToList();
            foreach (var roomId in roomIds)
            {
                await _roomLifecycleLock.AcquireAsync(roomId, ct);
            }
        }

        var sessionLifecycle = await _sessionEndAuthority.AcquireAsync(
            [id],
            ct);
        try
        {
            var session = await GetOwnedSessionForUpdateAsync(id, requestorId, ct);
            if (session.IncarnationId != request.ExpectedIncarnationId)
            {
                throw new NotFoundException(nameof(Session), id);
            }
            var before = session.ToDiscoveryTarget();
            var oldAudience = SessionAudience.Of(session);

            if (request.Title is not null)
            {
                session.UpdateTitle(request.Title);
            }

            if (request.Scope.HasValue)
            {
                var roomId = await _roomResolver.ResolveAsync(
                    requestorId,
                    request.Scope.Value,
                    request.RoomId,
                    ct);
                session.UpdateScope(
                    request.Scope.Value,
                    roomId);
            }

            if (request.DefaultAccess.HasValue)
            {
                session.UpdateDefaultAccess(request.DefaultAccess.Value.ToAccessLevel());
            }





            var newAudience = SessionAudience.Of(session);
            if (ScopeOrdering.RequiresOverrideReconciliation(oldAudience, newAudience))
            {
                await RevokeOrphanedOverridesAsync(session, requestorId, clientIp, userAgent, ct);
            }

            await _sessions.UpdateAsync(session, ct);
            await _unitOfWork.SaveChangesAsync(ct);
            await transaction.CommitAsync(ct);

            return new OwnedSessionUpdateResult(
                session.ToDetailResponse(AccessLevel.Inject),
                before,
                session.ToDiscoveryTarget(),
                sessionLifecycle);
        }
        catch
        {
            await sessionLifecycle.DisposeAsync();
            throw;
        }
    }

    private async Task RevokeOrphanedOverridesAsync(
        Session session,
        UserId actorUserId,
        string? clientIp,
        string? userAgent,
        CancellationToken ct)
    {
        var activeOverrides = await _overrides.GetActiveBySessionAsync(session.Id, ct);
        if (activeOverrides.Count == 0)
        {
            return;
        }

        var granteeIds = activeOverrides.Select(o => o.ActorUserId).Distinct().ToList();
        HashSet<UserId> friendIds = [];
        HashSet<UserId> roomMemberIds = [];

        if (session.Scope == SessionScope.Friends)
        {
            friendIds = [.. await _friendships.GetFriendIdsAsync(session.OwnerUserId, ct)];
        }
        else if (session.Scope == SessionScope.Room && session.RoomId.HasValue)
        {
            roomMemberIds = [.. await _roomMembers.GetMemberUserIdsAsync(session.RoomId.Value, ct)];
        }

        foreach (var accessOverride in activeOverrides)
        {
            var isFriend = friendIds.Contains(accessOverride.ActorUserId);
            var isRoomMember = roomMemberIds.Contains(accessOverride.ActorUserId);
            var wouldResolveWithoutOverride = AccessResolver.TryResolveAccess(
                isOwner: false,
                session.Scope,
                session.DefaultAccess,
                isFriend,
                isRoomMember,
                explicitOverride: null,
                out _);
            if (!wouldResolveWithoutOverride)
            {
                accessOverride.Revoke();
                await _overrides.UpdateAsync(accessOverride, ct);
                await _accessOverrideAudit.AddAsync(
                    AccessOverrideAuditEntry.Create(
                        session.Id,
                        actorUserId,
                        accessOverride.ActorUserId,
                        AccessOverrideAuditAction.Revoked,
                        AccessOverrideAuditReason.ScopeNarrowing,
                        clientIp,
                        userAgent),
                    ct);
            }
        }
    }

    private async Task<Session> GetOwnedSessionAsync(
        SessionId id,
        UserId requestorId,
        CancellationToken ct)
    {
        var session = await _sessions.GetByIdAsync(id, ct)
            ?? throw new NotFoundException(nameof(Session), id);

        if (!session.IsOwner(requestorId))
        {
            throw new NotFoundException(nameof(Session), id);
        }

        return session;
    }

    private async Task<Session> GetOwnedSessionForUpdateAsync(
        SessionId id,
        UserId requestorId,
        CancellationToken ct)
    {
        var session = await _sessions.GetByIdForUpdateAsync(id, ct)
            ?? throw new NotFoundException(nameof(Session), id);
        if (!session.IsOwner(requestorId))
        {
            throw new NotFoundException(nameof(Session), id);
        }
        return session;
    }
}
