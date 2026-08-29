using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Infrastructure.Persistence;
using Kodosi.Infrastructure.Persistence.Repositories;
using Microsoft.EntityFrameworkCore;
using Testcontainers.PostgreSql;

namespace Kodosi.HostTests;

public sealed class RoomMutationPostgresTests
{
    [Fact]
    public async Task ConcurrentExactDuplicateExecutesTaskMutationOnceAndSurvivesNewContext()
    {
        await using var database = await ReceiptDatabase.StartAsync();
        var actor = UserId.New();
        var room = Room.Create(
            RoomId.From(Guid.NewGuid()),
            actor,
            "Receipt room",
            $"receipt-{Guid.NewGuid():N}"[..20],
            1,
            [1],
            [2],
            "owner-device");
        var task = RoomTask.Create(
            Guid.NewGuid(),
            room.Id,
            actor,
            "ciphertext",
            null,
            null,
            null, DateTimeOffset.UtcNow);
        await database.SeedAsync(actor, room, task);

        var requestId = Guid.CreateVersion7();
        var calls = await Task.WhenAll(
            database.AssignAsync(actor, room.Id, task.Id, requestId, expectedRevision: 0),
            database.AssignAsync(actor, room.Id, task.Id, requestId, expectedRevision: 0));

        Assert.Single(calls, call => !call.Receipt.IsDuplicate);
        Assert.Single(calls, call => call.Receipt.IsDuplicate);
        Assert.All(calls, call => Assert.Equal(1, call.Receipt.Revision));

        await using (var verify = database.CreateContext())
        {
            var storedTask = await verify.RoomTasks.SingleAsync(
                candidate => candidate.Id == task.Id,
                TestContext.Current.CancellationToken);
            Assert.Equal(1, storedTask.Revision);
            Assert.Single(await verify.RoomMutationReceipts.ToListAsync(
                TestContext.Current.CancellationToken));
        }

        var afterRestart = await database.AssignAsync(
            actor,
            room.Id,
            task.Id,
            requestId,
            expectedRevision: 0);
        Assert.True(afterRestart.Receipt.IsDuplicate);
        Assert.Equal(1, afterRestart.Receipt.Revision);
    }

    [Fact]
    public async Task DistinctRequestsWithSameExpectedRevisionSerializeAndOneConflicts()
    {
        await using var database = await ReceiptDatabase.StartAsync();
        var actor = UserId.New();
        var room = Room.Create(
            RoomId.From(Guid.NewGuid()),
            actor,
            "Revision room",
            $"revision-{Guid.NewGuid():N}"[..20],
            1,
            [1],
            [2],
            "owner-device");
        var task = RoomTask.Create(
            Guid.NewGuid(),
            room.Id,
            actor,
            "ciphertext",
            null,
            null,
            null, DateTimeOffset.UtcNow);
        await database.SeedAsync(actor, room, task);

        var attempts = new[]
        {
            database.AssignOutcomeAsync(actor, room.Id, task.Id, Guid.CreateVersion7(), 0),
            database.AssignOutcomeAsync(actor, room.Id, task.Id, Guid.CreateVersion7(), 0),
        };
        var outcomes = await Task.WhenAll(attempts);

        Assert.Single(outcomes, outcome => outcome.Result is not null);
        Assert.Single(outcomes, outcome => outcome.Error is ConcurrentModificationException);
        await using var verify = database.CreateContext();
        Assert.Single(await verify.RoomMutationReceipts.ToListAsync(
            TestContext.Current.CancellationToken));
        Assert.Equal(1, (await verify.RoomTasks.SingleAsync(
            candidate => candidate.Id == task.Id,
            TestContext.Current.CancellationToken)).Revision);
    }

    [Fact]
    public async Task SameActorOperationAndRequestWithChangedTargetConflicts()
    {
        await using var database = await ReceiptDatabase.StartAsync();
        var actor = UserId.New();
        var room = Room.Create(
            RoomId.From(Guid.NewGuid()),
            actor,
            "Conflict room",
            $"conflict-{Guid.NewGuid():N}"[..20],
            1,
            [1],
            [2],
            "owner-device");
        var task = RoomTask.Create(
            Guid.NewGuid(),
            room.Id,
            actor,
            "ciphertext",
            null,
            null,
            null, DateTimeOffset.UtcNow);
        await database.SeedAsync(actor, room, task);
        var requestId = Guid.CreateVersion7();

        _ = await database.AssignAsync(actor, room.Id, task.Id, requestId, 0);
        var exception = await Assert.ThrowsAsync<RoomMutationReceiptTargetConflictException>(() =>
            database.AssignAsync(actor, room.Id, task.Id, requestId, 1));

        Assert.Equal("ROOM_MUTATION_TARGET_CONFLICT", exception.Code);
    }

    private sealed class ReceiptDatabase(
        PostgreSqlContainer container,
        DbContextOptions<KodosiDbContext> options) : IAsyncDisposable
    {
        public static async Task<ReceiptDatabase> StartAsync()
        {
            var container = new PostgreSqlBuilder("postgres:16-alpine")
                .WithDatabase("kodosi_room_receipt_test")
                .WithUsername("kodosi")
                .WithPassword("kodosi-test-password")
                .Build();
            await container.StartAsync(TestContext.Current.CancellationToken);
            var options = new DbContextOptionsBuilder<KodosiDbContext>()
                .UseNpgsql(container.GetConnectionString())
                .Options;
            await using var setup = new KodosiDbContext(options);
            await setup.Database.MigrateAsync(TestContext.Current.CancellationToken);
            return new ReceiptDatabase(container, options);
        }

        public KodosiDbContext CreateContext() => new(options);

        public async Task SeedAsync(UserId actor, Room room, RoomTask task)
        {
            await using var context = CreateContext();
            context.Users.Add(User.Create(
                actor,
                $"{actor.Value:N}@example.test",
                $"receipt-{actor.Value:N}"[..32],
                "Receipt actor"));
            context.Rooms.Add(room);
            context.RoomMembers.Add(DomainFixtureHydrator.RoomMember(room.Id, actor));
            context.RoomTasks.Add(task);
            await context.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        public async Task<(RoomTaskMutationResult? Result, Exception? Error)> AssignOutcomeAsync(
            UserId actor,
            RoomId roomId,
            Guid taskId,
            Guid requestId,
            long expectedRevision)
        {
            try
            {
                return (await AssignAsync(
                    actor,
                    roomId,
                    taskId,
                    requestId,
                    expectedRevision), null);
            }
            catch (Exception error)
            {
                return (null, error);
            }
        }

        public async Task<RoomTaskMutationResult> AssignAsync(
            UserId actor,
            RoomId roomId,
            Guid taskId,
            Guid requestId,
            long expectedRevision)
        {
            await using var context = CreateContext();
            var unitOfWork = new UnitOfWork(context);
            var service = new RoomTaskService(
                new RoomRepository(context),
                new RoomMemberRepository(context),
                new RoomTaskRepository(context),
                new SessionRepository(context),
                new RoomMutationReceiptRepository(context),
                new PostgresRoomLifecycleLock(context),
                unitOfWork,
                TimeProvider.System);
            return await service.AssignIdempotentlyAsync(
                requestId,
                roomId,
                taskId,
                expectedRevision,
                actor,
                sessionId: null,
                sessionIncarnationId: null,
                TestContext.Current.CancellationToken);
        }

        public ValueTask DisposeAsync() => container.DisposeAsync();
    }
}
