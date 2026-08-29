using Kodosi.Domain;
using Kodosi.Host.Realtime;

namespace Kodosi.HostTests;

public class RelayClientSendQueueTests
{
    [Fact]
    public void TryEnqueueFrame_ReturnsOverflow_WhenBacklogIsExceeded()
    {
        var queue = new RelayClientSendQueue();

        for (var i = 0; i < 64; i++)
        {
            Assert.Equal(
                RelayClientSendQueueWriteOutcome.Enqueued,
                queue.TryEnqueueFrame(WireMessage.EncryptedBinary(new byte[] { (byte)i })));
        }

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Overflow,
            queue.TryEnqueueFrame(WireMessage.EncryptedBinary(new byte[] { 255 })));
    }

    [Fact]
    public void TryEnqueueFrame_ReturnsDisconnected_WhenViewerKeepsLaggingDuringResync()
    {
        var queue = new RelayClientSendQueue();

        for (var i = 0; i < 64; i++)
        {
            Assert.Equal(
                RelayClientSendQueueWriteOutcome.Enqueued,
                queue.TryEnqueueFrame(WireMessage.EncryptedBinary(new byte[] { (byte)i })));
        }

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryQueueTerminalReplay([WireMessage.EncryptedBinary(new byte[] { 9 })], beginResync: true));

        for (var i = 0; i < 64; i++)
        {
            Assert.Equal(
                RelayClientSendQueueWriteOutcome.Enqueued,
                queue.TryEnqueueFrame(WireMessage.EncryptedBinary(new byte[] { (byte)i })));
        }

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Disconnected,
            queue.TryEnqueueFrame(WireMessage.EncryptedBinary(new byte[] { 255 })));
    }

    [Fact]
    public async Task Successful_Control_Enqueue_Does_Not_Complete_Connection()
    {
        var queue = new RelayClientSendQueue();
        var accepted = WireMessage.Json([1, 2, 3]);

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueControl(accepted));
        Assert.False(queue.CompletionToken.IsCancellationRequested);
        Assert.Null(queue.CompletionCause);
        Assert.Equal(
            accepted.Payload,
            (await queue.ReadAsync(CancellationToken.None))?.Payload);
        Assert.False(queue.CompletionToken.IsCancellationRequested);
    }

    [Fact]
    public async Task Control_Overflow_Completes_Queue_With_Lagging_Cause()
    {
        var queue = new RelayClientSendQueue();

        for (var i = 0; i < 16; i++)
        {
            Assert.Equal(
                RelayClientSendQueueWriteOutcome.Enqueued,
                queue.TryEnqueueControl(WireMessage.Json(new byte[] { (byte)i })));
        }

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Overflow,
            queue.TryEnqueueControl(WireMessage.Json(new byte[] { 255 })));
        Assert.Equal(QueueCompletionCause.Lagging, queue.CompletionCause);
        Assert.Null(await queue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task CompleteWithControlMessage_Delivers_Final_Message_Before_Close()
    {
        var queue = new RelayClientSendQueue();
        var final = WireMessage.Json(new byte[] { 7 });

        Assert.True(queue.TryCompleteWithControlMessage(final, QueueCompletionCause.AccessRevokedCascade));

        var dequeued = await queue.ReadAsync(CancellationToken.None);
        Assert.NotNull(dequeued);
        Assert.Equal(final.Payload, dequeued.Value.Payload);
        Assert.Null(await queue.ReadAsync(CancellationToken.None));
        Assert.Equal(QueueCompletionCause.AccessRevokedCascade, queue.CompletionCause);
    }

    [Fact]
    public async Task Frame_Lane_Accepts_One_Maximum_Legal_Terminal_Frame()
    {
        var queue = new RelayClientSendQueue();
        var frame = WireMessage.EncryptedBinary(
            new byte[RelayMessageLimits.TerminalCheckpointFrameMaxBytes]);

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueFrame(frame));
        Assert.Equal(frame.Payload, (await queue.ReadAsync(CancellationToken.None))?.Payload);
    }

    [Fact]
    public void Frame_Lane_Rejects_One_Byte_Above_Terminal_Contract()
    {
        var queue = new RelayClientSendQueue();
        var oversized = WireMessage.EncryptedBinary(
            new byte[RelayMessageLimits.TerminalCheckpointFrameMaxBytes + 1]);

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Overflow,
            queue.TryEnqueueFrame(oversized));
    }

    [Fact]
    public async Task Terminal_Replay_Admits_Valid_Checkpoint_Raw_Tail_And_Presentation_Atomically()
    {
        var queue = new RelayClientSendQueue();
        var messages = new List<WireMessage>
        {
            WireMessage.EncryptedBinary(
                new byte[RelayMessageLimits.TerminalCheckpointFrameMaxBytes]),
        };
        for (var index = 0; index < 4; index++)
        {
            messages.Add(WireMessage.EncryptedBinary(
                new byte[RelayMessageLimits.OuterMessageMaxBytes]));
        }
        messages.Add(WireMessage.EncryptedBinary(
            new byte[RelayMessageLimits.TerminalPresentationFrameMaxBytes]));

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryQueueTerminalReplay(messages));

        foreach (var expected in messages)
        {
            Assert.Equal(
                expected.Payload.Length,
                (await queue.ReadAsync(CancellationToken.None))?.Payload.Length);
        }
    }

    [Fact]
    public async Task Terminal_Replay_Preserves_Fifo_With_Normal_Control()
    {
        var queue = new RelayClientSendQueue();
        var accepted = WireMessage.Json([1]);
        var checkpoint = WireMessage.EncryptedBinary([2]);
        var raw = WireMessage.EncryptedBinary([3]);
        var status = WireMessage.Json([4]);

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueControl(accepted));
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryQueueTerminalReplay([checkpoint, raw]));
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueControl(status));

        Assert.Equal(accepted, await queue.ReadAsync(CancellationToken.None));
        Assert.Equal(checkpoint, await queue.ReadAsync(CancellationToken.None));
        Assert.Equal(raw, await queue.ReadAsync(CancellationToken.None));
        Assert.Equal(status, await queue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task Resync_Lane_Accepts_One_Maximum_Legal_Terminal_Frame()
    {
        var queue = new RelayClientSendQueue();
        var snapshot = WireMessage.EncryptedBinary(
            new byte[RelayMessageLimits.TerminalCheckpointFrameMaxBytes]);

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryQueueTerminalReplay([snapshot], beginResync: true));
        Assert.Equal(snapshot.Payload, (await queue.ReadAsync(CancellationToken.None))?.Payload);
    }

    [Fact]
    public async Task Control_Byte_Budget_Bounds_Large_Control_Backlog()
    {
        var queue = new RelayClientSendQueue();
        var payload = new byte[RelayMessageLimits.GetMaxBytes("term.semanticCheckpoint")];

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueControl(WireMessage.Json(payload)));

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Overflow,
            queue.TryEnqueueControl(WireMessage.Json([1])));
        Assert.Equal(QueueCompletionCause.Lagging, queue.CompletionCause);
        Assert.Null(await queue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task Control_Lane_Accepts_One_Maximum_Valid_KeyRotation()
    {
        var queue = new RelayClientSendQueue();
        var keyRotation = WireMessage.Json(
            new byte[RelayMessageLimits.GetMaxBytes("key.rotation")]);

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueControl(keyRotation));
        Assert.False(queue.CompletionToken.IsCancellationRequested);
        Assert.Equal(
            keyRotation.Payload,
            (await queue.ReadAsync(CancellationToken.None))?.Payload);
    }

    [Fact]
    public async Task Terminal_Message_Preempts_Full_Byte_Budgets()
    {
        var queue = new RelayClientSendQueue();
        var payload = new byte[2 * 1024 * 1024];
        for (var index = 0; index < 4; index++)
        {
            Assert.Equal(
                RelayClientSendQueueWriteOutcome.Enqueued,
                queue.TryEnqueueFrame(
                    WireMessage.EncryptedBinary(payload)));
        }
        var terminal = WireMessage.Json(new byte[1024]);

        Assert.True(queue.TryCompleteWithControlMessage(
            terminal,
            QueueCompletionCause.SessionEnd,
            CloseReason.SessionEnded));

        var read = await queue.ReadAsync(CancellationToken.None);
        Assert.Equal(terminal.Payload, read?.Payload);
        Assert.Null(await queue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task Pending_Permissions_Coalesces_To_Latest_Snapshot()
    {
        var queue = new RelayClientSendQueue(AccessLevel.Suggest);
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueuePendingPermissionsSnapshot(WireMessage.EncryptedBinary([0x06, 1])));
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueuePendingPermissionsSnapshot(WireMessage.EncryptedBinary([0x06, 2])));

        Assert.Equal(
            new byte[] { 0x06, 2 },
            (await queue.ReadAsync(CancellationToken.None))?.Payload);
    }

    [Fact]
    public async Task Key_Rotation_Atomically_Purges_Queued_Encrypted_Frames()
    {
        var queue = new RelayClientSendQueue(AccessLevel.Suggest);
        var preRotationControl = WireMessage.Json([0x10]);
        var receipt = WireMessage.Json([0x11]);
        var rotation = WireMessage.Json([0x22]);
        var control = WireMessage.Json([0x33]);
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueControl(preRotationControl));
        Assert.True(queue.BeginSemanticReceiptAdmission());
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueSemanticReceipt(Guid.NewGuid(), receipt));
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryQueueTerminalReplay(
            [
                WireMessage.EncryptedBinary([0x03, 1]),
                WireMessage.EncryptedBinary([0x04, 1]),
            ]));
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueFrame(WireMessage.EncryptedBinary([0x05, 1])));
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueuePendingPermissionsSnapshot(
                WireMessage.EncryptedBinary([0x06, 1])));

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueKeyRotation(rotation));
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.CompleteSemanticReceiptAdmission([]));
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueControl(control));
        queue.Complete();

        Assert.Equal(preRotationControl, await queue.ReadAsync(CancellationToken.None));
        Assert.Equal(rotation, await queue.ReadAsync(CancellationToken.None));
        Assert.Equal(receipt, await queue.ReadAsync(CancellationToken.None));
        Assert.Equal(control, await queue.ReadAsync(CancellationToken.None));
        Assert.Null(await queue.ReadAsync(CancellationToken.None));
    }
}
