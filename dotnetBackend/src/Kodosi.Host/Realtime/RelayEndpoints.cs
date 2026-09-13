using System.Net.WebSockets;
using System.Security.Claims;
using System.Security.Cryptography;
using System.Text.Json;
using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Security;
using Kodosi.Sessions;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Realtime;

internal static class RelayEndpoints
{
    public static void MapRelay(this IEndpointRouteBuilder app)
    {
        app.MapGet("/ws/host/{id:guid}", (Guid id, HttpContext context) => RunAsync(context, id, "host")).RequireAuthorization().RequireRateLimiting("socket");
        app.MapGet("/ws/participant/{id:guid}", (Guid id, HttpContext context) => RunAsync(context, id, "participant")).RequireAuthorization().RequireRateLimiting("socket");
        app.MapGet("/ws/events", (HttpContext context) => RunAsync(context, null, "events")).RequireAuthorization().RequireRateLimiting("socket");
    }

    private static async Task RunAsync(HttpContext context, Guid? sessionId, string purpose)
    {
        if (!context.WebSockets.IsWebSocketRequest) { context.Response.StatusCode = 400; return; }
        var providers = context.RequestServices;
        var current = await providers.GetRequiredService<CurrentUser>().GetAsync(context, context.RequestAborted);
        var signatureVerifier = providers.GetRequiredService<SignatureVerifier>();
        var directory = providers.GetRequiredService<RelayDirectory>();
        var gate = providers.GetRequiredService<AdmissionGate>();
        var scopes = providers.GetRequiredService<IServiceScopeFactory>();
        var logger = providers.GetRequiredService<ILoggerFactory>().CreateLogger("TerminalRelay");
        if (!long.TryParse(context.User.FindFirstValue("exp"), out var expiresSeconds)) { context.Response.StatusCode = 401; return; }
        var expires = DateTimeOffset.FromUnixTimeSeconds(expiresSeconds);
        if (expires <= DateTimeOffset.UtcNow) { context.Response.StatusCode = 401; return; }
        using var socket = await context.WebSockets.AcceptWebSocketAsync();
        SocketPeer? peer = null;
        RelayDirectory.LiveSession? live = null;
        try
        {
            using var handshakeDeadline = CancellationTokenSource.CreateLinkedTokenSource(context.RequestAborted);
            handshakeDeadline.CancelAfter(TimeSpan.FromSeconds(10));
            var helloFrame = await Wire.ReadAsync(socket, 16 * 1024, handshakeDeadline.Token);
            if (helloFrame is null || helloFrame.Value.Type != WebSocketMessageType.Text) throw ApiException.Invalid("Expected relay hello.");
            using var hello = Wire.Parse(helloFrame.Value.Bytes);
            var root = hello.RootElement;
            if (root.GetProperty("type").GetString() != "hello" || root.GetProperty("protocolVersion").GetInt32() != 11)
                throw ApiException.Invalid("Unsupported relay protocol.");
            var deviceId = DeviceIdRules.Require(root.GetProperty("deviceId").GetString());
            Guid? incarnationId = sessionId is null ? null : root.GetProperty("incarnationId").GetGuid();
            var checkpointChallenge = purpose == "participant" ? root.GetProperty("checkpointChallenge").GetString() : null;
            var connectionId = Guid.CreateVersion7().ToString("D");
            var challenge = RandomNumberGenerator.GetBytes(32);
            await socket.SendAsync(Wire.Encode(new { type = "challenge", connectionId, challenge = Convert.ToBase64String(challenge) }), WebSocketMessageType.Text, true, handshakeDeadline.Token);
            var authFrame = await Wire.ReadAsync(socket, 16 * 1024, handshakeDeadline.Token);
            if (authFrame is null || authFrame.Value.Type != WebSocketMessageType.Text) throw ApiException.Invalid("Expected relay device authentication.");
            using var auth = Wire.Parse(authFrame.Value.Bytes);
            if (auth.RootElement.GetProperty("type").GetString() != "authenticate") throw ApiException.Invalid("Expected device authentication.");
            var signature = Limits.Base64(auth.RootElement.GetProperty("signature").GetString(), "Device signature", IdentityWireFormat.MlDsa65SignatureLength);
            using (await gate.EnterAsync(handshakeDeadline.Token))
            {
                await using var scope = scopes.CreateAsyncScope();
                var devices = scope.ServiceProvider.GetRequiredService<DeviceService>();
                var device = await devices.RequireDeviceAsync(current.Id, deviceId, handshakeDeadline.Token);
                if (!signatureVerifier.Verify(device.SigningPublicKey, Proofs.Connection(current.Id, deviceId, connectionId, purpose, sessionId, incarnationId, challenge), signature))
                    throw ApiException.Forbidden("Invalid relay device proof.");
                peer = new SocketPeer(socket, current.Id, deviceId, connectionId);
                if (sessionId is { } id)
                {
                    var sessions = scope.ServiceProvider.GetRequiredService<SessionService>();
                    var state = purpose == "host"
                        ? await sessions.HostAsync(id, current.Id, deviceId, handshakeDeadline.Token)
                        : await sessions.AuthorizedAsync(id, current.Id, handshakeDeadline.Token);
                    SessionService.Incarnation(state, incarnationId ?? Guid.Empty);
                    if (purpose == "host")
                    {
                        state.Ready = false;
                        state.ExpiresAt = providers.GetRequiredService<TimeProvider>().GetUtcNow() + PublicationCleanup.GracePeriod;
                        directory.Invalidate(state, notifyHost: false);
                        var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
                        await using var transaction = await db.Database.BeginTransactionAsync(handshakeDeadline.Token);
                        await db.SessionKeys.Where(x => x.SessionId == id).ExecuteDeleteAsync(handshakeDeadline.Token);
                        await db.SaveChangesAsync(handshakeDeadline.Token);
                        await transaction.CommitAsync(handshakeDeadline.Token);
                        live = directory.RegisterHost(state, peer);
                    }
                    else
                    {
                        var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
                        if (!await db.SessionKeys.AnyAsync(x => x.SessionId == id && x.KeyGeneration == state.KeyGeneration
                            && x.RecipientUserId == current.Id && x.RecipientDeviceId == deviceId, handshakeDeadline.Token))
                            throw ApiException.Forbidden("This device has no current session key envelope.");
                        live = directory.RegisterParticipant(state, peer);
                    }
                    peer.Send(new { type = "ready", connectionId, incarnationId, authorizationRevision = state.AuthorizationRevision, keyGeneration = state.KeyGeneration });
                    if (purpose != "host") directory.BeginBootstrap(live, peer, checkpointChallenge);
                }
                else
                {
                    if (!directory.RegisterEvents(peer)) throw new ApiException(503, "Too many device event connections.");
                    peer.Send(new { type = "ready", connectionId });
                }
            }
            using var lifetime = CancellationTokenSource.CreateLinkedTokenSource(context.RequestAborted, peer.Stopped);
            var remaining = expires - DateTimeOffset.UtcNow;
            if (remaining <= TimeSpan.Zero) return;
            lifetime.CancelAfter(remaining);
            try
            {
                while (!lifetime.IsCancellationRequested)
                {
                    using var receiveDeadline = CancellationTokenSource.CreateLinkedTokenSource(lifetime.Token);
                    receiveDeadline.CancelAfter(TimeSpan.FromSeconds(90));
                    var frame = await Wire.ReadAsync(socket, TerminalReplay.MaximumFrame * 2, receiveDeadline.Token);
                    if (frame is null) break;
                    if (frame.Value.Type == WebSocketMessageType.Binary)
                    {
                        if (purpose != "host" || live is null) throw ApiException.Invalid("Only hosts publish terminal frames.");
                        directory.Output(live, peer, frame.Value.Bytes); continue;
                    }
                    using var message = Wire.Parse(frame.Value.Bytes);
                    var body = message.RootElement;
                    var type = body.GetProperty("type").GetString();
                    if (type == "ping") { peer.Send(new { type = "pong" }); continue; }
                    if (type == "pong") continue;
                    if (live is null) throw ApiException.Invalid("Unknown event subscription message.");
                    if (purpose == "host")
                    {
                        switch (type)
                        {
                            case "end":
                                await directory.EndSessionAsync(live.Id, peer, body.GetProperty("finalSequence").GetUInt64());
                                peer.Send(new { type = "endAcknowledged" });
                                break;
                            case "checkpoint":
                                directory.Checkpoint(live, peer, body.GetProperty("requestId").GetGuid(),
                                    Limits.Base64(body.GetProperty("frame").GetString(), "Terminal checkpoint", TerminalReplay.MaximumFrame),
                                    body.GetProperty("signature").GetString());
                                break;
                            case "metadataChanged": directory.MetadataChanged(live, peer); break;
                            case "controlResult": directory.Result(live, peer, body); break;
                            default: throw ApiException.Invalid("Unknown host message.");
                        }
                    }
                    else if (type == "control") directory.Control(live, peer, body);
                    else if (type == "checkpointRequest") directory.RequestCapture(live, peer, body.GetProperty("challenge").GetString());
                    else throw ApiException.Invalid("Unknown participant message.");
                }
            }
            finally { await lifetime.CancelAsync(); }
        }
        catch (Exception error) when (error is ApiException or JsonException or InvalidOperationException or KeyNotFoundException
                                      or FormatException or WebSocketException or OperationCanceledException or IOException)
        {
            logger.LogDebug("Relay {Purpose} ended: {Failure}", purpose, error.GetType().Name);
        }
        finally
        {
            if (peer is not null)
            {
                if (live is not null) directory.RemovePeer(live, peer, purpose == "host"); else directory.RemoveEvents(peer);
                await peer.DisposeAsync();
            }
            else socket.Abort();
        }
    }
}
