using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text;

namespace Kodosi.Security;

internal static class DomainTags
{
    public static ReadOnlySpan<byte> DevicePopV1 => "kodosi-device-pop-v1"u8;
    public static ReadOnlySpan<byte> DeviceSessionV1 => "kodosi-device-session-v1"u8;
    public static ReadOnlySpan<byte> DeviceCertV2 => "kodosi-device-cert-v2"u8;
    public static ReadOnlySpan<byte> DeviceListV1 => "kodosi-device-list-v1"u8;
}

internal static class DeviceIdRules
{
    public const int MaximumLength = 256;
    public static string Require(string? value)
    {
        if (string.IsNullOrWhiteSpace(value) || value.Length > MaximumLength || value != value.Trim() || value.Any(char.IsControl))
            throw ApiException.Invalid("Device ID must be canonical.");
        return value;
    }
}

internal static class Proofs
{
    public static byte[] Tagged(ReadOnlySpan<byte> domain, ReadOnlySpan<byte> body)
    {
        var result = new byte[domain.Length + body.Length];
        domain.CopyTo(result);
        body.CopyTo(result.AsSpan(domain.Length));
        return result;
    }

    public static byte[] DeviceSession(Guid user, string device, Guid challengeId, ReadOnlySpan<byte> challenge)
    {
        using var stream = new MemoryStream();
        stream.Write(DomainTags.DeviceSessionV1);
        foreach (var value in new[] { user.ToString("D"), device, challengeId.ToString("D") })
            CanonicalLengthPrefixedUtf8.Write(stream, value);
        stream.Write(challenge);
        return stream.ToArray();
    }
}
