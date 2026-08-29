using System.Text.Json.Serialization;

namespace Kodosi.Host.Realtime;

[JsonConverter(typeof(JsonStringEnumConverter<DiscoverySurface>))]
public enum DiscoverySurface
{
    [JsonStringEnumMemberName("friends")]
    Friends,
    [JsonStringEnumMemberName("room-catalog")]
    RoomCatalog,
    [JsonStringEnumMemberName("room-feed")]
    RoomFeed,
    [JsonStringEnumMemberName("room-chat")]
    RoomChat,
    [JsonStringEnumMemberName("room-tasks")]
    RoomTasks,
    [JsonStringEnumMemberName("own-sessions")]
    OwnSessions,
}

public sealed record DiscoveryInvalidatedMessage(
    [property: JsonPropertyName("surfaces")] IReadOnlyList<DiscoverySurface> Surfaces,
    [property: JsonPropertyName("roomId")]
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    string? RoomId = null)
{
    [JsonPropertyName("type")] public string Type => "discovery.invalidated";
}
