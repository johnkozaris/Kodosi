using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class DeviceRevocationAuditEntryConfiguration
    : IEntityTypeConfiguration<DeviceRevocationAuditEntry>
{
    public void Configure(EntityTypeBuilder<DeviceRevocationAuditEntry> builder)
    {
        builder.ToTable("device_revocation_audit");

        builder.HasKey(e => e.Id);
        builder.Property(e => e.Id).HasColumnName("id");
        builder.Property(e => e.RevocationId).HasColumnName("revocation_id");

        builder.Property(e => e.ActorUserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("actor_user_id");

        builder.Property(e => e.RevokedDeviceId)
            .HasColumnName("revoked_device_id")
            .HasMaxLength(256);

        builder.Property(e => e.SignerDeviceId)
            .HasColumnName("signer_device_id")
            .HasMaxLength(256);

        builder.Property(e => e.NewGeneration).HasColumnName("new_generation");
        builder.Property(e => e.IdentityRevision).HasColumnName("identity_revision");
        builder.Property(e => e.BlobsCascaded).HasColumnName("blobs_cascaded");
        builder.Property(e => e.AffectedSessionIds)
            .HasColumnName("affected_session_ids")
            .HasColumnType("uuid[]");
        builder.Property(e => e.AffectedSessionTargetsJson)
            .HasColumnName("affected_session_targets")
            .HasColumnType("jsonb");
        builder.Property(e => e.OccurredAt).HasColumnName("occurred_at");
        builder.Property(e => e.RealtimeEnforcedAt).HasColumnName("realtime_enforced_at");

        builder.Property(e => e.ClientIp).HasColumnName("client_ip").HasMaxLength(64);
        builder.Property(e => e.UserAgent).HasColumnName("user_agent").HasMaxLength(512);

        builder.HasIndex(e => e.RevocationId);
        builder.HasIndex(e => e.OccurredAt);
    }
}
