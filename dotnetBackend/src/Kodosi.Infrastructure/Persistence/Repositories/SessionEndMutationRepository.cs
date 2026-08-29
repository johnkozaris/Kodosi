using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text;
using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class SessionEndMutationRepository(KodosiDbContext context)
    : ISessionEndMutationRepository
{
    private static readonly byte[] LockDomain =
        Encoding.UTF8.GetBytes("kodosi:session-end-mutation:v1:");
    private readonly KodosiDbContext _context = context;

    public async Task AcquireAsync(
        UserId ownerUserId,
        Guid mutationId,
        CancellationToken ct = default)
    {
        if (_context.Database.CurrentTransaction is null)
        {
            throw new InvalidOperationException(
                "Session end mutation lock requires an active database transaction.");
        }

        Span<byte> input = stackalloc byte[LockDomain.Length + 32];
        LockDomain.CopyTo(input);
        ownerUserId.Value.TryWriteBytes(input[LockDomain.Length..]);
        mutationId.TryWriteBytes(input[(LockDomain.Length + 16)..]);
        Span<byte> digest = stackalloc byte[32];
        SHA256.HashData(input, digest);
        var key = BinaryPrimitives.ReadInt64BigEndian(digest);
        _ = await _context.Database.ExecuteSqlInterpolatedAsync(
            $"SELECT pg_advisory_xact_lock({key})",
            ct);
    }

    public Task<SessionEndMutation?> GetAsync(
        UserId ownerUserId,
        Guid mutationId,
        CancellationToken ct = default) =>
        _context.SessionEndMutations.SingleOrDefaultAsync(
            mutation => mutation.OwnerUserId == ownerUserId
                && mutation.MutationId == mutationId,
            ct);

    public Task AddAsync(
        SessionEndMutation mutation,
        CancellationToken ct = default) =>
        _context.SessionEndMutations.AddAsync(mutation, ct).AsTask();
}
