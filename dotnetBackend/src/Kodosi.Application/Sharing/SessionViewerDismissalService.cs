using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class SessionViewerDismissalService(
    ISessionRepository sessions,
    ISessionViewerDismissalRepository dismissals,
    SessionAccessService sessionAccess,
    ISessionAccessMutationRepository mutations,
    IUnitOfWork unitOfWork,
    TimeProvider timeProvider)
{
    private readonly ISessionRepository _sessions = sessions;
    private readonly ISessionViewerDismissalRepository _dismissals = dismissals;
    private readonly SessionAccessService _sessionAccess = sessionAccess;
    private readonly ISessionAccessMutationRepository _mutations = mutations;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;
    private readonly TimeProvider _timeProvider = timeProvider;

    public async Task<SessionDismissalResult> DismissAsync(
        SessionId sessionId,
        Guid expectedIncarnationId,
        Guid mutationId,
        UserId viewerUserId,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _mutations.AcquireAsync(viewerUserId, mutationId, ct);
        var session = await _sessions.GetByIdForUpdateAsync(sessionId, ct)
            ?? throw new NotFoundException(nameof(Session), sessionId);

        var existingMutation = await _mutations.GetAsync(
            viewerUserId,
            mutationId,
            ct);
        if (existingMutation is not null)
        {
            if (!existingMutation.Matches(
                    sessionId,
                    expectedIncarnationId,
                    SessionAccessMutationKind.Leave,
                    null,
                    null,
                    null))
            {
                throw new SessionAccessMutationTargetConflictException();
            }
            await transaction.CommitAsync(ct);
            return new SessionDismissalResult(
                session.ToDiscoveryTarget(),
                expectedIncarnationId,
                session.StartedAt,
                session.IncarnationId == expectedIncarnationId);
        }

        if (session.IsOwner(viewerUserId)
            || session.Status == SessionStatus.Ended
            || session.IncarnationId != expectedIncarnationId)
        {
            throw new NotFoundException(nameof(Session), sessionId);
        }

        try
        {
            await _sessionAccess.ResolveAccessAsync(session, viewerUserId, ct);
        }
        catch (PolicyViolationException)
        {
            throw new NotFoundException(nameof(Session), sessionId);
        }

        var dismissal = SessionViewerDismissal.Create(
            sessionId,
            viewerUserId);
        await _dismissals.AddIfAbsentAsync(dismissal, ct);
        await _mutations.AddAsync(
            SessionAccessMutation.Create(
                viewerUserId,
                mutationId,
                sessionId,
                expectedIncarnationId,
                SessionAccessMutationKind.Leave,
                null,
                null,
                null,
                _timeProvider.GetUtcNow()),
            ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return new SessionDismissalResult(
            session.ToDiscoveryTarget(),
            expectedIncarnationId,
            session.StartedAt,
            ShouldProject: true);
    }
}

public sealed record SessionDismissalResult(
    SessionDiscoveryTarget DiscoveryState,
    Guid IncarnationId,
    DateTimeOffset StartedAt,
    bool ShouldProject);
