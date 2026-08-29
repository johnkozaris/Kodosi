using System.Text.Json.Serialization;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

public sealed record DeviceProofChallengeMessage(
    [property: JsonPropertyName("connectionId")] string ConnectionId,
    [property: JsonPropertyName("purpose")] string Purpose,
    [property: JsonPropertyName("sessionId")]
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    string? SessionId,
    [property: JsonPropertyName("challenge")] string Challenge)
{
    [JsonPropertyName("type")] public string Type => "device.proofChallenge";
}

public sealed record DeviceProofResponseMessage(
    [property: JsonPropertyName("type")] string Type,
    [property: JsonPropertyName("deviceId")] string DeviceId,
    [property: JsonPropertyName("sessionId")]
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    string? SessionId,
    [property: JsonPropertyName("expectedIncarnationId")]
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    Guid? ExpectedIncarnationId,
    [property: JsonPropertyName("signature")] string Signature);

public sealed record HostHelloMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("sessionSecret")] string SessionSecret,
    [property: JsonPropertyName("deviceId")] string DeviceId,
    [property: JsonPropertyName("expectedIncarnationId")]
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    Guid? ExpectedIncarnationId = null,
    [property: JsonPropertyName("relayProtocolVersion")]
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    int? RelayProtocolVersion = null)
{
    [JsonPropertyName("type")] public string Type => "host.hello";
}

public sealed record HostEndMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("reason")] string Reason)
{
    [JsonPropertyName("type")] public string Type => "host.end";
}

public sealed record HostAcceptedMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("relayEpoch")] string RelayEpoch,
    [property: JsonPropertyName("incarnationId")] Guid IncarnationId = default,
    [property: JsonPropertyName("incarnationGeneration")] long IncarnationGeneration = 0,
    [property: JsonPropertyName("relayProtocolVersion")] int RelayProtocolVersion = RelayProtocolVersions.Current)
{
    [JsonPropertyName("type")] public string Type => "host.accepted";
}

public sealed record HostFenceAckMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("fenceId")] string FenceId)
{
    [JsonPropertyName("type")] public string Type => "host.fenceAck";
}

public sealed record HostStreamDemandMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("required")] bool Required,
    [property: JsonPropertyName("sharedParticipantCount")] int SharedParticipantCount,
    [property: JsonPropertyName("ownerParticipantCount")] int OwnerParticipantCount,
    [property: JsonPropertyName("reason")] string Reason)
{
    [JsonPropertyName("type")] public string Type => "host.streamDemand";
}

public sealed record HostKeyDistributionRequestedMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("fenceId")] string FenceId)
{
    [JsonPropertyName("type")] public string Type => "host.keyDistributionRequested";
}

public sealed record HostActionResultMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("incarnationId")] Guid IncarnationId,
    [property: JsonPropertyName("actionId")] string ActionId,
    [property: JsonPropertyName("requestId")] string RequestId,
    [property: JsonPropertyName("requestGeneration")] long RequestGeneration,
    [property: JsonPropertyName("requesterUserId")] string RequesterUserId,
    [property: JsonPropertyName("requesterDeviceId")] string RequesterDeviceId,
    [property: JsonPropertyName("status")] RelayActionStatus Status)
{
    [JsonPropertyName("type")] public string Type => "host.actionResult";
}

public sealed record HostActionResultAckMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("incarnationId")] Guid IncarnationId,
    [property: JsonPropertyName("actionId")] string ActionId,
    [property: JsonPropertyName("requesterUserId")] string RequesterUserId)
{
    [JsonPropertyName("type")] public string Type => "host.actionResultAck";
}

public sealed record HostSemanticSendMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("requestId")] Guid RequestId,
    [property: JsonPropertyName("incarnationId")] Guid IncarnationId,
    [property: JsonPropertyName("mode")] RelaySemanticMode Mode,
    [property: JsonPropertyName("payloadSha256")] string PayloadSha256,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("senderUserId")] string SenderUserId,
    [property: JsonPropertyName("senderDeviceId")] string SenderDeviceId,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "host.semanticSend";
}

