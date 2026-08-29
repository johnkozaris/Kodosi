using System.Net.WebSockets;

namespace Kodosi.Host.Realtime;

internal static class WebSocketHandshakeReader
{
    public static async Task<WebSocketMessageReader.Result?> ReceiveAsync(
        WebSocket webSocket,
        int maxMessageBytes,
        TimeSpan timeout,
        CancellationToken ct)
    {
        using var timeoutCts = CancellationTokenSource.CreateLinkedTokenSource(ct);
        timeoutCts.CancelAfter(timeout);
        try
        {
            return await WebSocketMessageReader.ReceiveAsync(
                webSocket,
                maxMessageBytes,
                timeoutCts.Token);
        }
        catch (OperationCanceledException) when (!ct.IsCancellationRequested)
        {
            webSocket.Abort();
            return null;
        }
    }
}
