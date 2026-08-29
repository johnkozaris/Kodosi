using System.Net;
using System.Security.Claims;
using System.Threading.RateLimiting;
using Microsoft.AspNetCore.HttpOverrides;
using OpenTelemetry.Exporter;
using OpenTelemetry.Metrics;
using OpenTelemetry.Resources;
using OpenTelemetry.Trace;
using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Host.Configuration;
using Kodosi.Host.Health;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;

namespace Kodosi.Host.DependencyInjection;

public static partial class ServiceRegistration
{
    private static void ConfigureRateLimiting(
        IServiceCollection services,
        RateLimitingOptions rateLimitingOptions)
    {
        services.AddRateLimiter(options =>
        {
            options.RejectionStatusCode = StatusCodes.Status429TooManyRequests;
            options.AddPolicy(RateLimitPolicyNames.Api, httpContext =>
                BuildPartition(httpContext, rateLimitingOptions.Api));
            options.AddPolicy(RateLimitPolicyNames.WebSocket, httpContext =>
                BuildPartition(httpContext, rateLimitingOptions.WebSockets));
            options.AddPolicy(RateLimitPolicyNames.DestructiveIdentity, httpContext =>
                BuildPartition(httpContext, rateLimitingOptions.DestructiveIdentity));
            options.AddPolicy(RateLimitPolicyNames.DestructiveEnrollment, httpContext =>
                BuildPartition(httpContext, rateLimitingOptions.DestructiveEnrollment));
            options.AddPolicy(RateLimitPolicyNames.DeviceProof, httpContext =>
                BuildPartition(httpContext, rateLimitingOptions.DeviceProof));
            options.AddPolicy(RateLimitPolicyNames.DeviceLinkInit, httpContext =>
                BuildPartition(httpContext, rateLimitingOptions.DeviceLinkInit));
        });
    }

    private static void ConfigureForwardedHeaders(
        IServiceCollection services,
        ProxyOptions proxyOptions,
        IReadOnlyList<IPAddress> knownProxyAddresses)
    {
        services.Configure<ForwardedHeadersOptions>(options =>
        {
            options.ForwardedHeaders = ForwardedHeaders.XForwardedFor | ForwardedHeaders.XForwardedProto;
            options.ForwardLimit = proxyOptions.ForwardLimit;
            options.KnownProxies.Clear();

            foreach (var knownProxy in knownProxyAddresses)
            {
                options.KnownProxies.Add(knownProxy);
            }
        });
    }

    private static void ConfigureObservability(
        IServiceCollection services,
        TelemetryOptions telemetryOptions)
    {
        services.AddSingleton<PostQuantumCryptoReadiness>();
        services.AddHealthChecks()
            .AddCheck<DatabaseHealthCheck>("postgresql-ready")
            .AddCheck<PostQuantumCryptoHealthCheck>("ml-dsa-available");
        services.AddSingleton<OperationalMetrics>();
        services.AddSingleton<IAuthMetrics, AuthMetricsAdapter>();
        services.AddSingleton<ISharingMetrics, SharingMetricsAdapter>();
        services.AddOpenTelemetry()
            .ConfigureResource(resource =>
            {
                resource.AddService(
                    telemetryOptions.ServiceName,
                    serviceVersion: string.IsNullOrWhiteSpace(telemetryOptions.ServiceVersion)
                        ? null
                        : telemetryOptions.ServiceVersion);

                foreach (var (key, value) in telemetryOptions.ResourceAttributes)
                {
                    if (!string.IsNullOrWhiteSpace(key) && !string.IsNullOrWhiteSpace(value))
                    {
                        resource.AddAttributes(new[] { new KeyValuePair<string, object>(key, value) });
                    }
                }
            })
            .WithMetrics(metrics =>
            {
                metrics
                    .AddAspNetCoreInstrumentation()
                    .AddHttpClientInstrumentation()
                    .AddMeter(OperationalMetrics.MeterName);

                ConfigureOtlpExporter(metrics, telemetryOptions);
            })
            .WithTracing(tracing =>
            {
                tracing
                    .AddAspNetCoreInstrumentation()
                    .AddHttpClientInstrumentation();

                ConfigureOtlpExporter(tracing, telemetryOptions);
            });
    }

