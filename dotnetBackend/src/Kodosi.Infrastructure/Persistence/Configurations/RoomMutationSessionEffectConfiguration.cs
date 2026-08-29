using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class RoomMutationSessionEffectConfiguration
    : IEntityTypeConfiguration<RoomMutationSessionEffect>
{
    public void Configure(EntityTypeBuilder<RoomMutationSessionEffect> builder)
    {
        builder.ToTable("room_mutation_session_effects");
        builder.HasKey(effect => new
        {
            effect.ActorUserId,
            effect.Operation,
            effect.RequestId,
            effect.SessionId,
            effect.SessionIncarnationId,
        });
        builder.Property(effect => effect.ActorUserId)
            .HasConversion(id => id.Value, value => UserId.From(value))
            .HasColumnName("actor_user_id");
        builder.Property(effect => effect.Operation)
            .HasConversion<string>()
            .HasColumnName("operation")
            .HasMaxLength(32);
        builder.Property(effect => effect.RequestId).HasColumnName("request_id");
        builder.Property(effect => effect.SessionId)
            .HasConversion(id => id.Value, value => SessionId.From(value))
            .HasColumnName("session_id");
        builder.Property(effect => effect.SessionIncarnationId)
            .HasColumnName("session_incarnation_id");
        builder.Property(effect => effect.OwnerUserId)
            .HasConversion(id => id.Value, value => UserId.From(value))
            .HasColumnName("owner_user_id");
        builder.Property(effect => effect.StartedAt).HasColumnName("started_at");
        builder.Property(effect => effect.EndedByRemoval).HasColumnName("ended_by_removal");

        builder.HasOne<RoomMutationReceipt>()
            .WithMany()
            .HasForeignKey(effect => new
            {
                effect.ActorUserId,
                effect.Operation,
                effect.RequestId,
            })
            .OnDelete(DeleteBehavior.Cascade);

        builder.ToTable(table => table.HasCheckConstraint(
            "CK_room_mutation_session_effects_remove_member_only",
            "operation = 'RemoveMember'"));
    }
}
