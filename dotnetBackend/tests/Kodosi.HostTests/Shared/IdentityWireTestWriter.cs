using System.Buffers.Binary;
using System.Text;

namespace Kodosi.HostTests;

internal static class IdentityWireTestWriter
{
    public static void WriteString(Stream stream, string value) =>
        WriteBytes(stream, Encoding.UTF8.GetBytes(value));

    public static void WriteBytes(Stream stream, ReadOnlySpan<byte> value)
    {
        WriteUInt32(stream, (uint)value.Length);
        stream.Write(value);
    }

    public static void WriteUInt32(Stream stream, uint value)
    {
        Span<byte> bytes = stackalloc byte[sizeof(uint)];
        BinaryPrimitives.WriteUInt32BigEndian(bytes, value);
        stream.Write(bytes);
    }

    public static void WriteUInt64(Stream stream, ulong value)
    {
        Span<byte> bytes = stackalloc byte[sizeof(ulong)];
        BinaryPrimitives.WriteUInt64BigEndian(bytes, value);
        stream.Write(bytes);
    }
}
