using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class SessionCreationReceiptLookup(
    ISessionRepository sessions,
    ISessionIncarnationRepository incarnations,
    IUserLifecycleLock userLifecycleLock,
    ISessionEndAuthority sessionEndAuthority,
    IUnitOfWork unitOfWork)
{
    private readonly ISessionRepository _sessions = sessions;
    private readonly ISessionIncarnationRepository _incarnations = incarnations;
    private readonly IUserLifecycleLock _userLifecycleLock = userLifecycleLock;
    private readonly ISessionEndAuthority _sessionEndAuthority = sessionEndAuthority;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;

    public async Task<SessionCreationReceiptResponse> GetOwnedAsync(
        SessionId sessionId,
        Guid createIdempotencyKey,
        UserId requestorId,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _userLifecycleLock.AcquireAsync(requestorId, ct);
        await using var sessionLifecycle = await _sessionEndAuthority.AcquireAsync(
            [sessionId],
            ct);
        var session = await _sessions.GetByIdAsync(sessionId, ct);
        if (session is null || !session.IsOwner(requestorId))
        {
            throw new NotFoundException("Session creation receipt", sessionId);
        }

        var incarnation = await _incarnations.GetByIdempotencyKeyAsync(
            sessionId,
            createIdempotencyKey,
            ct);
        if (incarnation is null)
        {
            throw new NotFoundException(
                "Session creation receipt",
                $"{sessionId}/{createIdempotencyKey}");
        }

        await transaction.CommitAsync(ct);
        return new SessionCreationReceiptResponse(
            incarnation.SessionId.Value,
            incarnation.IdempotencyKey,
            incarnation.IncarnationId,
            incarnation.Generation,
            incarnation.ProtocolVersion);
    }
}
