using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Infrastructure.Realtime;

namespace Kodosi.HostTests;

public sealed class LiveSessionPortsTests
{
    [Fact]
    public void Construction_Preserves_Exact_Port_References_And_Identity()
    {
        var sessionId = SessionId.New();
        var host = new HostSessionState();
        var stream = new EncryptedStreamCache();
        var participants = new ParticipantRoster();
        var incarnationId = Guid.CreateVersion7();

        var ports = new LiveSessionPorts(
            sessionId,
            host,
            stream,
            participants,
            participants,
            incarnationId);

        Assert.Equal(sessionId, ports.SessionId);
        Assert.Same(host, ports.Host);
        Assert.Same(stream, ports.Stream);
        Assert.Same(participants, ports.Participants);
        Assert.Same(participants, ports.Demand);
        Assert.Equal(incarnationId, ports.IncarnationId);
    }

    [Fact]
    public void Construction_Rejects_Empty_Incarnation_Id()
    {
        var error = Assert.Throws<ArgumentException>(() => new LiveSessionPorts(
            SessionId.New(),
            new HostSessionState(),
            new EncryptedStreamCache(),
            new ParticipantRoster(),
            new ParticipantRoster(),
            Guid.Empty));

        Assert.Equal("incarnationId", error.ParamName);
    }
}
