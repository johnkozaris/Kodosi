using System.Buffers.Binary;
using System.Text;
using Kodosi.Domain;

namespace Kodosi.HostTests;

internal static class TestDeviceCertificate
{
    public static UserDevice CreateDevice(
        UserId userId,
        string deviceId,
        byte[]? kemPublicKey = null,
        byte[]? signingPublicKey = null,
        string? deviceLabel = null,
        string? signerDeviceId = null,
        DateTimeOffset? issuedAt = null,
        DateTimeOffset? expiresAt = null)
    {
        var effectiveIssuedAt = issuedAt ?? DateTimeOffset.UtcNow.AddMinutes(-1);
        return UserDevice.CreateCertified(
            userId,
            deviceId,
            Body(
                userId,
                deviceId,
                deviceLabel ?? "Test device",
                signerDeviceId ?? deviceId,
                kemPublicKey ?? new byte[1184],
                signingPublicKey ?? new byte[1952],
                effectiveIssuedAt.ToUnixTimeMilliseconds(),
                expiresAt?.ToUnixTimeMilliseconds()),
            Signature(),
            effectiveIssuedAt);
    }

    public static byte[] Body(
        UserId userId,
        string deviceId,
        string deviceLabel,
        string signerDeviceId,
        byte[] kemPublicKey,
        byte[] signingPublicKey,
        long issuedAtMs,
        long? expiresAtMs = null)
    {
        using var body = new MemoryStream();
        WriteString(body, userId.Value.ToString());
        WriteString(body, deviceId);
        WriteString(body, deviceLabel);
        WriteString(body, signerDeviceId);
        WriteBytes(body, kemPublicKey);
        WriteBytes(body, signingPublicKey);
        WriteUInt64(body, checked((ulong)issuedAtMs));
        WriteUInt64(body, checked((ulong)(expiresAtMs ?? 0)));
        return body.ToArray();
    }

    public static byte[] Signature(byte marker = 1)
    {
        var signature = new byte[IdentityWireFormat.MlDsa65SignatureLength];
        signature[0] = marker;
        return signature;
    }

    private static void WriteString(Stream stream, string value) =>
        WriteBytes(stream, Encoding.UTF8.GetBytes(value));

    private static void WriteBytes(Stream stream, ReadOnlySpan<byte> value)
    {
        Span<byte> length = stackalloc byte[sizeof(uint)];
        BinaryPrimitives.WriteUInt32BigEndian(length, checked((uint)value.Length));
        stream.Write(length);
        stream.Write(value);
    }

    private static void WriteUInt64(Stream stream, ulong value)
    {
        Span<byte> bytes = stackalloc byte[sizeof(ulong)];
        BinaryPrimitives.WriteUInt64BigEndian(bytes, value);
        stream.Write(bytes);
    }
}
