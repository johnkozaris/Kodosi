using Microsoft.AspNetCore.Mvc;
using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;
using Kodosi.Host.Realtime;

using Kodosi.Host.Middleware;

namespace Kodosi.Host.Endpoints;

public static class RoomMemberEndpoints
{
    public static RouteGroupBuilder MapRoomMemberEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/rooms")
            .WithTags("Rooms");

        group.MapGet("/{roomId:guid}/members", async (
            Guid roomId,
            RoomService roomService,
            ICurrentUser currentUser) =>
        {
            var members = await roomService.GetMembersAsync(
                RoomId.From(roomId),
                currentUser.UserId);
            return members is null ? Results.NotFound() : Results.Ok(members);
        });

        group.MapDelete("/{roomId:guid}/members/{userId:guid}", async (
            Guid roomId,
            Guid userId,
            [FromBody] ReplaceRoomRosterRequest request,
            RoomMembershipWorkflowService membershipWorkflow,
            SessionAccessDisconnector accessDisconnector,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            HttpContext httpContext,
            CancellationToken ct) =>
        {
            var resolvedRoomId = RoomId.From(roomId);
            var removedUserId = UserId.From(userId);

            await using var removal = await membershipWorkflow.RemoveMemberIdempotentlyAsync(
                request.RequestId,
                resolvedRoomId,
                currentUser.UserId,
                removedUserId,
                request.RosterGeneration,
                Convert.FromBase64String(request.RosterBody),
                Convert.FromBase64String(request.RosterSignature),
                request.RosterSignerDeviceId,
                httpContext.ToRequestAuditContext(),
                ct);



            await accessDisconnector.DisconnectRemovedRoomMemberAsync(
                removal.RemovedUserId,
                removal.AffectedSessions,
                CancellationToken.None,
                lifecycleAlreadyHeld: true);
            await sharedSurfaceEvents.PublishRoomMemberRemovedAsync(
                resolvedRoomId, removal.RemovedUserId, CancellationToken.None);
            return Results.Ok(RoomResponseMappers.MapMutation(
                request.RequestId,
                "removeMember",
                removal.Receipt!));
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<ReplaceRoomRosterRequest>>()
        .WithMetadata(new RequestSizeLimitAttribute(
            RoomRequestLimits.SingleRosterBodyBytes))
        .Produces<RoomMutationResponse>();

        return group;
    }

    private static RequestAuditContext ToRequestAuditContext(this HttpContext httpContext)
    {
        var (ip, ua) = httpContext.ExtractAuditContext();
        return new RequestAuditContext(ip, ua);
    }
}
