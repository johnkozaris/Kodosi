using System.Collections.Concurrent;
using System.Text.Json;
using System.Threading.Channels;
using Kodosi.Sessions;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.TerminalConnections;

public sealed record TerminalStatus(long KeyVersion, long Epoch, string Nonce, string Ciphertext)
{
    internal static TerminalStatus? Read(JsonElement value)
    {
        if (value.ValueKind == JsonValueKind.Null) return null;
        var status = value.Deserialize<TerminalStatus>(Wire.Json) ?? throw ApiException.Invalid("Invalid terminal status.");
        if (status.KeyVersion < 1 || status.Epoch < 1
            || Limits.Base64(status.Nonce, "Terminal status nonce", 12).Length != 12
            || Limits.Base64(status.Ciphertext, "Terminal status", 1024).Length < 16)
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
