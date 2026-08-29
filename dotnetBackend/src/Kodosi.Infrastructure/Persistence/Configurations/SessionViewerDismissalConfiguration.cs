using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class SessionViewerDismissalConfiguration
    : IEntityTypeConfiguration<SessionViewerDismissal>
{
    public void Configure(EntityTypeBuilder<SessionViewerDismissal> builder)
    {
        builder.ToTable("session_viewer_dismissals");

        builder.HasKey(dismissal => new
        {
            dismissal.SessionId,
            dismissal.ViewerUserId,
        });

        builder.Property(dismissal => dismissal.SessionId)
            .HasConversion(id => id.Value, value => SessionId.From(value))
            .HasColumnName("session_id");

        builder.Property(dismissal => dismissal.ViewerUserId)
            .HasConversion(id => id.Value, value => UserId.From(value))
            .HasColumnName("viewer_user_id");

        builder.HasOne<Session>()
            .WithMany()
            .HasForeignKey(dismissal => dismissal.SessionId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(dismissal => dismissal.ViewerUserId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasIndex(dismissal => dismissal.ViewerUserId);
    }
}
