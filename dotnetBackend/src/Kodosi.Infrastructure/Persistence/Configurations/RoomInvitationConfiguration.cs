using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class RoomInvitationConfiguration : IEntityTypeConfiguration<RoomInvitation>
{
    public void Configure(EntityTypeBuilder<RoomInvitation> builder)
    {
        builder.ToTable("room_invitations");
        builder.HasKey(i => i.Id);

        builder.Property(i => i.Id).HasColumnName("id");

        builder.Property(i => i.RoomId)
            .HasConversion(id => id.Value, v => RoomId.From(v))
            .HasColumnName("room_id");

        builder.Property(i => i.InviteeUserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("invitee_user_id");

        builder.Property(i => i.InvitedByUserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("invited_by_user_id");

        builder.Property(i => i.Status)
            .HasConversion<string>()
            .HasColumnName("status")
            .HasMaxLength(16);

        builder.Property(i => i.CreatedAt).HasColumnName("created_at");
        builder.Property(i => i.RespondedAt).HasColumnName("responded_at");
        builder.Property(i => i.BaseRosterGeneration)
            .HasColumnName("base_roster_generation");
        builder.Property(i => i.ProposedRosterGeneration)
            .HasColumnName("proposed_roster_generation");
        builder.Property(i => i.ProposedRosterBody)
            .HasColumnName("proposed_roster_body")
            .IsRequired();
        builder.Property(i => i.ProposedRosterSignature)
            .HasColumnName("proposed_roster_signature")
            .IsRequired();
        builder.Property(i => i.ProposedRosterSignerDeviceId)
            .HasColumnName("proposed_roster_signer_device_id")
            .HasMaxLength(256)
            .IsRequired();
        builder.Property(i => i.ProposalBody)
            .HasColumnName("proposal_body")
            .IsRequired();
        builder.Property(i => i.ProposalSignature)
            .HasColumnName("proposal_signature")
            .IsRequired();
        builder.Property(i => i.ProposalSignerDeviceId)
            .HasColumnName("proposal_signer_device_id")
            .HasMaxLength(256)
            .IsRequired();
        builder.Property(i => i.ProposalHash)
            .HasColumnName("proposal_hash")
            .IsRequired();
        builder.Property(i => i.ProposalIssuedAt)
            .HasColumnName("proposal_issued_at");
        builder.Property(i => i.ExpiresAt)
            .HasColumnName("expires_at");
        builder.Property(i => i.DecisionBody)
            .HasColumnName("decision_body");
        builder.Property(i => i.DecisionSignature)
            .HasColumnName("decision_signature");
        builder.Property(i => i.DecisionSignerDeviceId)
            .HasColumnName("decision_signer_device_id")
            .HasMaxLength(256);
        builder.Property(i => i.DecisionIssuedAt)
            .HasColumnName("decision_issued_at");

        builder.Property(i => i.Version).IsRowVersion();

        builder.HasIndex(i => i.InviteeUserId);
        builder.HasIndex(i => i.InvitedByUserId);
        builder.HasIndex(i => i.ExpiresAt);
        builder.HasIndex(i => new { i.RoomId, i.InviteeUserId })
            .HasDatabaseName("UX_room_invitations_pending_room_invitee")
            .IsUnique()
            .HasFilter("\"status\" = 'Pending'");

        builder.HasOne<Room>()
            .WithMany()
            .HasForeignKey(i => i.RoomId)
            .OnDelete(DeleteBehavior.Cascade);

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(i => i.InviteeUserId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.ToTable(t => t.HasCheckConstraint(
            "CK_room_invitations_status_allowed_values",
            EnumCheckConstraint.AllowedValues<RoomInvitationStatus>("status")));
    }
}
