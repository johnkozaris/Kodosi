using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class IdentityExposureConfiguration : IEntityTypeConfiguration<IdentityExposure>
{
    public void Configure(EntityTypeBuilder<IdentityExposure> builder)
    {
        builder.ToTable("identity_exposures");
        builder.HasKey(exposure => new
        {
            exposure.IdentityOwnerUserId,
            exposure.RecipientUserId,
        });
        builder.Property(exposure => exposure.IdentityOwnerUserId)
            .HasConversion(id => id.Value, value => UserId.From(value))
            .HasColumnName("identity_owner_user_id");
        builder.Property(exposure => exposure.RecipientUserId)
            .HasConversion(id => id.Value, value => UserId.From(value))
            .HasColumnName("recipient_user_id");
        builder.Property(exposure => exposure.FirstExposedAt)
            .HasColumnName("first_exposed_at");
        builder.Property(exposure => exposure.LastExposedAt)
            .HasColumnName("last_exposed_at");
        builder.HasIndex(exposure => exposure.RecipientUserId);
    }
}
