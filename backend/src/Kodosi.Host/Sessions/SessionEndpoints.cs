using Kodosi.Accounts;
using Microsoft.AspNetCore.Mvc;

namespace Kodosi.Sessions;

internal static class SessionEndpoints
{
    public static void MapSessions(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/sessions").RequireAuthorization();
        group.MapGet("/", async (DeviceCaller caller, SessionService sessions, CancellationToken ct) =>
            Results.Ok(await sessions.ListAsync(caller.User.Id, caller.Device.Id, ct)));
        group.MapPost("/", async (SessionService.CreateSession body, DeviceCaller caller, SessionService sessions, CancellationToken ct) =>
            Results.Ok(await sessions.CreateAsync(caller.User.Id, caller.Device, body, ct)));
        group.MapGet("/{id:guid}", async (Guid id, DeviceCaller caller, SessionService sessions, CancellationToken ct) =>
            Results.Ok(await sessions.DescribeAsync(await sessions.AuthorizedAsync(id, caller.User.Id, ct), caller.User.Id, ct)));
        group.MapPatch("/{id:guid}", async (Guid id, SessionService.RenameSession body, DeviceCaller caller, SessionService sessions, CancellationToken ct) =>
            Results.Ok(await sessions.RenameAsync(id, caller.User.Id, body, ct)));
        group.MapPut("/{id:guid}/mission", async (Guid id, SessionService.AttachSession body, DeviceCaller caller, SessionService sessions, CancellationToken ct) =>
            Results.Ok(await sessions.AttachAsync(id, caller.User.Id, caller.Device, body, ct)));
        group.MapPut("/{id:guid}/members", async (Guid id, SessionService.ShareSession body, DeviceCaller caller, SessionService sessions, CancellationToken ct) =>
            Results.Ok(await sessions.ShareAsync(id, caller.User.Id, caller.Device.Id, body, ct)));
        group.MapDelete("/{id:guid}/members/me", async (Guid id, [FromBody] SessionService.LeaveSession body, DeviceCaller caller, SessionService sessions,
            CancellationToken ct) =>
        {
            await sessions.LeaveAsync(id, caller.User.Id, body, ct);
            return Results.NoContent();
        });
        group.MapPut("/{id:guid}/hidden", async (Guid id, SessionService.HideSession body, DeviceCaller caller, SessionService sessions,
            CancellationToken ct) =>
        {
            await sessions.HideAsync(id, caller.User.Id, body, ct);
            return Results.NoContent();
        });
        group.MapPut("/{id:guid}/close-request", async (Guid id, SessionService.RequestClose body, DeviceCaller caller, SessionService sessions,
            CancellationToken ct) =>
        {
            await sessions.RequestCloseAsync(id, caller.User.Id, caller.Device, body, ct);
            return Results.NoContent();
        });
        group.MapDelete("/{id:guid}/close-request", async (Guid id, Guid incarnationId, DeviceCaller caller, SessionService sessions,
            CancellationToken ct) =>
        {
            await sessions.RefuseCloseAsync(id, caller.User.Id, caller.Device.Id, incarnationId, ct);
            return Results.NoContent();
        });
        group.MapDelete("/{id:guid}", async (Guid id, Guid incarnationId, DeviceCaller caller, SessionService sessions, CancellationToken ct) =>
        {
            await sessions.EndAsync(id, caller.User.Id, caller.Device.Id, incarnationId, ct);
            return Results.NoContent();
        });
    }
}
