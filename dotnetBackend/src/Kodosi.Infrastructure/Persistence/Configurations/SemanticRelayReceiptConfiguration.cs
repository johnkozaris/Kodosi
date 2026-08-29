using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class SemanticRelayReceiptConfiguration
    : IEntityTypeConfiguration<SemanticRelayReceipt>
{
    public void Configure(EntityTypeBuilder<SemanticRelayReceipt> builder)
    {
        builder.ToTable(
            "semantic_relay_receipts",
            table =>
            {
                table.HasCheckConstraint(
                    "CK_semantic_relay_receipts_mode",
                    "mode IN ('queue', 'steer', 'stopAndSend')");
                table.HasCheckConstraint(
                    "CK_semantic_relay_receipts_outcome",
                    "outcome IN ('injected', 'cancelled', 'deliveryUnknown')");
                table.HasCheckConstraint(
                    "CK_semantic_relay_receipts_owner_is_requester",
                    "owner_user_id = requester_user_id");
                table.HasCheckConstraint(
                    "CK_semantic_relay_receipts_payload_sha256",
                    "payload_sha256 ~ '^[0-9a-f]{64}$'");
            });
        builder.HasKey(value => value.Id);
        builder.Property(value => value.Id).HasColumnName("id");
        builder.Property(value => value.RequestRowId).HasColumnName("request_row_id");
        builder.Property(value => value.SessionId)
            .HasConversion(value => value.Value, value => SessionId.From(value))
            .HasColumnName("session_id");
        builder.Property(value => value.IncarnationId).HasColumnName("incarnation_id");
        builder.Property(value => value.RequesterUserId)
            .HasConversion(value => value.Value, value => UserId.From(value))
            .HasColumnName("requester_user_id");
        builder.Property(value => value.RequesterDeviceId)
            .HasColumnName("requester_device_id").HasMaxLength(256);
        builder.Property(value => value.RequestId).HasColumnName("request_id");
        builder.Property(value => value.Mode).HasColumnName("mode").HasMaxLength(32);
        builder.Property(value => value.PayloadSha256)
            .HasColumnName("payload_sha256").HasMaxLength(64).IsFixedLength();
        builder.Property(value => value.Outcome).HasColumnName("outcome").HasMaxLength(32);
        builder.Property(value => value.OwnerUserId)
            .HasConversion(value => value.Value, value => UserId.From(value))
            .HasColumnName("owner_user_id");
        builder.Property(value => value.OwnerDeviceId)
            .HasColumnName("owner_device_id").HasMaxLength(256);
        builder.Property(value => value.Signature).HasColumnName("signature");
        builder.Property(value => value.StoredAt).HasColumnName("stored_at");
        builder.Property(value => value.AcknowledgedAt).HasColumnName("acknowledged_at");
        builder.HasOne<SemanticRelayRequest>()
            .WithOne()
            .HasForeignKey<SemanticRelayReceipt>(value => value.RequestRowId)
            .OnDelete(DeleteBehavior.Cascade);
        builder.HasIndex(value => value.RequestRowId).IsUnique();
        builder.HasIndex(value => new { value.RequesterUserId, value.RequestId }).IsUnique();
        builder.HasIndex(value => new { value.RequesterUserId, value.AcknowledgedAt });
    }
}
