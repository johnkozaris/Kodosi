using Microsoft.Extensions.Logging.Abstractions;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Auth;
using Kodosi.Host.Observability;

using Kodosi.Host.Realtime;

namespace Kodosi.HostTests;

public sealed class WebSocketJwtRevalidationLoopTests
{
    [Fact]
    public async Task Consecutive_Transients_Fail_Close_At_Threshold()
    {
        var revalidator = new FakeRevalidator(
            JwtRevalidationResult.Transient,
            JwtRevalidationResult.Transient,
            JwtRevalidationResult.Transient);
        var closed = new TaskCompletionSource();
        var metrics = new OperationalMetrics(new FakeDirectory());

        using var cts = new CancellationTokenSource(TimeSpan.FromSeconds(5));

        await WebSocketJwtRevalidationLoop.RunAsync(
            revalidator,
            token: "t",
            connectionId: "c",
            handlerKind: "Test",
            markRevokedAndCancel: () =>
            {
                closed.TrySetResult();
                return ValueTask.CompletedTask;
            },
            logger: NullLogger.Instance,
            metrics: metrics,
            additionalAuthorization: static _ => Task.FromResult(true),
            interval: TimeSpan.FromMilliseconds(10),
            ct: cts.Token);

        Assert.True(closed.Task.IsCompletedSuccessfully);
        Assert.Equal(3, revalidator.Calls);
        Assert.Equal(1, metrics.Snapshot().JwtRevalidationTransientThresholdCount);
    }

    [Fact]
    public async Task Valid_Resets_Transient_Counter()
    {
        var revalidator = new FakeRevalidator(
            JwtRevalidationResult.Transient,
            JwtRevalidationResult.Valid,
            JwtRevalidationResult.Transient,
            JwtRevalidationResult.Transient,
            JwtRevalidationResult.Valid);
        var closed = false;
        var metrics = new OperationalMetrics(new FakeDirectory());

        using var cts = new CancellationTokenSource();
        var run = WebSocketJwtRevalidationLoop.RunAsync(
            revalidator,
            token: "t",
            connectionId: "c",
            handlerKind: "Test",
            markRevokedAndCancel: () =>
            {
                closed = true;
                return ValueTask.CompletedTask;
            },
            logger: NullLogger.Instance,
            metrics: metrics,
            additionalAuthorization: static _ => Task.FromResult(true),
            interval: TimeSpan.FromMilliseconds(10),
            ct: cts.Token);

        var deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
        while (revalidator.Calls < 5 && DateTime.UtcNow < deadline)
        {
            await Task.Delay(10, TestContext.Current.CancellationToken);
        }
        cts.Cancel();
        await run;

        Assert.False(closed);
        Assert.Equal(0, metrics.Snapshot().JwtRevalidationTransientThresholdCount);
    }

    [Fact]
    public async Task Invalid_Closes_Immediately()
    {
        var revalidator = new FakeRevalidator(JwtRevalidationResult.Invalid);
        var closed = new TaskCompletionSource();
        var metrics = new OperationalMetrics(new FakeDirectory());

        using var cts = new CancellationTokenSource(TimeSpan.FromSeconds(5));

        await WebSocketJwtRevalidationLoop.RunAsync(
            revalidator,
            token: "t",
            connectionId: "c",
            handlerKind: "Test",
            markRevokedAndCancel: () =>
            {
                closed.TrySetResult();
                return ValueTask.CompletedTask;
            },
            logger: NullLogger.Instance,
            metrics: metrics,
            additionalAuthorization: static _ => Task.FromResult(true),
            interval: TimeSpan.FromMilliseconds(10),
            ct: cts.Token);

        Assert.True(closed.Task.IsCompletedSuccessfully);
        Assert.Equal(1, revalidator.Calls);
        Assert.Equal(0, metrics.Snapshot().JwtRevalidationTransientThresholdCount);
    }

    [Fact]
    public async Task Additional_Authorization_Failure_Closes_Immediately()
    {
        var revalidator = new FakeRevalidator(JwtRevalidationResult.Valid);
        var closed = new TaskCompletionSource();
        var authorizationChecks = 0;

        using var cts = new CancellationTokenSource(TimeSpan.FromSeconds(5));

        await WebSocketJwtRevalidationLoop.RunAsync(
            revalidator,
            token: "t",
            connectionId: "c",
            handlerKind: "Test",
            markRevokedAndCancel: () =>
            {
                closed.TrySetResult();
                return ValueTask.CompletedTask;
            },
            logger: NullLogger.Instance,
            metrics: null,
            additionalAuthorization: _ =>
            {
                authorizationChecks++;
                return Task.FromResult(false);
            },
            interval: TimeSpan.FromMilliseconds(10),
            ct: cts.Token);

        Assert.True(closed.Task.IsCompletedSuccessfully);
        Assert.Equal(1, revalidator.Calls);
        Assert.Equal(1, authorizationChecks);
    }

    private sealed class FakeRevalidator(params JwtRevalidationResult[] results) : IJwtRevalidator
    {
        public int Calls { get; private set; }

        public Task<JwtRevalidationResult> RevalidateAsync(string token, CancellationToken ct)
        {
            var index = Calls;
            Calls++;

            return Task.FromResult(index < results.Length ? results[index] : JwtRevalidationResult.Valid);
        }
    }

    private sealed class FakeDirectory : ILiveSessionStateDirectory
    {
        public IReadOnlyList<SessionId> GetActiveSessions() => [];

        public LiveSessionPorts GetOrCreate(SessionId sessionId) =>
            throw new NotSupportedException();

        public LiveSessionPorts? TryGet(SessionId sessionId) => null;

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

        public bool RemoveIfSame(SessionId sessionId, LiveSessionPorts expected) => false;

        public bool RemoveIfOwned(
            SessionId sessionId,
            LiveSessionCreationOwnership creationOwnership) => false;

    }
}
