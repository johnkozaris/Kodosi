using Kodosi.Domain;

namespace Kodosi.Application;

public interface ISessionEndAuthority
{
    ValueTask<IAsyncDisposable> AcquireAsync(
        IReadOnlyCollection<SessionId> sessionIds,
        CancellationToken ct = default);

    Task ProjectCommittedAsync(
        IReadOnlyCollection<CommittedSessionEnd> sessionEnds,
        CancellationToken ct = default);

    Task RetireEndedIncarnationAsync(
        SessionDiscoveryTarget endedSession,
        DateTimeOffset startedAt,
        CancellationToken ct = default);
}

public sealed record CommittedSessionEnd(
    LiveSessionTransitionResult Transition,
    CommittedSessionEndReason Reason);

public enum CommittedSessionEndReason
{
    OwnerIdentityReset,
    AccessRevoked,
}
