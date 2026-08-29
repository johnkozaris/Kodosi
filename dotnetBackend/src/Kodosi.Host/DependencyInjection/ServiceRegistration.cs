using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Host.Configuration;
using Kodosi.Host.Middleware;

namespace Kodosi.Host.DependencyInjection;

public static partial class ServiceRegistration
{
    public static IServiceCollection AddHostAdapters(
        this IServiceCollection services,
        IConfiguration configuration)
    {
        var authOptions = configuration.GetSection(OidcAuthOptions.SectionName).Get<OidcAuthOptions>() ?? new();
        var rateLimitingOptions = configuration.GetSection(RateLimitingOptions.SectionName).Get<RateLimitingOptions>() ?? new();
        var proxyOptions = configuration.GetSection(ProxyOptions.SectionName).Get<ProxyOptions>() ?? new();
        var telemetryOptions = configuration.GetSection(TelemetryOptions.SectionName).Get<TelemetryOptions>() ?? new();
        var webSocketSecurityOptions = configuration.GetSection(WebSocketSecurityOptions.SectionName).Get<WebSocketSecurityOptions>() ?? new();
        var requestLimitsOptions = configuration.GetSection(RequestLimitsOptions.SectionName).Get<RequestLimitsOptions>() ?? new();
        var knownProxyAddresses = ParseKnownProxyAddresses(proxyOptions);

        ValidateAuthOptions(authOptions);
        ValidateTelemetryOptions(telemetryOptions);
        HostOptionsValidator.Validate(
            rateLimitingOptions,
            webSocketSecurityOptions,
            requestLimitsOptions);

        services.AddHttpContextAccessor();
        services.AddSingleton<AuthenticatedUserSyncCache>();
        services.AddSingleton<AuthenticatedUserProvisioningLock>();
        services.AddScoped<ICurrentUser, AuthenticatedCurrentUser>();
        services.Configure<OidcAuthOptions>(
            configuration.GetSection(OidcAuthOptions.SectionName));
        services.Configure<WebSocketSecurityOptions>(
            configuration.GetSection(WebSocketSecurityOptions.SectionName));
        ConfigureAuthentication(services, authOptions);
        ConfigureRateLimiting(services, rateLimitingOptions);
        ConfigureForwardedHeaders(services, proxyOptions, knownProxyAddresses);
        ConfigureJsonContracts(services);
        ConfigureObservability(services, telemetryOptions);
        ConfigureRealtimeHost(services);

        return services;
    }
}
