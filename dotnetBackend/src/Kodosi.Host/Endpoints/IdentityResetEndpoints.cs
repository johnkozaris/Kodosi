using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;
using Kodosi.Host.Configuration;
using Kodosi.Host.Middleware;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Microsoft.AspNetCore.Mvc;

namespace Kodosi.Host.Endpoints;

public static class IdentityResetEndpoints
{
    private sealed record IdentityResetRequest(
        Guid? ChallengeId,
        string? SignerDeviceId,
        string? PopSignature);

    public static RouteGroupBuilder MapIdentityResetEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api")
            .WithTags("Devices");

        group.MapDelete("/me/identity", async (
            [FromBody] IdentityResetRequest? request,
            IdentityLifecycleService identityLifecycle,
            UserEventBroadcaster broadcaster,
            OperationalMetrics metrics,
            ICurrentUser currentUser,
            HttpContext httpContext,
            ILoggerFactory loggerFactory,
            CancellationToken ct) =>
        {
            var logger = loggerFactory.CreateLogger("Kodosi.Host.Endpoints.IdentityResetEndpoints");

            logger.LogInformation(
                "identity_reset start user={UserId}",
                currentUser.UserId.Value);

            var popPayload = request is null
                || request.ChallengeId is null
                || string.IsNullOrWhiteSpace(request.SignerDeviceId)
                || string.IsNullOrWhiteSpace(request.PopSignature)
                ? null
                : new IdentityResetPopPayload(
                    request.ChallengeId.Value,
                    request.SignerDeviceId!,
                    request.PopSignature!);

            var (ip, ua) = httpContext.ExtractAuditContext();
            var auditContext = new IdentityResetAuditContext(ClientIp: ip, UserAgent: ua);

            var result = await identityLifecycle.ResetIdentityAsync(
                currentUser.UserId, popPayload, auditContext, ct);

            try
            {
                broadcaster.PublishIdentityLifecycleChanged(
                    DiscoveryAudience.ForUsers(result.AudienceUserIds),
                    currentUser.UserId,
                    result.IdentityRevision,
                    incarnationId: null,
                    generation: 0);
            }
            catch (Exception ex)
            {
                logger.LogError(
                    ex,
                    "identity_reset audience publication failed after committed enforcement user={UserId}",
                    currentUser.UserId.Value);
            }

            metrics.RecordIdentityReset();
            logger.LogInformation(
                "identity_reset complete user={UserId} sessionsEnded={SessionsEnded} "
                + "devicesRemoved={DevicesRemoved} listsRemoved={ListsRemoved}",
                currentUser.UserId.Value,
                result.EndedSessionIds.Count,
                result.DevicesRemoved,
                result.ListsRemoved);

            return Results.Ok(new IdentityResetResponse(
                result.DevicesRemoved,
                result.ListsRemoved,
                result.EndedSessionIds.Count,
                result.SessionsAttempted));
        })
        .RequireRateLimiting(RateLimitPolicyNames.DestructiveIdentity);

        return group;
    }

    internal sealed record IdentityResetResponse(
        int DevicesRemoved,
        int ListsRemoved,
        int SessionsEnded,
        int SessionsAttempted);
}
