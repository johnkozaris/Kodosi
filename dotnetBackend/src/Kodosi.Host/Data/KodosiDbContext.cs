using Microsoft.EntityFrameworkCore;

namespace Kodosi.Data;

public sealed class KodosiDbContext(DbContextOptions<KodosiDbContext> options) : DbContext(options)
{
    public DbSet<User> Users => Set<User>();
    public DbSet<Device> Devices => Set<Device>();
    public DbSet<DeviceList> DeviceLists => Set<DeviceList>();
    public DbSet<DeviceChallenge> DeviceChallenges => Set<DeviceChallenge>();
    public DbSet<DeviceLink> DeviceLinks => Set<DeviceLink>();
    public DbSet<Friendship> Friendships => Set<Friendship>();
    public DbSet<Session> Sessions => Set<Session>();
    public DbSet<SessionMember> SessionMembers => Set<SessionMember>();
    public DbSet<SessionKeyEnvelope> SessionKeys => Set<SessionKeyEnvelope>();
    public DbSet<Room> Rooms => Set<Room>();
    public DbSet<RoomMember> RoomMembers => Set<RoomMember>();
    public DbSet<RoomInvitation> RoomInvitations => Set<RoomInvitation>();

    protected override void OnModelCreating(ModelBuilder model)
    {
        model.Entity<User>(e =>
        {
            e.ToTable("users");
            e.HasKey(x => x.Id);
            e.Property(x => x.Issuer).HasMaxLength(512);
            e.Property(x => x.Subject).HasMaxLength(512);
            e.Property(x => x.Handle).HasMaxLength(64);
            e.Property(x => x.DisplayName).HasMaxLength(128);
            e.Property(x => x.Email).HasMaxLength(320);
            e.Property(x => x.AvatarUrl).HasMaxLength(2048);
            e.HasIndex(x => new { x.Issuer, x.Subject }).IsUnique();
            e.HasIndex(x => x.Handle).IsUnique();
        });
        model.Entity<Device>(e =>
        {
            e.ToTable("devices"); e.HasKey(x => x.Id);
            e.Property(x => x.Id).HasMaxLength(256);
            e.Property(x => x.Label).HasMaxLength(128);
            e.Property(x => x.SignerDeviceId).HasMaxLength(256);
            e.HasIndex(x => x.UserId);
            e.HasOne<User>().WithMany().HasForeignKey(x => x.UserId).OnDelete(DeleteBehavior.Restrict);
        });
        model.Entity<DeviceList>(e =>
        {
            e.ToTable("device_lists"); e.HasKey(x => x.UserId);
            e.Property(x => x.SignerDeviceId).HasMaxLength(256);
            e.Property(x => x.Generation).IsConcurrencyToken();
            e.HasOne<User>().WithOne().HasForeignKey<DeviceList>(x => x.UserId).OnDelete(DeleteBehavior.Restrict);
        });
        model.Entity<DeviceChallenge>(e =>
        {
            e.ToTable("device_challenges"); e.HasKey(x => x.Id);
            e.HasIndex(x => x.ExpiresAt);
            e.HasOne<User>().WithMany().HasForeignKey(x => x.UserId).OnDelete(DeleteBehavior.Cascade);
        });
        model.Entity<DeviceLink>(e =>
        {
            e.ToTable("device_links"); e.HasKey(x => x.Id);
            e.Property(x => x.DeviceCodeHash).HasMaxLength(64);
            e.Property(x => x.UserCode).HasMaxLength(9);
            e.Property(x => x.DeviceId).HasMaxLength(256);
            e.Property(x => x.Label).HasMaxLength(128);
            e.Property(x => x.State).HasMaxLength(16);
            e.HasIndex(x => x.DeviceCodeHash).IsUnique();
            e.HasIndex(x => x.UserCode).IsUnique();
            e.HasIndex(x => x.ExpiresAt);
            e.HasOne<User>().WithMany().HasForeignKey(x => x.UserId).OnDelete(DeleteBehavior.Cascade);
        });
        model.Entity<Friendship>(e =>
        {
            e.ToTable("friendships"); e.HasKey(x => new { x.FirstUserId, x.SecondUserId });
            e.HasOne<User>().WithMany().HasForeignKey(x => x.FirstUserId).OnDelete(DeleteBehavior.Restrict);
            e.HasOne<User>().WithMany().HasForeignKey(x => x.SecondUserId).OnDelete(DeleteBehavior.Restrict);
        });
        model.Entity<Session>(e =>
        {
            e.ToTable("sessions"); e.HasKey(x => x.Id);
            e.Property(x => x.HostDeviceId).HasMaxLength(256);
            e.Property(x => x.HostName).HasMaxLength(128);
            e.Property(x => x.Name).HasMaxLength(128);
            e.Property(x => x.AuthorizationRevision).IsConcurrencyToken();
            e.HasIndex(x => x.IncarnationId).IsUnique();
            e.HasIndex(x => new { x.Ended, x.ExpiresAt });
            e.HasIndex(x => new { x.OwnerUserId, x.Ended });
            e.HasOne<User>().WithMany().HasForeignKey(x => x.OwnerUserId).OnDelete(DeleteBehavior.Restrict);
            e.HasOne<Device>().WithMany().HasForeignKey(x => x.HostDeviceId).OnDelete(DeleteBehavior.Restrict);
            e.HasOne<Room>().WithMany().HasForeignKey(x => x.RoomId).OnDelete(DeleteBehavior.SetNull);
        });
        model.Entity<SessionMember>(e =>
        {
            e.ToTable("session_members"); e.HasKey(x => new { x.SessionId, x.UserId });
            e.HasOne<Session>().WithMany().HasForeignKey(x => x.SessionId).OnDelete(DeleteBehavior.Cascade);
            e.HasOne<User>().WithMany().HasForeignKey(x => x.UserId).OnDelete(DeleteBehavior.Restrict);
        });
        model.Entity<SessionKeyEnvelope>(e =>
        {
            e.ToTable("session_keys"); e.HasKey(x => new { x.SessionId, x.RecipientDeviceId });
            e.Property(x => x.RecipientDeviceId).HasMaxLength(256);
            e.Property(x => x.SenderDeviceId).HasMaxLength(256);
            e.HasOne<Session>().WithMany().HasForeignKey(x => x.SessionId).OnDelete(DeleteBehavior.Cascade);
            e.HasOne<Device>().WithMany().HasForeignKey(x => x.RecipientDeviceId).OnDelete(DeleteBehavior.Restrict);
        });
        model.Entity<Room>(e =>
        {
            e.ToTable("rooms"); e.HasKey(x => x.Id);
            e.Property(x => x.Name).HasMaxLength(128);
            e.Property(x => x.Slug).HasMaxLength(64);
            e.HasIndex(x => x.Slug).IsUnique();
            e.HasOne<User>().WithMany().HasForeignKey(x => x.OwnerUserId).OnDelete(DeleteBehavior.Restrict);
        });
        model.Entity<RoomMember>(e =>
        {
            e.ToTable("room_members"); e.HasKey(x => new { x.RoomId, x.UserId });
            e.HasOne<Room>().WithMany().HasForeignKey(x => x.RoomId).OnDelete(DeleteBehavior.Cascade);
            e.HasOne<User>().WithMany().HasForeignKey(x => x.UserId).OnDelete(DeleteBehavior.Restrict);
        });
        model.Entity<RoomInvitation>(e =>
        {
            e.ToTable("room_invitations"); e.HasKey(x => x.Id);
            e.HasIndex(x => new { x.RoomId, x.UserId }).IsUnique();
            e.HasOne<Room>().WithMany().HasForeignKey(x => x.RoomId).OnDelete(DeleteBehavior.Cascade);
            e.HasOne<User>().WithMany().HasForeignKey(x => x.UserId).OnDelete(DeleteBehavior.Restrict);
        });
    }
}
