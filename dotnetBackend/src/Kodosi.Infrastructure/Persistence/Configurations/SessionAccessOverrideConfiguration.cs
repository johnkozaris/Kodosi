using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class SessionAccessOverrideConfiguration : IEntityTypeConfiguration<SessionAccessOverride>
{
    public void Configure(EntityTypeBuilder<SessionAccessOverride> builder)
    {
        builder.ToTable("session_access_overrides");

        builder.HasKey(o => new { o.SessionId, o.ActorUserId });

        builder.Property(o => o.SessionId)
            .HasConversion(id => id.Value, v => SessionId.From(v))
            .HasColumnName("session_id");

        builder.Property(o => o.ActorUserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("actor_user_id");

        builder.Property(o => o.AccessLevel)
            .HasConversion<string>()
            .HasColumnName("access_level")
            .HasMaxLength(32);

        builder.Property(o => o.GrantedByUserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("granted_by_user_id");

        builder.Property(o => o.CreatedAt).HasColumnName("created_at");
        builder.Property(o => o.RevokedAt).HasColumnName("revoked_at");
        builder.Property(o => o.ExpiresAt).HasColumnName("expires_at");

        builder.HasOne<Session>()
            .WithMany()
            .HasForeignKey(o => o.SessionId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(o => o.ActorUserId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(o => o.GrantedByUserId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasIndex(o => o.ExpiresAt)
            .HasFilter("\"revoked_at\" IS NULL AND \"expires_at\" IS NOT NULL");
    }
}
