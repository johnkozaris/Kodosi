using Kodosi.Domain;

namespace Kodosi.Application;

public interface IFriendshipAuditRepository
{
    Task AddAsync(FriendshipAuditEntry entry, CancellationToken ct = default);
}
