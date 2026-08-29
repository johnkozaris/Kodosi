using Kodosi.Application;

namespace Kodosi.Host.DependencyInjection;

public static class SessionsModule
{
    public static IServiceCollection AddSessionsModule(this IServiceCollection services)
    {
        services.AddScoped<SessionRoomResolver>();
        services.AddScoped<SessionCreator>();
        services.AddScoped<SessionKeyGenerationClaimer>();
        services.AddScoped<SessionReader>();
        services.AddScoped<SessionCreationReceiptLookup>();
        services.AddScoped<SessionUpdater>();
        return services;
    }
}
