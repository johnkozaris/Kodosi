using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class DeviceRegistrationChallengeConfiguration
    : IEntityTypeConfiguration<DeviceRegistrationChallenge>
{
    public void Configure(EntityTypeBuilder<DeviceRegistrationChallenge> builder)
    {
        builder.ToTable("device_registration_challenges");

        builder.HasKey(c => c.Id);

        builder.Property(c => c.Id).HasColumnName("id");

        builder.Property(c => c.UserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("user_id");

        builder.Property(c => c.Challenge)
            .HasColumnName("challenge")
            .IsRequired();

        builder.Property(c => c.ExpiresAt).HasColumnName("expires_at");
        builder.Property(c => c.CreatedAt).HasColumnName("created_at");

        builder.Property(c => c.Version).IsRowVersion();

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(c => c.UserId)
            .OnDelete(DeleteBehavior.Restrict);


        builder.HasIndex(c => c.ExpiresAt);
    }
}
