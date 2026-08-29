using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text;
using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence;

public sealed class PostgresRecipientDeviceLifecycleLock(KodosiDbContext context)
    : IRecipientDeviceLifecycleLock
{
    private static readonly byte[] KeyDomain =
        Encoding.UTF8.GetBytes("kodosi:recipient-device-lifecycle:v1:");
    private readonly KodosiDbContext _context = context;

    public async Task AcquireAsync(
        IReadOnlyCollection<UserId> recipientUserIds,
        CancellationToken ct = default)
    {
        if (_context.Database.CurrentTransaction is null)
        {
            throw new InvalidOperationException(
                "Recipient device lifecycle lock requires an active database transaction.");
        }

        foreach (var key in recipientUserIds
            .Distinct()
            .OrderBy(userId => userId.Value)
            .Select(ToAdvisoryKey))
        {
            _ = await _context.Database.ExecuteSqlInterpolatedAsync(
                $"SELECT pg_advisory_xact_lock({key})",
                ct);
        }
    }

    private static long ToAdvisoryKey(UserId userId)
    {
        Span<byte> input = stackalloc byte[KeyDomain.Length + 16];
        KeyDomain.CopyTo(input);
        userId.Value.TryWriteBytes(input[KeyDomain.Length..]);
        Span<byte> digest = stackalloc byte[32];
        SHA256.HashData(input, digest);
        return BinaryPrimitives.ReadInt64BigEndian(digest);
    }
}
