using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class SessionKeyBlobConfiguration : IEntityTypeConfiguration<SessionKeyBlob>
{
    public void Configure(EntityTypeBuilder<SessionKeyBlob> builder)
    {
        builder.ToTable(
            "session_key_blobs",
            table =>
            {
                table.HasCheckConstraint(
                    "CK_session_key_blobs_issued_at_ms_positive",
                    "\"issued_at_ms\" > 0");
            });

        builder.HasKey(b => new { b.SessionId, b.RecipientDeviceId });

        builder.Property(b => b.SessionId)
            .HasConversion(id => id.Value, v => SessionId.From(v))
            .HasColumnName("session_id");

        builder.Property(b => b.RecipientDeviceId)
            .HasColumnName("recipient_device_id")
            .HasMaxLength(256);

        builder.Property(b => b.EncryptedSessionKey)
            .HasColumnName("encrypted_session_key")
            .IsRequired();

        builder.Property(b => b.SenderDeviceId)
            .HasColumnName("sender_device_id")
            .HasMaxLength(256);

        builder.Property(b => b.SenderKemPublicKey)
            .HasColumnName("sender_kem_public_key")
            .IsRequired();

        builder.Property(b => b.KeyGeneration)
            .HasColumnName("key_generation");
        builder.Property(b => b.SignatureVersion)
            .HasColumnName("signature_version");

        builder.Property(b => b.Signature)
            .HasColumnName("signature");

        builder.Property(b => b.CreatedAt)
            .HasColumnName("created_at");

        builder.Property(b => b.IssuedAtMs)
            .HasColumnName("issued_at_ms")
            .IsRequired();

        builder.HasOne<Session>()
            .WithMany()
            .HasForeignKey(b => b.SessionId)
            .OnDelete(DeleteBehavior.Restrict);
    }
}
