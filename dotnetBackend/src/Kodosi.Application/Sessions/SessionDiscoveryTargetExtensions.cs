using Kodosi.Domain;

namespace Kodosi.Application;

internal static class SessionDiscoveryTargetExtensions
{
    public static SessionDiscoveryTarget ToDiscoveryTarget(this Session session)
        => new(
            session.Id,
            session.OwnerUserId,
            session.Scope,
            session.RoomId,
            session.IncarnationId);
}
