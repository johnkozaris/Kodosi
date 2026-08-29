using Kodosi.Domain;

namespace Kodosi.Application;

public static class ActiveDeviceAuthorization
{
    public static bool IsAuthorized(
        UserDevice? device,
        UserDeviceList? deviceList,
        UserId userId,
        DateTimeOffset now) =>
        Evaluate(device, deviceList, userId, now).Authorized;

    public static ActiveDeviceAuthorizationDecision Evaluate(
        UserDevice? device,
        UserDeviceList? deviceList,
        UserId userId,
        DateTimeOffset now)
    {
        if (deviceList is null
            || deviceList.UserId != userId
            || !IsCertificateActive(device, userId, now))
        {
            return new ActiveDeviceAuthorizationDecision(false, null);
        }

        var proof = deviceList.ParseBody();
        if (proof.ExpiresAtMs is { } expiresAtMs
            && expiresAtMs <= now.ToUnixTimeMilliseconds())
        {
            return new ActiveDeviceAuthorizationDecision(false, null);
        }
        if (!proof.Entries.Any(entry => entry.DeviceId == device!.DeviceId))
        {
            return new ActiveDeviceAuthorizationDecision(false, null);
        }

        var listExpiry = proof.ExpiresAtMs is { } expiry
            ? DateTimeOffset.FromUnixTimeMilliseconds(expiry)
            : (DateTimeOffset?)null;
        return new ActiveDeviceAuthorizationDecision(
            true,
            Min(device!.CertExpiresAt, listExpiry));
    }

    public static bool IsCurrent(
        UserDeviceList? deviceList,
        UserId userId,
        DateTimeOffset now)
    {
        if (deviceList is null || deviceList.UserId != userId)
        {
            return false;
        }
        var proof = deviceList.ParseBody();
        return proof.ExpiresAtMs is null
            || proof.ExpiresAtMs > now.ToUnixTimeMilliseconds();
    }

    public static bool IsCertificateActive(
        UserDevice? device,
        UserId userId,
        DateTimeOffset now)
    {
        if (device is null
            || device.UserId != userId
            || device.RevokedAt is not null)
        {
            return false;
        }
        device.RequireCertificateIntegrity();
        return device.CertExpiresAt is null || device.CertExpiresAt > now;
    }

    private static DateTimeOffset? Min(DateTimeOffset? left, DateTimeOffset? right)
    {
        if (left is null) return right;
        if (right is null) return left;
        return left <= right ? left : right;
    }
}

public readonly record struct ActiveDeviceAuthorizationDecision(
    bool Authorized,
    DateTimeOffset? ExpiresAt);
