using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text;

namespace Kodosi.Security;

internal static class DomainTags
{
    public static ReadOnlySpan<byte> DevicePopV1 => "kodosi-device-pop-v1"u8;
    public static ReadOnlySpan<byte> DeviceConnectionProofV1 => "kodosi-device-connection-proof-v1"u8;
    public static ReadOnlySpan<byte> DeviceHttpRequestProofV1 => "kodosi-device-http-request-proof-v1"u8;
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

    public static byte[] Http(Guid user, string device, Guid challengeId, string method,
        string target, string bodyHash, ReadOnlySpan<byte> challenge)
    {
        using var stream = new MemoryStream();
        stream.Write(DomainTags.DeviceHttpRequestProofV1);
        foreach (var value in new[] { user.ToString("D"), device, challengeId.ToString("D"),
                     method.ToUpperInvariant(), target, bodyHash.ToLowerInvariant() })
            CanonicalLengthPrefixedUtf8.Write(stream, value);
        stream.Write(challenge);
        return stream.ToArray();
    }

    public static byte[] Connection(Guid user, string device, string connection, string purpose,
        Guid? session, Guid? incarnation, ReadOnlySpan<byte> challenge)
    {
        using var stream = new MemoryStream();
        stream.Write(DomainTags.DeviceConnectionProofV1);
        foreach (var value in new[] { user.ToString("D"), device, connection, purpose,
                     session?.ToString("D") ?? "", incarnation?.ToString("D") ?? "" })
            CanonicalLengthPrefixedUtf8.Write(stream, value);
        stream.Write(challenge);
        return stream.ToArray();
    }
}
