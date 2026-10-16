using Kodosi.Data;
using Kodosi.Security;

namespace Kodosi.Devices;

public sealed record ApprovedDevice(string Id, Guid UserId, byte[] SigningPublicKey, long IssuedAtMs, long? ExpiresAtMs)
{
    internal static ApprovedDevice From(Device device) =>
        new(device.Id, device.UserId, device.SigningPublicKey, device.IssuedAtMs, device.ExpiresAtMs);
}

internal static class DeviceApproval
{
    public static bool IsCurrent(bool revoked, long? deviceExpiresAtMs, long? listExpiresAtMs, long nowMs, bool allowExpiredList) =>
        !revoked && !(deviceExpiresAtMs <= nowMs) && (allowExpiredList || !(listExpiresAtMs <= nowMs));

    public static HashSet<string> Listed(byte[] listBody) =>
        SignedDeviceList.Parse(listBody).Entries.Select(x => x.DeviceId).ToHashSet(StringComparer.Ordinal);
}
