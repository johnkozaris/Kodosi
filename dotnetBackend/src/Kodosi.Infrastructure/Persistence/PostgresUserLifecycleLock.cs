using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence;

public sealed class PostgresUserLifecycleLock(KodosiDbContext context)
    : IUserLifecycleLock
{
    private static ReadOnlySpan<byte> LockDomain => "kodosi-user-lifecycle"u8;
    private readonly KodosiDbContext _context = context;

    public async Task AcquireAsync(UserId userId, CancellationToken ct = default)
    {
        if (_context.Database.CurrentTransaction is null)
        {
            throw new InvalidOperationException(
                "User lifecycle lock requires an active database transaction.");
        }

        var (key1, key2) = PostgresAdvisoryKey.Derive(LockDomain, userId.Value);
        _ = await _context.Database.ExecuteSqlInterpolatedAsync(
            $"SELECT pg_advisory_xact_lock({key1}, {key2})",
            ct);
    }
}
