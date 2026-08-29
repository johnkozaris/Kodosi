using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text;
using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class RoomMutationReceiptRepository(KodosiDbContext context)
    : IRoomMutationReceiptRepository
{
    private static readonly byte[] LockDomain =
        Encoding.UTF8.GetBytes("kodosi:room-mutation-receipt:v1:");

    public async Task AcquireAsync(
        UserId actorUserId,
        RoomMutationOperation operation,
        Guid requestId,
        CancellationToken ct = default)
    {
        if (context.Database.CurrentTransaction is null)
        {
            throw new InvalidOperationException(
                "Room mutation receipt lock requires an active database transaction.");
        }

        Span<byte> input = stackalloc byte[LockDomain.Length + 36];
        LockDomain.CopyTo(input);
        actorUserId.Value.TryWriteBytes(input[LockDomain.Length..]);
        BinaryPrimitives.WriteInt32BigEndian(
            input.Slice(LockDomain.Length + 16, 4),
            (int)operation);
        requestId.TryWriteBytes(input[(LockDomain.Length + 20)..]);
        Span<byte> digest = stackalloc byte[32];
        SHA256.HashData(input, digest);
        var key = BinaryPrimitives.ReadInt64BigEndian(digest);
        _ = await context.Database.ExecuteSqlInterpolatedAsync(
            $"SELECT pg_advisory_xact_lock({key})",
            ct);
    }

    public Task<RoomMutationReceipt?> GetAsync(
        UserId actorUserId,
        RoomMutationOperation operation,
        Guid requestId,
        CancellationToken ct = default) =>
        context.RoomMutationReceipts.SingleOrDefaultAsync(
            receipt => receipt.ActorUserId == actorUserId
                && receipt.Operation == operation
                && receipt.RequestId == requestId,
            ct);

    public Task<RoomMutationReceipt?> FindAsync(
        UserId actorUserId,
        RoomMutationOperation operation,
        Guid requestId,
        CancellationToken ct = default) =>
        context.RoomMutationReceipts
            .AsNoTracking()
            .SingleOrDefaultAsync(
                receipt => receipt.ActorUserId == actorUserId
                    && receipt.Operation == operation
                    && receipt.RequestId == requestId,
                ct);

    public Task AddSessionEffectsAsync(
        IReadOnlyCollection<RoomMutationSessionEffect> effects,
        CancellationToken ct = default) =>
        context.RoomMutationSessionEffects.AddRangeAsync(effects, ct);

    public async Task<IReadOnlyList<RoomMutationSessionEffect>> GetSessionEffectsAsync(
        UserId actorUserId,
        Guid requestId,
        CancellationToken ct = default) =>
        await context.RoomMutationSessionEffects
            .AsNoTracking()
            .Where(effect => effect.ActorUserId == actorUserId
                && effect.Operation == RoomMutationOperation.RemoveMember
                && effect.RequestId == requestId)
            .OrderBy(effect => effect.SessionId)
            .ThenBy(effect => effect.SessionIncarnationId)
            .ToListAsync(ct);

    public Task AddAsync(
        RoomMutationReceipt receipt,
        CancellationToken ct = default) =>
        context.RoomMutationReceipts.AddAsync(receipt, ct).AsTask();
}
