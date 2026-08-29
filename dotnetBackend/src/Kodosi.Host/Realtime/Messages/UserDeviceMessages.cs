using System.Text.Json.Serialization;

namespace Kodosi.Host.Realtime;

public sealed record UserDeviceListChangedMessage(
    [property: JsonPropertyName("userId")] string UserId,
    [property: JsonPropertyName("generation")] long Generation)
{
    [JsonPropertyName("type")] public string Type => "user.deviceListChanged";
}

public sealed record UserDeviceLinkSnapshotEntry(
    [property: JsonPropertyName("userCode")] string UserCode,
    [property: JsonPropertyName("deviceLabel")] string DeviceLabel,
    [property: JsonPropertyName("expiresAt")] DateTimeOffset ExpiresAt);

public sealed record UserDeviceLinkSnapshotMessage(
    [property: JsonPropertyName("requests")] IReadOnlyList<UserDeviceLinkSnapshotEntry> Requests)
{
    [JsonPropertyName("type")] public string Type => "user.deviceLinkSnapshot";
}


public sealed record UserDeviceLinkRequestedMessage(
    [property: JsonPropertyName("userCode")] string UserCode,
    [property: JsonPropertyName("deviceLabel")] string DeviceLabel,
    [property: JsonPropertyName("expiresAt")] DateTimeOffset ExpiresAt)
{
    [JsonPropertyName("type")] public string Type => "user.deviceLinkRequested";
}

public sealed record UserDeviceLinkResolvedMessage(
    [property: JsonPropertyName("userCode")] string UserCode,
    [property: JsonPropertyName("outcome")] string Outcome)
{
    [JsonPropertyName("type")] public string Type => "user.deviceLinkResolved";
}
