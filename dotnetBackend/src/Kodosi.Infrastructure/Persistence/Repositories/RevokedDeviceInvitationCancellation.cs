using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class RevokedDeviceInvitationCancellation(KodosiDbContext context) : IRevokedDeviceInvitationCancellation
{
    public async Task<IReadOnlyList<UserId>> CancelPendingAsync(
        UserId ownerUserId,
        IReadOnlyCollection<string> revokedDeviceIds,
        DateTimeOffset cancelledAt,
        CancellationToken ct = default)
    {
        if (context.Database.CurrentTransaction is null)
        {
            throw new InvalidOperationException("Invitation cancellation requires the device revocation transaction.");
        }
        if (revokedDeviceIds.Count == 0) return [];
        // Invitation creation and acceptance acquire this owner's user lifecycle lock,
        // already held by device revocation, so no affected proposal can race this batch.
        var pending = await context.RoomInvitations
            .Where(invitation => invitation.InvitedByUserId == ownerUserId
                && invitation.Status == RoomInvitationStatus.Pending
                && (revokedDeviceIds.Contains(invitation.ProposalSignerDeviceId)
                    || revokedDeviceIds.Contains(invitation.ProposedRosterSignerDeviceId)))
            .ToListAsync(ct);
        foreach (var invitation in pending)
        {
            invitation.CancelByInviter(ownerUserId, cancelledAt);
        }
        return pending.Select(invitation => invitation.InviteeUserId).Distinct().ToList();
    }
}
