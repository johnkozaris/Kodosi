using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Infrastructure.Persistence;
using Kodosi.Infrastructure.Persistence.Repositories;
using Microsoft.EntityFrameworkCore;
using Testcontainers.PostgreSql;

namespace Kodosi.HostTests;

public sealed class PermissionDecisionAuditStoreTests
{
    [Fact]
    public async Task Completion_Uses_Canonical_Tuple_And_Classifies_Replays_Conflicts_And_Wrong_Kind()
    {
        await using var database = await TestDatabase.CreateAsync();
        var store = database.Store;
        var tuple = database.Tuple;

        var admission = await store.AdmitAsync(
            database.SessionId,
            database.RequesterUserId,
            "permission-action",
            "allow:tool-use",
            tuple,
            TestContext.Current.CancellationToken);
        Assert.Equal(PermissionDecisionAdmissionOutcome.Applied, admission.Outcome);

        var alteredRequest = await store.CompleteHostActionAsync(
            database.SessionId,
            database.RequesterUserId,
            "permission-action",
            tuple with { RequestId = "altered-tool-use" },
            InputAuditStatus.Dispatched,
            TestContext.Current.CancellationToken);
        Assert.Equal(HostActionCompletionOutcome.Conflict, alteredRequest.Outcome);
        Assert.Equal(tuple, alteredRequest.CanonicalTuple);

        var alteredDevice = await store.CompleteHostActionAsync(
            database.SessionId,
            database.RequesterUserId,
            "permission-action",
            tuple with { RequesterDeviceId = "altered-device" },
            InputAuditStatus.Dispatched,
            TestContext.Current.CancellationToken);
        Assert.Equal(HostActionCompletionOutcome.Conflict, alteredDevice.Outcome);
        Assert.Equal(tuple, alteredDevice.CanonicalTuple);

        var oldIncarnation = await store.CompleteHostActionAsync(
            database.SessionId,
            database.RequesterUserId,
            "permission-action",
            tuple with { SessionIncarnationId = Guid.CreateVersion7() },
            InputAuditStatus.Dispatched,
            TestContext.Current.CancellationToken);
        Assert.Equal(HostActionCompletionOutcome.Conflict, oldIncarnation.Outcome);

        var oldGeneration = await store.CompleteHostActionAsync(
            database.SessionId,
            database.RequesterUserId,
            "permission-action",
            tuple with { SessionIncarnationGeneration = tuple.SessionIncarnationGeneration + 1 },
            InputAuditStatus.Dispatched,
            TestContext.Current.CancellationToken);
        Assert.Equal(HostActionCompletionOutcome.Conflict, oldGeneration.Outcome);

        var applied = await store.CompleteHostActionAsync(
            database.SessionId,
            database.RequesterUserId,
            "permission-action",
            tuple,
            InputAuditStatus.Dispatched,
            TestContext.Current.CancellationToken);
        Assert.Equal(HostActionCompletionOutcome.Applied, applied.Outcome);
        Assert.Equal(tuple, applied.CanonicalTuple);

        var duplicate = await store.CompleteHostActionAsync(
            database.SessionId,
            database.RequesterUserId,
            "permission-action",
            tuple,
            InputAuditStatus.Dispatched,
            TestContext.Current.CancellationToken);
        Assert.Equal(HostActionCompletionOutcome.Duplicate, duplicate.Outcome);
        Assert.Equal(tuple, duplicate.CanonicalTuple);

        var opposite = await store.CompleteHostActionAsync(
            database.SessionId,
            database.RequesterUserId,
            "permission-action",
            tuple,
            InputAuditStatus.Rejected,
            TestContext.Current.CancellationToken);
        Assert.Equal(HostActionCompletionOutcome.Conflict, opposite.Outcome);
        Assert.Equal(tuple, opposite.CanonicalTuple);

        await database.AddWrongKindAsync("suggestion-action");
        var wrongKind = await store.CompleteHostActionAsync(
            database.SessionId,
            database.RequesterUserId,
            "suggestion-action",
            tuple,
            InputAuditStatus.Dispatched,
            TestContext.Current.CancellationToken);
        Assert.Equal(HostActionCompletionOutcome.Conflict, wrongKind.Outcome);

        var missing = await store.CompleteHostActionAsync(
            database.SessionId,
            database.RequesterUserId,
            "missing-action",
            tuple,
            InputAuditStatus.Dispatched,
            TestContext.Current.CancellationToken);
        Assert.Equal(HostActionCompletionOutcome.NotFound, missing.Outcome);
    }

