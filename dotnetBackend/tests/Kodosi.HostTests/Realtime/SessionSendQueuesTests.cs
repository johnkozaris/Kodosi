using System.Text;
using Kodosi.Domain;
using Kodosi.Host.Realtime;
using Kodosi.Host.Serialization;

namespace Kodosi.HostTests;

public class SessionSendQueuesTests
{
    [Fact]
    public async Task SetHostQueue_CompletesPreviousQueue()
    {
        var queues = new SessionSendQueues();
        var first = queues.SetHostQueue();

        _ = queues.SetHostQueue();

        Assert.Null(await first.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public void ClearHostQueue_IgnoresNonOwnerConnection()
    {
        var queues = new SessionSendQueues();
        var current = queues.SetHostQueue("host-2");

        queues.ClearHostQueue("host-1");

        Assert.Same(current, queues.HostQueue);
        Assert.Equal(ChannelByteSendQueueWriteOutcome.Enqueued, current.TryEnqueue([0x01]));
    }

    [Fact]
    public async Task AddParticipantQueue_CompletesPreviousQueue_ForSameConnection()
    {
        var queues = new SessionSendQueues();
        var first = queues.AddParticipantQueue("viewer-1");

        _ = queues.AddParticipantQueue("viewer-1");

        Assert.Null(await first.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task Participant_Control_Enqueue_Requires_Exact_Current_Queue()
    {
        var queues = new SessionSendQueues();
        var stale = queues.AddParticipantQueue("viewer-1");
        var current = queues.AddParticipantQueue("viewer-1");
        var message = WireMessage.Json(Encoding.UTF8.GetBytes("""{"type":"receipt"}"""));

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Closed,
            queues.TryEnqueueParticipantControlIfSame("viewer-1", stale, message));
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queues.TryEnqueueParticipantControlIfSame("viewer-1", current, message));

        Assert.Null(await stale.ReadAsync(TestContext.Current.CancellationToken));
        Assert.Equal(message, await current.ReadAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task CloseAll_Detaches_Participants_Before_Allowing_Replacement()
    {
        var queues = new SessionSendQueues();
        var userId = UserId.New();
        const string deviceId = "device-1";
        using var replacementPrepareStarted = new ManualResetEventSlim();
        using var allowReplacementAttach = new ManualResetEventSlim();
        var original = queues.AddPreparedParticipantQueue(
            "viewer-1",
            AccessLevel.Approve,
            static _ => { },
            semanticReceiptDestination: new(
                userId,
                deviceId));
        var replacementTask = Task.Run(() => queues.AddPreparedParticipantQueue(
            "viewer-1",
            AccessLevel.Approve,
            prepareQueue: _ =>
            {
                replacementPrepareStarted.Set();
                allowReplacementAttach.Wait(TestContext.Current.CancellationToken);
            },
            semanticReceiptDestination: new(
                userId,
                deviceId)));
        Assert.True(replacementPrepareStarted.Wait(
            TimeSpan.FromSeconds(1),
            TestContext.Current.CancellationToken));

        var closeTask = Task.Run(
            () => queues.CloseAll(),
            TestContext.Current.CancellationToken);
        await Task.Delay(25, TestContext.Current.CancellationToken);
        Assert.False(closeTask.IsCompleted);

        allowReplacementAttach.Set();
        var replacement = await replacementTask.WaitAsync(
            TestContext.Current.CancellationToken);
        await closeTask.WaitAsync(TestContext.Current.CancellationToken);

        Assert.Null(queues.GetParticipantQueue("viewer-1"));
        Assert.Empty(queues.GetSemanticReceiptParticipants(userId, deviceId));
        Assert.Equal(0, queues.ParticipantQueueCount);
        Assert.Null(await original.ReadAsync(TestContext.Current.CancellationToken));
        Assert.Null(await replacement.ReadAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public void SemanticReceiptEnumeration_Captures_Exact_Registered_Queue()
    {
        var queues = new SessionSendQueues();
        var userId = UserId.New();
        const string deviceId = "device-1";
        var queue = queues.AddPreparedParticipantQueue(
            "viewer-1",
            AccessLevel.Approve,
            static _ => { },
            semanticReceiptDestination: new(
                userId,
                deviceId));

        var participant = Assert.Single(
            queues.GetSemanticReceiptParticipants(userId, deviceId));

        Assert.Equal("viewer-1", participant.ConnectionId);
        Assert.Same(queue, participant.Queue);
    }

    [Fact]
    public async Task AddPreparedParticipantQueue_Attaches_Only_After_Startup_Preamble_Is_Queued()
    {
        var queues = new SessionSendQueues();
        using var prepareStarted = new ManualResetEventSlim();
        using var allowAttach = new ManualResetEventSlim();
        var accepted = WireMessage.Json(Encoding.UTF8.GetBytes("""{"type":"participant.accepted"}"""));

        var attachTask = Task.Run(() => queues.AddPreparedParticipantQueue(
            "viewer-1",
            AccessLevel.View,
            prepareQueue: queue =>
            {
                prepareStarted.Set();
                allowAttach.Wait(TestContext.Current.CancellationToken);
                Assert.Equal(RelayClientSendQueueWriteOutcome.Enqueued, queue.TryEnqueueControl(accepted));
            }));

        Assert.True(prepareStarted.Wait(TimeSpan.FromSeconds(1), TestContext.Current.CancellationToken));
        Assert.Null(queues.GetParticipantQueue("viewer-1"));
        var snapshotTask = Task.Run(queues.GetParticipantQueueSnapshot);
        await Task.Delay(25, TestContext.Current.CancellationToken);
        Assert.False(snapshotTask.IsCompleted);

        allowAttach.Set();
        var queue = await attachTask;
        var snapshot = await snapshotTask;

        Assert.Same(queue, queues.GetParticipantQueue("viewer-1"));
        Assert.Same(queue, Assert.Single(snapshot).Value);
        Assert.Equal(accepted, await queue.ReadAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task AddPreparedParticipantQueue_Converts_Missed_End_Broadcast_To_Terminal_Message()
    {
        var queues = new SessionSendQueues();
        var accepted = WireMessage.Json(Encoding.UTF8.GetBytes("""{"type":"participant.accepted"}"""));
        var ended = WireMessage.Json(Encoding.UTF8.GetBytes("""{"type":"session.ended","reason":"host_stopped"}"""));

        queues.MarkSessionEnded(CloseReason.HostStopped);
        var queue = queues.AddPreparedParticipantQueue(
            "viewer-1",
            AccessLevel.View,
            prepareQueue: prepared =>
            {
                Assert.Equal(RelayClientSendQueueWriteOutcome.Enqueued, prepared.TryEnqueueControl(accepted));
            },
            closeReason =>
            {
                Assert.Equal(CloseReason.HostStopped, closeReason);
                return ended;
            });

        Assert.Equal(QueueCompletionCause.SessionEnd, queue.CompletionCause);
        Assert.Equal(CloseReason.HostStopped, queue.CompletionCloseReason);
        Assert.Equal(ended, await queue.ReadAsync(TestContext.Current.CancellationToken));
        Assert.Null(await queue.ReadAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task SemanticReceiptAdmission_Merges_Replay_And_Live_By_RequestId()
    {
        var queue = new RelayClientSendQueue(AccessLevel.Approve);
        var requestId = Guid.CreateVersion7();
        var replay = new RelayClientSendQueue.SemanticReceiptDelivery(
            requestId,
            WireMessage.Json("replay"u8.ToArray()));
        var live = new RelayClientSendQueue.SemanticReceiptDelivery(
            requestId,
            WireMessage.Json("live"u8.ToArray()));
        Assert.True(queue.BeginSemanticReceiptAdmission());

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueSemanticReceipt(live.RequestId, live.Message));
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.CompleteSemanticReceiptAdmission([replay]));

        var delivered = await queue.ReadAsync(TestContext.Current.CancellationToken);
        Assert.NotNull(delivered);
        Assert.Equal(replay.Message.Kind, delivered.Value.Kind);
        Assert.Equal(replay.Message.Payload, delivered.Value.Payload);
        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => queue.ReadAsync(timeout.Token));
    }

    [Fact]
    public async Task SemanticReceiptAdmission_Is_Bounded_And_Closes_On_Overflow()
    {
        var queue = new RelayClientSendQueue(AccessLevel.Approve);
        Assert.True(queue.BeginSemanticReceiptAdmission());

        RelayClientSendQueueWriteOutcome outcome = RelayClientSendQueueWriteOutcome.Enqueued;
        for (var index = 0; index <= 16; index++)
        {
            outcome = queue.TryEnqueueSemanticReceipt(
                Guid.CreateVersion7(),
                WireMessage.Json([(byte)index]));
        }

        Assert.Equal(RelayClientSendQueueWriteOutcome.Overflow, outcome);
        Assert.Equal(QueueCompletionCause.Lagging, queue.CompletionCause);
        Assert.Null(await queue.ReadAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public void SemanticReceiptAdmission_Does_Not_Drain_After_Terminal_Close()
    {
        var queue = new RelayClientSendQueue(AccessLevel.Approve);
        Assert.True(queue.BeginSemanticReceiptAdmission());
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueSemanticReceipt(
                Guid.CreateVersion7(),
                WireMessage.Json("live"u8.ToArray())));

        queue.Complete(
            QueueCompletionCause.SessionEnd,
            discardPending: true,
            closeReason: CloseReason.SessionEnded);

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Closed,
            queue.CompleteSemanticReceiptAdmission([]));
        Assert.Equal(CloseReason.SessionEnded, queue.CompletionCloseReason);
    }

}
