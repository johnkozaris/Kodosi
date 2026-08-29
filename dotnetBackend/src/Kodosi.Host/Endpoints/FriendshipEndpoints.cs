using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;
using Kodosi.Host.Realtime;

using Kodosi.Host.Middleware;

namespace Kodosi.Host.Endpoints;

public static class FriendshipEndpoints
{
    public static RouteGroupBuilder MapFriendshipEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/friends")
            .WithTags("Friends");

        group.MapGet("/", async (
            UserService userService,
            ICurrentUser currentUser) =>
        {
            var friends = await userService.GetFriendsAsync(currentUser.UserId);
            return Results.Ok(friends.Select(u =>
                new UserSummaryResponse(u.Id.Value, u.Handle, u.DisplayName, u.AvatarUrl)));
        });

        group.MapGet("/requests", async (
            UserService userService,
            ICurrentUser currentUser) =>
        {
            var requests = await userService.GetFriendRequestsAsync(currentUser.UserId);
            return Results.Ok(requests);
        });

        group.MapPost("/requests", async (
            FriendRequestHandleRequest request,
            FriendshipWorkflowService friendships,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            HttpContext httpContext,
            CancellationToken ct) =>
        {
            try
            {
                var targetUserId = await friendships.SendRequestAsync(
                    currentUser.UserId,
                    request.Username,
                    httpContext.ToRequestAuditContext(),
                    ct);
                sharedSurfaceEvents.PublishFriendRequestsChanged(currentUser.UserId, targetUserId);
                return Results.NoContent();
            }
            catch (NotFoundException)
            {

                return Results.NoContent();
            }
            catch (DbUpdateException ex)
                when (EndpointPostgresExceptionHelpers.IsFriendshipConflict(ex))
            {
                throw new FriendRequestAlreadyExistsException();
            }
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<FriendRequestHandleRequest>>();

        group.MapPost("/requests/accept", async (
            FriendRequestHandleRequest request,
            FriendshipWorkflowService friendships,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            HttpContext httpContext,
            CancellationToken ct) =>
        {
            var friendUserId = await friendships.AcceptRequestAsync(
                currentUser.UserId,
                request.Username,
                httpContext.ToRequestAuditContext(),
                ct);
            sharedSurfaceEvents.PublishFriendshipAccepted(currentUser.UserId, friendUserId);
            return Results.Ok();
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<FriendRequestHandleRequest>>();

        group.MapPost("/requests/reject", async (
            FriendRequestHandleRequest request,
            FriendshipWorkflowService friendships,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            HttpContext httpContext,
            CancellationToken ct) =>
        {
            var otherUserId = await friendships.RejectRequestAsync(
                currentUser.UserId,
                request.Username,
                httpContext.ToRequestAuditContext(),
                ct);
            sharedSurfaceEvents.PublishFriendRequestsChanged(currentUser.UserId, otherUserId);
            return Results.NoContent();
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<FriendRequestHandleRequest>>();



        group.MapDelete("/requests/outgoing/{handle}", async (
            string handle,
            FriendshipWorkflowService friendships,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            HttpContext httpContext,
            CancellationToken ct) =>
        {
            var otherUserId = await friendships.CancelOutgoingRequestAsync(
                currentUser.UserId,
                handle,
                httpContext.ToRequestAuditContext(),
                ct);
            sharedSurfaceEvents.PublishFriendRequestsChanged(currentUser.UserId, otherUserId);
            return Results.NoContent();
        });

        group.MapDelete("/{handle}", async (
            string handle,
            FriendshipWorkflowService friendships,
            SessionAccessDisconnector accessDisconnector,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            HttpContext httpContext,
            CancellationToken ct) =>
        {
            await using var removal = await friendships.RemoveFriendshipAsync(
                currentUser.UserId,
                handle,
                httpContext.ToRequestAuditContext(),
                ct);

            await accessDisconnector.DisconnectFormerFriendAsync(
                currentUser.UserId,
                removal.OtherUserId,
                removal.OwnerSessions,
                CancellationToken.None,
                lifecycleAlreadyHeld: true);
            await accessDisconnector.DisconnectFormerFriendAsync(
                removal.OtherUserId,
                currentUser.UserId,
                removal.FormerFriendSessions,
                CancellationToken.None,
                lifecycleAlreadyHeld: true);
            sharedSurfaceEvents.PublishFriendshipRemoved(currentUser.UserId, removal.OtherUserId);
            return Results.NoContent();
        });

        return group;
    }

    private static RequestAuditContext ToRequestAuditContext(this HttpContext httpContext)
    {
        var (ip, ua) = httpContext.ExtractAuditContext();
        return new RequestAuditContext(ip, ua);
    }
}
