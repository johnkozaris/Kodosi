using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;
using Kodosi.Host.Realtime;
using Microsoft.AspNetCore.Mvc;

namespace Kodosi.Host.Endpoints;

public static class RoomChatEndpoints
{
    public static RouteGroupBuilder MapRoomChatEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/rooms")
            .WithTags("Rooms");

        group.MapPost("/{roomId:guid}/chat", async (
            Guid roomId,
            PostRoomChatRequest request,
            RoomChatService chatService,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var resolvedRoomId = RoomId.From(roomId);
            var message = await chatService.PostAsync(
                resolvedRoomId,
                request.MessageId,
                currentUser.UserId,
                request.AuthorSessionId,
                request.AuthorKind,
                request.Body,
                request.RecipientSessionIds,
                request.RecipientUserIds,
                ct);
            await sharedSurfaceEvents.PublishRoomChatMessageAsync(
                resolvedRoomId,
                currentUser.UserId,
                CancellationToken.None);
            return Results.Created(
                $"/api/rooms/{roomId}/chat/{message.Id}",
                RoomResponseMappers.MapChat(message));
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<PostRoomChatRequest>>()
        .WithMetadata(new RequestSizeLimitAttribute(
            RoomRequestLimits.EncryptedContentBodyBytes));

        group.MapGet("/{roomId:guid}/chat/{messageId:guid}", async (
            Guid roomId,
            Guid messageId,
            RoomChatService chatService,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var message = await chatService.GetByIdAsync(
                RoomId.From(roomId),
                messageId,
                currentUser.UserId,
                ct);
            return message is null
                ? Results.NotFound()
                : Results.Ok(RoomResponseMappers.MapChat(message));
        });

        group.MapGet("/{roomId:guid}/chat", async (
            Guid roomId,
            long? since,
            long? before,
            int? limit,
            RoomChatService chatService,
            ICurrentUser currentUser,
            HttpContext http,
            CancellationToken ct) =>
        {
            if (since is not null && before is not null)
            {
                throw new PolicyViolationException("Chat reads accept either 'since' or 'before', not both.");
            }
            if (before is <= 0)
            {
                throw new PolicyViolationException("Chat 'before' must be positive.");
            }
            var normalizedLimit = NormalizeReadLimit(limit);
            var page = before is not null
                ? await chatService.ListTailAsync(
                    RoomId.From(roomId),
                    currentUser.UserId,
                    before,
                    normalizedLimit,
                    ct)
                : await chatService.ListAsync(
                    RoomId.From(roomId),
                    currentUser.UserId,
                    since ?? 0,
                    normalizedLimit,
                    ct);
            ApplyReadPageHeaders(http.Response, roomId, normalizedLimit, page);
            return Results.Ok(page.Items.Select(RoomResponseMappers.MapChat));
        });

        return group;
    }

    internal static int NormalizeReadLimit(int? limit) =>
        RoomChatReadPolicy.Normalize(limit);

    internal static void ApplyReadPageHeaders(
        HttpResponse response,
        Guid roomId,
        int limit,
        RoomChatReadPage page)
    {
        response.Headers["Kodosi-Has-More"] = page.HasMore ? "true" : "false";
        if (page.NextBefore is { } nextBefore)
        {
            response.Headers["Kodosi-Next-Before"] =
                nextBefore.ToString(System.Globalization.CultureInfo.InvariantCulture);
            response.Headers.Link =
                $"</api/rooms/{roomId:D}/chat?before={nextBefore}&limit={limit}>; rel=\"next\"";
            return;
        }
        if (page.NextSince is not { } nextSince)
        {
            return;
        }

        response.Headers["Kodosi-Next-Since"] =
            nextSince.ToString(System.Globalization.CultureInfo.InvariantCulture);
        response.Headers.Link =
            $"</api/rooms/{roomId:D}/chat?since={nextSince}&limit={limit}>; rel=\"next\"";
    }
}
