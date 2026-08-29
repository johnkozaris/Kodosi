using Microsoft.Extensions.Logging.Abstractions;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class UserServiceTests
{
    [Fact]
    public async Task RejectFriendRequestAsync_Removes_Incoming_Request()
    {
        var currentUser = User.Create(UserId.New(), "current@example.com", "current", "Current User");
        var otherUser = User.Create(UserId.New(), "other@example.com", "other", "Other User");
        var friendship = Friendship.CreateRequest(otherUser.Id, currentUser.Id);
        var friendships = new TestFriendshipRepository(friendship);
        var unitOfWork = new TestUnitOfWork();
        var service = new UserService(
            new TestUserRepository(currentUser, otherUser),
            friendships,
            unitOfWork,
            new PermissiveThrottle(),
            NullLogger<UserService>.Instance);

        var otherUserId = await service.RejectFriendRequestAsync(
            currentUser.Id,
            otherUser.Handle,
            TestContext.Current.CancellationToken);

        Assert.Equal(otherUser.Id, otherUserId);
        Assert.True(friendships.RemoveCalled);
        Assert.Equal(1, unitOfWork.SaveCalls);
    }

    [Fact]
    public async Task RejectFriendRequestAsync_Rejects_Outgoing_Request_Cancellation()
    {
        var currentUser = User.Create(UserId.New(), "current@example.com", "current", "Current User");
        var otherUser = User.Create(UserId.New(), "other@example.com", "other", "Other User");
        var friendship = Friendship.CreateRequest(currentUser.Id, otherUser.Id);
        var friendships = new TestFriendshipRepository(friendship);
        var unitOfWork = new TestUnitOfWork();
        var service = new UserService(
            new TestUserRepository(currentUser, otherUser),
            friendships,
            unitOfWork,
            new PermissiveThrottle(),
            NullLogger<UserService>.Instance);

        var error = await Assert.ThrowsAsync<InvalidStateException>(() =>
            service.RejectFriendRequestAsync(
                currentUser.Id,
                otherUser.Handle,
                TestContext.Current.CancellationToken));

        Assert.Equal($"No incoming friend request from '{otherUser.Handle}'.", error.Message);
        Assert.False(friendships.RemoveCalled);
        Assert.Equal(0, unitOfWork.SaveCalls);
    }

    [Fact]
    public async Task SendFriendRequestAsync_Throttled_By_PerTarget_Window()
    {
        var sender = User.Create(UserId.New(), "sender@example.com", "sender", "Sender");
        var target = User.Create(UserId.New(), "target@example.com", "target", "Target");
        var friendships = new TestFriendshipRepository(friendship: null);
        var retryAfter = TimeSpan.FromHours(12);
        var throttle = new ThrowingThrottle(retryAfter);
        var service = new UserService(
            new TestUserRepository(sender, target),
            friendships,
            new TestUnitOfWork(),
            throttle,
            NullLogger<UserService>.Instance);

        var error = await Assert.ThrowsAsync<FriendRequestThrottledException>(() =>
            service.SendFriendRequestAsync(
                sender.Id,
                target.Handle,
                TestContext.Current.CancellationToken));

        Assert.Equal(retryAfter, error.RetryAfter);
    }

    [Fact]
    public async Task SendFriendRequestAsync_Existing_Pending_Returns_Conflict_Not_Throttle()
    {
        var sender = User.Create(UserId.New(), "sender@example.com", "sender", "Sender");
        var target = User.Create(UserId.New(), "target@example.com", "target", "Target");
        var existing = Friendship.CreateRequest(sender.Id, target.Id);
        var throttle = new ThrowingThrottle(TimeSpan.FromHours(12));
        var service = new UserService(
            new TestUserRepository(sender, target),
            new TestFriendshipRepository(existing),
            new TestUnitOfWork(),
            throttle,
            NullLogger<UserService>.Instance);

        await Assert.ThrowsAsync<ConflictException>(() =>
            service.SendFriendRequestAsync(
                sender.Id,
                target.Handle,
                TestContext.Current.CancellationToken));

        Assert.False(throttle.Called);
    }

    [Fact]
    public async Task SendFriendRequestAsync_Releases_Throttle_Slot_On_Save_Failure()
    {
        var sender = User.Create(UserId.New(), "sender@example.com", "sender", "Sender");
        var target = User.Create(UserId.New(), "target@example.com", "target", "Target");
        var friendships = new TestFriendshipRepository(friendship: null);
        var throttle = new PermissiveThrottle();
        var service = new UserService(
            new TestUserRepository(sender, target),
            friendships,
            new ThrowingUnitOfWork(),
            throttle,
            NullLogger<UserService>.Instance);

        await Assert.ThrowsAsync<InvalidOperationException>(() =>
            service.SendFriendRequestAsync(
                sender.Id,
                target.Handle,
                TestContext.Current.CancellationToken));

        Assert.Equal(1, throttle.ClaimCount);
        Assert.Equal(1, throttle.ReleaseCount);
    }

    [Fact]
    public async Task RemoveFriendship_Holds_Sorted_Session_Leases_Through_PostCommit_Fanout()
    {
        var currentUser = User.Create(UserId.New(),
            "current@example.com",
            "current",
            "Current User");
        var otherUser = User.Create(UserId.New(),
            "other@example.com",
            "other",
            "Other User");
        var friendship = Friendship.CreateRequest(
            currentUser.Id,
            otherUser.Id);
        friendship.Accept(otherUser.Id);
        var friendships = new TestFriendshipRepository(friendship);
        var unitOfWork = new WorkflowUnitOfWork();
        var users = new UserService(
            new TestUserRepository(currentUser, otherUser),
            friendships,
            unitOfWork,
            new PermissiveThrottle(),
            NullLogger<UserService>.Instance);
        var currentSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            currentUser.Id,
            "current-session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        var otherSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            otherUser.Id,
            "other-session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        var sessionRepository = new FriendSessionRepository(
            currentSession,
            otherSession);
        var authority = new FakeSessionEndAuthority();
        var userLock = new FakeUserLifecycleLock();
        var workflow = new FriendshipWorkflowService(
            users,
            new SessionAccessOverrideRevoker(
                sessionRepository,
                new FakeAccessOverrideRepository(),
                new FakeSessionKeyBlobRepository()),
            new NoOpFriendshipAuditRepository(),
            userLock,
            authority,
            unitOfWork);

        var result = await workflow.RemoveFriendshipAsync(
            currentUser.Id,
            otherUser.Handle,
            new RequestAuditContext("127.0.0.1", "xunit"),
            TestContext.Current.CancellationToken);

        Assert.Equal(
            new[] { currentUser.Id, otherUser.Id }
                .OrderBy(userId => userId.Value),
            userLock.AcquiredUserIds);
        Assert.Equal(
            new[] { currentSession.Id, otherSession.Id }
                .OrderBy(sessionId => sessionId.Value),
            authority.AcquiredSessionIds);
        Assert.True(authority.LeaseHeld);
        await result.DisposeAsync();
        Assert.False(authority.LeaseHeld);
    }

    [Fact]
    public async Task RemoveFriendship_Preflight_Rejects_Unauthorized_Request_Before_User_Locks()
    {
        var currentUser = User.Create(UserId.New(),
            "current@example.com",
            "current",
            "Current User");
        var otherUser = User.Create(UserId.New(),
            "other@example.com",
            "other",
            "Other User");
        var unitOfWork = new WorkflowUnitOfWork();
        var userLock = new FakeUserLifecycleLock();
        var workflow = new FriendshipWorkflowService(
            new UserService(
                new TestUserRepository(currentUser, otherUser),
                new TestFriendshipRepository(friendship: null),
                unitOfWork,
                new PermissiveThrottle(),
                NullLogger<UserService>.Instance),
            new SessionAccessOverrideRevoker(
                new FriendSessionRepository(),
                new FakeAccessOverrideRepository(),
                new FakeSessionKeyBlobRepository()),
            new NoOpFriendshipAuditRepository(),
            userLock,
            new FakeSessionEndAuthority(),
            unitOfWork);

        await Assert.ThrowsAsync<NotFoundException>(() =>
            workflow.RemoveFriendshipAsync(
                currentUser.Id,
                otherUser.Handle,
                new RequestAuditContext("127.0.0.1", "xunit"),
                TestContext.Current.CancellationToken));

        Assert.Empty(userLock.AcquiredUserIds);
        Assert.Equal(0, unitOfWork.BeginTransactionCalls);
    }

    [Fact]
    public async Task RemoveFriendship_Honors_Request_Cancellation_Before_User_Locks()
    {
        var currentUser = User.Create(UserId.New(),
            "current@example.com",
            "current",
            "Current User");
        var otherUser = User.Create(UserId.New(),
            "other@example.com",
            "other",
            "Other User");
        var friendship = Friendship.CreateRequest(
            currentUser.Id,
            otherUser.Id);
        friendship.Accept(otherUser.Id);
        var unitOfWork = new WorkflowUnitOfWork();
        var userLock = new FakeUserLifecycleLock();
        var workflow = new FriendshipWorkflowService(
            new UserService(
                new TestUserRepository(currentUser, otherUser),
                new TestFriendshipRepository(friendship),
                unitOfWork,
                new PermissiveThrottle(),
                NullLogger<UserService>.Instance),
            new SessionAccessOverrideRevoker(
                new FriendSessionRepository(),
                new FakeAccessOverrideRepository(),
                new FakeSessionKeyBlobRepository()),
            new NoOpFriendshipAuditRepository(),
            userLock,
            new FakeSessionEndAuthority(),
            unitOfWork);
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();

        await Assert.ThrowsAnyAsync<OperationCanceledException>(() =>
            workflow.RemoveFriendshipAsync(
                currentUser.Id,
                otherUser.Handle,
                new RequestAuditContext("127.0.0.1", "xunit"),
                cancellation.Token));

        Assert.Empty(userLock.AcquiredUserIds);
        Assert.Equal(0, unitOfWork.BeginTransactionCalls);
    }

    [Fact]
    public async Task RemoveFriendship_Revalidates_After_User_Locks_Before_Session_Locks()
    {
        var currentUser = User.Create(UserId.New(),
            "current@example.com",
            "current",
            "Current User");
        var otherUser = User.Create(UserId.New(),
            "other@example.com",
            "other",
            "Other User");
        var friendship = Friendship.CreateRequest(
            currentUser.Id,
            otherUser.Id);
        friendship.Accept(otherUser.Id);
        var friendships = new TestFriendshipRepository(friendship);
        var unitOfWork = new WorkflowUnitOfWork();
        var authority = new FakeSessionEndAuthority();
        var userLock = new ClearingUserLifecycleLock(
            friendships.Clear);
        var workflow = new FriendshipWorkflowService(
            new UserService(
                new TestUserRepository(currentUser, otherUser),
                friendships,
                unitOfWork,
                new PermissiveThrottle(),
                NullLogger<UserService>.Instance),
            new SessionAccessOverrideRevoker(
                new FriendSessionRepository(),
                new FakeAccessOverrideRepository(),
                new FakeSessionKeyBlobRepository()),
            new NoOpFriendshipAuditRepository(),
            userLock,
            authority,
            unitOfWork);

        await Assert.ThrowsAsync<NotFoundException>(() =>
            workflow.RemoveFriendshipAsync(
                currentUser.Id,
                otherUser.Handle,
                new RequestAuditContext("127.0.0.1", "xunit"),
                TestContext.Current.CancellationToken));

        Assert.Equal(2, userLock.AcquiredUserIds.Count);
        Assert.Empty(authority.AcquiredSessionIds);
    }

    private sealed class PermissiveThrottle : IFriendRequestThrottle
    {
        public int ClaimCount { get; private set; }
        public int ReleaseCount { get; private set; }

        public TimeSpan? TryClaim(UserId senderId, UserId targetId)
        {
            ClaimCount++;
            return null;
        }

        public void Release(UserId senderId, UserId targetId)
        {
            ReleaseCount++;
        }

        public void Sweep() { }
    }

    private sealed class ThrowingThrottle(TimeSpan retryAfter) : IFriendRequestThrottle
    {
        public bool Called { get; private set; }

        public TimeSpan? TryClaim(UserId senderId, UserId targetId)
        {
            Called = true;
            return retryAfter;
        }

        public void Release(UserId senderId, UserId targetId) { }

        public void Sweep() { }
    }

    private sealed class TestUserRepository(params User[] users) : IUserRepository
    {
        private readonly Dictionary<UserId, User> _usersById = users.ToDictionary(user => user.Id);
        private readonly Dictionary<string, User> _usersByHandle = users.ToDictionary(
            user => user.Handle,
            user => user,
            StringComparer.Ordinal);

        public Task<User?> GetByIdAsync(UserId id, CancellationToken ct = default)
            => Task.FromResult(_usersById.GetValueOrDefault(id));

        public Task<User?> GetByHandleAsync(string handle, CancellationToken ct = default)
            => Task.FromResult(_usersByHandle.GetValueOrDefault(handle));

        public Task<IReadOnlyList<string>> GetHandlesByPrefixAsync(string prefix, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<string>>([]);

        public Task<IReadOnlyList<User>> GetByIdsAsync(IReadOnlyList<UserId> ids, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<User>>(ids
                .Select(id => _usersById.GetValueOrDefault(id))
                .Where(user => user is not null)
                .Cast<User>()
                .ToList());

        public Task AddAsync(User user, CancellationToken ct = default) => Task.CompletedTask;
    }

    private sealed class TestFriendshipRepository(Friendship? friendship) : IFriendshipRepository
    {
        private Friendship? _friendship = friendship;

        public bool RemoveCalled { get; private set; }

        public Task<bool> AreFriendsAsync(UserId userA, UserId userB, CancellationToken ct = default)
            => Task.FromResult(
                _friendship is not null
                && (_friendship.UserLowId == userA || _friendship.UserHighId == userA)
                && (_friendship.UserLowId == userB || _friendship.UserHighId == userB)
                && _friendship.Status == FriendshipStatus.Accepted);

        public Task<IReadOnlyList<UserId>> GetFriendIdsAsync(UserId userId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<UserId>>([]);

        public Task<IReadOnlyList<Friendship>> GetPendingIncomingAsync(UserId userId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<Friendship>>([]);

        public Task<IReadOnlyList<Friendship>> GetPendingOutgoingAsync(UserId userId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<Friendship>>([]);

        public Task<Friendship?> GetAsync(UserId userA, UserId userB, CancellationToken ct = default)
            => Task.FromResult(
                _friendship is not null
                && (_friendship.UserLowId == userA || _friendship.UserHighId == userA)
                && (_friendship.UserLowId == userB || _friendship.UserHighId == userB)
                    ? _friendship
                    : null);

        public Task AddAsync(Friendship friendship, CancellationToken ct = default)
        {
            _friendship = friendship;
            return Task.CompletedTask;
        }

        public void Remove(Friendship friendship)
        {
            if (ReferenceEquals(_friendship, friendship))
            {
                _friendship = null;
            }

            RemoveCalled = true;
        }

        public void Clear() => _friendship = null;
    }

    private sealed class TestUnitOfWork : UnitOfWorkStub
    {
        public int SaveCalls { get; private set; }

        public override Task SaveChangesAsync(CancellationToken ct = default)
        {
            SaveCalls++;
            return Task.CompletedTask;
        }
    }

    private sealed class ThrowingUnitOfWork : UnitOfWorkStub
    {
        public override Task SaveChangesAsync(CancellationToken ct = default)
            => throw new InvalidOperationException("simulated DB failure");
    }

    private sealed class WorkflowUnitOfWork : UnitOfWorkStub
    {
        public int BeginTransactionCalls { get; private set; }

        public override Task SaveChangesAsync(CancellationToken ct = default) =>
            Task.CompletedTask;

        public override Task<ITransactionScope> BeginTransactionAsync(
            CancellationToken ct = default)
        {
            BeginTransactionCalls++;
            return Task.FromResult<ITransactionScope>(
                new CompletedTransactionScope());
        }
    }

    private sealed class ClearingUserLifecycleLock(Action clear)
        : IUserLifecycleLock
    {
        private readonly Action _clear = clear;

        public List<UserId> AcquiredUserIds { get; } = [];

        public Task AcquireAsync(
            UserId userId,
            CancellationToken ct = default)
        {
            AcquiredUserIds.Add(userId);
            if (AcquiredUserIds.Count == 2)
            {
                _clear();
            }
            return Task.CompletedTask;
        }
    }

    private sealed class NoOpFriendshipAuditRepository
        : IFriendshipAuditRepository
    {
        public Task AddAsync(
            FriendshipAuditEntry entry,
            CancellationToken ct = default) =>
            Task.CompletedTask;
    }

    private sealed class FriendSessionRepository(params Session[] sessions)
        : SessionRepositoryStub
    {
        private readonly IReadOnlyList<Session> _sessions = sessions;

        public override Task<IReadOnlyList<SessionCardProjection>>
            GetAllNonEndedFriendSessionsByOwnerAsync(
                UserId ownerUserId,
                CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<SessionCardProjection>>(
                _sessions
                    .Where(session =>
                        session.OwnerUserId == ownerUserId
                        && session.Status != SessionStatus.Ended)
                    .Select(session => new SessionCardProjection(
                        session.Id.Value,
                        session.Title,
                        session.Scope,
                        session.DefaultAccess,
                        session.Status,
                        session.OwnerUserId.Value,
                        "Owner",
                        null,
                        session.StartedAt,
                        session.IncarnationId,
                        session.ToolKind,
                        session.RoomId?.Value))
                    .ToList());
    }
}
