using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class FriendshipConfiguration : IEntityTypeConfiguration<Friendship>
{
    public void Configure(EntityTypeBuilder<Friendship> builder)
    {
        builder.ToTable("friendships");

        builder.HasKey(f => new { f.UserLowId, f.UserHighId });

        builder.Property(f => f.UserLowId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("user_low_id");

        builder.Property(f => f.UserHighId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("user_high_id");

        builder.Property(f => f.RequestorUserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("requestor_user_id");

        builder.Property(f => f.Status)
            .HasConversion<string>()
            .HasColumnName("status")
            .HasMaxLength(32);

        builder.Property(f => f.CreatedAt).HasColumnName("created_at");
        builder.Property(f => f.AcceptedAt).HasColumnName("accepted_at");



        builder.Property(f => f.Version).IsRowVersion();

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(f => f.UserLowId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(f => f.UserHighId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(f => f.RequestorUserId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasIndex(f => new { f.Status, f.RequestorUserId });
        builder.HasIndex(f => new { f.Status, f.UserLowId });
        builder.HasIndex(f => new { f.Status, f.UserHighId });
    }
}
