namespace Kodosi.Domain;

public static class DeviceHttpRequestProofPreimage
{
    public static byte[] Create(
        UserId userId,
        string deviceId,
        Guid challengeId,
        string method,
        string canonicalPathAndQuery,
        string bodySha256,
        ReadOnlySpan<byte> challenge)
    {
        if (userId.Value == Guid.Empty
            || string.IsNullOrWhiteSpace(deviceId)
            || challengeId == Guid.Empty
            || string.IsNullOrWhiteSpace(method)
            || string.IsNullOrWhiteSpace(canonicalPathAndQuery)
            || bodySha256.Length != 64
            || bodySha256.Any(character => !Uri.IsHexDigit(character))
            || challenge.Length != 32)
        {
            throw new DomainException("Device HTTP request proof tuple is invalid.");
        }

        using var stream = new MemoryStream();
        stream.Write(DomainTags.DeviceHttpRequestProofV1);
        foreach (var value in new[]
        {
            userId.Value.ToString("D").ToLowerInvariant(),
            DeviceIdRules.Require(deviceId),
            challengeId.ToString("D").ToLowerInvariant(),
            method.ToUpperInvariant(),
            canonicalPathAndQuery,
            bodySha256.ToLowerInvariant(),
        })
        {
            CanonicalLengthPrefixedUtf8.Write(stream, value);
        }
        stream.Write(challenge);
        return stream.ToArray();
    }
}
