using System.Net.WebSockets;
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

public sealed class ParticipantHandshakeTests
{
    [Fact]
    public async Task Owner_Reconnect_Replays_Exact_Device_Receipt_After_Acceptance()
    {
        var semanticRelay = new HandshakeSemanticRelayRepository();
        using var fixture = CreateFixture(
            [],
            ownerParticipant: true,
            semanticRelay: semanticRelay);
        semanticRelay.Pending =
        [
            SemanticRelayReceipt.Create(
                Guid.NewGuid(),
                fixture.Session.Id,
                fixture.Session.IncarnationId,
                fixture.Session.OwnerUserId,
                "viewer-device",
                Guid.CreateVersion7(),
                "steer",
                new string('a', 64),
                "injected",
                fixture.Session.OwnerUserId,
                "host-device",
                "owner-signature",
                DateTimeOffset.UnixEpoch),
        ];

        Assert.True(await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken));

        Assert.Equal(
            (fixture.Session.Id, fixture.Session.IncarnationId,
                fixture.Session.OwnerUserId, "viewer-device"),
            semanticRelay.Query);
        var acceptedMessage = await ReadJsonAsync(fixture.State.ParticipantQueue!);
        Assert.NotNull(JsonSerializer.Deserialize(
            acceptedMessage.Payload,
            WsJsonContext.Default.ParticipantAcceptedMessage));
        var receiptMessage = await ReadJsonAsync(fixture.State.ParticipantQueue!);
        var receipt = JsonSerializer.Deserialize(
            receiptMessage.Payload,
            WsJsonContext.Default.ParticipantSemanticReceiptMessage);
        Assert.Equal(semanticRelay.Pending[0].RequestId, receipt?.RequestId);
    }

    [Fact]
    public async Task Owner_Admission_Buffers_Receipt_Committed_During_Mailbox_Query()
    {
        var semanticRelay = new HandshakeSemanticRelayRepository();
        using var fixture = CreateFixture(
            [],
            ownerParticipant: true,
            semanticRelay: semanticRelay);
        var receipt = SemanticRelayReceipt.Create(
            Guid.NewGuid(),
            fixture.Session.Id,
            fixture.Session.IncarnationId,
            fixture.Session.OwnerUserId,
            "viewer-device",
            Guid.CreateVersion7(),
            "steer",
            new string('a', 64),
            "injected",
            fixture.Session.OwnerUserId,
            "host-device",
            "owner-signature",
            DateTimeOffset.UnixEpoch);
        semanticRelay.OnList = () =>
        {
            var queue = fixture.Broadcaster.TryGetParticipantQueue(
                fixture.Session.Id,
                fixture.State.ConnectionId);
            Assert.NotNull(queue);
            Assert.True(fixture.State.ParticipantRegistered);
            var delivery = SemanticReceiptWire.Delivery(receipt);
            Assert.True(fixture.Broadcaster.SendSemanticReceiptToParticipantIfSame(
                fixture.Session.Id,
                fixture.State.ConnectionId,
                queue,
                delivery.RequestId,
                delivery.Message));
        };

        Assert.True(await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken));

        _ = await ReadJsonAsync(fixture.State.ParticipantQueue!);
        var delivered = JsonSerializer.Deserialize(
            (await ReadJsonAsync(fixture.State.ParticipantQueue!)).Payload,
            WsJsonContext.Default.ParticipantSemanticReceiptMessage);
        Assert.Equal(receipt.RequestId, delivered?.RequestId);
    }

    [Fact]
    public async Task Shared_Viewer_Admission_Does_Not_Query_Semantic_Mailbox()
    {
        var semanticRelay = new HandshakeSemanticRelayRepository();
        using var fixture = CreateFixture(
            [AccessLevel.View, AccessLevel.View],
            semanticRelay: semanticRelay);

        Assert.True(await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken));
        Assert.Null(semanticRelay.Query);
    }

    [Theory]
    [InlineData(4)]
    [InlineData(5)]
    [InlineData(6)]
    [InlineData(7)]
    [InlineData(8)]
    public async Task Admission_Rejects_Noncurrent_Relay_Protocol(int version)
    {
        using var fixture = CreateFixture(
            [AccessLevel.View],
            relayProtocolVersion: version);

        Assert.False(await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken));
        Assert.False(fixture.State.ParticipantReserved);
        Assert.Equal("unsupported_data", fixture.WebSocket.LastCloseDescription);
    }

    [Fact]
    public async Task Admission_Rejects_Impossible_Future_Key_Generation_Before_Reservation()
    {
        using var fixture = CreateFixture(
            [AccessLevel.View],
            lastSeenKeyGeneration: 4);
        Assert.Equal(
            LiveSessionKeyRotationStoreResult.StoredNewGeneration,
            fixture.Runtimes.TryGet(fixture.Session.Id)!.Stream.TryStoreKeyRotation(
                3,
                [0x03]));

        Assert.False(await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken));
        Assert.False(fixture.State.ParticipantReserved);
        Assert.False(fixture.State.ParticipantRegistered);
        Assert.Equal(
            WebSocketCloseStatus.InvalidPayloadData,
            fixture.WebSocket.CloseStatus);
        Assert.Equal("unsupported_data", fixture.WebSocket.LastCloseDescription);
    }

    [Fact]
    public async Task Admission_Retries_Future_Key_Cursor_While_Stream_Authority_Is_Uninitialized()
    {
        using var fixture = CreateFixture(
            [AccessLevel.View],
            lastSeenKeyGeneration: 4);

        Assert.False(await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken));
        Assert.False(fixture.State.ParticipantReserved);
        Assert.False(fixture.State.ParticipantRegistered);
        Assert.Equal(
            WebSocketCloseStatus.EndpointUnavailable,
            fixture.WebSocket.CloseStatus);
        Assert.Equal(
            CloseReason.SessionRuntimeRecovering.ToWire(),
            fixture.WebSocket.LastCloseDescription);
    }

    [Theory]
    [InlineData(2)]
    [InlineData(3)]
    public async Task Admission_Accepts_Older_Or_Equal_Key_Generation(uint lastSeenKeyGeneration)
    {
        using var fixture = CreateFixture(
            [AccessLevel.View, AccessLevel.View],
            lastSeenKeyGeneration: lastSeenKeyGeneration);
        Assert.Equal(
            LiveSessionKeyRotationStoreResult.StoredNewGeneration,
            fixture.Runtimes.TryGet(fixture.Session.Id)!.Stream.TryStoreKeyRotation(
                3,
                [0x03]));

        Assert.True(await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken));
        Assert.True(fixture.State.ParticipantRegistered);
    }

    [Theory]
    [InlineData(SessionStatus.Live)]
    [InlineData(SessionStatus.Reconnecting)]
    public async Task ParticipantBeforeHostRestart_Retries_Until_SameIncarnation_Runtime_Is_Recreated(
        SessionStatus durableStatus)
    {
        using var fixture = CreateFixture(
            [AccessLevel.View, AccessLevel.View, AccessLevel.View],
            currentIncarnation: true,
            durableStatus: durableStatus,
            publishRuntime: false);

        Assert.False(await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken));
        Assert.Equal(
            WebSocketCloseStatus.EndpointUnavailable,
            fixture.WebSocket.CloseStatus);
        Assert.Equal(
            CloseReason.SessionRuntimeRecovering.ToWire(),
            fixture.WebSocket.LastCloseDescription);
        Assert.False(fixture.State.ParticipantReserved);
        Assert.False(fixture.State.ParticipantRegistered);

        if (fixture.Session.Status == SessionStatus.Reconnecting)
        {
            fixture.Session.ActivateHost("recovery-host");
            fixture.Session.ReleaseHostSlot("recovery-host");
        }
        var recoveredRuntime = fixture.Runtimes.CreateRuntime(fixture.Session.Id);
        recoveredRuntime.Host.SetStatus(SessionStatus.Live);
        var retrySocket = new JoinWebSocket(
            fixture.Session.Id.Value.ToString("D"),
            expectedIncarnationId: fixture.Session.IncarnationId,
            relayProtocolVersion: RelayProtocolVersions.Current);
        var retryState = new ParticipantConnectionState(
            "viewer-retry",
            fixture.Session.Id.Value.ToString(),
            fixture.State.AuthenticatedUserId);

        Assert.True(await fixture.Handshake.TryAcceptAsync(
            retrySocket,
            retryState,
            TestContext.Current.CancellationToken));
        Assert.True(retryState.ParticipantRegistered);
        Assert.Equal(WebSocketState.Open, retrySocket.State);

        retryState.LinkedCts?.Dispose();
        retryState.DisposeDeviceAuthorizationLifetime();
        retrySocket.Dispose();
    }

    [Fact]
    public async Task HostBeforeParticipantRestart_Accepts_SameIncarnation_Immediately()
    {
        using var fixture = CreateFixture(
            [AccessLevel.View, AccessLevel.View],
            currentIncarnation: true);

        Assert.True(await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken));
        Assert.True(fixture.State.ParticipantRegistered);
        Assert.Equal(WebSocketState.Open, fixture.WebSocket.State);
    }

    [Fact]
    public async Task PendingSession_WithMissingRuntime_Remains_Terminal()
    {
        using var fixture = CreateFixture(
            [AccessLevel.View],
            currentIncarnation: true,
            durableStatus: SessionStatus.Pending,
            publishRuntime: false);

        Assert.False(await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken));
        Assert.Equal(WebSocketCloseStatus.PolicyViolation, fixture.WebSocket.CloseStatus);
        Assert.Equal(
            CloseReason.SessionNotLive.ToWire(),
            fixture.WebSocket.LastCloseDescription);
    }

    [Fact]
    public async Task StaleIncarnation_Remains_Terminal_When_Runtime_Is_Missing()
    {
        using var fixture = CreateFixture(
            [AccessLevel.View],
            currentIncarnation: true,
            expectedIncarnationId: Guid.CreateVersion7(),
            publishRuntime: false);

        Assert.False(await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken));
        Assert.Equal(WebSocketCloseStatus.PolicyViolation, fixture.WebSocket.CloseStatus);
        Assert.Equal(
            CloseReason.SessionNotLive.ToWire(),
            fixture.WebSocket.LastCloseDescription);
    }

    [Fact]
    public async Task MissingRuntime_ForEndedSession_Remains_Terminal()
    {
        using var fixture = CreateFixture(
            [AccessLevel.View],
            currentIncarnation: true,
            durableStatus: SessionStatus.Ended,
            publishRuntime: false);

        Assert.False(await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken));
        Assert.Equal(WebSocketCloseStatus.PolicyViolation, fixture.WebSocket.CloseStatus);
        Assert.Equal(
            CloseReason.SessionNotFound.ToWire(),
            fixture.WebSocket.LastCloseDescription);
    }

    [Theory]
    [InlineData("checkpointRevision")]
    [InlineData("presentationRevision")]
    [InlineData("nextSequence")]
    public async Task Admission_Rejects_Join_Missing_Required_Terminal_Cursor(string omittedField)
    {
        using var fixture = CreateFixture(
            [AccessLevel.View],
            omittedJoinCursorField: omittedField);

        Assert.False(await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken));
        Assert.False(fixture.State.ParticipantReserved);
        Assert.Equal("expected_participant_join", fixture.WebSocket.LastCloseDescription);
    }

    [Fact]
    public async Task CurrentIncarnation_Requires_Matching_Expected_Before_Reservation()
    {
        using var missing = CreateFixture(
            [AccessLevel.View],
            currentIncarnation: true,
            omitExpectedIncarnation: true);

        Assert.False(await missing.Handshake.TryAcceptAsync(
            missing.WebSocket,
            missing.State,
            TestContext.Current.CancellationToken));
        Assert.False(missing.State.ParticipantReserved);
        Assert.False(missing.State.ParticipantRegistered);

        using var stale = CreateFixture(
            [AccessLevel.View],
            currentIncarnation: true,
            expectedIncarnationId: Guid.CreateVersion7());

        Assert.False(await stale.Handshake.TryAcceptAsync(
            stale.WebSocket,
            stale.State,
            TestContext.Current.CancellationToken));
        Assert.False(stale.State.ParticipantReserved);
        Assert.False(stale.State.ParticipantRegistered);
    }

    [Fact]
    public async Task CurrentIncarnation_Is_Echoed_In_Participant_Accepted()
    {
        using var fixture = CreateFixture(
            [AccessLevel.View, AccessLevel.View],
            currentIncarnation: true);

        Assert.True(await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken));

        var acceptedMessage = await ReadJsonAsync(fixture.State.ParticipantQueue!);
        var accepted = JsonSerializer.Deserialize(
            acceptedMessage.Payload,
            WsJsonContext.Default.ParticipantAcceptedMessage);
        Assert.Equal(fixture.Session.IncarnationId, accepted?.IncarnationId);
        Assert.Equal(
            fixture.Session.IncarnationGeneration,
            accepted?.IncarnationGeneration);
        Assert.Equal(RelayProtocolVersions.Current, accepted?.RelayProtocolVersion);
    }

    [Fact]
    public async Task Admission_Accepts_Join_Above_Legacy_8KiB_And_Within_Authority_Limit()
    {
        using var fixture = CreateFixture(
            [AccessLevel.View, AccessLevel.View],
            joinPaddingBytes: 9 * 1024);
        Assert.InRange(
            fixture.WebSocket.JoinMessageLength,
            (8 * 1024) + 1,
            RelayMessageLimits.GetMaxBytes("participant.join"));

        var accepted = await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken);

        Assert.True(accepted);
        Assert.True(fixture.State.ParticipantRegistered);
        Assert.Equal(WebSocketState.Open, fixture.WebSocket.State);
        Assert.True(fixture.ScopedLifetime.Created > 0);
        Assert.Equal(
            fixture.ScopedLifetime.Created,
            fixture.ScopedLifetime.Disposed);
    }

    [Fact]
    public async Task PostPublishFence_Removes_Orphan_Queues_When_Runtime_Disappears()
    {
        using var fixture = CreateFixture([AccessLevel.View, AccessLevel.View]);
        var siblingQueue = fixture.Broadcaster
            .GetOrCreateSession(fixture.Session.Id)
            .AddParticipantQueue("sibling", AccessLevel.View);
        fixture.AccessOverrides.OnGetActive = call =>
        {
            if (call == 2)
            {
                var current = fixture.Runtimes.TryGet(fixture.Session.Id);
                Assert.NotNull(current);
                Assert.True(fixture.Runtimes.RemoveIfSame(
                    fixture.Session.Id,
                    current));
            }
        };

        var accepted = await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken);

        Assert.True(accepted);
        Assert.Null(fixture.Runtimes.TryGet(fixture.Session.Id));
        Assert.Null(fixture.Broadcaster.TryGetSession(fixture.Session.Id));
        Assert.False(fixture.State.ParticipantRegistered);
        Assert.False(fixture.State.DbParticipantCounted);
        Assert.Equal(QueueCompletionCause.SessionEnd, fixture.State.ParticipantQueue?.CompletionCause);
        Assert.Equal(CloseReason.SessionNotLive, fixture.State.ParticipantQueue?.CompletionCloseReason);
        Assert.Equal(QueueCompletionCause.SessionEnd, siblingQueue.CompletionCause);
        Assert.Equal(CloseReason.SessionNotLive, siblingQueue.CompletionCloseReason);

        var message = await ReadJsonAsync(fixture.State.ParticipantQueue!);
        var ended = JsonSerializer.Deserialize(
            message.Payload,
            WsJsonContext.Default.SessionEndedMessage);
        Assert.Equal(CloseReason.SessionNotLive.ToWire(), ended?.Reason);
    }

    [Fact]
    public async Task PostPublishFence_Converts_Terminal_Session_End_To_Ended_Message()
    {
        using var fixture = CreateFixture([AccessLevel.View, AccessLevel.View]);
        fixture.AccessOverrides.OnGetActive = call =>
        {
            if (call == 2)
            {
                fixture.Broadcaster.BroadcastSessionEnded(fixture.Session.Id, CloseReason.HostStopped);
            }
        };

        var accepted = await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken);

        Assert.True(accepted);
        Assert.False(fixture.State.ParticipantRegistered);
        Assert.False(fixture.State.DbParticipantCounted);
        Assert.Equal(QueueCompletionCause.SessionEnd, fixture.State.ParticipantQueue?.CompletionCause);
        Assert.Equal(CloseReason.HostStopped, fixture.State.ParticipantQueue?.CompletionCloseReason);

        var message = await ReadJsonAsync(fixture.State.ParticipantQueue!);
        var ended = JsonSerializer.Deserialize(
            message.Payload,
            WsJsonContext.Default.SessionEndedMessage);
        Assert.Equal(CloseReason.HostStopped.ToWire(), ended?.Reason);
    }

    [Fact]
    public async Task PostPublishFence_Sends_AccessRevoked_When_Access_Disappears()
    {
        using var fixture = CreateFixture([AccessLevel.Suggest, null]);

        var accepted = await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken);

        Assert.True(accepted);
        Assert.False(fixture.State.ParticipantRegistered);
        Assert.False(fixture.State.DbParticipantCounted);
        Assert.Equal(QueueCompletionCause.AccessRevokedCascade, fixture.State.ParticipantQueue?.CompletionCause);
        Assert.Equal(CloseReason.AccessRevoked, fixture.State.ParticipantQueue?.CompletionCloseReason);

        var message = await ReadJsonAsync(fixture.State.ParticipantQueue!);
        var revoked = JsonSerializer.Deserialize(
            message.Payload,
            WsJsonContext.Default.SessionAccessRevokedMessage);
        Assert.Equal(fixture.Session.Id.Value.ToString(), revoked?.SessionId);
    }

    [Fact]
    public async Task PostPublishFence_Purges_Window_Frames_When_Access_Disappears()
    {
        using var fixture = CreateFixture([AccessLevel.Suggest, null]);
        fixture.AccessOverrides.OnGetActive = call =>
        {
            if (call == 2)
            {
                var queues = fixture.Broadcaster.TryGetSession(fixture.Session.Id);
                Assert.NotNull(queues);
                Assert.True(fixture.Broadcaster.BroadcastToDownstreamClientsIfSame(
                    fixture.Session.Id,
                    queues,
                    WireMessage.EncryptedBinary([0x01, 0x02, 0x03])));
            }
        };

        var accepted = await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken);

        Assert.True(accepted);
        Assert.Equal(QueueCompletionCause.AccessRevokedCascade, fixture.State.ParticipantQueue?.CompletionCause);

        var message = await ReadJsonAsync(fixture.State.ParticipantQueue!);
        var revoked = JsonSerializer.Deserialize(
            message.Payload,
            WsJsonContext.Default.SessionAccessRevokedMessage);
        Assert.Equal(fixture.Session.Id.Value.ToString(), revoked?.SessionId);
        Assert.Null(await fixture.State.ParticipantQueue!.ReadAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task PostPublishFence_Forces_Reconnect_When_Access_Is_Downgraded()
    {
        using var fixture = CreateFixture([AccessLevel.Suggest, AccessLevel.View]);

        var accepted = await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken);

        Assert.True(accepted);
        Assert.False(fixture.State.ParticipantRegistered);
        Assert.False(fixture.State.DbParticipantCounted);
        Assert.Equal(QueueCompletionCause.AccessRefresh, fixture.State.ParticipantQueue?.CompletionCause);
        Assert.Null(fixture.State.ParticipantQueue?.CompletionCloseReason);
        Assert.Null(await fixture.State.ParticipantQueue!.ReadAsync(TestContext.Current.CancellationToken));
        Assert.Null(fixture.State.SessionQueues?.GetParticipantQueue(fixture.State.ConnectionId));
    }

    [Fact]
    public async Task Capacity_Rejection_Rolls_Back_And_Releases_Lifecycle_Before_Bounded_Close()
    {
        using var fixture = CreateFixture(
            [AccessLevel.View],
            rosterCapacityReached: true,
            blockCloseOutput: true);

        var accepting = fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken);
        await fixture.WebSocket.CloseOutputStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);
        await using (await fixture.LifecycleGate.AcquireAsync(
            fixture.Session.Id,
            TestContext.Current.CancellationToken))
        {
            Assert.Equal(1, fixture.LifecycleGate.TestActiveSessionCount());
        }
        Assert.False(await accepting.WaitAsync(
            TimeSpan.FromSeconds(1),
            TestContext.Current.CancellationToken));

        for (var index = 0; index < 50; index++)
        {
            Assert.True(await fixture.Sessions.TryIncrementParticipantCountAsync(
                fixture.Session.Id,
                fixture.Session.StartedAt,
                50,
                TestContext.Current.CancellationToken));
        }
        Assert.False(await fixture.Sessions.TryIncrementParticipantCountAsync(
            fixture.Session.Id,
            fixture.Session.StartedAt,
            50,
            TestContext.Current.CancellationToken));
        Assert.Equal(WebSocketState.Aborted, fixture.WebSocket.State);
        Assert.Equal(0, fixture.LifecycleGate.TestActiveSessionCount());
    }

    [Fact]
    public async Task OwnerParticipant_Limit_Releases_After_IncarnationSafe_Teardown()
    {
        using var fixture = CreateFixture(
            [],
            ownerParticipant: true,
            ownerCapacityReached: true);

        Assert.False(await fixture.Handshake.TryAcceptAsync(
            fixture.WebSocket,
            fixture.State,
            TestContext.Current.CancellationToken));
        Assert.Equal(
            CloseReason.CapacityReached.ToWire(),
            fixture.WebSocket.LastCloseDescription);

        fixture.Runtimes.TryGet(fixture.Session.Id)!
            .Participants.RemoveOwnerParticipant("existing-owner-0");
        var retrySocket = new JoinWebSocket(
            fixture.Session.Id.Value.ToString("D"),
            expectedIncarnationId: fixture.Session.IncarnationId,
            relayProtocolVersion: RelayProtocolVersions.Current);
        var retryState = new ParticipantConnectionState(
            "owner-retry",
            fixture.Session.Id.Value.ToString(),
            fixture.Session.OwnerUserId);

        Assert.True(await fixture.Handshake.TryAcceptAsync(
            retrySocket,
            retryState,
            TestContext.Current.CancellationToken));
        Assert.Equal(
            8,
            fixture.Runtimes.TryGet(fixture.Session.Id)!
                .Demand.GetStreamDemand().OwnerParticipantCount);
    }

    private static Fixture CreateFixture(
        IReadOnlyList<AccessLevel?> accessSequence,
        int joinPaddingBytes = 0,
        bool rosterCapacityReached = false,
        bool blockCloseOutput = false,
        bool ownerParticipant = false,
        bool ownerCapacityReached = false,
        bool currentIncarnation = true,
        Guid? expectedIncarnationId = null,
        bool omitExpectedIncarnation = false,
        int? relayProtocolVersion = null,
        SessionStatus durableStatus = SessionStatus.Live,
        bool publishRuntime = true,
        string? omittedJoinCursorField = null,
        ISemanticRelayRepository? semanticRelay = null,
        uint lastSeenKeyGeneration = 0)
    {
        var ownerId = UserId.New();
        var viewerId = ownerParticipant ? ownerId : UserId.New();
        var session = Session.Create(
            SessionId.New(),
            Guid.CreateVersion7(),
            incarnationGeneration: 1,
            currentIncarnation
                ? Session.CurrentIncarnationProtocolVersion
                : Session.LegacyIncarnationProtocolVersion,
            ownerId,
            "Session",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            new Kodosi.Infrastructure.Crypto.OwnerSessionSecretHasher().Hash("owner-secret"));
        SetDurableStatus(session, durableStatus);

        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var accessOverrides = new SequencedAccessOverrideRepository(session.Id, viewerId, ownerId, accessSequence);
        var ports = publishRuntime ? runtimes.CreateRuntime(session.Id) : null;
        ports?.Host.SetStatus(SessionStatus.Live);
        if (rosterCapacityReached)
        {
            Assert.NotNull(ports);
            for (var index = 0; index < 50; index++)
            {
                Assert.True(ports.Participants.TryAddSharedParticipant(
                    $"existing-{index}",
                    50,
                    out _));
            }
        }
        if (ownerCapacityReached)
        {
            Assert.NotNull(ports);
            for (var index = 0;
                 index < 8;
                 index++)
            {
                Assert.True(ports.Participants.TryAddOwnerParticipant(
                    $"existing-owner-{index}",
                    8,
                    out _));
            }
        }

        var viewerDevice = TestDeviceCertificate.CreateDevice(
            viewerId,
            "viewer-device",
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Viewer",
            signerDeviceId: "viewer-device",
            issuedAt: DateTimeOffset.UtcNow.AddMinutes(-1),
            expiresAt: null);
        var viewerDeviceList = TestDeviceList.Create(
            viewerId,
            1,
            """[{"deviceId":"viewer-device","signerDeviceId":"viewer-device"}]""",
            "viewer-device",
            [2],
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
            null);
        var sessions = new FakeSessionRepository(session);
        var devices = new FakeUserDeviceRepository(viewerDevice);
        var deviceLists = new FakeUserDeviceListRepository(viewerDeviceList);
        var accessService = SecurityAuthorizationTestFactory.CreateAccessService(
            accessOverrideRepository: accessOverrides);
        var scopedLifetime = new ScopedLifetimeProbe();
        var services = new ServiceCollection()
            .AddScoped(_ => scopedLifetime.CreateLease())
            .AddScoped<ISessionRepository>(services =>
            {
                _ = services.GetRequiredService<ScopedLifetimeLease>();
                return sessions;
            })
            .AddScoped<IUserDeviceRepository>(services =>
            {
                _ = services.GetRequiredService<ScopedLifetimeLease>();
                return devices;
            })
            .AddScoped<IUserDeviceListRepository>(services =>
            {
                _ = services.GetRequiredService<ScopedLifetimeLease>();
                return deviceLists;
            })
            .AddScoped<IPopSignatureVerifier>(_ => new DeterministicPopVerifier())
            .AddScoped<SessionAccessService>(services =>
            {
                _ = services.GetRequiredService<ScopedLifetimeLease>();
                return accessService;
            })
            .BuildServiceProvider();
        var scopeFactory = services.GetRequiredService<IServiceScopeFactory>();
        var lifecycleGate = new SessionLifecycleGate();
        var handshake = new ParticipantHandshake(
            new ConnectionRegistry(),
            runtimes,
            new RealtimeDeviceAuthorizationReader(scopeFactory, TimeProvider.System),
            scopeFactory,
            broadcaster,
            new SessionReplaySender(metrics),
            metrics,
            lifecycleGate,
            NullLogger<ParticipantHandshake>.Instance,
            repairTracker: new RealtimePersistenceRepairTracker(TimeProvider.System),
            semanticRelay: semanticRelay ?? EmptySemanticRelayRepository.Instance,
            handshakeTimeout: TimeSpan.FromMilliseconds(25));

        return new Fixture(
            session,
            runtimes,
            broadcaster,
            accessOverrides,
            handshake,
            new JoinWebSocket(
                session.Id.Value.ToString("D"),
                joinPaddingBytes,
                blockCloseOutput,
                omitExpectedIncarnation
                    ? null
                    : expectedIncarnationId
                        ?? (currentIncarnation ? session.IncarnationId : null),
                relayProtocolVersion ?? RelayProtocolVersions.Current,
                omittedJoinCursorField,
                lastSeenKeyGeneration),
            new ParticipantConnectionState("viewer-1", session.Id.Value.ToString(), viewerId),
            sessions,
            lifecycleGate,
            services,
            scopedLifetime);
    }

    private static void SetDurableStatus(Session session, SessionStatus status)
    {
        switch (status)
        {
            case SessionStatus.Pending:
                break;
            case SessionStatus.Live:
                session.ActivateHost("test-host");
                session.ReleaseHostSlot("test-host");
                break;
            case SessionStatus.Reconnecting:
                session.ActivateHost("test-host");
                session.ReleaseHostSlot("test-host");
                session.MarkReconnecting();
                break;
            case SessionStatus.Ended:
                session.End();
                break;
            default:
                throw new ArgumentOutOfRangeException(nameof(status), status, null);
        }
    }

    private static async Task<WireMessage> ReadJsonAsync(RelayClientSendQueue queue)
    {
        var message = await queue.ReadAsync(TestContext.Current.CancellationToken);
        Assert.NotNull(message);
        Assert.Equal(WireMessageKind.Json, message.Value.Kind);
        return message.Value;
    }

    private sealed record Fixture(
        Session Session,
        LiveSessionStateDirectory Runtimes,
        SessionBroadcaster Broadcaster,
        SequencedAccessOverrideRepository AccessOverrides,
        ParticipantHandshake Handshake,
        JoinWebSocket WebSocket,
        ParticipantConnectionState State,
        FakeSessionRepository Sessions,
        SessionLifecycleGate LifecycleGate,
        ServiceProvider Services,
        ScopedLifetimeProbe ScopedLifetime) : IDisposable
    {
        public void Dispose()
        {
            State.LinkedCts?.Dispose();
            State.DisposeDeviceAuthorizationLifetime();
            WebSocket.Dispose();
            Services.Dispose();
        }
    }

    private sealed class HandshakeSemanticRelayRepository : ISemanticRelayRepository
    {
        public IReadOnlyList<SemanticRelayReceipt> Pending { get; set; } = [];
        public Action? OnList { get; set; }
        public (SessionId, Guid, UserId, string)? Query { get; private set; }

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
            Query = (sessionId, incarnationId, requesterUserId, requesterDeviceId);
            OnList?.Invoke();
            return Task.FromResult(Pending);
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

    private sealed class SequencedAccessOverrideRepository(
        SessionId sessionId,
        UserId actorId,
        UserId ownerId,
        IReadOnlyList<AccessLevel?> levels) : IAccessOverrideRepository
    {
        private readonly SessionId _sessionId = sessionId;
        private readonly UserId _actorId = actorId;
        private readonly UserId _ownerId = ownerId;
        private readonly IReadOnlyList<AccessLevel?> _levels = levels;
        private int _getActiveCalls;

        public Action<int>? OnGetActive { get; set; }

        public Task<SessionAccessOverride?> GetActiveAsync(
            SessionId sessionId,
            UserId actorUserId,
            CancellationToken ct = default)
        {
            _getActiveCalls++;
            OnGetActive?.Invoke(_getActiveCalls);
            var level = _levels[Math.Min(_getActiveCalls - 1, _levels.Count - 1)];
            return Task.FromResult(
                sessionId == _sessionId && actorUserId == _actorId && level.HasValue
                    ? SessionAccessOverride.Create(_sessionId, _actorId, level.Value, _ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow)
                    : null);
        }

        public Task<SessionAccessOverride?> GetAsync(
            SessionId sessionId,
            UserId actorUserId,
            CancellationToken ct = default) =>
            GetActiveAsync(sessionId, actorUserId, ct);

        public Task<IReadOnlyList<SessionAccessOverride>> GetActiveForActorAsync(
            UserId actorUserId,
            IReadOnlyList<SessionId> sessionIds,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<SessionAccessOverride>>([]);

        public Task<IReadOnlyList<SessionAccessOverride>> GetActiveBySessionAsync(
            SessionId sessionId,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<SessionAccessOverride>>([]);

        public Task<IReadOnlyList<SessionAccessOverride>> GetUnrevokedBySessionAsync(
            SessionId sessionId,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<SessionAccessOverride>>([]);

        public Task AddAsync(SessionAccessOverride accessOverride, CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task UpdateAsync(SessionAccessOverride accessOverride, CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task UpdateRangeAsync(
            IReadOnlyCollection<SessionAccessOverride> accessOverrides,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<UserId>> GetActiveRelatedUserIdsAsync(
            UserId userId,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<UserId>>([]);

        public Task<bool> HasActiveRelationshipAsync(UserId userA, UserId userB, CancellationToken ct = default) =>
            Task.FromResult(false);

        public Task<IReadOnlyList<SessionAccessOverride>> GetExpiredUnrevokedAsync(
            DateTimeOffset now,
            int limit,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<bool> TryRevokeExpiredAsync(
            SessionId sessionId,
            UserId actorUserId,
            DateTimeOffset observedExpiry,
            DateTimeOffset revokedAt,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
    }

    private sealed class DeterministicPopVerifier : IPopSignatureVerifier
    {
        public bool Verify(
            ReadOnlySpan<byte> publicKey,
            ReadOnlySpan<byte> message,
            ReadOnlySpan<byte> signature) =>
            !publicKey.IsEmpty && !message.IsEmpty && signature.SequenceEqual(new byte[] { 1 });
    }

    private sealed class JoinWebSocket
        : WebSocket
    {
        private readonly Queue<byte[]> _messages;
        private readonly bool _blockCloseOutput;
        private int _offset;
        private WebSocketCloseStatus? _closeStatus;
        private WebSocketState _state = WebSocketState.Open;

        public JoinWebSocket(
            string sessionId,
            int paddingBytes = 0,
            bool blockCloseOutput = false,
            Guid? expectedIncarnationId = null,
            int? relayProtocolVersion = null,
            string? omittedCursorField = null,
            uint lastSeenKeyGeneration = 0)
        {
            _blockCloseOutput = blockCloseOutput;
            var expectedIncarnationField = expectedIncarnationId is null
                ? string.Empty
                : $",\"expectedIncarnationId\":\"{expectedIncarnationId:D}\"";
            var protocolVersionField = relayProtocolVersion is null
                ? string.Empty
                : $",\"relayProtocolVersion\":{relayProtocolVersion.Value}";
            var checkpointCursor = omittedCursorField == "checkpointRevision"
                ? string.Empty
                : "\"checkpointRevision\":0,";
            var presentationCursor = omittedCursorField == "presentationRevision"
                ? string.Empty
                : "\"presentationRevision\":0,";
            var rawCursor = omittedCursorField == "nextSequence"
                ? string.Empty
                : "\"nextSequence\":0,";
            var joinMessage = System.Text.Encoding.UTF8.GetBytes(
                $$"""
                {"type":"participant.join",{{checkpointCursor}}{{presentationCursor}}{{rawCursor}}"lastSeenKeyGeneration":{{lastSeenKeyGeneration}},"deviceId":"viewer-device"{{expectedIncarnationField}}{{protocolVersionField}},"padding":"{{new string('x', paddingBytes)}}"}
                """);
            var proof = JsonSerializer.SerializeToUtf8Bytes(new DeviceProofResponseMessage(
                "device.proof",
                "viewer-device",
                sessionId,
                expectedIncarnationId,
                Convert.ToBase64String(new byte[] { 1 })));
            _messages = new Queue<byte[]>([proof, joinMessage]);
        }

        public int JoinMessageLength => _messages.Last().Length;
        public string? LastCloseDescription { get; private set; }
        public TaskCompletionSource CloseOutputStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public override WebSocketCloseStatus? CloseStatus => _closeStatus;
        public override string? CloseStatusDescription => LastCloseDescription;
        public override WebSocketState State => _state;
        public override string? SubProtocol => null;

        public override void Abort() => _state = WebSocketState.Aborted;

        public override Task CloseAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken)
        {
            _closeStatus = closeStatus;
            LastCloseDescription = statusDescription;
            _state = WebSocketState.Closed;
            return Task.CompletedTask;
        }

        public override Task CloseOutputAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken)
        {
            _closeStatus = closeStatus;
            LastCloseDescription = statusDescription;
            CloseOutputStarted.TrySetResult();
            if (_blockCloseOutput)
            {
                return new TaskCompletionSource(
                    TaskCreationOptions.RunContinuationsAsynchronously).Task;
            }
            _state = WebSocketState.CloseSent;
            return Task.CompletedTask;
        }

        public override void Dispose() { }

        public override Task<WebSocketReceiveResult> ReceiveAsync(
            ArraySegment<byte> buffer,
            CancellationToken cancellationToken)
        {
            if (_messages.Count == 0)
            {
                return Task.FromResult(new WebSocketReceiveResult(0, WebSocketMessageType.Close, true));
            }

            var current = _messages.Peek();
            var count = Math.Min(buffer.Count, current.Length - _offset);
            Buffer.BlockCopy(current, _offset, buffer.Array!, buffer.Offset, count);
            _offset += count;
            var endOfMessage = _offset == current.Length;
            if (endOfMessage)
            {
                _messages.Dequeue();
                _offset = 0;
            }
            return Task.FromResult(new WebSocketReceiveResult(
                count,
                WebSocketMessageType.Text,
                endOfMessage));
        }

        public override Task SendAsync(
            ArraySegment<byte> buffer,
            WebSocketMessageType messageType,
            bool endOfMessage,
            CancellationToken cancellationToken) =>
            Task.CompletedTask;
    }
}