    private static void ConfigureRealtimeHost(IServiceCollection services)
    {
        services.AddSingleton<SessionBroadcaster>();
        services.AddSingleton<UserEventBroadcaster>();
        services.AddSingleton<RealtimeDeviceAccessEnforcementCore>();
        services.AddSingleton<
            IIdentityResetRealtimeEnforcer,
            IdentityResetRealtimeEnforcer>();
        services.AddSingleton<SessionReplaySender>();
        services.AddSingleton<RealtimeDeviceAuthorizationReader>();
        services.AddSingleton<RelayMessageProcessor>();
        services.AddScoped<SessionAccessOverrideRevoker>();
        services.AddScoped<SessionAccessDisconnector>();
        services.AddScoped<DiscoveryAudienceResolver>();
        services.AddScoped<SharedSurfaceEventPublisher>();
        services.AddSingleton<RealtimePersistenceRepairTracker>(services =>
            new RealtimePersistenceRepairTracker(
                services.GetRequiredService<TimeProvider>(),
                services.GetRequiredService<IHostApplicationLifetime>().StopApplication,
                services.GetRequiredService<ILogger<RealtimePersistenceRepairTracker>>()));
        services.AddSingleton<GracefulShutdownHostedService>();
        services.AddHostedService(sp => sp.GetRequiredService<GracefulShutdownHostedService>());
        services.AddSingleton<SessionLifecycleGate>();
        services.AddSingleton<HostHandshake>();
        services.AddSingleton<HostTeardown>();
        services.AddSingleton<SessionEndCoordinator>();
        services.AddSingleton<ISessionEndAuthority>(
            services => services.GetRequiredService<SessionEndCoordinator>());
        services.AddHostedService<IdentityResetEnforcementHostedService>();
        services.AddHostedService<DeviceRevocationEnforcementHostedService>();
        services.AddHostedService<AccessOverrideExpiryEnforcementHostedService>();
        services.AddSingleton<HostMessageProcessor>();
        services.AddSingleton<HostSessionPump>();
        services.AddSingleton<ParticipantHandshake>();
        services.AddSingleton<ParticipantSessionPump>();
        services.AddSingleton<ParticipantTeardown>();
        services.AddSingleton<HostWebSocketHandler>();
        services.AddSingleton<ParticipantWebSocketHandler>();
        services.AddSingleton<UserEventsWebSocketHandler>();


        services.AddHostedService<RealtimeCounterResetHostedService>();
        services.AddHostedService<HeartbeatTimeoutHostedService>();
        services.AddHostedService<CleanupHostedService>();
    }

    private static RateLimitPartition<string> BuildPartition(
        HttpContext httpContext,
        FixedWindowPolicyOptions options)
    {
        var key =
            httpContext.User.FindFirstValue(AuthClaimTypes.UserId) ??
            httpContext.Connection.RemoteIpAddress?.ToString() ??
            "unknown";

        return RateLimitPartition.GetFixedWindowLimiter(key, _ => new FixedWindowRateLimiterOptions
        {
            PermitLimit = options.PermitLimit,
            Window = TimeSpan.FromSeconds(options.WindowSeconds),
            QueueLimit = options.QueueLimit,
            QueueProcessingOrder = QueueProcessingOrder.OldestFirst,
            AutoReplenishment = true,
        });
    }

    private static void ValidateTelemetryOptions(TelemetryOptions telemetryOptions)
    {
        if (string.IsNullOrWhiteSpace(telemetryOptions.ServiceName))
        {
            throw new InvalidOperationException("Telemetry:ServiceName must be configured.");
        }

        if (string.IsNullOrWhiteSpace(telemetryOptions.OtlpEndpoint))
        {
            return;
        }

        if (!Uri.TryCreate(telemetryOptions.OtlpEndpoint, UriKind.Absolute, out _))
        {
            throw new InvalidOperationException("Telemetry:OtlpEndpoint must be an absolute URI.");
        }

        if (!string.Equals(telemetryOptions.Protocol, "grpc", StringComparison.OrdinalIgnoreCase) &&
            !string.Equals(telemetryOptions.Protocol, "http/protobuf", StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException(
                "Telemetry:Protocol must be either 'grpc' or 'http/protobuf'.");
        }
    }

    private static IReadOnlyList<IPAddress> ParseKnownProxyAddresses(ProxyOptions proxyOptions)
    {
        if (proxyOptions.ForwardLimit < 1)
        {
            throw new InvalidOperationException("Proxy:ForwardLimit must be at least 1.");
        }

        var knownProxies = new List<IPAddress>();
        foreach (var knownProxy in proxyOptions.KnownProxies.Where(proxy => !string.IsNullOrWhiteSpace(proxy)))
        {
            if (!IPAddress.TryParse(knownProxy, out var ipAddress))
            {
                throw new InvalidOperationException(
                    $"Proxy:KnownProxies contains an invalid IP address '{knownProxy}'.");
            }

            knownProxies.Add(ipAddress);
        }

        return knownProxies;
    }

    private static void ConfigureOtlpExporter(MeterProviderBuilder metrics, TelemetryOptions telemetryOptions)
    {
        if (string.IsNullOrWhiteSpace(telemetryOptions.OtlpEndpoint))
        {
            return;
        }

        metrics.AddOtlpExporter(options =>
        {
            options.Endpoint = new Uri(telemetryOptions.OtlpEndpoint);
            options.Protocol = ParseOtlpProtocol(telemetryOptions.Protocol);
            options.Compression = telemetryOptions.UseGzipCompression
                ? OtlpExportCompression.GZip
                : OtlpExportCompression.None;
        });
    }

    private static void ConfigureOtlpExporter(TracerProviderBuilder tracing, TelemetryOptions telemetryOptions)
    {
        if (string.IsNullOrWhiteSpace(telemetryOptions.OtlpEndpoint))
        {
            return;
        }

        tracing.AddOtlpExporter(options =>
        {
            options.Endpoint = new Uri(telemetryOptions.OtlpEndpoint);
            options.Protocol = ParseOtlpProtocol(telemetryOptions.Protocol);
            options.Compression = telemetryOptions.UseGzipCompression
                ? OtlpExportCompression.GZip
                : OtlpExportCompression.None;
        });
    }

    private static OtlpExportProtocol ParseOtlpProtocol(string protocol)
    {
        return string.Equals(protocol, "http/protobuf", StringComparison.OrdinalIgnoreCase)
            ? OtlpExportProtocol.HttpProtobuf
            : OtlpExportProtocol.Grpc;
    }
}
