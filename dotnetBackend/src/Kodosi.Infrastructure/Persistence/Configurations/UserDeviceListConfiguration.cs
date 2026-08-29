using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class UserDeviceListConfiguration : IEntityTypeConfiguration<UserDeviceList>
{
    public void Configure(EntityTypeBuilder<UserDeviceList> builder)
    {
        builder.ToTable(
            "user_device_lists",
            table =>
            {
                table.HasCheckConstraint(
                    "CK_user_device_lists_generation_positive",
                    "\"generation\" >= 1");
                table.HasCheckConstraint(
                    "CK_user_device_lists_body_nonempty",
                    "octet_length(\"body\") > 0");
                table.HasCheckConstraint(
                    "CK_user_device_lists_signature_exact",
                    $"octet_length(\"signature\") = {IdentityWireFormat.MlDsa65SignatureLength}");
                table.HasCheckConstraint(
                    "CK_user_device_lists_body_bounded",
                    $"octet_length(\"body\") <= {IdentityWireFormat.MaxSignedDeviceListBodyLength}");
            });

        builder.HasKey(list => new { list.UserId, list.Generation });

        builder.Property(list => list.UserId)
            .HasConversion(id => id.Value, value => UserId.From(value))
            .HasColumnName("user_id");
        builder.Property(list => list.Generation).HasColumnName("generation");
        builder.Property(list => list.Body)
            .HasField("_body")
            .UsePropertyAccessMode(PropertyAccessMode.Field)
            .HasColumnName("body")
            .IsRequired();
        builder.Property(list => list.Signature)
            .HasField("_signature")
            .UsePropertyAccessMode(PropertyAccessMode.Field)
            .HasColumnName("signature")
            .HasMaxLength(IdentityWireFormat.MlDsa65SignatureLength)
            .IsRequired();
        builder.Property(list => list.CreatedAt).HasColumnName("created_at");

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(list => list.UserId)
            .OnDelete(DeleteBehavior.Restrict);
    }
}
