using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal sealed class RealtimeDeviceAuthorizationReader(
    IServiceScopeFactory scopeFactory,
    TimeProvider timeProvider)
{
    private readonly IServiceScopeFactory _scopeFactory = scopeFactory;
    private readonly TimeProvider _timeProvider = timeProvider;

    public async Task<ActiveDeviceAuthorizationDecision> EvaluateAsync(
        UserId userId,
        string deviceId,
        CancellationToken ct)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var devices = scope.ServiceProvider.GetRequiredService<IUserDeviceRepository>();
        var deviceLists = scope.ServiceProvider.GetRequiredService<IUserDeviceListRepository>();
        var device = await devices.GetByDeviceIdAsync(deviceId, ct);
        var deviceList = await deviceLists.GetLatestAsync(userId, ct);
        return ActiveDeviceAuthorization.Evaluate(
            device,
            deviceList,
            userId,
            _timeProvider.GetUtcNow());
    }
}
