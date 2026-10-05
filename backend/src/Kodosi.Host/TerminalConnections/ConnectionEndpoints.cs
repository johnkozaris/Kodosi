using System.Net.WebSockets;
using System.Security.Claims;
using System.Text.Json;
using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Security;
using Kodosi.Sessions;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.TerminalConnections;

internal static class ConnectionEndpoints
{
    private const int ProtocolVersion = 18;
    private const int PipeIdBytes = 16;
    private const int MessageBytes = 256 * 1024 + PipeIdBytes;
    private static readonly TimeSpan Idle = TimeSpan.FromSeconds(90);
    private static readonly TimeSpan Admission = TimeSpan.FromSeconds(5);

    public static void MapTerminalConnections(this IEndpointRouteBuilder app)
    {
        app.MapGet("/ws/device", RunAsync).RequireAuthorization().RequireRateLimiting("socket");
    }

    private static async Task RunAsync(HttpContext context)
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
        DeviceLink? link = null;
        string? deviceId = null;
        try
        {
            using var handshakeDeadline = CancellationTokenSource.CreateLinkedTokenSource(context.RequestAborted);
            handshakeDeadline.CancelAfter(TimeSpan.FromSeconds(10));
            var helloFrame = await Wire.ReadAsync(socket, 16 * 1024, handshakeDeadline.Token);
            if (helloFrame is null || helloFrame.Value.Type != WebSocketMessageType.Text) throw ApiException.Invalid("Expected device connection hello.");
            using var hello = Wire.Parse(helloFrame.Value.Bytes);
            var root = hello.RootElement;
            if (root.GetProperty("type").GetString() != "hello" || root.GetProperty("protocolVersion").GetInt32() != ProtocolVersion)
                throw ApiException.Invalid("Unsupported terminal connection protocol.");
            deviceId = DeviceIdRules.Require(root.GetProperty("deviceId").GetString());
            if (!deviceSessions.Holds(current.Id, deviceId, root.GetProperty("deviceSession").GetString()))
            {
                await socket.SendAsync(Wire.Encode(new { type = "refused", reason = "deviceSession" }), WebSocketMessageType.Text, true, handshakeDeadline.Token);
                throw new ApiException(428, "A device session is required.");
            }
            await RequireDeviceAsync(scopes, current.Id, deviceId, handshakeDeadline.Token);
            link = new DeviceLink(socket, current.Id, deviceId, Guid.CreateVersion7().ToString("D"));
            link.ExtendUntil(expires);
            directory.Register(link);
            await RequireDeviceAsync(scopes, current.Id, deviceId, handshakeDeadline.Token);
            link.Send(new { type = "ready", connectionId = link.ConnectionId });
            using var lifetime = CancellationTokenSource.CreateLinkedTokenSource(context.RequestAborted, link.Stopped, link.Expired);
            try { await ListenAsync(link, directory, scopes, metrics, lifetime.Token); }
            finally { await lifetime.CancelAsync(); }
        }
        catch (Exception error) when (error is ApiException or JsonException or InvalidOperationException or KeyNotFoundException
                                      or FormatException or WebSocketException or OperationCanceledException or IOException or TimeoutException)
        {
            if (link is not null) logger.LogDebug("Device connection ended: {Failure}", error.GetType().Name);
            else
            {
                var reason = error is ApiException refusal ? refusal.Status.ToString(System.Globalization.CultureInfo.InvariantCulture) : error.GetType().Name;
                metrics.Refused(reason);
                logger.LogInformation("Connection refused: account {Account}, device {Device}, reason {Reason}: {Message}",
                    current.Id, deviceId, reason, error is ApiException ? error.Message : "");
            }
        }
        finally
        {
            if (link is not null) { directory.Remove(link); await link.DisposeAsync(); }
            else socket.Abort();
        }
    }

    private static async Task ListenAsync(DeviceLink link, ConnectionDirectory directory, IServiceScopeFactory scopes, ServerMetrics metrics, CancellationToken ct)
    {
        while (!ct.IsCancellationRequested)
        {
            using var deadline = CancellationTokenSource.CreateLinkedTokenSource(ct);
            deadline.CancelAfter(Idle);
            var frame = await Wire.ReadAsync(link.Socket, MessageBytes, deadline.Token);
            if (frame is not { } received) return;
            if (received.Type == WebSocketMessageType.Binary)
            {
                if (received.Bytes.Length <= PipeIdBytes) throw ApiException.Invalid("Terminal channel message is too short.");
                var relayed = directory.Relay(link, new Guid(received.Bytes.AsSpan(0, PipeIdBytes), bigEndian: true), received.Bytes);
                if (relayed > 0) metrics.Relayed(relayed - PipeIdBytes);
                continue;
            }
            if (received.Bytes.Length > 16 * 1024) throw ApiException.Invalid("Connection message is too large.");
            using var message = Wire.Parse(received.Bytes);
            var root = message.RootElement;
            switch (root.GetProperty("type").GetString())
            {
                case "ping": link.Send(new { type = "pong" }); break;
                case "pong": break;
                case "host":
                {
                    var sessionId = root.GetProperty("sessionId").GetGuid(); var incarnationId = root.GetProperty("incarnationId").GetGuid();
                    var refusal = await AdmitAsync(scopes, ct, async (sessions, admission) =>
                    {
                        var state = await sessions.HostAsync(sessionId, link.UserId, link.DeviceId, admission);
                        SessionService.Incarnation(state, incarnationId);
                        directory.Host(link, state);
                    });
                    if (refusal is null) link.Send(new { type = "hosted", sessionId });
                    else link.Send(new { type = "unhosted", sessionId, status = refusal.Status, message = refusal.Message });
                    break;
                }
                case "unhost": directory.Unhost(link, root.GetProperty("sessionId").GetGuid()); break;
                case "open":
                {
                    var pipe = root.GetProperty("pipe").GetGuid();
                    var sessionId = root.GetProperty("sessionId").GetGuid(); var incarnationId = root.GetProperty("incarnationId").GetGuid();
                    async Task<Session> AuthorizedAsync(SessionService sessions, CancellationToken admission)
                    {
                        var state = await sessions.AuthorizedAsync(sessionId, link.UserId, admission);
                        SessionService.Incarnation(state, incarnationId);
                        return state;
                    }
                    var refusal = pipe == Guid.Empty ? ApiException.Invalid("A view identity is required.") : await AdmitAsync(scopes, ct, async (sessions, admission) =>
                    {
                        directory.Open(link, pipe, await AuthorizedAsync(sessions, admission));
                        try { await AuthorizedAsync(sessions, admission); }
                        catch { directory.Close(link, pipe); throw; }
                    });
                    if (refusal is not null)
                    {
                        metrics.Refused(refusal.Status.ToString(System.Globalization.CultureInfo.InvariantCulture));
                        link.Send(new { type = "closed", pipe, status = refusal.Status, message = refusal.Message });
                    }
                    break;
                }
                case "close": directory.Close(link, root.GetProperty("pipe").GetGuid()); break;
                default: throw ApiException.Invalid("Unknown connection message.");
            }
        }
    }

    private static async Task<ApiException?> AdmitAsync(IServiceScopeFactory scopes, CancellationToken ct, Func<SessionService, CancellationToken, Task> admit)
    {
        using var limit = CancellationTokenSource.CreateLinkedTokenSource(ct);
        limit.CancelAfter(Admission);
        try
        {
            await using var scope = scopes.CreateAsyncScope();
            await admit(scope.ServiceProvider.GetRequiredService<SessionService>(), limit.Token);
            return null;
        }
        catch (ApiException refusal) { return refusal; }
        catch (Exception error) when (!ct.IsCancellationRequested && (error is OperationCanceledException or DbUpdateException or Npgsql.NpgsqlException or TimeoutException
                                      or InvalidOperationException { InnerException: Npgsql.NpgsqlException or TimeoutException }))
        { return new ApiException(503, "The server could not check this terminal; try again."); }
    }

    private static async Task RequireDeviceAsync(IServiceScopeFactory scopes, Guid userId, string deviceId, CancellationToken ct)
    {
        await using var scope = scopes.CreateAsyncScope();
        await scope.ServiceProvider.GetRequiredService<DeviceService>().RequireDeviceAsync(userId, deviceId, ct);
    }
}
