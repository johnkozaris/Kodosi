using System.Security.Claims;
using Kodosi.Accounts;
using Kodosi.TerminalConnections;

namespace Kodosi.Devices;

internal static class DeviceEndpoints
{
    public static void MapDevices(this IEndpointRouteBuilder app)
    {
        var api = app.MapGroup("/api").RequireAuthorization();
        api.MapPost("/me/devices/challenge", (SignedIn me, DeviceService devices) => Results.Ok(devices.CreateChallenge(me.User.Id)))
            .RequireRateLimiting(RateLimits.Challenge);
        api.MapPost("/me/device-sessions", async (DeviceService.OpenDeviceSession body, SignedIn me, DeviceService devices, CancellationToken ct) =>
            Results.Ok(await devices.OpenSessionAsync(me.User.Id, body, ct))).RequireRateLimiting(RateLimits.Challenge);
        api.MapPost("/me/devices", async (DeviceService.RegisterDevice body, SignedIn me, DeviceService devices, CancellationToken ct) =>
        {
            await devices.EnrollAsync(me.User.Id, body, ct);
            return Results.Created($"/api/me/devices/{body.DeviceId}", (object?)null);
        }).RequireRateLimiting(RateLimits.Enrollment);
        api.MapGet("/me/identity", async (SignedIn me, HttpContext context, DeviceService devices, CancellationToken ct) =>
            Tagged(context, await devices.IdentityAsync(me.User.Id, me.User.Id, context.Request.Headers.IfNoneMatch, ct)));
        api.MapGet("/users/{userId:guid}/identity", async (Guid userId, DeviceCaller caller, HttpContext context, DeviceService devices, CancellationToken ct) =>
            Tagged(context, await devices.IdentityAsync(caller.User.Id, userId, context.Request.Headers.IfNoneMatch, ct)));
        api.MapPost("/me/identity/device-list", async (DeviceService.ReplaceDeviceList body, RenewingDevice caller, DeviceService devices, CancellationToken ct) =>
        {
            await devices.ReplaceListAsync(caller.User.Id, body, ct);
            return Results.NoContent();
        });
        api.MapPost("/me/connections/renew", (DeviceCaller caller, HttpContext context, ConnectionDirectory connections) =>
        {
            if (!long.TryParse(context.User.FindFirstValue("exp"), out var expires)) return Results.Unauthorized();
            connections.Renew(caller.User.Id, caller.Device.Id, DateTimeOffset.FromUnixTimeSeconds(expires));
            return Results.NoContent();
        });
        api.MapPost("/me/identity/reset", async (SignedIn me, HttpContext context, DeviceService devices, CancellationToken ct) =>
        {
            await devices.ResetIdentityAsync(me.User.Id, CurrentUser.SignedInAt(context.User), ct);
            return Results.NoContent();
        }).RequireRateLimiting(RateLimits.Enrollment);
        api.MapPost("/devices/link/init", async (DeviceService.LinkInit body, SignedIn me, DeviceService devices, CancellationToken ct) =>
            Results.Ok(await devices.StartLinkAsync(me.User.Id, body, ct))).RequireRateLimiting(RateLimits.Enrollment);
        api.MapPost("/me/recovery-key", async (DeviceService.RecoveryKeyWrite body, DeviceCaller caller, DeviceService devices, CancellationToken ct) =>
        {
            await devices.AddRecoveryKeyAsync(caller.User.Id, caller.Device, body, ct);
            return Results.Ok();
        }).RequireRateLimiting(RateLimits.Enrollment);
        api.MapGet("/me/recovery-key", async (SignedIn me, DeviceService devices, CancellationToken ct) =>
            Results.Ok(await devices.RecoveryBoxAsync(me.User.Id, ct))).RequireRateLimiting(RateLimits.Enrollment);
        api.MapPost("/me/devices/recover", async (DeviceService.RegisterDevice body, SignedIn me, DeviceService devices, CancellationToken ct) =>
        {
            await devices.RecoverAsync(me.User.Id, body, ct);
            return Results.Created($"/api/me/devices/{body.DeviceId}", (object?)null);
        }).RequireRateLimiting(RateLimits.Enrollment);
        api.MapGet("/devices/link/requests", async (DeviceCaller caller, DeviceService devices, CancellationToken ct) =>
            Results.Ok(await devices.PendingLinksAsync(caller.User.Id, ct)));
        api.MapPost("/devices/link/approve", async (DeviceService.LinkApprove body, DeviceCaller caller, DeviceService devices, CancellationToken ct) =>
        {
            await devices.ApproveLinkAsync(caller.User.Id, caller.Device, body, ct);
            return Results.Ok();
        }).RequireRateLimiting(RateLimits.Enrollment);
        api.MapPost("/devices/link/poll", async (DeviceService.LinkPoll body, SignedIn me, DeviceService devices, CancellationToken ct) =>
            Results.Ok(await devices.PollLinkAsync(me.User.Id, body.DeviceCode, ct))).RequireRateLimiting(RateLimits.Challenge);
        api.MapPost("/devices/link/ack", async (DeviceService.LinkAcknowledge body, SignedIn me, DeviceService devices, CancellationToken ct) =>
        {
            await devices.AcknowledgeLinkAsync(me.User.Id, body, ct);
            return Results.NoContent();
        });
        api.MapDelete("/devices/link/requests/{requestId:guid}", async (Guid requestId, SignedIn me, DeviceService devices, CancellationToken ct) =>
        {
            await devices.CancelLinkAsync(me.User.Id, requestId, ct);
            return Results.NoContent();
        });
    }

    private static IResult Tagged(HttpContext context, (DeviceService.IdentityBundle? Bundle, string Tag) identity)
    {
        context.Response.Headers.ETag = identity.Tag;
        return identity.Bundle is null ? Results.StatusCode(StatusCodes.Status304NotModified) : Results.Ok(identity.Bundle);
    }
}
