using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Infrastructure.Crypto;

namespace Kodosi.HostTests;

internal static class SecurityAuthorizationTestFactory
{
    public static SessionCreator CreateSessionCreator(
        Session? session = null,
        IRoomMemberRepository? roomMemberRepository = null,
        IAccessOverrideRepository? accessOverrideRepository = null,
        ISessionViewerDismissalRepository? dismissalRepository = null,
        ISessionEndAuthority? sessionEndAuthority = null)
    {
        var roomMembers = roomMemberRepository ?? new FakeRoomMemberRepository();
        var overrides = accessOverrideRepository ?? new FakeAccessOverrideRepository();
        var dismissals = dismissalRepository ?? new FakeSessionViewerDismissalRepository();
        var sessions = new FakeSessionRepository(session);
        return new SessionCreator(
            sessions,
            new FakeSessionIncarnationRepository(session),
            new OwnerSessionSecretHasher(),
            new SessionRoomResolver(roomMembers),
            new SessionAccessOverrideRevoker(
                sessions,
                overrides,
                new FakeSessionKeyBlobRepository()),
            dismissals,
            new FakeSessionKeyBlobRepository(),
            new FakeUserLifecycleLock(),
            new FakeRoomLifecycleLock(),
            sessionEndAuthority ?? new FakeSessionEndAuthority(),
            new FakeUnitOfWork());
    }

    public static SessionReader CreateSessionReader(
        Session session,
        IFriendshipRepository? friendshipRepository = null,
        IRoomMemberRepository? roomMemberRepository = null,
        IAccessOverrideRepository? accessOverrideRepository = null,
        ISessionViewerDismissalRepository? dismissalRepository = null) =>
        new(
            new FakeSessionRepository(session),
            CreateAccessService(
                friendshipRepository,
                roomMemberRepository,
                accessOverrideRepository,
                dismissalRepository),
            new FakeRuntimeDirectory());

    public static SessionUpdater CreateSessionUpdater(
        Session session,
        IFriendshipRepository? friendshipRepository = null,
        IRoomMemberRepository? roomMemberRepository = null,
        IAccessOverrideRepository? accessOverrideRepository = null,
        ISessionEndAuthority? sessionEndAuthority = null)
    {
        var roomMembers = roomMemberRepository ?? new FakeRoomMemberRepository();
        return new SessionUpdater(
            new FakeSessionRepository(session),
            new SessionRoomResolver(roomMembers),
            accessOverrideRepository ?? new FakeAccessOverrideRepository(),
            new FakeAccessOverrideAuditRepository(),
            friendshipRepository ?? new FakeFriendshipRepository(),
            roomMembers,
            new FakeUserLifecycleLock(),
            new FakeRoomLifecycleLock(),
            sessionEndAuthority ?? new FakeSessionEndAuthority(),
            new FakeUnitOfWork());
    }

    public static SessionAccessService CreateAccessService(
        IFriendshipRepository? friendshipRepository = null,
        IRoomMemberRepository? roomMemberRepository = null,
        IAccessOverrideRepository? accessOverrideRepository = null,
        ISessionViewerDismissalRepository? dismissalRepository = null)
    {
        return new SessionAccessService(
            friendshipRepository ?? new FakeFriendshipRepository(),
            roomMemberRepository ?? new FakeRoomMemberRepository(),
            accessOverrideRepository ?? new FakeAccessOverrideRepository(),
            dismissalRepository ?? new FakeSessionViewerDismissalRepository());
    }

    public static SessionKeyQueryService CreateSessionKeyQueryService(
        Session? session = null,
        IUserDeviceRepository? userDeviceRepository = null,
        IUserDeviceListRepository? userDeviceListRepository = null,
        ISessionKeyBlobRepository? sessionKeyBlobRepository = null,
        IFriendshipRepository? friendshipRepository = null,
        IRoomMemberRepository? roomMemberRepository = null,
        IAccessOverrideRepository? accessOverrideRepository = null,
        TimeProvider? timeProvider = null)
    {
        var accessService = CreateAccessService(
            friendshipRepository,
            roomMemberRepository,
            accessOverrideRepository);

        return new SessionKeyQueryService(
            new FakeSessionRepository(session),
            sessionKeyBlobRepository ?? new FakeSessionKeyBlobRepository(),
            userDeviceRepository ?? new FakeUserDeviceRepository(),
            userDeviceListRepository ?? new FakeUserDeviceListRepository(),
            accessService,
            timeProvider ?? TimeProvider.System);
    }

    public static Session CreateSession(
        UserId ownerId,
        SessionScope scope,
        RoomId? roomId = null)
    {
        return Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "Session",
            scope,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            new OwnerSessionSecretHasher().Hash("owner-secret"),
            roomId);
    }
}
