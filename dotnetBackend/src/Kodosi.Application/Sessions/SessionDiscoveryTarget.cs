using Kodosi.Domain;

namespace Kodosi.Application;

public sealed record SessionDiscoveryTarget(
    SessionId SessionId,
    UserId OwnerUserId,
    SessionScope Scope,
    RoomId? RoomId,
    Guid IncarnationId = default);
