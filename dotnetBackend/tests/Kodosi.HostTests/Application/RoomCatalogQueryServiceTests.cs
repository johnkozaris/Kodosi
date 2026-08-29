using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests.Application;

public sealed class RoomCatalogQueryServiceTests
{
    [Fact]
    public async Task Catalog_Is_Bounded_And_Uses_Last_Item_As_Cursor()
    {
        var userId = UserId.New();
        var rooms = Enumerable.Range(0, 4)
            .Select(index => Room.Create(
                RoomId.From(Guid.NewGuid()),
                userId,
                $"Room {index}",
                $"room-{index}",
                1,
                [1],
                [2],
                "device"))
            .ToList();
        var repository = new CatalogRoomRepository(rooms);
        var service = new RoomCatalogQueryService(
            repository,
            new CatalogMemberRepository(userId),
            new CatalogTransitionRepository());

        var page = await service.ListAsync(
            userId,
            cursor: null,
            limit: 99,
            TestContext.Current.CancellationToken);

        Assert.Equal(3, page.Items.Count);
        Assert.True(page.HasMore);
        Assert.NotNull(page.NextCursor);
        Assert.Equal(4, repository.LastLimit);
        Assert.NotNull(FeedCursor.Decode(page.NextCursor));
    }

    [Fact]
    public async Task Trust_Pages_Conceal_Rooms_From_Nonmembers()
    {
        var ownerId = UserId.New();
        var room = Room.Create(
            RoomId.From(Guid.NewGuid()), ownerId, "Room", "room", 1, [1], [2], "device");
        var service = new RoomCatalogQueryService(
            new CatalogRoomRepository([room]),
            new CatalogMemberRepository(ownerId),
            new CatalogTransitionRepository());

        Assert.Null(await service.ListAdmissionProofsAsync(
            room.Id,
            UserId.New(),
            afterUserId: null,
            limit: 4,
            TestContext.Current.CancellationToken));
        Assert.Null(await service.ListTransitionsAsync(
            room.Id,
            UserId.New(),
            afterGeneration: 0,
            limit: 8,
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task Trust_Page_Continuations_Match_Last_Returned_Item()
    {
        var ownerId = UserId.New();
        var room = Room.Create(
            RoomId.From(Guid.NewGuid()), ownerId, "Room", "room", 1, [1], [2], "device");
        var memberRepository = new CatalogMemberRepository(ownerId);
        var members = Enumerable.Range(0, 5)
            .Select(_ => DomainFixtureHydrator.RoomMember(room.Id, UserId.New(), RoomRole.Member))
            .OrderBy(member => member.UserId.Value)
            .ToList();
        memberRepository.Proofs.AddRange(members);
        var transitions = new CatalogTransitionRepository();
        transitions.Items.AddRange(Enumerable.Range(1, 9)
            .Select(generation => transition(room, generation)));
        var service = new RoomCatalogQueryService(
            new CatalogRoomRepository([room]), memberRepository, transitions);

        var proofPage = await service.ListAdmissionProofsAsync(
            room.Id,
            ownerId,
            afterUserId: null,
            limit: 99,
            TestContext.Current.CancellationToken);
        var transitionPage = await service.ListTransitionsAsync(
            room.Id,
            ownerId,
            afterGeneration: 0,
            limit: 99,
            TestContext.Current.CancellationToken);

        Assert.NotNull(proofPage);
        Assert.Equal(4, proofPage.Items.Count);
        Assert.Equal(proofPage.Items[^1].UserId.Value, proofPage.NextUserId);
        Assert.NotNull(transitionPage);
        Assert.Equal(8, transitionPage.Items.Count);
        Assert.Equal(transitionPage.Items[^1].Generation, transitionPage.NextGeneration);
    }

    private static RoomRosterTransition transition(Room room, int generation)
    {
        var copy = Room.Create(
            room.Id,
            room.OwnerUserId,
            room.Name,
            room.Slug,
            1,
            [1],
            [2],
            "device");
        for (var current = 2; current <= generation; current++)
        {
            copy.ReplaceRosterForRemoval(current, [1], [2], "device");
        }
        return RoomRosterTransition.Create(copy);
    }

    private sealed class CatalogRoomRepository(IReadOnlyList<Room> rooms) : IRoomRepository
    {
        public int LastLimit { get; private set; }
        public Task<Room?> GetByIdAsync(RoomId id, CancellationToken ct = default) =>
            Task.FromResult(rooms.FirstOrDefault(room => room.Id == id));
        public Task AddAsync(Room room, CancellationToken ct = default) =>
            throw new NotSupportedException();
        public Task<IReadOnlyList<Room>> GetByMemberPageAsync(
            UserId userId,
            FeedCursor? cursor,
            int limit,
            CancellationToken ct = default)
        {
            LastLimit = limit;
            return Task.FromResult<IReadOnlyList<Room>>(rooms.Take(limit).ToList());
        }
        public Task<IReadOnlyList<Room>> GetByIdsAsync(
            IReadOnlyCollection<RoomId> ids,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<Room>>(rooms.Where(room => ids.Contains(room.Id)).ToList());
    }

    private sealed class CatalogMemberRepository(UserId memberId) : IRoomMemberRepository
    {
        public List<RoomMember> Proofs { get; } = [];
        public Task<bool> IsMemberAsync(RoomId roomId, UserId userId, CancellationToken ct = default) =>
            Task.FromResult(userId == memberId);
        public Task<IReadOnlyList<RoomMember>> GetAdmissionProofPageAfterAsync(
            RoomId roomId,
            Guid? afterUserId,
            int limit,
            CancellationToken ct = default)
        {
            var cursor = afterUserId ?? Guid.Empty;
            return Task.FromResult<IReadOnlyList<RoomMember>>(Proofs
                .Where(member => member.UserId.Value.CompareTo(cursor) > 0)
                .OrderBy(member => member.UserId.Value)
                .Take(limit)
                .ToList());
        }
        public Task<IReadOnlyList<RoomId>> GetActiveRoomIdsForUserAsync(UserId userId, IReadOnlyList<RoomId> roomIds, CancellationToken ct = default) => throw new NotSupportedException();
        public Task<RoomMember?> GetAsync(RoomId roomId, UserId userId, CancellationToken ct = default) => throw new NotSupportedException();
        public Task AddAsync(RoomMember member, CancellationToken ct = default) => throw new NotSupportedException();
        public Task<IReadOnlyList<UserId>> GetMemberUserIdsAsync(RoomId roomId, CancellationToken ct = default) => throw new NotSupportedException();
        public Task<IReadOnlyList<RoomMember>> GetActiveByRoomAsync(RoomId roomId, CancellationToken ct = default) => throw new NotSupportedException();
        public Task<IReadOnlyList<UserId>> GetRoomPeerUserIdsAsync(UserId userId, CancellationToken ct = default) => throw new NotSupportedException();
    }

    private sealed class CatalogTransitionRepository : IRoomRosterTransitionRepository
    {
        public List<RoomRosterTransition> Items { get; } = [];
        public Task AddAsync(RoomRosterTransition transition, CancellationToken ct = default) => throw new NotSupportedException();
        public Task<IReadOnlyList<RoomRosterTransition>> GetPageAfterAsync(RoomId roomId, long afterGeneration, int limit, CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<RoomRosterTransition>>(Items
                .Where(item => item.RoomId == roomId && item.Generation > afterGeneration)
                .OrderBy(item => item.Generation)
                .Take(limit)
                .ToList());
    }
}
