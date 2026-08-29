using System.Net.WebSockets;
using System.Text.Json;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging.Abstractions;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Auth;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Realtime;

using Kodosi.Host.Serialization;

namespace Kodosi.HostTests;

public sealed class UserEventBroadcasterTests
{
    [Fact]
    public async Task UserEvents_Authorizes_Device_Before_Starting_Send_Pump()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        using var services = new ServiceCollection()
            .AddScoped<IUserDeviceRepository>(_ => new FakeUserDeviceRepository())
            .AddScoped<IUserDeviceListRepository>(_ => new FakeUserDeviceListRepository())
            .AddScoped<IIdentityExposureRepository>(_ => new FakeIdentityExposureRepository())
            .AddScoped<IDeviceLinkRequestRepository>(_ => new EmptyDeviceLinkRequestRepository())
            .AddScoped<IPopSignatureVerifier>(_ => new DeterministicPopVerifier())
            .BuildServiceProvider();
        var handler = new UserEventsWebSocketHandler(
            broadcaster,
            new RealtimeDeviceAuthorizationReader(
                services.GetRequiredService<IServiceScopeFactory>(),
                TimeProvider.System),
            new AlwaysValidJwtRevalidator(),
            services.GetRequiredService<IServiceScopeFactory>(),
            metrics,
            NullLogger<UserEventsWebSocketHandler>.Instance);
        using var webSocket = new NoSendWebSocket("revoked-device");

        await handler.HandleAsync(
            webSocket,
            UserId.New(),
            accessToken: null,
            ct: TestContext.Current.CancellationToken);

