using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;
using Kodosi.Host.Realtime;

using Kodosi.Host.Middleware;
using Microsoft.AspNetCore.Mvc;

namespace Kodosi.Host.Endpoints;

public static class RoomInvitationEndpoints
{
    public static RouteGroupBuilder MapRoomInvitationEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/rooms")
            .WithTags("Rooms");

        group.MapPost("/{roomId:guid}/invitations", async (
            Guid roomId,
            InviteRoomMemberRequest request,
            RoomInvitationService invitationService,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var invitation = await invitationService.InviteAsync(
                request.InvitationId,
                RoomId.From(roomId),
                currentUser.UserId,
                UserId.From(request.InviteeUserId),
                Convert.FromBase64String(request.ProposalBody),
                Convert.FromBase64String(request.ProposalSignature),
                request.ProposalSignerDeviceId,
                request.ProposedRosterGeneration,
                Convert.FromBase64String(request.ProposedRosterBody),
                Convert.FromBase64String(request.ProposedRosterSignature),
                request.ProposedRosterSignerDeviceId,
                ct);
            sharedSurfaceEvents.PublishRoomInvitationsChanged(
                currentUser.UserId, invitation.InviteeUserId);
            var response = RoomResponseMappers.MapInvitation(
                await invitationService.GetContextAsync(invitation, ct));
            return Results.Created(
                $"/api/rooms/invitations/{invitation.Id}",
                response);
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<InviteRoomMemberRequest>>()
        .WithMetadata(new RequestSizeLimitAttribute(
            RoomRequestLimits.InvitationWithRosterBodyBytes))
        .Produces<RoomInvitationResponse>(StatusCodes.Status201Created);



        group.MapGet("/invitations/incoming", async (
            RoomInvitationService invitationService,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var entries = await invitationService.GetIncomingAsync(currentUser.UserId, ct);
            return Results.Ok(entries.Select(RoomResponseMappers.MapInvitation));
        });

        group.MapGet("/invitations/outgoing", async (
            RoomInvitationService invitationService,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var entries = await invitationService.GetOutgoingAsync(currentUser.UserId, ct);
            return Results.Ok(entries.Select(RoomResponseMappers.MapInvitation));
        });

        group.MapPost("/invitations/{invitationId:guid}/accept", async (
            Guid invitationId,
            RoomInvitationDecisionRequest request,
            RoomInvitationService invitationService,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            HttpContext httpContext,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var (ip, ua) = httpContext.ExtractAuditContext();
            var mutation = await invitationService.AcceptIdempotentlyAsync(
                request.RequestId,
                invitationId,
                currentUser.UserId,
                Convert.FromBase64String(request.DecisionBody),
                Convert.FromBase64String(request.DecisionSignature),
                request.DecisionSignerDeviceId,
                new RequestAuditContext(ip, ua),
                ct);
            await sharedSurfaceEvents.PublishRoomMemberAddedAsync(
                mutation.Invitation.RoomId,
                mutation.Invitation.InviteeUserId,
                CancellationToken.None);
            sharedSurfaceEvents.PublishRoomInvitationsChanged(
                mutation.Invitation.InvitedByUserId,
                mutation.Invitation.InviteeUserId);
            return Results.Ok(RoomResponseMappers.MapMutation(
                request.RequestId,
                "acceptInvitation",
                mutation.Receipt));
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<RoomInvitationDecisionRequest>>()
        .Produces<RoomMutationResponse>();

        group.MapPost("/invitations/{invitationId:guid}/decline", async (
            Guid invitationId,
            RoomInvitationDecisionRequest request,
            RoomInvitationService invitationService,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var mutation = await invitationService.DeclineIdempotentlyAsync(
                request.RequestId,
                invitationId,
                currentUser.UserId,
                Convert.FromBase64String(request.DecisionBody),
                Convert.FromBase64String(request.DecisionSignature),
                request.DecisionSignerDeviceId,
                ct);
            sharedSurfaceEvents.PublishRoomInvitationsChanged(
                mutation.Invitation.InvitedByUserId,
                mutation.Invitation.InviteeUserId);
            return Results.Ok(RoomResponseMappers.MapMutation(
                request.RequestId,
                "declineInvitation",
                mutation.Receipt));
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<RoomInvitationDecisionRequest>>()
        .Produces<RoomMutationResponse>();

        group.MapDelete("/invitations/{invitationId:guid}", async (
            Guid invitationId,
            [FromBody] CancelRoomInvitationRequest request,
            RoomInvitationService invitationService,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var mutation = await invitationService.CancelIdempotentlyAsync(
                request.RequestId,
                invitationId,
                currentUser.UserId,
                ct);
            sharedSurfaceEvents.PublishRoomInvitationsChanged(
                mutation.Invitation.InvitedByUserId,
                mutation.Invitation.InviteeUserId);
            return Results.Ok(RoomResponseMappers.MapMutation(
                request.RequestId,
                "cancelInvitation",
                mutation.Receipt));
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<CancelRoomInvitationRequest>>()
        .Produces<RoomMutationResponse>();

        return group;
    }
}
