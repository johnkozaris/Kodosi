using Kodosi.Host.Realtime;

namespace Kodosi.HostTests;

public class ChannelByteSendQueueTests
{
    [Fact]
    public void TryEnqueue_ReturnsFull_WhenBacklogIsExceeded()
    {
        var queue = new ChannelByteSendQueue();

        for (var i = 0; i < 256; i++)
        {
            Assert.Equal(ChannelByteSendQueueWriteOutcome.Enqueued, queue.TryEnqueue(new byte[] { (byte)i }));
        }

        Assert.Equal(
            ChannelByteSendQueueWriteOutcome.Full,
            queue.TryEnqueue(new byte[] { 255 }));
    }

    [Fact]
    public async Task ReadAsync_ReturnsQueuedMessagesInOrder()
    {
        var queue = new ChannelByteSendQueue();
        var first = new byte[] { 1 };
        var second = new byte[] { 2 };

        queue.TryEnqueue(first);
        queue.TryEnqueue(second);

        Assert.Equal(first, await queue.ReadAsync(CancellationToken.None));
        Assert.Equal(second, await queue.ReadAsync(CancellationToken.None));

        queue.Complete();
        Assert.Null(await queue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task Complete_WithDiscardPending_Drops_QueuedMessages()
    {
        var queue = new ChannelByteSendQueue();
        Assert.Equal(
            ChannelByteSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueue([1]));

        queue.Complete(CloseReason.AccessRevoked, discardPending: true);

        Assert.Null(await queue.ReadAsync(CancellationToken.None));
        Assert.Equal(CloseReason.AccessRevoked, queue.CompletionReason);
    }

    [Fact]
    public void Complete_Retains_First_Close_Reason()
    {
        var queue = new ChannelByteSendQueue();

        queue.Complete(CloseReason.ServerRestarting);
        queue.Complete(CloseReason.HostStopped);

        Assert.Equal(CloseReason.ServerRestarting, queue.CompletionReason);
    }

    [Fact]
    public async Task Aggregate_Byte_Budget_Bounds_2MiB_Messages_And_Releases_On_Read()
    {
        var queue = new ChannelByteSendQueue();
        var payload = new byte[2 * 1024 * 1024];

        for (var index = 0; index < 4; index++)
        {
            Assert.Equal(
                ChannelByteSendQueueWriteOutcome.Enqueued,
                queue.TryEnqueue(payload));
        }
        Assert.Equal(
            ChannelByteSendQueueWriteOutcome.Full,
            queue.TryEnqueue([1]));

        Assert.NotNull(await queue.ReadAsync(CancellationToken.None));
        Assert.Equal(
            ChannelByteSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueue(payload));
    }

    [Fact]
    public async Task Discard_Releases_Byte_Budget()
    {
        var queue = new ChannelByteSendQueue(
            maxBacklogBytes: 2 * 1024 * 1024);
        Assert.Equal(
            ChannelByteSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueue(new byte[2 * 1024 * 1024]));

        queue.Complete(CloseReason.AccessRevoked, discardPending: true);

        Assert.Null(await queue.ReadAsync(CancellationToken.None));
    }
}
