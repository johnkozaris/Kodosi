using Kodosi.Application;

namespace Kodosi.Host.DependencyInjection;

public static class RealtimeApplicationModule
{
    public static IServiceCollection AddRealtimeApplicationModule(this IServiceCollection services)
    {
        services.AddScoped<HostSessionActivator>();
        services.AddScoped<LiveSessionTransitionOrchestrator>();
        services.AddScoped<LiveSessionStatusReader>();
        services.AddScoped<LiveSessionTerminator>();
        return services;
    }
}