        Assert.Equal(1, webSocket.SendCalls);
        Assert.Equal(CloseReason.AccessRevoked.ToWire(), webSocket.CloseDescription);
        Assert.Equal(0, broadcaster.ActiveConnectionCount);
    }

    [Fact]
    public async Task Final_Authorization_Failure_Removes_The_Registered_Connection()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var userId = UserId.New();
        var deviceId = "device-1";
        var now = DateTimeOffset.UtcNow;
        var device = TestDeviceCertificate.CreateDevice(
            userId,
            deviceId,
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Laptop",
            signerDeviceId: deviceId,
            issuedAt: now.AddMinutes(-1),
            expiresAt: now.AddMinutes(5));
        var list = TestDeviceList.Create(
            userId,
            1,
            """[{"deviceId":"device-1","signerDeviceId":"device-1"}]""",
            deviceId,
            [2],
            now.AddMinutes(-1).ToUnixTimeMilliseconds(),
            now.AddMinutes(5).ToUnixTimeMilliseconds());
        var devices = new ThrowOnSecondDeviceLookupRepository(
            new FakeUserDeviceRepository(device));
        using var services = new ServiceCollection()
            .AddScoped<IUserDeviceRepository>(_ => devices)
            .AddScoped<IUserDeviceListRepository>(_ =>
                new FakeUserDeviceListRepository(list))
            .AddScoped<IIdentityExposureRepository>(_ => new FakeIdentityExposureRepository())
            .AddScoped<IDeviceLinkRequestRepository>(_ => new EmptyDeviceLinkRequestRepository())
            .AddScoped<IPopSignatureVerifier>(_ => new DeterministicPopVerifier())
            .BuildServiceProvider();
        var handler = new UserEventsWebSocketHandler(
            broadcaster,
            new RealtimeDeviceAuthorizationReader(
                services.GetRequiredService<IServiceScopeFactory>(),
                TimeProvider.System),
            new AlwaysValidJwtRevalidator(),
            services.GetRequiredService<IServiceScopeFactory>(),
            metrics,
            NullLogger<UserEventsWebSocketHandler>.Instance);
        using var webSocket = new NoSendWebSocket(deviceId);

        await Assert.ThrowsAsync<InvalidOperationException>(() =>
            handler.HandleAsync(
                webSocket,
                userId,
                accessToken: null,
                ct: TestContext.Current.CancellationToken));

        Assert.Equal(0, broadcaster.ActiveConnectionCount);
    }

    [Fact]
    public async Task UserEvents_ReplaysOfflineIdentityWithdrawalBeforeStartingLivePump()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var recipientId = UserId.New();
        var withdrawnId = UserId.New();
        var deviceId = "recipient-device";
        var now = DateTimeOffset.UtcNow;
        var device = TestDeviceCertificate.CreateDevice(
            recipientId,
            deviceId,
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Recipient device",
            signerDeviceId: deviceId,
            issuedAt: now.AddMinutes(-1),
            expiresAt: now.AddMinutes(5));
        var list = TestDeviceList.Create(
            recipientId,
            1,
            TestDeviceList.Entries([(deviceId, deviceId)]),
            deviceId,
            [2],
            now.AddMinutes(-1).ToUnixTimeMilliseconds(),
            now.AddMinutes(5).ToUnixTimeMilliseconds());
        using var services = new ServiceCollection()
            .AddScoped<IUserDeviceRepository>(_ => new FakeUserDeviceRepository(device))
            .AddScoped<IUserDeviceListRepository>(_ => new FakeUserDeviceListRepository(list))
            .AddScoped<IIdentityExposureRepository>(_ =>
                new FakeIdentityExposureRepository(
                    new IdentityLifecycleProjection(
                        withdrawnId,
                        2,
                        null,
                        0)))
            .AddScoped<IDeviceLinkRequestRepository>(_ => new EmptyDeviceLinkRequestRepository())
            .AddScoped<IPopSignatureVerifier>(_ => new DeterministicPopVerifier())
            .BuildServiceProvider();
        var handler = new UserEventsWebSocketHandler(
            broadcaster,
            new RealtimeDeviceAuthorizationReader(
                services.GetRequiredService<IServiceScopeFactory>(),
                TimeProvider.System),
            new AlwaysValidJwtRevalidator(),
            services.GetRequiredService<IServiceScopeFactory>(),
            metrics,
            NullLogger<UserEventsWebSocketHandler>.Instance);
        using var webSocket = new CloseAfterFirstSendWebSocket(deviceId);

        await handler.HandleAsync(
            webSocket,
            recipientId,
            accessToken: null,
            ct: TestContext.Current.CancellationToken);

        Assert.Equal(2, webSocket.SentMessages.Count);
        var payload = webSocket.SentMessages[0];
        var message = JsonSerializer.Deserialize(
            payload,
            WsJsonContext.Default.UserIdentityLifecycleChangedMessage);
        Assert.Equal(withdrawnId.Value.ToString(), message?.UserId);
        Assert.Equal(2, message?.IdentityRevision);
        Assert.Equal("withdrawn", message?.State);
        Assert.Null(message?.IncarnationId);
        Assert.Equal(0, message?.Generation);
        using var snapshot = JsonDocument.Parse(webSocket.SentMessages[1]);
        Assert.Equal(
            "user.deviceLinkSnapshot",
            snapshot.RootElement.GetProperty("type").GetString());
        Assert.Empty(snapshot.RootElement.GetProperty("requests").EnumerateArray());
        Assert.Equal(0, broadcaster.ActiveConnectionCount);
    }

    [Fact]
    public async Task UserEvents_Streams_More_Than_Queue_Capacity_Of_Durable_Withdrawals()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var recipientId = UserId.New();
        var deviceId = "recipient-device";
        var now = DateTimeOffset.UtcNow;
        var device = TestDeviceCertificate.CreateDevice(
            recipientId,
            deviceId,
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Recipient device",
            signerDeviceId: deviceId,
            issuedAt: now.AddMinutes(-1),
            expiresAt: now.AddMinutes(5));
        var list = TestDeviceList.Create(
            recipientId,
            1,
            TestDeviceList.Entries([(deviceId, deviceId)]),
            deviceId,
            [2],
            now.AddMinutes(-1).ToUnixTimeMilliseconds(),
            now.AddMinutes(5).ToUnixTimeMilliseconds());
        var withdrawn = Enumerable.Range(0, 300).Select(_ => UserId.New()).ToArray();
        using var services = new ServiceCollection()
            .AddScoped<IUserDeviceRepository>(_ => new FakeUserDeviceRepository(device))
            .AddScoped<IUserDeviceListRepository>(_ => new FakeUserDeviceListRepository(list))
            .AddScoped<IIdentityExposureRepository>(_ =>
                new FakeIdentityExposureRepository(withdrawn.Select(userId =>
                    new IdentityLifecycleProjection(userId, 2, null, 0)).ToArray()))
            .AddScoped<IDeviceLinkRequestRepository>(_ => new EmptyDeviceLinkRequestRepository())
            .AddScoped<IPopSignatureVerifier>(_ => new DeterministicPopVerifier())
            .BuildServiceProvider();
        var handler = new UserEventsWebSocketHandler(
            broadcaster,
            new RealtimeDeviceAuthorizationReader(
                services.GetRequiredService<IServiceScopeFactory>(),
                TimeProvider.System),
            new AlwaysValidJwtRevalidator(),
            services.GetRequiredService<IServiceScopeFactory>(),
            metrics,
            NullLogger<UserEventsWebSocketHandler>.Instance);
        using var webSocket = new RecordingSendWebSocket(deviceId);

        await handler.HandleAsync(
            webSocket,
            recipientId,
            accessToken: null,
            ct: TestContext.Current.CancellationToken);

        Assert.Equal(301, webSocket.SentMessages.Count);
        using var snapshot = JsonDocument.Parse(webSocket.SentMessages[^1]);
        Assert.Equal(
            "user.deviceLinkSnapshot",
            snapshot.RootElement.GetProperty("type").GetString());
        Assert.Empty(snapshot.RootElement.GetProperty("requests").EnumerateArray());
        Assert.Equal(0, metrics.Snapshot().QueueOverflowCount);
        Assert.Equal(0, broadcaster.ActiveConnectionCount);
    }

    [Fact]
    public async Task PrimedRegistration_DefersConcurrentLiveEventUntilCompletion()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var recipientId = UserId.New();
        var registration = broadcaster.RegisterPrimed(
            "primed",
            recipientId,
            "recipient-device");

        broadcaster.PublishInvalidation(
            DiscoveryAudience.ForUsers([recipientId]),
            new HashSet<DiscoverySurface> { DiscoverySurface.Friends });
        using (var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25)))
        {
            await Assert.ThrowsAsync<OperationCanceledException>(
                () => registration.Queue.ReadAsync(timeout.Token));
        }
        Assert.Equal(
            ChannelByteSendQueueWriteOutcome.Enqueued,
            registration.CompletePriming());

        var liveBytes = await registration.Queue.ReadAsync(
            TestContext.Current.CancellationToken);
        Assert.NotNull(liveBytes);
        using var live = JsonDocument.Parse(liveBytes);
        Assert.Equal(
            "discovery.invalidated",
            live.RootElement.GetProperty("type").GetString());
    }

    [Fact]
    public async Task RegistrationFence_Rejects_UserEvent_Connection_Before_Send_Pump()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var userId = UserId.New();

        using (broadcaster.FenceRegistrations(userId, ["reset-device"]))
        {
            var queue = broadcaster.Register(
                "late-user-event",
                userId,
                "reset-device");

            Assert.Equal(CloseReason.AccessRevoked, queue.CompletionReason);
            Assert.Null(await queue.ReadAsync(CancellationToken.None));
            Assert.Equal(0, broadcaster.ActiveConnectionCount);
        }
    }

    [Fact]
    public async Task PublishInvalidation_Sends_To_Targeted_User_Only()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var targetUserId = UserId.New();
        var otherUserId = UserId.New();
        var targetQueue = broadcaster.Register("target", targetUserId, "target-device");
        var otherQueue = broadcaster.Register("other", otherUserId, "other-device");

        broadcaster.PublishInvalidation(
            DiscoveryAudience.ForUsers([targetUserId]),
            new HashSet<DiscoverySurface>
            {
                DiscoverySurface.RoomFeed,
                DiscoverySurface.Friends,
            });

        var payload = await targetQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(payload);

        var message = JsonSerializer.Deserialize(
            payload!,
            WsJsonContext.Default.DiscoveryInvalidatedMessage);
        Assert.Equal(
            [DiscoverySurface.Friends, DiscoverySurface.RoomFeed],
            message?.Surfaces);

        broadcaster.Remove("other");
        Assert.Null(await otherQueue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task PublishDeviceListChanged_Reaches_Audience_Only()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var targetUserId = UserId.New();
        var audienceUserId = UserId.New();
        var unrelatedUserId = UserId.New();

        var targetQueue = broadcaster.Register("target", targetUserId, "target-device");
        var audienceQueue = broadcaster.Register("audience", audienceUserId, "audience-device");
        var unrelatedQueue = broadcaster.Register("unrelated", unrelatedUserId, "unrelated-device");

        broadcaster.PublishDeviceListChanged(
            DiscoveryAudience.ForUsers([targetUserId, audienceUserId]),
            targetUserId,
            generation: 7);

        var targetPayload = await targetQueue.ReadAsync(CancellationToken.None);
        var audiencePayload = await audienceQueue.ReadAsync(CancellationToken.None);

        Assert.NotNull(targetPayload);
        Assert.NotNull(audiencePayload);

        var message = JsonSerializer.Deserialize(
            audiencePayload!,
            WsJsonContext.Default.UserDeviceListChangedMessage);
        Assert.Equal(targetUserId.Value.ToString(), message?.UserId);
        Assert.Equal(7, message?.Generation);

        broadcaster.Remove("unrelated");
        Assert.Null(await unrelatedQueue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public void PublishDeviceListChanged_Rejects_Nonpositive_Generation()
    {
        var broadcaster = new UserEventBroadcaster(
            new OperationalMetrics(new LiveSessionStateDirectory()),
            NullLogger<UserEventBroadcaster>.Instance);

        Assert.Throws<ArgumentOutOfRangeException>(() =>
            broadcaster.PublishDeviceListChanged(
                DiscoveryAudience.ForUsers([UserId.New()]),
                UserId.New(),
                generation: 0));
    }

    [Fact]
    public async Task DisconnectDevice_Closes_Only_The_Revoked_Device()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var userId = UserId.New();
        var revokedQueue = broadcaster.Register("revoked", userId, "device-revoked");
        var activeQueue = broadcaster.Register("active", userId, "device-active");
        Assert.Equal(
            ChannelByteSendQueueWriteOutcome.Enqueued,
            revokedQueue.TryEnqueue([1]));

        Assert.Equal(1, broadcaster.DisconnectDevice(userId, "device-revoked"));
        Assert.Null(await revokedQueue.ReadAsync(CancellationToken.None));
        Assert.Equal(CloseReason.AccessRevoked, revokedQueue.CompletionReason);

        broadcaster.PublishDeviceLinkRequested(
            userId,
            "ABCD-EFGH",
            "new device",
            DateTimeOffset.UtcNow.AddMinutes(5));
        Assert.NotNull(await activeQueue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task PublishDeviceListChanged_Empty_Audience_Is_Noop()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var queue = broadcaster.Register("c", UserId.New(), "device-c");

        broadcaster.PublishDeviceListChanged(
            DiscoveryAudience.ForUsers([]),
            UserId.New(),
            generation: 1);

        broadcaster.Remove("c");
        Assert.Null(await queue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task Queue_Overflow_Discards_Backlog_And_Forces_Reconnect()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var userId = UserId.New();
        var queue = broadcaster.Register(
            "lagging",
            userId,
            "lagging-device");

        for (var index = 0; index <= 256; index++)
        {
            broadcaster.PublishDeviceLinkResolved(
                userId,
                $"code-{index}",
                "approved");
        }

        Assert.True(queue.CompletionToken.IsCancellationRequested);
        Assert.Equal(CloseReason.ServerError, queue.CompletionReason);
        Assert.Null(await queue.ReadAsync(CancellationToken.None));
        Assert.Equal(1, metrics.Snapshot().QueueOverflowCount);
    }

    private sealed class EmptyDeviceLinkRequestRepository : IDeviceLinkRequestRepository
    {
        public Task<IReadOnlyList<DeviceLinkRequest>> ListPendingForUserAsync(
            UserId userId,
            DateTimeOffset now,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<DeviceLinkRequest>>([]);

        public Task AddAsync(DeviceLinkRequest request, CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<DeviceLinkRequest?> GetByDeviceCodeAsync(
            string deviceCode,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<DeviceLinkRequest?> GetByUserCodeAsync(
            string userCode,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<bool> IsUserCodeAvailableAsync(
            string userCode,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public void Update(DeviceLinkRequest request) => throw new NotSupportedException();

        public Task<DeviceLinkCancelOutcome> CancelPendingAsync(
            string userCode,
            UserId userId,
            DateTimeOffset cancelledAt,
            CancellationToken ct = default) =>
            Task.FromResult(DeviceLinkCancelOutcome.NotFound);

        public Task<DeviceLinkAcknowledgeOutcome> AcknowledgeApprovedAsync(
            Guid requestId,
            UserId userId,
            string deviceId,
            DateTimeOffset now,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<int> InvalidateApprovedBeforeGenerationAsync(
            UserId userId,
            long committedGeneration,
            Guid? excludingRequestId,
            DateTimeOffset invalidatedAt,
            CancellationToken ct = default) =>
            Task.FromResult(0);

        public Task<int> InvalidateOutstandingForUserAsync(
            UserId userId,
            DateTimeOffset invalidatedAt,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<int> DeleteStaleAsync(
            DateTimeOffset now,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
    }

    private sealed class FakeIdentityExposureRepository(
        params IdentityLifecycleProjection[] lifecycleSnapshot)
        : IIdentityExposureRepository
    {
        private readonly IReadOnlyList<IdentityLifecycleProjection> _lifecycleSnapshot =
            lifecycleSnapshot;

        public Task RecordAsync(
            UserId identityOwnerUserId,
            UserId recipientUserId,
            DateTimeOffset exposedAt,
            CancellationToken ct = default) =>
            Task.CompletedTask;

        public Task<IReadOnlyList<UserId>> GetHistoricalPeerUserIdsAsync(
            UserId userId,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<UserId>>([]);

        public Task<IReadOnlyList<IdentityLifecycleProjection>> GetLifecycleSnapshotForRecipientAsync(
            UserId recipientUserId,
            CancellationToken ct = default) =>
            Task.FromResult(_lifecycleSnapshot);
    }

    private sealed class AlwaysValidJwtRevalidator : IJwtRevalidator
    {
        public Task<JwtRevalidationResult> RevalidateAsync(
            string token,
            CancellationToken ct) =>
            Task.FromResult(JwtRevalidationResult.Valid);
    }

    private sealed class ThrowOnSecondDeviceLookupRepository(
        IUserDeviceRepository inner)
        : IUserDeviceRepository
    {
        private int _lookupCount;

        public Task<IReadOnlyList<UserDevice>> GetByUserIdAsync(
            UserId userId,
            CancellationToken ct = default) =>
            inner.GetByUserIdAsync(userId, ct);

        public Task<UserDevice?> GetByDeviceIdAsync(
            string deviceId,
            CancellationToken ct = default)
        {
            _lookupCount++;
            return _lookupCount == 2
                ? Task.FromException<UserDevice?>(
                    new InvalidOperationException("simulated authorization query failure"))
                : inner.GetByDeviceIdAsync(deviceId, ct);
        }

        public Task<IReadOnlyDictionary<string, UserId>> GetUserIdsByDeviceIdsAsync(
            IReadOnlyCollection<string> deviceIds,
            CancellationToken ct = default) =>
            inner.GetUserIdsByDeviceIdsAsync(deviceIds, ct);

        public Task<IReadOnlyList<UserDevice>> GetByUserIdsAsync(
            IReadOnlyList<UserId> userIds,
            CancellationToken ct = default) =>
            inner.GetByUserIdsAsync(userIds, ct);

        public Task AddAsync(UserDevice device, CancellationToken ct = default) =>
            inner.AddAsync(device, ct);

        public Task<int> RemoveAllForUserAsync(
            UserId userId,
            CancellationToken ct = default) =>
            inner.RemoveAllForUserAsync(userId, ct);

        public void Update(UserDevice device) => inner.Update(device);
    }

    private sealed class DeterministicPopVerifier : IPopSignatureVerifier
    {
        public bool Verify(
            ReadOnlySpan<byte> publicKey,
            ReadOnlySpan<byte> message,
            ReadOnlySpan<byte> signature) =>
            !publicKey.IsEmpty && !message.IsEmpty && signature.SequenceEqual(new byte[] { 1 });
    }

    private sealed class RecordingSendWebSocket(string deviceId) : NoSendWebSocket(deviceId)
    {
        public List<byte[]> SentMessages { get; } = [];

        public override Task SendAsync(
            ArraySegment<byte> buffer,
            WebSocketMessageType messageType,
            bool endOfMessage,
            CancellationToken cancellationToken)
        {
            if (JsonDocument.Parse(buffer).RootElement.GetProperty("type").GetString()
                == "device.proofChallenge")
            {
                return Task.CompletedTask;
            }
            SentMessages.Add(buffer.ToArray());
            return Task.CompletedTask;
        }
    }

    private sealed class CloseAfterFirstSendWebSocket(string deviceId) : NoSendWebSocket(deviceId)
    {
        public List<byte[]> SentMessages { get; } = [];

        public override Task SendAsync(
            ArraySegment<byte> buffer,
            WebSocketMessageType messageType,
            bool endOfMessage,
            CancellationToken cancellationToken)
        {
            if (JsonDocument.Parse(buffer).RootElement.GetProperty("type").GetString()
                == "device.proofChallenge")
            {
                return Task.CompletedTask;
            }
            SentMessages.Add(buffer.ToArray());
            Abort();
            return Task.CompletedTask;
        }
    }

    private class NoSendWebSocket(string deviceId) : WebSocket
    {
        private readonly byte[] _proof = System.Text.Encoding.UTF8.GetBytes(
            JsonSerializer.Serialize(new DeviceProofResponseMessage(
                "device.proof",
                deviceId,
                null,
                null,
                Convert.ToBase64String(new byte[] { 1 }))));
        private bool _proofRead;
        private WebSocketState _state = WebSocketState.Open;
        private WebSocketCloseStatus? _closeStatus;

        public virtual int SendCalls { get; protected set; }
        public string? CloseDescription { get; private set; }
        public override WebSocketCloseStatus? CloseStatus => _closeStatus;
        public override string? CloseStatusDescription => CloseDescription;
        public override WebSocketState State => _state;
        public override string? SubProtocol => null;

        public override void Abort() => _state = WebSocketState.Aborted;

        public override Task CloseAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken)
        {
            _closeStatus = closeStatus;
            CloseDescription = statusDescription;
            _state = WebSocketState.Closed;
            return Task.CompletedTask;
        }

        public override Task CloseOutputAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken)
        {
            _closeStatus = closeStatus;
            CloseDescription = statusDescription;
            _state = WebSocketState.CloseSent;
            return Task.CompletedTask;
        }

        public override void Dispose()
        {
        }

        public override Task<WebSocketReceiveResult> ReceiveAsync(
            ArraySegment<byte> buffer,
            CancellationToken cancellationToken)
        {
            if (!_proofRead)
            {
                _proofRead = true;
                Array.Copy(_proof, buffer.Array!, _proof.Length);
                return Task.FromResult(new WebSocketReceiveResult(
                    _proof.Length,
                    WebSocketMessageType.Text,
                    endOfMessage: true));
            }
            return Task.FromResult(new WebSocketReceiveResult(
                0,
                WebSocketMessageType.Close,
                endOfMessage: true));
        }

        public override Task SendAsync(
            ArraySegment<byte> buffer,
            WebSocketMessageType messageType,
            bool endOfMessage,
            CancellationToken cancellationToken)
        {
            SendCalls++;
            return Task.CompletedTask;
        }
    }
}
