using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class UserDeviceConfiguration : IEntityTypeConfiguration<UserDevice>
{
    public void Configure(EntityTypeBuilder<UserDevice> builder)
    {
        builder.ToTable("user_devices");

        builder.HasKey(d => new { d.UserId, d.DeviceId });

        builder.Property(d => d.UserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("user_id");

        builder.Property(d => d.DeviceId)
            .HasColumnName("device_id")
            .HasMaxLength(256);

        builder.Property(d => d.CreatedAt).HasColumnName("created_at");

        builder.Ignore(d => d.KemPublicKey);
        builder.Ignore(d => d.SigningPublicKey);
        builder.Ignore(d => d.DeviceLabel);
        builder.Ignore(d => d.CertSignerDeviceId);
        builder.Ignore(d => d.CertIssuedAt);
        builder.Ignore(d => d.CertExpiresAt);

        builder.Property(d => d.DeviceCertificate)
            .HasField("_deviceCertificate")
            .UsePropertyAccessMode(PropertyAccessMode.Field)
            .HasColumnName("device_certificate")
            .IsRequired();

        builder.Property(d => d.DeviceCertificateSignature)
            .HasField("_deviceCertificateSignature")
            .UsePropertyAccessMode(PropertyAccessMode.Field)
            .HasColumnName("device_certificate_signature")
            .HasMaxLength(IdentityWireFormat.MlDsa65SignatureLength)
            .IsRequired();

        builder.ToTable(table =>
        {
            table.HasCheckConstraint(
                "CK_user_devices_certificate_bounded",
                $"octet_length(\"device_certificate\") BETWEEN 1 AND {IdentityWireFormat.MaxDeviceCertificateBodyLength}");
            table.HasCheckConstraint(
                "CK_user_devices_certificate_signature_exact",
                $"octet_length(\"device_certificate_signature\") = {IdentityWireFormat.MlDsa65SignatureLength}");
        });

        builder.Property(d => d.RevokedAt).HasColumnName("revoked_at");
        builder.Property(d => d.RevokedByDeviceId)
            .HasColumnName("revoked_by_device_id")
            .HasMaxLength(256);

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(d => d.UserId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasIndex(d => d.DeviceId).IsUnique();
        builder.HasIndex(d => new { d.UserId, d.RevokedAt });
    }
}
