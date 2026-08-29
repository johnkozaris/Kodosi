using Kodosi.Application;
using Kodosi.Host.Configuration;
using Kodosi.Infrastructure.RateLimiting;

namespace Kodosi.Host.DependencyInjection;

public static class RateLimitingInfrastructureModule
{
    public static IServiceCollection AddRateLimitingInfrastructure(
        this IServiceCollection services,
        IConfiguration configuration)
    {
        services.AddSingleton<IFriendRequestThrottle, InMemoryFriendRequestThrottle>();
        var options = configuration
            .GetSection(RateLimitingOptions.SectionName)
            .Get<RateLimitingOptions>() ?? new();
        services.AddSingleton<IDeviceLinkPollThrottle>(serviceProvider =>
            new InMemoryDeviceLinkPollThrottle(
                options.DeviceLinkPoll.PermitLimit,
                TimeSpan.FromSeconds(options.DeviceLinkPoll.WindowSeconds),
                serviceProvider.GetRequiredService<TimeProvider>()));
        return services;
    }
}
