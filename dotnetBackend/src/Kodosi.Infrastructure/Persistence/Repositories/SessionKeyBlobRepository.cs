using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class SessionKeyBlobRepository(KodosiDbContext context) : ISessionKeyBlobRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task<SessionKeyBlob?> GetForDeviceAsync(
        SessionId sessionId,
        string recipientDeviceId,
        CancellationToken ct = default)
        => await _context.SessionKeyBlobs
            .FirstOrDefaultAsync(b => b.SessionId == sessionId
                && b.RecipientDeviceId == recipientDeviceId, ct);

    public async Task AddRangeAsync(
        IReadOnlyList<SessionKeyBlob> blobs,
        CancellationToken ct = default)
        => await _context.SessionKeyBlobs.AddRangeAsync(blobs, ct);

    public Task DeleteForSessionAsync(
        SessionId sessionId,
        CancellationToken ct = default)



        => _context.SessionKeyBlobs
            .Where(b => b.SessionId == sessionId)
            .ExecuteDeleteAsync(ct);

    public async Task<IReadOnlyList<DeviceRevocationSessionTarget>>
        GetSessionTargetsForRecipientDevicesAsync(
            IReadOnlyCollection<string> recipientDeviceIds,
            CancellationToken ct = default)
    {
        if (recipientDeviceIds.Count == 0)
        {
            return [];
        }

        var current = await _context.SessionKeyBlobs
            .Where(blob => recipientDeviceIds.Contains(blob.RecipientDeviceId))
            .Join(
                _context.Sessions,
                blob => blob.SessionId,
                session => session.Id,
                (_, session) => new
                {
                    session.Id,
                    session.IncarnationId,
                })
            .Distinct()
            .OrderBy(target => target.Id)
            .ThenBy(target => target.IncarnationId)
            .ToListAsync(ct);
        return current
            .Select(target => new DeviceRevocationSessionTarget(
                target.Id,
                target.IncarnationId))
            .ToList();
    }

    public async Task<IReadOnlyList<SessionId>> GetSessionIdsForRecipientDevicesAsync(
        IReadOnlyCollection<string> recipientDeviceIds,
        CancellationToken ct = default)
    {
        if (recipientDeviceIds.Count == 0)
        {
            return [];
        }

        return await _context.SessionKeyBlobs
            .Where(blob => recipientDeviceIds.Contains(blob.RecipientDeviceId))
            .Select(blob => blob.SessionId)
            .Distinct()
            .ToListAsync(ct);
    }

    public Task<int> DeleteForRecipientDevicesAsync(
        IReadOnlyCollection<string> recipientDeviceIds,
        CancellationToken ct = default)
    {
        if (recipientDeviceIds.Count == 0)
        {
            return Task.FromResult(0);
        }

        return _context.SessionKeyBlobs
            .Where(b => recipientDeviceIds.Contains(b.RecipientDeviceId))
            .ExecuteDeleteAsync(ct);
    }
}
