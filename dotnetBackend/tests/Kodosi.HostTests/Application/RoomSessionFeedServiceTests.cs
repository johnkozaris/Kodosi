using System.Text;
using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Serialization;

using static Kodosi.HostTests.SecurityAuthorizationTestFactory;

namespace Kodosi.HostTests;

public sealed class RoomSessionFeedServiceTests
{
    [Fact]
    public async Task RoomFeed_Rejects_NonMembers()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = CreateSession(ownerId, SessionScope.Room, roomId);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");

        var service = CreateRoomFeedService(new FakeSessionRepository(session), new FakeRoomMemberRepository());

        await Assert.ThrowsAsync<PolicyViolationException>(
            () => service.GetRoomFeedAsync(roomId, actorId, ct: TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task RoomFeed_Rejects_Removed_Cursor_As_Invalid()
    {
        var actorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var startedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_752_806_523_123);
        var legacyCursor = Convert.ToBase64String(
            Encoding.UTF8.GetBytes($"{startedAt.ToUnixTimeMilliseconds()}_{Guid.NewGuid():D}"));
        var service = CreateRoomFeedService(
            new FakeSessionRepository(),
            new FakeRoomMemberRepository((roomId, actorId)));

        var error = await Assert.ThrowsAsync<DomainException>(() => service.GetRoomFeedAsync(
            roomId,
            actorId,
            cursor: legacyCursor,
            ct: TestContext.Current.CancellationToken));

        Assert.Equal("The feed cursor is invalid.", error.Message);
    }

    [Fact]
    public async Task RoomFeed_Returns_Sessions_For_Members()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = CreateSession(ownerId, SessionScope.Room, roomId);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");

        var service = CreateRoomFeedService(
            new FakeSessionRepository(session),
            new FakeRoomMemberRepository((roomId, actorId)));

        var result = await service.GetRoomFeedAsync(roomId, actorId, ct: TestContext.Current.CancellationToken);

        var card = Assert.Single(result.Items);
        Assert.Equal(session.Id.Value.ToString(), card.Id);
        Assert.Equal(AccessLevel.Suggest, card.Access);
    }

    [Fact]
    public async Task RoomFeed_Returns_Override_Access_For_Members()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = CreateSession(ownerId, SessionScope.Room, roomId);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");

        var service = CreateRoomFeedService(
            new FakeSessionRepository(session),
            new FakeRoomMemberRepository((roomId, actorId)),
            new FakeAccessOverrideRepository(
                SessionAccessOverride.Create(session.Id, actorId, AccessLevel.View, ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow)));

        var result = await service.GetRoomFeedAsync(roomId, actorId, ct: TestContext.Current.CancellationToken);

