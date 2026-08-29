using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Configurations;

public sealed class SessionConfiguration : IEntityTypeConfiguration<Session>
{
    public void Configure(EntityTypeBuilder<Session> builder)
    {
        builder.ToTable(
            "sessions",
            table =>
            {
                table.HasCheckConstraint(
                    "CK_sessions_room_scope_matches_room_id",
                    """
                    ("scope" = 'Room' AND "room_id" IS NOT NULL)
                    OR ("scope" <> 'Room' AND "room_id" IS NULL)
                    """);
                table.HasCheckConstraint(
                    "CK_sessions_scope_allowed_values",
                    EnumCheckConstraint.AllowedValues<SessionScope>("scope"));
            });

        builder.HasKey(cs => cs.Id);
        builder.Property(cs => cs.Id)
            .HasConversion(id => id.Value, v => SessionId.From(v))
            .HasColumnName("id");

        builder.Property(cs => cs.IncarnationId)
            .HasColumnName("incarnation_id");
        builder.Property(cs => cs.IncarnationGeneration)
            .HasColumnName("incarnation_generation");
        builder.Property(cs => cs.IncarnationProtocolVersion)
            .HasColumnName("incarnation_protocol_version");

        builder.Property(cs => cs.OwnerUserId)
            .HasConversion(id => id.Value, v => UserId.From(v))
            .HasColumnName("owner_user_id");

        builder.Property(cs => cs.ToolKind)
            .HasConversion<string>()
            .HasColumnName("tool_kind")
            .HasMaxLength(32);

        builder.Property(cs => cs.Title).HasColumnName("title").HasMaxLength(256);

        builder.Property(cs => cs.Scope)
            .HasConversion<string>()
            .HasColumnName("scope")
            .HasMaxLength(32);

        builder.Property(cs => cs.RoomId)
            .HasConversion(
                id => id == null ? (Guid?)null : id.Value.Value,
                v => v.HasValue ? RoomId.From(v.Value) : null)
            .HasColumnName("room_id");

        builder.Property(cs => cs.DefaultAccess)
            .HasConversion<string>()
            .HasColumnName("default_access")
            .HasMaxLength(32);

        builder.Property(cs => cs.Status)
            .HasConversion<string>()
            .HasColumnName("status")
            .HasMaxLength(32);

        builder.Property(cs => cs.OwnerSessionSecretHash)
            .HasColumnName("owner_session_secret_hash")
            .HasMaxLength(512);

        builder.Property(cs => cs.StartedAt).HasColumnName("started_at");
        builder.Property(cs => cs.EndedAt).HasColumnName("ended_at");
        builder.Property(cs => cs.LastHeartbeatAt).HasColumnName("last_heartbeat_at");

        builder.Property(cs => cs.Version).IsRowVersion();

        builder.Property(cs => cs.HostConnectionSlot)
            .HasColumnName("host_connection_slot")
            .HasMaxLength(256);
        builder.Property(cs => cs.HostClaimedAt).HasColumnName("host_claimed_at");
        builder.Property(cs => cs.HostReleasedAt).HasColumnName("host_released_at");
        builder.Property(cs => cs.LiveParticipantCount)
            .HasColumnName("live_participant_count")
            .HasDefaultValue(0);
        builder.Property(cs => cs.CurrentKeyGeneration)
            .HasColumnName("current_key_generation")
            .HasDefaultValue(0);

        builder.HasOne<User>()
            .WithMany()
            .HasForeignKey(cs => cs.OwnerUserId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasOne<Room>()
            .WithMany()
            .HasForeignKey(cs => cs.RoomId)
            .OnDelete(DeleteBehavior.Restrict);

        builder.HasIndex(cs => cs.OwnerUserId);
        builder.HasIndex(cs => cs.Status);
        builder.HasIndex(cs => new { cs.Scope, cs.Status });
        builder.HasIndex(cs => new { cs.OwnerUserId, cs.Status, cs.Scope });
    }
}
