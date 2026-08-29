namespace Kodosi.Domain;

public static class SemanticReceiptPreimage
{
    public static byte[] Create(
        SessionId sessionId,
        Guid incarnationId,
        Guid requestId,
        string mode,
        string payloadSha256,
        string outcome,
        UserId requesterUserId,
        string requesterDeviceId,
        UserId ownerUserId,
        string ownerDeviceId)
    {
        if (sessionId.Value == Guid.Empty
            || incarnationId == Guid.Empty
            || requestId == Guid.Empty
            || requesterUserId.Value == Guid.Empty
            || ownerUserId.Value == Guid.Empty
            || !SemanticRelayWireRules.IsMode(mode)
            || !SemanticRelayWireRules.IsLowerHexSha256(payloadSha256)
            || !SemanticRelayWireRules.IsOutcome(outcome)
            || string.IsNullOrWhiteSpace(requesterDeviceId)
            || string.IsNullOrWhiteSpace(ownerDeviceId))
        {
            throw new DomainException("Semantic receipt preimage fields are invalid.");
        }

        using var stream = new MemoryStream();
        stream.Write(DomainTags.SemanticReceiptV1);
        CanonicalLengthPrefixedUtf8.Write(stream, sessionId.Value.ToString("D").ToLowerInvariant());
        CanonicalLengthPrefixedUtf8.Write(stream, incarnationId.ToString("D").ToLowerInvariant());
        CanonicalLengthPrefixedUtf8.Write(stream, requestId.ToString("D").ToLowerInvariant());
        CanonicalLengthPrefixedUtf8.Write(stream, mode);
        CanonicalLengthPrefixedUtf8.Write(stream, payloadSha256);
        CanonicalLengthPrefixedUtf8.Write(stream, outcome);
        CanonicalLengthPrefixedUtf8.Write(stream, requesterUserId.Value.ToString("D").ToLowerInvariant());
        CanonicalLengthPrefixedUtf8.Write(stream, requesterDeviceId);
        CanonicalLengthPrefixedUtf8.Write(stream, ownerUserId.Value.ToString("D").ToLowerInvariant());
        CanonicalLengthPrefixedUtf8.Write(stream, ownerDeviceId);
        return stream.ToArray();
    }
}
