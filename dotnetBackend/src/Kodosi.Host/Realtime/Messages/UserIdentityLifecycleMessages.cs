using System.Text.Json.Serialization;

namespace Kodosi.Host.Realtime;

public sealed record UserIdentityLifecycleChangedMessage(
    [property: JsonPropertyName("userId")] string UserId,
    [property: JsonPropertyName("identityRevision")] long IdentityRevision,
    [property: JsonPropertyName("incarnationId")] Guid? IncarnationId,
    [property: JsonPropertyName("state")] string State,
    [property: JsonPropertyName("generation")] long Generation)
{
    [JsonPropertyName("type")] public string Type => "user.identityLifecycleChanged";
}
