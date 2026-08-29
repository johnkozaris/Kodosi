using Kodosi.Domain;

namespace Kodosi.Application;

public interface ISessionKeyBlobRepository
{
    Task<SessionKeyBlob?> GetForDeviceAsync(
        SessionId sessionId,
        string recipientDeviceId,
        CancellationToken ct = default);

    Task AddRangeAsync(
        IReadOnlyList<SessionKeyBlob> blobs,
        CancellationToken ct = default);

    Task DeleteForSessionAsync(
        SessionId sessionId,
        CancellationToken ct = default);

    Task<IReadOnlyList<DeviceRevocationSessionTarget>>
        GetSessionTargetsForRecipientDevicesAsync(
            IReadOnlyCollection<string> recipientDeviceIds,
            CancellationToken ct = default);

    Task<IReadOnlyList<SessionId>> GetSessionIdsForRecipientDevicesAsync(
        IReadOnlyCollection<string> recipientDeviceIds,
        CancellationToken ct = default);

    Task<int> DeleteForRecipientDevicesAsync(
        IReadOnlyCollection<string> recipientDeviceIds,
        CancellationToken ct = default);
}
