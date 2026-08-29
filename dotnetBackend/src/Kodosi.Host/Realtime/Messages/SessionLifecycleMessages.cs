using System.Text.Json.Serialization;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

public sealed record SessionStatusMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("status")]
    [property: JsonConverter(typeof(JsonStringEnumConverter<SessionStatus>))]
    SessionStatus Status)
{
    [JsonPropertyName("type")] public string Type => "session.status";
}

public sealed record SessionEndedMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("reason")] string Reason)
{
    [JsonPropertyName("type")] public string Type => "session.ended";
}

public sealed record HostParticipantChangedMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("fenceId")] string FenceId,
    [property: JsonPropertyName("participantUserId")] string ParticipantUserId,
    [property: JsonPropertyName("participantCount")] int ParticipantCount,
    [property: JsonPropertyName("action")] string Action)
{
    [JsonPropertyName("type")] public string Type => "host.participantChanged";
}

public sealed record HostAccessRevokedMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("fenceId")] string FenceId,
    [property: JsonPropertyName("revokedUserId")] string RevokedUserId)
{
    [JsonPropertyName("type")] public string Type => "host.accessRevoked";
}

public sealed record SessionAccessRevokedMessage(
    [property: JsonPropertyName("sessionId")] string SessionId)
{
    [JsonPropertyName("type")] public string Type => "session.accessRevoked";
}

public sealed record KeyRotationMessage(
    [property: JsonPropertyName("sessionId")] string SessionId,
    [property: JsonPropertyName("keyGeneration")] int KeyGeneration)
{
    [JsonPropertyName("type")] public string Type => "key.rotation";
}
