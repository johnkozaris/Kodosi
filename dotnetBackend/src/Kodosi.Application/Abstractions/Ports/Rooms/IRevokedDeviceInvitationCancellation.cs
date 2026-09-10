using Kodosi.Domain;

namespace Kodosi.Application;

public interface IRevokedDeviceInvitationCancellation
{
    Task<IReadOnlyList<UserId>> CancelPendingAsync(
        UserId ownerUserId,
        IReadOnlyCollection<string> revokedDeviceIds,
        DateTimeOffset cancelledAt,
        CancellationToken ct = default);
}
