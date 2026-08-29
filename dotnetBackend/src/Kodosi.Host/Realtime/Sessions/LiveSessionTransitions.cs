using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal static class LiveSessionTransitions
{
    public static Task<LiveSessionTransitionResult>
        TransitionTimedOutHostToReconnectingAsync(
            IServiceScopeFactory scopeFactory,
            ILogger logger,
            SessionId sessionId,
            DateTimeOffset expectedStartedAt,
            string expectedConnectionId,
            CancellationToken ct)
    {
        return ExecuteAsync(
            scopeFactory,
            logger,
            services => services.GetRequiredService<LiveSessionTransitionOrchestrator>()
                .TransitionTimedOutHostToReconnectingAsync(
                    sessionId,
                    expectedStartedAt,
                    expectedConnectionId,
                    ct),
            ct,
            publishOnlyWhenApplied: true);
    }

    public static Task<LiveSessionTransitionResult>
        TransitionExpectedHostToReconnectingAsync(
            IServiceScopeFactory scopeFactory,
            ILogger logger,
            SessionId sessionId,
            DateTimeOffset expectedStartedAt,
            Guid expectedIncarnationId,
            string expectedConnectionId,
            CancellationToken ct)
    {
        return ExecuteAsync(
            scopeFactory,
            logger,
            services => services.GetRequiredService<LiveSessionTransitionOrchestrator>()
                .TransitionExpectedHostToReconnectingAsync(
                    sessionId,
                    expectedStartedAt,
                    expectedIncarnationId,
                    expectedConnectionId,
                    ct),
            ct,
            publishOnlyWhenApplied: true);
    }

    public static Task<LiveSessionTransitionResult> EndHostedSessionAsync(
        IServiceScopeFactory scopeFactory,
        ILogger logger,
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        string expectedConnectionId,
        CancellationToken ct)
    {
        return ExecuteAsync(
            scopeFactory,
            logger,
            services => services.GetRequiredService<LiveSessionTerminator>()
                .EndHostedSessionAsync(
                    sessionId,
                    expectedStartedAt,
                    expectedConnectionId,
                    ct),
            ct,
            publishOnlyWhenApplied: true);
    }

    public static Task<LiveSessionTransitionResult> EndOwnedSessionIdempotentlyAsync(
        IServiceScopeFactory scopeFactory,
        ILogger logger,
        SessionId sessionId,
        UserId ownerUserId,
        Guid expectedIncarnationId,
        Guid mutationId,
        Guid attemptId,
        CancellationToken ct)
    {
        return ExecuteAsync(
            scopeFactory,
            logger,
            services => services.GetRequiredService<LiveSessionTerminator>()
                .EndOwnedSessionIdempotentlyAsync(
                    sessionId,
                    ownerUserId,
                    expectedIncarnationId,
                    mutationId,
                    attemptId,
                    ct),
            ct,
            publishOnlyWhenApplied: true);
    }

    public static Task<LiveSessionTransitionResult> EndDisconnectedSessionAsync(
        IServiceScopeFactory scopeFactory,
        ILogger logger,
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        CancellationToken ct)
    {
        return ExecuteAsync(
            scopeFactory,
            logger,
            services => services.GetRequiredService<LiveSessionTerminator>()
                .EndDisconnectedSessionAsync(sessionId, expectedStartedAt, ct),
            ct,
            publishOnlyWhenApplied: true);
    }

    public static Task<LiveSessionTransitionResult> EndOrphanedSessionAsync(
        IServiceScopeFactory scopeFactory,
        ILogger logger,
        OrphanedSessionCandidate candidate,
        CancellationToken ct)
    {
        return ExecuteAsync(
            scopeFactory,
            logger,
            services => services.GetRequiredService<LiveSessionTerminator>()
                .EndOrphanedSessionAsync(candidate, ct),
            ct,
            publishOnlyWhenApplied: true);
    }

    private static async Task<LiveSessionTransitionResult> ExecuteAsync(
        IServiceScopeFactory scopeFactory,
        ILogger logger,
        Func<IServiceProvider, Task<LiveSessionTransitionResult>> transition,
        CancellationToken ct,
        bool publishOnlyWhenApplied = false)
    {
        await using var scope = scopeFactory.CreateAsyncScope();
        var sharedSurfaceEvents = scope.ServiceProvider.GetRequiredService<SharedSurfaceEventPublisher>();
        var result = await transition(scope.ServiceProvider);
        if (!publishOnlyWhenApplied || result.Outcome == SessionTransitionOutcome.Applied)
        {
            await SessionLifecycleInvalidationPublisher.TryPublishAsync(
                sharedSurfaceEvents,
                result,
                logger,
                ct);
        }
        return result;
    }
}
