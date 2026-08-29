using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class AccessOverrideAuditEntryConfiguration
    : IEntityTypeConfiguration<AccessOverrideAuditEntry>
{
    public void Configure(EntityTypeBuilder<AccessOverrideAuditEntry> builder)
    {
        builder.ToTable("access_override_audit");

        builder.HasKey(e => e.Id);
        builder.Property(e => e.Id).HasColumnName("id");

        builder.Property(e => e.SessionId)
            .HasConversion(id => id.Value, v => SessionId.From(v))
            .HasColumnName("session_id");

        builder.Property(e => e.ActorUserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("actor_user_id");

        builder.Property(e => e.GranteeUserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("grantee_user_id");

        builder.Property(e => e.Action)
            .HasConversion<string>()
            .HasMaxLength(32)
            .HasColumnName("action");

        builder.Property(e => e.Reason)
            .HasConversion<string>()
            .HasMaxLength(32)
            .HasColumnName("reason");

        builder.Property(e => e.OccurredAt).HasColumnName("occurred_at");
        builder.Property(e => e.SessionIncarnationId)
            .HasColumnName("session_incarnation_id");
        builder.Property(e => e.SessionStartedAt).HasColumnName("session_started_at");
        builder.Property(e => e.ExpectedExpiresAt).HasColumnName("expected_expires_at");
        builder.Property(e => e.ExpectedRevokedAt).HasColumnName("expected_revoked_at");
        builder.Property(e => e.RealtimeEnforcedAt).HasColumnName("realtime_enforced_at");
        builder.Property(e => e.ClientIp).HasColumnName("client_ip").HasMaxLength(64);
        builder.Property(e => e.UserAgent).HasColumnName("user_agent").HasMaxLength(512);

        builder.HasIndex(e => new { e.OccurredAt, e.Id })
            .HasFilter("realtime_enforced_at IS NULL");
        builder.ToTable(table => table.HasCheckConstraint(
            "CK_access_override_audit_pending_expiry_evidence",
            "realtime_enforced_at IS NOT NULL OR (action = 'Revoked' AND reason = 'Expired' AND session_incarnation_id IS NOT NULL AND session_started_at IS NOT NULL AND expected_expires_at IS NOT NULL AND expected_revoked_at IS NOT NULL)"));
    }
}
