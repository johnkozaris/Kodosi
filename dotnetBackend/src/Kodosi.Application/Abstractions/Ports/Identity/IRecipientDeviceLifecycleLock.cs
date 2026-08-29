using Kodosi.Domain;

namespace Kodosi.Application;

public interface IRecipientDeviceLifecycleLock
{
    Task AcquireAsync(
        IReadOnlyCollection<UserId> recipientUserIds,
        CancellationToken ct = default);
}
