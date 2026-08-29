using System.Collections.Concurrent;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging.Abstractions;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Infrastructure.Realtime;

namespace Kodosi.HostTests;

public sealed class AuditWriterTests
{
    [Fact]
    public async Task AppendAsync_Is_Idempotent_For_Replayed_ClientCommand()
    {
        var store = new FakeAuditStore();
        using var services = new ServiceCollection()
            .AddScoped<IInputAuditRepository>(_ => new FakeInputAuditRepository(store))
            .AddScoped<IUnitOfWork>(_ => new FakeUnitOfWork(store))
            .BuildServiceProvider();
        var writer = new AuditWriter(
            services.GetRequiredService<IServiceScopeFactory>(),
            NullLogger<AuditWriter>.Instance);
        var sessionId = SessionId.New();
        var userId = UserId.New();

        await writer.StartAsync(CancellationToken.None);

        try
        {
            var first = await writer.AppendAsync(
                sessionId,
                userId,
                "action-replay",
                InputAuditKind.Suggestion,
                "pwd",
                InputAuditStatus.Rejected,
                CancellationToken.None);
            var second = await writer.AppendAsync(
                sessionId,
                userId,
                "action-replay",
                InputAuditKind.Suggestion,
                "pwd",
                InputAuditStatus.Rejected,
                CancellationToken.None);

            Assert.Equal(AuditAppendOutcome.Appended, first);
            Assert.Equal(AuditAppendOutcome.AlreadyExists, second);
            Assert.Single(store.Entries);
            Assert.Equal(InputAuditStatus.Rejected, store.Entries.Values.Single().Status);
        }
        finally
        {
            await writer.StopAsync(CancellationToken.None);
        }
    }

    [Fact]
    public async Task RecordDuplicateAsync_Increments_Existing_Forensic_Row()
    {
        var store = new FakeAuditStore();
        using var services = new ServiceCollection()
            .AddScoped<IInputAuditRepository>(_ => new FakeInputAuditRepository(store))
            .AddScoped<IUnitOfWork>(_ => new FakeUnitOfWork(store))
            .BuildServiceProvider();
        var writer = new AuditWriter(
            services.GetRequiredService<IServiceScopeFactory>(),
            NullLogger<AuditWriter>.Instance);
        var sessionId = SessionId.New();
        var userId = UserId.New();
        await writer.StartAsync(CancellationToken.None);

        try
        {
            Assert.Equal(
                AuditAppendOutcome.Appended,
                await writer.AppendAsync(
                    sessionId,
                    userId,
                    "action-duplicate",
                    InputAuditKind.Inject,
                    "payload",
                    InputAuditStatus.Dispatched,
                    CancellationToken.None));

            Assert.True(await writer.RecordDuplicateAsync(
                sessionId,
                userId,
                "action-duplicate",
                InputAuditKind.Inject,
                "payload",
                CancellationToken.None));

            var entry = Assert.Single(store.Entries.Values);
            Assert.Equal(InputAuditStatus.Dispatched, entry.Status);
            Assert.Equal(1, entry.DuplicateCount);
            Assert.NotNull(entry.LastDuplicateAt);
        }
        finally
        {
            await writer.StopAsync(CancellationToken.None);
        }
    }

    [Fact]
    public async Task AppendAsync_Batches_Queued_Appends_Into_One_Save()
    {
        var store = new FakeAuditStore();
        using var services = new ServiceCollection()
            .AddScoped<IInputAuditRepository>(_ => new FakeInputAuditRepository(store))
            .AddScoped<IUnitOfWork>(_ => new FakeUnitOfWork(store))
            .BuildServiceProvider();
        var writer = new AuditWriter(
            services.GetRequiredService<IServiceScopeFactory>(),
            NullLogger<AuditWriter>.Instance);
        var sessionId = SessionId.New();
        var userId = UserId.New();

        var appendTasks = new[]
        {
            writer.AppendAsync(sessionId, userId, "action-1", InputAuditKind.Suggestion, "pwd", InputAuditStatus.Pending, CancellationToken.None),
            writer.AppendAsync(sessionId, userId, "action-2", InputAuditKind.Suggestion, "ls", InputAuditStatus.Pending, CancellationToken.None),
            writer.AppendAsync(sessionId, userId, "action-3", InputAuditKind.Inject, "echo hi", InputAuditStatus.Pending, CancellationToken.None),
        };

        await writer.StartAsync(CancellationToken.None);

        try
        {
            var results = await Task.WhenAll(appendTasks);

            Assert.All(results, result => Assert.Equal(AuditAppendOutcome.Appended, result));
            Assert.Equal(3, store.Entries.Count);
            Assert.Equal(1, store.BatchLookupCalls);
            Assert.Equal(1, store.BatchAddCalls);
            Assert.Equal(1, store.SaveChangesCalls);
            Assert.Equal(0, store.SingleAddCalls);
        }
        finally
        {
            await writer.StopAsync(CancellationToken.None);
        }
    }

