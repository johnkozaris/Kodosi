using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class SessionReader(
    ISessionRepository sessions,
    SessionAccessService sessionAccess,
    ILiveSessionStateDirectory runtimes)
{
    private readonly ISessionRepository _sessions = sessions;
    private readonly SessionAccessService _sessionAccess = sessionAccess;
    private readonly ILiveSessionStateDirectory _runtimes = runtimes;

    public async Task<IReadOnlyList<SessionCardResponse>> GetMySessionsAsync(
        UserId userId,
        CancellationToken ct = default)
    {
        var projections = await _sessions.GetByOwnerProjectedAsync(userId, ct);
        return projections.Select(p => p.ToCardResponse(AccessLevel.Inject, _runtimes)).ToList();
    }

    public async Task<SessionDetailResponse> GetByIdAsync(
        SessionId id,
        UserId requestorId,
        CancellationToken ct = default)
    {
        var session = await _sessions.GetByIdAsync(id, ct)
            ?? throw new NotFoundException(nameof(Session), id);

        AccessLevel effectiveAccess;
        try
        {
            effectiveAccess = await _sessionAccess.ResolveAccessAsync(session, requestorId, ct);
        }
        catch (PolicyViolationException)
        {
            throw new NotFoundException(nameof(Session), id);
        }

        return session.ToDetailResponse(effectiveAccess);
    }

    public async Task<IReadOnlySet<UserId>> GetViewerIdsWithoutAccessAsync(
        SessionId id,
        UserId requestorId,
        IReadOnlyCollection<UserId> viewerUserIds,
        CancellationToken ct = default)
    {
        if (viewerUserIds.Count == 0)
        {
            return new HashSet<UserId>();
        }

        var session = await GetOwnedSessionAsync(id, requestorId, ct);
        return await _sessionAccess.GetDeniedUserIdsAsync(session, viewerUserIds, ct);
    }

    public async Task<AccessLevel?> ResolveViewerAccessAsync(
        SessionId id,
        UserId ownerUserId,
        UserId viewerUserId,
        CancellationToken ct = default)
    {
        var session = await GetOwnedSessionAsync(id, ownerUserId, ct);
        try
        {
            return await _sessionAccess.ResolveAccessAsync(
                session,
                viewerUserId,
                ct);
        }
        catch (PolicyViolationException)
        {
            return null;
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
}
