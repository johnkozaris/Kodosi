namespace Kodosi.Domain;

public enum SemanticRequestState
{
    Pending,
    Dispatched,
    ReceiptStored,
    Acknowledged,
}

public sealed class SemanticRelayRequest
{
    public Guid Id { get; private set; }
    public SessionId SessionId { get; private set; }
    public Guid IncarnationId { get; private set; }
    public UserId RequesterUserId { get; private set; }
    public string RequesterDeviceId { get; private set; } = string.Empty;
    public Guid RequestId { get; private set; }
    public string Mode { get; private set; } = string.Empty;
    public string PayloadSha256 { get; private set; } = string.Empty;
    public SemanticRequestState State { get; private set; }
    public DateTimeOffset CreatedAt { get; private set; }
    public DateTimeOffset UpdatedAt { get; private set; }

    private SemanticRelayRequest() { }

    public static SemanticRelayRequest Create(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        Guid requestId,
        string mode,
        string payloadSha256,
        DateTimeOffset now)
    {
        if (!SemanticRelayWireRules.IsCanonicalUuidV7(incarnationId)
            || !SemanticRelayWireRules.IsCanonicalUuidV7(requestId))
        {
            throw new DomainException("Semantic relay identifiers must be canonical UUIDv7 values.");
        }
        if (string.IsNullOrWhiteSpace(requesterDeviceId)
            || !SemanticRelayWireRules.IsMode(mode)
            || !SemanticRelayWireRules.IsLowerHexSha256(payloadSha256))
        {
            throw new DomainException("Semantic relay request fields are invalid.");
        }
        return new SemanticRelayRequest
        {
            Id = Guid.NewGuid(),
            SessionId = sessionId,
            IncarnationId = incarnationId,
            RequesterUserId = requesterUserId,
            RequesterDeviceId = requesterDeviceId,
            RequestId = requestId,
            Mode = mode,
            PayloadSha256 = payloadSha256,
            State = SemanticRequestState.Pending,
            CreatedAt = now,
            UpdatedAt = now,
        };
    }

    public bool Matches(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        string mode,
        string payloadSha256) =>
        SessionId == sessionId
        && IncarnationId == incarnationId
        && RequesterUserId == requesterUserId
        && string.Equals(RequesterDeviceId, requesterDeviceId, StringComparison.Ordinal)
        && string.Equals(Mode, mode, StringComparison.Ordinal)
        && string.Equals(PayloadSha256, payloadSha256, StringComparison.Ordinal);

    public void MarkReceiptStored(DateTimeOffset now)
    {
        State = SemanticRequestState.ReceiptStored;
        UpdatedAt = now;
    }

    public void MarkAcknowledged(DateTimeOffset now)
    {
        if (State == SemanticRequestState.Acknowledged)
        {
            return;
        }
        if (State != SemanticRequestState.ReceiptStored)
        {
            throw new DomainException("A semantic request cannot be acknowledged before its receipt is stored.");
        }
        State = SemanticRequestState.Acknowledged;
        UpdatedAt = now;
    }
}
