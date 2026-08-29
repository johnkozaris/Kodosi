using Kodosi.Host.Auth;
using Kodosi.Host.Observability;

namespace Kodosi.Host.Realtime;

internal static class WebSocketJwtRevalidationLoop
{
    public static readonly TimeSpan Interval = TimeSpan.FromMinutes(1);

    internal const int TransientCloseThreshold = 3;

    public static Task RunAsync(
        IJwtRevalidator revalidator,
        string token,
        string connectionId,
        string handlerKind,
        Func<ValueTask> markRevokedAndCancel,
        ILogger logger,
        OperationalMetrics? metrics,
        CancellationToken ct)
        => RunAsync(
            revalidator,
            token,
            connectionId,
            handlerKind,
            markRevokedAndCancel,
            logger,
            metrics,
            static _ => Task.FromResult(true),
            Interval,
            ct);

    public static Task RunAsync(
        IJwtRevalidator revalidator,
        string token,
        string connectionId,
        string handlerKind,
        Func<ValueTask> markRevokedAndCancel,
        ILogger logger,
        OperationalMetrics? metrics,
        Func<CancellationToken, Task<bool>> additionalAuthorization,
        CancellationToken ct)
        => RunAsync(
            revalidator,
            token,
            connectionId,
            handlerKind,
            markRevokedAndCancel,
            logger,
            metrics,
            additionalAuthorization,
            Interval,
            ct);

    internal static async Task RunAsync(
        IJwtRevalidator revalidator,
        string token,
        string connectionId,
        string handlerKind,
        Func<ValueTask> markRevokedAndCancel,
        ILogger logger,
        OperationalMetrics? metrics,
        Func<CancellationToken, Task<bool>> additionalAuthorization,
        TimeSpan interval,
        CancellationToken ct)
    {
        var state = new LoopState();
        try
        {
            if (await TryRevalidateOrCancelAsync(
                revalidator,
                token,
                connectionId,
                handlerKind,
                markRevokedAndCancel,
                logger,
                metrics,
                additionalAuthorization,
                state,
                ct))
            {
                return;
            }

            using var timer = new PeriodicTimer(interval);
            while (await timer.WaitForNextTickAsync(ct))
            {
                if (await TryRevalidateOrCancelAsync(
                    revalidator,
                    token,
                    connectionId,
                    handlerKind,
                    markRevokedAndCancel,
                    logger,
                    metrics,
                    additionalAuthorization,
                    state,
                    ct))
                {
                    return;
                }
            }
        }
        catch (OperationCanceledException)
        {
        }
    }

    private static async Task<bool> TryRevalidateOrCancelAsync(
        IJwtRevalidator revalidator,
        string token,
        string connectionId,
        string handlerKind,
        Func<ValueTask> markRevokedAndCancel,
        ILogger logger,
        OperationalMetrics? metrics,
        Func<CancellationToken, Task<bool>> additionalAuthorization,
        LoopState state,
        CancellationToken ct)
    {
        var result = await revalidator.RevalidateAsync(token, ct);
        switch (result)
        {
            case JwtRevalidationResult.Invalid:
                logger.LogInformation(
                    "{HandlerKind} WS {ConnectionId}: revalidation failed; tearing down",
                    handlerKind,
                    connectionId);
                await markRevokedAndCancel();
                return true;
            case JwtRevalidationResult.Transient:
                state.ConsecutiveTransients++;
                if (state.ConsecutiveTransients >= TransientCloseThreshold)
                {
                    metrics?.RecordJwtRevalidationTransientThresholdReached();
                    logger.LogWarning(
                        "{HandlerKind} WS {ConnectionId}: {Count} consecutive transient revalidations; fail-closing",
                        handlerKind,
                        connectionId,
                        state.ConsecutiveTransients);
                    await markRevokedAndCancel();
                    return true;
                }
                return false;
            case JwtRevalidationResult.Valid:
                state.ConsecutiveTransients = 0;
                if (!await additionalAuthorization(ct))
                {
                    logger.LogInformation(
                        "{HandlerKind} WS {ConnectionId}: device authorization failed; tearing down",
                        handlerKind,
                        connectionId);
                    await markRevokedAndCancel();
                    return true;
                }
                return false;
            default:
                state.ConsecutiveTransients = 0;
                return false;
        }
    }

    private sealed class LoopState
    {
        public int ConsecutiveTransients;
    }
}