    [Fact]
    public async Task Admission_RoundTrips_Exact_Maximum_Requester_Device_Id()
    {
        await using var database = await TestDatabase.CreateAsync();
        var tuple = database.Tuple with { RequesterDeviceId = new string('d', 256) };

        var applied = await database.Store.AdmitAsync(
            database.SessionId,
            database.RequesterUserId,
            "maximum-device-id",
            "allow:tool-use",
            tuple,
            TestContext.Current.CancellationToken);
        Assert.Equal(PermissionDecisionAdmissionOutcome.Applied, applied.Outcome);
        Assert.Equal(tuple, applied.CanonicalTuple);

        var duplicate = await database.Store.AdmitAsync(
            database.SessionId,
            database.RequesterUserId,
            "maximum-device-id",
            "allow:tool-use",
            tuple,
            TestContext.Current.CancellationToken);
        Assert.Equal(PermissionDecisionAdmissionOutcome.PendingDuplicate, duplicate.Outcome);
        Assert.Equal(tuple, duplicate.CanonicalTuple);

        var alteredPayload = await database.Store.AdmitAsync(
            database.SessionId,
            database.RequesterUserId,
            "maximum-device-id",
            "deny:tool-use",
            tuple,
            TestContext.Current.CancellationToken);
        Assert.Equal(PermissionDecisionAdmissionOutcome.Conflict, alteredPayload.Outcome);
        Assert.Equal(tuple, alteredPayload.CanonicalTuple);
    }

    [Fact]
    public async Task Exact_Failed_Dispatch_Is_Rearmed_For_Retry()
    {
        await using var database = await TestDatabase.CreateAsync();
        const string actionId = "retryable-dispatch";
        const string payload = "allow:tool-use";

        var applied = await database.Store.AdmitAsync(
            database.SessionId,
            database.RequesterUserId,
            actionId,
            payload,
            database.Tuple,
            TestContext.Current.CancellationToken);
        Assert.Equal(PermissionDecisionAdmissionOutcome.Applied, applied.Outcome);
        Assert.True(await database.Store.MarkDispatchFailedAsync(
            database.SessionId,
            database.RequesterUserId,
            actionId,
            database.Tuple,
            TestContext.Current.CancellationToken));

        var altered = await database.Store.AdmitAsync(
            database.SessionId,
            database.RequesterUserId,
            actionId,
            "deny:tool-use",
            database.Tuple,
            TestContext.Current.CancellationToken);
        Assert.Equal(PermissionDecisionAdmissionOutcome.Conflict, altered.Outcome);

        var rearmed = await database.Store.AdmitAsync(
            database.SessionId,
            database.RequesterUserId,
            actionId,
            payload,
            database.Tuple,
            TestContext.Current.CancellationToken);
        Assert.Equal(PermissionDecisionAdmissionOutcome.Rearmed, rearmed.Outcome);
        Assert.Equal(database.Tuple, rearmed.CanonicalTuple);

        var pending = await database.Store.AdmitAsync(
            database.SessionId,
            database.RequesterUserId,
            actionId,
            payload,
            database.Tuple,
            TestContext.Current.CancellationToken);
        Assert.Equal(PermissionDecisionAdmissionOutcome.PendingDuplicate, pending.Outcome);
    }

    [Fact]
    public async Task Concurrent_Admissions_Persist_Before_Enqueue_And_Preserve_One_Canonical_Tuple()
    {
        await using var database = await TestDatabase.CreateAsync();
        var firstTuple = database.Tuple;
        var secondTuple = firstTuple with
        {
            RequestId = "altered-tool-use",
            RequesterDeviceId = "altered-device",
        };

        var admissions = await Task.WhenAll(
            database.Store.AdmitAsync(
                database.SessionId,
                database.RequesterUserId,
                "concurrent-admission",
                "allow:tool-use",
                firstTuple,
                TestContext.Current.CancellationToken),
            database.Store.AdmitAsync(
                database.SessionId,
                database.RequesterUserId,
                "concurrent-admission",
                "allow:altered-tool-use",
                secondTuple,
                TestContext.Current.CancellationToken));

        Assert.Single(admissions, result =>
            result.Outcome == PermissionDecisionAdmissionOutcome.Applied);
        Assert.Single(admissions, result =>
            result.Outcome == PermissionDecisionAdmissionOutcome.Conflict);
        Assert.Equal(admissions[0].CanonicalTuple, admissions[1].CanonicalTuple);
    }

