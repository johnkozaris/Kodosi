using System.Collections.Concurrent;
using System.Reflection;
using System.Text;
using System.Text.Json;
using Microsoft.Extensions.Logging.Abstractions;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Realtime;

using Kodosi.Host.Serialization;

namespace Kodosi.HostTests;

public sealed class SessionBroadcasterTests
{
    [Fact]
    public void RemoveSession_Drops_PreHost_Host_Fence_State_And_Releases_Lock()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();

        var sendQueues = broadcaster.GetOrCreateSession(sessionId);
        broadcaster.NotifyHostKeyDistributionRequested(sessionId);

        Assert.Equal(0, HostFenceLockCount(broadcaster));
        Assert.Equal(1, PreHostFenceQueueCount(broadcaster));

        Assert.True(broadcaster.RemoveSessionIfSame(sessionId, sendQueues));

        Assert.Equal(0, HostFenceLockCount(broadcaster));
        Assert.Equal(0, PreHostFenceQueueCount(broadcaster));
    }

    [Fact]
    public void NotifyHostKeyDistributionRequested_Drops_PreHost_State_And_Releases_Lock()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();

        broadcaster.NotifyHostKeyDistributionRequested(sessionId);

        Assert.Equal(0, HostFenceLockCount(broadcaster));
        Assert.Equal(0, PreHostFenceQueueCount(broadcaster));
    }

    [Fact]
    public async Task Fenced_Broadcasts_Do_Not_Reach_Replacement_Queues()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            new OperationalMetrics(runtimeDirectory),
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var staleQueues = broadcaster.GetOrCreateSession(sessionId);
        Assert.True(broadcaster.RemoveSessionIfSame(sessionId, staleQueues));
        var replacementQueues = broadcaster.GetOrCreateSession(sessionId);
        var replacementParticipant = replacementQueues.AddParticipantQueue(
            "replacement",
            AccessLevel.Inject);
        var replacementHost = replacementQueues.SetHostQueue("replacement-host");

        Assert.False(broadcaster.BroadcastToDownstreamClientsIfSame(
            sessionId,
            staleQueues,
            WireMessage.Json("frame"u8.ToArray())));
        Assert.False(broadcaster.BroadcastPendingPermissionsIfSame(
            sessionId,
            staleQueues,
            WireMessage.EncryptedBinary([0x06])));

        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(50));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            async () => await replacementParticipant.ReadAsync(timeout.Token));
        using var hostTimeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(50));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            async () => await replacementHost.ReadAsync(hostTimeout.Token));
    }

    [Fact]
    public async Task BroadcastPendingPermissions_Skips_View_And_Includes_Eligible_Roles()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var viewQueue = queues.AddParticipantQueue("viewer", AccessLevel.View);
        var suggestQueue = queues.AddParticipantQueue("suggester", AccessLevel.Suggest);
        var injectQueue = queues.AddParticipantQueue("owner-self", AccessLevel.Inject);
        var approverQueue = queues.AddParticipantQueue("approver", AccessLevel.Approve);

        var snapshotBytes = new byte[] { 0x06, 1, 2, 3 };
        Assert.True(broadcaster.BroadcastPendingPermissionsIfSame(
            sessionId,
            queues,
            WireMessage.EncryptedBinary(snapshotBytes)));

        using var viewTimeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(50));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            async () => await viewQueue.ReadAsync(viewTimeout.Token));
        var suggestMessage = await suggestQueue.ReadAsync(CancellationToken.None);
        var injectMessage = await injectQueue.ReadAsync(CancellationToken.None);
        var approverMessage = await approverQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(suggestMessage);
        Assert.Equal(WireMessageKind.EncryptedBinary, suggestMessage.Value.Kind);
        Assert.Equal(snapshotBytes, suggestMessage.Value.Payload);
        Assert.NotNull(injectMessage);
        Assert.Equal(snapshotBytes, injectMessage.Value.Payload);
        Assert.NotNull(approverMessage);
        Assert.Equal(snapshotBytes, approverMessage.Value.Payload);
    }

    private static object GetFences(SessionBroadcaster broadcaster)
    {
        var field = typeof(SessionBroadcaster)
            .GetField("_fences", BindingFlags.Instance | BindingFlags.NonPublic);
        Assert.NotNull(field);
        var fences = field.GetValue(broadcaster);
        Assert.NotNull(fences);
        return fences;
    }

    private static int HostFenceLockCount(SessionBroadcaster broadcaster)
    {
        var fences = GetFences(broadcaster);
        var field = fences.GetType()
            .GetField("_hostFenceLocks", BindingFlags.Instance | BindingFlags.NonPublic);
        Assert.NotNull(field);
        var hostFenceLocks = field.GetValue(fences);
        Assert.NotNull(hostFenceLocks);
        var countProperty = hostFenceLocks.GetType().GetProperty("Count");
        Assert.NotNull(countProperty);
        return Assert.IsType<int>(countProperty.GetValue(hostFenceLocks));
    }

    private static int PreHostFenceQueueCount(SessionBroadcaster broadcaster)
    {
        var fences = GetFences(broadcaster);
        var field = fences.GetType()
            .GetField("_preHostFenceQueues", BindingFlags.Instance | BindingFlags.NonPublic);
        Assert.NotNull(field);
        var preHostFenceQueues = Assert.IsType<ConcurrentDictionary<SessionId, PendingHostFenceQueue>>(
            field.GetValue(fences));
        return preHostFenceQueues.Count;
    }

    [Fact]
    public async Task BroadcastTerminal_Overflow_Resyncs_With_Checkpoint_And_Raw_Continuation()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var queue = broadcaster
            .GetOrCreateSession(sessionId)
            .AddParticipantQueue("lagging", AccessLevel.View);
        var stream = runtimeDirectory.CreateRuntime(sessionId).Stream;
        var checkpoint = new byte[] { 0x03, 0x01 };
        var firstRaw = new byte[] { 0x04, 0x01 };
        var secondRaw = new byte[] { 0x04, 0x02 };
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreEncryptedCheckpoint(1, 10, checkpoint));
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreRawBatch(10, 12, firstRaw));
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            stream.TryStoreRawBatch(12, 14, secondRaw));
        for (var index = 0; index < 64; index++)
        {
            Assert.Equal(
                RelayClientSendQueueWriteOutcome.Enqueued,
                queue.TryEnqueueFrame(WireMessage.EncryptedBinary([(byte)index])));
        }

        Assert.True(broadcaster.BroadcastToDownstreamClientsIfSame(
            sessionId,
            broadcaster.TryGetSession(sessionId)!,
            WireMessage.EncryptedBinary([0x04, 0xFF])));

        Assert.Equal(
            checkpoint,
            (await queue.ReadAsync(CancellationToken.None))?.Payload);
        Assert.Equal(
            firstRaw,
            (await queue.ReadAsync(CancellationToken.None))?.Payload);
        Assert.Equal(
            secondRaw,
            (await queue.ReadAsync(CancellationToken.None))?.Payload);
        Assert.Null(queue.CompletionCause);
        Assert.Equal(1, metrics.Snapshot().ResyncCount);
    }

    [Fact]
    public async Task DisconnectParticipant_Sends_AccessRevoked_And_Drops_Buffered_Frames()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var participantQueue = queues.AddParticipantQueue("viewer-1");

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            participantQueue.TryEnqueueFrame(WireMessage.Json(Encoding.UTF8.GetBytes("stale-frame"))));

        broadcaster.DisconnectParticipant(sessionId, "viewer-1");

        var firstMessage = await participantQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(firstMessage);
        Assert.Equal(WireMessageKind.Json, firstMessage.Value.Kind);

        var revoked = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(firstMessage.Value.Payload),
            WsJsonContext.Default.SessionAccessRevokedMessage);
        Assert.Equal(sessionId.Value.ToString(), revoked?.SessionId);
        Assert.Null(await participantQueue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task Complete_Does_Not_Clobber_AccessRevoked_Message_After_Disconnect()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var participantQueue = queues.AddParticipantQueue("viewer-1");

        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            participantQueue.TryEnqueueFrame(WireMessage.Json(Encoding.UTF8.GetBytes("stale-frame"))));

        broadcaster.DisconnectParticipant(sessionId, "viewer-1");
        participantQueue.Complete(QueueCompletionCause.Timeout, discardPending: true);

        var firstMessage = await participantQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(firstMessage);

        var revoked = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(firstMessage.Value.Payload),
            WsJsonContext.Default.SessionAccessRevokedMessage);
        Assert.Equal(sessionId.Value.ToString(), revoked?.SessionId);
        Assert.Null(await participantQueue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task QueueFencedParticipantEffects_Cannot_Target_Reused_ConnectionId()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            new OperationalMetrics(runtimeDirectory),
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var retired = queues.AddParticipantQueue("viewer-1");
        var replacement = queues.AddParticipantQueue("viewer-1");
        var message = WireMessage.Json(Encoding.UTF8.GetBytes("stale-result"));

        Assert.False(broadcaster.SendToParticipantIfSame(
            sessionId,
            "viewer-1",
            retired,
            message));
        Assert.False(broadcaster.DisconnectParticipantIfSame(
            sessionId,
            "viewer-1",
            retired));
        Assert.False(broadcaster.DisconnectParticipantForRefreshIfSame(
            sessionId,
            "viewer-1",
            retired));

        Assert.Null(replacement.CompletionCause);
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            replacement.TryEnqueueControl(message));
        var delivered = await replacement.ReadAsync(CancellationToken.None);
        Assert.Equal(message.Payload, delivered?.Payload);
    }

    [Fact]
    public async Task NotifyHostKeyDistributionRequested_Enqueues_Control_Message()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var hostQueue = queues.SetHostQueue();

        broadcaster.NotifyHostKeyDistributionRequested(sessionId);

        var payload = await hostQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(payload);

        var message = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(payload!),
            WsJsonContext.Default.HostKeyDistributionRequestedMessage);
        Assert.Equal(sessionId.Value.ToString(), message?.SessionId);
        Assert.False(string.IsNullOrWhiteSpace(message?.FenceId));
    }

    [Fact]
    public async Task HostFence_Replays_Until_Acknowledged()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            new OperationalMetrics(runtimeDirectory),
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var hostQueue = broadcaster.GetOrCreateSession(sessionId).SetHostQueue();

        broadcaster.NotifyHostKeyDistributionRequested(sessionId);
        var firstPayload = await hostQueue.ReadAsync(CancellationToken.None);
        var first = JsonSerializer.Deserialize(
            firstPayload!,
            WsJsonContext.Default.HostKeyDistributionRequestedMessage);
        Assert.NotNull(first);

        broadcaster.ReplayUnacknowledgedHostFences(sessionId);
        var replayPayload = await hostQueue.ReadAsync(CancellationToken.None);
        var replay = JsonSerializer.Deserialize(
            replayPayload!,
            WsJsonContext.Default.HostKeyDistributionRequestedMessage);
        Assert.Equal(first.FenceId, replay?.FenceId);

        broadcaster.AcknowledgeHostFence(sessionId, first.FenceId);
        broadcaster.ReplayUnacknowledgedHostFences(sessionId);
        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(
            () => hostQueue.ReadAsync(timeout.Token));
    }

    [Fact]
    public async Task HostFence_Replay_Continues_Past_One_Host_Queue_Burst()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            new OperationalMetrics(runtimeDirectory),
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        runtimeDirectory.CreateRuntime(sessionId);
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var originalQueue = queues.SetHostQueue("host-1");
        const int FenceCount = 300;

        for (var index = 0; index < FenceCount; index++)
        {
            broadcaster.NotifyHostAccessRevoked(sessionId, UserId.New());
            Assert.NotNull(await originalQueue.ReadAsync(TestContext.Current.CancellationToken));
        }

        queues.ClearHostQueue("host-1");
        var replacementQueue = queues.SetHostQueue("host-2");
        broadcaster.ReplayUnacknowledgedHostFences(sessionId);

        var replayedFenceIds = new HashSet<string>(StringComparer.Ordinal);
        for (var index = 0; index < FenceCount; index++)
        {
            var payload = await replacementQueue.ReadAsync(TestContext.Current.CancellationToken);
            var message = JsonSerializer.Deserialize(
                payload!,
                WsJsonContext.Default.HostAccessRevokedMessage);
            Assert.NotNull(message);
            Assert.True(replayedFenceIds.Add(message.FenceId));
            broadcaster.ContinueUnacknowledgedHostFenceReplay(sessionId);
        }
        Assert.Equal(FenceCount, replayedFenceIds.Count);
    }

    [Fact]
    public void HostFence_PreHost_Cap_Rejects_Later_Host_Admission_FailClosed()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            new OperationalMetrics(runtimeDirectory),
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var queues = broadcaster.GetOrCreateSession(sessionId);

        for (var index = 0; index <= 128; index++)
        {
            broadcaster.NotifyHostAccessRevoked(sessionId, UserId.New());
        }

        Assert.Null(queues.TrySetHostQueue("host-after-overflow", "hello"u8.ToArray()));
    }

    [Fact]
    public async Task HostFence_Unacknowledged_Cap_Disconnects_Host_FailClosed()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            new OperationalMetrics(runtimeDirectory),
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        runtimeDirectory.CreateRuntime(sessionId);
        var hostQueue = broadcaster.GetOrCreateSession(sessionId).SetHostQueue("host-1");

        for (var index = 0; index < 1_024; index++)
        {
            broadcaster.NotifyHostAccessRevoked(sessionId, UserId.New());
            Assert.NotNull(await hostQueue.ReadAsync(TestContext.Current.CancellationToken));
        }

        broadcaster.NotifyHostAccessRevoked(sessionId, UserId.New());

        Assert.Equal(CloseReason.ServerError, hostQueue.CompletionReason);
    }

    [Fact]
    public async Task HostFence_Acknowledge_Serializes_With_New_Tracking()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            new OperationalMetrics(runtimeDirectory),
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var hostQueue = broadcaster.GetOrCreateSession(sessionId).SetHostQueue();
        var fences = Assert.IsType<HostFenceCoordinator>(GetFences(broadcaster));

        broadcaster.NotifyHostKeyDistributionRequested(sessionId);
        var first = JsonSerializer.Deserialize(
            (await hostQueue.ReadAsync(CancellationToken.None))!,
            WsJsonContext.Default.HostKeyDistributionRequestedMessage);
        Assert.NotNull(first);

        Task acknowledgeTask;
        using (var fenceLock = fences.AcquireLock(sessionId))
        {
            lock (fenceLock.SyncRoot)
            {
                using var acknowledgeStarted = new ManualResetEventSlim();
                acknowledgeTask = Task.Run(() =>
                {
                    acknowledgeStarted.Set();
                    broadcaster.AcknowledgeHostFence(sessionId, first.FenceId);
                }, TestContext.Current.CancellationToken);
                Assert.True(acknowledgeStarted.Wait(
                    TimeSpan.FromSeconds(1),
                    TestContext.Current.CancellationToken));
                Assert.False(SpinWait.SpinUntil(
                    () => acknowledgeTask.IsCompleted,
                    TimeSpan.FromMilliseconds(100)));

                broadcaster.NotifyHostKeyDistributionRequested(sessionId);
            }
        }

        await acknowledgeTask;
        var second = JsonSerializer.Deserialize(
            (await hostQueue.ReadAsync(CancellationToken.None))!,
            WsJsonContext.Default.HostKeyDistributionRequestedMessage);
        Assert.NotNull(second);

        broadcaster.ReplayUnacknowledgedHostFences(sessionId);
        var replay = JsonSerializer.Deserialize(
            (await hostQueue.ReadAsync(CancellationToken.None))!,
            WsJsonContext.Default.HostKeyDistributionRequestedMessage);
        Assert.Equal(second.FenceId, replay?.FenceId);
    }

    [Fact]
    public async Task PendingHostKeyDistribution_Is_Replayed_When_Host_Reconnects()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();

        runtimeDirectory.CreateRuntime(sessionId);
        broadcaster.NotifyHostKeyDistributionRequested(sessionId);

        var ownerQueue = broadcaster.GetOrCreateSession(sessionId).SetHostQueue();
        broadcaster.FlushPendingHostFences(sessionId);

        var payload = await ownerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(payload);
        var message = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(payload!),
            WsJsonContext.Default.HostKeyDistributionRequestedMessage);
        Assert.Equal(sessionId.Value.ToString(), message?.SessionId);
    }

    [Fact]
    public async Task PendingHostKeyDistribution_Is_Replayed_When_Request_Arrives_Before_Runtime_State()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();

        broadcaster.GetOrCreateSession(sessionId);
        broadcaster.NotifyHostKeyDistributionRequested(sessionId);

        Assert.Null(runtimeDirectory.TryGet(sessionId));

        runtimeDirectory.CreateRuntime(sessionId);
        var ownerQueue = broadcaster.GetOrCreateSession(sessionId).SetHostQueue();
        broadcaster.FlushPendingHostFences(sessionId);

        var payload = await ownerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(payload);
        var message = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(payload!),
            WsJsonContext.Default.HostKeyDistributionRequestedMessage);
        Assert.Equal(sessionId.Value.ToString(), message?.SessionId);
    }

    [Fact]
    public async Task PendingParticipantChanged_Coalesces_By_User_And_Action()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var participantUserId = UserId.New();

        runtimeDirectory.CreateRuntime(sessionId);
        broadcaster.NotifyHostParticipantChanged(sessionId, 1, "joined", participantUserId);
        broadcaster.NotifyHostParticipantChanged(sessionId, 2, "joined", participantUserId);

        var ownerQueue = broadcaster.GetOrCreateSession(sessionId).SetHostQueue();
        broadcaster.FlushPendingHostFences(sessionId);

        var payload = await ownerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(payload);
        var message = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(payload!),
            WsJsonContext.Default.HostParticipantChangedMessage);
        Assert.Equal(participantUserId.Value.ToString(), message?.ParticipantUserId);
        Assert.Equal(2, message?.ParticipantCount);
        using var timeoutCts = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(() => ownerQueue.ReadAsync(timeoutCts.Token));
    }

    [Fact]
    public async Task PendingAccessRevoked_Coalesces_By_User()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var revokedUserId = UserId.New();

        runtimeDirectory.CreateRuntime(sessionId);
        broadcaster.NotifyHostAccessRevoked(sessionId, revokedUserId);
        broadcaster.NotifyHostAccessRevoked(sessionId, revokedUserId);

        var ownerQueue = broadcaster.GetOrCreateSession(sessionId).SetHostQueue();
        broadcaster.FlushPendingHostFences(sessionId);

        var payload = await ownerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(payload);
        var message = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(payload!),
            WsJsonContext.Default.HostAccessRevokedMessage);
        Assert.Equal(revokedUserId.Value.ToString(), message?.RevokedUserId);
        using var timeoutCts = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(() => ownerQueue.ReadAsync(timeoutCts.Token));
    }

    [Fact]
    public async Task PendingHostFences_Preserve_Fifo_Order_After_Partial_Flush_Failure()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var firstRevokedUserId = UserId.New();
        var secondRevokedUserId = UserId.New();

        runtimeDirectory.CreateRuntime(sessionId);
        var ownerQueue = broadcaster.GetOrCreateSession(sessionId).SetHostQueue();
        for (var index = 0; index < 256; index++)
        {
            Assert.Equal(
                ChannelByteSendQueueWriteOutcome.Enqueued,
                ownerQueue.TryEnqueue(Encoding.UTF8.GetBytes($$"""{"type":"filler","index":{{index}}}""")));
        }

        broadcaster.NotifyHostAccessRevoked(sessionId, firstRevokedUserId);
        broadcaster.NotifyHostAccessRevoked(sessionId, secondRevokedUserId);

        Assert.NotNull(await ownerQueue.ReadAsync(CancellationToken.None));
        broadcaster.FlushPendingHostFences(sessionId);
        for (var index = 0; index < 255; index++)
        {
            Assert.NotNull(await ownerQueue.ReadAsync(CancellationToken.None));
        }

        var firstPayload = await ownerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(firstPayload);
        var firstMessage = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(firstPayload!),
            WsJsonContext.Default.HostAccessRevokedMessage);
        Assert.Equal(firstRevokedUserId.Value.ToString(), firstMessage?.RevokedUserId);

        broadcaster.FlushPendingHostFences(sessionId);
        var secondPayload = await ownerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(secondPayload);
        var secondMessage = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(secondPayload!),
            WsJsonContext.Default.HostAccessRevokedMessage);
        Assert.Equal(secondRevokedUserId.Value.ToString(), secondMessage?.RevokedUserId);
    }

    [Fact]
    public async Task BroadcastSessionStatus_Also_Fans_Out_To_Participant_Queues()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var participantQueue = queues.AddParticipantQueue("participant-1");

        broadcaster.BroadcastSessionStatus(sessionId, SessionStatus.Live);

        var payload = await participantQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(payload);
        Assert.Equal(WireMessageKind.Json, payload.Value.Kind);

        var message = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(payload.Value.Payload),
            WsJsonContext.Default.SessionStatusMessage);
        Assert.Equal(SessionStatus.Live, message?.Status);
    }

    [Fact]
    public void ForceDisconnectHost_Preserves_Requested_Close_Reason()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var hostQueue = broadcaster.GetOrCreateSession(sessionId).SetHostQueue("host-1");

        broadcaster.ForceDisconnectHost(sessionId, CloseReason.OwnerIdentityReset);

        Assert.Equal(CloseReason.OwnerIdentityReset, hostQueue.CompletionReason);
    }

    [Fact]
    public void ForceDisconnectHost_Allows_Replacement_Host_Admission()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            new OperationalMetrics(runtimeDirectory),
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var original = queues.SetHostQueue("host-1");

        broadcaster.ForceDisconnectHost(sessionId, CloseReason.AccessRevoked);

        Assert.Equal(CloseReason.AccessRevoked, original.CompletionReason);
        Assert.NotNull(queues.TrySetHostQueue("host-2", "hello"u8.ToArray()));
    }

    [Fact]
    public void RemoveSession_Preserves_Ended_Close_Reason_For_Participants()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var participantQueue = queues.AddParticipantQueue("participant-1");

        Assert.True(broadcaster.RemoveSessionIfSame(
            sessionId,
            queues,
            CloseReason.HostStopped));

        Assert.Equal(QueueCompletionCause.SessionEnd, participantQueue.CompletionCause);
        Assert.Equal(CloseReason.HostStopped, participantQueue.CompletionCloseReason);
    }

    [Fact]
    public async Task BroadcastSessionEnded_Delivers_End_Message_Even_When_Control_Backlog_Is_Full()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var participantQueue = broadcaster.GetOrCreateSession(sessionId).AddParticipantQueue("participant-1");

        for (var i = 0; i < 16; i++)
        {
            Assert.Equal(
                RelayClientSendQueueWriteOutcome.Enqueued,
                participantQueue.TryEnqueueControl(WireMessage.Json(Encoding.UTF8.GetBytes($$"""{"type":"control","index":{{i}}}"""))));
        }

        broadcaster.BroadcastSessionEnded(sessionId, CloseReason.HostStopped);

        Assert.Equal(QueueCompletionCause.SessionEnd, participantQueue.CompletionCause);
        Assert.Equal(CloseReason.HostStopped, participantQueue.CompletionCloseReason);
        var queued = await participantQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(queued);
        var message = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(queued.Value.Payload),
            WsJsonContext.Default.SessionEndedMessage);
        Assert.Equal(sessionId.Value.ToString(), message?.SessionId);
        Assert.Equal("host_stopped", message?.Reason);
        Assert.Null(await participantQueue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task BroadcastSessionEnded_Repeated_Call_Preserves_First_Terminal_Notification()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var participantQueue = broadcaster
            .GetOrCreateSession(sessionId)
            .AddParticipantQueue("participant-1");

        broadcaster.BroadcastSessionEnded(sessionId, CloseReason.HostStopped);
        broadcaster.BroadcastSessionEnded(sessionId, CloseReason.SessionEnded);

        Assert.Equal(CloseReason.HostStopped, participantQueue.CompletionCloseReason);
        var queued = await participantQueue.ReadAsync(CancellationToken.None);
        var message = JsonSerializer.Deserialize(
            queued!.Value.Payload,
            WsJsonContext.Default.SessionEndedMessage);
        Assert.Equal("host_stopped", message?.Reason);
        Assert.Null(await participantQueue.ReadAsync(CancellationToken.None));
    }
}
