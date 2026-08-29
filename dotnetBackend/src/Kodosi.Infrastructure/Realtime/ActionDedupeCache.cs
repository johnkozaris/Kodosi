using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Realtime;

public sealed class ActionDedupeCache(
    TimeProvider? timeProvider = null,
    int maxEntries = 50_000) : Kodosi.Application.IActionDedupeCache
{
    private readonly System.Threading.Lock _gate = new();
    private readonly Dictionary<DedupeKey, Entry> _seen = [];
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;
    private static readonly TimeSpan Ttl = TimeSpan.FromMinutes(5);
    private readonly int _maxEntries = maxEntries > 0
        ? maxEntries
        : throw new ArgumentOutOfRangeException(nameof(maxEntries));
    private long _nextLeaseId;

    private readonly record struct DedupeKey(Guid SessionId, Guid UserId, string ActionId);

    private sealed class Entry(
        long leaseId,
        DateTimeOffset seenAt,
        string? requestId)
    {
        public long LeaseId { get; } = leaseId;
        public DateTimeOffset SeenAt { get; set; } = seenAt;
        public bool AuditEmitted { get; set; }
        public string? RequestId { get; } = requestId;
        public TaskCompletionSource<ActionDedupeFinalOutcome> Completion { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
    }

    public ActionDedupeClaim Claim(
        SessionId sessionId,
        UserId userId,
        string actionId,
        string? requestId = null)
    {
        var now = _timeProvider.GetUtcNow();
        var key = new DedupeKey(sessionId.Value, userId.Value, actionId);

        lock (_gate)
        {
            if (_seen.TryGetValue(key, out var existing))
            {
                if (now - existing.SeenAt < Ttl)
                {
                    return new ActionDedupeClaim(
                        ActionDedupeClaimKind.Duplicate,
                        0,
                        existing.Completion.Task,
                        existing.RequestId);
                }

                _seen.Remove(key);
                existing.Completion.TrySetResult(ActionDedupeFinalOutcome.Busy);
            }

            if (_seen.Count >= _maxEntries)
            {
                return new ActionDedupeClaim(
                    ActionDedupeClaimKind.Saturated,
                    0,
                    null,
                    requestId);
            }

            var leaseId = ++_nextLeaseId;
            _seen.Add(key, new Entry(leaseId, now, requestId));
            return new ActionDedupeClaim(
                ActionDedupeClaimKind.New,
                leaseId,
                null,
                requestId);
        }
    }

    public void Complete(
        SessionId sessionId,
        UserId userId,
        string actionId,
        long leaseId,
        ActionDedupeFinalOutcome outcome)
    {
        var key = new DedupeKey(sessionId.Value, userId.Value, actionId);
        Entry? completed = null;
        lock (_gate)
        {
            if (!_seen.TryGetValue(key, out var entry) || entry.LeaseId != leaseId)
            {
                return;
            }

            completed = entry;
            if (outcome is ActionDedupeFinalOutcome.Accepted
                or ActionDedupeFinalOutcome.Duplicate)
            {
                entry.SeenAt = _timeProvider.GetUtcNow();
            }
            else
            {
                _seen.Remove(key);
            }
        }

        completed.Completion.TrySetResult(outcome);
    }

    public void CompleteCurrent(
        SessionId sessionId,
        UserId userId,
        string actionId,
        ActionDedupeFinalOutcome outcome)
    {
        var key = new DedupeKey(sessionId.Value, userId.Value, actionId);
        long leaseId;
        lock (_gate)
        {
            if (!_seen.TryGetValue(key, out var entry))
            {
                return;
            }
            leaseId = entry.LeaseId;
        }
        Complete(sessionId, userId, actionId, leaseId, outcome);
    }

    public bool TryClaimAuditSlot(SessionId sessionId, UserId userId, string actionId)
    {
        var key = new DedupeKey(sessionId.Value, userId.Value, actionId);
        lock (_gate)
        {
            if (!_seen.TryGetValue(key, out var existing))
            {
                return false;
            }

            if (existing.AuditEmitted)
            {
                return false;
            }

            existing.AuditEmitted = true;
            return true;
        }
    }

    public void Sweep()
    {
        var cutoff = _timeProvider.GetUtcNow() - Ttl;
        List<Entry> expired = [];
        lock (_gate)
        {
            foreach (var (key, entry) in _seen.ToList())
            {
                if (entry.SeenAt < cutoff)
                {
                    _seen.Remove(key);
                    expired.Add(entry);
                }
            }
        }

        foreach (var entry in expired)
        {
            entry.Completion.TrySetResult(ActionDedupeFinalOutcome.Busy);
        }
    }
}
