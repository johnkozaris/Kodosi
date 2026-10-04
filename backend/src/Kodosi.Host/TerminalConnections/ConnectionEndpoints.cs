using System.Buffers;
using System.Net.WebSockets;
using System.Security.Claims;
using System.Text.Json;
using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Security;
using Kodosi.Sessions;

namespace Kodosi.TerminalConnections;

internal static class ConnectionEndpoints
{
    private const int ProtocolVersion = 17;
    private const int PipeMessageBytes = 256 * 1024;
    private static readonly TimeSpan Idle = TimeSpan.FromSeconds(90);
    private static readonly TimeSpan SlowSend = TimeSpan.FromSeconds(30);
    private static readonly TimeSpan JoinWait = TimeSpan.FromSeconds(10);

    public static void MapTerminalConnections(this IEndpointRouteBuilder app)
    {
        app.MapGet("/ws/host/{id:guid}", (Guid id, HttpContext context) => RunAsync(context, id, null, "host")).RequireAuthorization().RequireRateLimiting("socket");
        app.MapGet("/ws/participant/{id:guid}", (Guid id, HttpContext context) => RunAsync(context, id, null, "participant")).RequireAuthorization().RequireRateLimiting("socket");
        app.MapGet("/ws/relay/{id:guid}/{channel:guid}", (Guid id, Guid channel, HttpContext context) => RunAsync(context, id, channel, "relay")).RequireAuthorization().RequireRateLimiting("socket");
        app.MapGet("/ws/events", (HttpContext context) => RunAsync(context, null, null, "events")).RequireAuthorization().RequireRateLimiting("socket");
    }

    private static async Task RunAsync(HttpContext context, Guid? sessionId, Guid? channelId, string purpose)
    {
        if (!context.WebSockets.IsWebSocketRequest) { context.Response.StatusCode = 400; return; }
        var providers = context.RequestServices;
        var current = await providers.GetRequiredService<CurrentUser>().GetAsync(context, context.RequestAborted);
        var deviceSessions = providers.GetRequiredService<DeviceSessions>();
        var directory = providers.GetRequiredService<ConnectionDirectory>();
        var metrics = providers.GetRequiredService<ServerMetrics>();
        var scopes = providers.GetRequiredService<IServiceScopeFactory>();
        var logger = providers.GetRequiredService<ILoggerFactory>().CreateLogger("TerminalConnections");
        if (!long.TryParse(context.User.FindFirstValue("exp"), out var expiresSeconds)) { context.Response.StatusCode = 401; return; }
        var expires = DateTimeOffset.FromUnixTimeSeconds(expiresSeconds);
        if (expires <= DateTimeOffset.UtcNow) { context.Response.StatusCode = 401; return; }
        using var socket = await context.WebSockets.AcceptWebSocketAsync();
        Peer? peer = null;
        ConnectionDirectory.LiveSession? live = null;
        Pipe? pipe = null;
        var admitted = false;
        string? deviceId = null;
        try
        {
            using var handshakeDeadline = CancellationTokenSource.CreateLinkedTokenSource(context.RequestAborted);
            handshakeDeadline.CancelAfter(TimeSpan.FromSeconds(10));
            var helloFrame = await Wire.ReadAsync(socket, 16 * 1024, handshakeDeadline.Token);
            if (helloFrame is null || helloFrame.Value.Type != WebSocketMessageType.Text) throw ApiException.Invalid("Expected terminal connection hello.");
            using var hello = Wire.Parse(helloFrame.Value.Bytes);
            var root = hello.RootElement;
            if (root.GetProperty("type").GetString() != "hello" || root.GetProperty("protocolVersion").GetInt32() != ProtocolVersion)
                throw ApiException.Invalid("Unsupported terminal connection protocol.");
            deviceId = DeviceIdRules.Require(root.GetProperty("deviceId").GetString());
            Guid? incarnationId = sessionId is null ? null : root.GetProperty("incarnationId").GetGuid();
            if (!deviceSessions.Holds(current.Id, deviceId, root.GetProperty("deviceSession").GetString()))
            {
                await socket.SendAsync(Wire.Encode(new { type = "refused", reason = "deviceSession" }), WebSocketMessageType.Text, true, handshakeDeadline.Token);
                throw new ApiException(428, "A device session is required.");
            }
            var connectionId = Guid.CreateVersion7().ToString("D");
            async Task<Session?> AdmitAsync()
            {
                await using var scope = scopes.CreateAsyncScope();
                await scope.ServiceProvider.GetRequiredService<DeviceService>().RequireDeviceAsync(current.Id, deviceId, handshakeDeadline.Token);
                if (sessionId is not { } id) return null;
                var sessions = scope.ServiceProvider.GetRequiredService<SessionService>();
                var state = purpose == "participant"
                    ? await sessions.AuthorizedAsync(id, current.Id, handshakeDeadline.Token)
                    : await sessions.HostAsync(id, current.Id, deviceId, handshakeDeadline.Token);
                SessionService.Incarnation(state, incarnationId ?? Guid.Empty);
                return state;
            }
            var ready = new { type = "ready", connectionId, incarnationId };
            if (await AdmitAsync() is { } state)
            {
                if (purpose == "host")
                {
                    var host = new SocketPeer(socket, current.Id, deviceId, connectionId);
                    peer = host; peer.ExtendUntil(expires);
                    live = directory.RegisterHost(state, host);
                }
                else
                {
                    peer = new Peer(socket, current.Id, deviceId, connectionId); peer.ExtendUntil(expires);
                    (live, pipe) = purpose == "participant" ? directory.OpenPipe(state, peer) : directory.JoinPipe(state, channelId ?? Guid.Empty, peer);
                }
            }
            else
            {
                var listener = new SocketPeer(socket, current.Id, deviceId, connectionId);
                peer = listener; peer.ExtendUntil(expires);
                if (!directory.RegisterEvents(listener)) throw new ApiException(503, "Too many device event connections.");
            }
            await AdmitAsync();
            if (peer is SocketPeer queued) queued.Send(ready);
            else await socket.SendAsync(Wire.Encode(ready), WebSocketMessageType.Text, true, handshakeDeadline.Token);
            admitted = true;
            if (purpose == "relay") pipe?.Joined.TrySetResult(peer);
            using var lifetime = CancellationTokenSource.CreateLinkedTokenSource(context.RequestAborted, peer.Stopped, peer.Expired);
            try
            {
                if (pipe is not null)
                {
                    var other = purpose == "participant" ? await pipe.Joined.Task.WaitAsync(JoinWait, lifetime.Token) : pipe.Viewer;
                    await RelayAsync(socket, other.Socket, metrics, lifetime.Token);
                }
                else await ListenAsync((SocketPeer)peer, lifetime.Token);
            }
            finally { await lifetime.CancelAsync(); }
        }
        catch (Exception error) when (error is ApiException or JsonException or InvalidOperationException or KeyNotFoundException
                                      or FormatException or WebSocketException or OperationCanceledException or IOException or TimeoutException)
        {
            if (admitted) logger.LogDebug("Terminal connection {Purpose} ended: {Failure}", purpose, error.GetType().Name);
            else
            {
                var reason = error is ApiException refusal ? refusal.Status.ToString(System.Globalization.CultureInfo.InvariantCulture) : error.GetType().Name;
                metrics.Refused(reason);
                logger.LogInformation("Connection refused: purpose {Purpose}, account {Account}, device {Device}, terminal {Terminal}, reason {Reason}: {Message}",
                    purpose, current.Id, deviceId, sessionId, reason, error is ApiException ? error.Message : "");
            }
        }
        finally
        {
            if (live is not null && pipe is not null) ConnectionDirectory.ClosePipe(live, pipe);
            else if (live is not null) directory.RemoveHost(live);
            else if (peer is SocketPeer listener) directory.RemoveEvents(listener);
            if (peer is not null) await peer.DisposeAsync(); else socket.Abort();
        }
    }