    [Fact]
    public async Task AppendAsync_Falls_Back_To_Individual_Persistence_When_Batch_Save_Fails()
    {
        var store = new FakeAuditStore();
        store.BatchFailureActionIds.Add("action-bad");
        store.SingleFailureActionIds.Add("action-bad");

        using var services = new ServiceCollection()
            .AddScoped<IInputAuditRepository>(_ => new FakeInputAuditRepository(store))
            .AddScoped<IUnitOfWork>(_ => new FakeUnitOfWork(store))
            .BuildServiceProvider();
        var writer = new AuditWriter(
            services.GetRequiredService<IServiceScopeFactory>(),
            NullLogger<AuditWriter>.Instance);
        var sessionId = SessionId.New();
        var userId = UserId.New();

        var appendTasks = new[]
        {
            writer.AppendAsync(sessionId, userId, "action-good", InputAuditKind.Suggestion, "pwd", InputAuditStatus.Pending, CancellationToken.None),
            writer.AppendAsync(sessionId, userId, "action-bad", InputAuditKind.Suggestion, "ls", InputAuditStatus.Pending, CancellationToken.None),
        };

        await writer.StartAsync(CancellationToken.None);

        try
        {
            var results = await Task.WhenAll(appendTasks);

            Assert.Equal(AuditAppendOutcome.Appended, results[0]);
            Assert.Equal(AuditAppendOutcome.Failed, results[1]);
            Assert.Single(store.Entries);
            Assert.Contains((sessionId, userId, "action-good"), store.Entries.Keys);
            Assert.Equal(1, store.BatchAddCalls);
            Assert.Equal(2, store.SingleAddCalls);
        }
        finally
        {
            await writer.StopAsync(CancellationToken.None);
        }
    }

    [Fact]
    public async Task StopAsync_Completes_Writer_And_Drains_Every_Accepted_Batch()
    {
        var store = new FakeAuditStore();
        using var services = new ServiceCollection()
            .AddScoped<IInputAuditRepository>(_ => new FakeInputAuditRepository(store))
            .AddScoped<IUnitOfWork>(_ => new FakeUnitOfWork(store))
            .BuildServiceProvider();
        var writer = new AuditWriter(
            services.GetRequiredService<IServiceScopeFactory>(),
            NullLogger<AuditWriter>.Instance);
        var sessionId = SessionId.New();
        var userId = UserId.New();
        var appends = Enumerable.Range(0, 125)
            .Select(index => writer.AppendAsync(
                sessionId,
                userId,
                $"shutdown-{index}",
                InputAuditKind.Suggestion,
                "payload",
                InputAuditStatus.Pending,
                CancellationToken.None))
            .ToArray();

        await writer.StartAsync(TestContext.Current.CancellationToken);
        await writer.StopAsync(TestContext.Current.CancellationToken);
        var outcomes = await Task.WhenAll(appends);

        Assert.All(
            outcomes,
            outcome => Assert.Equal(AuditAppendOutcome.Appended, outcome));
        Assert.Equal(125, store.Entries.Count);
        Assert.True(store.SaveChangesCalls >= 3);
    }

    [Fact]
    public async Task StopAsync_Resolves_Accepted_Completions_When_Drain_Deadline_Expires()
    {
        var store = new FakeAuditStore { BlockBatchLookup = true };
        using var services = new ServiceCollection()
            .AddScoped<IInputAuditRepository>(_ => new FakeInputAuditRepository(store))
            .AddScoped<IUnitOfWork>(_ => new FakeUnitOfWork(store))
            .BuildServiceProvider();
        var writer = new AuditWriter(
            services.GetRequiredService<IServiceScopeFactory>(),
            NullLogger<AuditWriter>.Instance,
            shutdownDrainDeadline: TimeSpan.FromMilliseconds(20));
        await writer.StartAsync(TestContext.Current.CancellationToken);
        var append = writer.AppendAsync(
            SessionId.New(),
            UserId.New(),
            "deadline",
            InputAuditKind.Inject,
            "payload",
            InputAuditStatus.Pending,
            TestContext.Current.CancellationToken);

        await Task.Delay(10, TestContext.Current.CancellationToken);
        await writer.StopAsync(TestContext.Current.CancellationToken);

        Assert.Equal(AuditAppendOutcome.Failed, await append);
        Assert.NotNull(writer.ExecuteTask);
        Assert.True(writer.ExecuteTask.IsCompleted);
    }

