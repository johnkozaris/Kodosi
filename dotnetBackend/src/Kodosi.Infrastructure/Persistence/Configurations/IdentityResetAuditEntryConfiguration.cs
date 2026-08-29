using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class IdentityResetAuditEntryConfiguration
    : IEntityTypeConfiguration<IdentityResetAuditEntry>
{
    public void Configure(EntityTypeBuilder<IdentityResetAuditEntry> builder)
    {
        builder.ToTable("identity_reset_audit");

        builder.HasKey(e => e.Id);
        builder.Property(e => e.Id).HasColumnName("id");

        builder.Property(e => e.UserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("user_id");

        builder.Property(e => e.ResetAt).HasColumnName("reset_at");
        builder.Property(e => e.SessionsAttempted).HasColumnName("sessions_attempted");
        builder.Property(e => e.SessionsEnded).HasColumnName("sessions_ended");
        builder.Property(e => e.DevicesRemoved).HasColumnName("devices_removed");
        builder.Property(e => e.DeviceListsRemoved).HasColumnName("device_lists_removed");


        builder.Property(e => e.ClientIp).HasColumnName("client_ip").HasMaxLength(64);
        builder.Property(e => e.UserAgent).HasColumnName("user_agent").HasMaxLength(512);
        builder.Property(e => e.RemovedDeviceIds)
            .HasColumnName("removed_device_ids")
            .HasColumnType("text[]");
        builder.Property(e => e.EndedSessionIds)
            .HasColumnName("ended_session_ids")
            .HasColumnType("uuid[]");
        builder.Property(e => e.SessionsWithRevokedKeys)
            .HasColumnName("sessions_with_revoked_keys")
            .HasColumnType("uuid[]");
        builder.Property(e => e.AudienceUserIds)
            .HasColumnName("audience_user_ids")
            .HasColumnType("uuid[]");
        builder.Property(e => e.SessionTargetsJson)
            .HasColumnName("session_targets")
            .HasColumnType("jsonb");
        builder.Property(e => e.EndedSessionTargetsJson)
            .HasColumnName("ended_session_targets")
            .HasColumnType("jsonb");
        builder.Property(e => e.IdentityRevision)
            .HasColumnName("identity_revision");
        builder.Property(e => e.RealtimeEnforcedAt)
            .HasColumnName("realtime_enforced_at");


        builder.HasIndex(e => e.UserId);
        builder.HasIndex(e => e.ResetAt);
        builder.HasIndex(e => e.RealtimeEnforcedAt);
    }
}
