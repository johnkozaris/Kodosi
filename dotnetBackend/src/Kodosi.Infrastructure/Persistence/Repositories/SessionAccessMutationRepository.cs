using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text;
using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class SessionAccessMutationRepository(KodosiDbContext context)
    : ISessionAccessMutationRepository
{
    private static readonly byte[] LockDomain =
        Encoding.UTF8.GetBytes("kodosi:session-access-mutation:v1:");

    public async Task AcquireAsync(
        UserId requesterUserId,
        Guid mutationId,
        CancellationToken ct = default)
    {
        if (context.Database.CurrentTransaction is null)
        {
            throw new InvalidOperationException(
                "Session access mutation lock requires an active database transaction.");
        }
        Span<byte> input = stackalloc byte[LockDomain.Length + 32];
        LockDomain.CopyTo(input);
        requesterUserId.Value.TryWriteBytes(input[LockDomain.Length..]);
        mutationId.TryWriteBytes(input[(LockDomain.Length + 16)..]);
        Span<byte> digest = stackalloc byte[32];
        SHA256.HashData(input, digest);
        var key = BinaryPrimitives.ReadInt64BigEndian(digest);
        _ = await context.Database.ExecuteSqlInterpolatedAsync(
            $"SELECT pg_advisory_xact_lock({key})",
            ct);
    }

    public Task<SessionAccessMutation?> GetAsync(
        UserId requesterUserId,
        Guid mutationId,
        CancellationToken ct = default) =>
        context.SessionAccessMutations.SingleOrDefaultAsync(
            mutation => mutation.RequesterUserId == requesterUserId
                && mutation.MutationId == mutationId,
            ct);

    public Task AddAsync(
        SessionAccessMutation mutation,
        CancellationToken ct = default) =>
        context.SessionAccessMutations.AddAsync(mutation, ct).AsTask();
}
