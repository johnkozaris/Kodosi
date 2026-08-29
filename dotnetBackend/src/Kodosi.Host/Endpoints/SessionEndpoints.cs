using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;
using Kodosi.Host.Realtime;

using Kodosi.Host.Middleware;

namespace Kodosi.Host.Endpoints;

public static class SessionEndpoints
{
    public static RouteGroupBuilder MapSessionEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/sessions")
            .WithTags("Sessions");

        group.MapPost("/", async (
            CreateSessionRequest request,
            SessionCreator creator,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var created = await creator.CreateOwnedAsync(currentUser.UserId, request, ct);
            await sharedSurfaceEvents.PublishSessionChangeAsync(null, created.SharingState, ct);
            return Results.Created($"/api/sessions/{created.Response.Id}", created.Response);
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<CreateSessionRequest>>();


        group.MapGet("/mine", async (
            SessionReader reader,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var sessions = await reader.GetMySessionsAsync(currentUser.UserId, ct);
            return Results.Ok(sessions);
        });

        group.MapGet("/{id:guid}/creation-receipts/{createIdempotencyKey}", async (
            Guid id,
            string createIdempotencyKey,
            SessionCreationReceiptLookup lookup,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var receipt = await lookup.GetOwnedAsync(
                SessionId.From(id),
                ParseUuidV7(createIdempotencyKey, nameof(createIdempotencyKey)),
                currentUser.UserId,
                ct);
            return Results.Ok(receipt);
        });

        group.MapGet("/{id:guid}", async (
            Guid id,
            SessionReader reader,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var result = await reader.GetByIdAsync(SessionId.From(id), currentUser.UserId, ct);
            return Results.Ok(result);
        });

        group.MapPatch("/{id:guid}", async (
            Guid id,
            UpdateSessionRequest request,
            SessionUpdater updater,
            SessionAccessDisconnector accessDisconnector,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            HttpContext httpContext,
            CancellationToken ct) =>
        {
            var sessionId = SessionId.From(id);

            var (clientIp, userAgent) = httpContext.ExtractAuditContext();
            await using var updated = await updater.UpdateOwnedAsync(
                sessionId, currentUser.UserId, request, ct,
                clientIp: clientIp, userAgent: userAgent);

            if (request.Scope.HasValue || request.DefaultAccess.HasValue)
            {
                await accessDisconnector.DisconnectAfterScopeChangeAsync(
                    sessionId,
                    currentUser.UserId,
                    updated.Response.StartedAt,
                    CancellationToken.None,
                    lifecycleAlreadyHeld: true);
            }

            await sharedSurfaceEvents.PublishSessionChangeAsync(
                updated.Before,
                updated.After,
                CancellationToken.None);
            return Results.Ok(updated.Response);
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<UpdateSessionRequest>>();

        group.MapDelete("/{id:guid}", async (
            Guid id,
            Guid incarnationId,
            SessionEndCoordinator coordinator,
            ICurrentUser currentUser,
            HttpContext httpContext,
            CancellationToken ct) =>
            await EndOwnedIdempotentlyAsync(
                id,
                incarnationId,
                RequiredUuidV7Header(httpContext, "Idempotency-Key"),
                RequiredUuidV7Header(httpContext, "Kodosi-Attempt-Id"),
                coordinator,
                currentUser.UserId,
                ct));

        return group;
    }

    internal static Guid ParseUuidV7(string raw, string parameterName)
    {
        if (!Guid.TryParse(raw, out var value) || value.Version != 7)
        {
            throw new InvalidParameterException(parameterName, "must be a UUIDv7");
        }
        return value;
    }

    internal static async Task<IResult> EndOwnedIdempotentlyAsync(
        Guid id,
        Guid incarnationId,
        Guid mutationId,
        Guid attemptId,
        SessionEndCoordinator coordinator,
        UserId ownerUserId,
        CancellationToken ct)
    {
        if (incarnationId == Guid.Empty)
        {
            throw new MissingRequiredParameterException("incarnationId");
        }
        if (mutationId == Guid.Empty)
        {
            throw new MissingRequiredParameterException("Idempotency-Key");
        }
        if (attemptId == Guid.Empty)
        {
            throw new MissingRequiredParameterException("Kodosi-Attempt-Id");
        }
        await coordinator.EndOwnedIdempotentlyAsync(
            SessionId.From(id),
            ownerUserId,
            incarnationId,
            mutationId,
            attemptId,
            CloseReason.HostStopped,
            ct);
        return Results.NoContent();
    }

    internal static Guid RequiredUuidV7Header(HttpContext context, string name)
    {
        var raw = context.Request.Headers[name].ToString();
        if (string.IsNullOrWhiteSpace(raw))
        {
            throw new MissingRequiredParameterException(name);
        }
        if (!Guid.TryParse(raw, out var value) || value.Version != 7)
        {
            throw new InvalidParameterException(name, "must be a UUIDv7");
        }
        return value;
    }
}
