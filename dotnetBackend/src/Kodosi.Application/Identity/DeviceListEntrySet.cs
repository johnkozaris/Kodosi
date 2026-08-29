using Kodosi.Domain;

namespace Kodosi.Application;

public static class DeviceListEntrySet
{
    public static void RequireCertificateSignerContinuity(
        IEnumerable<(string DeviceId, string SignerDeviceId)> entries,
        IReadOnlyDictionary<string, UserDevice> devices)
    {
        foreach (var (deviceId, signerDeviceId) in entries)
        {
            if (!devices.TryGetValue(deviceId, out var device)
                || !string.Equals(
                    device.CertSignerDeviceId,
                    signerDeviceId,
                    StringComparison.Ordinal))
            {
                throw new DeviceEnrollmentException(
                    $"Signed list entry for {deviceId} does not preserve its certificate signer.");
            }
        }
    }
}
