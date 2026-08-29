using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class SessionEndMutationConfiguration
    : IEntityTypeConfiguration<SessionEndMutation>
{
    public void Configure(EntityTypeBuilder<SessionEndMutation> builder)
    {
        builder.ToTable("session_end_mutations");
        builder.HasKey(mutation => new
        {
            mutation.OwnerUserId,
            mutation.MutationId,
        });
        builder.Property(mutation => mutation.OwnerUserId)
            .HasConversion(id => id.Value, value => UserId.From(value))
            .HasColumnName("owner_user_id");
        builder.Property(mutation => mutation.MutationId)
            .HasColumnName("mutation_id");
        builder.Property(mutation => mutation.SessionId)
            .HasConversion(id => id.Value, value => SessionId.From(value))
            .HasColumnName("session_id");
        builder.Property(mutation => mutation.IncarnationId)
            .HasColumnName("incarnation_id");
        builder.Property(mutation => mutation.FirstAttemptId)
            .HasColumnName("first_attempt_id");
        builder.Property(mutation => mutation.CreatedAt)
            .HasColumnName("created_at");

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(mutation => mutation.OwnerUserId)
            .OnDelete(DeleteBehavior.Cascade);
    }
}
