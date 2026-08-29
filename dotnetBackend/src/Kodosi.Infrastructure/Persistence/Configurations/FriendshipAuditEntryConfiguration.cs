using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class FriendshipAuditEntryConfiguration
    : IEntityTypeConfiguration<FriendshipAuditEntry>
{
    public void Configure(EntityTypeBuilder<FriendshipAuditEntry> builder)
    {
        builder.ToTable("friendship_audit");

        builder.HasKey(e => e.Id);
        builder.Property(e => e.Id).HasColumnName("id");

        builder.Property(e => e.ActorUserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("actor_user_id");

        builder.Property(e => e.OtherUserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("other_user_id");

        builder.Property(e => e.Action)
            .HasConversion<string>()
            .HasMaxLength(32)
            .HasColumnName("action");

        builder.Property(e => e.OccurredAt).HasColumnName("occurred_at");

        builder.Property(e => e.ClientIp).HasColumnName("client_ip").HasMaxLength(64);
        builder.Property(e => e.UserAgent).HasColumnName("user_agent").HasMaxLength(512);


        builder.HasIndex(e => e.ActorUserId);
        builder.HasIndex(e => e.OtherUserId);
    }
}
