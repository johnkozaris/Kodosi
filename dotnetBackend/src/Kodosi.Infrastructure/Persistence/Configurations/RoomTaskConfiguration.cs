using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class RoomTaskConfiguration : IEntityTypeConfiguration<RoomTask>
{
    public void Configure(EntityTypeBuilder<RoomTask> builder)
    {
        builder.ToTable("room_tasks");
        builder.HasKey(t => t.Id);

        builder.Property(t => t.Id).HasColumnName("id");

        builder.Property(t => t.RoomId)
            .HasConversion(id => id.Value, v => RoomId.From(v))
            .HasColumnName("room_id");

        builder.Property(t => t.CreatedByUserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("created_by_user_id");

        builder.Property(t => t.Title)
            .HasColumnName("title")
            .HasColumnType("text")
            .IsRequired();

        builder.Property(t => t.Description).HasColumnName("description");

        builder.Property(t => t.Status)
            .HasConversion<string>()
            .HasColumnName("status")
            .HasMaxLength(16);

        builder.Property(t => t.AssignedSessionId).HasColumnName("assigned_session_id");
        builder.Property(t => t.AssignedSessionIncarnationId)
            .HasColumnName("assigned_session_incarnation_id");
        builder.Property(t => t.DueAt).HasColumnName("due_at");
        builder.Property(t => t.CreatedAt).HasColumnName("created_at");
        builder.Property(t => t.UpdatedAt).HasColumnName("updated_at");
        builder.Property(t => t.CompletedAt).HasColumnName("completed_at");
        builder.Property(t => t.Result).HasColumnName("result");
        builder.Property(t => t.ResultAuthorUserId)
            .HasConversion(
                id => id.HasValue ? id.Value.Value : (Guid?)null,
                value => value.HasValue ? UserId.From(value.Value) : null)
            .HasColumnName("result_author_user_id");
        builder.Property(t => t.Revision).HasColumnName("revision");
        builder.Property(t => t.Version).IsRowVersion();

        builder.HasIndex(t => new { t.RoomId, t.Status });
        builder.HasIndex(t => t.AssignedSessionId);

        builder.ToTable(table => table.HasCheckConstraint(
            "CK_room_tasks_assignee_incarnation_pair",
            "(assigned_session_id IS NULL) = (assigned_session_incarnation_id IS NULL)"));

        builder.HasOne<Room>()
            .WithMany()
            .HasForeignKey(t => t.RoomId)
            .OnDelete(DeleteBehavior.Cascade);

        builder.ToTable(t => t.HasCheckConstraint(
            "CK_room_tasks_status_allowed_values",
            EnumCheckConstraint.AllowedValues<RoomTaskStatus>("status")));
    }
}
