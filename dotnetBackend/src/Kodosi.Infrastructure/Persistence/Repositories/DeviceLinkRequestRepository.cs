using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class DeviceLinkRequestRepository(KodosiDbContext context) : IDeviceLinkRequestRepository
{



    private static readonly TimeSpan TerminalRetention = TimeSpan.FromHours(24);

    private readonly KodosiDbContext _context = context;

    public async Task AddAsync(DeviceLinkRequest request, CancellationToken ct = default)
        => await _context.DeviceLinkRequests.AddAsync(request, ct);

    public Task<DeviceLinkRequest?> GetByDeviceCodeAsync(
        string deviceCode, CancellationToken ct = default)
        => _context.DeviceLinkRequests
            .FirstOrDefaultAsync(r => r.DeviceCode == deviceCode, ct);

    public Task<DeviceLinkRequest?> GetByUserCodeAsync(
        string userCode, CancellationToken ct = default)
        => _context.DeviceLinkRequests
            .FirstOrDefaultAsync(r => r.UserCode == userCode, ct);

    public async Task<bool> IsUserCodeAvailableAsync(
        string userCode, CancellationToken ct = default)
        => !await _context.DeviceLinkRequests
            .AsNoTracking()
            .AnyAsync(r => r.UserCode == userCode, ct);

    public async Task<IReadOnlyList<DeviceLinkRequest>> ListPendingForUserAsync(
        UserId userId,
        DateTimeOffset now,
        CancellationToken ct = default) =>
        await _context.DeviceLinkRequests
            .AsNoTracking()
            .Where(r =>
                r.UserId == userId
                && r.ApprovedAt == null
                && r.CancelledAt == null
                && r.ExpiresAt > now)
            .OrderBy(r => r.CreatedAt)
            .ToListAsync(ct);

    public void Update(DeviceLinkRequest request)
        => _context.DeviceLinkRequests.Update(request);

    public async Task<DeviceLinkCancelOutcome> CancelPendingAsync(
        string userCode,
        UserId userId,
        DateTimeOffset cancelledAt,
        CancellationToken ct = default)
    {
        var updated = await _context.DeviceLinkRequests
            .Where(request =>
                request.UserCode == userCode
                && request.UserId == userId
                && request.ApprovedAt == null
                && request.CancelledAt == null
                && request.ExpiresAt > cancelledAt)
            .ExecuteUpdateAsync(
                setters => setters.SetProperty(request => request.CancelledAt, cancelledAt),
                ct);
        if (updated == 1)
        {
            return DeviceLinkCancelOutcome.Cancelled;
        }

        var state = await _context.DeviceLinkRequests
            .AsNoTracking()
            .Where(request => request.UserCode == userCode && request.UserId == userId)
            .Select(request => new
            {
                request.ApprovedAt,
                request.CancelledAt,
                request.ExpiresAt,
            })
            .SingleOrDefaultAsync(ct);
        if (state is null)
        {
            return DeviceLinkCancelOutcome.NotFound;
        }
        if (state.CancelledAt.HasValue)
        {
            return DeviceLinkCancelOutcome.AlreadyCancelled;
        }
        if (state.ApprovedAt.HasValue)
        {
            return DeviceLinkCancelOutcome.Approved;
        }
        return state.ExpiresAt <= cancelledAt
            ? DeviceLinkCancelOutcome.Expired
            : DeviceLinkCancelOutcome.NotFound;
    }

    public async Task<DeviceLinkAcknowledgeOutcome> AcknowledgeApprovedAsync(
        Guid requestId,
        UserId userId,
        string deviceId,
        DateTimeOffset now,
        CancellationToken ct = default)
    {
        var updated = await _context.DeviceLinkRequests
            .Where(r =>
                r.Id == requestId
                && r.UserId == userId
                && r.DeviceId == deviceId
                && r.ApprovedAt != null
                && r.AcknowledgedAt == null
                && r.CancelledAt == null
                && r.ResultExpiresAt > now)
            .ExecuteUpdateAsync(
                setters => setters.SetProperty(r => r.AcknowledgedAt, now),
                ct);
        if (updated == 1)
        {
            return DeviceLinkAcknowledgeOutcome.Acknowledged;
        }

        var state = await _context.DeviceLinkRequests
            .AsNoTracking()
            .Where(r =>
                r.Id == requestId
                && r.UserId == userId
                && r.DeviceId == deviceId)
            .Select(r => new { r.AcknowledgedAt, r.CancelledAt })
            .SingleOrDefaultAsync(ct);
        if (state?.AcknowledgedAt is not null)
        {
            return DeviceLinkAcknowledgeOutcome.AlreadyAcknowledged;
        }
        return state?.CancelledAt is not null
            ? DeviceLinkAcknowledgeOutcome.Invalidated
            : DeviceLinkAcknowledgeOutcome.Unavailable;
    }

    public Task<int> InvalidateApprovedBeforeGenerationAsync(
        UserId userId,
        long committedGeneration,
        Guid? excludingRequestId,
        DateTimeOffset invalidatedAt,
        CancellationToken ct = default) =>
        _context.DeviceLinkRequests
            .Where(request =>
                request.UserId == userId
                && request.Id != excludingRequestId
                && request.ApprovedAt != null
                && request.AcknowledgedAt == null
                && request.CancelledAt == null
                && request.DeviceListGeneration < committedGeneration)
            .ExecuteUpdateAsync(
                setters => setters.SetProperty(request => request.CancelledAt, invalidatedAt),
                ct);

    public Task<int> InvalidateOutstandingForUserAsync(
        UserId userId,
        DateTimeOffset invalidatedAt,
        CancellationToken ct = default) =>
        _context.DeviceLinkRequests
            .Where(request =>
                request.UserId == userId
                && request.AcknowledgedAt == null
                && request.CancelledAt == null)
            .ExecuteUpdateAsync(
                setters => setters.SetProperty(request => request.CancelledAt, invalidatedAt),
                ct);

    public Task<int> DeleteStaleAsync(DateTimeOffset now, CancellationToken ct = default)
    {
        var cutoff = now.Subtract(TerminalRetention);
        return _context.DeviceLinkRequests
            .Where(r =>
                (r.CancelledAt != null && r.CancelledAt <= cutoff)
                || (r.CancelledAt == null
                    && r.ApprovedAt == null
                    && r.ExpiresAt <= cutoff)
                || (r.CancelledAt == null
                    && r.ApprovedAt != null
                    && r.AcknowledgedAt == null
                    && r.ResultExpiresAt <= cutoff)
                || (r.AcknowledgedAt != null
                    && r.AcknowledgedAt <= cutoff))
            .ExecuteDeleteAsync(ct);
    }
}
