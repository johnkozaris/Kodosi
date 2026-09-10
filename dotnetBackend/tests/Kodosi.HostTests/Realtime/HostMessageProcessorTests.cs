using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging.Abstractions;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Realtime;
using Kodosi.Infrastructure.Crypto;

using Kodosi.Host.Serialization;

namespace Kodosi.HostTests;

public sealed class HostMessageProcessorTests
{
    [Fact]
    public async Task Action_Result_For_Shared_Approver_Commits_Before_Exact_Host_Ack()
    {
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var connections = new ConnectionRegistry();
        var dedupe = new ActionDedupeCache();
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var ownerUserId = UserId.New();
        var approverUserId = UserId.New();
        var completionStore = new ActionResultCompletionStore(
            approverUserId,
            "action-1",
            new PermissionDecisionPendingTuple(
                incarnationId,
                1,
                "tool-1",
                7,
                "approver-device"));
        var processor = CreateProcessor(
            runtimes,
            broadcaster,
            metrics,
            connections: connections,
            dedupeCache: dedupe,
            permissionDecisionAuditStore: completionStore);
        var runtime = runtimes.CreateRuntime(sessionId);
        Assert.True(runtime.Host.TryClaimHost("host", new CancellationTokenSource()));
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var hostQueue = queues.SetHostQueue("host");
        var approverQueue = queues.AddParticipantQueue("approver");
        connections.RegisterSharedParticipant(
            "approver",
            approverUserId,
            "approver-device",
            sessionId);
        _ = dedupe.Claim(sessionId, approverUserId, "action-1", "tool-1");
        var result = new HostActionResultMessage(
            sessionId.Value.ToString(),
            incarnationId,
            "action-1",
            "tool-1",
            7,
            approverUserId.Value.ToString(),
            "approver-device",
            RelayActionStatus.Accepted);

        var outcome = await processor.ProcessAsync(
            RelayOutbound.Encode(result),
            new HostMessageSource(
                sessionId,
                "host",
                runtime,
                queues,
                runtime.IncarnationId,
                incarnationId,
                ownerUserId,
                "host-device",
                1),
            TestContext.Current.CancellationToken);

        Assert.Equal(HostProcessOutcomeKind.Continue, outcome.Kind);
        Assert.Equal(InputAuditStatus.Dispatched, completionStore.TerminalStatus);
        Assert.Equal(1, completionStore.CompletionCalls);
        var participant = JsonSerializer.Deserialize(
            (await approverQueue.ReadAsync(TestContext.Current.CancellationToken))!.Value.Payload,
            WsJsonContext.Default.ActionResultMessage);
        Assert.Equal(RelayActionStatus.Accepted, participant?.Status);
        var ack = JsonSerializer.Deserialize(
            (await hostQueue.ReadAsync(TestContext.Current.CancellationToken))!,
            WsJsonContext.Default.HostActionResultAckMessage);
        Assert.Equal(incarnationId, ack?.IncarnationId);
        Assert.Equal("action-1", ack?.ActionId);
        Assert.Equal(approverUserId.Value.ToString(), ack?.RequesterUserId);
    }

    [Fact]
    public async Task Action_Result_Replay_Is_Reacknowledged_Without_Rewriting_Audit()
    {
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var ownerUserId = UserId.New();
        var requesterUserId = UserId.New();
        var completionStore = new ActionResultCompletionStore(
            requesterUserId,
            "action-1",
            new PermissionDecisionPendingTuple(
                incarnationId,
                1,
                "tool-1",
                7,
                "requester-device"),
            HostActionCompletionOutcome.Duplicate);
        var processor = CreateProcessor(
            runtimes,
            broadcaster,
            metrics,
            connections: new ConnectionRegistry(),
            dedupeCache: new ActionDedupeCache(),
            permissionDecisionAuditStore: completionStore);
        var runtime = runtimes.CreateRuntime(sessionId);
        Assert.True(runtime.Host.TryClaimHost("host", new CancellationTokenSource()));
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var hostQueue = queues.SetHostQueue("host");
        var result = new HostActionResultMessage(
            sessionId.Value.ToString(),
            incarnationId,
            "action-1",
            "tool-1",
            7,
            requesterUserId.Value.ToString(),
            "requester-device",
            RelayActionStatus.Accepted);

        var outcome = await processor.ProcessAsync(
            RelayOutbound.Encode(result),
            new HostMessageSource(
                sessionId,
                "host",
                runtime,
                queues,
                runtime.IncarnationId,
                incarnationId,
                ownerUserId,
                "host-device",
                1),
            TestContext.Current.CancellationToken);

        Assert.Equal(HostProcessOutcomeKind.Continue, outcome.Kind);
        Assert.Null(completionStore.TerminalStatus);
        var ack = JsonSerializer.Deserialize(
            (await hostQueue.ReadAsync(TestContext.Current.CancellationToken))!,
            WsJsonContext.Default.HostActionResultAckMessage);
        Assert.Equal("action-1", ack?.ActionId);
    }

