namespace Kodosi.Domain;

public static class SemanticReceiptAckPreimage
{
    public static byte[] Create(
        SessionId sessionId,
        Guid incarnationId,
        Guid requestId,
        UserId requesterUserId,
        string requesterDeviceId)
    {
        if (sessionId.Value == Guid.Empty
            || incarnationId == Guid.Empty
            || requestId == Guid.Empty
            || requesterUserId.Value == Guid.Empty)
        {
            throw new DomainException("Semantic receipt acknowledgement identifiers must be non-empty.");
        }
        if (string.IsNullOrWhiteSpace(requesterDeviceId))
        {
            throw new DomainException("Semantic receipt acknowledgement device ID is required.");
        }

        using var stream = new MemoryStream();
        stream.Write(DomainTags.SemanticReceiptAckV1);
        CanonicalLengthPrefixedUtf8.Write(stream, sessionId.Value.ToString("D").ToLowerInvariant());
        CanonicalLengthPrefixedUtf8.Write(stream, incarnationId.ToString("D").ToLowerInvariant());
        CanonicalLengthPrefixedUtf8.Write(stream, requestId.ToString("D").ToLowerInvariant());
        CanonicalLengthPrefixedUtf8.Write(stream, requesterUserId.Value.ToString("D").ToLowerInvariant());
        CanonicalLengthPrefixedUtf8.Write(stream, requesterDeviceId);
        return stream.ToArray();
    }
}
