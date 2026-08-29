using System.Buffers.Binary;
using System.Text;

namespace Kodosi.DomainTests;

internal static class IdentityWireTestBody
{
    private static readonly UTF8Encoding Utf8 = new(false, true);

    public static byte[] DeviceCertificate(
        Guid userId,
        string deviceId = "d1",
        string deviceLabel = "Test device",
        string signerDeviceId = "d1",
        byte[]? kemPublicKey = null,
        byte[]? signingPublicKey = null,
        long issuedAtMs = 1,
        long? expiresAtMs = null,
        string? userIdText = null)
    {
        using var body = new MemoryStream();
        WriteString(body, userIdText ?? userId.ToString());
        WriteString(body, deviceId);
        WriteString(body, deviceLabel);
        WriteString(body, signerDeviceId);
        WriteBytes(body, kemPublicKey ?? new byte[1184]);
        WriteBytes(body, signingPublicKey ?? new byte[1952]);
        WriteUInt64(body, checked((ulong)issuedAtMs));
        WriteUInt64(body, checked((ulong)(expiresAtMs ?? 0)));
        return body.ToArray();
    }

    public static byte[] DeviceList(
        Guid userId,
        long generation = 1,
        string deviceId = "d1",
        string signerDeviceId = "d1",
        long issuedAtMs = 1,
        long? expiresAtMs = null)
    {
        using var body = new MemoryStream();
        WriteString(body, userId.ToString());
        WriteUInt64(body, checked((ulong)generation));
        WriteUInt32(body, 1);
        WriteString(body, deviceId);
        WriteString(body, signerDeviceId);
        WriteString(body, signerDeviceId);
        WriteUInt64(body, checked((ulong)issuedAtMs));
        WriteUInt64(body, checked((ulong)(expiresAtMs ?? 0)));
        return body.ToArray();
    }

    private static void WriteString(Stream stream, string value)
    {
        var bytes = Utf8.GetBytes(value);
        WriteUInt32(stream, checked((uint)bytes.Length));
        stream.Write(bytes);
    }

    private static void WriteBytes(Stream stream, byte[] value)
    {
        WriteUInt32(stream, checked((uint)value.Length));
        stream.Write(value);
    }

    private static void WriteUInt32(Stream stream, uint value)
    {
        Span<byte> bytes = stackalloc byte[4];
        BinaryPrimitives.WriteUInt32BigEndian(bytes, value);
        stream.Write(bytes);
    }

    private static void WriteUInt64(Stream stream, ulong value)
    {
        Span<byte> bytes = stackalloc byte[8];
        BinaryPrimitives.WriteUInt64BigEndian(bytes, value);
        stream.Write(bytes);
    }
}
