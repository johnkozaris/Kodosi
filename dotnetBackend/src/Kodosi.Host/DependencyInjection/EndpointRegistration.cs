using Microsoft.AspNetCore.Diagnostics.HealthChecks;
using Microsoft.Extensions.Diagnostics.HealthChecks;
using Kodosi.Application;
using Kodosi.Host.Configuration;
using Kodosi.Host.Endpoints;
using Kodosi.Host.Observability;

namespace Kodosi.Host.DependencyInjection;

public static class EndpointRegistration
{
    public static WebApplication MapAllEndpoints(this WebApplication app)
    {
        app.MapSessionEndpoints().AsAuthenticatedApiGroup();
        app.MapRoomFeedEndpoints().AsAuthenticatedApiGroup();
        app.MapSharingEndpoints().AsAuthenticatedApiGroup();
        app.MapRoomCrudEndpoints().AsAuthenticatedApiGroup();
        app.MapRoomMemberEndpoints().AsAuthenticatedApiGroup();
        app.MapRoomInvitationEndpoints().AsAuthenticatedApiGroup();
        app.MapRoomChatEndpoints().AsAuthenticatedApiGroup();
        app.MapRoomTaskEndpoints().AsAuthenticatedApiGroup();
        app.MapRoomMutationReceiptEndpoints().AsAuthenticatedApiGroup();
        app.MapFriendshipEndpoints().AsAuthenticatedApiGroup();
        app.MapUserEndpoints().AsAuthenticatedApiGroup();
        app.MapDeviceEnrollmentEndpoints().AsAuthenticatedApiGroup();
        app.MapDeviceLookupEndpoints().AsAuthenticatedApiGroup();
        app.MapDeviceListEndpoints().AsAuthenticatedApiGroup();
        app.MapIdentityResetEndpoints().AsAuthenticatedApiGroup();
        app.MapDeviceLinkEndpoints().AsAuthenticatedApiGroup();
        app.MapSessionKeyEndpoints().AsAuthenticatedApiGroup();
        app.MapSemanticReceiptEndpoints().AsAuthenticatedApiGroup();

        app.MapWebSocketEndpoints();

        app.MapGet("/health/live", () => Results.Ok(BuildHealthStatus("healthy")))
            .AllowAnonymous()
            .WithTags("Health");


        app.MapGet("/health/ready", async (
                GracefulShutdownHostedService shutdown,
                HealthCheckService healthChecks,
                CancellationToken ct) =>
            {
                if (shutdown.IsDraining)
                {
                    return Results.StatusCode(StatusCodes.Status503ServiceUnavailable);
                }

                var report = await healthChecks.CheckHealthAsync(ct);
                return report.Status == HealthStatus.Healthy
                    ? Results.Ok(BuildHealthStatus("ready"))
                    : Results.StatusCode(StatusCodes.Status503ServiceUnavailable);
            })
            .AllowAnonymous()
            .WithTags("Health");
        app.MapHealthChecks("/health", new HealthCheckOptions())
            .RequireAuthorization()
            .WithTags("Health");
        app.MapGet("/metrics", (OperationalMetrics metrics) => Results.Ok(metrics.Snapshot()))
            .AsAuthenticatedApiGroup()
            .WithTags("Operations");

        return app;
    }

    internal static HealthStatusResponse BuildHealthStatus(string status) =>
        new(
            status,
            BackendContractVersions.Api,
            BackendContractVersions.Auth);



    private static T AsAuthenticatedApiGroup<T>(this T builder)
        where T : IEndpointConventionBuilder
    {
        builder.RequireAuthorization();
        builder.RequireRateLimiting(RateLimitPolicyNames.Api);
        return builder;
    }
}