    [Fact]
    public async Task Concurrent_Opposite_Completions_Are_Atomic()
    {
        await using var database = await TestDatabase.CreateAsync();
        var store = database.Store;
        await store.AdmitAsync(
            database.SessionId,
            database.RequesterUserId,
            "concurrent-action",
            "allow:tool-use",
            database.Tuple,
            TestContext.Current.CancellationToken);

        var completions = await Task.WhenAll(
            store.CompleteHostActionAsync(
                database.SessionId,
                database.RequesterUserId,
                "concurrent-action",
                database.Tuple,
                InputAuditStatus.Dispatched,
                TestContext.Current.CancellationToken),
            store.CompleteHostActionAsync(
                database.SessionId,
                database.RequesterUserId,
                "concurrent-action",
                database.Tuple,
                InputAuditStatus.Rejected,
                TestContext.Current.CancellationToken));

        Assert.Single(completions, result => result.Outcome == HostActionCompletionOutcome.Applied);
        Assert.Single(completions, result => result.Outcome == HostActionCompletionOutcome.Conflict);
        await using var verification = await database.Factory.CreateDbContextAsync(
            TestContext.Current.CancellationToken);
        var status = await verification.InputAuditEntries
            .Where(entry => entry.ClientCommandId == "concurrent-action")
            .Select(entry => entry.Status)
            .SingleAsync(TestContext.Current.CancellationToken);
        Assert.Contains(status, new[] { InputAuditStatus.Dispatched, InputAuditStatus.Rejected });
    }

    private sealed class TestDbContextFactory(
        DbContextOptions<KodosiDbContext> options) : IDbContextFactory<KodosiDbContext>
    {
        public KodosiDbContext CreateDbContext() => new(options);

        public Task<KodosiDbContext> CreateDbContextAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult(CreateDbContext());
    }

    private sealed class TestDatabase : IAsyncDisposable
    {
        private readonly PostgreSqlContainer _container;

        private TestDatabase(
            PostgreSqlContainer container,
            TestDbContextFactory factory,
            SessionId sessionId,
            UserId requesterUserId,
            PermissionDecisionPendingTuple tuple)
        {
            _container = container;
            Factory = factory;
            SessionId = sessionId;
            RequesterUserId = requesterUserId;
            Tuple = tuple;
            Store = new PermissionDecisionAuditStore(factory, TimeProvider.System);
        }

        public TestDbContextFactory Factory { get; }
        public PermissionDecisionAuditStore Store { get; }
        public SessionId SessionId { get; }
        public UserId RequesterUserId { get; }
        public PermissionDecisionPendingTuple Tuple { get; }

        public static async Task<TestDatabase> CreateAsync()
        {
            var container = new PostgreSqlBuilder("postgres:16-alpine")
                .WithDatabase("kodosi_permission_audit_test")
                .WithUsername("kodosi")
                .WithPassword("kodosi-test-password")
                .Build();
            await container.StartAsync(TestContext.Current.CancellationToken);
            var options = new DbContextOptionsBuilder<KodosiDbContext>()
                .UseNpgsql(container.GetConnectionString())
                .Options;
            var factory = new TestDbContextFactory(options);
            await using var context = new KodosiDbContext(options);
            await context.Database.MigrateAsync(TestContext.Current.CancellationToken);

            var requester = User.Create(UserId.New(),
                "permission-requester@example.test",
                $"permission-{Guid.NewGuid():N}"[..32],
                "Permission Requester");
            var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
                requester.Id,
                "Permission decision audit",
                SessionScope.JustMe,
                ToolKind.Terminal,
                AccessLevel.View,
                "owner-secret-hash");
            context.Users.Add(requester);
            context.Sessions.Add(session);
            await context.SaveChangesAsync(TestContext.Current.CancellationToken);

            return new TestDatabase(
                container,
                factory,
                session.Id,
                requester.Id,
                new PermissionDecisionPendingTuple(
                    session.IncarnationId,
                    session.IncarnationGeneration,
                    "tool-use",
                    7,
                    "requester-device"));
        }

        public async Task AddWrongKindAsync(string actionId)
        {
            await using var context = await Factory.CreateDbContextAsync(
                TestContext.Current.CancellationToken);
            context.InputAuditEntries.Add(InputAuditEntry.Create(
                SessionId,
                RequesterUserId,
                actionId,
                InputAuditKind.Suggestion,
                null,
                0));
            await context.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        public ValueTask DisposeAsync() => _container.DisposeAsync();
    }
}
