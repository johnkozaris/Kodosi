using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Infrastructure.Realtime;

namespace Kodosi.HostTests;

public sealed class LiveSessionStateDirectoryEvictionTests
{
    [Fact]
    public void GetOrCreate_Returns_Ownership_Only_To_Atomic_Creator()
    {
        var directory = new LiveSessionStateDirectory();
        var sessionId = SessionId.New();

        var winner = directory.CreateRuntime(
            sessionId,
            out var winnerOwnership);
        var loser = directory.CreateRuntime(
            sessionId,
            out var loserOwnership);

        Assert.Same(winner, loser);
        Assert.NotNull(winnerOwnership);
        Assert.Null(loserOwnership);
        Assert.False(directory.RemoveIfOwned(
            sessionId,
            new LiveSessionCreationOwnership()));
        Assert.Same(winner, directory.TryGet(sessionId));
        Assert.True(directory.RemoveIfOwned(
            sessionId,
            winnerOwnership!));
        Assert.Null(directory.TryGet(sessionId));
    }

    [Fact]
    public void RemoveIfSame_Does_Not_Remove_Replacement_Runtime()
    {
        var directory = new LiveSessionStateDirectory();
        var sessionId = SessionId.New();
        var stale = directory.CreateRuntime(sessionId);
        Assert.NotEqual(Guid.Empty, stale.IncarnationId);
        Assert.True(directory.RemoveIfSame(sessionId, stale));
        var replacement = directory.CreateRuntime(sessionId);

        Assert.NotEqual(Guid.Empty, replacement.IncarnationId);
        Assert.NotEqual(stale.IncarnationId, replacement.IncarnationId);
        Assert.False(directory.RemoveIfSame(sessionId, stale));
        Assert.Same(replacement, directory.TryGet(sessionId));
        Assert.True(directory.RemoveIfSame(sessionId, replacement));
        Assert.Null(directory.TryGet(sessionId));
    }
}
