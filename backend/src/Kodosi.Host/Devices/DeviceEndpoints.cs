using System.Security.Claims;
using Kodosi.Accounts;
using Kodosi.TerminalConnections;

namespace Kodosi.Devices;

internal static class DeviceEndpoints
{
    public static void MapDevices(this IEndpointRouteBuilder app)
    {
        var api = app.MapGroup("/api").RequireAuthorization();
        api.MapPost("/me/devices/challenge", async (HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
            Results.Ok(service.CreateChallenge((await users.GetAsync(context, ct)).Id))).RequireRateLimiting("challenge");
        api.MapPost("/me/device-sessions", async (DeviceService.OpenDeviceSession body, HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
            Results.Ok(await service.OpenSessionAsync((await users.GetAsync(context, ct)).Id, body, ct))).RequireRateLimiting("challenge");
        api.MapPost("/me/devices", async (DeviceService.RegisterDevice body, HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
        {
            await service.EnrollAsync((await users.GetAsync(context, ct)).Id, body, ct);
            return Results.Created($"/api/me/devices/{body.DeviceId}", (object?)null);
        }).RequireRateLimiting("enrollment");
        api.MapGet("/me/identity", async (HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
        {
            var user = await users.GetAsync(context, ct);
            return Tagged(context, await service.IdentityAsync(user.Id, user.Id, context.Request.Headers.IfNoneMatch, ct));
        });
        api.MapGet("/users/{userId:guid}/identity", async (Guid userId, HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
        {
            var user = await users.GetAsync(context, ct);
            await service.RequireProofAsync(context, user.Id, ct);
            return Tagged(context, await service.IdentityAsync(user.Id, userId, context.Request.Headers.IfNoneMatch, ct));
        });
        api.MapPost("/me/identity/device-list", async (DeviceService.ReplaceDeviceList body, HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
        {
            var user = await users.GetAsync(context, ct);
            await service.RequireProofAsync(context, user.Id, ct, allowExpiredList: true);
            await service.ReplaceListAsync(user.Id, body, ct);
            return Results.NoContent();
        });
        api.MapPost("/me/connections/renew", async (HttpContext context, CurrentUser users, DeviceService service, ConnectionDirectory connections, CancellationToken ct) =>
        {
            var user = await users.GetAsync(context, ct); var device = await service.RequireProofAsync(context, user.Id, ct);
            if (!long.TryParse(context.User.FindFirstValue("exp"), out var expires)) return Results.Unauthorized();
            connections.Renew(user.Id, device.Id, DateTimeOffset.FromUnixTimeSeconds(expires));
            return Results.NoContent();
        });
        api.MapPost("/me/identity/reset", async (HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
        {
            var user = await users.GetAsync(context, ct);
            await service.ResetIdentityAsync(user.Id, AuthenticatedAt(context.User), ct);
            return Results.NoContent();
        }).RequireRateLimiting("enrollment");
        api.MapPost("/devices/link/init", async (DeviceService.LinkInit body, HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
            Results.Ok(await service.StartLinkAsync((await users.GetAsync(context, ct)).Id, body, ct))).RequireRateLimiting("enrollment");
        api.MapGet("/devices/link/requests", async (HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
        {
            var user = await users.GetAsync(context, ct); await service.RequireProofAsync(context, user.Id, ct);
            return Results.Ok(await service.PendingLinksAsync(user.Id, ct));
        });
        api.MapPost("/devices/link/approve", async (DeviceService.LinkApprove body, HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
        {
            var user = await users.GetAsync(context, ct); var device = await service.RequireProofAsync(context, user.Id, ct);
            await service.ApproveLinkAsync(user.Id, device, body, ct); return Results.Ok();
        }).RequireRateLimiting("enrollment");
        api.MapPost("/devices/link/poll", async (DeviceService.LinkPoll body, HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
            Results.Ok(await service.PollLinkAsync((await users.GetAsync(context, ct)).Id, body.DeviceCode, ct))).RequireRateLimiting("challenge");
        api.MapPost("/devices/link/ack", async (DeviceService.LinkAcknowledge body, HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
        {
            await service.AcknowledgeLinkAsync((await users.GetAsync(context, ct)).Id, body, ct); return Results.NoContent();
        });
        api.MapDelete("/devices/link/requests/{requestId:guid}", async (Guid requestId, HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
        {
            await service.CancelLinkAsync((await users.GetAsync(context, ct)).Id, requestId, ct); return Results.NoContent();
        });
    }

    private static IResult Tagged(HttpContext context, (DeviceService.IdentityBundle? Bundle, string Tag) identity)
    {
        context.Response.Headers.ETag = identity.Tag;
        return identity.Bundle is null ? Results.StatusCode(StatusCodes.Status304NotModified) : Results.Ok(identity.Bundle);
    }

    private static DateTimeOffset? AuthenticatedAt(ClaimsPrincipal principal)
    {
        var value = principal.FindFirstValue("auth_time") ?? principal.FindFirstValue("iat");
        return long.TryParse(value, out var seconds) ? DateTimeOffset.FromUnixTimeSeconds(seconds) : null;
    }
}