public sealed record HostSemanticCancelMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("requestId")] Guid RequestId,
    [property: JsonPropertyName("incarnationId")] Guid IncarnationId,
    [property: JsonPropertyName("mode")] RelaySemanticMode Mode,
    [property: JsonPropertyName("payloadSha256")] string PayloadSha256,
    [property: JsonPropertyName("requesterUserId")] string RequesterUserId,
    [property: JsonPropertyName("requesterDeviceId")] string RequesterDeviceId,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "host.semanticCancel";
}

public sealed record HostSemanticReceiptMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("incarnationId")] Guid IncarnationId,
    [property: JsonPropertyName("requestId")] Guid RequestId,
    [property: JsonPropertyName("mode")] RelaySemanticMode Mode,
    [property: JsonPropertyName("payloadSha256")] string PayloadSha256,
    [property: JsonPropertyName("outcome")] RelaySemanticOutcome Outcome,
    [property: JsonPropertyName("requesterUserId")] string RequesterUserId,
    [property: JsonPropertyName("requesterDeviceId")] string RequesterDeviceId,
    [property: JsonPropertyName("ownerUserId")] string OwnerUserId,
    [property: JsonPropertyName("ownerDeviceId")] string OwnerDeviceId,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "host.semanticReceipt";
}

public sealed record HostSemanticReceiptAckMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("incarnationId")] Guid IncarnationId,
    [property: JsonPropertyName("requestId")] Guid RequestId,
    [property: JsonPropertyName("requesterUserId")] string RequesterUserId,
    [property: JsonPropertyName("requesterDeviceId")] string RequesterDeviceId)
{
    [JsonPropertyName("type")] public string Type => "host.semanticReceiptAck";
}

public sealed record HostStopMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("commandId")] string CommandId,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("senderUserId")] string SenderUserId,
    [property: JsonPropertyName("senderDeviceId")] string SenderDeviceId,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "host.stop";
}

public sealed record HostInterruptMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("commandId")] string CommandId,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("senderUserId")] string SenderUserId,
    [property: JsonPropertyName("senderDeviceId")] string SenderDeviceId,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "host.interrupt";
}



public sealed record HostInputMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("commandId")] string CommandId,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("senderUserId")] string SenderUserId,
    [property: JsonPropertyName("senderDeviceId")] string SenderDeviceId,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "host.input";
}



public sealed record HostPermissionDecisionMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("commandId")] string CommandId,
    [property: JsonPropertyName("requestId")] string RequestId,
    [property: JsonPropertyName("requestGeneration")] long RequestGeneration,
    [property: JsonPropertyName("decision")] string Decision,
    [property: JsonPropertyName("deciderUserId")] string DeciderUserId,
    [property: JsonPropertyName("deciderDeviceId")][property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? DeciderDeviceId,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "host.permissionDecision";
}

public sealed record HostResizeMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("commandId")] string CommandId,
    [property: JsonPropertyName("rows")] ushort Rows,
    [property: JsonPropertyName("cols")] ushort Cols,
    [property: JsonPropertyName("widthPixels")][property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] uint? WidthPixels,
    [property: JsonPropertyName("heightPixels")][property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] uint? HeightPixels,
    [property: JsonPropertyName("cellWidthPixels")][property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] uint? CellWidthPixels,
    [property: JsonPropertyName("cellHeightPixels")][property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] uint? CellHeightPixels,
    [property: JsonPropertyName("claim")] bool Claim,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("senderUserId")] string SenderUserId,
    [property: JsonPropertyName("senderDeviceId")] string SenderDeviceId,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "host.resize";
}

public sealed record HostFocusChangedMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("commandId")] string CommandId,
    [property: JsonPropertyName("clientId")] string ClientId,
    [property: JsonPropertyName("focused")] bool Focused,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("senderUserId")] string SenderUserId,
    [property: JsonPropertyName("senderDeviceId")] string? SenderDeviceId,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "host.focusChanged";
}

public sealed record HostParticipantDisconnectedMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("clientId")] string ClientId)
{
    [JsonPropertyName("type")] public string Type => "host.participantDisconnected";
}

public sealed record HostSuggestionMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("suggestionId")] string SuggestionId,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("senderUserId")] string SenderUserId,
    [property: JsonPropertyName("senderDeviceId")] string SenderDeviceId,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "host.suggestion";
}
