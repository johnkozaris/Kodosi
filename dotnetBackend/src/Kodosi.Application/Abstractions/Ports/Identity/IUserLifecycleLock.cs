using Kodosi.Domain;

namespace Kodosi.Application;

public interface IUserLifecycleLock
{
    Task AcquireAsync(UserId userId, CancellationToken ct = default);
}
