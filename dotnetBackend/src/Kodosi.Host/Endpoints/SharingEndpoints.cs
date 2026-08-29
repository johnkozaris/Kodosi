using System.ComponentModel.DataAnnotations;
using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;
using Kodosi.Host.Realtime;

using Kodosi.Host.Middleware;

namespace Kodosi.Host.Endpoints;

public static class SharingEndpoints
{
    public static RouteGroupBuilder MapSharingEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/sessions/{id:guid}/access")
            .WithTags("Sharing");

        group.MapGet("/", async (
            Guid id,
            Guid expectedIncarnationId,
            SessionAccessQueryService accessQuery,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var grants = await accessQuery.GetActiveGrantsAsync(
                SessionId.From(id),
                expectedIncarnationId,
                currentUser.UserId,
                ct);
            return Results.Ok(grants);
        });

        group.MapPost("/", async (
            Guid id,
            GrantAccessRequest request,
            SharingService sharingService,
            SessionAccessDisconnector accessDisconnector,
            ICurrentUser currentUser,
            HttpContext httpContext,
            CancellationToken ct) =>
        {
            var sessionId = SessionId.From(id);
            var actorUserId = UserId.From(request.ActorUserId);
            var (ip, ua) = httpContext.ExtractAuditContext();
            await using var mutation = await sharingService.GrantAccessAsync(
                sessionId,
                request.ExpectedIncarnationId,
                request.MutationId,
                actorUserId,
                request.AccessLevel,
                currentUser.UserId,
                request.ExpiresAt,
                ct,
                ip,
                ua);
            await accessDisconnector.RefreshGrantedAccessAsync(
                mutation,
                CancellationToken.None,
                lifecycleAlreadyHeld: true);

            return Results.NoContent();
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<GrantAccessRequest>>();

        group.MapDelete("/{actorUserId:guid}", async (
            Guid id,
            Guid actorUserId,
            Guid expectedIncarnationId,
            Guid mutationId,
            SharingService sharingService,
            SessionAccessDisconnector accessDisconnector,
            ICurrentUser currentUser,
            HttpContext httpContext,
            CancellationToken ct) =>
        {
            if (expectedIncarnationId == Guid.Empty)
            {
                throw new MissingRequiredParameterException("expectedIncarnationId");
            }
            if (mutationId == Guid.Empty || mutationId.Version != 7)
            {
                throw new MissingRequiredParameterException("mutationId");
            }
            var sessionId = SessionId.From(id);
            var revokedUserId = UserId.From(actorUserId);

            var (ip, ua) = httpContext.ExtractAuditContext();
            await using var mutation = await sharingService.RevokeAccessAsync(
                sessionId,
                expectedIncarnationId,
                mutationId,
                revokedUserId,
                currentUser.UserId,
                ct,
                ip,
                ua);
            await accessDisconnector.DisconnectRevokedAccessAsync(
                mutation,
                CancellationToken.None,
                lifecycleAlreadyHeld: true);

            return Results.NoContent();
        });

        group.MapGet("/mutations/{mutationId:guid}", async (
            Guid id,
            Guid mutationId,
            Guid expectedIncarnationId,
            SessionAccessMutationReceiptLookup lookup,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            if (mutationId == Guid.Empty || mutationId.Version != 7)
            {
                return Results.NotFound();
            }
            var receipt = await lookup.FindAsync(
                currentUser.UserId,
                SessionId.From(id),
                expectedIncarnationId,
                mutationId,
                ct);
            return receipt is null ? Results.NotFound() : Results.Ok(receipt);
        });

        group.MapDelete("/me", async (
            Guid id,
            Guid expectedIncarnationId,
            Guid mutationId,
            SessionViewerDismissalService dismissalService,
            SessionAccessDisconnector accessDisconnector,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            if (expectedIncarnationId == Guid.Empty)
            {
                throw new MissingRequiredParameterException("expectedIncarnationId");
            }
            if (mutationId == Guid.Empty || mutationId.Version != 7)
            {
                throw new MissingRequiredParameterException("mutationId");
            }
            var sessionId = SessionId.From(id);
            var viewerUserId = currentUser.UserId;
            var session = await dismissalService.DismissAsync(
                sessionId,
                expectedIncarnationId,
                mutationId,
                viewerUserId,
                ct);

            if (session.ShouldProject)
            {
                await accessDisconnector.DisconnectDismissedViewerAsync(
                    sessionId,
                    session.IncarnationId,
                    viewerUserId,
                    session.StartedAt,
                    CancellationToken.None);
                sharedSurfaceEvents.PublishSessionDismissed(
                    viewerUserId,
                    session.DiscoveryState);
            }
            return Results.NoContent();
        });

        return group;
    }

}

public sealed record GrantAccessRequest(
    [property: NotEmptyGuid]
    Guid ExpectedIncarnationId,
    [property: UuidV7]
    Guid MutationId,
    [property: NotEmptyGuid]
    Guid ActorUserId,
    [property: EnumDataType(typeof(AccessLevel))]
    AccessLevel AccessLevel,
    DateTimeOffset ExpiresAt);
