using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class RoomMutationReceiptConfiguration
    : IEntityTypeConfiguration<RoomMutationReceipt>
{
    public void Configure(EntityTypeBuilder<RoomMutationReceipt> builder)
    {
        builder.ToTable("room_mutation_receipts");
        builder.HasKey(receipt => new
        {
            receipt.ActorUserId,
            receipt.Operation,
            receipt.RequestId,
        });

        builder.Property(receipt => receipt.ActorUserId)
            .HasConversion(id => id.Value, value => UserId.From(value))
            .HasColumnName("actor_user_id");
        builder.Property(receipt => receipt.Operation)
            .HasConversion<string>()
            .HasColumnName("operation")
            .HasMaxLength(32);
        builder.Property(receipt => receipt.RequestId)
            .HasColumnName("request_id");
        builder.Property(receipt => receipt.TargetFingerprint)
            .HasColumnName("target_fingerprint")
            .HasMaxLength(RoomMutationReceipt.FingerprintLength)
            .IsRequired();
        builder.Property(receipt => receipt.RoomId)
            .HasConversion(id => id.Value, value => RoomId.From(value))
            .HasColumnName("room_id");
        builder.Property(receipt => receipt.EntityId)
            .HasColumnName("entity_id");
        builder.Property(receipt => receipt.Result)
            .HasColumnName("result")
            .HasMaxLength(32)
            .IsRequired();
        builder.Property(receipt => receipt.AssigneeSessionId)
            .HasColumnName("assignee_session_id");
        builder.Property(receipt => receipt.AssigneeSessionIncarnationId)
            .HasColumnName("assignee_session_incarnation_id");
        builder.Property(receipt => receipt.Revision)
            .HasColumnName("revision");
        builder.Property(receipt => receipt.CreatedAt)
            .HasColumnName("created_at");

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(receipt => receipt.ActorUserId)
            .OnDelete(DeleteBehavior.Cascade);
        builder.HasOne<Room>()
            .WithMany()
            .HasForeignKey(receipt => receipt.RoomId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.ToTable(table => table.HasCheckConstraint(
            "CK_room_mutation_receipts_operation_allowed_values",
            EnumCheckConstraint.AllowedValues<RoomMutationOperation>("operation")));
        builder.ToTable(table => table.HasCheckConstraint(
            "CK_room_mutation_receipts_fingerprint_length",
            "octet_length(target_fingerprint) = 32"));
        builder.ToTable(table => table.HasCheckConstraint(
            "CK_room_mutation_receipts_assignee_pair",
            "(assignee_session_id IS NULL) = (assignee_session_incarnation_id IS NULL)"));
        builder.ToTable(table => table.HasCheckConstraint(
            "CK_room_mutation_receipts_revision_nonnegative",
            "revision IS NULL OR revision >= 0"));
    }
}
