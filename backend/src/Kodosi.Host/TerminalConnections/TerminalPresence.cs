using System.Collections.Concurrent;
using System.Text.Json;
using System.Threading.Channels;
using Kodosi.Sessions;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.TerminalConnections;

// The agent of a terminal and the state that its program reports, never its title or text.
public sealed record TerminalStatus(string? Program, string? State, string? Kind)
{
    private static readonly string[] States = ["idle", "working", "blocked", "done", "error"];
    private static readonly string[] Kinds = ["permission", "question", "auth"];

    internal static TerminalStatus? Read(JsonElement value)
    {
        if (value.ValueKind == JsonValueKind.Null) return null;
        var status = value.Deserialize<TerminalStatus>(Wire.Json) ?? throw ApiException.Invalid("Invalid terminal status.");
        if (status.Program is { } program && !(program.Length is > 0 and <= 32 && program.All(c => char.IsAsciiLetterLower(c) || char.IsAsciiDigit(c) || c == '-'))
            || status.State is { } state && !States.Contains(state)
            || status.Kind is { } kind && (status.State != "blocked" || !Kinds.Contains(kind)))
            throw ApiException.Invalid("Invalid terminal status.");
        return status;
    }
}

internal sealed class TerminalPresence(IServiceScopeFactory scopes, ConnectionDirectory directory, ILogger<TerminalPresence> logger)
    : BackgroundService
{
    private readonly Channel<Guid> changed = Channel.CreateUnbounded<Guid>(new UnboundedChannelOptions { SingleReader = true });
    private readonly ConcurrentDictionary<Guid, byte> waiting = new();

    private void Change(Guid sessionId)
    {
        if (waiting.TryAdd(sessionId, 0)) changed.Writer.TryWrite(sessionId);
    }

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        directory.HostChanged += Change;
        try
        {
            await foreach (var sessionId in changed.Reader.ReadAllAsync(stoppingToken))
            {
                waiting.TryRemove(sessionId, out _);
                try
                {
                    await using var scope = scopes.CreateAsyncScope();
                    await scope.ServiceProvider.GetRequiredService<SessionService>().AnnounceAsync(sessionId, stoppingToken);
                }
                catch (Exception error) when (error is DbUpdateException or Npgsql.NpgsqlException or TimeoutException
                                              or InvalidOperationException { InnerException: Npgsql.NpgsqlException or TimeoutException })
                {
                    logger.LogDebug("A terminal announcement waits for the database: {Failure}", error.GetType().Name);
                    _ = Task.Delay(TimeSpan.FromSeconds(1), stoppingToken)
                        .ContinueWith(_ => Change(sessionId), CancellationToken.None, TaskContinuationOptions.OnlyOnRanToCompletion, TaskScheduler.Default);
                }
            }
        }
        finally { directory.HostChanged -= Change; }
    }
}
