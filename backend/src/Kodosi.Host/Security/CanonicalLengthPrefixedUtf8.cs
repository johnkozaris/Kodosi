using System.Buffers.Binary;
using System.Text;

namespace Kodosi.Security;

internal static class CanonicalLengthPrefixedUtf8
{
    public static void Write(Stream stream, string value)
    {
        var bytes = Encoding.UTF8.GetBytes(value);
        Span<byte> length = stackalloc byte[sizeof(uint)];
        BinaryPrimitives.WriteUInt32BigEndian(length, checked((uint)bytes.Length));
        stream.Write(length);
        stream.Write(bytes);
    }
}
