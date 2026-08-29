using System.Text;
using System.Text.Json;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging.Abstractions;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Realtime;

using Kodosi.Host.Serialization;

namespace Kodosi.HostTests;

public sealed class RelayMessageProcessorTests
{
    [Fact]
    public async Task Semantic_Send_Forwards_Exact_Tuple_And_Marks_Dispatched()
    {
        var fixture = CreateSemanticSendFixture();
        var message = fixture.Message;

        var outcome = await fixture.Processor.ProcessAsync(
            JsonSerializer.Serialize(
                message,
                WsJsonContext.Default.ParticipantSemanticSendMessage),
            fixture.Context,
            TestContext.Current.CancellationToken);

        Assert.Equal(RelayMessageProcessOutcome.Handled, outcome);
        var hostBytes = await fixture.HostQueue.ReadAsync(
            TestContext.Current.CancellationToken);
        var hostMessage = JsonSerializer.Deserialize(
            hostBytes!,
            WsJsonContext.Default.HostSemanticSendMessage);
        Assert.Equal(message.RequestId, hostMessage?.RequestId);
        Assert.Equal(fixture.Context.UserId.Value.ToString(), hostMessage?.SenderUserId);
        Assert.Equal(fixture.Context.DeviceId, hostMessage?.SenderDeviceId);
        Assert.True(fixture.Repository.Dispatched);
    }

