using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class SessionAccessMutationConfiguration
    : IEntityTypeConfiguration<SessionAccessMutation>
{
    public void Configure(EntityTypeBuilder<SessionAccessMutation> builder)
    {
        builder.ToTable(
            "session_access_mutations",
            table =>
            {
                table.HasCheckConstraint(
                    "CK_session_access_mutations_kind",
                    "kind IN ('Grant', 'Revoke', 'Leave')");
                table.HasCheckConstraint(
                    "CK_session_access_mutations_shape",
                    "(kind = 'Grant' AND target_user_id IS NOT NULL AND access_level IS NOT NULL AND requested_expires_at IS NOT NULL) OR "
                    + "(kind = 'Revoke' AND target_user_id IS NOT NULL AND access_level IS NULL AND requested_expires_at IS NULL) OR "
                    + "(kind = 'Leave' AND target_user_id IS NULL AND access_level IS NULL AND requested_expires_at IS NULL)");
                table.HasCheckConstraint(
                    "CK_session_access_mutations_access_level",
                    "access_level IS NULL OR access_level IN ('View', 'Suggest', 'Inject', 'Approve')");
            });
        builder.HasKey(mutation => new
        {
            mutation.RequesterUserId,
            mutation.MutationId,
        });
        builder.Property(mutation => mutation.RequesterUserId)
            .HasConversion(id => id.Value, value => UserId.From(value))
            .HasColumnName("requester_user_id");
        builder.Property(mutation => mutation.MutationId).HasColumnName("mutation_id");
        builder.Property(mutation => mutation.SessionId)
            .HasConversion(id => id.Value, value => SessionId.From(value))
            .HasColumnName("session_id");
        builder.Property(mutation => mutation.IncarnationId).HasColumnName("incarnation_id");
        builder.Property(mutation => mutation.Kind)
            .HasConversion<string>()
            .HasColumnName("kind")
            .HasMaxLength(16);
        builder.Property(mutation => mutation.TargetUserId)
            .HasConversion(
                id => id == null ? (Guid?)null : id.Value.Value,
                value => value == null ? null : UserId.From(value.Value))
            .HasColumnName("target_user_id");
        builder.Property(mutation => mutation.AccessLevel)
            .HasConversion<string>()
            .HasColumnName("access_level")
            .HasMaxLength(32);
        builder.Property(mutation => mutation.RequestedExpiresAt)
            .HasColumnName("requested_expires_at");
        builder.Property(mutation => mutation.CreatedAt).HasColumnName("created_at");
        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(mutation => mutation.RequesterUserId)
            .OnDelete(DeleteBehavior.Cascade);
    }
}
