
namespace Kodosi.Domain;

public sealed class InputAuditEntry
{
    public Guid Id { get; private set; }
    public SessionId SessionId { get; private set; }
    public UserId SenderUserId { get; private set; }
    public string ClientCommandId { get; private set; } = string.Empty;
    public InputAuditKind Kind { get; private set; }
    public string? PayloadSha256 { get; private set; }
    public int PayloadBytesLen { get; private set; }
    public InputAuditStatus Status { get; private set; }
    public DateTimeOffset SubmittedAt { get; private set; }
    public DateTimeOffset? DispatchedAt { get; private set; }
    public DateTimeOffset? CompletedAt { get; private set; }
    public int DuplicateCount { get; private set; }
    public DateTimeOffset? LastDuplicateAt { get; private set; }
    public Guid? PendingSessionIncarnationId { get; private set; }
    public long? PendingSessionIncarnationGeneration { get; private set; }
    public string? PendingRequestId { get; private set; }
    public long? PendingRequestGeneration { get; private set; }
    public string? PendingRequesterDeviceId { get; private set; }

    private InputAuditEntry() { }

    public static InputAuditEntry Create(
        SessionId sessionId,
        UserId senderUserId,
        string clientCommandId,
        InputAuditKind kind,
        string? payloadSha256,
        int payloadBytesLen)
    {
        return new InputAuditEntry
        {
            Id = Guid.NewGuid(),
            SessionId = sessionId,
            SenderUserId = senderUserId,
            ClientCommandId = clientCommandId,
            Kind = kind,
            PayloadSha256 = payloadSha256,
            PayloadBytesLen = payloadBytesLen,
            Status = InputAuditStatus.Pending,
            SubmittedAt = DateTimeOffset.UtcNow,
        };
    }

    public static InputAuditEntry CreatePendingPermissionDecision(
        SessionId sessionId,
        UserId senderUserId,
        string clientCommandId,
        string? payloadSha256,
        int payloadBytesLen,
        Guid sessionIncarnationId,
        long sessionIncarnationGeneration,
        string requestId,
        long requestGeneration,
        string requesterDeviceId)
    {
        if (sessionIncarnationId == Guid.Empty
            || sessionIncarnationGeneration <= 0
            || string.IsNullOrWhiteSpace(requestId)
            || requestGeneration <= 0
            || string.IsNullOrWhiteSpace(requesterDeviceId))
        {
            throw new DomainException("A pending permission decision requires its canonical session and requester tuple.");
        }

        var entry = Create(
            sessionId,
            senderUserId,
            clientCommandId,
            InputAuditKind.PermissionDecision,
            payloadSha256,
            payloadBytesLen);
        entry.PendingSessionIncarnationId = sessionIncarnationId;
        entry.PendingSessionIncarnationGeneration = sessionIncarnationGeneration;
        entry.PendingRequestId = requestId;
        entry.PendingRequestGeneration = requestGeneration;
        entry.PendingRequesterDeviceId = requesterDeviceId;
        return entry;
    }

    public void MarkDispatched()
    {
        Status = InputAuditStatus.Dispatched;
        DispatchedAt = DateTimeOffset.UtcNow;
    }

    public void MarkRejected()
    {
        Status = InputAuditStatus.Rejected;
        CompletedAt = DateTimeOffset.UtcNow;
    }

    public void MarkFailed()
    {
        Status = InputAuditStatus.Failed;
        CompletedAt = DateTimeOffset.UtcNow;
    }

    public void MarkDuplicate()
    {
        Status = InputAuditStatus.Duplicate;
        DuplicateCount++;
        LastDuplicateAt = DateTimeOffset.UtcNow;
        CompletedAt = LastDuplicateAt;
    }

    public bool TryRearm(InputAuditStatus initialStatus)
    {
        if (Status is not (InputAuditStatus.Failed or InputAuditStatus.Rejected))
        {
            return false;
        }
        if (initialStatus is not (InputAuditStatus.Pending or InputAuditStatus.Rejected))
        {
            throw new DomainException("A retried audit action must restart as pending or rejected.");
        }

        Status = initialStatus;
        SubmittedAt = DateTimeOffset.UtcNow;
        DispatchedAt = null;
        CompletedAt = initialStatus == InputAuditStatus.Rejected
            ? SubmittedAt
            : null;
        return true;
    }
}
