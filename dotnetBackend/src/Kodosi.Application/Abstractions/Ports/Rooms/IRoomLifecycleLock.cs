using Kodosi.Domain;

namespace Kodosi.Application;

public interface IRoomLifecycleLock
{
    Task AcquireAsync(RoomId roomId, CancellationToken ct = default);
}
