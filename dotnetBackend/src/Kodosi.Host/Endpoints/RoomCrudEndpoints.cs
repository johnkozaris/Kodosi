using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;
using Kodosi.Host.Realtime;
using Microsoft.AspNetCore.Mvc;

namespace Kodosi.Host.Endpoints;

public static class RoomCrudEndpoints
{
    public static RouteGroupBuilder MapRoomCrudEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/rooms")
            .WithTags("Rooms");

        group.MapPost("/", async (
            CreateRoomRequest request,
            RoomService roomService,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            try
            {
                var room = await roomService.CreateAsync(
                    RoomId.From(request.RoomId),
                    currentUser.UserId,
                    request.Name,
                    request.Slug,
                    request.RosterGeneration,
                    Convert.FromBase64String(request.RosterBody),
                    Convert.FromBase64String(request.RosterSignature),
                    request.RosterSignerDeviceId,
                    ct);
                sharedSurfaceEvents.PublishRoomCreated(currentUser.UserId);
                return Results.Created(
                    $"/api/rooms/{room.Id.Value}",
                    RoomResponseMappers.MapRoom(
                        room,
                        [],
                        [RoomRosterTransition.Create(room)]));
            }
            catch (DbUpdateException ex)
                when (EndpointPostgresExceptionHelpers.IsRoomSlugConflict(ex))
            {
                throw new RoomSlugConflictException(request.Slug);
            }
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<CreateRoomRequest>>()
        .WithMetadata(new RequestSizeLimitAttribute(
            RoomRequestLimits.SingleRosterBodyBytes));

        group.MapGet("/", async (
            string? cursor,
            int? limit,
            RoomCatalogQueryService query,
            ICurrentUser currentUser,
            HttpContext http,
            CancellationToken ct) =>
        {
            var page = await query.ListAsync(
                currentUser.UserId,
                cursor,
                limit,
                ct);
            http.Response.Headers["Kodosi-Has-More"] = page.HasMore ? "true" : "false";
            if (page.NextCursor is { } nextCursor)
            {
                http.Response.Headers["Kodosi-Next-Cursor"] = nextCursor;
            }
            return Results.Ok(page.Items.Select(room => RoomResponseMappers.MapRoom(
                room,
                [])));
        });

        group.MapGet("/{id:guid}", async (
            Guid id,
            RoomService roomService,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var room = await roomService.GetByIdAsync(RoomId.From(id), currentUser.UserId, ct);
            if (room is null) return Results.NotFound();

            return Results.Ok(RoomResponseMappers.MapRoom(room, []));
        });

        group.MapGet("/{id:guid}/admission-proofs", async (
            Guid id,
            Guid? afterUserId,
            int? limit,
            RoomCatalogQueryService query,
            ICurrentUser currentUser,
            HttpContext http,
            CancellationToken ct) =>
        {
            var page = await query.ListAdmissionProofsAsync(
                RoomId.From(id),
                currentUser.UserId,
                afterUserId,
                limit,
                ct);
            if (page is null)
            {
                return Results.NotFound();
            }
            http.Response.Headers["Kodosi-Has-More"] = page.HasMore ? "true" : "false";
            if (page.NextUserId is { } nextUserId)
            {
                http.Response.Headers["Kodosi-Next-User-Id"] = nextUserId.ToString("D");
            }
            return Results.Ok(page.Items.Select(RoomResponseMappers.MapAdmissionProof));
        });

        group.MapGet("/{id:guid}/roster-transitions", async (
            Guid id,
            long? afterGeneration,
            int? limit,
            RoomCatalogQueryService query,
            ICurrentUser currentUser,
            HttpContext http,
            CancellationToken ct) =>
        {
            var page = await query.ListTransitionsAsync(
                RoomId.From(id),
                currentUser.UserId,
                afterGeneration ?? 0,
                limit,
                ct);
            if (page is null)
            {
                return Results.NotFound();
            }
            http.Response.Headers["Kodosi-Has-More"] = page.HasMore ? "true" : "false";
            if (page.NextGeneration is { } nextGeneration)
            {
                http.Response.Headers["Kodosi-Next-Generation"] = nextGeneration.ToString(
                    System.Globalization.CultureInfo.InvariantCulture);
            }
            return Results.Ok(page.Items.Select(RoomResponseMappers.MapRosterTransition));
        });

        return group;
    }
}
