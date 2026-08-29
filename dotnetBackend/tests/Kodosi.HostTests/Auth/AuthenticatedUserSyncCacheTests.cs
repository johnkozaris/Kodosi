using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Middleware;

namespace Kodosi.HostTests;

public sealed class AuthenticatedUserSyncCacheTests
{
    [Fact]
    public void Delimiter_Aliased_Identities_Do_Not_Share_Cache_Entries()
    {
        var cache = new AuthenticatedUserSyncCache();
        var now = DateTimeOffset.UtcNow;
        var cachedUserId = UserId.New();
        var first = Profile(new AuthenticatedExternalIdentity("a", "b", "c\nd"));
        var formerlyAliased = Profile(new AuthenticatedExternalIdentity("a\nb", "c", "d"));
        cache.RecordSync(first, cachedUserId, "first", now, TimeSpan.FromMinutes(1));

        Assert.False(cache.TryGetFresh(
            formerlyAliased,
            TimeSpan.FromMinutes(1),
            now,
            out _));
    }

    [Fact]
    public void Expired_Entries_Are_Removed()
    {
        var cache = new AuthenticatedUserSyncCache(maxEntries: 2);
        var profile = Profile("one");
        var now = DateTimeOffset.UtcNow;
        cache.RecordSync(profile, UserId.New(), "one", now, TimeSpan.FromMinutes(1));

        Assert.False(cache.TryGetFresh(
            profile,
            TimeSpan.FromMinutes(1),
            now.AddMinutes(2),
            out _));
        Assert.Equal(0, cache.Count);
    }

    [Fact]
    public void RecordSync_Evicts_Oldest_Entry_At_Capacity()
    {
        var cache = new AuthenticatedUserSyncCache(maxEntries: 2);
        var now = DateTimeOffset.UtcNow;
        cache.RecordSync(Profile("one"), UserId.New(), "one", now, TimeSpan.FromHours(1));
        cache.RecordSync(Profile("two"), UserId.New(), "two", now.AddSeconds(1), TimeSpan.FromHours(1));
        cache.RecordSync(Profile("three"), UserId.New(), "three", now.AddSeconds(2), TimeSpan.FromHours(1));

        Assert.Equal(2, cache.Count);
        Assert.False(cache.TryGetFresh(
            Profile("one"),
            TimeSpan.FromHours(1),
            now.AddSeconds(3),
            out _));
    }

    [Fact]
    public async Task ProvisioningLock_Serializes_One_Identity_And_Releases_Key()
    {
        var keyedLock = new AuthenticatedUserProvisioningLock();
        var active = 0;
        var maxActive = 0;

        var tasks = Enumerable.Range(0, 8).Select(async _ =>
        {
            await using var lease = await keyedLock.AcquireAsync(
                ["provider\nissuer\nsubject"],
                TestContext.Current.CancellationToken);
            var current = Interlocked.Increment(ref active);
            InterlockedExtensions.Max(ref maxActive, current);
            await Task.Delay(5, TestContext.Current.CancellationToken);
            Interlocked.Decrement(ref active);
        });

        await Task.WhenAll(tasks);
        Assert.Equal(1, maxActive);
        await using var reacquired = await keyedLock.AcquireAsync(
            ["provider\nissuer\nsubject"],
            TestContext.Current.CancellationToken);
    }

    [Fact]
    public async Task ProvisioningLock_Serializes_Overlapping_Identity_Sets()
    {
        var keyedLock = new AuthenticatedUserProvisioningLock();
        var firstEntered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var releaseFirst = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var secondEntered = false;

        var first = Task.Run(async () =>
        {
            await using var lease = await keyedLock.AcquireAsync(
                ["provider\nissuer\nprimary-a", "provider\nissuer\nshared"],
                TestContext.Current.CancellationToken);
            firstEntered.SetResult();
            await releaseFirst.Task;
        }, TestContext.Current.CancellationToken);
        await firstEntered.Task;
        var second = Task.Run(async () =>
        {
            await using var lease = await keyedLock.AcquireAsync(
                ["provider\nissuer\nprimary-b", "provider\nissuer\nshared"],
                TestContext.Current.CancellationToken);
            secondEntered = true;
        }, TestContext.Current.CancellationToken);

        await Task.Delay(20, TestContext.Current.CancellationToken);
        Assert.False(secondEntered);
        releaseFirst.SetResult();
        await Task.WhenAll(first, second);
        Assert.True(secondEntered);
        await using var reacquired = await keyedLock.AcquireAsync(
            ["provider\nissuer\nshared"],
            TestContext.Current.CancellationToken);
    }

    private static AuthenticatedUserProfile Profile(string subject) =>
        Profile(new AuthenticatedExternalIdentity("test", "https://issuer.test", subject));

    private static AuthenticatedUserProfile Profile(AuthenticatedExternalIdentity identity) =>
        new(
            identity,
            [identity],
            identity.Subject,
            $"{identity.Subject}@example.test",
            identity.Subject,
            null);

    private static class InterlockedExtensions
    {
        public static void Max(ref int target, int value)
        {
            var current = Volatile.Read(ref target);
            while (value > current)
            {
                var observed = Interlocked.CompareExchange(ref target, value, current);
                if (observed == current) return;
                current = observed;
            }
        }
    }
}
