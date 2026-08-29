using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Infrastructure.Realtime;

namespace Kodosi.HostTests;

public sealed class ActionDedupeCacheTests
{
    [Fact]
    public async Task Claim_Allows_Only_One_Concurrent_First_Writer()
    {
        const string actionId = "action-123";
        var sessionId = SessionId.New();
        var userId = UserId.New();
        var cache = new ActionDedupeCache();
        var gate = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);

        var tasks = Enumerable.Range(0, 16)
            .Select(_ => Task.Run(async () =>
            {
                await gate.Task;
                return cache.Claim(sessionId, userId, actionId);
            }))
            .ToArray();

        gate.SetResult();
        var results = await Task.WhenAll(tasks);

        Assert.Single(results, claim => claim.Kind == ActionDedupeClaimKind.New);
        Assert.Equal(
            results.Length - 1,
            results.Count(claim => claim.Kind == ActionDedupeClaimKind.Duplicate));
    }

    [Fact]
    public void TryClaimAuditSlot_Returns_True_Once_Per_ActionId()
    {
        const string actionId = "action-456";
        var sessionId = SessionId.New();
        var userId = UserId.New();
        var cache = new ActionDedupeCache();

        Assert.Equal(
            ActionDedupeClaimKind.New,
            cache.Claim(sessionId, userId, actionId).Kind);
        Assert.Equal(
            ActionDedupeClaimKind.Duplicate,
            cache.Claim(sessionId, userId, actionId).Kind);
        Assert.True(cache.TryClaimAuditSlot(sessionId, userId, actionId));
        Assert.False(cache.TryClaimAuditSlot(sessionId, userId, actionId));
    }

    [Fact]
    public void TryClaimAuditSlot_Returns_False_When_ActionId_Unknown()
    {
        var cache = new ActionDedupeCache();
        Assert.False(cache.TryClaimAuditSlot(SessionId.New(), UserId.New(), "never-seen"));
    }

    [Fact]
    public async Task TryClaimAuditSlot_Is_Atomic_Under_Contention()
    {
        const string actionId = "action-789";
        var sessionId = SessionId.New();
        var userId = UserId.New();
        var cache = new ActionDedupeCache();
        _ = cache.Claim(sessionId, userId, actionId);

        var tasks = Enumerable.Range(0, 32)
            .Select(_ => Task.Run(() =>
                cache.TryClaimAuditSlot(sessionId, userId, actionId)))
            .ToArray();

        var results = await Task.WhenAll(tasks);
        Assert.Single(results, claimed => claimed);
    }

    [Fact]
    public void Claim_Scopes_ActionId_By_Session_And_User()
    {
        const string actionId = "action-shared";
        var sessionId = SessionId.New();
        var otherSessionId = SessionId.New();
        var userId = UserId.New();
        var otherUserId = UserId.New();
        var cache = new ActionDedupeCache();

        Assert.Equal(ActionDedupeClaimKind.New, cache.Claim(sessionId, userId, actionId).Kind);
        Assert.Equal(ActionDedupeClaimKind.Duplicate, cache.Claim(sessionId, userId, actionId).Kind);
        Assert.Equal(ActionDedupeClaimKind.New, cache.Claim(otherSessionId, userId, actionId).Kind);
        Assert.Equal(ActionDedupeClaimKind.New, cache.Claim(sessionId, otherUserId, actionId).Kind);
    }

    [Fact]
    public void Claim_Returns_Saturated_Without_Overshooting_Capacity()
    {
        var cache = new ActionDedupeCache(maxEntries: 2);
        var sessionId = SessionId.New();
        var userId = UserId.New();

        Assert.Equal(ActionDedupeClaimKind.New, cache.Claim(sessionId, userId, "action-1").Kind);
        Assert.Equal(ActionDedupeClaimKind.New, cache.Claim(sessionId, userId, "action-2").Kind);
        Assert.Equal(ActionDedupeClaimKind.Saturated, cache.Claim(sessionId, userId, "action-3").Kind);
    }

    [Fact]
    public async Task Duplicate_Replays_Original_Final_Outcome()
    {
        var cache = new ActionDedupeCache();
        var sessionId = SessionId.New();
        var userId = UserId.New();
        const string actionId = "action-final";
        var first = cache.Claim(sessionId, userId, actionId, "request-original");
        var duplicate = cache.Claim(sessionId, userId, actionId, "request-retry");

        cache.Complete(
            sessionId,
            userId,
            actionId,
            first.LeaseId,
            ActionDedupeFinalOutcome.Accepted);

        Assert.Equal(
            ActionDedupeFinalOutcome.Accepted,
            await duplicate.OriginalOutcome!);
        Assert.Equal("request-original", duplicate.OriginalRequestId);
    }

    [Theory]
    [InlineData(ActionDedupeFinalOutcome.Busy)]
    [InlineData(ActionDedupeFinalOutcome.Rejected)]
    public void Failed_Outcomes_Do_Not_Permanently_Reserve_Action(
        ActionDedupeFinalOutcome outcome)
    {
        var cache = new ActionDedupeCache();
        var sessionId = SessionId.New();
        var userId = UserId.New();
        const string actionId = "action-retry";
        var first = cache.Claim(sessionId, userId, actionId);

        cache.Complete(sessionId, userId, actionId, first.LeaseId, outcome);

        Assert.Equal(
            ActionDedupeClaimKind.New,
            cache.Claim(sessionId, userId, actionId).Kind);
    }
}
