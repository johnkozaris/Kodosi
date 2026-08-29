using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class SessionAccessQueryService(
    ISessionRepository sessions,
    IAccessOverrideRepository overrides,
    IUserRepository userRepository)
{
    private readonly ISessionRepository _sessions = sessions;
    private readonly IAccessOverrideRepository _overrides = overrides;
    private readonly IUserRepository _userRepository = userRepository;

    public async Task<SessionAccessGrantsResponse> GetActiveGrantsAsync(
        SessionId sessionId,
        Guid expectedIncarnationId,
        UserId requestorId,
        CancellationToken ct = default)
    {
        var session = await _sessions.GetByIdAsync(sessionId, ct)
            ?? throw new NotFoundException("Session", sessionId);

        if (!session.IsOwner(requestorId) || session.IncarnationId != expectedIncarnationId)
        {
            throw new NotFoundException("Session", sessionId);
        }

        var grants = await _overrides.GetActiveBySessionAsync(sessionId, ct);
        var userIds = grants.Select(grant => grant.ActorUserId).Distinct().ToList();
        var users = userIds.Count == 0
            ? []
            : await _userRepository.GetByIdsAsync(userIds, ct);
        var userLookup = users.ToDictionary(user => user.Id);
        var projected = grants
            .Where(grant => userLookup.ContainsKey(grant.ActorUserId))
            .Select(grant =>
            {
                var user = userLookup[grant.ActorUserId];
                return new SessionAccessGrantResponse(
                    grant.ActorUserId.Value,
                    user.Handle,
                    user.DisplayName,
                    grant.AccessLevel,
                    grant.CreatedAt,
                    grant.ExpiresAt);
            })
            .ToList();

        session = await _sessions.GetByIdAsync(sessionId, ct)
            ?? throw new NotFoundException("Session", sessionId);
        if (!session.IsOwner(requestorId) || session.IncarnationId != expectedIncarnationId)
        {
            throw new NotFoundException("Session", sessionId);
        }
        return new SessionAccessGrantsResponse(expectedIncarnationId, projected);
    }
}
