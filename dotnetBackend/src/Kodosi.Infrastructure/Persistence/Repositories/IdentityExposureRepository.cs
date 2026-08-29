using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class IdentityExposureRepository(KodosiDbContext context)
    : IIdentityExposureRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task RecordAsync(
        UserId identityOwnerUserId,
        UserId recipientUserId,
        DateTimeOffset exposedAt,
        CancellationToken ct = default)
    {
        if (identityOwnerUserId == recipientUserId)
        {
            return;
        }
        var existing = await _context.IdentityExposures.FindAsync(
            [identityOwnerUserId, recipientUserId],
            ct);
        if (existing is null)
        {
            await _context.IdentityExposures.AddAsync(
                IdentityExposure.Create(identityOwnerUserId, recipientUserId, exposedAt),
                ct);
        }
        else
        {
            existing.MarkExposed(exposedAt);
        }
    }

    public async Task<IReadOnlyList<UserId>> GetHistoricalPeerUserIdsAsync(
        UserId userId,
        CancellationToken ct = default)
    {
        var userIds = await _context.Database
            .SqlQuery<Guid>($"""
                SELECT DISTINCT exposure.user_id AS "Value"
                FROM (
                    SELECT recipient_user_id AS user_id
                    FROM identity_exposures
                    WHERE identity_owner_user_id = {userId.Value}

                    UNION

                    SELECT identity_owner_user_id AS user_id
                    FROM identity_exposures
                    WHERE recipient_user_id = {userId.Value}

                    UNION

                    SELECT CASE
                        WHEN friendship.actor_user_id = {userId.Value}
                            THEN friendship.other_user_id
                        ELSE friendship.actor_user_id
                    END AS user_id
                    FROM friendship_audit AS friendship
                    WHERE friendship.action = 'RequestAccepted'
                      AND (
                          friendship.actor_user_id = {userId.Value}
                          OR friendship.other_user_id = {userId.Value}
                      )

                    UNION

                    SELECT peer.user_id
                    FROM room_members AS mine
                    INNER JOIN room_members AS peer
                        ON peer.room_id = mine.room_id
                       AND (mine.revoked_at IS NULL OR peer.created_at <= mine.revoked_at)
                       AND (peer.revoked_at IS NULL OR mine.created_at <= peer.revoked_at)
                    WHERE mine.user_id = {userId.Value}
                      AND peer.user_id <> {userId.Value}

                    UNION

                    SELECT CASE
                        WHEN session.owner_user_id = {userId.Value}
                            THEN access_override.actor_user_id
                        ELSE session.owner_user_id
                    END AS user_id
                    FROM session_access_overrides AS access_override
                    INNER JOIN sessions AS session
                        ON session.id = access_override.session_id
                    WHERE (
                        session.owner_user_id = {userId.Value}
                        OR access_override.actor_user_id = {userId.Value}
                    )
                      AND session.owner_user_id <> access_override.actor_user_id
                ) AS exposure
                ORDER BY "Value"
                """)
            .ToListAsync(ct);
        return userIds.Select(UserId.From).ToList();
    }

    public async Task<IReadOnlyList<IdentityLifecycleProjection>> GetLifecycleSnapshotForRecipientAsync(
        UserId recipientUserId,
        CancellationToken ct = default)
    {
        var rows = await _context.Database
            .SqlQuery<IdentityLifecycleProjectionRow>($"""
                SELECT
                    target.id AS "UserId",
                    target.identity_revision AS "IdentityRevision",
                    target.identity_incarnation_id AS "IdentityIncarnationId",
                    COALESCE(MAX(device_list.generation), 0) AS "Generation"
                FROM users AS target
                LEFT JOIN user_device_lists AS device_list
                    ON device_list.user_id = target.id
                WHERE target.id IN (
                    SELECT exposure.identity_owner_user_id
                    FROM identity_exposures AS exposure
                    WHERE exposure.recipient_user_id = {recipientUserId.Value}

                    UNION

                    SELECT reset.user_id
                    FROM identity_reset_audit AS reset
                    WHERE {recipientUserId.Value} = ANY(reset.audience_user_ids)
                )
                GROUP BY
                    target.id,
                    target.identity_revision,
                    target.identity_incarnation_id
                ORDER BY target.id
                """)
            .ToListAsync(ct);
        return rows.Select(row => new IdentityLifecycleProjection(
            UserId.From(row.UserId),
            row.IdentityRevision,
            row.IdentityIncarnationId,
            row.Generation)).ToList();
    }

    private sealed record IdentityLifecycleProjectionRow(
        Guid UserId,
        long IdentityRevision,
        Guid? IdentityIncarnationId,
        long Generation);
}