    [Theory]
    [InlineData("altered-request", "approver-device")]
    [InlineData("tool-1", "altered-device")]
    public async Task Action_Result_Host_Correlation_Assertions_Cannot_Change_Canonical_Route(
        string assertedRequestId,
        string assertedDeviceId)
    {
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var connections = new ConnectionRegistry();
        var dedupe = new ActionDedupeCache();
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var requesterUserId = UserId.New();
        var canonicalTuple = new PermissionDecisionPendingTuple(
            incarnationId,
            7,
            "tool-1",
            7,
            "approver-device");
        var completionStore = new ActionResultCompletionStore(
            requesterUserId,
            "action-1",
            canonicalTuple,
            HostActionCompletionOutcome.Conflict);
        var processor = CreateProcessor(
            runtimes,
            broadcaster,
            metrics,
            connections: connections,
            dedupeCache: dedupe,
            permissionDecisionAuditStore: completionStore);
        var runtime = runtimes.CreateRuntime(sessionId);
        Assert.True(runtime.Host.TryClaimHost("host", new CancellationTokenSource()));
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var hostQueue = queues.SetHostQueue("host");
        var canonicalQueue = queues.AddParticipantQueue("canonical");
        var assertedQueue = queues.AddParticipantQueue("asserted");
        connections.RegisterSharedParticipant(
            "canonical",
            requesterUserId,
            canonicalTuple.RequesterDeviceId,
            sessionId);
        connections.RegisterSharedParticipant(
            "asserted",
            requesterUserId,
            assertedDeviceId,
            sessionId);
        var outcome = await processor.ProcessAsync(
            RelayOutbound.Encode(new HostActionResultMessage(
                sessionId.Value.ToString(),
                incarnationId,
                "action-1",
                assertedRequestId,
                7,
                requesterUserId.Value.ToString(),
                assertedDeviceId,
                RelayActionStatus.Accepted)),
            new HostMessageSource(
                sessionId,
                "host",
                runtime,
                queues,
                runtime.IncarnationId,
                incarnationId,
                UserId.New(),
                "host-device",
                7),
            TestContext.Current.CancellationToken);

        Assert.Equal(HostProcessOutcomeKind.ProtocolViolation, outcome.Kind);
        Assert.Equal(CloseReason.InvalidMessage, outcome.CloseReason);
        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => canonicalQueue.ReadAsync(timeout.Token));
        using var assertedTimeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => assertedQueue.ReadAsync(assertedTimeout.Token));
        using var ackTimeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => hostQueue.ReadAsync(ackTimeout.Token));
    }

    [Fact]
    public async Task Semantic_Receipt_Is_Stored_Before_Host_Ack_And_Routed_To_Exact_Device()
    {
        if (!MLDsa.IsSupported)
            return;

        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var connections = new ConnectionRegistry();
        var semanticRelay = new RecordingSemanticRelayRepository();
        var ownerUserId = UserId.New();
        using var signingKey = MLDsa.GenerateKey(MLDsaAlgorithm.MLDsa65);
        var verifier = CreateSemanticReceiptVerifier(
            ownerUserId,
            "host-device",
            signingKey.ExportMLDsaPublicKey());
        var processor = CreateProcessor(
            runtimes,
            broadcaster,
            metrics,
            scopeFactory: new FakeScopeFactory(verifier),
            semanticRelay: semanticRelay,
            connections: connections);
        var sessionId = SessionId.New();
        var sessionIncarnationId = Guid.CreateVersion7();
        var requestId = Guid.CreateVersion7();
        var runtime = runtimes.CreateRuntime(sessionId);
        Assert.True(runtime.Host.TryClaimHost(
            "host",
            new CancellationTokenSource()));
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var hostQueue = queues.SetHostQueue("host");
        var requesterQueue = queues.AddPreparedParticipantQueue(
            "requester",
            AccessLevel.View,
            static _ => { },
            semanticReceiptDestination: new SessionSendQueues.SemanticReceiptDestination(ownerUserId, "requester-device"));
        var otherQueue = queues.AddPreparedParticipantQueue(
            "other-device",
            AccessLevel.View,
            static _ => { },
            semanticReceiptDestination: new SessionSendQueues.SemanticReceiptDestination(ownerUserId, "other-device"));
        connections.RegisterOwnerParticipant(
            "requester",
            ownerUserId,
            "requester-device",
            sessionId);
        connections.RegisterOwnerParticipant(
            "other-device",
            ownerUserId,
            "other-device",
            sessionId);
        var source = new HostMessageSource(
            sessionId,
            "host",
            runtime,
            queues,
            runtime.IncarnationId,
            sessionIncarnationId,
            ownerUserId,
            "host-device");
        var payloadSha256 = new string('a', 64);
        var signature = SignSemanticReceipt(
            signingKey,
            sessionId,
            sessionIncarnationId,
            requestId,
            "stopAndSend",
            payloadSha256,
            "injected",
            ownerUserId,
            "requester-device",
            "host-device");
        var receipt = new HostSemanticReceiptMessage(
            sessionId.Value.ToString(),
            sessionIncarnationId,
            requestId,
            RelaySemanticMode.StopAndSend,
            payloadSha256,
            RelaySemanticOutcome.Injected,
            ownerUserId.Value.ToString(),
            "requester-device",
            ownerUserId.Value.ToString(),
            "host-device",
            signature);

        var outcome = await processor.ProcessAsync(
            RelayOutbound.Encode(receipt),
            source,
            TestContext.Current.CancellationToken);

        Assert.Equal(HostProcessOutcomeKind.Continue, outcome.Kind);
        Assert.True(semanticRelay.StoreCompleted);
        Assert.Equal(
            (sessionId, sessionIncarnationId, ownerUserId, "requester-device", requestId,
                "stopAndSend", payloadSha256, "injected", ownerUserId,
                "host-device", signature),
            semanticRelay.Stored);
        using var deliveryTimeout = CancellationTokenSource.CreateLinkedTokenSource(TestContext.Current.CancellationToken);
        deliveryTimeout.CancelAfter(TimeSpan.FromSeconds(5));
        var participantBytes = await requesterQueue.ReadAsync(deliveryTimeout.Token);
        var participantReceipt = JsonSerializer.Deserialize(
            participantBytes!.Value.Payload,
            WsJsonContext.Default.ParticipantSemanticReceiptMessage);
        Assert.Equal(requestId, participantReceipt?.RequestId);
        using (var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25)))
        {
            await Assert.ThrowsAnyAsync<OperationCanceledException>(
                () => otherQueue.ReadAsync(timeout.Token));
        }
        var ackBytes = await hostQueue.ReadAsync(deliveryTimeout.Token);
        var ack = JsonSerializer.Deserialize(
            ackBytes!,
            WsJsonContext.Default.HostSemanticReceiptAckMessage);
        Assert.Equal(requestId, ack?.RequestId);
        Assert.Equal("requester-device", ack?.RequesterDeviceId);
    }

    [Fact]
    public async Task Semantic_Receipt_Invalid_Owner_Signature_Has_No_Side_Effects()
    {
        if (!MLDsa.IsSupported)
            return;

        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(runtimes, metrics, NullLoggerFactory.Instance);
        var semanticRelay = new RecordingSemanticRelayRepository();
        var connections = new ConnectionRegistry();
        var ownerUserId = UserId.New();
        using var signingKey = MLDsa.GenerateKey(MLDsaAlgorithm.MLDsa65);
        var processor = CreateProcessor(
            runtimes,
            broadcaster,
            metrics,
            scopeFactory: new FakeScopeFactory(CreateSemanticReceiptVerifier(
                ownerUserId,
                "host-device",
                signingKey.ExportMLDsaPublicKey())),
            semanticRelay: semanticRelay,
            connections: connections);
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var requestId = Guid.CreateVersion7();
        var runtime = runtimes.CreateRuntime(sessionId);
        Assert.True(runtime.Host.TryClaimHost("host", new CancellationTokenSource()));
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var hostQueue = queues.SetHostQueue("host");
        var participantQueue = queues.AddParticipantQueue("requester");
        connections.RegisterOwnerParticipant(
            "requester",
            ownerUserId,
            "requester-device",
            sessionId);
        var signature = SignSemanticReceipt(
            signingKey,
            sessionId,
            incarnationId,
            requestId,
            "steer",
            new string('a', 64),
            "injected",
            ownerUserId,
            "requester-device",
            "host-device");
        var receipt = new HostSemanticReceiptMessage(
            sessionId.Value.ToString(),
            incarnationId,
            requestId,
            RelaySemanticMode.Steer,
            new string('b', 64),
            RelaySemanticOutcome.Injected,
            ownerUserId.Value.ToString(),
            "requester-device",
            ownerUserId.Value.ToString(),
            "host-device",
            signature);

        var outcome = await processor.ProcessAsync(
            RelayOutbound.Encode(receipt),
            new HostMessageSource(
                sessionId,
                "host",
                runtime,
                queues,
                runtime.IncarnationId,
                incarnationId,
                ownerUserId,
                "host-device"),
            TestContext.Current.CancellationToken);

        Assert.Equal(HostProcessOutcomeKind.ProtocolViolation, outcome.Kind);
        Assert.False(semanticRelay.StoreCompleted);
        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => participantQueue.ReadAsync(timeout.Token));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => hostQueue.ReadAsync(timeout.Token));
    }

    [Fact]
    public async Task Semantic_Receipt_Cannot_Target_A_Different_Requester_Account()
    {
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var semanticRelay = new RecordingSemanticRelayRepository();
        var processor = CreateProcessor(
            runtimes,
            broadcaster,
            metrics,
            semanticRelay: semanticRelay,
            connections: new ConnectionRegistry());
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var ownerUserId = UserId.New();
        var runtime = runtimes.CreateRuntime(sessionId);
        Assert.True(runtime.Host.TryClaimHost("host", new CancellationTokenSource()));
        var queues = broadcaster.GetOrCreateSession(sessionId);
        queues.SetHostQueue("host");
        var receipt = new HostSemanticReceiptMessage(
            sessionId.Value.ToString(),
            incarnationId,
            Guid.CreateVersion7(),
            RelaySemanticMode.Steer,
            new string('a', 64),
            RelaySemanticOutcome.Injected,
            UserId.New().Value.ToString(),
            "requester-device",
            ownerUserId.Value.ToString(),
            "host-device",
            "owner-signature");

        var outcome = await processor.ProcessAsync(
            RelayOutbound.Encode(receipt),
            new HostMessageSource(
                sessionId,
                "host",
                runtime,
                queues,
                runtime.IncarnationId,
                incarnationId,
                ownerUserId,
                "host-device",
                1),
            TestContext.Current.CancellationToken);

        Assert.Equal(HostProcessOutcomeKind.ProtocolViolation, outcome.Kind);
        Assert.False(semanticRelay.StoreCompleted);
    }

    [Fact]
    public async Task Semantic_Receipt_Conflict_Is_Rejected_Without_Host_Ack()
    {
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var semanticRelay = new RecordingSemanticRelayRepository
        {
            StoreResult = false,
        };
        var processor = CreateProcessor(
            runtimes,
            broadcaster,
            metrics,
            semanticRelay: semanticRelay,
            connections: new ConnectionRegistry());
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var ownerUserId = UserId.New();
        var runtime = runtimes.CreateRuntime(sessionId);
        Assert.True(runtime.Host.TryClaimHost(
            "host",
            new CancellationTokenSource()));
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var hostQueue = queues.SetHostQueue("host");
        var receipt = new HostSemanticReceiptMessage(
            sessionId.Value.ToString(),
            incarnationId,
            Guid.CreateVersion7(),
            RelaySemanticMode.Queue,
            new string('a', 64),
            RelaySemanticOutcome.Cancelled,
            ownerUserId.Value.ToString(),
            "requester-device",
            ownerUserId.Value.ToString(),
            "host-device",
            "owner-signature");

        var outcome = await processor.ProcessAsync(
            RelayOutbound.Encode(receipt),
            new HostMessageSource(
                sessionId,
                "host",
                runtime,
                queues,
                runtime.IncarnationId,
                incarnationId,
                ownerUserId,
                "host-device",
                1),
            TestContext.Current.CancellationToken);

        Assert.Equal(HostProcessOutcomeKind.ProtocolViolation, outcome.Kind);
        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => hostQueue.ReadAsync(timeout.Token));
    }

    [Theory]
    [InlineData("host.fenceAck")]
    public async Task ProcessAsync_Rejects_Host_Control_Message_Above_PerType_Limit(
        string messageType)
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimeDirectory, broadcaster, metrics);
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        var payload = Encoding.UTF8.GetBytes(
            $$"""{"type":"{{messageType}}","padding":"{{new string('x', RelayMessageLimits.GetMaxBytes(messageType))}}"}""");
        Assert.InRange(
            payload.Length,
            RelayMessageLimits.GetMaxBytes(messageType) + 1,
            RelayMessageLimits.OuterMessageMaxBytes);

        var outcome = await processor.ProcessAsync(
            payload,
            CreateSource(broadcaster, sessionId, runtime),
            TestContext.Current.CancellationToken);

        Assert.Equal(HostProcessOutcomeKind.ProtocolViolation, outcome.Kind);
        Assert.Equal(CloseReason.InvalidMessage, outcome.CloseReason);
    }

    [Fact]
    public async Task ProcessAsync_Returns_ProtocolViolation_For_Unknown_Message_Type()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimeDirectory, broadcaster, metrics);
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);

        var outcome = await processor.ProcessAsync(
            Encoding.UTF8.GetBytes("""{"type":"host.futureMessage","sessionId":"session-123"}"""),
            CreateSource(broadcaster, sessionId, runtime),
            CancellationToken.None);

        Assert.Equal(HostProcessOutcomeKind.ProtocolViolation, outcome.Kind);
        Assert.Equal(CloseReason.InvalidMessage, outcome.CloseReason);
    }

    [Fact]
    public async Task ProcessAsync_Returns_ProtocolViolation_For_Malformed_Json()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimeDirectory, broadcaster, metrics);
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);

        var outcome = await processor.ProcessAsync(
            Encoding.UTF8.GetBytes("{not-json"),
            CreateSource(broadcaster, sessionId, runtime),
            CancellationToken.None);

        Assert.Equal(HostProcessOutcomeKind.ProtocolViolation, outcome.Kind);
        Assert.Equal(CloseReason.InvalidMessage, outcome.CloseReason);
    }

    [Fact]
    public async Task ProcessAsync_Returns_ProtocolViolation_For_Missing_Message_Type()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimeDirectory, broadcaster, metrics);
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);

        var outcome = await processor.ProcessAsync(
            "{}"u8.ToArray(),
            CreateSource(broadcaster, sessionId, runtime),
            CancellationToken.None);

        Assert.Equal(HostProcessOutcomeKind.ProtocolViolation, outcome.Kind);
        Assert.Equal(CloseReason.InvalidMessage, outcome.CloseReason);
    }

    [Fact]
    public async Task ProcessAsync_Returns_ProtocolViolation_For_Malformed_Known_Host_Messages()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimeDirectory, broadcaster, metrics);
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        Assert.True(runtime.Host.TryClaimHost(
            "test-host",
            new CancellationTokenSource()));
        _ = broadcaster.GetOrCreateSession(sessionId);
        var payloads = new[]
        {
            """{"type":"host.heartbeat"}""",
            $$"""{"type":"key.rotation","sessionId":"{{sessionId.Value}}"}""",
            """{"type":"host.end","reason":"host_stopped"}""",
        };

        foreach (var payload in payloads)
        {
            var outcome = await processor.ProcessAsync(
                Encoding.UTF8.GetBytes(payload),
                CreateSource(broadcaster, sessionId, runtime),
                CancellationToken.None);

            Assert.Equal(HostProcessOutcomeKind.ProtocolViolation, outcome.Kind);
            Assert.Equal(CloseReason.InvalidMessage, outcome.CloseReason);
        }
    }

    [Theory]
    [InlineData(0x03, RelayMessageLimits.TerminalCheckpointFrameMaxBytes)]
    [InlineData(0x05, RelayMessageLimits.TerminalPresentationFrameMaxBytes)]
    [InlineData(0x04, RelayMessageLimits.OuterMessageMaxBytes)]
    [InlineData(0x06, RelayMessageLimits.OuterMessageMaxBytes)]
    public async Task ProcessBinary_Rejects_Frame_Above_Its_Exact_Type_Limit(
        byte frameType,
        int maximum)
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimeDirectory, broadcaster, metrics);
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        var frame = new byte[maximum + 1];
        frame[0] = frameType;

        var outcome = await ProcessBinaryAsync(
            processor,
            broadcaster,
            frame,
            sessionId,
            runtime);

        Assert.Equal(HostProcessOutcomeKind.ProtocolViolation, outcome.Kind);
        Assert.Equal(CloseReason.UnsupportedData, outcome.CloseReason);
        var replay = runtime.Stream.GetReplayState();
        Assert.Null(replay.TerminalCheckpoint);
        Assert.Empty(replay.TerminalRawBatches);
        Assert.Null(replay.TerminalPresentation);
    }

    [Theory]
    [InlineData(0x02)]
    [InlineData(0x7F)]
    public async Task ProcessBinary_Returns_ProtocolViolation_For_Unsupported_Frame_Type(byte frameType)
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimeDirectory, broadcaster, metrics);
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);

        var frame = new byte[21];
        frame[0] = frameType;

        var outcome = await ProcessBinaryAsync(
            processor,
            broadcaster,
            frame,
            sessionId,
            runtime);

        Assert.Equal(HostProcessOutcomeKind.ProtocolViolation, outcome.Kind);
        Assert.Equal(CloseReason.UnsupportedData, outcome.CloseReason);
    }

    [Fact]
    public async Task PendingPermissions_Frame_Is_Cached_And_Forwarded_To_Eligible_Participants()
    {
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(runtimes, metrics, NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimes, broadcaster, metrics);
        var sessionId = SessionId.New();
        var incarnationId = Guid.NewGuid();
        var runtime = runtimes.CreateRuntime(sessionId);
        Assert.True(runtime.Host.TryClaimHost("test-host", new CancellationTokenSource()));
        Assert.Equal(
            LiveSessionKeyRotationStoreResult.StoredNewGeneration,
            runtime.Stream.TryStoreKeyRotation(1, [1]));
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var eligible = queues.AddParticipantQueue("eligible", AccessLevel.Suggest);
        var viewer = queues.AddParticipantQueue("viewer", AccessLevel.View);
        var frame = new byte[54];
        frame[0] = 0x06;
        BinaryPrimitives.WriteUInt32BigEndian(frame.AsSpan(1, 4), 1);
        BinaryPrimitives.WriteUInt64BigEndian(frame.AsSpan(5, 8), 1);
        BinaryPrimitives.WriteUInt64BigEndian(frame.AsSpan(13, 8), 1);
        sessionId.Value.ToByteArray(bigEndian: true).CopyTo(frame, 21);
        incarnationId.ToByteArray(bigEndian: true).CopyTo(frame, 37);
        frame[53] = 1;
        var source = new HostMessageSource(
            sessionId,
            "test-host",
            runtime,
            queues,
            runtime.IncarnationId,
            incarnationId);

        var outcome = await processor.ProcessBinaryAsync(
            frame,
            source,
            TestContext.Current.CancellationToken);

        Assert.Equal(HostProcessOutcomeKind.Continue, outcome.Kind);
        Assert.Equal(1UL, runtime.Stream.GetReplayState().PendingPermissions?.SnapshotGeneration);
        Assert.Equal(
            frame,
            (await eligible.ReadAsync(TestContext.Current.CancellationToken))?.Payload);
        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(50));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            async () => await viewer.ReadAsync(timeout.Token));
    }

    [Fact]
    public async Task KeyRotation_Clears_Stale_Encrypted_Replay_Blob()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimeDirectory, broadcaster, metrics);
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            runtime.Stream.TryStoreEncryptedCheckpoint(5, 0, [0x03, 0xAA]));

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var viewerQueue = queues.AddParticipantQueue("viewer-1");
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            viewerQueue.TryEnqueueFrame(WireMessage.EncryptedBinary([0x03, 0x01])));
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            viewerQueue.TryEnqueuePendingPermissionsSnapshot(
                WireMessage.EncryptedBinary([0x06, 0x01])));

        var payload = $$"""{"type":"key.rotation","sessionId":"{{sessionId.Value}}","keyGeneration":9}""";

        var shouldEnd = await processor.ProcessAsync(
            Encoding.UTF8.GetBytes(payload),
            CreateSource(broadcaster, sessionId, runtime),
            CancellationToken.None);

        Assert.False(shouldEnd.CloseReason.HasValue);
        Assert.Null(runtime.Stream.GetReplayState().TerminalCheckpoint);
        var queued = await viewerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(queued);
        Assert.Equal(WireMessageKind.Json, queued.Value.Kind);
        Assert.Equal(Encoding.UTF8.GetBytes(payload), queued.Value.Payload);
        viewerQueue.Complete();
        Assert.Null(await viewerQueue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task ProcessBinary_Rejects_Replay_Of_Same_Counter()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimeDirectory, broadcaster, metrics);
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        Assert.Equal(
            LiveSessionKeyRotationStoreResult.StoredNewGeneration,
            runtime.Stream.TryStoreKeyRotation(1, KeyRotation(1)));
        runtime.Participants.TryAddSharedParticipant("viewer-1", 50, out _);


        var frame = new byte[]
        {
            0x03,
            0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0, 10,
            0, 0, 0, 0, 0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0, 0,
            0xAA,
        };
        _ = await ProcessBinaryAsync(
            processor,
            broadcaster,
            frame,
            sessionId,
            runtime);
        Assert.NotNull(runtime.Stream.GetReplayState().TerminalCheckpoint);
        var rejectBefore = metrics.Snapshot().FrameAdmissionRejectCount;

        _ = await ProcessBinaryAsync(
            processor,
            broadcaster,
            frame,
            sessionId,
            runtime);

        var rejectAfter = metrics.Snapshot().FrameAdmissionRejectCount;
        Assert.Equal(rejectBefore + 1, rejectAfter);
    }

    [Fact]
    public async Task ProcessBinary_Rejects_Forward_KeyGen_Until_KeyRotation()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimeDirectory, broadcaster, metrics);
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        var queues = broadcaster.GetOrCreateSession(sessionId);
        queues.AddParticipantQueue("viewer-1");
        runtime.Participants.TryAddSharedParticipant("viewer-1", 50, out _);
        Assert.Equal(
            LiveSessionKeyRotationStoreResult.StoredNewGeneration,
            runtime.Stream.TryStoreKeyRotation(1, KeyRotation(1)));

        var frameGen1 = new byte[]
        {
            0x03,
            0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0, 5,
            0, 0, 0, 0, 0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0, 0,
            0xBB,
        };
        var frameGen2 = new byte[]
        {
            0x03,
            0, 0, 0, 2,
            0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 2,
            0, 0, 0, 0, 0, 0, 0, 0,
            0xCC,
        };
        var frameReplay = new byte[]
        {
            0x03,
            0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0, 6,
            0, 0, 0, 0, 0, 0, 0, 3,
            0, 0, 0, 0, 0, 0, 0, 0,
            0xDD,
        };
        _ = await ProcessBinaryAsync(
            processor,
            broadcaster,
            frameGen1,
            sessionId,
            runtime);
        var rejectBeforeForward = metrics.Snapshot().FrameAdmissionRejectCount;

        _ = await ProcessBinaryAsync(
            processor,
            broadcaster,
            frameGen2,
            sessionId,
            runtime);
        Assert.Equal(
            rejectBeforeForward + 1,
            metrics.Snapshot().FrameAdmissionRejectCount);

        var rotationPayload = $$"""{"type":"key.rotation","sessionId":"{{sessionId.Value}}","keyGeneration":2}""";
        await processor.ProcessAsync(
            Encoding.UTF8.GetBytes(rotationPayload),
            CreateSource(broadcaster, sessionId, runtime),
            CancellationToken.None);
        var rejectBeforeAdmittedFrame = metrics.Snapshot().FrameAdmissionRejectCount;
        _ = await ProcessBinaryAsync(
            processor,
            broadcaster,
            frameGen2,
            sessionId,
            runtime);
        Assert.Equal(
            rejectBeforeAdmittedFrame,
            metrics.Snapshot().FrameAdmissionRejectCount);

        _ = await ProcessBinaryAsync(
            processor,
            broadcaster,
            frameReplay,
            sessionId,
            runtime);
        Assert.Equal(
            rejectBeforeAdmittedFrame + 1,
            metrics.Snapshot().FrameAdmissionRejectCount);
    }

    [Fact]
    public async Task KeyRotation_With_Negative_KeyGeneration_Is_Rejected_Before_Broadcast()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimeDirectory, broadcaster, metrics);
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var viewerQueue = queues.AddParticipantQueue("viewer");

        var payload = $$"""{"type":"key.rotation","sessionId":"{{sessionId.Value}}","keyGeneration":-1}""";

        var outcome = await processor.ProcessAsync(
            Encoding.UTF8.GetBytes(payload),
            CreateSource(broadcaster, sessionId, runtime),
            CancellationToken.None);

        Assert.Equal(HostProcessOutcomeKind.ProtocolViolation, outcome.Kind);
        Assert.Equal(CloseReason.InvalidMessage, outcome.CloseReason);
        viewerQueue.Complete();
        Assert.Null(await viewerQueue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task KeyRotation_Duplicate_Generation_Is_Idempotent_Without_Rebroadcast()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimeDirectory, broadcaster, metrics);
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        var viewerQueue = broadcaster.GetOrCreateSession(sessionId).AddParticipantQueue("viewer");
        var payload = $$"""{"type":"key.rotation","sessionId":"{{sessionId.Value}}","keyGeneration":1}""";
        var payloadBytes = Encoding.UTF8.GetBytes(payload);

        await processor.ProcessAsync(
            payloadBytes,
            CreateSource(broadcaster, sessionId, runtime),
            CancellationToken.None);
        Assert.NotNull(await viewerQueue.ReadAsync(CancellationToken.None));
        var rejectionsBeforeDuplicate = metrics.Snapshot().KeyRotationRejectionCount;

        await processor.ProcessAsync(
            payloadBytes,
            CreateSource(broadcaster, sessionId, runtime),
            CancellationToken.None);

        Assert.Equal(rejectionsBeforeDuplicate, metrics.Snapshot().KeyRotationRejectionCount);
        using var timeoutCts = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(() => viewerQueue.ReadAsync(timeoutCts.Token));
    }

    [Fact]
    public async Task ProcessBinary_Rejects_Unprimed_Encrypted_Frame()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimeDirectory, broadcaster, metrics);
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Participants.TryAddSharedParticipant("viewer-1", 50, out _);

        var frame = new byte[]
        {
            0x03,
            0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0, 0,
            0xAA,
        };

        _ = await ProcessBinaryAsync(
            processor,
            broadcaster,
            frame,
            sessionId,
            runtime);

        Assert.Equal(1, metrics.Snapshot().FrameAdmissionRejectCount);
        Assert.Null(runtime.Stream.GetReplayState().TerminalCheckpoint);
    }

    [Fact]
    public async Task AccessRevoke_Is_Replayed_When_Host_Reconnects()
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

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var ownerQueue = queues.SetHostQueue();

        broadcaster.FlushPendingHostFences(sessionId);

        var queued = await ownerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(queued);

        var message = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(queued!),
            WsJsonContext.Default.HostAccessRevokedMessage);

        Assert.Equal(sessionId.Value.ToString(), message?.SessionId);
        Assert.Equal(revokedUserId.Value.ToString(), message?.RevokedUserId);
    }

    [Fact]
    public async Task AccessRevokes_Preserve_All_Pending_Users_Until_Host_Reconnects()
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
        broadcaster.NotifyHostAccessRevoked(sessionId, firstRevokedUserId);
        broadcaster.NotifyHostAccessRevoked(sessionId, secondRevokedUserId);

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var ownerQueue = queues.SetHostQueue();

        broadcaster.FlushPendingHostFences(sessionId);

        var firstPayload = await ownerQueue.ReadAsync(CancellationToken.None);
        var secondPayload = await ownerQueue.ReadAsync(CancellationToken.None);

        Assert.NotNull(firstPayload);
        Assert.NotNull(secondPayload);

        var firstMessage = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(firstPayload!),
            WsJsonContext.Default.HostAccessRevokedMessage);
        var secondMessage = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(secondPayload!),
            WsJsonContext.Default.HostAccessRevokedMessage);

        Assert.Equal(firstRevokedUserId.Value.ToString(), firstMessage?.RevokedUserId);
        Assert.Equal(secondRevokedUserId.Value.ToString(), secondMessage?.RevokedUserId);
    }

    [Fact]
    public async Task ReplayEncryptedCheckpoint_And_Raw_Are_Skipped_When_Participant_Has_Exact_Cursors()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var replaySender = new SessionReplaySender(metrics);
        var sessionId = SessionId.New();
        var sessionIdString = sessionId.Value.ToString();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            runtime.Stream.TryStoreEncryptedCheckpoint(11, 4, [0x03, 0xAA, 0xBB]));
        Assert.Equal(
            LiveTerminalFrameStoreOutcome.Stored,
            runtime.Stream.TryStoreRawBatch(4, 6, [0x04, 0xAA, 0xBB]));

        var viewerQueue = new RelayClientSendQueue();

        replaySender.QueueReplay(
            runtime.Stream.GetReplayState(),
            viewerQueue,
            checkpointRevision: 11,
            presentationRevision: 0,
            nextSequence: 6,
            lastSeenKeyGeneration: 0);
        viewerQueue.Complete(discardPending: false);

        Assert.Null(await viewerQueue.ReadAsync(CancellationToken.None));
    }


    [Fact]
    public async Task HostEnd_Persists_After_The_Socket_Token_Is_Cancelled()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var session = CreateSession();
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        session.ActivateHost("host-1");

        using var services = CreateHostEndServices(session);
        var processor = CreateProcessor(
            runtimeDirectory,
            broadcaster,
            metrics,
            services.GetRequiredService<IServiceScopeFactory>());
        var runtime = runtimeDirectory.CreateRuntime(session.Id);
        runtime.Host.TryClaimHost(
            "host-1",
            new CancellationTokenSource());
        runtime.Host.SetSessionStartedAt(session.StartedAt);
        runtime.Host.SetStatus(SessionStatus.Live);
        var viewerQueue = broadcaster.GetOrCreateSession(session.Id).AddParticipantQueue("viewer-1");
        var payload =
            $$"""{"type":"host.end","sessionId":"{{session.Id.Value}}","reason":"host_stopped"}""";

        using var socketCts = new CancellationTokenSource();
        socketCts.Cancel();
        var shouldEnd = await processor.ProcessAsync(
            Encoding.UTF8.GetBytes(payload),
            CreateSource(broadcaster, session.Id, runtime, "host-1"),
            socketCts.Token);

        Assert.Equal(CloseReason.HostStopped, shouldEnd.CloseReason);
        Assert.Equal(SessionStatus.Ended, runtime.Host.Status);

        var queued = await viewerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(queued);
        var message = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(queued.Value.Payload),
            WsJsonContext.Default.SessionEndedMessage);
        Assert.Equal(session.Id.Value.ToString(), message?.SessionId);
        Assert.Equal("host_stopped", message?.Reason);

        var repeated = await processor.ProcessAsync(
            Encoding.UTF8.GetBytes(payload),
            CreateSource(broadcaster, session.Id, runtime, "host-1"),
            CancellationToken.None);

        Assert.Equal(HostProcessOutcomeKind.End, repeated.Kind);
        Assert.Null(await viewerQueue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task HostEnd_Closes_Host_Without_Fanout_When_DurableTransitionFails()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();

        using var services = CreateHostEndServices();
        var processor = CreateProcessor(
            runtimeDirectory,
            broadcaster,
            metrics,
            services.GetRequiredService<IServiceScopeFactory>());
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost(
            "host-1",
            new CancellationTokenSource());
        runtime.Host.SetSessionStartedAt(DateTimeOffset.UtcNow);
        runtime.Host.SetStatus(SessionStatus.Live);
        var viewerQueue = broadcaster.GetOrCreateSession(sessionId).AddParticipantQueue("viewer-1");
        var payload = $$"""{"type":"host.end","sessionId":"{{sessionId.Value}}","reason":"host_stopped"}""";

        var shouldEnd = await processor.ProcessAsync(
            Encoding.UTF8.GetBytes(payload),
            CreateSource(broadcaster, sessionId, runtime, "host-1"),
            CancellationToken.None);

        Assert.Equal(HostProcessOutcomeKind.ProtocolViolation, shouldEnd.Kind);
        Assert.Equal(CloseReason.ServerError, shouldEnd.CloseReason);
        Assert.Equal(SessionStatus.Live, runtime.Host.Status);

        using var timeoutCts = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(() => viewerQueue.ReadAsync(timeoutCts.Token));
    }

    [Theory]
    [InlineData("server_error")]
    [InlineData("auth_revoked")]
    [InlineData("capacity_reached")]
    [InlineData("session_timeout")]
    [InlineData("ended")]
    [InlineData("made_up_reason")]
    public async Task HostEnd_Rejects_Reasons_Outside_Host_Authority(
        string reason)
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var session = CreateSession();
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");

        using var services = CreateHostEndServices(session);
        var processor = CreateProcessor(
            runtimeDirectory,
            broadcaster,
            metrics,
            services.GetRequiredService<IServiceScopeFactory>());
        var runtime = runtimeDirectory.CreateRuntime(session.Id);
        runtime.Host.SetStatus(SessionStatus.Live);
        var viewerQueue = broadcaster.GetOrCreateSession(session.Id).AddParticipantQueue("viewer-1");
        var payload =
            $$"""{"type":"host.end","sessionId":"{{session.Id.Value}}","reason":"{{reason}}"}""";

        var shouldEnd = await processor.ProcessAsync(
            Encoding.UTF8.GetBytes(payload),
            CreateSource(broadcaster, session.Id, runtime),
            CancellationToken.None);

        Assert.Equal(HostProcessOutcomeKind.ProtocolViolation, shouldEnd.Kind);
        Assert.Equal(CloseReason.InvalidMessage, shouldEnd.CloseReason);
        Assert.Equal(SessionStatus.Live, runtime.Host.Status);
        Assert.Equal(SessionStatus.Live, session.Status);
        using var timeoutCts = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(() => viewerQueue.ReadAsync(timeoutCts.Token));
    }

    [Fact]
    public async Task Delayed_Old_KeyRotation_Cannot_Mutate_Republished_Session()
    {
        var fixture = CreateStaleHostFixture();
        var payload = Encoding.UTF8.GetBytes(
            $$"""{"type":"key.rotation","sessionId":"{{fixture.SessionId.Value}}","keyGeneration":1}""");

        var outcome = await fixture.Processor.ProcessAsync(
            payload,
            CreateSource(
                fixture.SessionId,
                fixture.OldRuntime,
                fixture.OldQueues,
                "old-host"),
            TestContext.Current.CancellationToken);

        Assert.Equal(CloseReason.SessionNotLive, outcome.CloseReason);
        Assert.Null(fixture.ReplacementRuntime.Stream.GetReplayState().KeyRotation);
        Assert.Null(fixture.ReplacementQueue.CompletionCause);
    }

    [Fact]
    public async Task Delayed_Old_BinaryFrame_Cannot_Mutate_Republished_Session()
    {
        var fixture = CreateStaleHostFixture();
        var frame = new byte[]
        {
            0x03,
            0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0, 0,
            0xAA,
        };

        var outcome = await fixture.Processor.ProcessBinaryAsync(
            frame,
            CreateSource(
                fixture.SessionId,
                fixture.OldRuntime,
                fixture.OldQueues,
                "old-host"),
            TestContext.Current.CancellationToken);

        Assert.Equal(CloseReason.SessionNotLive, outcome.CloseReason);
        Assert.Null(
            fixture.ReplacementRuntime.Stream.GetReplayState().TerminalCheckpoint);
        Assert.Null(fixture.ReplacementQueue.CompletionCause);
    }

    [Fact]
    public async Task Delayed_Old_KeyRotation_Cannot_Impersonate_Replacement_On_Same_Runtime()
    {
        var fixture = CreateReconnectedHostFixture();
        var payload = Encoding.UTF8.GetBytes(
            $$"""{"type":"key.rotation","sessionId":"{{fixture.SessionId.Value}}","keyGeneration":1}""");

        var outcome = await fixture.Processor.ProcessAsync(
            payload,
            fixture.OldSource,
            TestContext.Current.CancellationToken);

        Assert.Equal(CloseReason.SessionNotLive, outcome.CloseReason);
        Assert.Null(fixture.Runtime.Stream.GetReplayState().KeyRotation);
        Assert.False(fixture.ReplacementViewerQueue.TryTakePendingTerminalMessage(out _));
    }

    [Fact]
    public async Task Delayed_Old_BinaryFrame_Cannot_Impersonate_Replacement_On_Same_Runtime()
    {
        var fixture = CreateReconnectedHostFixture();
        Assert.Equal(
            LiveSessionKeyRotationStoreResult.StoredNewGeneration,
            fixture.Runtime.Stream.TryStoreKeyRotation(1, KeyRotation(1)));
        fixture.Runtime.Participants.TryAddSharedParticipant(
            "replacement-viewer",
            50,
            out _);
        var frame = new byte[]
        {
            0x03,
            0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0, 0,
            0xAA,
        };

        var outcome = await fixture.Processor.ProcessBinaryAsync(
            frame,
            fixture.OldSource,
            TestContext.Current.CancellationToken);

        Assert.Equal(CloseReason.SessionNotLive, outcome.CloseReason);
        Assert.Null(fixture.Runtime.Stream.GetReplayState().TerminalCheckpoint);
        Assert.False(fixture.ReplacementViewerQueue.TryTakePendingTerminalMessage(out _));
    }

    [Fact]
    public async Task Mismatched_Runtime_Incarnation_Is_Rejected_Before_Text_Effect()
    {
        var fixture = CreateReconnectedHostFixture();
        var replacementSource = CreateSource(
            fixture.SessionId,
            fixture.Runtime,
            fixture.Queues,
            "replacement-host") with
        {
            RuntimeIncarnationId = Guid.NewGuid(),
        };
        var payload = Encoding.UTF8.GetBytes(
            $$"""{"type":"key.rotation","sessionId":"{{fixture.SessionId.Value}}","keyGeneration":1}""");

        var outcome = await fixture.Processor.ProcessAsync(
            payload,
            replacementSource,
            TestContext.Current.CancellationToken);

        Assert.Equal(CloseReason.SessionNotLive, outcome.CloseReason);
        Assert.Null(fixture.Runtime.Stream.GetReplayState().KeyRotation);
    }

    private sealed class RecordingSemanticRelayRepository : ISemanticRelayRepository
    {
        public bool StoreResult { get; init; } = true;
        public bool StoreCompleted { get; private set; }
        public (SessionId, Guid, UserId, string, Guid, string, string, string, UserId,
            string, string)? Stored
        { get; private set; }

        public Task<SemanticRequestClaim> ClaimRequestAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            string mode,
            string payloadSha256,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task<SemanticRequestClaim?> FindExactRequestAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            string mode,
            string payloadSha256,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task MarkDispatchedAsync(
            Guid requestRowId,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task<bool> StoreReceiptAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            string mode,
            string payloadSha256,
            string outcome,
            UserId ownerUserId,
            string ownerDeviceId,
            string signature,
            CancellationToken ct = default)
        {
            Stored = (
                sessionId,
                incarnationId,
                requesterUserId,
                requesterDeviceId,
                requestId,
                mode,
                payloadSha256,
                outcome,
                ownerUserId,
                ownerDeviceId,
                signature);
            StoreCompleted = true;
            return Task.FromResult(StoreResult);
        }

        public Task<IReadOnlyList<SemanticRelayReceipt>> ListPendingReceiptsForDeviceAsync(
            UserId requesterUserId,
            string requesterDeviceId,
            int limit,
            SemanticReceiptCursor? cursor = null,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task<IReadOnlyList<SemanticRelayReceipt>> ListPendingReceiptsAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            int limit,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<SemanticRelayReceipt>>([]);

        public Task<bool> AcknowledgeReceiptAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task<int> DeleteAcknowledgedBeforeAsync(
            DateTimeOffset cutoff,
            int limit,
            CancellationToken ct = default) => throw new NotSupportedException();
    }

    private static SemanticReceiptVerifier CreateSemanticReceiptVerifier(
        UserId userId,
        string deviceId,
        byte[] signingPublicKey)
    {
        var now = DateTimeOffset.UtcNow;
        var device = TestDeviceCertificate.CreateDevice(
            userId,
            deviceId,
            kemPublicKey: new byte[1184],
            signingPublicKey: signingPublicKey,
            deviceLabel: "Host device",
            signerDeviceId: deviceId,
            issuedAt: now.AddMinutes(-1),
            expiresAt: null);
        var deviceList = TestDeviceList.Create(
            userId,
            1,
            $$"""[{"deviceId":"{{deviceId}}","signerDeviceId":"{{deviceId}}"}]""",
            deviceId,
            [2],
            now.AddMinutes(-1).ToUnixTimeMilliseconds(),
            null);
        return new SemanticReceiptVerifier(
            new FakeUserDeviceRepository(device),
            new FakeUserDeviceListRepository(deviceList),
            new MLDsaPopSignatureVerifier(),
            TimeProvider.System);
    }

    private static string SignSemanticReceipt(
        MLDsa signingKey,
        SessionId sessionId,
        Guid incarnationId,
        Guid requestId,
        string mode,
        string payloadSha256,
        string outcome,
        UserId userId,
        string requesterDeviceId,
        string ownerDeviceId)
    {
        var preimage = SemanticReceiptPreimage.Create(
            sessionId,
            incarnationId,
            requestId,
            mode,
            payloadSha256,
            outcome,
            userId,
            requesterDeviceId,
            userId,
            ownerDeviceId);
        var signature = new byte[MLDsaAlgorithm.MLDsa65.SignatureSizeInBytes];
        signingKey.SignData(preimage, signature, context: ReadOnlySpan<byte>.Empty);
        return Convert.ToBase64String(signature);
    }

    private sealed class FakeScopeFactory(
        ISemanticReceiptVerifier? verifier = null) : IServiceScopeFactory
    {
        private readonly ISemanticReceiptVerifier _verifier =
            verifier ?? new FixedSemanticReceiptVerifier(true);

        public IServiceScope CreateScope() => new FakeScope(_verifier);
    }

    private sealed class FakeScope(ISemanticReceiptVerifier verifier) : IServiceScope
    {
        public IServiceProvider ServiceProvider { get; } = new FakeServiceProvider(verifier);
        public void Dispose() { }
    }

    private sealed class FakeServiceProvider(ISemanticReceiptVerifier verifier) : IServiceProvider
    {
        public object? GetService(Type serviceType) =>
            serviceType == typeof(ISemanticReceiptVerifier) ? verifier : null;
    }

    private sealed class FixedSemanticReceiptVerifier(bool result) : ISemanticReceiptVerifier
    {
        public Task<bool> VerifyAsync(
            UserId authenticatedUserId,
            string authenticatedDeviceId,
            SemanticReceiptEnvelope receipt,
            CancellationToken ct = default) => Task.FromResult(result);
    }

    private static async Task<HostProcessOutcome> ProcessBinaryAsync(
        HostMessageProcessor processor,
        SessionBroadcaster broadcaster,
        byte[] frame,
        SessionId sessionId,
        LiveSessionPorts runtime)
    {
        if (runtime.Host.HostConnectionId is null)
        {
            Assert.True(runtime.Host.TryClaimHost(
                "test-host",
                new CancellationTokenSource()));
        }
        var queues = broadcaster.GetOrCreateSession(sessionId);
        return await processor.ProcessBinaryAsync(
            frame,
            CreateSource(sessionId, runtime, queues),
            TestContext.Current.CancellationToken);
    }

    private static HostMessageSource CreateSource(
        SessionBroadcaster broadcaster,
        SessionId sessionId,
        LiveSessionPorts runtime,
        string? connectionId = null) =>
        CreateSource(
            sessionId,
            runtime,
            broadcaster.GetOrCreateSession(sessionId),
            connectionId);

    private static HostMessageSource CreateSource(
        SessionId sessionId,
        LiveSessionPorts runtime,
        SessionSendQueues queues,
        string? connectionId = null) =>
        new(
            sessionId,
            connectionId ?? runtime.Host.HostConnectionId ?? string.Empty,
            runtime,
            queues,
            runtime.IncarnationId);

    private static HostMessageProcessor CreateProcessor(
        ILiveSessionStateDirectory runtimeDirectory,
        SessionBroadcaster broadcaster,
        OperationalMetrics metrics,
        IServiceScopeFactory? scopeFactory = null,
        ISemanticRelayRepository? semanticRelay = null,
        IConnectionRegistry? connections = null,
        IActionDedupeCache? dedupeCache = null,
        IPermissionDecisionAuditStore? permissionDecisionAuditStore = null)
    {
        scopeFactory ??= new FakeScopeFactory();
        semanticRelay ??= UnsupportedSemanticRelayRepository.Instance;
        connections ??= new ConnectionRegistry();
        dedupeCache ??= new ActionDedupeCache();
        permissionDecisionAuditStore ??=
            UnsupportedPermissionDecisionAuditStore.Instance;
        var lifecycleGate = new SessionLifecycleGate();
        var teardown = new HostTeardown(
            new ConnectionRegistry(),
            runtimeDirectory,
            broadcaster,
            scopeFactory,
            metrics,
            lifecycleGate,
            NullLogger<HostTeardown>.Instance,
            repairTracker: new RealtimePersistenceRepairTracker(TimeProvider.System));
        var sessionEndCoordinator = new SessionEndCoordinator(
            scopeFactory,
            runtimeDirectory,
            broadcaster,
            teardown,
            lifecycleGate,
            NullLogger<SessionEndCoordinator>.Instance);
        return new HostMessageProcessor(
            broadcaster,
            sessionEndCoordinator,
            lifecycleGate,
            runtimeDirectory,
            metrics,
            NullLogger<HostMessageProcessor>.Instance,
            semanticRelay,
            connections,
            dedupeCache,
            permissionDecisionAuditStore,
            scopeFactory);
    }

    private static StaleHostFixture CreateStaleHostFixture()
    {
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimes, broadcaster, metrics);
        var sessionId = SessionId.New();
        var oldRuntime = runtimes.CreateRuntime(sessionId);
        Assert.True(oldRuntime.Host.TryClaimHost(
            "old-host",
            new CancellationTokenSource()));
        var oldQueues = broadcaster.GetOrCreateSession(sessionId);
        oldQueues.SetHostQueue("old-host");
        Assert.True(runtimes.RemoveIfSame(sessionId, oldRuntime));
        Assert.True(broadcaster.RemoveSessionIfSame(sessionId, oldQueues));
        var replacementRuntime = runtimes.CreateRuntime(sessionId);
        Assert.True(replacementRuntime.Host.TryClaimHost(
            "replacement-host",
            new CancellationTokenSource()));
        var replacementQueues = broadcaster.GetOrCreateSession(sessionId);
        replacementQueues.SetHostQueue("replacement-host");
        var replacementQueue =
            replacementQueues.AddParticipantQueue("replacement-viewer");
        return new StaleHostFixture(
            processor,
            sessionId,
            oldRuntime,
            oldQueues,
            replacementRuntime,
            replacementQueue);
    }

    private static ReconnectedHostFixture CreateReconnectedHostFixture()
    {
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var processor = CreateProcessor(runtimes, broadcaster, metrics);
        var sessionId = SessionId.New();
        var runtime = runtimes.CreateRuntime(sessionId);
        Assert.True(runtime.Host.TryClaimHost(
            "old-host",
            new CancellationTokenSource()));
        var queues = broadcaster.GetOrCreateSession(sessionId);
        queues.SetHostQueue("old-host");
        var oldSource = CreateSource(
            sessionId,
            runtime,
            queues,
            "old-host");
        runtime.Host.ReleaseHost("old-host");
        Assert.True(runtime.Host.TryClaimHost(
            "replacement-host",
            new CancellationTokenSource()));
        queues.SetHostQueue("replacement-host");
        var replacementViewerQueue =
            queues.AddParticipantQueue("replacement-viewer");
        return new ReconnectedHostFixture(
            processor,
            sessionId,
            runtime,
            queues,
            oldSource,
            replacementViewerQueue);
    }

    private sealed class ActionResultCompletionStore(
        UserId requesterUserId,
        string actionId,
        PermissionDecisionPendingTuple canonicalTuple,
        HostActionCompletionOutcome outcome = HostActionCompletionOutcome.Applied) : IPermissionDecisionAuditStore
    {
        public int CompletionCalls { get; private set; }
        public InputAuditStatus? TerminalStatus { get; private set; }

        public Task<PermissionDecisionAdmissionResult> AdmitAsync(
            SessionId sessionId,
            UserId admittedRequesterUserId,
            string admittedActionId,
            string auditPayload,
            PermissionDecisionPendingTuple pendingTuple,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<bool> MarkDispatchFailedAsync(
            SessionId sessionId,
            UserId admittedRequesterUserId,
            string admittedActionId,
            PermissionDecisionPendingTuple pendingTuple,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<HostActionCompletionResult> CompleteHostActionAsync(
            SessionId sessionId,
            UserId assertedRequesterUserId,
            string assertedActionId,
            PermissionDecisionPendingTuple assertedTuple,
            InputAuditStatus terminalStatus,
            CancellationToken ct = default)
        {
            CompletionCalls++;
            if (outcome == HostActionCompletionOutcome.Applied)
            {
                TerminalStatus = terminalStatus;
            }
            return Task.FromResult(new HostActionCompletionResult(
                outcome,
                requesterUserId,
                actionId,
                canonicalTuple));
        }
    }

    private sealed record StaleHostFixture(
        HostMessageProcessor Processor,
        SessionId SessionId,
        LiveSessionPorts OldRuntime,
        SessionSendQueues OldQueues,
        LiveSessionPorts ReplacementRuntime,
        RelayClientSendQueue ReplacementQueue);

    private sealed record ReconnectedHostFixture(
        HostMessageProcessor Processor,
        SessionId SessionId,
        LiveSessionPorts Runtime,
        SessionSendQueues Queues,
        HostMessageSource OldSource,
        RelayClientSendQueue ReplacementViewerQueue);

    private static ServiceProvider CreateHostEndServices(Session? session = null)
    {
        var metrics = new OperationalMetrics(new LiveSessionStateDirectory());
        return new ServiceCollection()
            .AddSingleton(new UserEventBroadcaster(
                metrics,
                NullLogger<UserEventBroadcaster>.Instance))
            .AddScoped<ISessionRepository>(_ => new FakeSessionRepository(session))
            .AddScoped<ISessionKeyBlobRepository>(_ => new FakeSessionKeyBlobRepository())
            .AddScoped<ISessionEndMutationRepository>(
                _ => new FakeSessionEndMutationRepository())
            .AddScoped<IFriendshipRepository>(_ => new FakeFriendshipRepository())
            .AddScoped<IOwnerSessionSecretHasher, OwnerSessionSecretHasher>()
            .AddScoped<IUnitOfWork, FakeUnitOfWork>()
            .AddScoped<IRoomMemberRepository>(_ => new FakeRoomMemberRepository())
            .AddScoped<ISessionViewerDismissalRepository>(
                _ => new FakeSessionViewerDismissalRepository())
            .AddScoped<DiscoveryAudienceResolver>()
            .AddScoped<SharedSurfaceEventPublisher>()
            .AddScoped<LiveSessionTransitionOrchestrator>()
            .AddScoped<LiveSessionStatusReader>()
            .AddScoped<LiveSessionTerminator>()
            .BuildServiceProvider();
    }

    private static Session CreateSession()
    {
        return Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "Test",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            new OwnerSessionSecretHasher().Hash("owner-secret"));
    }

    private static byte[] KeyRotation(uint keyGeneration)
        => [(byte)keyGeneration];

    private sealed class FakeSessionRepository : SessionRepositoryStub
    {
        private readonly Dictionary<SessionId, Session> _sessions = new();

        public FakeSessionRepository(Session? session = null)
        {
            if (session is not null)
            {
                _sessions[session.Id] = session;
            }
        }

        public override Task<Session?> GetByIdAsync(SessionId id, CancellationToken ct = default)
        {
            _sessions.TryGetValue(id, out var session);
            return Task.FromResult(session);
        }

        public override Task<Session?> GetByIdForUpdateAsync(
            SessionId id,
            CancellationToken ct = default)
        {
            _sessions.TryGetValue(id, out var session);
            return Task.FromResult(session);
        }

        public override Task AddAsync(Session session, CancellationToken ct = default)
        {
            _sessions[session.Id] = session;
            return Task.CompletedTask;
        }

        public override Task UpdateAsync(Session session, CancellationToken ct = default)
        {
            _sessions[session.Id] = session;
            return Task.CompletedTask;
        }

        public override Task<IReadOnlyList<SessionCardProjection>> GetByOwnerProjectedAsync(UserId ownerUserId, CancellationToken ct = default)
            => throw new NotSupportedException();

        public override Task<IReadOnlyList<SessionCardProjection>> GetLiveByRoomProjectedAsync(
            RoomId roomId, FeedCursor? cursor = null, int limit = 20,
            ToolKind? toolKindFilter = null, DateTimeOffset? since = null, CancellationToken ct = default)
            => throw new NotSupportedException();

    }

    private sealed class FakeUnitOfWork : UnitOfWorkStub
    {
        public override Task SaveChangesAsync(CancellationToken ct = default) => Task.CompletedTask;

        public override Task<ITransactionScope> BeginTransactionAsync(
            CancellationToken ct = default) =>
            Task.FromResult<ITransactionScope>(new CompletedTransactionScope());
    }

    private sealed class FakeFriendshipRepository : IFriendshipRepository
    {
        public Task<bool> AreFriendsAsync(UserId userA, UserId userB, CancellationToken ct = default)
            => Task.FromResult(false);

        public Task<IReadOnlyList<UserId>> GetFriendIdsAsync(UserId userId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<UserId>>([]);

        public Task<IReadOnlyList<Friendship>> GetPendingIncomingAsync(UserId userId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<Friendship>>([]);

        public Task<IReadOnlyList<Friendship>> GetPendingOutgoingAsync(UserId userId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<Friendship>>([]);

        public Task<Friendship?> GetAsync(UserId userA, UserId userB, CancellationToken ct = default)
            => Task.FromResult<Friendship?>(null);

        public Task AddAsync(Friendship friendship, CancellationToken ct = default)
            => Task.CompletedTask;

        public void Remove(Friendship friendship) { }
    }

    private sealed class FakeRoomMemberRepository : IRoomMemberRepository
    {
        public Task<bool> IsMemberAsync(RoomId roomId, UserId userId, CancellationToken ct = default)
            => Task.FromResult(false);

        public Task<IReadOnlyList<RoomId>> GetActiveRoomIdsForUserAsync(
            UserId userId,
            IReadOnlyList<RoomId> roomIds,
            CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<RoomId>>([]);

        public Task<RoomMember?> GetAsync(RoomId roomId, UserId userId, CancellationToken ct = default)
            => Task.FromResult<RoomMember?>(null);

        public Task AddAsync(RoomMember member, CancellationToken ct = default)
            => Task.CompletedTask;

        public Task<IReadOnlyList<UserId>> GetMemberUserIdsAsync(RoomId roomId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<UserId>>([]);

        public Task<IReadOnlyList<RoomMember>> GetActiveByRoomAsync(
            RoomId roomId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<RoomMember>> GetAdmissionProofPageAfterAsync(
            RoomId roomId,
            Guid? afterUserId,
            int limit,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<UserId>> GetRoomPeerUserIdsAsync(
            UserId userId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
    }
}
