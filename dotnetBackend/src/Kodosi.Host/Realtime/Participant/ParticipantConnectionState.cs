using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal sealed class ParticipantConnectionState(
    string connectionId,
    string sessionIdText,
    UserId authenticatedUserId,
    TimeProvider? timeProvider = null) : DeviceAuthorizedConnectionState(
        connectionId,
        sessionIdText,
        authenticatedUserId,
        timeProvider)
{
    public byte[]? DeviceProofChallenge { get; set; }
    public ParticipantAccessDecision? AccessDecision { get; set; }
    public SessionSendQueues? SessionQueues { get; set; }
    public RelayClientSendQueue? ParticipantQueue { get; set; }
    public IParticipantActionAuthority? ActionAuthority { get; set; }
    public bool DbParticipantCounted { get; set; }
    public bool ParticipantReserved { get; set; }
    public bool ParticipantRegistered { get; set; }
}

internal readonly record struct ParticipantAccessDecision(
    bool IsOwnerParticipant,
    AccessLevel AccessLevel,
    DateTimeOffset SessionStartedAt = default,
    Guid SessionIncarnationId = default,
    long SessionIncarnationGeneration = default,
    SessionStatus DurableSessionStatus = SessionStatus.Live);

internal readonly record struct ParticipantAccessResolution(
    ParticipantAccessDecision Decision,
    bool AcceptsIncarnationHandshake);
