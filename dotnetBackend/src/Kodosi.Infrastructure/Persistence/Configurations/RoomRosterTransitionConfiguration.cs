using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class RoomRosterTransitionConfiguration
    : IEntityTypeConfiguration<RoomRosterTransition>
{
    public void Configure(EntityTypeBuilder<RoomRosterTransition> builder)
    {
        builder.ToTable("room_roster_transitions");
        builder.HasKey(transition => new { transition.RoomId, transition.Generation });
        builder.Property(transition => transition.RoomId)
            .HasConversion(id => id.Value, value => RoomId.From(value))
            .HasColumnName("room_id");
        builder.Property(transition => transition.Generation).HasColumnName("generation");
        builder.Property(transition => transition.RosterBody)
            .HasColumnName("roster_body").IsRequired();
        builder.Property(transition => transition.RosterSignature)
            .HasColumnName("roster_signature").IsRequired();
        builder.Property(transition => transition.RosterSignerDeviceId)
            .HasColumnName("roster_signer_device_id").HasMaxLength(256).IsRequired();
        builder.Property(transition => transition.AdmissionInvitationId)
            .HasColumnName("admission_invitation_id");
        builder.Property(transition => transition.CreatedAt).HasColumnName("created_at");
        builder.HasOne<Room>().WithMany().HasForeignKey(transition => transition.RoomId)
            .OnDelete(DeleteBehavior.Cascade);
        builder.HasOne<RoomInvitation>().WithMany()
            .HasForeignKey(transition => transition.AdmissionInvitationId)
            .OnDelete(DeleteBehavior.Restrict);
    }
}
