namespace Kodosi.Domain;

public static class DeviceConnectionProofPreimage
{
    public static byte[] Create(
        UserId userId,
        string deviceId,
        string connectionId,
        string purpose,
        string? sessionId,
        Guid? incarnationId,
        ReadOnlySpan<byte> challenge)
    {
        if (userId.Value == Guid.Empty
            || string.IsNullOrWhiteSpace(deviceId)
            || string.IsNullOrWhiteSpace(connectionId)
            || string.IsNullOrWhiteSpace(purpose)
            || challenge.Length != 32)
        {
            throw new DomainException("Device connection proof identity and challenge must be complete.");
        }

        using var stream = new MemoryStream();
        stream.Write(DomainTags.DeviceConnectionProofV1);
        CanonicalLengthPrefixedUtf8.Write(stream, userId.Value.ToString("D").ToLowerInvariant());
        CanonicalLengthPrefixedUtf8.Write(stream, DeviceIdRules.Require(deviceId));
        CanonicalLengthPrefixedUtf8.Write(stream, connectionId);
        CanonicalLengthPrefixedUtf8.Write(stream, purpose);
        CanonicalLengthPrefixedUtf8.Write(stream, sessionId ?? string.Empty);
        CanonicalLengthPrefixedUtf8.Write(
            stream,
            incarnationId?.ToString("D").ToLowerInvariant() ?? string.Empty);
        stream.Write(challenge);
        return stream.ToArray();
    }
}
