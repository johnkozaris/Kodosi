using System.Buffers;
using System.Net.WebSockets;
using System.Security.Claims;
using System.Security.Cryptography;
using System.Text.Json;
using Kodosi.Accounts;
using Kodosi.Admission;
using Kodosi.Devices;
using Kodosi.Security;
using Kodosi.Sessions;

namespace Kodosi.TerminalConnections;

internal static class ConnectionEndpoints
{
    private const int ProtocolVersion = 16;
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
        var signatureVerifier = providers.GetRequiredService<SignatureVerifier>();
        var directory = providers.GetRequiredService<ConnectionDirectory>();
        var gate = providers.GetRequiredService<AdmissionGate>();
        var scopes = providers.GetRequiredService<IServiceScopeFactory>();
        var logger = providers.GetRequiredService<ILoggerFactory>().CreateLogger("TerminalConnections");
        if (!long.TryParse(context.User.FindFirstValue("exp"), out var expiresSeconds)) { context.Response.StatusCode = 401; return; }
        var expires = DateTimeOffset.FromUnixTimeSeconds(expiresSeconds);
        if (expires <= DateTimeOffset.UtcNow) { context.Response.StatusCode = 401; return; }
        using var socket = await context.WebSockets.AcceptWebSocketAsync();
        Peer? peer = null;
        ConnectionDirectory.LiveSession? live = null;
        Pipe? pipe = null;
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
            var deviceId = DeviceIdRules.Require(root.GetProperty("deviceId").GetString());
            Guid? incarnationId = sessionId is null ? null : root.GetProperty("incarnationId").GetGuid();
            var connectionId = Guid.CreateVersion7().ToString("D");
            var challenge = RandomNumberGenerator.GetBytes(32);
            await socket.SendAsync(Wire.Encode(new { type = "challenge", connectionId, challenge = Convert.ToBase64String(challenge) }), WebSocketMessageType.Text, true, handshakeDeadline.Token);
            var authFrame = await Wire.ReadAsync(socket, 16 * 1024, handshakeDeadline.Token);
            if (authFrame is null || authFrame.Value.Type != WebSocketMessageType.Text) throw ApiException.Invalid("Expected terminal connection device authentication.");
            using var auth = Wire.Parse(authFrame.Value.Bytes);
            if (auth.RootElement.GetProperty("type").GetString() != "authenticate") throw ApiException.Invalid("Expected device authentication.");
            var signature = Limits.Base64(auth.RootElement.GetProperty("signature").GetString(), "Device signature", IdentityWireFormat.MlDsa65SignatureLength);
            using (await gate.EnterSharedAsync(handshakeDeadline.Token))
            {
                await using var scope = scopes.CreateAsyncScope();
                var devices = scope.ServiceProvider.GetRequiredService<DeviceService>();
                var device = await devices.RequireDeviceAsync(current.Id, deviceId, handshakeDeadline.Token);
                if (!signatureVerifier.Verify(device.SigningPublicKey, Proofs.Connection(current.Id, deviceId, connectionId, purpose, sessionId, incarnationId, challenge), signature))
                    throw ApiException.Forbidden("Invalid terminal connection device proof.");
                var ready = Wire.Encode(new { type = "ready", connectionId, incarnationId });
                if (sessionId is { } id)
                {
                    var sessions = scope.ServiceProvider.GetRequiredService<SessionService>();
                    var state = purpose == "participant"
                        ? await sessions.AuthorizedAsync(id, current.Id, handshakeDeadline.Token)
                        : await sessions.HostAsync(id, current.Id, deviceId, handshakeDeadline.Token);
                    SessionService.Incarnation(state, incarnationId ?? Guid.Empty);
                    if (purpose == "host")
                    {
                        var host = new SocketPeer(socket, current.Id, deviceId, connectionId);
                        peer = host; peer.ExtendUntil(expires);
                        live = directory.RegisterHost(state, host);
                        host.Send(new { type = "ready", connectionId, incarnationId });
                    }
                    else
                    {
                        peer = new Peer(socket, current.Id, deviceId, connectionId); peer.ExtendUntil(expires);
                        (live, pipe) = purpose == "participant" ? directory.OpenPipe(state, peer) : directory.JoinPipe(state, channelId ?? Guid.Empty, peer);
                        await socket.SendAsync(ready, WebSocketMessageType.Text, true, handshakeDeadline.Token);
                    }
                }
                else
                {
                    var listener = new SocketPeer(socket, current.Id, deviceId, connectionId);
                    peer = listener; peer.ExtendUntil(expires);
                    if (!directory.RegisterEvents(listener)) throw new ApiException(503, "Too many device event connections.");
                    listener.Send(new { type = "ready", connectionId });
                }
            }
            using var lifetime = CancellationTokenSource.CreateLinkedTokenSource(context.RequestAborted, peer.Stopped, peer.Expired);
            try
            {
                if (pipe is not null)
                {
                    var other = purpose == "participant" ? await pipe.Joined.Task.WaitAsync(JoinWait, lifetime.Token) : pipe.Viewer;
                    await RelayAsync(socket, other.Socket, lifetime.Token);
                }
                else await ListenAsync((SocketPeer)peer, lifetime.Token);
            }
            finally { await lifetime.CancelAsync(); }
        }
        catch (Exception error) when (error is ApiException or JsonException or InvalidOperationException or KeyNotFoundException
                                      or FormatException or WebSocketException or OperationCanceledException or IOException or TimeoutException)
        {
            logger.LogDebug("Terminal connection {Purpose} ended: {Failure}", purpose, error.GetType().Name);
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

    private static async Task RelayAsync(WebSocket from, WebSocket to, CancellationToken ct)
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
            }
        }
        finally { ArrayPool<byte>.Shared.Return(buffer); }
    }
}
