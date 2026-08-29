using System.Net.WebSockets;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal sealed class ParticipantWebSocketHandler(
    ParticipantHandshake handshake,
    ParticipantSessionPump sessionPump,
    ParticipantTeardown teardown,
    ILogger<ParticipantWebSocketHandler> logger,
    TimeProvider timeProvider)
{
    private readonly ParticipantHandshake _handshake = handshake;
    private readonly ParticipantSessionPump _sessionPump = sessionPump;
    private readonly ParticipantTeardown _teardown = teardown;
    private readonly ILogger<ParticipantWebSocketHandler> _logger = logger;
    private readonly TimeProvider _timeProvider = timeProvider;

    public async Task HandleAsync(
        WebSocket webSocket,
        string sessionIdStr,
        UserId authenticatedUserId,
        string? accessToken,
        CancellationToken ct)
    {
        var state = new ParticipantConnectionState(
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
        }
        catch (OperationCanceledException)
        {
        }
        catch (WebSocketException ex)
        {
            _logger.LogWarning(ex, "Participant WebSocket error for {ConnectionId}", state.ConnectionId);
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Unhandled participant WebSocket failure for {ConnectionId}", state.ConnectionId);
        }
        finally
        {
            await _teardown.RunAsync(webSocket, state, ct);
        }
    }
}
