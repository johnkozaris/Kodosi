using System.Net.WebSockets;

namespace Kodosi.Host.Realtime;

internal static class WebSocketCloseHelper
{
    private static readonly TimeSpan DefaultDeadline = TimeSpan.FromSeconds(5);

    public static async Task CloseOutputAsync(
        WebSocket webSocket,
        WebSocketCloseStatus closeStatus,
        string closeReason,
        CancellationToken ct,
        TimeSpan? deadline = null)
    {
        if (webSocket.State is not (WebSocketState.Open or WebSocketState.CloseReceived))
        {
            return;
        }

        using var closeCts = CancellationTokenSource.CreateLinkedTokenSource(ct);
        Task closeTask;
        try
        {


            closeTask = webSocket.CloseOutputAsync(
                closeStatus,
                closeReason,
                closeCts.Token);
        }
        catch (Exception ex) when (ex is WebSocketException
            or ObjectDisposedException
            or InvalidOperationException)
        {
            closeCts.Cancel();
            webSocket.Abort();
            return;
        }

        try
        {
            await closeTask.WaitAsync(deadline ?? DefaultDeadline, ct);
        }
        catch (TimeoutException)
        {
            closeCts.Cancel();
            webSocket.Abort();
            ObserveFault(closeTask);
        }
        catch (OperationCanceledException)
        {
            closeCts.Cancel();
            webSocket.Abort();
            ObserveFault(closeTask);
        }
        catch (Exception ex) when (ex is WebSocketException
            or ObjectDisposedException
            or InvalidOperationException)
        {
            closeCts.Cancel();
            webSocket.Abort();
            ObserveFault(closeTask);
        }
    }

    private static void ObserveFault(Task task)
    {
        _ = task.ContinueWith(
            static completed => _ = completed.Exception,
            CancellationToken.None,
            TaskContinuationOptions.OnlyOnFaulted
                | TaskContinuationOptions.ExecuteSynchronously,
            TaskScheduler.Default);
    }
}
