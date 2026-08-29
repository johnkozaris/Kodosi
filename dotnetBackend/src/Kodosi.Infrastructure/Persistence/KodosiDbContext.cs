using Microsoft.EntityFrameworkCore;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence;

public sealed class KodosiDbContext(DbContextOptions<KodosiDbContext> options) : DbContext(options)
{
    public DbSet<User> Users => Set<User>();
    public DbSet<ExternalIdentity> ExternalIdentities => Set<ExternalIdentity>();
    public DbSet<Friendship> Friendships => Set<Friendship>();
    public DbSet<Room> Rooms => Set<Room>();
    public DbSet<RoomRosterTransition> RoomRosterTransitions => Set<RoomRosterTransition>();
    public DbSet<RoomMember> RoomMembers => Set<RoomMember>();
    public DbSet<RoomInvitation> RoomInvitations => Set<RoomInvitation>();
    public DbSet<RoomChatMessage> RoomChatMessages => Set<RoomChatMessage>();
    public DbSet<RoomTask> RoomTasks => Set<RoomTask>();
    public DbSet<RoomMutationReceipt> RoomMutationReceipts => Set<RoomMutationReceipt>();
    public DbSet<RoomMutationSessionEffect> RoomMutationSessionEffects =>
        Set<RoomMutationSessionEffect>();
    public DbSet<Session> Sessions => Set<Session>();
    public DbSet<SessionEndMutation> SessionEndMutations => Set<SessionEndMutation>();
    public DbSet<SessionAccessMutation> SessionAccessMutations => Set<SessionAccessMutation>();
    public DbSet<SessionAccessOverride> SessionAccessOverrides => Set<SessionAccessOverride>();
    public DbSet<SessionViewerDismissal> SessionViewerDismissals => Set<SessionViewerDismissal>();
    public DbSet<InputAuditEntry> InputAuditEntries => Set<InputAuditEntry>();
    public DbSet<SemanticRelayRequest> SemanticRelayRequests => Set<SemanticRelayRequest>();
    public DbSet<SemanticRelayReceipt> SemanticRelayReceipts => Set<SemanticRelayReceipt>();
    public DbSet<IdentityResetAuditEntry> IdentityResetAuditEntries =>
        Set<IdentityResetAuditEntry>();
    public DbSet<IdentityExposure> IdentityExposures => Set<IdentityExposure>();
    public DbSet<UserDevice> UserDevices => Set<UserDevice>();
    public DbSet<UserDeviceList> UserDeviceLists => Set<UserDeviceList>();
    public DbSet<SessionKeyBlob> SessionKeyBlobs => Set<SessionKeyBlob>();
    public DbSet<DeviceRegistrationChallenge> DeviceRegistrationChallenges =>
        Set<DeviceRegistrationChallenge>();
    public DbSet<DeviceLinkRequest> DeviceLinkRequests => Set<DeviceLinkRequest>();

    protected override void OnModelCreating(ModelBuilder modelBuilder)
    {
        modelBuilder.ApplyConfigurationsFromAssembly(typeof(KodosiDbContext).Assembly);
    }
}
