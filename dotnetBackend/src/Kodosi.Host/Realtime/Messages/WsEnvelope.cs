using System.Text.Json.Serialization;

namespace Kodosi.Host.Realtime;

public sealed record WsEnvelope(
    [property: JsonPropertyName("type")] string Type);
