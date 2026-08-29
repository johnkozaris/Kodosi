using System.Collections.Frozen;

namespace Kodosi.Host.Realtime;

internal static class RelayMessageLimits
{
    public const int OuterMessageMaxBytes = 2 * 1024 * 1024;
    public const int TerminalCheckpointFrameMaxBytes = (8 * 1024 * 1024) + 63;
    public const int TerminalPresentationFrameMaxBytes = (8 * 1024 * 1024) + 37;
    public const int HostReceiveMaxBytes = TerminalCheckpointFrameMaxBytes;

    private const int ControlMessageMaxBytes = 16 * 1024;
    private const int FocusMessageMaxBytes = 1024;

    internal static readonly FrozenDictionary<string, int> MessageMaxBytes =
        new Dictionary<string, int>(StringComparer.Ordinal)
        {
            ["device.proof"] = ControlMessageMaxBytes,
            ["device.proofChallenge"] = ControlMessageMaxBytes,
            ["host.hello"] = OuterMessageMaxBytes,
            ["host.heartbeat"] = OuterMessageMaxBytes,
            ["host.end"] = OuterMessageMaxBytes,
            ["host.fenceAck"] = ControlMessageMaxBytes,
            ["term.semanticCheckpoint"] = TerminalCheckpointFrameMaxBytes,
            ["term.rawBatch"] = OuterMessageMaxBytes,
            ["term.presentation"] = TerminalPresentationFrameMaxBytes,
            ["permission.pendingSnapshot"] = OuterMessageMaxBytes,
            ["key.rotation"] = OuterMessageMaxBytes,
            ["host.accepted"] = OuterMessageMaxBytes,
            ["host.streamDemand"] = OuterMessageMaxBytes,
            ["host.keyDistributionRequested"] = OuterMessageMaxBytes,
            ["host.actionResult"] = ControlMessageMaxBytes,
            ["host.actionResultAck"] = ControlMessageMaxBytes,
            ["host.semanticSend"] = ControlMessageMaxBytes,
            ["host.semanticCancel"] = ControlMessageMaxBytes,
            ["host.semanticReceipt"] = ControlMessageMaxBytes,
            ["host.semanticReceiptAck"] = ControlMessageMaxBytes,
            ["host.stop"] = OuterMessageMaxBytes,
            ["host.interrupt"] = OuterMessageMaxBytes,
            ["host.input"] = OuterMessageMaxBytes,
            ["host.resize"] = ControlMessageMaxBytes,
            ["host.focusChanged"] = FocusMessageMaxBytes,
            ["host.participantDisconnected"] = FocusMessageMaxBytes,
            ["host.suggestion"] = OuterMessageMaxBytes,
            ["host.participantChanged"] = OuterMessageMaxBytes,
            ["host.accessRevoked"] = OuterMessageMaxBytes,
            ["host.permissionDecision"] = OuterMessageMaxBytes,
            ["participant.semanticSend"] = ControlMessageMaxBytes,
            ["participant.semanticCancel"] = ControlMessageMaxBytes,
            ["participant.semanticReceipt"] = ControlMessageMaxBytes,
            ["participant.semanticReceiptAck"] = ControlMessageMaxBytes,
            ["participant.join"] = ControlMessageMaxBytes,
            ["participant.heartbeat"] = ControlMessageMaxBytes,
            ["participant.suggest"] = ControlMessageMaxBytes,
            ["participant.inject"] = OuterMessageMaxBytes,
            ["participant.resize"] = ControlMessageMaxBytes,
            ["participant.focusChanged"] = FocusMessageMaxBytes,
            ["participant.permissionDecision"] = OuterMessageMaxBytes,
            ["participant.stop"] = ControlMessageMaxBytes,
            ["participant.interrupt"] = ControlMessageMaxBytes,
            ["participant.accepted"] = ControlMessageMaxBytes,
            ["action.result"] = ControlMessageMaxBytes,
            ["session.status"] = ControlMessageMaxBytes,
            ["session.ended"] = ControlMessageMaxBytes,
            ["session.accessRevoked"] = ControlMessageMaxBytes,
        }.ToFrozenDictionary(StringComparer.Ordinal);

    internal static readonly FrozenDictionary<string, string> ParticipantToHostTransforms =
        new Dictionary<string, string>(StringComparer.Ordinal)
        {
            ["participant.semanticSend"] = "host.semanticSend",
            ["participant.semanticCancel"] = "host.semanticCancel",
            ["participant.suggest"] = "host.suggestion",
            ["participant.inject"] = "host.input",
            ["participant.resize"] = "host.resize",
            ["participant.focusChanged"] = "host.focusChanged",
            ["participant.permissionDecision"] = "host.permissionDecision",
            ["participant.stop"] = "host.stop",
            ["participant.interrupt"] = "host.interrupt",
        }.ToFrozenDictionary(StringComparer.Ordinal);

    public static int GetMaxBytes(string messageType) =>
        MessageMaxBytes.TryGetValue(messageType, out var maxBytes)
            ? maxBytes
            : throw new ArgumentOutOfRangeException(
                nameof(messageType),
                messageType,
                "Unknown session relay message type.");

    public static bool IsWithinLimit(string messageType, int byteCount) =>
        byteCount <= GetMaxBytes(messageType);

    public static bool IsTransformedParticipantOutputWithinLimit(
        string participantMessageType,
        int byteCount) =>
        ParticipantToHostTransforms.TryGetValue(
            participantMessageType,
            out var hostMessageType)
            ? IsWithinLimit(hostMessageType, byteCount)
            : throw new ArgumentOutOfRangeException(
                nameof(participantMessageType),
                participantMessageType,
                "Participant message has no host transformation.");
}
