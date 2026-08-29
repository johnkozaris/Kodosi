namespace Kodosi.Domain;

public sealed class SemanticRelayReceipt
{
    public Guid Id { get; private set; }
    public Guid RequestRowId { get; private set; }
    public SessionId SessionId { get; private set; }
    public Guid IncarnationId { get; private set; }
    public UserId RequesterUserId { get; private set; }
    public string RequesterDeviceId { get; private set; } = string.Empty;
    public Guid RequestId { get; private set; }
    public string Mode { get; private set; } = string.Empty;
    public string PayloadSha256 { get; private set; } = string.Empty;
    public string Outcome { get; private set; } = string.Empty;
    public UserId OwnerUserId { get; private set; }
    public string OwnerDeviceId { get; private set; } = string.Empty;
    public string Signature { get; private set; } = string.Empty;
    public DateTimeOffset StoredAt { get; private set; }
    public DateTimeOffset? AcknowledgedAt { get; private set; }

    private SemanticRelayReceipt() { }

    public static SemanticRelayReceipt Create(
        Guid requestRowId,
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        Guid requestId,
        string mode,
        string payloadSha256,
        string outcome,
        UserId ownerUserId,
        string ownerDeviceId,
        string signature,
        DateTimeOffset now)
    {
        if (requestRowId == Guid.Empty
            || !SemanticRelayWireRules.IsCanonicalUuidV7(incarnationId)
            || !SemanticRelayWireRules.IsCanonicalUuidV7(requestId)
            || string.IsNullOrWhiteSpace(requesterDeviceId)
            || string.IsNullOrWhiteSpace(ownerDeviceId)
            || string.IsNullOrWhiteSpace(signature)
            || !SemanticRelayWireRules.IsMode(mode)
            || !SemanticRelayWireRules.IsLowerHexSha256(payloadSha256)
            || !SemanticRelayWireRules.IsOutcome(outcome))
        {
            throw new DomainException("Semantic relay receipt fields are invalid.");
        }
        return new SemanticRelayReceipt
        {
            Id = Guid.NewGuid(),
            RequestRowId = requestRowId,
            SessionId = sessionId,
            IncarnationId = incarnationId,
            RequesterUserId = requesterUserId,
            RequesterDeviceId = requesterDeviceId,
            RequestId = requestId,
            Mode = mode,
            PayloadSha256 = payloadSha256,
            Outcome = outcome,
            OwnerUserId = ownerUserId,
            OwnerDeviceId = ownerDeviceId,
            Signature = signature,
            StoredAt = now,
        };
    }

    public void Acknowledge(DateTimeOffset now)
    {
        AcknowledgedAt ??= now;
    }
}
