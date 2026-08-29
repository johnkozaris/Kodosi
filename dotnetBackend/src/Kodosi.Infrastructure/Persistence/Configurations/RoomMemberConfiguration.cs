using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class RoomMemberConfiguration : IEntityTypeConfiguration<RoomMember>
{
    public void Configure(EntityTypeBuilder<RoomMember> builder)
    {
        builder.ToTable("room_members");
        builder.ToTable(table => table.HasCheckConstraint(
            "CK_room_members_active_admission_proof",
            """
            "revoked_at" IS NOT NULL
            OR "role" = 'Owner'
            OR (
                "admission_invitation_id" IS NOT NULL
                AND "admission_proposal_body" IS NOT NULL
                AND "admission_proposal_signature" IS NOT NULL
                AND "admission_proposal_signer_device_id" IS NOT NULL
                AND "admission_proposal_hash" IS NOT NULL
                AND "admission_decision_body" IS NOT NULL
                AND "admission_decision_signature" IS NOT NULL
                AND "admission_decision_signer_device_id" IS NOT NULL
                AND "admission_expires_at" IS NOT NULL
            )
            """));

        builder.HasKey(wm => new { wm.RoomId, wm.UserId });

        builder.Property(wm => wm.RoomId)
            .HasConversion(id => id.Value, v => RoomId.From(v))
            .HasColumnName("room_id");

        builder.Property(wm => wm.UserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("user_id");

        builder.Property(wm => wm.Role)
            .HasConversion<string>()
            .HasColumnName("role")
            .HasMaxLength(32);

        builder.Property(wm => wm.CreatedAt).HasColumnName("created_at");
        builder.Property(wm => wm.RevokedAt).HasColumnName("revoked_at");
        builder.Property(wm => wm.AdmissionInvitationId)
            .HasColumnName("admission_invitation_id");
        builder.Property(wm => wm.AdmissionProposalBody)
            .HasColumnName("admission_proposal_body");
        builder.Property(wm => wm.AdmissionProposalSignature)
            .HasColumnName("admission_proposal_signature");
        builder.Property(wm => wm.AdmissionProposalSignerDeviceId)
            .HasColumnName("admission_proposal_signer_device_id")
            .HasMaxLength(256);
        builder.Property(wm => wm.AdmissionProposalHash)
            .HasColumnName("admission_proposal_hash");
        builder.Property(wm => wm.AdmissionDecisionBody)
            .HasColumnName("admission_decision_body");
        builder.Property(wm => wm.AdmissionDecisionSignature)
            .HasColumnName("admission_decision_signature");
        builder.Property(wm => wm.AdmissionDecisionSignerDeviceId)
            .HasColumnName("admission_decision_signer_device_id")
            .HasMaxLength(256);
        builder.Property(wm => wm.AdmissionExpiresAt)
            .HasColumnName("admission_expires_at");

        builder.Property(wm => wm.Version).IsRowVersion();

        builder.HasOne<Room>()
            .WithMany()
            .HasForeignKey(wm => wm.RoomId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(wm => wm.UserId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasIndex(wm => new { wm.UserId, wm.RevokedAt });
        builder.HasIndex(wm => wm.AdmissionInvitationId)
            .IsUnique()
            .HasFilter("\"admission_invitation_id\" IS NOT NULL");
    }
}
