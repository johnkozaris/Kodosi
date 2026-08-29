using Kodosi.Domain;

namespace Kodosi.Application;



public sealed class LiveSessionTerminator(
    ISessionRepository sessions,
    ISessionKeyBlobRepository keyBlobs,
    IUnitOfWork unitOfWork,
    ISessionEndMutationRepository endMutations)
{
    private readonly ISessionRepository _sessions = sessions;
    private readonly ISessionKeyBlobRepository _keyBlobs = keyBlobs;
    private readonly ISessionEndMutationRepository _endMutations = endMutations;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;

    public async Task<LiveSessionTransitionResult> EndOwnedSessionIdempotentlyAsync(
        SessionId sessionId,
        UserId requestorId,
        Guid expectedIncarnationId,
        Guid mutationId,
        Guid attemptId,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _endMutations.AcquireAsync(requestorId, mutationId, ct);
        var existing = await _endMutations.GetAsync(requestorId, mutationId, ct);
        if (existing is not null)
        {
            if (!existing.Matches(sessionId, expectedIncarnationId))
            {
                throw new SessionEndMutationTargetConflictException();
            }

            var current = await _sessions.GetByIdForUpdateAsync(sessionId, ct);
            await transaction.CommitAsync(ct);
            return current is null || current.IncarnationId != expectedIncarnationId
                ? new LiveSessionTransitionResult(
                    SessionTransitionOutcome.AlreadyInTargetState,
                    null)
                : CreateTransitionResult(
                    SessionTransitionOutcome.AlreadyInTargetState,
                    current);
        }

        var session = await _sessions.GetByIdForUpdateAsync(sessionId, ct);
        if (session is null || !session.IsOwner(requestorId))
        {
            throw new NotFoundException(nameof(Session), sessionId);
        }
        if (session.IncarnationId != expectedIncarnationId)
        {
            await _endMutations.AddAsync(
                SessionEndMutation.Create(
                    requestorId,
                    mutationId,
                    sessionId,
                    expectedIncarnationId,
                    attemptId,
                    DateTimeOffset.UtcNow),
                ct);
            await _unitOfWork.SaveChangesAsync(ct);
            await transaction.CommitAsync(ct);
            return new LiveSessionTransitionResult(
                SessionTransitionOutcome.AlreadyInTargetState,
                null);
        }

        var result = await EndSessionInsideExistingTransactionAsync(session, ct);
        await _endMutations.AddAsync(
            SessionEndMutation.Create(
                requestorId,
                mutationId,
                sessionId,
                expectedIncarnationId,
                attemptId,
                DateTimeOffset.UtcNow),
            ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return result;
    }

    public async Task<LiveSessionTransitionResult> EndHostedSessionAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        string expectedConnectionId,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        var session = await _sessions.GetByIdForUpdateAsync(sessionId, ct);
        if (session is null)
        {
            return new LiveSessionTransitionResult(
                SessionTransitionOutcome.NotFound,
                null);
        }
        if (session.StartedAt != expectedStartedAt
            || !string.Equals(
                session.HostConnectionSlot,
                expectedConnectionId,
                StringComparison.Ordinal))
        {
            return CreateTransitionResult(SessionTransitionOutcome.Rejected, session);
        }

        var result = await EndSessionInsideExistingTransactionAsync(session, ct);
        await transaction.CommitAsync(ct);
        return result;
    }

    internal async Task<LiveSessionTransitionResult> EndSessionInsideExistingTransactionAsync(
        Session session,
        CancellationToken ct)
    {
        if (session.Status == SessionStatus.Ended)
        {
            return CreateTransitionResult(SessionTransitionOutcome.AlreadyInTargetState, session);
        }

        await ApplyEndWithBlobCascadeAsync(session, ct);
        return CreateTransitionResult(SessionTransitionOutcome.Applied, session);
    }

    public async Task<LiveSessionTransitionResult> EndDisconnectedSessionAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        var session = await _sessions.GetByIdForUpdateAsync(sessionId, ct);
        if (session is null)
        {
            return new LiveSessionTransitionResult(
                SessionTransitionOutcome.NotFound,
                null);
        }
        if (session.StartedAt != expectedStartedAt
            || session.HostConnectionSlot is not null)
        {
            return CreateTransitionResult(SessionTransitionOutcome.Rejected, session);
        }

        var result = await EndDisconnectedSessionInsideExistingTransactionAsync(
            session,
            ct);
        await transaction.CommitAsync(ct);
        return result;
    }

    public async Task<LiveSessionTransitionResult> EndOrphanedSessionAsync(
        OrphanedSessionCandidate candidate,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        var session = await _sessions.GetByIdForUpdateAsync(
            candidate.SessionId,
            ct);
        if (session is null)
        {
            return new LiveSessionTransitionResult(
                SessionTransitionOutcome.NotFound,
                null);
        }

        var markerMatches = candidate.ObservedHostReleasedAt.HasValue
            ? session.HostReleasedAt == candidate.ObservedHostReleasedAt
            : session.HostReleasedAt is null
                && session.LastHeartbeatAt
                    == candidate.ObservedLastHeartbeatAt;
        if (session.StartedAt != candidate.StartedAt
            || session.HostConnectionSlot is not null
            || !markerMatches)
        {
            return CreateTransitionResult(
                SessionTransitionOutcome.Rejected,
                session);
        }

        var result = await EndDisconnectedSessionInsideExistingTransactionAsync(
            session,
            ct);
        await transaction.CommitAsync(ct);
        return result;
    }

    private async Task<LiveSessionTransitionResult>
        EndDisconnectedSessionInsideExistingTransactionAsync(
            Session session,
            CancellationToken ct)
    {
        if (session.Status == SessionStatus.Ended)
        {
            return CreateTransitionResult(
                SessionTransitionOutcome.AlreadyInTargetState,
                session);
        }



        if (session.Status is not (
            SessionStatus.Pending or
            SessionStatus.Live or
            SessionStatus.Reconnecting))
        {
            return CreateTransitionResult(SessionTransitionOutcome.Rejected, session);
        }

        await ApplyEndWithBlobCascadeAsync(session, ct);
        return CreateTransitionResult(SessionTransitionOutcome.Applied, session);
    }

    private async Task ApplyEndWithBlobCascadeAsync(
    Session session,
    CancellationToken ct)
    {
        session.End();
        await _sessions.UpdateAsync(session, ct);
        await _keyBlobs.DeleteForSessionAsync(session.Id, ct);
        await _unitOfWork.SaveChangesAsync(ct);
    }

    private static LiveSessionTransitionResult CreateTransitionResult(
        SessionTransitionOutcome outcome,
        Session session)
    {
        return new LiveSessionTransitionResult(
            outcome,
            session.ToDiscoveryTarget(),
            session.StartedAt);
    }
}
