using System.Globalization;
using System.Net.WebSockets;
using System.Security.Claims;
using System.Text.Json;
using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Security;
using Kodosi.Sessions;

namespace Kodosi.TerminalConnections;

internal static partial class ConnectionEndpoints
{
    internal const int PipeIdBytes = 16;
    internal const int MessageBytes = 256 * 1024 + PipeIdBytes;
    internal const int ControlBytes = 16 * 1024;
    private const int MessageBurst = 4096;
    private const int MessagesEachSecond = 128;
    private const int DatabaseCost = 2;
    private const int StatusBurst = 600;
    private const int StatusesEachSecond = 10;
    private static readonly TimeSpan Handshake = TimeSpan.FromSeconds(10);
    private static readonly TimeSpan Admission = TimeSpan.FromSeconds(5);

    public static void MapTerminalConnections(this IEndpointRouteBuilder app) =>
        app.MapGet("/ws/device", RunAsync).RequireAuthorization().RequireRateLimiting(RateLimits.Socket);

    private static async Task RunAsync(HttpContext context, IServiceScopeFactory scopes, ConnectionDirectory directory, DeviceSessions deviceSessions,
        ServerMetrics metrics, TimeProvider clock, ILoggerFactory loggers)
    {
        if (!context.WebSockets.IsWebSocketRequest) { context.Response.StatusCode = StatusCodes.Status400BadRequest; return; }
        var logger = loggers.CreateLogger("TerminalConnections");
        User user;
        await using (var scope = scopes.CreateAsyncScope())
            user = await scope.ServiceProvider.GetRequiredService<CurrentUser>().GetAsync(context, context.RequestAborted);
        if (!long.TryParse(context.User.FindFirstValue("exp"), out var expiresSeconds)) { context.Response.StatusCode = StatusCodes.Status401Unauthorized; return; }
        var expires = DateTimeOffset.FromUnixTimeSeconds(expiresSeconds);
        if (expires <= clock.GetUtcNow()) { context.Response.StatusCode = StatusCodes.Status401Unauthorized; return; }
        using var socket = await context.WebSockets.AcceptWebSocketAsync();
        using var reader = new MessageReader(socket);
        DeviceConnection? connection = null;
        string? deviceId = null;
        try
        {
            using (var handshake = CancellationTokenSource.CreateLinkedTokenSource(context.RequestAborted))
            {
                handshake.CancelAfter(Handshake);
                (deviceId, var secret) = await HelloAsync(socket, reader, handshake.Token);
                await RequireSessionAsync(socket, deviceSessions, user.Id, deviceId, secret, handshake.Token);
                await RequireApprovedAsync(scopes, user.Id, deviceId, handshake.Token);
                connection = new DeviceConnection(socket, user.Id, deviceId, directory.Relays, clock);
                connection.ExtendUntil(expires);
                directory.Register(connection);
                await RequireApprovedAsync(scopes, user.Id, deviceId, handshake.Token);
            }
            connection.Send(Wire.Ready);
            using var lifetime = CancellationTokenSource.CreateLinkedTokenSource(context.RequestAborted, connection.Stopped, connection.Expired);
            await ListenAsync(connection, reader, directory, scopes, metrics, clock, logger, lifetime.Token);
        }
        catch (Exception error) when (error is ApiException or JsonException or InvalidOperationException or KeyNotFoundException or FormatException
                                      or WebSocketException or OperationCanceledException or IOException or TimeoutException)
        {
            if (connection is not null) Ended(logger, error.GetType().Name);
            else
            {
                var reason = error is ApiException refusal ? refusal.Status.ToString(CultureInfo.InvariantCulture) : error.GetType().Name;
                metrics.Refused(reason);
                Refused(logger, user.Id, deviceId, reason, error is ApiException ? error.Message : "");
            }
        }
        finally
        {
            if (connection is not null)
            {
                directory.Remove(connection);
                await connection.DisposeAsync();
            }
            else socket.Abort();
        }
    }

    private static async Task<(string DeviceId, string? Session)> HelloAsync(WebSocket socket, MessageReader reader, CancellationToken ct)
    {
        if (await reader.ReadAsync(ControlBytes, ct) != WebSocketMessageType.Text) throw ApiException.Invalid("Expected device connection hello.");
        using var hello = JsonDocument.Parse(reader.Memory, new JsonDocumentOptions { MaxDepth = 32 });
        var root = hello.RootElement;
        if (root.GetProperty("type").GetString() != "hello") throw ApiException.Invalid("Expected device connection hello.");
        if (root.GetProperty("protocolVersion").GetInt32() is < ContractVersions.OldestTerminalConnection or > ContractVersions.TerminalConnection)
        {
            await socket.SendAsync(Wire.Encode(new RefusedNotice("version", ContractVersions.OldestTerminalConnection, ContractVersions.TerminalConnection)),
                WebSocketMessageType.Text, true, ct);
            throw ApiException.Invalid("Unsupported terminal connection protocol.");
        }
        return (DeviceIdRules.Require(root.GetProperty("deviceId").GetString()), root.GetProperty("deviceSession").GetString());
    }

    private static async Task RequireSessionAsync(WebSocket socket, DeviceSessions deviceSessions, Guid userId, string deviceId, string? secret,
        CancellationToken ct)
    {
        if (deviceSessions.Holds(userId, deviceId, secret)) return;
        await socket.SendAsync(Wire.Encode(new RefusedNotice("deviceSession")), WebSocketMessageType.Text, true, ct);
        throw new ApiException(428, "A device session is required.");
    }

