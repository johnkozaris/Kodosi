using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class UserConfiguration : IEntityTypeConfiguration<User>
{
    public void Configure(EntityTypeBuilder<User> builder)
    {
        builder.ToTable("users");

        builder.HasKey(u => u.Id);
        builder.Property(u => u.Id)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("id");

        builder.Property(u => u.Email).HasColumnName("email").HasMaxLength(320);
        builder.Property(u => u.Handle).HasColumnName("handle").HasMaxLength(64);
        builder.HasIndex(u => u.Handle).IsUnique();
        builder.Property(u => u.DisplayName).HasColumnName("display_name").HasMaxLength(128);
        builder.Property(u => u.AvatarUrl).HasColumnName("avatar_url").HasMaxLength(2048);
        builder.Property(u => u.CreatedAt).HasColumnName("created_at");
        builder.Property(u => u.IdentityRevision).HasColumnName("identity_revision");
        builder.Property(u => u.IdentityIncarnationId).HasColumnName("identity_incarnation_id");
    }
}
