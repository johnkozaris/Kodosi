using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class RoomChatMessageConfiguration : IEntityTypeConfiguration<RoomChatMessage>
{
    public void Configure(EntityTypeBuilder<RoomChatMessage> builder)
    {
        builder.ToTable(
            "room_chat_messages",
            table =>
            {
                table.HasCheckConstraint(
                    "ck_room_chat_messages_body_length",
                    $"char_length(body) BETWEEN 1 AND {RoomInputRules.EncryptedContentMaxLength}");
                table.HasCheckConstraint(
                    "ck_room_chat_messages_recipient_count",
                    "cardinality(recipient_session_ids) + cardinality(recipient_user_ids) "
                    + $"<= {RoomInputRules.ChatRecipientMaxCount}");
            });
        builder.HasKey(m => m.Id);

        builder.Property(m => m.Id).HasColumnName("id");

        builder.Property(m => m.RoomId)
            .HasConversion(id => id.Value, v => RoomId.From(v))
            .HasColumnName("room_id");

        builder.Property(m => m.AuthorUserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("author_user_id");

        builder.Property(m => m.AuthorSessionId).HasColumnName("author_session_id");
        builder.Property(m => m.AuthorKind)
            .HasColumnName("author_kind")
            .HasConversion<string>()
            .HasMaxLength(16);
        builder.Property(m => m.RecipientSessionIds)
            .HasColumnName("recipient_session_ids")
            .HasColumnType("uuid[]")
            .HasDefaultValueSql("'{}'::uuid[]")
            .IsRequired();
        builder.Property(m => m.RecipientUserIds)
            .HasColumnName("recipient_user_ids")
            .HasColumnType("uuid[]")
            .HasDefaultValueSql("'{}'::uuid[]")
            .IsRequired();

        builder.Property(m => m.Body)
            .HasColumnName("body")
            .HasColumnType("text")
            .IsRequired();

        builder.Property(m => m.Seq).HasColumnName("seq");
        builder.Property(m => m.PostedAt).HasColumnName("posted_at");

        builder.HasIndex(m => new { m.RoomId, m.Seq }).IsUnique();

        builder.HasOne<Room>()
            .WithMany()
            .HasForeignKey(m => m.RoomId)
            .OnDelete(DeleteBehavior.Cascade);
    }
}
