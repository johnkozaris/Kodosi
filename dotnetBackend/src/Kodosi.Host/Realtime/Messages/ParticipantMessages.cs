using System.Text.Json.Serialization;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;



public sealed record ParticipantJoinMessage(
    [property: JsonPropertyName("checkpointRevision")]
    [property: JsonRequired]
    ulong CheckpointRevision,
    [property: JsonPropertyName("presentationRevision")]
    [property: JsonRequired]
    ulong PresentationRevision,
    [property: JsonPropertyName("nextSequence")]
    [property: JsonRequired]
    ulong NextSequence,
    [property: JsonPropertyName("lastSeenKeyGeneration")]
    [property: JsonRequired]
    uint LastSeenKeyGeneration,
    [property: JsonPropertyName("deviceId")] string DeviceId,
    [property: JsonPropertyName("expectedIncarnationId")]
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    Guid? ExpectedIncarnationId = null,
    [property: JsonPropertyName("relayProtocolVersion")]
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    int? RelayProtocolVersion = null)
{
    [JsonPropertyName("type")] public string Type => "participant.join";
}

[JsonConverter(typeof(JsonStringEnumConverter<RelaySemanticMode>))]
public enum RelaySemanticMode
{
    [JsonStringEnumMemberName("queue")]
    Queue,
    [JsonStringEnumMemberName("steer")]
    Steer,
    [JsonStringEnumMemberName("stopAndSend")]
    StopAndSend,
}

[JsonConverter(typeof(JsonStringEnumConverter<RelaySemanticOutcome>))]
public enum RelaySemanticOutcome
{
    [JsonStringEnumMemberName("injected")]
    Injected,
    [JsonStringEnumMemberName("cancelled")]
    Cancelled,
    [JsonStringEnumMemberName("deliveryUnknown")]
    DeliveryUnknown,
}

public sealed record ParticipantSemanticSendMessage(
    [property: JsonPropertyName("requestId")] Guid RequestId,
    [property: JsonPropertyName("incarnationId")] Guid IncarnationId,
    [property: JsonPropertyName("mode")] RelaySemanticMode Mode,
    [property: JsonPropertyName("payloadSha256")] string PayloadSha256,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "participant.semanticSend";
}

public sealed record ParticipantSemanticCancelMessage(
    [property: JsonPropertyName("requestId")] Guid RequestId,
    [property: JsonPropertyName("incarnationId")] Guid IncarnationId,
    [property: JsonPropertyName("mode")] RelaySemanticMode Mode,
    [property: JsonPropertyName("payloadSha256")] string PayloadSha256,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "participant.semanticCancel";
}

public sealed record ParticipantSemanticReceiptAckMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("incarnationId")] Guid IncarnationId,
    [property: JsonPropertyName("requestId")] Guid RequestId,
    [property: JsonPropertyName("requesterUserId")] string RequesterUserId,
    [property: JsonPropertyName("requesterDeviceId")] string RequesterDeviceId,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "participant.semanticReceiptAck";
}

public sealed record ParticipantSemanticReceiptMessage(
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
    [JsonPropertyName("type")] public string Type => "participant.semanticReceipt";
}




public sealed record ParticipantPermissionDecisionMessage(
    [property: JsonPropertyName("actionId")] string ActionId,
    [property: JsonPropertyName("requestId")] string RequestId,
    [property: JsonPropertyName("requestGeneration")] long RequestGeneration,
    [property: JsonPropertyName("decision")] string Decision,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "participant.permissionDecision";
}

public sealed record ParticipantSuggestMessage(
    [property: JsonPropertyName("actionId")] string ActionId,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "participant.suggest";
}

public sealed record ParticipantInjectMessage(
    [property: JsonPropertyName("actionId")] string ActionId,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "participant.inject";
}

public sealed record ParticipantResizeMessage(
    [property: JsonPropertyName("actionId")] string ActionId,
    [property: JsonPropertyName("rows")] ushort Rows,
    [property: JsonPropertyName("cols")] ushort Cols,
    [property: JsonPropertyName("widthPixels")][property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] uint? WidthPixels,
    [property: JsonPropertyName("heightPixels")][property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] uint? HeightPixels,
    [property: JsonPropertyName("cellWidthPixels")][property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] uint? CellWidthPixels,
    [property: JsonPropertyName("cellHeightPixels")][property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] uint? CellHeightPixels,


    [property: JsonPropertyName("claim")] bool Claim,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "participant.resize";
}

public sealed record ParticipantFocusChangedMessage(
    [property: JsonPropertyName("actionId")] string ActionId,
    [property: JsonPropertyName("focused")] bool? Focused,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "participant.focusChanged";
}

public sealed record ParticipantStopMessage(
    [property: JsonPropertyName("actionId")] string ActionId,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "participant.stop";
}

public sealed record ParticipantInterruptMessage(
    [property: JsonPropertyName("actionId")] string ActionId,
    [property: JsonPropertyName("nonce")] string Nonce,
    [property: JsonPropertyName("ciphertext")] string Ciphertext,
    [property: JsonPropertyName("signature")] string Signature)
{
    [JsonPropertyName("type")] public string Type => "participant.interrupt";
}

public sealed record ParticipantAcceptedMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("access")]
    [property: JsonConverter(typeof(JsonStringEnumConverter<AccessLevel>))]
    AccessLevel Access,
    [property: JsonPropertyName("capabilities")] ushort Capabilities,
    [property: JsonPropertyName("incarnationId")] Guid IncarnationId = default,
    [property: JsonPropertyName("incarnationGeneration")] long IncarnationGeneration = 0,
    [property: JsonPropertyName("relayProtocolVersion")] int RelayProtocolVersion = RelayProtocolVersions.Current)
{
    [JsonPropertyName("type")] public string Type => "participant.accepted";
}


[JsonConverter(typeof(JsonStringEnumConverter<RelayActionStatus>))]
public enum RelayActionStatus
{
    [JsonStringEnumMemberName("accepted")]
    Accepted,
    [JsonStringEnumMemberName("duplicate")]
    Duplicate,
    [JsonStringEnumMemberName("busy")]
    Busy,
    [JsonStringEnumMemberName("rejected")]
    Rejected,
}

public sealed record ActionResultMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("actionId")] string ActionId,
    [property: JsonPropertyName("status")] RelayActionStatus Status,
    [property: JsonPropertyName("requestId")]
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    string? RequestId = null,
    [property: JsonPropertyName("requestGeneration")]
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    long? RequestGeneration = null)
{
    [JsonPropertyName("type")] public string Type => "action.result";
}
