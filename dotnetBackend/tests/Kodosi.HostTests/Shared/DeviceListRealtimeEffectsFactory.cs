using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Endpoints;
using Kodosi.Host.Realtime;
using Microsoft.Extensions.Logging;

namespace Kodosi.HostTests;

internal static class DeviceListRealtimeEffectsFactory
{
    public static DeviceListRealtimeEffects Create(
        IConnectionRegistry connections,
        SessionBroadcaster sessions,
        UserEventBroadcaster userEvents,
        ISessionEndAuthority sessionEndAuthority,
        IDeviceRevocationSessionResolver sessionResolver,
        ILiveSessionStateDirectory runtimes,
        ILogger<DeviceListRealtimeEffects> logger) =>
        new(
            new RealtimeDeviceAccessEnforcementCore(
                connections,
                sessions,
                userEvents),
            userEvents,
            sessionEndAuthority,
            sessionResolver,
            runtimes,
            logger);
}
