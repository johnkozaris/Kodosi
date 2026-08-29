using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class InputAuditEntryConfiguration : IEntityTypeConfiguration<InputAuditEntry>
{
    public void Configure(EntityTypeBuilder<InputAuditEntry> builder)
    {
        builder.ToTable("input_audit");

        builder.HasKey(a => a.Id);
        builder.Property(a => a.Id).HasColumnName("id");

        builder.Property(a => a.SessionId)
            .HasConversion(id => id.Value, v => SessionId.From(v))
            .HasColumnName("session_id");

        builder.Property(a => a.SenderUserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("sender_user_id");

        builder.Property(a => a.ClientCommandId).HasColumnName("client_command_id").HasMaxLength(128);

        builder.Property(a => a.Kind)
            .HasConversion<string>()
            .HasColumnName("kind")
            .HasMaxLength(32);

        builder.Property(a => a.PayloadSha256).HasColumnName("payload_sha256").HasMaxLength(64);
        builder.Property(a => a.PayloadBytesLen).HasColumnName("payload_bytes_len");

        builder.Property(a => a.Status)
            .HasConversion<string>()
            .HasColumnName("status")
            .HasMaxLength(32);

        builder.Property(a => a.SubmittedAt).HasColumnName("submitted_at");
        builder.Property(a => a.DispatchedAt).HasColumnName("dispatched_at");
        builder.Property(a => a.CompletedAt).HasColumnName("completed_at");
        builder.Property(a => a.DuplicateCount)
            .HasColumnName("duplicate_count")
            .HasDefaultValue(0);
        builder.Property(a => a.LastDuplicateAt).HasColumnName("last_duplicate_at");
        builder.Property(a => a.PendingSessionIncarnationId)
            .HasColumnName("pending_session_incarnation_id");
        builder.Property(a => a.PendingSessionIncarnationGeneration)
            .HasColumnName("pending_session_incarnation_generation");
        builder.Property(a => a.PendingRequestId)
            .HasColumnName("pending_request_id")
            .HasMaxLength(128);
        builder.Property(a => a.PendingRequestGeneration)
            .HasColumnName("pending_request_generation");
        builder.Property(a => a.PendingRequesterDeviceId)
            .HasColumnName("pending_requester_device_id")
            .HasMaxLength(256);

        builder.ToTable(table => table.HasCheckConstraint(
            "ck_input_audit_permission_pending_tuple",
            "(pending_session_incarnation_id IS NULL AND pending_session_incarnation_generation IS NULL AND pending_request_id IS NULL AND pending_request_generation IS NULL AND pending_requester_device_id IS NULL) OR (pending_session_incarnation_id IS NOT NULL AND pending_session_incarnation_generation IS NOT NULL AND pending_session_incarnation_generation > 0 AND pending_request_id IS NOT NULL AND pending_request_generation IS NOT NULL AND pending_request_generation > 0 AND pending_requester_device_id IS NOT NULL)"));

        builder.HasOne<Session>()
            .WithMany()
            .HasForeignKey(a => a.SessionId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(a => a.SenderUserId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasIndex(a => a.SenderUserId);
        builder.HasIndex(a => new { a.SessionId, a.SenderUserId, a.ClientCommandId }).IsUnique();
    }
}
