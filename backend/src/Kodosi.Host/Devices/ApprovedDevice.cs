using Kodosi.Data;
using Kodosi.Security;

namespace Kodosi.Devices;

/// <summary>An approved device of an account, with the fields that requests use.</summary>
public sealed record ApprovedDevice(string Id, Guid UserId, byte[] SigningPublicKey, long IssuedAtMs, long? ExpiresAtMs)
{
    internal static ApprovedDevice From(Device device) =>
        new(device.Id, device.UserId, device.SigningPublicKey, device.IssuedAtMs, device.ExpiresAtMs);
}

/// <summary>The rule for an approved device: not revoked, not expired, and in the current signed list of its account.</summary>
internal static class DeviceApproval
{
    public static bool IsCurrent(bool revoked, long? deviceExpiresAtMs, long? listExpiresAtMs, long nowMs, bool allowExpiredList) =>
        !revoked && !(deviceExpiresAtMs <= nowMs) && (allowExpiredList || !(listExpiresAtMs <= nowMs));

    public static HashSet<string> Listed(byte[] listBody) =>
        SignedDeviceList.Parse(listBody).Entries.Select(x => x.DeviceId).ToHashSet(StringComparer.Ordinal);
}
