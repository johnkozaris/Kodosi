using System.Buffers;
using System.Collections.Concurrent;
using System.Text.Json.Serialization;
using System.Threading.Channels;
using Kodosi.Data;
using Kodosi.Sessions;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.TerminalConnections;

[JsonUnmappedMemberHandling(JsonUnmappedMemberHandling.Disallow)]
public sealed record TerminalStatus(string? Program = null, string? State = null, string? Kind = null)
{
    private static readonly SearchValues<char> ProgramCharacters = SearchValues.Create("abcdefghijklmnopqrstuvwxyz0123456789-");

    internal static TerminalStatus? Checked(TerminalStatus? status) =>
        status is null || status.IsValid ? status : throw ApiException.Invalid("Invalid terminal status.");

    private bool IsValid =>
        (Program is null || (Program.Length is > 0 and <= 32 && !Program.AsSpan().ContainsAnyExcept(ProgramCharacters)))
        && State is null or "idle" or "working" or "blocked" or "done" or "error"
        && (Kind is null || (State == "blocked" && Kind is "permission" or "question" or "auth"));
}

internal sealed partial class TerminalPresence(IServiceScopeFactory scopes, ConnectionDirectory directory, TimeProvider clock,
    ILogger<TerminalPresence> logger) : BackgroundService
{
    internal static readonly TimeSpan Spacing = TimeSpan.FromSeconds(1);
    private const int RememberedTerminals = 65_536;
    private readonly Channel<Guid> changed = Channel.CreateUnbounded<Guid>(new UnboundedChannelOptions { SingleReader = true });
    private readonly ConcurrentDictionary<Guid, byte> waiting = new();
    private readonly Dictionary<Guid, long> announced = [];

    private void Change(Guid sessionId)
    {
        if (waiting.TryAdd(sessionId, 0)) changed.Writer.TryWrite(sessionId);
    }

    private void Later(Guid sessionId, TimeSpan delay, CancellationToken ct) =>
        _ = Task.Delay(delay, clock, ct).ContinueWith(_ => changed.Writer.TryWrite(sessionId), CancellationToken.None,
            TaskContinuationOptions.OnlyOnRanToCompletion, TaskScheduler.Default);

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        directory.HostChanged += Change;
        try
        {
            await foreach (var sessionId in changed.Reader.ReadAllAsync(stoppingToken))
            {
                var now = clock.GetTimestamp();
                if (announced.TryGetValue(sessionId, out var last) && clock.GetElapsedTime(last, now) is var since && since < Spacing)
                {
                    Later(sessionId, Spacing - since, stoppingToken);
                    continue;
                }
                waiting.TryRemove(sessionId, out _);
                announced[sessionId] = now;
                if (announced.Count > RememberedTerminals) Forget(now);
                try
                {
                    await AnnounceAsync(sessionId, stoppingToken);
                }
                catch (Exception error) when (!stoppingToken.IsCancellationRequested)
                {
                    if (!Failures.IsDatabaseUnavailable(error)) Failed(logger, error);
                    else if (waiting.TryAdd(sessionId, 0)) Later(sessionId, Spacing, stoppingToken);
                }
            }
        }
        catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
        {
        }
        finally
        {
            directory.HostChanged -= Change;
        }
    }

    private async Task AnnounceAsync(Guid sessionId, CancellationToken ct)
    {
        await using var scope = scopes.CreateAsyncScope();
        var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
        var session = await db.Sessions.Where(x => x.Id == sessionId && !x.Ended)
            .Select(x => new { x.IncarnationId, x.OwnerUserId, x.HostDeviceId }).SingleOrDefaultAsync(ct);
        if (session is null) return;
        var people = await TerminalAccess.PeopleAsync(db, sessionId, ct);
        directory.NotifyTerminal(sessionId, session.IncarnationId, session.OwnerUserId, session.HostDeviceId, people);
    }

    private void Forget(long now)
    {
        foreach (var (sessionId, at) in announced.ToArray())
            if (clock.GetElapsedTime(at, now) >= Spacing) announced.Remove(sessionId);
    }

    [LoggerMessage(Level = LogLevel.Error, Message = "A terminal announcement failed")]
    private static partial void Failed(ILogger logger, Exception error);
}