    private static async Task ListenAsync(DeviceConnection connection, MessageReader reader, ConnectionDirectory directory, IServiceScopeFactory scopes,
        ServerMetrics metrics, TimeProvider clock, ILogger logger, CancellationToken ct)
    {
        var messages = new TokenBucket(MessageBurst, MessagesEachSecond, clock);
        var statuses = new TokenBucket(StatusBurst, StatusesEachSecond, clock);
        while (!ct.IsCancellationRequested)
        {
            var type = await reader.ReadAsync(MessageBytes, ct);
            if (type is null) return;
            connection.Heard();
            if (type == WebSocketMessageType.Binary)
            {
                if (reader.Length <= PipeIdBytes) throw ApiException.Invalid("Terminal channel message is too short.");
                var relayed = directory.Relay(connection, new Guid(reader.Message[..PipeIdBytes], bigEndian: true), reader.Message);
                if (relayed > 0) metrics.Relayed(relayed - PipeIdBytes);
                continue;
            }
            if (reader.Length > ControlBytes) throw ApiException.Invalid("Connection message is too large.");
            var message = Wire.Decode(reader.Message);
            if (!messages.TryTake(message is HostMessage or OpenMessage ? DatabaseCost : 1))
            {
                metrics.Refused("messages");
                Flooded(logger, connection.UserId, connection.DeviceId);
                return;
            }
            switch (message)
            {
                case PingMessage:
                    connection.Send(Wire.Pong);
                    break;
                case PongMessage:
                    break;
                case HostMessage host:
                {
                    var refusal = await AdmitAsync(scopes, ct, async (sessions, admission) =>
                    {
                        var session = await sessions.HostAsync(host.SessionId, connection.UserId, connection.DeviceId, admission);
                        SessionService.Incarnation(session, host.IncarnationId);
                        directory.Host(connection, session);
                    });
                    connection.Send(Wire.Encode(refusal is null
                        ? new HostedNotice(host.SessionId)
                        : new UnhostedNotice(host.SessionId, refusal.Status, refusal.Message)));
                    break;
                }
                case UnhostMessage unhost:
                    directory.Unhost(connection, unhost.SessionId);
                    break;
                case StatusMessage status:
                {
                    var reported = TerminalStatus.Checked(status.Status);
                    if (statuses.TryTake(1)) directory.Share(connection, status.SessionId, status.IncarnationId, reported);
                    break;
                }
                case OpenMessage open:
                {
                    var refusal = open.Pipe == Guid.Empty ? ApiException.Invalid("A view identity is required.") : await AdmitAsync(scopes, ct, async (sessions, admission) =>
                    {
                        directory.Open(connection, open.Pipe, await AuthorizedAsync(sessions, open, connection.UserId, admission));
                        try { await AuthorizedAsync(sessions, open, connection.UserId, admission); }
                        catch { directory.Close(connection, open.Pipe); throw; }
                    });
                    if (refusal is not null)
                    {
                        metrics.Refused(refusal.Status.ToString(CultureInfo.InvariantCulture));
                        connection.Send(Wire.Encode(new ClosedNotice(open.Pipe, refusal.Status, refusal.Message)));
                    }
                    break;
                }
                case CloseMessage close:
                    directory.Close(connection, close.Pipe);
                    break;
            }
        }
    }

    private static async Task<Session> AuthorizedAsync(SessionService sessions, OpenMessage open, Guid userId, CancellationToken ct)
    {
        var session = await sessions.AuthorizedAsync(open.SessionId, userId, ct);
        SessionService.Incarnation(session, open.IncarnationId);
        return session;
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
        catch (ApiException refusal)
        {
            return refusal;
        }
        catch (Exception error) when (!ct.IsCancellationRequested && (error is OperationCanceledException || Failures.IsDatabaseUnavailable(error)))
        {
            return new ApiException(503, "The server could not check this terminal; try again.");
        }
    }

    private static async Task RequireApprovedAsync(IServiceScopeFactory scopes, Guid userId, string deviceId, CancellationToken ct)
    {
        await using var scope = scopes.CreateAsyncScope();
        await scope.ServiceProvider.GetRequiredService<DeviceService>().RequireDeviceAsync(userId, deviceId, ct);
    }

    [LoggerMessage(Level = LogLevel.Debug, Message = "Device connection ended: {Failure}")]
    private static partial void Ended(ILogger logger, string failure);

    [LoggerMessage(Level = LogLevel.Information, Message = "Connection refused: account {Account}, device {Device}, reason {Reason}: {Detail}")]
    private static partial void Refused(ILogger logger, Guid account, string? device, string reason, string detail);

    [LoggerMessage(Level = LogLevel.Warning, Message = "Device connection closed for too many messages: account {Account}, device {Device}")]
    private static partial void Flooded(ILogger logger, Guid account, string device);
}

/// <summary>
/// The messages that one device connection can send: a burst, then a steady number each second. The control burst fits
/// a reconnect with the most terminals and views; a message that reads the database costs more.
/// </summary>
internal struct TokenBucket(double burst, double eachSecond, TimeProvider clock)
{
    private readonly double burst = burst;
    private readonly double eachSecond = eachSecond;
    private readonly TimeProvider clock = clock;
    private double tokens = burst;
    private long last = clock.GetTimestamp();

    public bool TryTake(int cost)
    {
        var now = clock.GetTimestamp();
        tokens = Math.Min(burst, tokens + clock.GetElapsedTime(last, now).TotalSeconds * eachSecond);
        last = now;
        if (tokens < cost) return false;
        tokens -= cost;
        return true;
    }
}
