using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class SemanticRelayRequestConfiguration
    : IEntityTypeConfiguration<SemanticRelayRequest>
{
    public void Configure(EntityTypeBuilder<SemanticRelayRequest> builder)
    {
        builder.ToTable(
            "semantic_relay_requests",
            table =>
            {
                table.HasCheckConstraint(
                    "CK_semantic_relay_requests_mode",
                    "mode IN ('queue', 'steer', 'stopAndSend')");
                table.HasCheckConstraint(
                    "CK_semantic_relay_requests_state",
                    "state IN ('Pending', 'Dispatched', 'ReceiptStored', 'Acknowledged')");
                table.HasCheckConstraint(
                    "CK_semantic_relay_requests_payload_sha256",
                    "payload_sha256 ~ '^[0-9a-f]{64}$'");
            });
        builder.HasKey(value => value.Id);
        builder.Property(value => value.Id).HasColumnName("id");
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
        builder.Property(value => value.State)
            .HasConversion<string>().HasColumnName("state").HasMaxLength(32);
        builder.Property(value => value.CreatedAt).HasColumnName("created_at");
        builder.Property(value => value.UpdatedAt).HasColumnName("updated_at");
        builder.HasIndex(value => new { value.RequesterUserId, value.RequestId }).IsUnique();
        builder.HasIndex(value => new { value.SessionId, value.IncarnationId });
    }
}
