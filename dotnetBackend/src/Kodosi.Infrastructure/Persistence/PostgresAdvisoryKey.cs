using System.Buffers.Binary;
using System.Security.Cryptography;

namespace Kodosi.Infrastructure.Persistence;

internal static class PostgresAdvisoryKey
{
    public static (int First, int Second) Derive(ReadOnlySpan<byte> domain, Guid id)
    {
        if (domain.IsEmpty)
        {
            throw new ArgumentException("Advisory-lock domain is required.", nameof(domain));
        }

        Span<byte> idBytes = stackalloc byte[16];
        id.TryWriteBytes(idBytes);
        Span<byte> input = stackalloc byte[domain.Length + idBytes.Length];
        domain.CopyTo(input);
        idBytes.CopyTo(input[domain.Length..]);
        Span<byte> digest = stackalloc byte[32];
        SHA256.HashData(input, digest);
        return (
            BinaryPrimitives.ReadInt32BigEndian(digest[..4]),
            BinaryPrimitives.ReadInt32BigEndian(digest[4..8]));
    }
}
