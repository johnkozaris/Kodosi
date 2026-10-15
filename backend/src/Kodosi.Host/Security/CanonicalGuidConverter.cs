using System.Text.Json;
using System.Text.Json.Serialization;

namespace Kodosi.Security;

internal sealed class CanonicalGuidConverter : JsonConverter<Guid>
{
    public override Guid Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options) =>
        reader.TokenType == JsonTokenType.String ? Parse(reader.GetString()) : throw new JsonException("Expected a canonical UUID.");

    public override Guid ReadAsPropertyName(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options) => Parse(reader.GetString());

    public override void Write(Utf8JsonWriter writer, Guid value, JsonSerializerOptions options) => writer.WriteStringValue(value);

    private static Guid Parse(string? value) =>
        Guid.TryParseExact(value, "D", out var id) && id != Guid.Empty && value == id.ToString("D")
            ? id : throw new JsonException("Expected a lowercase nonzero UUID.");
}
