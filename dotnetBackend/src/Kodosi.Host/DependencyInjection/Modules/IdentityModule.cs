using Kodosi.Application;
using Kodosi.Host.Endpoints;
using Kodosi.Host.Auth;
using Kodosi.Host.Identity;

namespace Kodosi.Host.DependencyInjection;

public static class IdentityModule
{
    public static IServiceCollection AddIdentityModule(this IServiceCollection services)
    {
        services.AddScoped<UserIdentityService>();
        services.AddScoped<UserIdentityBundleService>();
        services.AddScoped<ArtifactEndorsementService>();
        services.AddScoped<PopChallengeConsumer>();
        services.AddScoped<IdentityResetPopVerifier>();
        services.AddScoped<IdentityResetCascade>();
        services.AddScoped<IdentityLifecycleService>();
        services.AddScoped<DeviceListReplacementService>();
        services.AddScoped<DeviceRegistrationChallengeService>();
        services.AddScoped<DeviceHttpRequestProofVerifier>();
        services.AddScoped<DeviceEnrollmentService>();
        services.AddScoped<DeviceEnrollmentPublisher>();
        services.AddScoped<DeviceLinkRequestService>();
        services.AddScoped<DeviceLinkRequestPublisher>();
        services.AddScoped<ISignedDeviceListVerifier, SignedDeviceListVerifier>();
        services.AddSingleton<IDeviceListRealtimeEffects, DeviceListRealtimeEffects>();
        services.AddScoped<DeviceLinkApprovalService>();
        services.AddScoped<DeviceLinkPollService>();
        services.AddScoped<IDeviceEnrollmentVerifier, DeviceEnrollmentVerifier>();
        services.AddScoped<IDeviceLinkRealtimeEffects, DeviceLinkRealtimeEffects>();
        return services;
    }
}
