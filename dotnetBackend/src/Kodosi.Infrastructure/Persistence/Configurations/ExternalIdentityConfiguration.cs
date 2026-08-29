using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class ExternalIdentityConfiguration : IEntityTypeConfiguration<ExternalIdentity>
{
    public void Configure(EntityTypeBuilder<ExternalIdentity> builder)
    {
        builder.ToTable("external_identities");

        builder.HasKey(identity => identity.Id);
        builder.Property(identity => identity.Id).HasColumnName("id");

        builder.Property(identity => identity.UserId)
            .HasConversion(id => id.Value, value => UserId.From(value))
            .HasColumnName("user_id");

        builder.Property(identity => identity.Provider).HasColumnName("provider").HasMaxLength(64);
        builder.Property(identity => identity.Issuer).HasColumnName("issuer").HasMaxLength(512);
        builder.Property(identity => identity.Subject).HasColumnName("subject").HasMaxLength(512);
        builder.Property(identity => identity.LinkedAt).HasColumnName("linked_at");




        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(identity => identity.UserId)
            .OnDelete(DeleteBehavior.Cascade);

        builder.HasIndex(identity => new { identity.Provider, identity.Issuer, identity.Subject }).IsUnique();
        builder.HasIndex(identity => identity.UserId);
    }
}