    private static async Task ListenAsync(SocketPeer peer, CancellationToken ct)
    {
        while (!ct.IsCancellationRequested)
        {
            using var deadline = CancellationTokenSource.CreateLinkedTokenSource(ct);
            deadline.CancelAfter(Idle);
            var frame = await Wire.ReadAsync(peer.Socket, 16 * 1024, deadline.Token);
            if (frame is null) return;
            if (frame.Value.Type != WebSocketMessageType.Text) throw ApiException.Invalid("Unexpected connection message.");
            using var message = Wire.Parse(frame.Value.Bytes);
            var type = message.RootElement.GetProperty("type").GetString();
            if (type == "ping") peer.Send(new { type = "pong" });
            else if (type != "pong") throw ApiException.Invalid("Unknown connection message.");
        }
    }

    private static async Task RelayAsync(WebSocket from, WebSocket to, ServerMetrics metrics, CancellationToken ct)
    {
        var buffer = ArrayPool<byte>.Shared.Rent(64 * 1024);
        try
        {
            var message = 0;
            while (true)
            {
                using var idle = CancellationTokenSource.CreateLinkedTokenSource(ct);
                idle.CancelAfter(Idle);
                var read = await from.ReceiveAsync(buffer.AsMemory(), idle.Token);
                if (read.MessageType == WebSocketMessageType.Close) return;
                if (read.MessageType != WebSocketMessageType.Binary) throw ApiException.Invalid("Unexpected terminal channel message.");
                message += read.Count;
                if (message > PipeMessageBytes) throw ApiException.Invalid("Terminal channel message is too large.");
                if (read.EndOfMessage) message = 0;
                using var send = CancellationTokenSource.CreateLinkedTokenSource(ct);
                send.CancelAfter(SlowSend);
                await to.SendAsync(buffer.AsMemory(0, read.Count), WebSocketMessageType.Binary, read.EndOfMessage, send.Token);
                metrics.Relayed(read.Count);
            }
        }
        finally { ArrayPool<byte>.Shared.Return(buffer); }
    }
}
