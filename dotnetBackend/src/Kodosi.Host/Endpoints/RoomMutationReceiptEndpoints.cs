using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;

namespace Kodosi.Host.Endpoints;

public static class RoomMutationReceiptEndpoints
{
    public static RouteGroupBuilder MapRoomMutationReceiptEndpoints(
        this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/rooms/mutations")
            .WithTags("Rooms");

        group.MapGet("/{operation}/{requestId}", async (
            string operation,
            string requestId,
            RoomMutationReceiptLookup lookup,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var parsedRequestId = SessionEndpoints.ParseUuidV7(
                requestId,
                nameof(requestId));
            var receipt = await lookup.FindAsync(
                currentUser.UserId,
                RoomMutationReceiptLookup.ParseWireOperation(operation),
                parsedRequestId,
                ct);
            return receipt is null
                ? Results.NotFound()
                : Results.Ok(RoomResponseMappers.MapMutationReceipt(receipt));
        })
        .Produces<RoomMutationReceiptResponse>()
        .Produces(StatusCodes.Status404NotFound);

        return group;
    }
}
