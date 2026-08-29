using Kodosi.Domain;

namespace Kodosi.Application;

public interface ILiveSessionStateDirectory
{
    LiveSessionPorts? TryGet(SessionId sessionId);

    bool TryClaimHost(
        SessionId sessionId,
        string connectionId,
        CancellationTokenSource hostLifetime,
        out LiveSessionPorts? ports,
        out LiveSessionCreationOwnership? creationOwnership);

    bool RemoveIfSame(SessionId sessionId, LiveSessionPorts expected);

    bool RemoveIfOwned(
        SessionId sessionId,
        LiveSessionCreationOwnership creationOwnership);

    IReadOnlyList<SessionId> GetActiveSessions();
}

public sealed class LiveSessionCreationOwnership;
