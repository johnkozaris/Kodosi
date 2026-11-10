using Kodosi.Accounts;
using Kodosi.Data;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Push;

internal static class PushEndpoints
{
    public static void MapPush(this IEndpointRouteBuilder app)
    {
        var api = app.MapGroup("/api").RequireAuthorization();
        api.MapPut("/me/push-token", async (PushTokens.Registration body, DeviceCaller caller, PushTokens tokens, CancellationToken ct) =>
        {
            await tokens.RegisterAsync(caller.User.Id, caller.Device.Id, body, ct);
            return Results.NoContent();
        });
        api.MapDelete("/me/push-token", async (DeviceCaller caller, PushTokens tokens, CancellationToken ct) =>
        {
            await tokens.RemoveAsync(caller.Device.Id, ct);
            return Results.NoContent();
        });
    }
}

public sealed class PushTokens(KodosiDbContext db, PushSettings settings, TimeProvider clock)
{
    public async Task RegisterAsync(Guid userId, string deviceId, Registration registration, CancellationToken ct)
    {
        var (platform, token, environment) = Checked(registration);
        if (!settings.Serves(platform)) throw ApiException.Missing("Push notifications are off on this server.");
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        await db.PushTokens.Where(x => x.Token == token && x.DeviceId != deviceId).ExecuteDeleteAsync(ct);
        var kept = await db.PushTokens.SingleOrDefaultAsync(x => x.DeviceId == deviceId, ct);
        if (kept is null) db.PushTokens.Add(kept = new PushToken { DeviceId = deviceId, UserId = userId });
        kept.Platform = platform;
        kept.Token = token;
        kept.Environment = environment;
        kept.UpdatedAt = clock.GetUtcNow();
        await db.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
    }

    public Task RemoveAsync(string deviceId, CancellationToken ct) =>
        db.PushTokens.Where(x => x.DeviceId == deviceId).ExecuteDeleteAsync(ct);

    private static (string Platform, string Token, string? Environment) Checked(Registration registration) => registration switch
    {
        { Platform: "apns", Environment: not ("development" or "production") } =>
            throw ApiException.Invalid("An APNs token needs the environment development or production."),
        { Platform: "apns", Token: { Length: >= 64 and <= 200 } token } when token.All(char.IsAsciiHexDigit) =>
            ("apns", token, registration.Environment),
        { Platform: "apns" } => throw ApiException.Invalid("An APNs token has 64 to 200 hexadecimal digits."),
        { Platform: "fcm", Environment: not null } => throw ApiException.Invalid("An FCM token has no environment."),
        { Platform: "fcm", Token: { Length: >= 1 and <= 4096 } token } when token.All(c => c is >= ' ' and <= '~') =>
            ("fcm", token, null),
        { Platform: "fcm" } => throw ApiException.Invalid("An FCM token has 1 to 4096 printable ASCII characters."),
        _ => throw ApiException.Invalid("The push platform must be apns or fcm."),
    };

    public sealed record Registration(string Platform, string Token, string? Environment = null);
}
