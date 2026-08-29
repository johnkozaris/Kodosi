using Kodosi.Domain;

namespace Kodosi.Application;

public enum DeviceLinkCancelOutcome
{
    Cancelled,
    AlreadyCancelled,
    Approved,
    Expired,
    NotFound,
}

public enum DeviceLinkAcknowledgeOutcome
{
    Acknowledged,
    AlreadyAcknowledged,
    Invalidated,
    Unavailable,
}

public interface IDeviceLinkRequestRepository
{
    Task AddAsync(DeviceLinkRequest request, CancellationToken ct = default);

    Task<DeviceLinkRequest?> GetByDeviceCodeAsync(string deviceCode, CancellationToken ct = default);

    Task<DeviceLinkRequest?> GetByUserCodeAsync(string userCode, CancellationToken ct = default);

    Task<bool> IsUserCodeAvailableAsync(string userCode, CancellationToken ct = default);

    Task<IReadOnlyList<DeviceLinkRequest>> ListPendingForUserAsync(
        UserId userId,
        DateTimeOffset now,
        CancellationToken ct = default);

    void Update(DeviceLinkRequest request);

    Task<DeviceLinkCancelOutcome> CancelPendingAsync(
        string userCode,
        UserId userId,
        DateTimeOffset cancelledAt,
        CancellationToken ct = default);

    Task<DeviceLinkAcknowledgeOutcome> AcknowledgeApprovedAsync(
        Guid requestId,
        UserId userId,
        string deviceId,
        DateTimeOffset now,
        CancellationToken ct = default);

    Task<int> InvalidateApprovedBeforeGenerationAsync(
        UserId userId,
        long committedGeneration,
        Guid? excludingRequestId,
        DateTimeOffset invalidatedAt,
        CancellationToken ct = default);

    Task<int> InvalidateOutstandingForUserAsync(
        UserId userId,
        DateTimeOffset invalidatedAt,
        CancellationToken ct = default);

    Task<int> DeleteStaleAsync(DateTimeOffset now, CancellationToken ct = default);
}
