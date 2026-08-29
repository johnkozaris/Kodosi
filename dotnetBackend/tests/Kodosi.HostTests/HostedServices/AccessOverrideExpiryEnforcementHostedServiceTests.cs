using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Realtime;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging.Abstractions;

namespace Kodosi.HostTests;

public sealed class AccessOverrideExpiryEnforcementHostedServiceTests
{
    [Fact]
    public async Task Obsolete_Work_Completes_Without_Realtime_Enforcement()
    {
        var work = CreateWork();
        var durability = new FakeDurability(work) { IsCurrent = false };
        var authority = new RecordingSessionEndAuthority();
        await using var provider = new ServiceCollection().BuildServiceProvider();
        var worker = new AccessOverrideExpiryEnforcementHostedService(
            durability,
            provider.GetRequiredService<IServiceScopeFactory>(),
            authority,
            NullLogger<AccessOverrideExpiryEnforcementHostedService>.Instance,
            TimeProvider.System);

        await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);

        Assert.Equal([work.AuditEntryId], durability.Completed);
        Assert.True(authority.Acquired);
        Assert.True(authority.Released);
        Assert.False(durability.EnforcementInvoked);
    }

    [Fact]
    public async Task Timed_Out_Work_Remains_Pending_For_Retry()
    {
        var work = CreateWork();
        var durability = new FakeDurability(work) { BlockUntilCancelled = true };
        var authority = new RecordingSessionEndAuthority();
        await using var provider = new ServiceCollection().BuildServiceProvider();
        var worker = new AccessOverrideExpiryEnforcementHostedService(
            durability,
            provider.GetRequiredService<IServiceScopeFactory>(),
            authority,
            NullLogger<AccessOverrideExpiryEnforcementHostedService>.Instance,
            TimeProvider.System)
        {
            EnforcementAttemptTimeout = TimeSpan.FromMilliseconds(20),
        };

        await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);

        Assert.True(durability.AttemptWasCancelled);
        Assert.Empty(durability.Completed);
        Assert.True(authority.Released);
    }

    [Fact]
    public async Task Failed_Item_Remains_Pending_While_Later_Work_Still_Completes()
    {
        var failed = CreateWork();
        var later = CreateWork();
        var durability = new FakeDurability(failed, later)
        {
            IsCurrent = false,
            FailAuditEntryId = failed.AuditEntryId,
        };
        await using var provider = new ServiceCollection().BuildServiceProvider();
        var worker = new AccessOverrideExpiryEnforcementHostedService(
            durability,
            provider.GetRequiredService<IServiceScopeFactory>(),
            new RecordingSessionEndAuthority(),
            NullLogger<AccessOverrideExpiryEnforcementHostedService>.Instance,
            TimeProvider.System);

        await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);

        Assert.Equal([later.AuditEntryId], durability.Completed);
    }

    [Fact]
    public async Task Failed_Full_Page_Does_Not_Starve_Later_Work()
    {
        var failed = Enumerable.Range(0, 100)
            .Select(_ => CreateWork())
            .ToArray();
        var later = CreateWork();
        var ordered = failed
            .Append(later)
            .OrderByDescending(work => work.OccurredAt)
            .ThenByDescending(work => work.AuditEntryId)
            .ToArray();
        later = ordered[^1];
        var durability = new FakeDurability(ordered)
        {
            IsCurrent = false,
            FailedAuditEntryIds = ordered[..100]
                .Select(work => work.AuditEntryId)
                .ToHashSet(),
        };
        await using var provider = new ServiceCollection().BuildServiceProvider();
        var worker = new AccessOverrideExpiryEnforcementHostedService(
            durability,
            provider.GetRequiredService<IServiceScopeFactory>(),
            new RecordingSessionEndAuthority(),
            NullLogger<AccessOverrideExpiryEnforcementHostedService>.Instance,
            TimeProvider.System);

        await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);
        Assert.Empty(durability.Completed);

        await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);

        Assert.Equal([later.AuditEntryId], durability.Completed);
    }

    [Fact]
    public async Task Continuous_Appends_Do_Not_Starve_Old_Failed_Work()
    {
        var occurredAt = new DateTimeOffset(2026, 8, 20, 12, 0, 0, TimeSpan.Zero);
        var failed = Enumerable.Range(0, 201)
            .Select(index => CreateWork(occurredAt.AddSeconds(index)))
            .ToArray();
        var durability = new FakeDurability(failed)
        {
            IsCurrent = false,
            FailedAuditEntryIds = failed
                .Select(work => work.AuditEntryId)
                .ToHashSet(),
        };
        await using var provider = new ServiceCollection().BuildServiceProvider();
        var worker = new AccessOverrideExpiryEnforcementHostedService(
            durability,
            provider.GetRequiredService<IServiceScopeFactory>(),
            new RecordingSessionEndAuthority(),
            NullLogger<AccessOverrideExpiryEnforcementHostedService>.Instance,
            TimeProvider.System);

        for (var sweep = 0; sweep < 3; sweep++)
        {
            await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);
            durability.Work.AddRange(Enumerable.Range(0, 100)
                .Select(index => CreateWork(occurredAt.AddMinutes(10 + sweep).AddMilliseconds(index))));
        }

        Assert.Contains(
            failed[0].AuditEntryId,
            durability.AttemptedAuditEntryIds);
        Assert.All(
            failed,
            work => Assert.Equal(
                1,
                durability.AttemptedAuditEntryIds.Count(id => id == work.AuditEntryId)));
    }

    [Fact]
    public async Task Load_Failure_Does_Not_End_The_Sweep()
    {
        var durability = new FakeDurability(CreateWork()) { FailLoad = true };
        await using var provider = new ServiceCollection().BuildServiceProvider();
        var worker = new AccessOverrideExpiryEnforcementHostedService(
            durability,
            provider.GetRequiredService<IServiceScopeFactory>(),
            new RecordingSessionEndAuthority(),
            NullLogger<AccessOverrideExpiryEnforcementHostedService>.Instance,
            TimeProvider.System);

        await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);

        Assert.Empty(durability.Completed);
    }

    private static AccessOverrideExpiryEnforcementWork CreateWork(
        DateTimeOffset? occurredAt = null)
    {
        var startedAt = new DateTimeOffset(2026, 8, 20, 10, 0, 0, TimeSpan.Zero);
        var expiresAt = startedAt.AddMinutes(5);
        return new AccessOverrideExpiryEnforcementWork(
            Guid.CreateVersion7(),
            occurredAt ?? expiresAt.AddSeconds(1),
            SessionId.New(),
            UserId.New(),
            Guid.CreateVersion7(),
            startedAt,
            expiresAt,
            expiresAt.AddSeconds(1));
    }

    private sealed class FakeDurability(params AccessOverrideExpiryEnforcementWork[] work)
        : IAccessOverrideExpiryDurabilityCoordinator
    {
        public List<AccessOverrideExpiryEnforcementWork> Work { get; } = [.. work];
        public List<Guid> Completed { get; } = [];
        public List<Guid> AttemptedAuditEntryIds { get; } = [];
        public bool IsCurrent { get; init; } = true;
        public bool BlockUntilCancelled { get; init; }
        public bool FailLoad { get; init; }
        public Guid? FailAuditEntryId { get; init; }
        public HashSet<Guid> FailedAuditEntryIds { get; init; } = [];
        public bool AttemptWasCancelled { get; private set; }
        public bool EnforcementInvoked { get; private set; }

        public Task<IReadOnlyList<AccessOverrideExpiryEnforcementWork>> GetPendingEnforcementAsync(
            int limit,
            CancellationToken ct = default,
            AccessOverrideExpiryEnforcementCursor? before = null)
        {
            if (FailLoad)
            {
                throw new InvalidOperationException("load failed");
            }
            return Task.FromResult<IReadOnlyList<AccessOverrideExpiryEnforcementWork>>(
                Work
                    .Where(candidate => before is not { } cursor
                        || candidate.OccurredAt < cursor.OccurredAt
                        || candidate.OccurredAt == cursor.OccurredAt
                            && candidate.AuditEntryId.CompareTo(cursor.AuditEntryId) < 0)
                    .OrderByDescending(candidate => candidate.OccurredAt)
                    .ThenByDescending(candidate => candidate.AuditEntryId)
                    .Take(limit)
                    .ToList());
        }

        public async Task<bool> ExecuteIfCurrentAsync(
            AccessOverrideExpiryEnforcementWork candidate,
            Func<CancellationToken, Task> enforce,
            CancellationToken ct = default)
        {
            AttemptedAuditEntryIds.Add(candidate.AuditEntryId);
            if (FailAuditEntryId == candidate.AuditEntryId
                || FailedAuditEntryIds.Contains(candidate.AuditEntryId))
            {
                throw new InvalidOperationException("enforcement failed");
            }
            if (BlockUntilCancelled)
            {
                try
                {
                    await Task.Delay(Timeout.InfiniteTimeSpan, ct);
                }
                catch (OperationCanceledException) when (ct.IsCancellationRequested)
                {
                    AttemptWasCancelled = true;
                    throw;
                }
            }
            if (!IsCurrent)
            {
                return false;
            }
            EnforcementInvoked = true;
            await enforce(ct);
            return true;
        }

        public Task CompleteEnforcementAsync(
            Guid auditEntryId,
            CancellationToken ct = default)
        {
            Completed.Add(auditEntryId);
            return Task.CompletedTask;
        }
    }

    private sealed class RecordingSessionEndAuthority : ISessionEndAuthority
    {
        public bool Acquired { get; private set; }
        public bool Released { get; private set; }

        public ValueTask<IAsyncDisposable> AcquireAsync(
            IReadOnlyCollection<SessionId> sessionIds,
            CancellationToken ct = default)
        {
            Assert.Single(sessionIds);
            Acquired = true;
            return ValueTask.FromResult<IAsyncDisposable>(new Lease(this));
        }

        public Task ProjectCommittedAsync(
            IReadOnlyCollection<CommittedSessionEnd> sessionEnds,
            CancellationToken ct = default) => Task.CompletedTask;

        public Task RetireEndedIncarnationAsync(
            SessionDiscoveryTarget endedSession,
            DateTimeOffset startedAt,
            CancellationToken ct = default) => Task.CompletedTask;

        private sealed class Lease(RecordingSessionEndAuthority owner) : IAsyncDisposable
        {
            public ValueTask DisposeAsync()
            {
                owner.Released = true;
                return ValueTask.CompletedTask;
            }
        }
    }
}
