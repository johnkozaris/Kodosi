using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal interface IParticipantActionAuthority
{
    ValueTask<ParticipantActionDispatch> TryDispatchAsync(
        SessionCapability requiredCapability,
        Func<ChannelByteSendQueueWriteOutcome> dispatch,
        CancellationToken ct);
}

internal sealed class ParticipantActionAuthority(
    SessionId sessionId,
    string connectionId,
    UserId userId,
    string deviceId,
    ParticipantAccessDecision accessDecision,
    LiveSessionPorts expectedRuntime,
    SessionSendQueues expectedQueues,
    RelayClientSendQueue expectedParticipantQueue,
    ILiveSessionStateDirectory runtimes,
    IConnectionRegistry connections,
    SessionBroadcaster broadcaster,
    SessionLifecycleGate lifecycleGate) : IParticipantActionAuthority
{
    private readonly Guid _expectedRuntimeIncarnationId =
        expectedRuntime.IncarnationId;

    public async ValueTask<ParticipantActionDispatch> TryDispatchAsync(
        SessionCapability requiredCapability,
        Func<ChannelByteSendQueueWriteOutcome> dispatch,
        CancellationToken ct)
    {
        await using var lifecycle = await lifecycleGate.AcquireAsync(
            sessionId,
            ct);
        var currentRuntime = runtimes.TryGet(sessionId);
        if (!ReferenceEquals(currentRuntime, expectedRuntime)
            || currentRuntime.IncarnationId != _expectedRuntimeIncarnationId
            || !ReferenceEquals(
                broadcaster.TryGetSession(sessionId),
                expectedQueues)
            || !ReferenceEquals(
                expectedQueues.GetParticipantQueue(connectionId),
                expectedParticipantQueue)
            || expectedParticipantQueue.CompletionCause is not null
            || !connections.IsCurrentConnection(
                connectionId,
                userId,
                deviceId,
                sessionId,
                accessDecision.IsOwnerParticipant
                    ? SessionConnectionKind.OwnerParticipant
                    : SessionConnectionKind.SharedParticipant)
            || !RuntimeAccessGate.Allows(
                SessionCapabilities.FromAccess(
                    accessDecision.AccessLevel,
                    accessDecision.IsOwnerParticipant),
                requiredCapability,
                currentRuntime.Host.HostReady,
                currentRuntime.Host.Status))
        {
            return ParticipantActionDispatch.Rejected;
        }

        if (!expectedParticipantQueue.TryExecuteWhileOpen(
                dispatch,
                out var sendOutcome))
        {
            return ParticipantActionDispatch.Rejected;
        }

        return new ParticipantActionDispatch(
            Authorized: true,
            sendOutcome);
    }
}

internal readonly record struct ParticipantActionDispatch(
    bool Authorized,
    ChannelByteSendQueueWriteOutcome SendOutcome)
{
    public static ParticipantActionDispatch Rejected =>
        new(false, ChannelByteSendQueueWriteOutcome.Closed);
}
