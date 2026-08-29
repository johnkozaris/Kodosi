using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class SessionKeyGenerationClaimer(
    ISessionRepository sessions,
    IUnitOfWork unitOfWork)
{
    private readonly ISessionRepository _sessions = sessions;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;

    public async Task<SessionKeyGenerationClaimResult> ClaimNextAsync(
        SessionId sessionId,
        UserId requestorId,
        Guid expectedIncarnationId,
        int expectedCurrentGeneration,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        var session = await _sessions.GetByIdForUpdateAsync(sessionId, ct);
        if (session is null || !session.IsOwner(requestorId))
        {
            return new SessionKeyGenerationClaimResult(
                SessionKeyGenerationClaimState.NotFound);
        }
        if (session.IncarnationId != expectedIncarnationId)
        {
            return new SessionKeyGenerationClaimResult(
                SessionKeyGenerationClaimState.StaleIncarnation);
        }
        if (session.Status == SessionStatus.Ended)
        {
            return new SessionKeyGenerationClaimResult(
                SessionKeyGenerationClaimState.Ended);
        }
        if (session.CurrentKeyGeneration != expectedCurrentGeneration)
        {
            return new SessionKeyGenerationClaimResult(
                SessionKeyGenerationClaimState.GenerationChanged,
                session.CurrentKeyGeneration);
        }
        if (session.CurrentKeyGeneration == int.MaxValue)
        {
            return new SessionKeyGenerationClaimResult(
                SessionKeyGenerationClaimState.Exhausted);
        }

        session.FenceKeyPublication();
        await _sessions.UpdateAsync(session, ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return new SessionKeyGenerationClaimResult(
            SessionKeyGenerationClaimState.Claimed,
            session.CurrentKeyGeneration);
    }

    public async Task<SessionKeyGenerationReadResult> ReadCurrentAsync(
        SessionId sessionId,
        UserId requestorId,
        Guid expectedIncarnationId,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        var session = await _sessions.GetByIdForUpdateAsync(sessionId, ct);
        if (session is null || !session.IsOwner(requestorId))
        {
            return new SessionKeyGenerationReadResult(
                SessionKeyGenerationReadState.NotFound);
        }
        if (session.IncarnationId != expectedIncarnationId)
        {
            return new SessionKeyGenerationReadResult(
                SessionKeyGenerationReadState.StaleIncarnation);
        }
        if (session.Status == SessionStatus.Ended)
        {
            return new SessionKeyGenerationReadResult(
                SessionKeyGenerationReadState.Ended);
        }

        await transaction.CommitAsync(ct);
        return new SessionKeyGenerationReadResult(
            SessionKeyGenerationReadState.Found,
            session.CurrentKeyGeneration);
    }
}

public enum SessionKeyGenerationClaimState
{
    Claimed,
    NotFound,
    StaleIncarnation,
    GenerationChanged,
    Ended,
    Exhausted,
}

public readonly record struct SessionKeyGenerationClaimResult(
    SessionKeyGenerationClaimState State,
    int? Generation = null);

public enum SessionKeyGenerationReadState
{
    Found,
    NotFound,
    StaleIncarnation,
    Ended,
}

public readonly record struct SessionKeyGenerationReadResult(
    SessionKeyGenerationReadState State,
    int? Generation = null);
