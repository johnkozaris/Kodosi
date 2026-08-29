using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class SessionKeyQueryService(
    ISessionRepository sessions,
    ISessionKeyBlobRepository keyBlobs,
    IUserDeviceRepository devices,
    IUserDeviceListRepository deviceLists,
    SessionAccessService accessService,
    TimeProvider timeProvider)
{
    private readonly ISessionRepository _sessions = sessions;
    private readonly ISessionKeyBlobRepository _keyBlobs = keyBlobs;
    private readonly IUserDeviceRepository _devices = devices;
    private readonly IUserDeviceListRepository _deviceLists = deviceLists;
    private readonly SessionAccessService _accessService = accessService;
    private readonly TimeProvider _timeProvider = timeProvider;

    public async Task<SessionKeyFetchResult> GetMySessionKeyAsync(
        SessionId sessionId,
        UserId actorUserId,
        string deviceId,
        CancellationToken ct = default)
    {
        var session = await _sessions.GetByIdAsync(sessionId, ct)
            ?? throw new NotFoundException(nameof(Session), sessionId);

        try
        {
            await _accessService.ResolveAccessAsync(session, actorUserId, ct);
        }
        catch (PolicyViolationException)
        {
            throw new NotFoundException(nameof(Session), sessionId);
        }

        if (session.Status == SessionStatus.Ended)
        {
            return SessionKeyFetchResult.SessionNotLive();
        }



        var userDevices = await _devices.GetByUserIdAsync(actorUserId, ct);
        var device = userDevices.FirstOrDefault(device =>
            string.Equals(device.DeviceId, deviceId, StringComparison.Ordinal));
        var deviceList = await _deviceLists.GetLatestAsync(actorUserId, ct);
        var deviceIsActive = ActiveDeviceAuthorization.IsAuthorized(
            device,
            deviceList,
            actorUserId,
            _timeProvider.GetUtcNow());
        if (!deviceIsActive)
        {
            return SessionKeyFetchResult.UnknownDevice();
        }

        var blob = await _keyBlobs.GetForDeviceAsync(sessionId, deviceId, ct);
        if (blob?.KeyGeneration == session.CurrentKeyGeneration)
        {
            return SessionKeyFetchResult.Ready(
                blob,
                session.IncarnationId,
                session.IncarnationProtocolVersion);
        }

        return session.Status is SessionStatus.Live or SessionStatus.Reconnecting
            ? SessionKeyFetchResult.PendingDistribution()
            : SessionKeyFetchResult.SessionNotLive();
    }

    public async Task<bool> ShouldRequestHostKeyDistributionAsync(
        SessionId sessionId,
        CancellationToken ct = default)
    {
        var session = await _sessions.GetByIdAsync(sessionId, ct);
        return session?.Status is SessionStatus.Live or SessionStatus.Reconnecting;
    }
}

public sealed record SessionKeyFetchResult(
    SessionKeyFetchState State,
    SessionKeyBlob? Blob = null,
    Guid? IncarnationId = null,
    int? IncarnationProtocolVersion = null)
{
    public static SessionKeyFetchResult Ready(
        SessionKeyBlob blob,
        Guid incarnationId,
        int incarnationProtocolVersion) =>
        new(
            SessionKeyFetchState.Ready,
            blob,
            incarnationId,
            incarnationProtocolVersion);

    public static SessionKeyFetchResult PendingDistribution() =>
        new(SessionKeyFetchState.PendingDistribution);

    public static SessionKeyFetchResult UnknownDevice() =>
        new(SessionKeyFetchState.UnknownDevice);

    public static SessionKeyFetchResult SessionNotLive() =>
        new(SessionKeyFetchState.SessionNotLive);
}
