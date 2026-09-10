using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Domain;
using Kodosi.Host.Realtime;
using Microsoft.AspNetCore.Mvc;

namespace Kodosi.Host.Endpoints;

public static class RoomTaskEndpoints
{
    public static RouteGroupBuilder MapRoomTaskEndpoints(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/rooms")
            .WithTags("Rooms");

        group.MapPost("/{roomId:guid}/tasks", async (
            Guid roomId,
            CreateRoomTaskRequest request,
            RoomTaskService taskService,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var resolvedRoomId = RoomId.From(roomId);
            var task = await taskService.CreateAsync(
                resolvedRoomId,
                request.TaskId,
                currentUser.UserId,
                request.Title,
                request.Description,
                request.AssignedSessionId,
                request.AssignedSessionIncarnationId,
                request.DueAt,
                ct);
            await sharedSurfaceEvents.PublishRoomTaskChangedAsync(
                resolvedRoomId,
                currentUser.UserId,
                CancellationToken.None);
            return Results.Created(
                $"/api/rooms/{roomId}/tasks/{task.Id}",
                RoomResponseMappers.MapTask(task));
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<CreateRoomTaskRequest>>()
        .WithMetadata(new RequestSizeLimitAttribute(
            RoomRequestLimits.EncryptedContentBodyBytes));

        group.MapGet("/{roomId:guid}/tasks", async (
            Guid roomId,
            string? status,
            Guid? assignee,
            int? offset,
            int? limit,
            string? snapshot,
            RoomTaskService taskService,
            ICurrentUser currentUser,
            HttpContext http,
            CancellationToken ct) =>
        {


            RoomTaskStatus? statusFilter = null;
            if (!string.IsNullOrWhiteSpace(status))
            {
                if (!Enum.TryParse<RoomTaskStatus>(status, ignoreCase: true, out var parsed)
                    || !Enum.IsDefined(parsed))
                {
                    return Results.Problem(
                        detail: $"Unknown task status: '{status}'.",
                        statusCode: StatusCodes.Status400BadRequest);
                }
                statusFilter = parsed;
            }
            var page = await taskService.ListAsync(
                RoomId.From(roomId),
                currentUser.UserId,
                statusFilter,
                assignee,
                offset ?? 0,
                limit ?? 100,
                ct,
                snapshot);
            ApplyReadPageHeaders(http.Response, roomId, page, statusFilter, assignee, limit ?? 100);
            return Results.Ok(page.Items.Select(RoomResponseMappers.MapTask));
        });

        group.MapGet("/{roomId:guid}/tasks/{taskId:guid}", async (
            Guid roomId,
            Guid taskId,
            RoomTaskService taskService,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var task = await taskService.GetByIdAsync(
                RoomId.From(roomId),
                taskId,
                currentUser.UserId,
                ct);
            return task is null
                ? Results.NotFound()
                : Results.Ok(RoomResponseMappers.MapTask(task));
        });

        group.MapPost("/{roomId:guid}/tasks/{taskId:guid}/status", async (
            Guid roomId,
            Guid taskId,
            UpdateRoomTaskStatusRequest request,
            RoomTaskService taskService,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            if (!Enum.TryParse<RoomTaskStatus>(request.Status, ignoreCase: true, out var to)
                || !Enum.IsDefined(to))
            {
                return Results.Problem(
                    detail: $"Unknown task status: '{request.Status}'.",
                    statusCode: StatusCodes.Status400BadRequest);
            }
            var mutation = await taskService.TransitionIdempotentlyAsync(
                request.RequestId,
                RoomId.From(roomId),
                taskId,
                request.ExpectedTaskRevision,
                currentUser.UserId,
                request.ActorSessionId,
                request.ActorSessionIncarnationId,
                to,
                request.Result,
                ct);
            await sharedSurfaceEvents.PublishRoomTaskChangedAsync(
                RoomId.From(roomId),
                currentUser.UserId,
                CancellationToken.None);
            return Results.Ok(RoomResponseMappers.MapMutation(
                request.RequestId,
                "tasks.transition",
                mutation.Receipt));
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<UpdateRoomTaskStatusRequest>>()
        .WithMetadata(new RequestSizeLimitAttribute(
            RoomRequestLimits.EncryptedContentBodyBytes))
        .Produces<RoomMutationResponse>();

        group.MapPost("/{roomId:guid}/tasks/{taskId:guid}/assign", async (
            Guid roomId,
            Guid taskId,
            AssignRoomTaskRequest request,
            RoomTaskService taskService,
            SharedSurfaceEventPublisher sharedSurfaceEvents,
            ICurrentUser currentUser,
            CancellationToken ct) =>
        {
            var mutation = await taskService.AssignIdempotentlyAsync(
                request.RequestId,
                RoomId.From(roomId),
                taskId,
                request.ExpectedTaskRevision,
                currentUser.UserId,
                request.SessionId,
                request.SessionIncarnationId,
                ct);
            await sharedSurfaceEvents.PublishRoomTaskChangedAsync(
                RoomId.From(roomId),
                currentUser.UserId,
                CancellationToken.None);
            return Results.Ok(RoomResponseMappers.MapMutation(
                request.RequestId,
                "tasks.assign",
                mutation.Receipt));
        })
        .AddEndpointFilter<DataAnnotationsValidationFilter<AssignRoomTaskRequest>>()
        .Produces<RoomMutationResponse>();

        return group;
    }

    internal static void ApplyReadPageHeaders(
        HttpResponse response,
        Guid roomId,
        RoomTaskReadPage page,
        RoomTaskStatus? statusFilter = null,
        Guid? assignee = null,
        int limit = 100)
    {
        response.Headers["Kodosi-Task-Snapshot"] = page.Snapshot;
        response.Headers["Kodosi-Has-More"] = page.HasMore ? "true" : "false";
        if (page.NextOffset is not { } nextOffset)
        {
            return;
        }

        response.Headers["Kodosi-Next-Offset"] =
            nextOffset.ToString(System.Globalization.CultureInfo.InvariantCulture);
        var statusQuery = statusFilter is { } status ? $"&status={status}" : string.Empty;
        var assigneeQuery = assignee is { } id ? $"&assignee={id:D}" : string.Empty;
        response.Headers.Link = $"</api/rooms/{roomId:D}/tasks?offset={nextOffset}&limit={limit}&snapshot={page.Snapshot}{statusQuery}{assigneeQuery}>; rel=\"next\"";
    }
}
