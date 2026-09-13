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
    public static ReadOnlySpan<byte> SessionKeyBlobV2 => "kodosi-session-key-blob-v2"u8;
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

    public static byte[] SessionKey(Guid session, Guid incarnation, string recipientDevice,
        ReadOnlySpan<byte> wrappedKey, uint generation, ulong issuedAtMs)
    {
        using var hasher = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        hasher.AppendData(DomainTags.SessionKeyBlobV2);
        UInt32(hasher, sizeof(ulong));
        UInt64(hasher, issuedAtMs);
        UInt32(hasher, sizeof(uint));
        UInt32(hasher, generation);
        Bytes(hasher, Encoding.UTF8.GetBytes(session.ToString("D")));
        Bytes(hasher, Encoding.UTF8.GetBytes(incarnation.ToString("D")));
        Bytes(hasher, Encoding.UTF8.GetBytes(recipientDevice));
        Bytes(hasher, wrappedKey);
        return hasher.GetHashAndReset();
    }

    private static void Bytes(IncrementalHash hash, ReadOnlySpan<byte> bytes)
    {
        UInt32(hash, checked((uint)bytes.Length));
        hash.AppendData(bytes);
    }
    private static void UInt32(IncrementalHash hash, uint value)
    {
        Span<byte> bytes = stackalloc byte[4];
        BinaryPrimitives.WriteUInt32BigEndian(bytes, value);
        hash.AppendData(bytes);
    }
    private static void UInt64(IncrementalHash hash, ulong value)
    {
        Span<byte> bytes = stackalloc byte[8];
        BinaryPrimitives.WriteUInt64BigEndian(bytes, value);
        hash.AppendData(bytes);
    }
}
