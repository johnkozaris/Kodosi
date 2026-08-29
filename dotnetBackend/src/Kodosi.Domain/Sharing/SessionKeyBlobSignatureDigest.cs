using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text;

namespace Kodosi.Domain;

public static class SessionKeyBlobSignatureDigest
{
    public const int LegacyVersion = 1;
    public const int CurrentVersion = 2;

    public static byte[] ComputeV1(
        string sessionId,
        string recipientDeviceId,
        ReadOnlySpan<byte> encryptedSessionKey,
        uint keyGeneration,
        ulong issuedAtMs)
    {
        ArgumentNullException.ThrowIfNull(sessionId);
        ArgumentNullException.ThrowIfNull(recipientDeviceId);

        var sessionIdBytes = Encoding.UTF8.GetBytes(sessionId);
        var recipientDeviceIdBytes = Encoding.UTF8.GetBytes(recipientDeviceId);
        using var hasher = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);

        hasher.AppendData(DomainTags.SessionKeyV2);
        AppendUInt32(hasher, sizeof(ulong));
        AppendUInt64(hasher, issuedAtMs);
        AppendUInt32(hasher, sizeof(uint));
        AppendUInt32(hasher, keyGeneration);
        AppendLengthPrefixed(hasher, sessionIdBytes);
        AppendLengthPrefixed(hasher, recipientDeviceIdBytes);
        AppendLengthPrefixed(hasher, encryptedSessionKey);

        return hasher.GetHashAndReset();
    }

    public static byte[] ComputeV2(
        string sessionId,
        Guid incarnationId,
        string recipientDeviceId,
        ReadOnlySpan<byte> encryptedSessionKey,
        uint keyGeneration,
        ulong issuedAtMs)
    {
        ArgumentNullException.ThrowIfNull(sessionId);
        ArgumentNullException.ThrowIfNull(recipientDeviceId);
        if (incarnationId == Guid.Empty)
        {
            throw new ArgumentException(
                "Session incarnation ID cannot be empty.",
                nameof(incarnationId));
        }

        var sessionIdBytes = Encoding.UTF8.GetBytes(sessionId);
        var incarnationIdBytes = Encoding.UTF8.GetBytes(
            incarnationId.ToString("D").ToLowerInvariant());
        var recipientDeviceIdBytes = Encoding.UTF8.GetBytes(recipientDeviceId);
        using var hasher = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);

        hasher.AppendData(DomainTags.SessionKeyBlobV2);
        AppendUInt32(hasher, sizeof(ulong));
        AppendUInt64(hasher, issuedAtMs);
        AppendUInt32(hasher, sizeof(uint));
        AppendUInt32(hasher, keyGeneration);
        AppendLengthPrefixed(hasher, sessionIdBytes);
        AppendLengthPrefixed(hasher, incarnationIdBytes);
        AppendLengthPrefixed(hasher, recipientDeviceIdBytes);
        AppendLengthPrefixed(hasher, encryptedSessionKey);

        return hasher.GetHashAndReset();
    }

    private static void AppendLengthPrefixed(
        IncrementalHash hasher,
        ReadOnlySpan<byte> value)
    {
        AppendUInt32(hasher, checked((uint)value.Length));
        hasher.AppendData(value);
    }

    private static void AppendUInt32(IncrementalHash hasher, uint value)
    {
        Span<byte> bytes = stackalloc byte[sizeof(uint)];
        BinaryPrimitives.WriteUInt32BigEndian(bytes, value);
        hasher.AppendData(bytes);
    }

    private static void AppendUInt64(IncrementalHash hasher, ulong value)
    {
        Span<byte> bytes = stackalloc byte[sizeof(ulong)];
        BinaryPrimitives.WriteUInt64BigEndian(bytes, value);
        hasher.AppendData(bytes);
    }
}
