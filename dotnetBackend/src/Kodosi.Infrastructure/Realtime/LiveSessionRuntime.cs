using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Realtime;

internal sealed class LiveSessionRuntime
{
    public LiveSessionRuntime(SessionId sessionId, TimeProvider? timeProvider = null)
    {
        var host = new HostSessionState(timeProvider);
        var stream = new EncryptedStreamCache();
        var participants = new ParticipantRoster(timeProvider);
        Ports = new LiveSessionPorts(
            sessionId,
            host,
            stream,
            participants,
            participants,
            Guid.NewGuid());
    }

    public LiveSessionPorts Ports { get; }
    public LiveSessionCreationOwnership CreationOwnership { get; } = new();
}
