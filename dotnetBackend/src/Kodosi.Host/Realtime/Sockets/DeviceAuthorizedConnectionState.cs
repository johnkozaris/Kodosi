using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal abstract class DeviceAuthorizedConnectionState(
    string connectionId,
    string sessionIdText,
    UserId authenticatedUserId,
    TimeProvider? timeProvider = null)
{
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;
    private CancellationTokenRegistration? _deviceAuthorizationRegistration;

    public string ConnectionId { get; } = connectionId;
    public string SessionIdText { get; } = sessionIdText;
    public UserId AuthenticatedUserId { get; } = authenticatedUserId;
    public SessionId? SessionId { get; set; }
    public LiveSessionPorts? Ports { get; set; }
    public CancellationTokenSource? LinkedCts { get; set; }
    public bool AuthRevoked { get; set; }
    public bool DeviceAccessRevoked { get; private set; }
    public string? DeviceId { get; set; }



    public CancellationTokenSource DeviceAuthorizationCts { get; } =
        new(Timeout.InfiniteTimeSpan, timeProvider ?? TimeProvider.System);

    public void StartDeviceAuthorizationLifetime(DateTimeOffset? expiresAt)
    {
        _deviceAuthorizationRegistration ??=
            DeviceAuthorizationCts.Token.Register(() => DeviceAccessRevoked = true);
        RescheduleDeviceAuthorizationExpiry(expiresAt);
    }

    public void RescheduleDeviceAuthorizationExpiry(DateTimeOffset? expiresAt)
    {
        if (expiresAt is null)
        {
            DeviceAuthorizationCts.CancelAfter(Timeout.InfiniteTimeSpan);
            return;
        }

        var delay = expiresAt.Value - _timeProvider.GetUtcNow();
        if (delay <= TimeSpan.Zero)
        {
            DeviceAuthorizationCts.Cancel();
            return;
        }

        DeviceAuthorizationCts.CancelAfter(delay);
    }

    public void DisposeDeviceAuthorizationLifetime()
    {
        _deviceAuthorizationRegistration?.Dispose();
        DeviceAuthorizationCts.Dispose();
    }
}
