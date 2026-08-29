using Kodosi.Domain;

namespace Kodosi.Application;

public sealed record IdentityLifecycleProjection(
    UserId UserId,
    long IdentityRevision,
    Guid? IdentityIncarnationId,
    long Generation);

public interface IIdentityExposureRepository
{
    Task RecordAsync(
        UserId identityOwnerUserId,
        UserId recipientUserId,
        DateTimeOffset exposedAt,
        CancellationToken ct = default);

    Task<IReadOnlyList<UserId>> GetHistoricalPeerUserIdsAsync(
        UserId userId,
        CancellationToken ct = default);

    Task<IReadOnlyList<IdentityLifecycleProjection>> GetLifecycleSnapshotForRecipientAsync(
        UserId recipientUserId,
        CancellationToken ct = default);
}
