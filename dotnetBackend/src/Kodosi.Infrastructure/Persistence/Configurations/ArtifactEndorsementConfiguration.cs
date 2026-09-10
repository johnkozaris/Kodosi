using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class ArtifactEndorsementConfiguration : IEntityTypeConfiguration<ArtifactEndorsement>
{
    public void Configure(EntityTypeBuilder<ArtifactEndorsement> builder)
    {
        builder.ToTable("artifact_endorsements", table =>
        {
            table.HasCheckConstraint("CK_artifact_endorsements_digest", "artifact_digest ~ '^[0-9a-f]{64}$'");
            table.HasCheckConstraint("CK_artifact_endorsements_signature", "octet_length(signature) = 3309");
        });
        builder.HasKey(row => new { row.UserId, row.IdentityIncarnationId, row.ArtifactDigest });
        builder.Property(row => row.UserId).HasConversion(id => id.Value, value => UserId.From(value)).HasColumnName("user_id");
        builder.Property(row => row.IdentityIncarnationId).HasColumnName("identity_incarnation_id");
        builder.Property(row => row.ArtifactDigest).HasColumnName("artifact_digest").HasMaxLength(64);
        builder.Property(row => row.EndorserDeviceId).HasColumnName("endorser_device_id").HasMaxLength(DeviceIdRules.MaximumLength);
        builder.Property(row => row.Signature).HasColumnName("signature").IsRequired();
        builder.HasOne<User>().WithMany().HasForeignKey(row => row.UserId).OnDelete(DeleteBehavior.Cascade);
    }
}
