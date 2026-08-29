using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class LiveSessionTransitionOrchestrator(
    ISessionRepository sessions,
    IUnitOfWork unitOfWork)
{
    private readonly ISessionRepository _sessions = sessions;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;

    public async Task<LiveSessionTransitionResult> TransitionTimedOutHostToReconnectingAsync(
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
        if (session.Status != SessionStatus.Live
            || session.StartedAt != expectedStartedAt
            || !string.Equals(
                session.HostConnectionSlot,
                expectedConnectionId,
                StringComparison.Ordinal))
        {
            return CreateTransitionResult(SessionTransitionOutcome.Rejected, session);
        }

        session.MarkReconnecting();
        session.ReleaseHostSlot(expectedConnectionId);
        await _sessions.UpdateAsync(session, ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return CreateTransitionResult(SessionTransitionOutcome.Applied, session);
    }

    public async Task<LiveSessionTransitionResult> TransitionExpectedHostToReconnectingAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        Guid expectedIncarnationId,
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
        if (session.Status != SessionStatus.Live
            || session.StartedAt != expectedStartedAt
            || session.IncarnationId != expectedIncarnationId
            || !string.Equals(
                session.HostConnectionSlot,
                expectedConnectionId,
                StringComparison.Ordinal))
        {
            return CreateTransitionResult(SessionTransitionOutcome.Rejected, session);
        }

        session.MarkReconnecting();
        session.ReleaseHostSlot(expectedConnectionId);
        await _sessions.UpdateAsync(session, ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return CreateTransitionResult(SessionTransitionOutcome.Applied, session);
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
