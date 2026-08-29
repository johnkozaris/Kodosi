using System.Net.WebSockets;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal sealed class HostWebSocketHandler(
    HostHandshake handshake,
    HostSessionPump sessionPump,
    HostTeardown teardown,
    ILogger<HostWebSocketHandler> logger,
    TimeProvider timeProvider)
{
    private readonly HostHandshake _handshake = handshake;
    private readonly HostSessionPump _sessionPump = sessionPump;
    private readonly HostTeardown _teardown = teardown;
    private readonly ILogger<HostWebSocketHandler> _logger = logger;
    private readonly TimeProvider _timeProvider = timeProvider;

    public async Task HandleAsync(
        WebSocket webSocket,
        string sessionIdStr,
        UserId authenticatedUserId,
        string? accessToken,
        CancellationToken ct)
    {
        var state = new HostConnectionState(
            Guid.NewGuid().ToString(),
            sessionIdStr,
            authenticatedUserId,
            _timeProvider);

        try
        {
            if (!await _handshake.TryAcceptAsync(webSocket, state, ct))
            {
                return;
            }

            var result = await _sessionPump.RunAsync(webSocket, state, accessToken, ct);
            state.AuthRevoked = result.AuthRevoked;
            state.HostEnded = result.HostEnded;
            state.CloseReason = result.CloseReason;
        }
        catch (OperationCanceledException)
        {
        }
        catch (WebSocketException ex)
        {
            _logger.LogWarning(ex, "Host WebSocket error for {ConnectionId}", state.ConnectionId);
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Unhandled host WebSocket failure for {ConnectionId}", state.ConnectionId);
        }
        finally
        {
            await _teardown.RunAsync(webSocket, state, ct);
        }
    }
}