    private sealed class FakeAuditStore
    {
        public ConcurrentDictionary<(SessionId SessionId, UserId UserId, string ActionId), InputAuditEntry> Entries { get; } =
            new();
        public int SingleAddCalls;
        public int BatchAddCalls;
        public int BatchLookupCalls;
        public int SaveChangesCalls;
        public bool BlockBatchLookup;
        public HashSet<string> BatchFailureActionIds { get; } = new(StringComparer.Ordinal);
        public HashSet<string> SingleFailureActionIds { get; } = new(StringComparer.Ordinal);
    }

    private sealed class FakeInputAuditRepository(FakeAuditStore store) : IInputAuditRepository
    {
        public Task AddAsync(InputAuditEntry entry, CancellationToken ct = default)
        {
            Interlocked.Increment(ref store.SingleAddCalls);
            if (store.SingleFailureActionIds.Contains(entry.ClientCommandId))
            {
                throw new InvalidOperationException("single insert failed");
            }

            if (!store.Entries.TryAdd((entry.SessionId, entry.SenderUserId, entry.ClientCommandId), entry))
            {
                throw new InvalidOperationException("duplicate audit append");
            }

            return Task.CompletedTask;
        }

        public Task AddRangeAsync(IEnumerable<InputAuditEntry> entries, CancellationToken ct = default)
        {
            Interlocked.Increment(ref store.BatchAddCalls);
            var bufferedEntries = entries.ToArray();
            if (bufferedEntries.Any(entry => store.BatchFailureActionIds.Contains(entry.ClientCommandId)))
            {
                throw new InvalidOperationException("batch insert failed");
            }

            foreach (var entry in bufferedEntries)
            {
                if (!store.Entries.TryAdd((entry.SessionId, entry.SenderUserId, entry.ClientCommandId), entry))
                {
                    throw new InvalidOperationException("duplicate audit append");
                }
            }

            return Task.CompletedTask;
        }

        public Task<InputAuditEntry?> GetByClientCommandAsync(
            SessionId sessionId,
            UserId senderUserId,
            string clientCommandId,
            CancellationToken ct = default)
        {
            store.Entries.TryGetValue((sessionId, senderUserId, clientCommandId), out var entry);
            return Task.FromResult(entry);
        }

        public async Task<IReadOnlySet<InputAuditLookupKey>> GetExistingClientCommandsAsync(
            IReadOnlyCollection<InputAuditLookupKey> keys,
            CancellationToken ct = default)
        {
            Interlocked.Increment(ref store.BatchLookupCalls);
            if (store.BlockBatchLookup)
            {
                await Task.Delay(Timeout.InfiniteTimeSpan, ct);
            }
            var existing = keys
                .Where(key => store.Entries.ContainsKey((key.SessionId, key.SenderUserId, key.ClientCommandId)))
                .ToHashSet();
            return existing;
        }

        public Task<bool> TryRecordDuplicateAsync(
            SessionId sessionId,
            UserId senderUserId,
            string clientCommandId,
            DateTimeOffset occurredAt,
            CancellationToken ct = default)
        {
            if (!store.Entries.TryGetValue(
                    (sessionId, senderUserId, clientCommandId),
                    out var entry))
            {
                return Task.FromResult(false);
            }

            typeof(InputAuditEntry)
                .GetProperty(nameof(InputAuditEntry.DuplicateCount))!
                .SetValue(entry, entry.DuplicateCount + 1);
            typeof(InputAuditEntry)
                .GetProperty(nameof(InputAuditEntry.LastDuplicateAt))!
                .SetValue(entry, occurredAt);
            return Task.FromResult(true);
        }
    }

    private sealed class FakeUnitOfWork(FakeAuditStore store) : UnitOfWorkStub
    {
        public override Task SaveChangesAsync(CancellationToken ct = default)
        {
            Interlocked.Increment(ref store.SaveChangesCalls);
            return Task.CompletedTask;
        }
    }
}
