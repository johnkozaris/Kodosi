using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class PermissionDecisionAuditStore(
    IDbContextFactory<KodosiDbContext> contextFactory,
    TimeProvider timeProvider) : IPermissionDecisionAuditStore
{
    private readonly IDbContextFactory<KodosiDbContext> _contextFactory = contextFactory;
    private readonly TimeProvider _timeProvider = timeProvider;

    public async Task<PermissionDecisionAdmissionResult> AdmitAsync(
        SessionId sessionId,
        UserId requesterUserId,
        string actionId,
        string auditPayload,
        PermissionDecisionPendingTuple pendingTuple,
        CancellationToken ct = default)
    {
        await using var context = await _contextFactory.CreateDbContextAsync(ct);
        await using var transaction = await context.Database.BeginTransactionAsync(ct);
        await LockKeyAsync(context, sessionId, requesterUserId, actionId, ct);

        var payloadBytes = System.Text.Encoding.UTF8.GetBytes(auditPayload);
        var payloadSha256 = Convert.ToHexStringLower(
            System.Security.Cryptography.SHA256.HashData(payloadBytes));
        var entry = await FindAsync(context, sessionId, requesterUserId, actionId, ct);
        if (entry is not null)
        {
            var outcome = ClassifyAdmission(
                entry,
                pendingTuple,
                payloadSha256,
                payloadBytes.Length);
            if (outcome == PermissionDecisionAdmissionOutcome.PendingDuplicate)
            {
                await context.InputAuditEntries
                    .Where(value => value.Id == entry.Id)
                    .ExecuteUpdateAsync(
                        setters => setters
                            .SetProperty(value => value.DuplicateCount, value => value.DuplicateCount + 1)
                            .SetProperty(value => value.LastDuplicateAt, _ => _timeProvider.GetUtcNow()),
                        ct);
                await transaction.CommitAsync(ct);
            }
            else if (outcome == PermissionDecisionAdmissionOutcome.Rearmed)
            {
                var updated = await context.InputAuditEntries
                    .Where(value =>
                        value.Id == entry.Id
                        && value.Status == InputAuditStatus.Failed)
                    .ExecuteUpdateAsync(
                        setters => setters
                            .SetProperty(value => value.Status, InputAuditStatus.Pending)
                            .SetProperty(value => value.CompletedAt, (DateTimeOffset?)null),
                        ct);
                if (updated != 1)
                {
                    return new PermissionDecisionAdmissionResult(
                        PermissionDecisionAdmissionOutcome.Failed,
                        ReadTuple(entry));
                }
                await transaction.CommitAsync(ct);
            }
            return new PermissionDecisionAdmissionResult(outcome, ReadTuple(entry));
        }

        var newEntry = InputAuditEntry.CreatePendingPermissionDecision(
            sessionId,
            requesterUserId,
            actionId,
            payloadSha256,
            payloadBytes.Length,
            pendingTuple.SessionIncarnationId,
            pendingTuple.SessionIncarnationGeneration,
            pendingTuple.RequestId,
            pendingTuple.RequestGeneration,
            pendingTuple.RequesterDeviceId);
        await context.InputAuditEntries.AddAsync(newEntry, ct);
        await context.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return new PermissionDecisionAdmissionResult(
            PermissionDecisionAdmissionOutcome.Applied,
            pendingTuple);
    }

    public async Task<bool> MarkDispatchFailedAsync(
        SessionId sessionId,
        UserId requesterUserId,
        string actionId,
        PermissionDecisionPendingTuple pendingTuple,
        CancellationToken ct = default)
    {
        await using var context = await _contextFactory.CreateDbContextAsync(ct);
        var completedAt = _timeProvider.GetUtcNow();
        var updated = await context.InputAuditEntries
            .Where(value =>
                value.SessionId == sessionId
                && value.SenderUserId == requesterUserId
                && value.ClientCommandId == actionId
                && value.Kind == InputAuditKind.PermissionDecision
                && value.Status == InputAuditStatus.Pending
                && value.PendingSessionIncarnationId == pendingTuple.SessionIncarnationId
                && value.PendingSessionIncarnationGeneration == pendingTuple.SessionIncarnationGeneration
                && value.PendingRequestId == pendingTuple.RequestId
                && value.PendingRequestGeneration == pendingTuple.RequestGeneration
                && value.PendingRequesterDeviceId == pendingTuple.RequesterDeviceId)
            .ExecuteUpdateAsync(
                setters => setters
                    .SetProperty(value => value.Status, InputAuditStatus.Failed)
                    .SetProperty(value => value.CompletedAt, completedAt),
                ct);
        return updated == 1;
    }

    public async Task<HostActionCompletionResult> CompleteHostActionAsync(
        SessionId sessionId,
        UserId assertedRequesterUserId,
        string assertedActionId,
        PermissionDecisionPendingTuple assertedTuple,
        InputAuditStatus terminalStatus,
        CancellationToken ct = default)
    {
        if (terminalStatus is not (InputAuditStatus.Dispatched or InputAuditStatus.Rejected))
        {
            throw new ArgumentOutOfRangeException(nameof(terminalStatus));
        }

        await using var context = await _contextFactory.CreateDbContextAsync(ct);
        await using var transaction = await context.Database.BeginTransactionAsync(ct);
        await LockKeyAsync(context, sessionId, assertedRequesterUserId, assertedActionId, ct);

        var entry = await FindAsync(
            context,
            sessionId,
            assertedRequesterUserId,
            assertedActionId,
            ct);
        if (entry is null)
        {
            return new HostActionCompletionResult(
                HostActionCompletionOutcome.NotFound,
                assertedRequesterUserId,
                assertedActionId);
        }

        var canonicalTuple = ReadTuple(entry);
        if (entry.Kind != InputAuditKind.PermissionDecision
            || canonicalTuple is null
            || canonicalTuple.Value != assertedTuple)
        {
            return new HostActionCompletionResult(
                HostActionCompletionOutcome.Conflict,
                entry.SenderUserId,
                entry.ClientCommandId,
                canonicalTuple);
        }

        if (entry.Status == terminalStatus)
        {
            return new HostActionCompletionResult(
                HostActionCompletionOutcome.Duplicate,
                entry.SenderUserId,
                entry.ClientCommandId,
                canonicalTuple);
        }
        if (entry.Status is InputAuditStatus.Dispatched or InputAuditStatus.Rejected
            || entry.Status != InputAuditStatus.Pending)
        {
            return new HostActionCompletionResult(
                HostActionCompletionOutcome.Conflict,
                entry.SenderUserId,
                entry.ClientCommandId,
                canonicalTuple);
        }

        var completedAt = _timeProvider.GetUtcNow();
        var pending = context.InputAuditEntries
            .Where(value => value.Id == entry.Id && value.Status == InputAuditStatus.Pending);
        if (terminalStatus == InputAuditStatus.Dispatched)
        {
            await pending.ExecuteUpdateAsync(
                setters => setters
                    .SetProperty(value => value.Status, terminalStatus)
                    .SetProperty(value => value.DispatchedAt, completedAt),
                ct);
        }
        else
        {
            await pending.ExecuteUpdateAsync(
                setters => setters
                    .SetProperty(value => value.Status, terminalStatus)
                    .SetProperty(value => value.CompletedAt, completedAt),
                ct);
        }
        await transaction.CommitAsync(ct);
        return new HostActionCompletionResult(
            HostActionCompletionOutcome.Applied,
            entry.SenderUserId,
            entry.ClientCommandId,
            canonicalTuple);
    }

    private static async Task<InputAuditEntry?> FindAsync(
        KodosiDbContext context,
        SessionId sessionId,
        UserId requesterUserId,
        string actionId,
        CancellationToken ct) =>
        await context.InputAuditEntries
            .AsNoTracking()
            .SingleOrDefaultAsync(value =>
                value.SessionId == sessionId
                && value.SenderUserId == requesterUserId
                && value.ClientCommandId == actionId,
                ct);

    private static Task LockKeyAsync(
        KodosiDbContext context,
        SessionId sessionId,
        UserId requesterUserId,
        string actionId,
        CancellationToken ct) =>
        context.Database.ExecuteSqlInterpolatedAsync(
            $"SELECT pg_advisory_xact_lock(hashtextextended({sessionId.Value.ToString() + requesterUserId.Value.ToString() + actionId}, 0))",
            ct);

    private static PermissionDecisionAdmissionOutcome ClassifyAdmission(
        InputAuditEntry entry,
        PermissionDecisionPendingTuple pendingTuple,
        string payloadSha256,
        int payloadBytesLength)
    {
        if (entry.Kind != InputAuditKind.PermissionDecision
            || ReadTuple(entry) != pendingTuple
            || !string.Equals(entry.PayloadSha256, payloadSha256, StringComparison.Ordinal)
            || entry.PayloadBytesLen != payloadBytesLength)
        {
            return PermissionDecisionAdmissionOutcome.Conflict;
        }
        return entry.Status switch
        {
            InputAuditStatus.Pending => PermissionDecisionAdmissionOutcome.PendingDuplicate,
            InputAuditStatus.Failed => PermissionDecisionAdmissionOutcome.Rearmed,
            InputAuditStatus.Dispatched => PermissionDecisionAdmissionOutcome.AcceptedDuplicate,
            InputAuditStatus.Rejected => PermissionDecisionAdmissionOutcome.RejectedDuplicate,
            _ => PermissionDecisionAdmissionOutcome.Conflict,
        };
    }

    private static PermissionDecisionPendingTuple? ReadTuple(InputAuditEntry entry) =>
        entry.PendingSessionIncarnationId is { } incarnationId
        && entry.PendingSessionIncarnationGeneration is { } generation
        && entry.PendingRequestId is { } requestId
        && entry.PendingRequestGeneration is { } requestGeneration
        && entry.PendingRequesterDeviceId is { } requesterDeviceId
            ? new PermissionDecisionPendingTuple(
                incarnationId,
                generation,
                requestId,
                requestGeneration,
                requesterDeviceId)
            : null;
}
