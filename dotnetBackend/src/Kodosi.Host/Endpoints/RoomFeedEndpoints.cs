using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;

namespace Kodosi.Host.Endpoints;

public static class RoomFeedEndpoints
{
    public static RouteGroupBuilder MapRoomFeedEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/feed")
            .WithTags("Feed");

        group.MapGet("/room/{roomId:guid}", async (
            Guid roomId,
            string? cursor,
            int? limit,
            string? toolKind,
            string? sort,
            DateTimeOffset? since,
            RoomSessionFeedService roomFeed,
            ICurrentUser currentUser,
            CancellationToken ct) =>
            GetRoomFeedAsync(
                roomId,
                cursor,
                limit,
                toolKind,
                sort,
                since,
                roomFeed,
                currentUser.UserId,
                ct))
        .ProducesProblem(StatusCodes.Status400BadRequest);

        return group;
    }

    internal static async Task<IResult> GetRoomFeedAsync(
        Guid roomId,
        string? cursor,
        int? limit,
        string? toolKind,
        string? sort,
        DateTimeOffset? since,
        RoomSessionFeedService roomFeed,
        UserId currentUserId,
        CancellationToken ct)
    {
        if (!FeedQueryParsers.TryParseToolKind(toolKind, out var toolKindFilter))
        {
            return Results.BadRequest();
        }

        var sessions = await roomFeed.GetRoomFeedAsync(
            RoomId.From(roomId),
            currentUserId,
            cursor,
            limit ?? 20,
            toolKindFilter,
            sort,
            since,
            ct);
        return Results.Ok(sessions);
    }
}
