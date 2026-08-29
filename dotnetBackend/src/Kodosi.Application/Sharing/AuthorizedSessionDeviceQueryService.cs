using Kodosi.Domain;

namespace Kodosi.Application;

public sealed record AuthorizedSessionDevice(UserId UserId, string DeviceId);

public sealed class AuthorizedSessionDeviceQueryService(
    ISessionRepository sessions,
    SessionAccessService access,
    IUserDeviceRepository devices,
    IUserDeviceListRepository deviceLists,
    TimeProvider timeProvider)
{
    private readonly ISessionRepository _sessions = sessions;
    private readonly SessionAccessService _access = access;
    private readonly IUserDeviceRepository _devices = devices;
    private readonly IUserDeviceListRepository _deviceLists = deviceLists;
    private readonly TimeProvider _timeProvider = timeProvider;

    public async Task<IReadOnlyList<AuthorizedSessionDevice>?> ListAsync(
        SessionId sessionId,
        UserId requesterId,
        CancellationToken ct = default)
    {
        var session = await _sessions.GetByIdAsync(sessionId, ct);
        if (session is null || !session.IsOwner(requesterId))
        {
            return null;
        }

        var authorizedUserIds = await _access.GetAuthorizedUserIdsAsync(session, ct);
        var candidateDevices = await _devices.GetByUserIdsAsync(authorizedUserIds, ct);
        var listsByUser = (await _deviceLists.GetLatestByUserIdsAsync(authorizedUserIds, ct))
            .ToDictionary(list => list.UserId);
        var now = _timeProvider.GetUtcNow();

        return candidateDevices
            .Where(device => listsByUser.TryGetValue(device.UserId, out var list)
                && ActiveDeviceAuthorization.IsAuthorized(device, list, device.UserId, now))
            .OrderBy(device => device.UserId.Value)
            .ThenBy(device => device.DeviceId, StringComparer.Ordinal)
            .Select(device => new AuthorizedSessionDevice(device.UserId, device.DeviceId))
            .ToArray();
    }
}
