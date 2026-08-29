using Kodosi.Domain;

namespace Kodosi.Application;


public static class DeviceListAudience
{
    public static List<UserId> Build(
        IEnumerable<UserId> friendIds,
        IEnumerable<UserId> roomPeerIds,
        UserId selfUserId)
    {
        return friendIds
            .Concat(roomPeerIds)
            .Concat([selfUserId])
            .Distinct()
            .ToList();
    }
}
