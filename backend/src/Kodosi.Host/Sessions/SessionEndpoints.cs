using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Devices;

namespace Kodosi.Sessions;

internal static class SessionEndpoints
{
    public static void MapSessions(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/sessions").RequireAuthorization();
        group.MapGet("/", async (HttpContext ctx, CurrentUser users, DeviceService devices, SessionService sessions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await sessions.ListAsync(actor.User.Id, ct)); });
        group.MapPost("/", async (SessionService.CreateSession body, HttpContext ctx, CurrentUser users, DeviceService devices, SessionService sessions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await sessions.CreateAsync(actor.User.Id, actor.Device, body, ct)); });
        group.MapGet("/{id:guid}", async (Guid id, HttpContext ctx, CurrentUser users, DeviceService devices, SessionService sessions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await sessions.DescribeAsync(await sessions.AuthorizedAsync(id, actor.User.Id, ct), ct)); });
        group.MapPatch("/{id:guid}", async (Guid id, SessionService.RenameSession body, HttpContext ctx, CurrentUser users, DeviceService devices, SessionService sessions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await sessions.RenameAsync(id, actor.User.Id, actor.Device.Id, body, ct)); });
        group.MapPut("/{id:guid}/mission", async (Guid id, SessionService.AttachSession body, HttpContext ctx, CurrentUser users, DeviceService devices, SessionService sessions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await sessions.AttachAsync(id, actor.User.Id, actor.Device.Id, body, ct)); });
        group.MapPut("/{id:guid}/members", async (Guid id, SessionService.ShareSession body, HttpContext ctx, CurrentUser users, DeviceService devices, SessionService sessions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await sessions.ShareAsync(id, actor.User.Id, actor.Device.Id, body, ct)); });
        group.MapDelete("/{id:guid}/members/me", async (Guid id, HttpContext ctx, CurrentUser users, DeviceService devices, SessionService sessions, CancellationToken ct) =>
        {
            var actor = await Actor(ctx, users, devices, ct);
            var body = await ctx.Request.ReadFromJsonAsync<SessionService.LeaveSession>(cancellationToken: ct) ?? throw ApiException.Invalid("A current session identity is required.");
            await sessions.LeaveAsync(id, actor.User.Id, body, ct); return Results.NoContent();
        });
        group.MapDelete("/{id:guid}", async (Guid id, Guid incarnationId, HttpContext ctx, CurrentUser users, DeviceService devices, SessionService sessions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); await sessions.EndAsync(id, actor.User.Id, actor.Device.Id, incarnationId, ct); return Results.NoContent(); });
        group.MapGet("/{id:guid}/devices", async (Guid id, HttpContext ctx, CurrentUser users, DeviceService devices, SessionService sessions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await sessions.AuthorizedDevicesAsync(id, actor.User.Id, actor.Device.Id, ct)); });
        group.MapPost("/{id:guid}/keys/rotate", async (Guid id, SessionService.RotateKeys body, HttpContext ctx, CurrentUser users, DeviceService devices, SessionService sessions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await sessions.RotateAsync(id, actor.User.Id, actor.Device.Id, body, ct)); });
        group.MapPost("/{id:guid}/keys", async (Guid id, SessionService.PublishKeys body, HttpContext ctx, CurrentUser users, DeviceService devices, SessionService sessions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); await sessions.PublishKeysAsync(id, actor.User.Id, actor.Device.Id, body, ct); return Results.NoContent(); });
        group.MapGet("/{id:guid}/keys/mine", async (Guid id, HttpContext ctx, CurrentUser users, DeviceService devices, SessionService sessions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await sessions.MyKeyAsync(id, actor.User.Id, actor.Device.Id, ct)); });
    }
    private static async Task<(User User, Device Device)> Actor(HttpContext ctx, CurrentUser users, DeviceService devices, CancellationToken ct)
    { var user = await users.GetAsync(ctx, ct); return (user, await devices.RequireProofAsync(ctx, user.Id, ct)); }
}
