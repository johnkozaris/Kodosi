using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class HostSessionActivator(
    ISessionRepository sessions,
    IOwnerSessionSecretHasher secretHasher,
    IUnitOfWork unitOfWork)
{
    private readonly ISessionRepository _sessions = sessions;
    private readonly IOwnerSessionSecretHasher _secretHasher = secretHasher;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;

    public async Task<HostSessionActivationResult> ActivateInsideExistingTransactionAsync(
        SessionId sessionId,
        UserId authenticatedUserId,
        string sessionSecret,
        Guid? expectedIncarnationId,
        string connectionId,
        Func<bool> tryClaimRuntime,
        CancellationToken ct)
    {
        var session = await _sessions.GetByIdForUpdateAsync(sessionId, ct);
        if (session is null
            || session.Status == SessionStatus.Ended
            || !session.IsOwner(authenticatedUserId)
            || !session.AcceptsIncarnationHandshake(expectedIncarnationId)
            || !_secretHasher.Verify(
                session.OwnerSessionSecretHash,
                sessionSecret))
        {
            return new HostSessionActivationResult(
                HostSessionActivationOutcome.InvalidSessionOrSecret,
                null);
        }
        if (session.HostConnectionSlot is not null)
        {
            return new HostSessionActivationResult(
                HostSessionActivationOutcome.AlreadyHosted,
                session.ToDiscoveryTarget());
        }
        if (!tryClaimRuntime())
        {
            return new HostSessionActivationResult(
                HostSessionActivationOutcome.AlreadyHosted,
                session.ToDiscoveryTarget());
        }

        session.ActivateHost(connectionId);
        await _sessions.UpdateAsync(session, ct);
        await _unitOfWork.SaveChangesAsync(ct);
        return new HostSessionActivationResult(
            HostSessionActivationOutcome.Applied,
            session.ToDiscoveryTarget(),
            session.StartedAt,
            session.IncarnationId,
            session.IncarnationGeneration);
    }
}

public sealed record HostSessionActivationResult(
    HostSessionActivationOutcome Outcome,
    SessionDiscoveryTarget? SharingState,
    DateTimeOffset? SessionStartedAt = null,
    Guid? SessionIncarnationId = null,
    long? SessionIncarnationGeneration = null);

public enum HostSessionActivationOutcome
{
    Applied,
    InvalidSessionOrSecret,
    AlreadyHosted,
}
