using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Projects;
using Kodosi.Security;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Devices;

public sealed partial class DeviceService
{
    public static readonly TimeSpan ResetWait = TimeSpan.FromDays(1);

    public async Task<object?> RequestResetAsync(Guid userId, ResetRequest request, DateTimeOffset? authenticatedAt, CancellationToken ct)
    {
        var now = clock.GetUtcNow();
        var deviceId = DeviceIdRules.Require(request.DeviceId);
        var label = Limits.Text(request.DeviceLabel, "Device label", 128);
        var signing = Limits.Base64(request.SigningPublicKey, "Signing key", IdentityWireFormat.MlDsa65PublicKeyLength);
        if (signing.Length != IdentityWireFormat.MlDsa65PublicKeyLength) throw ApiException.Invalid("Device keys have invalid lengths.");
        if (IsRecoveryId(deviceId)) throw ApiException.Invalid("This device identifier is for a recovery key.");
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        await db.LockUserAsync(userId, ct);
        var reset = await db.IdentityResets.SingleOrDefaultAsync(x => x.UserId == userId, ct);
        var waiting = reset is not null && reset.DeviceId == deviceId && reset.SigningPublicKey.AsSpan().SequenceEqual(signing);
        if (!waiting) CurrentUser.RequireRecentSignIn(authenticatedAt, now, "Sign in again to start fresh on this device.");
        if (await db.Devices.AnyAsync(x => x.Id == deviceId, ct)) throw ApiException.Conflict("This device identity has already been used.");
        if (!await HasIdentityAsync(userId, ct))
        {
            if (reset is not null) db.IdentityResets.Remove(reset);
            await db.SaveChangesAsync(ct);
            await transaction.CommitAsync(ct);
            return null;
        }
        if (!waiting)
        {
            reset ??= db.IdentityResets.Add(new IdentityReset { UserId = userId }).Entity;
            reset.Id = Guid.CreateVersion7();
            reset.DeviceId = deviceId;
            reset.DeviceLabel = label;
            reset.SigningPublicKey = signing;
            reset.RequestedAt = now;
            reset.ReadyAt = now + ResetWait;
            await db.SaveChangesAsync(ct);
            await transaction.CommitAsync(ct);
            connections.Notify([userId], "devices");
        }
        return ResetView(reset!, now);
    }

    public async Task<object> PendingResetAsync(Guid userId, CancellationToken ct)
    {
        var reset = await db.IdentityResets.AsNoTracking().SingleOrDefaultAsync(x => x.UserId == userId, ct)
            ?? throw ApiException.Missing();
        return ResetView(reset, clock.GetUtcNow());
    }

    public async Task CancelResetAsync(Guid userId, Guid requestId, CancellationToken ct)
    {
        if (await db.IdentityResets.Where(x => x.UserId == userId && x.Id == requestId).ExecuteDeleteAsync(ct) == 0)
            throw ApiException.Missing();
        connections.Notify([userId], "devices");
    }

    private async Task<bool> HasIdentityAsync(Guid userId, CancellationToken ct) =>
        await db.DeviceLists.AnyAsync(x => x.UserId == userId, ct)
        || await db.Devices.AnyAsync(x => x.UserId == userId && !x.Revoked, ct);

    private async Task<Replaced> ReplaceIdentityAsync(Guid userId, string deviceId, byte[] signingKey, CancellationToken ct)
    {
        var reset = await db.IdentityResets.SingleOrDefaultAsync(x => x.UserId == userId, ct);
        if (reset is null || reset.DeviceId != deviceId || !reset.SigningPublicKey.AsSpan().SequenceEqual(signingKey)
            || reset.ReadyAt > clock.GetUtcNow())
            throw ApiException.Forbidden("An existing trusted device must approve this device.");
        var devices = await db.Devices.Where(x => x.UserId == userId && !x.Revoked).ToListAsync(ct);
        foreach (var device in devices) device.Revoked = true;
        await db.PushTokens.Where(x => x.UserId == userId).ExecuteDeleteAsync(ct);
        if (await db.DeviceLists.SingleOrDefaultAsync(x => x.UserId == userId, ct) is { } list) db.DeviceLists.Remove(list);
        await db.FriendLists.Where(x => x.UserId == userId).ExecuteDeleteAsync(ct);
        await db.DeviceLinks.Where(x => x.UserId == userId && x.State == "pending")
            .ExecuteUpdateAsync(set => set.SetProperty(x => x.State, "cancelled"), ct);
        db.IdentityResets.Remove(reset);
        var affected = await InvalidateUserSessionsAsync(userId, devices, ct);
        await db.SaveChangesAsync(ct);
        return new(devices, affected);
    }

    private async Task PublishReplacedAsync(Guid userId, Replaced replaced, CancellationToken ct)
    {
        sessions.Remove(userId);
        foreach (var device in replaced.Devices) connections.RemoveDevice(userId, device.Id);
        await PublishSessionChangesAsync(userId, replaced.Sessions, replaced.Devices, ct);
        await ProjectMembership.NotifyProjectsOfAsync(db, connections, userId, ct);
    }

    private static object ResetView(IdentityReset reset, DateTimeOffset now) => new
    {
        requestId = reset.Id,
        reset.DeviceId,
        reset.DeviceLabel,
        requestedAtMs = reset.RequestedAt.ToUnixTimeMilliseconds(),
        readyAtMs = reset.ReadyAt.ToUnixTimeMilliseconds(),
        ready = reset.ReadyAt <= now,
    };

    private sealed record Replaced(List<Device> Devices, List<Session> Sessions);

    public sealed record ResetRequest(string DeviceId, string DeviceLabel, string SigningPublicKey);
}