        var card = Assert.Single(result.Items);
        Assert.Equal(AccessLevel.View, card.Access);
    }

    [Fact]
    public async Task RoomFeed_Excludes_Session_Dismissed_By_Viewer()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = CreateSession(ownerId, SessionScope.Room, roomId);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var dismissals = new FakeSessionViewerDismissalRepository(
            SessionViewerDismissal.Create(
                session.Id,
                actorId));
        var service = CreateRoomFeedService(
            new FakeSessionRepository(session),
            new FakeRoomMemberRepository((roomId, actorId)),
            dismissals: dismissals);

        var result = await service.GetRoomFeedAsync(
            roomId,
            actorId,
            ct: TestContext.Current.CancellationToken);

        Assert.Empty(result.Items);
        Assert.Equal(1, dismissals.DismissedSessionBatchCalls);
    }

    [Fact]
    public async Task RoomFeed_Batches_PerCard_RoomMembershipChecks()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var first = CreateSession(ownerId, SessionScope.Room, roomId);
        var second = CreateSession(ownerId, SessionScope.Room, roomId);
        first.ActivateHost("test-host");
        first.ReleaseHostSlot("test-host");
        second.ActivateHost("test-host");
        second.ReleaseHostSlot("test-host");
        var sessions = new FakeSessionRepository(first);
        await sessions.AddAsync(second, TestContext.Current.CancellationToken);
        var roomMembers = new TrackingRoomMemberRepository(actorId, roomId);

        var service = CreateRoomFeedService(sessions, roomMembers);
        var result = await service.GetRoomFeedAsync(roomId, actorId, ct: TestContext.Current.CancellationToken);

        Assert.Equal(2, result.Items.Count);
        Assert.All(result.Items, card => Assert.Equal(AccessLevel.Suggest, card.Access));
        Assert.Equal(1, roomMembers.IsMemberCalls);
        Assert.Equal(1, roomMembers.GetActiveRoomIdsCalls);
        Assert.Equal([roomId], roomMembers.LastRequestedRoomIds);
    }

    [Fact]
    public async Task RoomFeed_ParticipantSort_Returns_Complete_Snapshots_When_Counts_Change()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var first = CreateSession(ownerId, SessionScope.Room, roomId);
        var second = CreateSession(ownerId, SessionScope.Room, roomId);
        var third = CreateSession(ownerId, SessionScope.Room, roomId);
        first.ActivateHost("test-host");
        first.ReleaseHostSlot("test-host");
        second.ActivateHost("test-host");
        second.ReleaseHostSlot("test-host");
        third.ActivateHost("test-host");
        third.ReleaseHostSlot("test-host");
        var sessions = new FakeSessionRepository(first);
        await sessions.AddAsync(second, TestContext.Current.CancellationToken);
        await sessions.AddAsync(third, TestContext.Current.CancellationToken);
        var runtimes = new ParticipantCountRuntimeDirectory(
            (first.Id, 1),
            (second.Id, 3),
            (third.Id, 1));
        var service = CreateRoomFeedService(
            sessions,
            new FakeRoomMemberRepository((roomId, actorId)),
            runtimes: runtimes);

        var firstSnapshot = await service.GetRoomFeedAsync(
            roomId,
            actorId,
            limit: 3,
            sortBy: "participants",
            ct: TestContext.Current.CancellationToken);
        runtimes.SetParticipantCount(first.Id, 5);
        runtimes.SetParticipantCount(second.Id, 0);
        runtimes.SetParticipantCount(third.Id, 2);
        var secondSnapshot = await service.GetRoomFeedAsync(
            roomId,
            actorId,
            limit: 3,
            sortBy: "participants",
            ct: TestContext.Current.CancellationToken);

        var expectedIds = new[]
        {
            first.Id.Value.ToString(),
            second.Id.Value.ToString(),
            third.Id.Value.ToString(),
        };
        Assert.Equal(expectedIds.Order(), firstSnapshot.Items.Select(item => item.Id).Order());
        Assert.Equal(expectedIds.Order(), secondSnapshot.Items.Select(item => item.Id).Order());
        Assert.Equal(expectedIds.Length, firstSnapshot.Items.Select(item => item.Id).Distinct().Count());
        Assert.Equal(expectedIds.Length, secondSnapshot.Items.Select(item => item.Id).Distinct().Count());
        Assert.Equal(second.Id.Value.ToString(), firstSnapshot.Items[0].Id);
        Assert.Equal(first.Id.Value.ToString(), secondSnapshot.Items[0].Id);
        Assert.Equal(third.Id.Value.ToString(), secondSnapshot.Items[1].Id);
        Assert.False(firstSnapshot.HasMore);
        Assert.False(secondSnapshot.HasMore);
        Assert.Null(firstSnapshot.NextCursor);
        Assert.Null(secondSnapshot.NextCursor);
    }

    [Fact]
    public async Task RoomFeed_ParticipantSort_Rejects_All_Cursors()
    {
        var actorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var service = CreateRoomFeedService(
            new FakeSessionRepository(),
            new FakeRoomMemberRepository((roomId, actorId)));
        var cursor = new FeedCursor(DateTimeOffset.UtcNow, Guid.NewGuid()).Encode();

        await Assert.ThrowsAsync<DomainException>(() => service.GetRoomFeedAsync(
            roomId,
            actorId,
            cursor: cursor,
            sortBy: "participants",
            ct: TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task RoomFeed_ParticipantSort_Truncates_To_The_Requested_Limit()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var first = CreateSession(ownerId, SessionScope.Room, roomId);
        var second = CreateSession(ownerId, SessionScope.Room, roomId);
        var third = CreateSession(ownerId, SessionScope.Room, roomId);
        first.ActivateHost("test-host");
        first.ReleaseHostSlot("test-host");
        second.ActivateHost("test-host");
        second.ReleaseHostSlot("test-host");
        third.ActivateHost("test-host");
        third.ReleaseHostSlot("test-host");
        var sessions = new FakeSessionRepository(first);
        await sessions.AddAsync(second, TestContext.Current.CancellationToken);
        await sessions.AddAsync(third, TestContext.Current.CancellationToken);
        var service = CreateRoomFeedService(
            sessions,
            new FakeRoomMemberRepository((roomId, actorId)),
            runtimes: new ParticipantCountRuntimeDirectory(
                (first.Id, 1),
                (second.Id, 9),
                (third.Id, 4)));

        var result = await service.GetRoomFeedAsync(
            roomId,
            actorId,
            limit: 2,
            sortBy: "participants",
            ct: TestContext.Current.CancellationToken);

        Assert.Equal(
            [second.Id.Value.ToString(), third.Id.Value.ToString()],
            result.Items.Select(item => item.Id));
        Assert.True(result.Truncated);
        Assert.False(result.HasMore);
        Assert.Null(result.NextCursor);
    }

    [Fact]
    public async Task RoomFeed_ParticipantSort_Ranks_Globally_Before_Truncating()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var first = CreateSession(ownerId, SessionScope.Room, roomId);
        var second = CreateSession(ownerId, SessionScope.Room, roomId);
        var third = CreateSession(ownerId, SessionScope.Room, roomId);
        first.ActivateHost("test-host");
        first.ReleaseHostSlot("test-host");
        second.ActivateHost("test-host");
        second.ReleaseHostSlot("test-host");
        third.ActivateHost("test-host");
        third.ReleaseHostSlot("test-host");
        var sessions = new FakeSessionRepository(first);
        await sessions.AddAsync(second, TestContext.Current.CancellationToken);
        await sessions.AddAsync(third, TestContext.Current.CancellationToken);
        var roomMembers = new FakeRoomMemberRepository((roomId, actorId));



        var startedAtOrder = await CreateRoomFeedService(sessions, roomMembers).GetRoomFeedAsync(
            roomId,
            actorId,
            limit: 3,
            ct: TestContext.Current.CancellationToken);
        var lastByStartedAt = startedAtOrder.Items[^1].Id;
        var runtimes = new ParticipantCountRuntimeDirectory(
            (first.Id, first.Id.Value.ToString() == lastByStartedAt ? 9 : 1),
            (second.Id, second.Id.Value.ToString() == lastByStartedAt ? 9 : 1),
            (third.Id, third.Id.Value.ToString() == lastByStartedAt ? 9 : 1));

        var result = await CreateRoomFeedService(sessions, roomMembers, runtimes: runtimes)
            .GetRoomFeedAsync(
                roomId,
                actorId,
                limit: 1,
                sortBy: "participants",
                ct: TestContext.Current.CancellationToken);

        var card = Assert.Single(result.Items);
        Assert.Equal(lastByStartedAt, card.Id);
        Assert.Equal(9, card.ParticipantCount);
        Assert.True(result.Truncated);
        Assert.False(result.HasMore);
        Assert.Null(result.NextCursor);
    }

    [Fact]
    public async Task RoomFeed_ParticipantSort_Truncates_At_The_Scan_Cap()
    {
        const int maxLimit = 100;
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var sessions = new FakeSessionRepository();
        for (var index = 0; index < maxLimit + 1; index++)
        {
            var session = CreateSession(ownerId, SessionScope.Room, roomId);
            session.ActivateHost("test-host");
            session.ReleaseHostSlot("test-host");
            await sessions.AddAsync(session, TestContext.Current.CancellationToken);
        }

        var service = CreateRoomFeedService(
            sessions,
            new FakeRoomMemberRepository((roomId, actorId)));

        var result = await service.GetRoomFeedAsync(
            roomId,
            actorId,
            limit: int.MaxValue,
            sortBy: "participants",
            ct: TestContext.Current.CancellationToken);

        Assert.Equal(maxLimit, result.Items.Count);
        Assert.Equal(maxLimit, result.Items.Select(item => item.Id).Distinct().Count());
        Assert.True(result.Truncated);
        Assert.False(result.HasMore);
        Assert.Null(result.NextCursor);
    }

    [Fact]
    public async Task RoomFeed_ParticipantSort_Reports_Complete_Snapshot_As_Untruncated()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = CreateSession(ownerId, SessionScope.Room, roomId);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var service = CreateRoomFeedService(
            new FakeSessionRepository(session),
            new FakeRoomMemberRepository((roomId, actorId)));

        var result = await service.GetRoomFeedAsync(
            roomId,
            actorId,
            limit: 20,
            sortBy: "participants",
            ct: TestContext.Current.CancellationToken);

        Assert.Single(result.Items);
        Assert.False(result.Truncated);
        Assert.False(result.HasMore);
        Assert.Null(result.NextCursor);
    }

    [Fact]
    public async Task RoomFeed_ParticipantSort_Batches_Authorization_Once_Per_Request()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var first = CreateSession(ownerId, SessionScope.Room, roomId);
        var second = CreateSession(ownerId, SessionScope.Room, roomId);
        first.ActivateHost("test-host");
        first.ReleaseHostSlot("test-host");
        second.ActivateHost("test-host");
        second.ReleaseHostSlot("test-host");
        var sessions = new FakeSessionRepository(first);
        await sessions.AddAsync(second, TestContext.Current.CancellationToken);
        var roomMembers = new TrackingRoomMemberRepository(actorId, roomId);
        var dismissals = new FakeSessionViewerDismissalRepository();

        var result = await CreateRoomFeedService(sessions, roomMembers, dismissals: dismissals)
            .GetRoomFeedAsync(
                roomId,
                actorId,
                limit: 1,
                sortBy: "participants",
                ct: TestContext.Current.CancellationToken);

        Assert.Single(result.Items);
        Assert.True(result.Truncated);
        Assert.Equal(1, roomMembers.IsMemberCalls);
        Assert.Equal(1, roomMembers.GetActiveRoomIdsCalls);
        Assert.Equal(1, dismissals.DismissedSessionBatchCalls);
    }

    [Fact]
    public async Task RoomFeed_CursorPage_Never_Reports_More_Without_A_Cursor()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var first = CreateSession(ownerId, SessionScope.Room, roomId);
        var second = CreateSession(ownerId, SessionScope.Room, roomId);
        first.ActivateHost("test-host");
        first.ReleaseHostSlot("test-host");
        second.ActivateHost("test-host");
        second.ReleaseHostSlot("test-host");
        var sessions = new FakeSessionRepository(first);
        await sessions.AddAsync(second, TestContext.Current.CancellationToken);
        var service = CreateRoomFeedService(
            sessions,
            new FakeRoomMemberRepository((roomId, actorId)));

        var result = await service.GetRoomFeedAsync(
            roomId,
            actorId,
            limit: 1,
            ct: TestContext.Current.CancellationToken);

        Assert.Single(result.Items);
        Assert.True(result.HasMore);
        Assert.NotNull(result.NextCursor);
        Assert.False(result.Truncated);
    }

    [Fact]
    public async Task RoomFeed_ParticipantSort_Wire_Envelope_Reports_Truncation_Without_A_Cursor()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var first = CreateSession(ownerId, SessionScope.Room, roomId);
        var second = CreateSession(ownerId, SessionScope.Room, roomId);
        first.ActivateHost("test-host");
        first.ReleaseHostSlot("test-host");
        second.ActivateHost("test-host");
        second.ReleaseHostSlot("test-host");
        var sessions = new FakeSessionRepository(first);
        await sessions.AddAsync(second, TestContext.Current.CancellationToken);
        var service = CreateRoomFeedService(
            sessions,
            new FakeRoomMemberRepository((roomId, actorId)));
        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web);
        HostJsonSerializerOptions.Configure(options);

        var response = await service.GetRoomFeedAsync(
            roomId,
            actorId,
            limit: 1,
            sortBy: "participants",
            ct: TestContext.Current.CancellationToken);

        using var document = JsonDocument.Parse(JsonSerializer.Serialize(response, options));
        var root = document.RootElement;
        Assert.Equal(1, root.GetProperty("items").GetArrayLength());
        Assert.Equal(JsonValueKind.Null, root.GetProperty("nextCursor").ValueKind);
        Assert.False(root.GetProperty("hasMore").GetBoolean());
        Assert.True(root.GetProperty("truncated").GetBoolean());
    }

    private static RoomSessionFeedService CreateRoomFeedService(
        FakeSessionRepository sessions,
        IRoomMemberRepository roomMembers,
        IAccessOverrideRepository? overrides = null,
        ISessionViewerDismissalRepository? dismissals = null,
        ILiveSessionStateDirectory? runtimes = null)
    {
        return new RoomSessionFeedService(
            sessions,
            roomMembers,
            overrides ?? new FakeAccessOverrideRepository(),
            dismissals ?? new FakeSessionViewerDismissalRepository(),
            runtimes ?? new FakeRuntimeDirectory());
    }

    private sealed class ParticipantCountRuntimeDirectory : ILiveSessionStateDirectory
    {
        private readonly Dictionary<SessionId, MutableStreamDemand> _demands;
        private readonly Dictionary<SessionId, LiveSessionPorts> _ports;

        public ParticipantCountRuntimeDirectory(
            params (SessionId SessionId, int ParticipantCount)[] counts)
        {
            var demands = counts.ToDictionary(
                entry => entry.SessionId,
                entry => new MutableStreamDemand(entry.ParticipantCount));
            _demands = demands;
            _ports = demands.ToDictionary(
                entry => entry.Key,
                entry => new LiveSessionPorts(
                    entry.Key,
                    null!,
                    null!,
                    null!,
                    entry.Value,
                    Guid.CreateVersion7()));
        }

        public void SetParticipantCount(SessionId sessionId, int participantCount) =>
            _demands[sessionId].ParticipantCount = participantCount;

        public LiveSessionPorts GetOrCreate(SessionId sessionId) => _ports[sessionId];

        public LiveSessionPorts? TryGet(SessionId sessionId) =>
            _ports.GetValueOrDefault(sessionId);

        public bool TryClaimHost(
            SessionId sessionId,
            string connectionId,
            CancellationTokenSource hostLifetime,
            out LiveSessionPorts? ports,
            out LiveSessionCreationOwnership? creationOwnership)
        {
            ports = null;
            creationOwnership = null;
            throw new NotSupportedException();
        }

        public void Remove(SessionId sessionId) { }

        public bool RemoveIfSame(SessionId sessionId, LiveSessionPorts expected)
        {
            if (!_ports.TryGetValue(sessionId, out var current)
                || !ReferenceEquals(current, expected))
            {
                return false;
            }

            _demands.Remove(sessionId);
            return _ports.Remove(sessionId);
        }

        public bool RemoveIfOwned(
            SessionId sessionId,
            LiveSessionCreationOwnership creationOwnership) => false;

        public IReadOnlyList<SessionId> GetActiveSessions() => [.. _ports.Keys];

    }

    private sealed class MutableStreamDemand(int participantCount) : ILiveStreamDemand
    {
        public int ParticipantCount { get; set; } = participantCount;

        public StreamDemandSnapshot GetStreamDemand() =>
            new(ParticipantCount, 0);
    }

    private sealed class TrackingRoomMemberRepository(UserId memberUserId, params RoomId[] memberRoomIds) : IRoomMemberRepository
    {
        private readonly UserId _memberUserId = memberUserId;
        private readonly HashSet<RoomId> _memberRoomIds = [.. memberRoomIds];

        public int IsMemberCalls { get; private set; }
        public int GetActiveRoomIdsCalls { get; private set; }
        public IReadOnlyList<RoomId> LastRequestedRoomIds { get; private set; } = [];

        public Task<bool> IsMemberAsync(RoomId roomId, UserId userId, CancellationToken ct = default)
        {
            IsMemberCalls++;
            return Task.FromResult(userId == _memberUserId && _memberRoomIds.Contains(roomId));
        }

        public Task<IReadOnlyList<RoomId>> GetActiveRoomIdsForUserAsync(
            UserId userId,
            IReadOnlyList<RoomId> roomIds,
            CancellationToken ct = default)
        {
            GetActiveRoomIdsCalls++;
            LastRequestedRoomIds = [.. roomIds];
            return Task.FromResult<IReadOnlyList<RoomId>>(
                userId == _memberUserId
                    ? roomIds.Where(_memberRoomIds.Contains).Distinct().ToList()
                    : []);
        }

        public Task<RoomMember?> GetAsync(RoomId roomId, UserId userId, CancellationToken ct = default)
            => Task.FromResult<RoomMember?>(null);

        public Task AddAsync(RoomMember member, CancellationToken ct = default)
            => Task.CompletedTask;

        public Task<IReadOnlyList<UserId>> GetMemberUserIdsAsync(RoomId roomId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<UserId>>([]);

        public Task<IReadOnlyList<RoomMember>> GetActiveByRoomAsync(
            RoomId roomId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<RoomMember>> GetAdmissionProofPageAfterAsync(
            RoomId roomId,
            Guid? afterUserId,
            int limit,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<UserId>> GetRoomPeerUserIdsAsync(
            UserId userId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
    }
}
