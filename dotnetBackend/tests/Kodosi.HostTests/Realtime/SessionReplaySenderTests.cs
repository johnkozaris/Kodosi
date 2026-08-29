using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Realtime;

namespace Kodosi.HostTests;

public sealed class SessionReplaySenderTests
{
    [Fact]
    public async Task Replay_Queues_Current_Terminal_And_Eligible_Pending_Snapshot()
    {
        var runtimes = new LiveSessionStateDirectory();
        var sender = new SessionReplaySender(new OperationalMetrics(runtimes));
        var incarnation = Guid.NewGuid();
        var replay = new LiveSessionReplayState(
            new LiveTerminalCheckpointReplay(2, 0, [0x03, 1]),
            [],
            new LiveTerminalPresentationReplay(3, [0x05, 1]),
            null,
            1,
            new LivePendingPermissionsReplay(incarnation, 4, [0x06, 1]));
        var queue = new RelayClientSendQueue(AccessLevel.Suggest);

        var outcome = sender.QueueReplay(replay, queue, 0, 0, 0, 0);

        Assert.Equal(SessionReplayQueueOutcome.Complete, outcome);
        Assert.Equal(0x03, (await queue.ReadAsync(TestContext.Current.CancellationToken))!.Value.Payload[0]);
        Assert.Equal(0x05, (await queue.ReadAsync(TestContext.Current.CancellationToken))!.Value.Payload[0]);
        Assert.Equal(0x06, (await queue.ReadAsync(TestContext.Current.CancellationToken))!.Value.Payload[0]);
    }

    [Fact]
    public async Task Replay_Does_Not_Expose_Pending_Snapshot_To_View_Only()
    {
        var runtimes = new LiveSessionStateDirectory();
        var sender = new SessionReplaySender(new OperationalMetrics(runtimes));
        var replay = new LiveSessionReplayState(
            new LiveTerminalCheckpointReplay(1, 0, [0x03]),
            [],
            null,
            null,
            1,
            new LivePendingPermissionsReplay(Guid.NewGuid(), 1, [0x06]));
        var queue = new RelayClientSendQueue(AccessLevel.View);

        Assert.Equal(
            SessionReplayQueueOutcome.Complete,
            sender.QueueReplay(replay, queue, 0, 0, 0, 0));
        Assert.Equal(0x03, (await queue.ReadAsync(TestContext.Current.CancellationToken))!.Value.Payload[0]);
        queue.Complete();
        Assert.Null(await queue.ReadAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task Owner_Receives_Pending_Snapshot_Regardless_Of_Granted_Role()
    {
        var runtimes = new LiveSessionStateDirectory();
        var sender = new SessionReplaySender(new OperationalMetrics(runtimes));
        var replay = new LiveSessionReplayState(
            null,
            [],
            null,
            null,
            1,
            new LivePendingPermissionsReplay(Guid.NewGuid(), 1, [0x06]));
        var queue = new RelayClientSendQueue(AccessLevel.View);
        queue.MarkOwnerParticipant();

        Assert.Equal(
            SessionReplayQueueOutcome.Complete,
            sender.QueueReplay(replay, queue, 0, 0, 0, 0));
        Assert.Equal(0x06, (await queue.ReadAsync(TestContext.Current.CancellationToken))!.Value.Payload[0]);
    }
}
