using System.Net.WebSockets;

namespace Kodosi.Host.Realtime;

internal static class WebSocketSendDeadline
{
    private static readonly TimeSpan CancellationGrace = TimeSpan.FromMilliseconds(50);

    public static async Task<bool> SendAsync(
        WebSocket webSocket,
        byte[] payload,
        WebSocketMessageType messageType,
        TimeSpan deadline,
        CancellationToken ct,
        bool throwOnCancellation = true)
    {
        using var sendCts = CancellationTokenSource.CreateLinkedTokenSource(ct);
        var sendTask = webSocket.SendAsync(
            new ArraySegment<byte>(payload),
            messageType,
            endOfMessage: true,
            sendCts.Token);
        try
        {
            await sendTask.WaitAsync(deadline, ct);
            return true;
        }
        catch (TimeoutException)
        {
            sendCts.Cancel();
            webSocket.Abort();
            ObserveFault(sendTask);
            return false;
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            sendCts.Cancel();
            try
            {
                await sendTask.WaitAsync(CancellationGrace);
            }
            catch (OperationCanceledException)
            {
            }
            catch (TimeoutException)
            {
                webSocket.Abort();
                ObserveFault(sendTask);
            }

            if (throwOnCancellation)
            {
                throw new OperationCanceledException(ct);
            }
            return false;
        }
    }

    private static void ObserveFault(Task sendTask)
    {
        _ = sendTask.ContinueWith(
            static completed => _ = completed.Exception,
            CancellationToken.None,
            TaskContinuationOptions.OnlyOnFaulted
                | TaskContinuationOptions.ExecuteSynchronously,
            TaskScheduler.Default);
    }
}
