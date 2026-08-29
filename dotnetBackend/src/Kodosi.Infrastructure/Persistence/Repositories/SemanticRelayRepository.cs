using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;
using Npgsql;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class SemanticRelayRepository(
    IDbContextFactory<KodosiDbContext> dbFactory,
    TimeProvider timeProvider) : ISemanticRelayRepository
{
    public async Task<SemanticRequestClaim> ClaimRequestAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        Guid requestId,
        string mode,
        string payloadSha256,
        CancellationToken ct = default)
    {
        await using var db = await dbFactory.CreateDbContextAsync(ct);
        var existing = await db.SemanticRelayRequests.SingleOrDefaultAsync(
            value => value.RequesterUserId == requesterUserId
                && value.RequestId == requestId,
            ct);
        if (existing is not null)
        {
            return new SemanticRequestClaim(
                existing.Matches(
                    sessionId,
                    incarnationId,
                    requesterUserId,
                    requesterDeviceId,
                    mode,
                    payloadSha256)
                    ? SemanticRequestClaimKind.ExactDuplicate
                    : SemanticRequestClaimKind.Conflict,
                existing);
        }

        var created = SemanticRelayRequest.Create(
            sessionId,
            incarnationId,
            requesterUserId,
            requesterDeviceId,
            requestId,
            mode,
            payloadSha256,
            timeProvider.GetUtcNow());
        db.SemanticRelayRequests.Add(created);
        try
        {
            await db.SaveChangesAsync(ct);
            return new SemanticRequestClaim(SemanticRequestClaimKind.Created, created);
        }
        catch (DbUpdateException exception) when (
            exception.InnerException is PostgresException
            {
                SqlState: PostgresErrorCodes.UniqueViolation,
                ConstraintName: "IX_semantic_relay_requests_requester_user_id_request_id",
            })
        {
            db.ChangeTracker.Clear();
            existing = await db.SemanticRelayRequests.SingleAsync(
                value => value.RequesterUserId == requesterUserId
                    && value.RequestId == requestId,
                ct);
            return new SemanticRequestClaim(
                existing.Matches(
                    sessionId,
                    incarnationId,
                    requesterUserId,
                    requesterDeviceId,
                    mode,
                    payloadSha256)
                    ? SemanticRequestClaimKind.ExactDuplicate
                    : SemanticRequestClaimKind.Conflict,
                existing);
        }
    }

    public async Task<SemanticRequestClaim?> FindExactRequestAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        Guid requestId,
        string mode,
        string payloadSha256,
        CancellationToken ct = default)
    {
        await using var db = await dbFactory.CreateDbContextAsync(ct);
        var request = await db.SemanticRelayRequests.SingleOrDefaultAsync(
            value => value.RequesterUserId == requesterUserId
                && value.RequestId == requestId,
            ct);
        if (request is null)
        {
            return null;
        }
        return new SemanticRequestClaim(
            request.Matches(
                sessionId,
                incarnationId,
                requesterUserId,
                requesterDeviceId,
                mode,
                payloadSha256)
                ? SemanticRequestClaimKind.ExactDuplicate
                : SemanticRequestClaimKind.Conflict,
            request);
    }

    public async Task MarkDispatchedAsync(Guid requestRowId, CancellationToken ct = default)
    {
        await using var db = await dbFactory.CreateDbContextAsync(ct);
        var now = timeProvider.GetUtcNow();
        _ = await db.SemanticRelayRequests
            .Where(value => value.Id == requestRowId
                && value.State == SemanticRequestState.Pending)
            .ExecuteUpdateAsync(
                updates => updates
                    .SetProperty(
                        value => value.State,
                        SemanticRequestState.Dispatched)
                    .SetProperty(value => value.UpdatedAt, now),
                ct);
    }

    public async Task<bool> StoreReceiptAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        Guid requestId,
        string mode,
        string payloadSha256,
        string outcome,
        UserId ownerUserId,
        string ownerDeviceId,
        string signature,
        CancellationToken ct = default)
    {
        await using var db = await dbFactory.CreateDbContextAsync(ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var request = await db.SemanticRelayRequests.SingleOrDefaultAsync(
            value => value.RequesterUserId == requesterUserId
                && value.RequestId == requestId,
            ct);
        if (request is null
            || !request.Matches(
                sessionId,
                incarnationId,
                requesterUserId,
                requesterDeviceId,
                mode,
                payloadSha256))
        {
            return false;
        }
        _ = await db.Database.ExecuteSqlInterpolatedAsync(
            $"SELECT 1 FROM semantic_relay_requests WHERE id = {request.Id} FOR UPDATE",
            ct);
        await db.Entry(request).ReloadAsync(ct);
        var existing = await db.SemanticRelayReceipts.SingleOrDefaultAsync(
            value => value.RequestRowId == request.Id,
            ct);
        if (existing is not null)
        {
            return existing.SessionId == sessionId
                && existing.IncarnationId == incarnationId
                && existing.RequesterUserId == requesterUserId
                && existing.RequesterDeviceId == requesterDeviceId
                && existing.RequestId == requestId
                && existing.Mode == mode
                && existing.PayloadSha256 == payloadSha256
                && existing.Outcome == outcome
                && existing.OwnerUserId == ownerUserId
                && existing.OwnerDeviceId == ownerDeviceId;
        }
        db.SemanticRelayReceipts.Add(SemanticRelayReceipt.Create(
            request.Id,
            sessionId,
            incarnationId,
            requesterUserId,
            requesterDeviceId,
            requestId,
            mode,
            payloadSha256,
            outcome,
            ownerUserId,
            ownerDeviceId,
            signature,
            timeProvider.GetUtcNow()));
        request.MarkReceiptStored(timeProvider.GetUtcNow());
        await db.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return true;
    }

    public async Task<IReadOnlyList<SemanticRelayReceipt>> ListPendingReceiptsForDeviceAsync(
        UserId requesterUserId,
        string requesterDeviceId,
        int limit,
        SemanticReceiptCursor? cursor = null,
        CancellationToken ct = default)
    {
        ArgumentOutOfRangeException.ThrowIfLessThan(limit, 1);
        await using var db = await dbFactory.CreateDbContextAsync(ct);
        var query = db.SemanticRelayReceipts
            .AsNoTracking()
            .Where(value => value.RequesterUserId == requesterUserId
                && value.RequesterDeviceId == requesterDeviceId
                && value.AcknowledgedAt == null);
        if (cursor is not null)
        {
            query = query.Where(value => value.StoredAt > cursor.StoredAt
                || (value.StoredAt == cursor.StoredAt && value.Id.CompareTo(cursor.Id) > 0));
        }
        return await query
            .OrderBy(value => value.StoredAt)
            .ThenBy(value => value.Id)
            .Take(limit)
            .ToListAsync(ct);
    }

    public async Task<IReadOnlyList<SemanticRelayReceipt>> ListPendingReceiptsAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        int limit,
        CancellationToken ct = default)
    {
        ArgumentOutOfRangeException.ThrowIfLessThan(limit, 1);
        await using var db = await dbFactory.CreateDbContextAsync(ct);
        return await db.SemanticRelayReceipts
            .AsNoTracking()
            .Where(value => value.SessionId == sessionId
                && value.IncarnationId == incarnationId
                && value.RequesterUserId == requesterUserId
                && value.RequesterDeviceId == requesterDeviceId
                && value.AcknowledgedAt == null)
            .OrderBy(value => value.StoredAt)
            .ThenBy(value => value.Id)
            .Take(limit)
            .ToListAsync(ct);
    }

    public async Task<bool> AcknowledgeReceiptAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        Guid requestId,
        CancellationToken ct = default)
    {
        await using var db = await dbFactory.CreateDbContextAsync(ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var receipt = await db.SemanticRelayReceipts.SingleOrDefaultAsync(
            value => value.SessionId == sessionId
                && value.IncarnationId == incarnationId
                && value.RequesterUserId == requesterUserId
                && value.RequesterDeviceId == requesterDeviceId
                && value.RequestId == requestId,
            ct);
        if (receipt is null)
        {
            return false;
        }
        var request = await db.SemanticRelayRequests.SingleAsync(
            value => value.Id == receipt.RequestRowId,
            ct);
        var now = timeProvider.GetUtcNow();
        receipt.Acknowledge(now);
        request.MarkAcknowledged(now);
        await db.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return true;
    }

    public async Task<int> DeleteAcknowledgedBeforeAsync(
        DateTimeOffset cutoff,
        int limit,
        CancellationToken ct = default)
    {
        ArgumentOutOfRangeException.ThrowIfLessThan(limit, 1);
        await using var db = await dbFactory.CreateDbContextAsync(ct);
        var requestIds = await db.SemanticRelayReceipts
            .AsNoTracking()
            .Where(receipt => receipt.AcknowledgedAt != null
                && receipt.AcknowledgedAt <= cutoff)
            .OrderBy(receipt => receipt.AcknowledgedAt)
            .ThenBy(receipt => receipt.Id)
            .Select(receipt => receipt.RequestRowId)
            .Take(limit)
            .ToListAsync(ct);
        if (requestIds.Count == 0)
        {
            return 0;
        }
        return await db.SemanticRelayRequests
            .Where(request => requestIds.Contains(request.Id)
                && request.State == SemanticRequestState.Acknowledged)
            .ExecuteDeleteAsync(ct);
    }
}
