using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class LiveSessionPorts
{
    public LiveSessionPorts(
        SessionId sessionId,
        ILiveSessionHostState host,
        ILiveSessionStreamCache stream,
        ILiveParticipantRoster participants,
        ILiveStreamDemand demand,
        Guid incarnationId)
    {
        if (incarnationId == Guid.Empty)
        {
            throw new ArgumentException(
                "Runtime incarnation ID must not be empty.",
                nameof(incarnationId));
        }

        SessionId = sessionId;
        Host = host;
        Stream = stream;
        Participants = participants;
        Demand = demand;
        IncarnationId = incarnationId;
    }

    public SessionId SessionId { get; }
    public ILiveSessionHostState Host { get; }
    public ILiveSessionStreamCache Stream { get; }
    public ILiveParticipantRoster Participants { get; }
    public ILiveStreamDemand Demand { get; }
    public Guid IncarnationId { get; }
}
