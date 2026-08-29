using Kodosi.Domain;

namespace Kodosi.Application;

public interface IRoomMemberAuditRepository
{
    Task AddAsync(RoomMemberAuditEntry entry, CancellationToken ct = default);
}
