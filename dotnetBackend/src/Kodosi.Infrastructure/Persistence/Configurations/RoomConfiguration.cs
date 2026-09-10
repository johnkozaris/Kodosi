using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class RoomConfiguration : IEntityTypeConfiguration<Room>
{
    public void Configure(EntityTypeBuilder<Room> builder)
    {
        builder.ToTable("rooms");

        builder.HasKey(w => w.Id);
        builder.Property(w => w.Id)
            .HasConversion(id => id.Value, v => RoomId.From(v))
            .HasColumnName("id");

        builder.Property(w => w.OwnerUserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("owner_user_id");

        builder.Property(w => w.Name)
            .HasColumnName("name")
            .HasMaxLength(RoomInputRules.RoomNameMaxLength);
        builder.Property(w => w.Slug)
            .HasColumnName("slug")
            .HasMaxLength(RoomInputRules.RoomSlugMaxLength);
        builder.HasIndex(w => w.Slug).IsUnique();

        builder.Property(w => w.CreatedAt).HasColumnName("created_at");
        builder.Property(w => w.TaskRevision).HasColumnName("task_revision").HasDefaultValue(0L);
        builder.ToTable(table => table.HasCheckConstraint(
            "CK_rooms_task_revision_nonnegative", "task_revision >= 0"));
        builder.Property(w => w.RosterGeneration)
            .HasColumnName("roster_generation");
        builder.Property(w => w.RosterBody).HasColumnName("roster_body").IsRequired();
        builder.Property(w => w.RosterSignature).HasColumnName("roster_signature").IsRequired();
        builder.Property(w => w.RosterSignerDeviceId)
            .HasColumnName("roster_signer_device_id")
            .HasMaxLength(256)
            .IsRequired();
        builder.Property(w => w.RosterActivationInvitationId)
            .HasColumnName("roster_activation_invitation_id");
        builder.Property(w => w.RosterActivationInviteeUserId)
            .HasConversion(
                id => id.HasValue ? id.Value.Value : (Guid?)null,
                value => value.HasValue ? UserId.From(value.Value) : (UserId?)null)
            .HasColumnName("roster_activation_invitee_user_id");
        builder.Property(w => w.RosterActivationProposalBody)
            .HasColumnName("roster_activation_proposal_body");
        builder.Property(w => w.RosterActivationProposalSignature)
            .HasColumnName("roster_activation_proposal_signature");
        builder.Property(w => w.RosterActivationProposalSignerDeviceId)
            .HasColumnName("roster_activation_proposal_signer_device_id")
            .HasMaxLength(256);
        builder.Property(w => w.RosterActivationProposalHash)
            .HasColumnName("roster_activation_proposal_hash");
        builder.Property(w => w.RosterActivationDecisionBody)
            .HasColumnName("roster_activation_decision_body");
        builder.Property(w => w.RosterActivationDecisionSignature)
            .HasColumnName("roster_activation_decision_signature");
        builder.Property(w => w.RosterActivationDecisionSignerDeviceId)
            .HasColumnName("roster_activation_decision_signer_device_id")
            .HasMaxLength(256);
        builder.Property(w => w.RosterActivationExpiresAt)
            .HasColumnName("roster_activation_expires_at");
        builder.Property(w => w.Version).IsRowVersion();

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(w => w.OwnerUserId)
            .OnDelete(DeleteBehavior.Restrict);
    }
}
