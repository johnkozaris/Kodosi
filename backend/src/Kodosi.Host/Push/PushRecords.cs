using Kodosi.Data;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Push;

internal sealed partial class PushRecords(IServiceScopeFactory scopes, ILogger<PushRecords> logger)
{
    public async Task<List<PushRecord>> LoadAsync(CancellationToken ct)
    {
        try
        {
            await using var scope = scopes.CreateAsyncScope();
            var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            return await (from push in db.TerminalPushes.AsNoTracking()
                          join session in db.Sessions on push.SessionId equals session.Id
                          where !session.Ended
                          select new PushRecord(session.OwnerUserId, push.SessionId,
                              new TerminalStatus(push.Program, push.State, push.Kind, push.Account, true), push.At))
                .ToListAsync(ct);
        }
        catch (Exception error) when (!ct.IsCancellationRequested)
        {
            NotLoaded(logger, error);
            return [];
        }
    }

    // Returns false only when the database is unreachable so the caller requeues these records.
    public async Task<bool> SaveAsync(IReadOnlyDictionary<Guid, PushRecord?> records, CancellationToken ct)
    {
        try
        {
            await using var scope = scopes.CreateAsyncScope();
            var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            var terminals = records.Keys.ToArray();
            var kept = records.Values.OfType<PushRecord>().ToList();
            await using var transaction = await db.Database.BeginTransactionAsync(ct);
            await db.TerminalPushes.Where(push => terminals.Contains(push.SessionId)).ExecuteDeleteAsync(ct);
            if (kept.Count > 0)
            {
                var pushed = kept.Select(record => record.TerminalId).ToArray();
                var live = await db.Sessions.Where(session => pushed.Contains(session.Id) && !session.Ended).Select(session => session.Id).ToListAsync(ct);
                db.TerminalPushes.AddRange(kept.Where(record => live.Contains(record.TerminalId)).Select(record => new TerminalPush
                {
                    SessionId = record.TerminalId,
                    Program = record.Status.Program,
                    State = record.Status.State ?? "",
                    Kind = record.Status.Kind,
                    Account = record.Status.Account,
                    At = record.At,
                }));
                await db.SaveChangesAsync(ct);
            }
            await transaction.CommitAsync(ct);
            return true;
        }
        catch (Exception error) when (!ct.IsCancellationRequested && Failures.IsDatabaseUnavailable(error))
        {
            Unreachable(logger, Failures.Describe(error));
            return false;
        }
        catch (Exception error) when (!ct.IsCancellationRequested)
        {
            NotSaved(logger, error);
            return true;
        }
    }

    [LoggerMessage(Level = LogLevel.Warning, Message = "The pushes from before the start are not known, so a terminal that still waits can get one more")]
    private static partial void NotLoaded(ILogger logger, Exception error);

    [LoggerMessage(Level = LogLevel.Warning, Message = "The pushes wait for the database: {Failure}")]
    private static partial void Unreachable(ILogger logger, string failure);

    [LoggerMessage(Level = LogLevel.Error, Message = "The pushes were not saved")]
    private static partial void NotSaved(ILogger logger, Exception error);
}