    [Fact]
    public async Task Semantic_Send_Wrong_Incarnation_Is_Never_Claimed()
    {
        var fixture = CreateSemanticSendFixture();
        var message = fixture.Message with
        {
            IncarnationId = Guid.CreateVersion7(),
        };

        var outcome = await fixture.Processor.ProcessAsync(
            JsonSerializer.Serialize(
                message,
                WsJsonContext.Default.ParticipantSemanticSendMessage),
            fixture.Context,
            TestContext.Current.CancellationToken);

        Assert.Equal(RelayMessageProcessOutcome.Handled, outcome);
        Assert.False(fixture.Repository.HasRequest);
        Assert.False(fixture.Repository.Dispatched);
        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => fixture.HostQueue.ReadAsync(timeout.Token));
    }

    [Fact]
    public async Task Semantic_Send_Exact_Duplicate_May_Redispatch_But_Conflict_Does_Not()
    {
        var fixture = CreateSemanticSendFixture();
        var json = JsonSerializer.Serialize(
            fixture.Message,
            WsJsonContext.Default.ParticipantSemanticSendMessage);

        Assert.Equal(
            RelayMessageProcessOutcome.Handled,
            await fixture.Processor.ProcessAsync(
                json,
                fixture.Context,
                TestContext.Current.CancellationToken));
        _ = await fixture.HostQueue.ReadAsync(TestContext.Current.CancellationToken);
        Assert.Equal(
            RelayMessageProcessOutcome.Handled,
            await fixture.Processor.ProcessAsync(
                json,
                fixture.Context,
                TestContext.Current.CancellationToken));
        var duplicateBytes = await fixture.HostQueue.ReadAsync(
            TestContext.Current.CancellationToken);
        Assert.NotNull(duplicateBytes);

        var conflicting = fixture.Message with
        {
            PayloadSha256 = new string('b', 64),
        };
        Assert.Equal(
            RelayMessageProcessOutcome.Handled,
            await fixture.Processor.ProcessAsync(
                JsonSerializer.Serialize(
                    conflicting,
                    WsJsonContext.Default.ParticipantSemanticSendMessage),
                fixture.Context,
                TestContext.Current.CancellationToken));
        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => fixture.HostQueue.ReadAsync(timeout.Token));
        Assert.Equal(2, fixture.Repository.DispatchCount);
    }

    [Fact]
    public async Task Semantic_Send_Pending_Claim_Can_Retry_After_Closed_Host_Queue()
    {
        var fixture = CreateSemanticSendFixture(hostQueueInitiallyOpen: false);
        var json = JsonSerializer.Serialize(
            fixture.Message,
            WsJsonContext.Default.ParticipantSemanticSendMessage);

        Assert.Equal(
            RelayMessageProcessOutcome.Handled,
            await fixture.Processor.ProcessAsync(
                json,
                fixture.Context,
                TestContext.Current.CancellationToken));
        Assert.False(fixture.Repository.Dispatched);

        var hostQueue = fixture.Queues.SetHostQueue("host");
        Assert.Equal(
            RelayMessageProcessOutcome.Handled,
            await fixture.Processor.ProcessAsync(
                json,
                fixture.Context,
                TestContext.Current.CancellationToken));
        Assert.NotNull(await hostQueue.ReadAsync(TestContext.Current.CancellationToken));
        Assert.True(fixture.Repository.Dispatched);
    }

    [Fact]
    public async Task Semantic_Receipt_Ack_Verifies_Exact_Device_Before_Mailbox_Acknowledgement()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var semanticRelay = new RecordingSemanticAckRepository();
        var userId = UserId.New();
        const string deviceId = "device-1";
        var device = TestDeviceCertificate.CreateDevice(
            userId,
            deviceId,
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Device",
            signerDeviceId: deviceId,
            issuedAt: DateTimeOffset.UtcNow.AddMinutes(-1),
            expiresAt: null);
        var deviceList = TestDeviceList.Create(
            userId,
            1,
            $$"""[{"deviceId":"{{deviceId}}","signerDeviceId":"{{deviceId}}"}]""",
            deviceId,
            [2],
            DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds(),
            null);
        using var services = new ServiceCollection()
            .AddSingleton(TimeProvider.System)
            .AddScoped<IUserDeviceRepository>(_ => new FakeUserDeviceRepository(device))
            .AddScoped<IUserDeviceListRepository>(_ =>
                new FakeUserDeviceListRepository(deviceList))
            .AddSingleton<IPopSignatureVerifier>(new AlwaysValidSignatureVerifier())
            .AddScoped<SemanticReceiptAckVerifier>()
            .BuildServiceProvider();
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            new RecordingAuditWriter(),
            semanticRelay,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            services.GetRequiredService<IServiceScopeFactory>(),
            UnsupportedPermissionDecisionAuditStore.Instance);
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var requestId = Guid.CreateVersion7();
        var nextRequestId = Guid.CreateVersion7();
        semanticRelay.PendingReceipts = [SemanticRelayReceipt.Create(
            Guid.NewGuid(),
            sessionId,
            incarnationId,
            userId,
            deviceId,
            nextRequestId,
            "steer",
            new string('b', 64),
            "injected",
            userId,
            "owner-device",
            Convert.ToBase64String([2]),
            DateTimeOffset.UtcNow)];
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        var participantQueue = broadcaster.GetOrCreateSession(sessionId).AddParticipantQueue("owner");
        var message = new ParticipantSemanticReceiptAckMessage(
            sessionId.Value.ToString(),
            incarnationId,
            requestId,
            userId.Value.ToString(),
            deviceId,
            Convert.ToBase64String([1]));

        var outcome = await processor.ProcessAsync(
            JsonSerializer.Serialize(
                message,
                WsJsonContext.Default.ParticipantSemanticReceiptAckMessage),
            new RelayMessageContext(
                sessionId,
                "owner",
                userId,
                AccessLevel.Approve,
                IsOwnerParticipant: true,
                runtime.Host,
                runtime.Participants,
                AllowingParticipantActionAuthority.Instance,
                deviceId,
                incarnationId),
            TestContext.Current.CancellationToken);

        Assert.Equal(RelayMessageProcessOutcome.Handled, outcome);
        Assert.Equal(
            (sessionId, incarnationId, userId, deviceId, requestId),
            semanticRelay.Acknowledged);
        Assert.Equal(1, semanticRelay.LastListLimit);
        Assert.Equal(["acknowledge", "list"], semanticRelay.Calls);
        var nextBytes = await participantQueue.ReadAsync(TestContext.Current.CancellationToken);
        var next = JsonSerializer.Deserialize(
            nextBytes!.Value.Payload,
            WsJsonContext.Default.ParticipantSemanticReceiptMessage);
        Assert.Equal(nextRequestId, next?.RequestId);
    }

    [Fact]
    public async Task Semantic_Receipt_Ack_Target_Mismatch_Does_Not_Touch_Mailbox()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var semanticRelay = new RecordingSemanticAckRepository();
        var processor = new RelayMessageProcessor(
            new SessionBroadcaster(
                runtimeDirectory,
                metrics,
                NullLoggerFactory.Instance),
            new ActionDedupeCache(),
            new RecordingAuditWriter(),
            semanticRelay,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var userId = UserId.New();
        var incarnationId = Guid.CreateVersion7();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        var message = new ParticipantSemanticReceiptAckMessage(
            sessionId.Value.ToString(),
            incarnationId,
            Guid.CreateVersion7(),
            userId.Value.ToString(),
            "other-device",
            Convert.ToBase64String([1]));

        var outcome = await processor.ProcessAsync(
            JsonSerializer.Serialize(
                message,
                WsJsonContext.Default.ParticipantSemanticReceiptAckMessage),
            new RelayMessageContext(
                sessionId,
                "owner",
                userId,
                AccessLevel.Approve,
                IsOwnerParticipant: true,
                runtime.Host,
                runtime.Participants,
                AllowingParticipantActionAuthority.Instance,
                "device-1",
                incarnationId),
            TestContext.Current.CancellationToken);

        Assert.Equal(RelayMessageProcessOutcome.ProtocolViolation, outcome);
        Assert.Null(semanticRelay.Acknowledged);
    }

    [Fact]
    public async Task FocusChanged_Rejects_When_Transformed_Host_Message_Exceeds_Limit()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new RecordingAuditWriter();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("test-host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);
        var hostQueue = broadcaster.GetOrCreateSession(sessionId).SetHostQueue();
        var connectionId = new string('c', 128);
        var userId = UserId.New();
        var deviceId = new string('d', 256);
        var message = new ParticipantFocusChangedMessage(
            "focus-action",
            true,
            "nonce",
            new string('x', 600),
            "signature");
        var json = JsonSerializer.Serialize(
            message,
            WsJsonContext.Default.ParticipantFocusChangedMessage);
        var inputBytes = Encoding.UTF8.GetByteCount(json);
        var transformedBytes = RelayOutbound.Encode(new HostFocusChangedMessage(
            sessionId.Value.ToString(),
            message.ActionId,
            connectionId,
            message.Focused!.Value,
            message.Nonce,
            message.Ciphertext,
            userId.Value.ToString(),
            deviceId,
            message.Signature));
        Assert.InRange(
            inputBytes,
            1,
            RelayMessageLimits.GetMaxBytes("participant.focusChanged"));
        Assert.True(
            transformedBytes.Length
                > RelayMessageLimits.GetMaxBytes("host.focusChanged"));
        var context = new RelayMessageContext(
            sessionId,
            connectionId,
            userId,
            AccessLevel.Inject,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            deviceId);

        var outcome = await processor.ProcessAsync(
            json,
            context,
            TestContext.Current.CancellationToken);

        Assert.Equal(RelayMessageProcessOutcome.ProtocolViolation, outcome);
        Assert.Empty(auditWriter.Appends);
        using var readCts = new CancellationTokenSource(TimeSpan.FromMilliseconds(20));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => hostQueue.ReadAsync(readCts.Token));
    }

    [Fact]
    public async Task ProcessAsync_Rejects_Participant_Message_Above_PerType_Limit()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var processor = new RelayMessageProcessor(
            new SessionBroadcaster(
                runtimeDirectory,
                metrics,
                NullLoggerFactory.Instance),
            new ActionDedupeCache(),
            new RecordingAuditWriter(),
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        var payload =
            $$"""{"type":"participant.focusChanged","padding":"{{new string('x', RelayMessageLimits.GetMaxBytes("participant.focusChanged"))}}"}""";
        Assert.InRange(
            Encoding.UTF8.GetByteCount(payload),
            RelayMessageLimits.GetMaxBytes("participant.focusChanged") + 1,
            RelayMessageLimits.OuterMessageMaxBytes);
        var context = new RelayMessageContext(
            sessionId,
            "viewer",
            UserId.New(),
            AccessLevel.Inject,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance);

        var outcome = await processor.ProcessAsync(
            payload,
            context,
            TestContext.Current.CancellationToken);

        Assert.Equal(RelayMessageProcessOutcome.ProtocolViolation, outcome);
    }

    [Fact]
    public async Task Suggest_Is_Accepted_And_Updates_Audit_When_Owner_Queue_Accepts()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new RecordingAuditWriter();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("test-host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var viewerQueue = queues.AddParticipantQueue("viewer-1");
        var ownerQueue = queues.SetHostQueue();

        var context = new RelayMessageContext(
            sessionId,
            "viewer-1",
            UserId.New(),
            AccessLevel.Suggest,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: viewerQueue);

        await processor.ProcessAsync(
            """{"type":"participant.suggest","actionId":"action-accept","nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}""",
            context,
            CancellationToken.None);

        var viewerMessage = await viewerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(viewerMessage);

        var result = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(viewerMessage.Value.Payload),
            WsJsonContext.Default.ActionResultMessage);

        Assert.Equal(RelayActionStatus.Accepted, result?.Status);
        Assert.Equal(sessionId.Value.ToString(), result?.SessionId);
        Assert.NotNull(await ownerQueue.ReadAsync(CancellationToken.None));
        Assert.Equal([(InputAuditKind.Suggestion, InputAuditStatus.Pending)], auditWriter.Appends);
        Assert.Equal([InputAuditStatus.Dispatched], auditWriter.Updates);
    }

    [Fact]
    public async Task Suggest_Is_Rejected_When_Runtime_Is_Reconnecting()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new RecordingAuditWriter();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("test-host", new CancellationTokenSource());
        runtime.Host.SetStatus(SessionStatus.Reconnecting);

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var viewerQueue = queues.AddParticipantQueue("viewer-1");
        queues.SetHostQueue();

        var context = new RelayMessageContext(
            sessionId,
            "viewer-1",
            UserId.New(),
            AccessLevel.Suggest,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: viewerQueue);

        await processor.ProcessAsync(
            """{"type":"participant.suggest","actionId":"action-1","nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}""",
            context,
            CancellationToken.None);

        var message = await viewerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(message);

        var result = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(message.Value.Payload),
            WsJsonContext.Default.ActionResultMessage);

        Assert.Equal(RelayActionStatus.Rejected, result?.Status);
        Assert.Equal(1, metrics.Snapshot().ActionRejectCount);
        Assert.Equal([(InputAuditKind.Suggestion, InputAuditStatus.Rejected)], auditWriter.Appends);
    }

    [Fact]
    public async Task Suggest_Duplicate_Replays_Accepted_And_Writes_One_Duplicate_Audit_Row()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new RecordingAuditWriter();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("test-host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var viewerQueue = queues.AddParticipantQueue("viewer-1");
        var ownerQueue = queues.SetHostQueue();

        var context = new RelayMessageContext(
            sessionId,
            "viewer-1",
            UserId.New(),
            AccessLevel.Suggest,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: viewerQueue);

        const string message = """{"type":"participant.suggest","actionId":"action-dup","nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}""";

        await processor.ProcessAsync(message, context, CancellationToken.None);
        _ = await viewerQueue.ReadAsync(CancellationToken.None);
        _ = await ownerQueue.ReadAsync(CancellationToken.None);

        await processor.ProcessAsync(message, context, CancellationToken.None);
        var duplicateMessage = await viewerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(duplicateMessage);
        var duplicateResult = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(duplicateMessage.Value.Payload),
            WsJsonContext.Default.ActionResultMessage);
        Assert.Equal(RelayActionStatus.Accepted, duplicateResult?.Status);

        await processor.ProcessAsync(message, context, CancellationToken.None);
        _ = await viewerQueue.ReadAsync(CancellationToken.None);
        await processor.ProcessAsync(message, context, CancellationToken.None);
        _ = await viewerQueue.ReadAsync(CancellationToken.None);

        Assert.Equal(
            [
                (InputAuditKind.Suggestion, InputAuditStatus.Pending),
                (InputAuditKind.Suggestion, InputAuditStatus.Duplicate),
            ],
            auditWriter.Appends);
        Assert.Equal([InputAuditStatus.Dispatched], auditWriter.Updates);

        using var ownerReadCts = new CancellationTokenSource(TimeSpan.FromMilliseconds(50));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => ownerQueue.ReadAsync(ownerReadCts.Token));
    }

    [Fact]
    public async Task Suggest_Reused_Rejected_ActionId_Can_Succeed_After_Access_Upgrade()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new RecordingAuditWriter();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("test-host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var viewerQueue = queues.AddParticipantQueue("viewer-1");
        var ownerQueue = queues.SetHostQueue();
        var userId = UserId.New();
        var rejectedContext = new RelayMessageContext(
            sessionId,
            "viewer-1",
            userId,
            AccessLevel.View,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: viewerQueue);
        var allowedContext = new RelayMessageContext(
            sessionId,
            "viewer-1",
            userId,
            AccessLevel.Suggest,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: viewerQueue);

        const string message = """{"type":"participant.suggest","actionId":"action-rejected-dup","nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}""";

        await processor.ProcessAsync(message, rejectedContext, CancellationToken.None);

        var rejectedMessage = await viewerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(rejectedMessage);

        var rejectedResult = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(rejectedMessage.Value.Payload),
            WsJsonContext.Default.ActionResultMessage);

        Assert.Equal(RelayActionStatus.Rejected, rejectedResult?.Status);

        await processor.ProcessAsync(message, allowedContext, CancellationToken.None);

        var acceptedMessage = await viewerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(acceptedMessage);

        var acceptedResult = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(acceptedMessage.Value.Payload),
            WsJsonContext.Default.ActionResultMessage);

        Assert.Equal(RelayActionStatus.Accepted, acceptedResult?.Status);
        Assert.Equal(
            [
                (InputAuditKind.Suggestion, InputAuditStatus.Rejected),
                (InputAuditKind.Suggestion, InputAuditStatus.Pending),
            ],
            auditWriter.Appends);
        Assert.Equal([InputAuditStatus.Dispatched], auditWriter.Updates);
        Assert.NotNull(await ownerQueue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task Inject_Is_Accepted_And_Updates_Audit_When_Owner_Queue_Accepts()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new RecordingAuditWriter();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("test-host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var viewerQueue = queues.AddParticipantQueue("viewer-1");
        var ownerQueue = queues.SetHostQueue();

        var context = new RelayMessageContext(
            sessionId,
            "viewer-1",
            UserId.New(),
            AccessLevel.Inject,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: viewerQueue);

        await processor.ProcessAsync(
            """{"type":"participant.inject","actionId":"inject-accept","nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}""",
            context,
            CancellationToken.None);

        var viewerMessage = await viewerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(viewerMessage);

        var result = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(viewerMessage.Value.Payload),
            WsJsonContext.Default.ActionResultMessage);

        Assert.Equal(RelayActionStatus.Accepted, result?.Status);

        var ownerMessage = await ownerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(ownerMessage);

        var hostInput = JsonSerializer.Deserialize(
            ownerMessage!,
            WsJsonContext.Default.HostInputMessage);

        Assert.Equal("inject-accept", hostInput?.CommandId);
        Assert.Equal("nonce", hostInput?.Nonce);
        Assert.Equal("ciphertext", hostInput?.Ciphertext);
        Assert.Equal([(InputAuditKind.Inject, InputAuditStatus.Pending)], auditWriter.Appends);
        Assert.Equal([InputAuditStatus.Dispatched], auditWriter.Updates);
    }

    [Fact]
    public async Task Approver_Cannot_Inject_Arbitrary_Terminal_Input()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            new RecordingAuditWriter(),
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("test-host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var approverQueue = queues.AddParticipantQueue("approver-1", AccessLevel.Approve);
        var ownerQueue = queues.SetHostQueue();
        var context = new RelayMessageContext(
            sessionId,
            "approver-1",
            UserId.New(),
            AccessLevel.Approve,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: approverQueue);

        await processor.ProcessAsync(
            """{"type":"participant.inject","actionId":"approver-inject","nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}""",
            context,
            CancellationToken.None);

        var resultMessage = await approverQueue.ReadAsync(CancellationToken.None);
        var result = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(resultMessage!.Value.Payload),
            WsJsonContext.Default.ActionResultMessage);
        Assert.Equal(RelayActionStatus.Rejected, result?.Status);
        using var ownerReadCts = new CancellationTokenSource(TimeSpan.FromMilliseconds(50));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => ownerQueue.ReadAsync(ownerReadCts.Token));
    }

    [Fact]
    public async Task Inject_Is_Busy_When_Audit_Append_Does_Not_Persist()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new FailingAuditWriter();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("test-host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var viewerQueue = queues.AddParticipantQueue("viewer-1");
        var ownerQueue = queues.SetHostQueue();

        var context = new RelayMessageContext(
            sessionId,
            "viewer-1",
            UserId.New(),
            AccessLevel.Inject,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: viewerQueue);

        await processor.ProcessAsync(
            """{"type":"participant.inject","actionId":"action-2","nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}""",
            context,
            CancellationToken.None);

        var viewerMessage = await viewerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(viewerMessage);

        var result = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(viewerMessage.Value.Payload),
            WsJsonContext.Default.ActionResultMessage);

        Assert.Equal(RelayActionStatus.Busy, result?.Status);
        Assert.Equal(1, metrics.Snapshot().ActionRejectCount);
        Assert.Equal(1, metrics.Snapshot().QueueOverflowCount);

        using var ownerReadCts = new CancellationTokenSource(TimeSpan.FromMilliseconds(50));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => ownerQueue.ReadAsync(ownerReadCts.Token));
    }

    [Fact]
    public async Task Resize_Waits_For_Host_Result_After_Owner_Queue_Accepts()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new RecordingAuditWriter();
        var hostResults = new RecordingPermissionDecisionAuditStore();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: hostResults);
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("test-host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var viewerQueue = queues.AddParticipantQueue("viewer-1");
        var ownerQueue = queues.SetHostQueue();

        var context = new RelayMessageContext(
            sessionId,
            "viewer-1",
            UserId.New(),
            AccessLevel.Inject,
            IsOwnerParticipant: true,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: viewerQueue);

        await processor.ProcessAsync(
            """{"type":"participant.resize","actionId":"resize-accept","rows":24,"cols":80,"claim":false,"nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}""",
            context,
            CancellationToken.None);

        using (var viewerReadCts = new CancellationTokenSource(TimeSpan.FromMilliseconds(50)))
        {
            await Assert.ThrowsAnyAsync<OperationCanceledException>(() =>
                viewerQueue.ReadAsync(viewerReadCts.Token));
        }

        var ownerMessage = await ownerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(ownerMessage);

        var hostResize = JsonSerializer.Deserialize(
            ownerMessage!,
            WsJsonContext.Default.HostResizeMessage);

        Assert.Equal("resize-accept", hostResize?.CommandId);
        Assert.Equal((ushort)24, hostResize?.Rows);
        Assert.Equal((ushort)80, hostResize?.Cols);
        Assert.Equal(1, hostResults.PendingCount);
        Assert.Equal(["24x80"], hostResults.PendingPayloads);
        Assert.Empty(auditWriter.Appends);
        Assert.Empty(auditWriter.Updates);
    }

    [Fact]
    public async Task Resize_Is_Rejected_For_NonOwner_Even_With_Inject_Access()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new RecordingAuditWriter();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("test-host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var viewerQueue = queues.AddParticipantQueue("viewer-1");
        queues.SetHostQueue();

        var context = new RelayMessageContext(
            sessionId,
            "viewer-1",
            UserId.New(),
            AccessLevel.Inject,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: viewerQueue);

        _ = await processor.ProcessAsync(
            """{"type":"participant.resize","actionId":"resize-deny","rows":40,"cols":120,"claim":false,"nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}""",
            context,
            CancellationToken.None);

        var message = await viewerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(message);

        var result = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(message.Value.Payload),
            WsJsonContext.Default.ActionResultMessage);

        Assert.Equal(RelayActionStatus.Rejected, result?.Status);
        Assert.Equal([(InputAuditKind.Resize, InputAuditStatus.Rejected)], auditWriter.Appends);
    }

    [Theory]
    [InlineData("""{"type":"participant.resize","rows":24,"cols":80}""")]
    [InlineData("""{"type":"participant.resize","actionId":"action-1","cols":80}""")]
    [InlineData("""{"type":"participant.resize","actionId":"action-1","rows":24}""")]
    [InlineData("""{"type":"participant.resize","actionId":"action-1","rows":0,"cols":80}""")]
    [InlineData("""{"type":"participant.resize","actionId":"action-1","rows":24,"cols":0}""")]
    [InlineData("""{"type":"participant.resize","actionId":"action-1","rows":513,"cols":80}""")]
    [InlineData("""{"type":"participant.resize","actionId":"action-1","rows":24,"cols":1025}""")]
    public async Task Resize_Is_ProtocolViolation_For_Malformed_Or_OutOfRange_Payloads(string payload)
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            new RecordingAuditWriter(),
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        var context = new RelayMessageContext(
            sessionId,
            "viewer-1",
            UserId.New(),
            AccessLevel.Inject,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance);

        var outcome = await processor.ProcessAsync(payload, context, CancellationToken.None);

        Assert.Equal(RelayMessageProcessOutcome.ProtocolViolation, outcome);
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public async Task FocusChanged_Is_Accepted_And_Forwards_Host_FocusChanged_When_Owner_Queue_Accepts(
        bool focused)
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new RecordingAuditWriter();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("test-host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var viewerQueue = queues.AddParticipantQueue("viewer-1");
        var ownerQueue = queues.SetHostQueue();

        var context = new RelayMessageContext(
            sessionId,
            "viewer-1",
            UserId.New(),
            AccessLevel.Inject,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: viewerQueue);

        var actionId = focused ? "focus-1" : "blur-1";
        var json = $$"""{"type":"participant.focusChanged","actionId":"{{actionId}}","focused":{{(focused ? "true" : "false")}},"nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}""";

        await processor.ProcessAsync(json, context, CancellationToken.None);

        var viewerMessage = await viewerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(viewerMessage);
        var result = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(viewerMessage.Value.Payload),
            WsJsonContext.Default.ActionResultMessage);
        Assert.Equal(RelayActionStatus.Accepted, result?.Status);

        var ownerMessage = await ownerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(ownerMessage);
        var hostFocus = JsonSerializer.Deserialize(
            ownerMessage!,
            WsJsonContext.Default.HostFocusChangedMessage);

        Assert.Equal(actionId, hostFocus?.CommandId);
        Assert.Equal("viewer-1", hostFocus?.ClientId);
        Assert.Equal(focused, hostFocus?.Focused);
        Assert.Equal(
            [(InputAuditKind.FocusChange, InputAuditStatus.Pending)],
            auditWriter.Appends);
        Assert.Equal([InputAuditStatus.Dispatched], auditWriter.Updates);
    }

    [Fact]
    public async Task FocusChanged_Is_Rejected_When_Access_Below_Inject()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new RecordingAuditWriter();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("test-host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var viewerQueue = queues.AddParticipantQueue("viewer-1");
        queues.SetHostQueue();

        var context = new RelayMessageContext(
            sessionId,
            "viewer-1",
            UserId.New(),
            AccessLevel.Suggest,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: viewerQueue);

        _ = await processor.ProcessAsync(
            """{"type":"participant.focusChanged","actionId":"focus-deny","focused":true,"nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}""",
            context,
            CancellationToken.None);

        var message = await viewerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(message);

        var result = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(message.Value.Payload),
            WsJsonContext.Default.ActionResultMessage);

        Assert.Equal(RelayActionStatus.Rejected, result?.Status);
        Assert.Equal(
            [(InputAuditKind.FocusChange, InputAuditStatus.Rejected)],
            auditWriter.Appends);
    }

    [Theory]
    [InlineData("""{"type":"participant.focusChanged","focused":true}""")]
    [InlineData("""{"type":"participant.focusChanged","actionId":"action-1"}""")]
    [InlineData("""{"type":"participant.focusChanged","actionId":"","focused":true}""")]
    public async Task FocusChanged_Is_ProtocolViolation_For_Malformed_Payloads(string payload)
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            new RecordingAuditWriter(),
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        var context = new RelayMessageContext(
            sessionId,
            "viewer-1",
            UserId.New(),
            AccessLevel.Inject,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance);

        var outcome = await processor.ProcessAsync(payload, context, CancellationToken.None);

        Assert.Equal(RelayMessageProcessOutcome.ProtocolViolation, outcome);
    }

    [Fact]
    public async Task Suggest_Is_Rejected_When_Owner_Is_Not_Ready()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new RecordingAuditWriter();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("test-host", new CancellationTokenSource());
        runtime.Host.SetHostReady(false);
        runtime.Host.SetStatus(SessionStatus.Live);

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var viewerQueue = queues.AddParticipantQueue("viewer-1");
        queues.SetHostQueue();

        var context = new RelayMessageContext(
            sessionId,
            "viewer-1",
            UserId.New(),
            AccessLevel.Suggest,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: viewerQueue);

        _ = await processor.ProcessAsync(
            """{"type":"participant.suggest","actionId":"action-not-ready","nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}""",
            context,
            CancellationToken.None);

        var message = await viewerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(message);

        var result = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(message.Value.Payload),
            WsJsonContext.Default.ActionResultMessage);

        Assert.Equal(RelayActionStatus.Rejected, result?.Status);
        Assert.Equal([(InputAuditKind.Suggestion, InputAuditStatus.Rejected)], auditWriter.Appends);
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
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            new RecordingAuditWriter(),
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);

        var context = new RelayMessageContext(
            sessionId,
            "viewer-1",
            UserId.New(),
            AccessLevel.View,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance);

        var outcome = await processor.ProcessAsync(
            """{"type":"participant.unknown"}""",
            context,
            CancellationToken.None);

        Assert.Equal(RelayMessageProcessOutcome.ProtocolViolation, outcome);
    }

    [Fact]
    public async Task ProcessAsync_Returns_ProtocolViolation_For_Malformed_Known_Participant_Messages()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            new RecordingAuditWriter(),
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        var context = new RelayMessageContext(
            sessionId,
            "viewer-1",
            UserId.New(),
            AccessLevel.Inject,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance);

        foreach (var payload in new[]
                 {
                     """{}""",
                      """{"type":"participant.suggest","nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}""",
                      """{"type":"participant.suggest","actionId":"action-123"}""",
                      """{"type":"participant.suggest","actionId":"action-123","nonce":"","ciphertext":"","signature":""}""",
                      """{"type":"participant.inject","nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}""",
                      """{"type":"participant.inject","actionId":"action-456"}""",
                      """{"type":"participant.inject","actionId":"action-456","nonce":"","ciphertext":"","signature":""}""",
                  })
        {
            var outcome = await processor.ProcessAsync(payload, context, CancellationToken.None);

            Assert.Equal(RelayMessageProcessOutcome.ProtocolViolation, outcome);
        }
    }

    [Fact]
    public async Task PermissionDecision_PendingDuplicate_Redispatches_Only_With_A_New_Process_Lease()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var auditStore = new RecordingPermissionDecisionAuditStore();
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);
        var hostQueue = broadcaster.GetOrCreateSession(sessionId).SetHostQueue();
        var context = new RelayMessageContext(
            sessionId,
            "approver",
            UserId.New(),
            AccessLevel.Approve,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            "approver-device",
            Guid.CreateVersion7(),
            7);
        const string payload =
            """
            {"type":"participant.permissionDecision","actionId":"pending-recovery","requestId":"tool-use-7","requestGeneration":7,"decision":"allow","nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}
            """;

        var firstProcess = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            new RecordingAuditWriter(),
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: auditStore);
        Assert.Equal(
            RelayMessageProcessOutcome.Handled,
            await firstProcess.ProcessAsync(
                payload,
                context,
                TestContext.Current.CancellationToken));
        Assert.NotNull(await hostQueue.ReadAsync(TestContext.Current.CancellationToken));

        var restartedProcess = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            new RecordingAuditWriter(),
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: auditStore);
        Assert.Equal(
            RelayMessageProcessOutcome.Handled,
            await restartedProcess.ProcessAsync(
                payload,
                context,
                TestContext.Current.CancellationToken));
        Assert.NotNull(await hostQueue.ReadAsync(TestContext.Current.CancellationToken));

        Assert.Equal(
            RelayMessageProcessOutcome.Handled,
            await restartedProcess.ProcessAsync(
                payload,
                context,
                TestContext.Current.CancellationToken));
        using (var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25)))
        {
            await Assert.ThrowsAnyAsync<OperationCanceledException>(
                () => hostQueue.ReadAsync(timeout.Token));
        }

        var laterRestart = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            new RecordingAuditWriter(),
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: auditStore);
        const string alteredPayload =
            """
            {"type":"participant.permissionDecision","actionId":"pending-recovery","requestId":"tool-use-7","requestGeneration":7,"decision":"allow","nonce":"nonce","ciphertext":"altered","signature":"signature"}
            """;
        Assert.Equal(
            RelayMessageProcessOutcome.Handled,
            await laterRestart.ProcessAsync(
                alteredPayload,
                context,
                TestContext.Current.CancellationToken));
        using var alteredTimeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => hostQueue.ReadAsync(alteredTimeout.Token));
    }

    [Fact]
    public async Task PermissionDecision_Allows_Envelope_Above_16KiB_Within_2MiB_Authority()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            new RecordingAuditWriter(),
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var participantQueue = queues.AddParticipantQueue("approver");
        var hostQueue = queues.SetHostQueue();
        var context = new RelayMessageContext(
            sessionId,
            "approver",
            UserId.New(),
            AccessLevel.Approve,
            IsOwnerParticipant: false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: participantQueue);
        var payload = JsonSerializer.Serialize(
            new ParticipantPermissionDecisionMessage(
                "decision-large",
                "request-1",
                7,
                "allow",
                "nonce",
                new string('x', 20 * 1024),
                "signature"),
            WsJsonContext.Default.ParticipantPermissionDecisionMessage);

        var outcome = await processor.ProcessAsync(
            payload,
            context,
            TestContext.Current.CancellationToken);

        Assert.Equal(RelayMessageProcessOutcome.Handled, outcome);
        var hostPayload = await hostQueue.ReadAsync(
            TestContext.Current.CancellationToken);
        var hostDecision = JsonSerializer.Deserialize(
            hostPayload!,
            WsJsonContext.Default.HostPermissionDecisionMessage);
        Assert.Equal("decision-large", hostDecision?.CommandId);
        Assert.Equal("request-1", hostDecision?.RequestId);
        Assert.Equal(sessionId.Value.ToString(), hostDecision?.SessionId);
        using (var pendingTimeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25)))
        {
            await Assert.ThrowsAnyAsync<OperationCanceledException>(
                () => participantQueue.ReadAsync(pendingTimeout.Token));
        }

        var retryPayload = JsonSerializer.Serialize(
            new ParticipantPermissionDecisionMessage(
                "decision-large",
                "request-forged-retry",
                8,
                "allow",
                "nonce",
                "ciphertext",
                "signature"),
            WsJsonContext.Default.ParticipantPermissionDecisionMessage);
        _ = await processor.ProcessAsync(
            retryPayload,
            context,
            TestContext.Current.CancellationToken);
        using var retryTimeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => participantQueue.ReadAsync(retryTimeout.Token));
        using var hostRetryTimeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => hostQueue.ReadAsync(hostRetryTimeout.Token));
    }

    [Fact]
    public async Task PermissionDecision_Rejection_Preserves_Original_ToolUse_RequestId()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            new RecordingAuditWriter(),
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var participantQueue = queues.AddParticipantQueue("viewer");
        _ = queues.SetHostQueue();
        var context = new RelayMessageContext(
            sessionId,
            "viewer",
            UserId.New(),
            AccessLevel.View,
            false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: participantQueue);
        const string message =
            """
            {"type":"participant.permissionDecision","actionId":"decision-rejected","requestId":"tool-use-42","requestGeneration":7,"decision":"allow","nonce":"nonce","ciphertext":"ciphertext","signature":"signature"}
            """;

        _ = await processor.ProcessAsync(
            message,
            context,
            TestContext.Current.CancellationToken);

        var resultMessage = await participantQueue.ReadAsync(
            TestContext.Current.CancellationToken);
        var result = JsonSerializer.Deserialize(
            resultMessage!.Value.Payload,
            WsJsonContext.Default.ActionResultMessage);
        Assert.Equal(RelayActionStatus.Rejected, result?.Status);
        Assert.Equal("tool-use-42", result?.RequestId);
    }

    [Fact]
    public async Task ProcessAsync_Does_Not_Refresh_Participant_Activity_For_Protocol_Violations()
    {
        var clock = new TestTimeProvider(new DateTimeOffset(2026, 4, 26, 12, 0, 0, TimeSpan.Zero));
        var participants = new ParticipantRoster(clock);
        Assert.True(participants.TryAddSharedParticipant("viewer-1", int.MaxValue, out _));
        clock.Advance(TimeSpan.FromMinutes(2));

        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            new RecordingAuditWriter(),
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        var context = new RelayMessageContext(
            sessionId,
            "viewer-1",
            UserId.New(),
            AccessLevel.Inject,
            IsOwnerParticipant: false,
            runtime.Host,
            participants,
            AllowingParticipantActionAuthority.Instance);

        var outcome = await processor.ProcessAsync(
            """{"type":"participant.inject","actionId":"action-456","nonce":"","ciphertext":"","signature":""}""",
            context,
            CancellationToken.None);

        Assert.Equal(RelayMessageProcessOutcome.ProtocolViolation, outcome);
        Assert.Equal(
            ["viewer-1"],
            participants.GetStaleSharedParticipantConnectionIds(TimeSpan.FromMinutes(1)));
    }

    [Fact]
    public async Task Durable_Existing_Action_Is_Returned_As_Duplicate_Without_Redispatch()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new ExistingAuditWriter();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var participantQueue = queues.AddParticipantQueue("viewer");
        var hostQueue = queues.SetHostQueue();
        var context = new RelayMessageContext(
            sessionId,
            "viewer",
            UserId.New(),
            AccessLevel.Inject,
            false,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            ParticipantQueue: participantQueue);

        await processor.ProcessAsync(
            """{"type":"participant.inject","actionId":"durable-duplicate","nonce":"n","ciphertext":"c","signature":"s"}""",
            context,
            CancellationToken.None);

        var resultPayload = await participantQueue.ReadAsync(CancellationToken.None);
        var result = JsonSerializer.Deserialize(
            resultPayload!.Value.Payload,
            WsJsonContext.Default.ActionResultMessage);
        Assert.Equal(RelayActionStatus.Duplicate, result?.Status);
        Assert.Equal(1, auditWriter.DuplicateRecords);
        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(() => hostQueue.ReadAsync(timeout.Token));
    }

    [Fact]
    public async Task Revocation_During_Audit_Prevents_Host_Dispatch()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var connectionRegistry = new ConnectionRegistry();
        var lifecycleGate = new SessionLifecycleGate();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new BlockingAuditWriter();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var participantQueue = queues.AddParticipantQueue("viewer");
        var hostQueue = queues.SetHostQueue();
        var userId = UserId.New();
        const string deviceId = "viewer-device";
        connectionRegistry.RegisterSharedParticipant(
            "viewer",
            userId,
            deviceId,
            sessionId);
        var accessDecision = new ParticipantAccessDecision(
            IsOwnerParticipant: false,
            AccessLevel.Inject,
            DateTimeOffset.UtcNow);
        var context = new RelayMessageContext(
            sessionId,
            "viewer",
            userId,
            AccessLevel.Inject,
            false,
            runtime.Host,
            runtime.Participants,
            new ParticipantActionAuthority(
                sessionId,
                "viewer",
                userId,
                deviceId,
                accessDecision,
                runtime,
                queues,
                participantQueue,
                runtimeDirectory,
                connectionRegistry,
                broadcaster,
                lifecycleGate),
            deviceId);

        var processing = processor.ProcessAsync(
            """{"type":"participant.inject","actionId":"revoked-during-audit","nonce":"n","ciphertext":"c","signature":"s"}""",
            context,
            CancellationToken.None);
        await auditWriter.AppendStarted.Task;
        await using (await lifecycleGate.AcquireAsync(
            sessionId,
            TestContext.Current.CancellationToken))
        {
            participantQueue.Complete(
                QueueCompletionCause.AccessRevokedCascade,
                discardPending: true);
        }
        auditWriter.AllowAppend.TrySetResult();
        await processing;

        Assert.Contains(InputAuditStatus.Rejected, auditWriter.Updates);
        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(() => hostQueue.ReadAsync(timeout.Token));
    }

    [Fact]
    public async Task Stale_Action_Cannot_Dispatch_To_Replacement_Incarnation()
    {
        var runtimes = new LiveSessionStateDirectory();
        var connections = new ConnectionRegistry();
        var lifecycleGate = new SessionLifecycleGate();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new BlockingAuditWriter();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var oldRuntime = runtimes.CreateRuntime(sessionId);
        oldRuntime.Host.TryClaimHost(
            "old-host",
            new CancellationTokenSource());
        oldRuntime.Host.SetHostReady(true);
        oldRuntime.Host.SetStatus(SessionStatus.Live);
        var oldQueues = broadcaster.GetOrCreateSession(sessionId);
        var oldParticipantQueue =
            oldQueues.AddParticipantQueue("viewer");
        oldQueues.SetHostQueue("old-host");
        var userId = UserId.New();
        const string deviceId = "viewer-device";
        connections.RegisterSharedParticipant(
            "viewer",
            userId,
            deviceId,
            sessionId);
        var context = new RelayMessageContext(
            sessionId,
            "viewer",
            userId,
            AccessLevel.Inject,
            false,
            oldRuntime.Host,
            oldRuntime.Participants,
            new ParticipantActionAuthority(
                sessionId,
                "viewer",
                userId,
                deviceId,
                new ParticipantAccessDecision(
                    IsOwnerParticipant: false,
                    AccessLevel.Inject,
                    DateTimeOffset.UtcNow),
                oldRuntime,
                oldQueues,
                oldParticipantQueue,
                runtimes,
                connections,
                broadcaster,
                lifecycleGate),
            deviceId);
        var processing = processor.ProcessAsync(
            """{"type":"participant.inject","actionId":"stale-incarnation","nonce":"n","ciphertext":"c","signature":"s"}""",
            context,
            CancellationToken.None);
        await auditWriter.AppendStarted.Task;

        ChannelByteSendQueue replacementHostQueue;
        await using (await lifecycleGate.AcquireAsync(
            sessionId,
            TestContext.Current.CancellationToken))
        {
            Assert.True(runtimes.RemoveIfSame(sessionId, oldRuntime));
            Assert.True(broadcaster.RemoveSessionIfSame(
                sessionId,
                oldQueues));
            connections.Remove("viewer");
            var replacementRuntime = runtimes.CreateRuntime(sessionId);
            replacementRuntime.Host.TryClaimHost(
                "replacement-host",
                new CancellationTokenSource());
            replacementRuntime.Host.SetHostReady(true);
            replacementRuntime.Host.SetStatus(SessionStatus.Live);
            var replacementQueues =
                broadcaster.GetOrCreateSession(sessionId);
            replacementQueues.AddParticipantQueue("viewer");
            replacementHostQueue =
                replacementQueues.SetHostQueue("replacement-host");
            connections.RegisterSharedParticipant(
                "viewer",
                userId,
                deviceId,
                sessionId);
        }

        auditWriter.AllowAppend.TrySetResult();
        await processing;

        Assert.Contains(InputAuditStatus.Rejected, auditWriter.Updates);
        using var timeout =
            new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(
            () => replacementHostQueue.ReadAsync(timeout.Token));
    }

    [Fact]
    public async Task Stale_Action_Cannot_Dispatch_After_Connection_Authority_Changes()
    {
        var runtimes = new LiveSessionStateDirectory();
        var connections = new ConnectionRegistry();
        var lifecycleGate = new SessionLifecycleGate();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var auditWriter = new BlockingAuditWriter();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            auditWriter,
            NullSemanticRelayRepository.Instance,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var runtime = runtimes.CreateRuntime(sessionId);
        runtime.Host.TryClaimHost("host", new CancellationTokenSource());
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var participantQueue =
            queues.AddParticipantQueue("viewer");
        var hostQueue = queues.SetHostQueue("host");
        var userId = UserId.New();
        const string deviceId = "viewer-device";
        connections.RegisterSharedParticipant(
            "viewer",
            userId,
            deviceId,
            sessionId);
        var context = new RelayMessageContext(
            sessionId,
            "viewer",
            userId,
            AccessLevel.Inject,
            false,
            runtime.Host,
            runtime.Participants,
            new ParticipantActionAuthority(
                sessionId,
                "viewer",
                userId,
                deviceId,
                new ParticipantAccessDecision(
                    IsOwnerParticipant: false,
                    AccessLevel.Inject,
                    DateTimeOffset.UtcNow),
                runtime,
                queues,
                participantQueue,
                runtimes,
                connections,
                broadcaster,
                lifecycleGate),
            deviceId);
        var processing = processor.ProcessAsync(
            """{"type":"participant.inject","actionId":"stale-authority","nonce":"n","ciphertext":"c","signature":"s"}""",
            context,
            CancellationToken.None);
        await auditWriter.AppendStarted.Task;

        await using (await lifecycleGate.AcquireAsync(
            sessionId,
            TestContext.Current.CancellationToken))
        {
            connections.RegisterSharedParticipant(
                "viewer",
                UserId.New(),
                "replacement-device",
                sessionId);
        }

        auditWriter.AllowAppend.TrySetResult();
        await processing;

        Assert.Contains(InputAuditStatus.Rejected, auditWriter.Updates);
        using var timeout =
            new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(
            () => hostQueue.ReadAsync(timeout.Token));
    }

    private static SemanticSendFixture CreateSemanticSendFixture(
        bool hostQueueInitiallyOpen = true)
    {
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var repository = new StatefulSemanticRelayRepository();
        var processor = new RelayMessageProcessor(
            broadcaster,
            new ActionDedupeCache(),
            new RecordingAuditWriter(),
            repository,
            metrics,
            NullLogger<RelayMessageProcessor>.Instance,
            UnsupportedServiceScopeFactory.Instance,
            permissionDecisionAuditStore: new RecordingPermissionDecisionAuditStore());
        var sessionId = SessionId.New();
        var userId = UserId.New();
        var incarnationId = Guid.CreateVersion7();
        var runtime = runtimes.CreateRuntime(sessionId);
        runtime.Host.SetStatus(SessionStatus.Live);
        runtime.Host.SetHostReady(true);
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var hostQueue = hostQueueInitiallyOpen
            ? queues.SetHostQueue("host")
            : new ChannelByteSendQueue();
        var message = new ParticipantSemanticSendMessage(
            Guid.CreateVersion7(),
            incarnationId,
            RelaySemanticMode.Steer,
            new string('a', 64),
            "nonce",
            "ciphertext",
            "signature");
        var context = new RelayMessageContext(
            sessionId,
            "owner",
            userId,
            AccessLevel.Approve,
            IsOwnerParticipant: true,
            runtime.Host,
            runtime.Participants,
            AllowingParticipantActionAuthority.Instance,
            "owner-device",
            incarnationId);
        return new SemanticSendFixture(
            processor,
            repository,
            queues,
            hostQueue,
            message,
            context);
    }

    private sealed record SemanticSendFixture(
        RelayMessageProcessor Processor,
        StatefulSemanticRelayRepository Repository,
        SessionSendQueues Queues,
        ChannelByteSendQueue HostQueue,
        ParticipantSemanticSendMessage Message,
        RelayMessageContext Context);

    private sealed class StatefulSemanticRelayRepository : ISemanticRelayRepository
    {
        private SemanticRelayRequest? _request;
        public int DispatchCount { get; private set; }
        public bool Dispatched => DispatchCount > 0;
        public bool HasRequest => _request is not null;

        public Task<SemanticRequestClaim> ClaimRequestAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            string mode,
            string payloadSha256,
            CancellationToken ct = default)
        {
            if (_request is null)
            {
                _request = SemanticRelayRequest.Create(
                    sessionId,
                    incarnationId,
                    requesterUserId,
                    requesterDeviceId,
                    requestId,
                    mode,
                    payloadSha256,
                    DateTimeOffset.UnixEpoch);
                return Task.FromResult(new SemanticRequestClaim(
                    SemanticRequestClaimKind.Created,
                    _request));
            }
            return Task.FromResult(new SemanticRequestClaim(
                _request.RequesterUserId == requesterUserId
                    && _request.RequestId == requestId
                    && _request.Matches(
                        sessionId,
                        incarnationId,
                        requesterUserId,
                        requesterDeviceId,
                        mode,
                        payloadSha256)
                    ? SemanticRequestClaimKind.ExactDuplicate
                    : SemanticRequestClaimKind.Conflict,
                _request));
        }

        public Task MarkDispatchedAsync(
            Guid requestRowId,
            CancellationToken ct = default)
        {
            Assert.Equal(_request?.Id, requestRowId);
            DispatchCount++;
            return Task.CompletedTask;
        }

        public Task<SemanticRequestClaim?> FindExactRequestAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            string mode,
            string payloadSha256,
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
            CancellationToken ct = default) => throw new NotSupportedException();

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

    private sealed class RecordingSemanticAckRepository : ISemanticRelayRepository
    {
        public (SessionId, Guid, UserId, string, Guid)? Acknowledged { get; private set; }
        public IReadOnlyList<SemanticRelayReceipt> PendingReceipts { get; set; } = [];
        public List<string> Calls { get; } = [];
        public Action? BeforeList { get; set; }
        public bool AcknowledgeResult { get; set; } = true;
        public int LastListLimit { get; private set; }

        public Task<bool> AcknowledgeReceiptAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            CancellationToken ct = default)
        {
            Calls.Add("acknowledge");
            Acknowledged = (
                sessionId,
                incarnationId,
                requesterUserId,
                requesterDeviceId,
                requestId);
            return Task.FromResult(AcknowledgeResult);
        }

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
            CancellationToken ct = default) => throw new NotSupportedException();

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
            CancellationToken ct = default)
        {
            Calls.Add("list");
            BeforeList?.Invoke();
            LastListLimit = limit;
            return Task.FromResult<IReadOnlyList<SemanticRelayReceipt>>(
                PendingReceipts.Take(limit).ToArray());
        }

        public Task<int> DeleteAcknowledgedBeforeAsync(
            DateTimeOffset cutoff,
            int limit,
            CancellationToken ct = default) => throw new NotSupportedException();
    }

    private sealed class NullSemanticRelayRepository : ISemanticRelayRepository
    {
        public static NullSemanticRelayRepository Instance { get; } = new();

        public Task<SemanticRequestClaim> ClaimRequestAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            string mode,
            string payloadSha256,
            CancellationToken ct = default) =>
            Task.FromResult(new SemanticRequestClaim(
                SemanticRequestClaimKind.Created,
                SemanticRelayRequest.Create(
                    sessionId,
                    incarnationId,
                    requesterUserId,
                    requesterDeviceId,
                    requestId,
                    mode,
                    payloadSha256,
                    DateTimeOffset.UtcNow)));

        public Task<SemanticRequestClaim?> FindExactRequestAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            string mode,
            string payloadSha256,
            CancellationToken ct = default) =>
            Task.FromResult<SemanticRequestClaim?>(null);

        public Task MarkDispatchedAsync(
            Guid requestRowId,
            CancellationToken ct = default) => Task.CompletedTask;

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
            CancellationToken ct = default) => Task.FromResult(false);

        public Task<IReadOnlyList<SemanticRelayReceipt>> ListPendingReceiptsForDeviceAsync(
            UserId requesterUserId,
            string requesterDeviceId,
            int limit,
            SemanticReceiptCursor? cursor = null,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<SemanticRelayReceipt>>([]);

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
            CancellationToken ct = default) => Task.FromResult(false);

        public Task<int> DeleteAcknowledgedBeforeAsync(
            DateTimeOffset cutoff,
            int limit,
            CancellationToken ct = default) => Task.FromResult(0);
    }

    private sealed class AllowingParticipantActionAuthority
        : IParticipantActionAuthority
    {
        public static AllowingParticipantActionAuthority Instance { get; } =
            new();

        public ValueTask<ParticipantActionDispatch> TryDispatchAsync(
            SessionCapability requiredCapability,
            Func<ChannelByteSendQueueWriteOutcome> dispatch,
            CancellationToken ct) =>
            ValueTask.FromResult(
                new ParticipantActionDispatch(
                    Authorized: true,
                    dispatch()));
    }

    private sealed class RecordingPermissionDecisionAuditStore : IPermissionDecisionAuditStore
    {
        private readonly Dictionary<
            (SessionId, UserId, string),
            (PermissionDecisionPendingTuple Tuple, string AuditPayload)> _pending = [];

        public int PendingCount => _pending.Count;
        public IReadOnlyCollection<string> PendingPayloads =>
            _pending.Values.Select(value => value.AuditPayload).ToArray();

        public Task<PermissionDecisionAdmissionResult> AdmitAsync(
            SessionId sessionId,
            UserId requesterUserId,
            string actionId,
            string auditPayload,
            PermissionDecisionPendingTuple pendingTuple,
            CancellationToken ct = default)
        {
            var key = (sessionId, requesterUserId, actionId);
            if (_pending.TryGetValue(key, out var existing))
            {
                return Task.FromResult(new PermissionDecisionAdmissionResult(
                    existing.Tuple == pendingTuple
                        && existing.AuditPayload == auditPayload
                        ? PermissionDecisionAdmissionOutcome.PendingDuplicate
                        : PermissionDecisionAdmissionOutcome.Conflict,
                    existing.Tuple));
            }
            _pending.Add(key, (pendingTuple, auditPayload));
            return Task.FromResult(new PermissionDecisionAdmissionResult(
                PermissionDecisionAdmissionOutcome.Applied,
                pendingTuple));
        }

        public Task<bool> MarkDispatchFailedAsync(
            SessionId sessionId,
            UserId requesterUserId,
            string actionId,
            PermissionDecisionPendingTuple pendingTuple,
            CancellationToken ct = default) => Task.FromResult(true);

        public Task<HostActionCompletionResult> CompleteHostActionAsync(
            SessionId sessionId,
            UserId assertedRequesterUserId,
            string assertedActionId,
            PermissionDecisionPendingTuple assertedTuple,
            InputAuditStatus terminalStatus,
            CancellationToken ct = default) => throw new NotSupportedException();
    }

    private sealed class RecordingAuditWriter : IAuditWriter
    {
        public List<(InputAuditKind Kind, InputAuditStatus Status)> Appends { get; } = [];
        public List<(InputAuditKind Kind, string Payload, InputAuditStatus Status)> AppendsWithPayload { get; } = [];
        public List<InputAuditStatus> Updates { get; } = [];
        public InputAuditStatus? CurrentStatus { get; private set; }
        public int DuplicateRecords { get; private set; }

        public Task<AuditAppendOutcome> AppendAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditKind kind,
            string payload,
            InputAuditStatus status,
            CancellationToken ct = default)
        {
            Appends.Add((kind, status));
            AppendsWithPayload.Add((kind, payload, status));
            CurrentStatus = status;
            return Task.FromResult(AuditAppendOutcome.Appended);
        }

        public Task<bool> RecordDuplicateAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditKind kind,
            string payload,
            CancellationToken ct = default)
        {
            DuplicateRecords++;
            Appends.Add((kind, InputAuditStatus.Duplicate));
            AppendsWithPayload.Add((kind, payload, InputAuditStatus.Duplicate));
            return Task.FromResult(true);
        }

        public Task<bool> UpdateStatusAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditStatus status,
            CancellationToken ct = default)
        {
            Updates.Add(status);
            CurrentStatus = status;
            return Task.FromResult(true);
        }

        public Task<bool> TryRearmAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditStatus initialStatus,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<InputAuditStatus?> GetStatusAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            CancellationToken ct = default) =>
            Task.FromResult(CurrentStatus);
    }

    private sealed class ExistingAuditWriter : IAuditWriter
    {
        public int DuplicateRecords { get; private set; }

        public Task<AuditAppendOutcome> AppendAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditKind kind,
            string payload,
            InputAuditStatus status,
            CancellationToken ct = default) =>
            Task.FromResult(AuditAppendOutcome.AlreadyExists);

        public Task<bool> RecordDuplicateAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditKind kind,
            string payload,
            CancellationToken ct = default)
        {
            DuplicateRecords++;
            return Task.FromResult(true);
        }

        public Task<bool> UpdateStatusAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditStatus status,
            CancellationToken ct = default) =>
            Task.FromResult(true);

        public Task<bool> TryRearmAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditStatus initialStatus,
            CancellationToken ct = default) =>
            Task.FromResult(false);

        public Task<InputAuditStatus?> GetStatusAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            CancellationToken ct = default) =>
            Task.FromResult<InputAuditStatus?>(null);
    }

    private sealed class BlockingAuditWriter : IAuditWriter
    {
        public TaskCompletionSource AppendStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource AllowAppend { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public List<InputAuditStatus> Updates { get; } = [];

        public async Task<AuditAppendOutcome> AppendAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditKind kind,
            string payload,
            InputAuditStatus status,
            CancellationToken ct = default)
        {
            AppendStarted.TrySetResult();
            await AllowAppend.Task.WaitAsync(ct);
            return AuditAppendOutcome.Appended;
        }

        public Task<bool> RecordDuplicateAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditKind kind,
            string payload,
            CancellationToken ct = default) =>
            Task.FromResult(true);

        public Task<bool> UpdateStatusAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditStatus status,
            CancellationToken ct = default)
        {
            Updates.Add(status);
            return Task.FromResult(true);
        }

        public Task<bool> TryRearmAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditStatus initialStatus,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<InputAuditStatus?> GetStatusAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
    }

    private sealed class FailingAuditWriter : IAuditWriter
    {
        public Task<AuditAppendOutcome> AppendAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditKind kind,
            string payload,
            InputAuditStatus status,
            CancellationToken ct = default)
            => Task.FromResult(AuditAppendOutcome.Failed);

        public Task<bool> RecordDuplicateAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditKind kind,
            string payload,
            CancellationToken ct = default)
            => Task.FromResult(false);

        public Task<bool> UpdateStatusAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditStatus status,
            CancellationToken ct = default)
            => Task.FromResult(false);

        public Task<bool> TryRearmAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            InputAuditStatus initialStatus,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<InputAuditStatus?> GetStatusAsync(
            SessionId sessionId,
            UserId userId,
            string clientCommandId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
    }
}
