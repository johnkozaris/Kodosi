using Kodosi.Accounts;
using Microsoft.EntityFrameworkCore;
using Kodosi.Data;

namespace Kodosi.Devices;

internal static class DeviceEndpoints
{
    public static void MapDevices(this IEndpointRouteBuilder app)
    {
        var api = app.MapGroup("/api").RequireAuthorization();
        foreach (var path in new[] { "/me/devices/challenge", "/me/device-proofs/challenge" })
            api.MapPost(path, async (HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
                Results.Ok(await service.CreateChallengeAsync((await users.GetAsync(context, ct)).Id, ct))).RequireRateLimiting("challenge");
        api.MapPost("/me/devices", async (DeviceService.RegisterDevice body, HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
        {
            await service.EnrollAsync((await users.GetAsync(context, ct)).Id, body, ct);
            return Results.Created($"/api/me/devices/{body.DeviceId}", (object?)null);
        }).RequireRateLimiting("enrollment");
        api.MapGet("/me/identity", async (HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
        {
            var user = await users.GetAsync(context, ct);
            return Results.Ok(await service.IdentityAsync(user.Id, user.Id, ct));
        });
        api.MapGet("/users/{userId:guid}/identity", async (Guid userId, HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
        {
            var user = await users.GetAsync(context, ct);
            await service.RequireProofAsync(context, user.Id, ct);
            return Results.Ok(await service.IdentityAsync(user.Id, userId, ct));
        });
        api.MapPost("/me/identity/device-list", async (DeviceService.ReplaceDeviceList body, HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
        {
            var user = await users.GetAsync(context, ct);
            await service.RequireProofAsync(context, user.Id, ct, allowExpiredList: true);
            await service.ReplaceListAsync(user.Id, body, ct);
            return Results.NoContent();
        });
        api.MapGet("/me/devices", async (HttpContext context, CurrentUser users, DeviceService service, KodosiDbContext db, CancellationToken ct) =>
        {
            var user = await users.GetAsync(context, ct);
            var self = await service.RequireProofAsync(context, user.Id, ct);
            var devices = await db.Devices.AsNoTracking().Where(x => x.UserId == user.Id && !x.Revoked)
                .Select(x => new { deviceId = x.Id, x.Label, certSignerDeviceId = x.SignerDeviceId, certIssuedAtMs = x.IssuedAtMs }).ToListAsync(ct);
            return Results.Ok(new { selfDeviceId = self.Id, devices });
        });
        api.MapPost("/devices/link/init", async (DeviceService.LinkInit body, HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
            Results.Ok(await service.StartLinkAsync((await users.GetAsync(context, ct)).Id, body, ct))).RequireRateLimiting("enrollment");
        api.MapGet("/devices/link/pending", async (string userCode, HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
        {
            var user = await users.GetAsync(context, ct); await service.RequireProofAsync(context, user.Id, ct);
            return Results.Ok(await service.PendingLinkAsync(user.Id, userCode, ct));
        });
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
        api.MapDelete("/devices/link/requests/{userCode}", async (string userCode, HttpContext context, CurrentUser users, DeviceService service, CancellationToken ct) =>
        {
            await service.CancelLinkAsync((await users.GetAsync(context, ct)).Id, userCode, ct); return Results.NoContent();
        });
    }
}
