using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class DeviceLinkRequestConfiguration : IEntityTypeConfiguration<DeviceLinkRequest>
{
    public void Configure(EntityTypeBuilder<DeviceLinkRequest> builder)
    {
        builder.ToTable("device_link_requests");

        builder.HasKey(r => r.Id);
        builder.Property(r => r.Id).HasColumnName("id");

        builder.Property(r => r.UserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("user_id");

        builder.Property(r => r.DeviceCode)
            .HasColumnName("device_code")
            .HasMaxLength(256)
            .IsRequired();

        builder.Property(r => r.UserCode)
            .HasColumnName("user_code")
            .HasMaxLength(32)
            .IsRequired();

        builder.Property(r => r.DeviceId)
            .HasColumnName("device_id")
            .HasMaxLength(256)
            .IsRequired();

        builder.Property(r => r.DeviceLabel)
            .HasColumnName("device_label")
            .HasMaxLength(128)
            .IsRequired();

        builder.Property(r => r.KemPublicKey)
            .HasColumnName("kem_public_key")
            .HasMaxLength(1184)
            .IsRequired();

        builder.Property(r => r.SigningPublicKey)
            .HasColumnName("signing_public_key")
            .HasMaxLength(1952)
            .IsRequired();

        builder.Property(r => r.ExpiresAt).HasColumnName("expires_at");
        builder.Property(r => r.ApprovedAt).HasColumnName("approved_at");
        builder.Property(r => r.AcknowledgedAt).HasColumnName("acknowledged_at");
        builder.Property(r => r.ResultExpiresAt).HasColumnName("result_expires_at");
        builder.Property(r => r.CancelledAt).HasColumnName("cancelled_at");

        builder.Property(r => r.DeviceListGeneration).HasColumnName("device_list_generation");

        builder.Property(r => r.CreatedAt).HasColumnName("created_at");
        builder.Property(r => r.Version).IsRowVersion();

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(r => r.UserId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasIndex(r => r.DeviceCode).IsUnique();
        builder.HasIndex(r => r.UserCode).IsUnique();
        builder.HasIndex(r => r.ExpiresAt);
        builder.HasIndex(r => r.ResultExpiresAt);
    }
}
