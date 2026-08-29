using System.Diagnostics.Metrics;
using Kodosi.Application;
using Kodosi.Host.Realtime;

namespace Kodosi.Host.Observability;

public sealed class OperationalMetrics : IDisposable
{
    public const string MeterName = "Kodosi.Host";

    private static readonly Meter Meter = new(MeterName);
    private static readonly Counter<long> ReconnectCounter = Meter.CreateCounter<long>(
        "Kodosi.realtime.reconnects",
        description: "Host reconnect transitions observed by the backend connection bridge.");
    private static readonly Counter<long> ResyncCounter = Meter.CreateCounter<long>(
        "Kodosi.realtime.participant_resyncs",
        description: "Participant resync snapshots queued after lag detection.");
    private static readonly Counter<long> ActionRejectCounter = Meter.CreateCounter<long>(
        "Kodosi.realtime.action_rejections",
        description: "Participant actions rejected by runtime access or session state.");
    private static readonly Counter<long> QueueOverflowCounter = Meter.CreateCounter<long>(
        "Kodosi.realtime.queue_overflows",
        description: "Bounded realtime send queues that overflowed, tagged by lane.");
    private static readonly Counter<long> CapacityRejectCounter = Meter.CreateCounter<long>(
        "Kodosi.realtime.capacity_rejections",
        description: "Participant join requests rejected due to runtime capacity.");
    private static readonly Counter<long> FrameAdmissionRejectCounter = Meter.CreateCounter<long>(
        "Kodosi.realtime.frame_admission_rejections",
        description: "Encrypted host frames dropped by the (keyGen, counter) admission gate (replay, regression, or cross-generation). Elevated rates signal a compromised host or broken key-rotation pipeline.");
    private static readonly Counter<long> ControlQueueOverflowDisconnectCounter = Meter.CreateCounter<long>(
        "Kodosi.realtime.control_queue_overflows",
        description: "Participant control send queues that exceeded their cap; the lagging participant was disconnected rather than dropping non-droppable control traffic.");
    private static readonly Counter<long> JwtRevalidationTransientCounter = Meter.CreateCounter<long>(
        "Kodosi.auth.jwt_revalidation_transient",
        description: "JWT revalidation encountered a transient non-SecurityTokenException and kept the socket alive (e.g. JWKS fetch blip). Elevated rates indicate a prolonged JWKS outage where expired/revoked tokens may remain accepted.");
    private static readonly Counter<long> JwtRevalidationInvalidCounter = Meter.CreateCounter<long>(
        "Kodosi.auth.jwt_revalidation_invalid",
        description: "JWT revalidation determined the token is definitively invalid (expired, bad signature, signing-key rotated, etc.) and the socket was torn down with auth_revoked. Pair with `jwt_revalidation_transient` to compute the definitive/transient ratio — a spike in transient without a corresponding spike in invalid is a JWKS outage signal.");
    private static readonly Counter<long> JwtRevalidationTransientThresholdCounter = Meter.CreateCounter<long>(
        "Kodosi.auth.jwt_revalidation_transient_threshold_closed",
        description: "Sockets force-closed after N consecutive transient JWT revalidation failures — fail-close on suspected JWKS manipulation. Nonzero rate should alert: either the JWKS endpoint is wedged or an attacker is inducing transient failures to extend a revoked token's streaming window.");
    private static readonly Counter<long> IdentityResetCounter = Meter.CreateCounter<long>(
        "Kodosi.identity.reset",
        description: "Successful DELETE /me/identity invocations. Pair with audit log for user-level forensics.");
    private static readonly Counter<long> HostSlotTimeoutReleaseCounter = Meter.CreateCounter<long>(
        "Kodosi.realtime.host_slot_timeout_releases",
        description: "Host session slots released by HeartbeatTimeoutHostedService after the host's WebSocket died without a clean close. Elevated rates suggest network-layer issues or host processes being SIGKILL'd mid-session.");
    private static readonly Counter<long> KeyRotationRejectionCounter = Meter.CreateCounter<long>(
        "Kodosi.realtime.key_rotation_rejections",
        description: "key.rotation messages rejected because the incoming generation did not strictly exceed the current one. Elevated rates signal a host bug or a replayed attack.");
    private static readonly Counter<long> ParticipantDecrementFailureCounter = Meter.CreateCounter<long>(
        "Kodosi.realtime.participant_decrement_failures",
        description: "DecrementParticipantCountAsync threw (typically Postgres down). Heartbeat sweep backstops but rate should be near zero in steady state.");
    private static readonly Counter<long> AccessCascadeDisconnectCounter = Meter.CreateCounter<long>(
        "Kodosi.access.cascade_disconnects",
        description: "Shared-participant sockets force-closed by SessionAccessRefreshService (unfriend, room-member removal, scope-narrowing, access-override revoke). Useful for auditing revocation fan-out cost.");
    private static readonly Counter<long> FriendRequestThrottleRejectionCounter = Meter.CreateCounter<long>(
        "Kodosi.friend.request_throttle_rejections",
        description: "Outgoing friend requests rejected by the per-user rate limiter.");
    private static readonly Histogram<double> HeartbeatSweepDurationMs = Meter.CreateHistogram<double>(
        "Kodosi.realtime.heartbeat_sweep_duration_ms",
        unit: "ms",
        description: "End-to-end duration of a HeartbeatTimeoutHostedService sweep. p95 drift upward correlates with DB latency or an expanding live-session set.");
    private static readonly Counter<long> DrainStartedCounter = Meter.CreateCounter<long>(
        "Kodosi.host.drain_started",
        description: "GracefulShutdownHostedService started a drain. Steady rate should match deploy cadence.");
    private static readonly Counter<long> DrainCompletedCounter = Meter.CreateCounter<long>(
        "Kodosi.host.drain_completed",
        description: "GracefulShutdownHostedService finished a drain cleanly. Divergence from drain_started indicates drains that exceeded the hosting shutdown timeout.");
    private static readonly Histogram<double> DrainDurationMs = Meter.CreateHistogram<double>(
        "Kodosi.host.drain_duration_ms",
        unit: "ms",
        description: "End-to-end duration of a graceful shutdown drain (from first batch close to last wait). p99 against the configured shutdown-timeout flags misconfigured operators or pathological socket counts.");
    private static readonly Counter<long> HostWebSocketCloseCounter = Meter.CreateCounter<long>(
        "Kodosi.realtime.host_ws_closes",
        description: "Host WebSocket close events, tagged by typed close reason. Distribution by reason surfaces authentication churn, capacity contention, handshake protocol errors, and normal shutdowns.");
    private static readonly Histogram<double> JwtRevalidationDurationMs = Meter.CreateHistogram<double>(
        "Kodosi.auth.jwt_revalidation_duration_ms",
        unit: "ms",
        description: "End-to-end duration of `JwtRevalidator.RevalidateAsync` — covers JWKS fetch + signature verify + claim parsing. p95 trending upward indicates a slow JWKS endpoint OR slow crypto; pair with `jwt_revalidation_transient` to isolate transient failures.");
    private static readonly Counter<long> PopFailureCounter = Meter.CreateCounter<long>(
        "Kodosi.identity.pop_verification_failures",
        description: "Proof-of-possession verification failures, tagged by `reason` (challenge_expired, signature_invalid, signer_not_enrolled) and `endpoint` (identity_reset, device_registration, device_list_replacement). Distinguishing reason + endpoint separates attack attempts (signature_invalid on identity_reset) from client bugs (challenge_expired on device_registration).");
    private static readonly Counter<long> SessionKeySkewRejectionCounter = Meter.CreateCounter<long>(
        "Kodosi.sharing.session_key_skew_rejections",
        description: "Session-key blobs rejected for IssuedAtMs outside the 15-minute skew window. Steady rate = client clock drift; spike without audience change = replay attempt.");
    private long _reconnectCount;
    private long _resyncCount;
    private long _actionRejectCount;
    private long _queueOverflowCount;
    private long _capacityRejectCount;
    private long _frameAdmissionRejectCount;
    private long _controlQueueOverflowDisconnectCount;
    private long _jwtRevalidationTransientCount;
    private long _jwtRevalidationInvalidCount;
    private long _jwtRevalidationTransientThresholdCount;
    private long _identityResetCount;
    private long _hostSlotTimeoutReleaseCount;
    private long _keyRotationRejectionCount;
    private long _participantDecrementFailureCount;
    private long _accessCascadeDisconnectCount;
    private long _friendRequestThrottleRejectionCount;
    private long _drainStartedCount;
    private long _drainCompletedCount;
    private long _popVerificationFailureCount;
    private long _sessionKeySkewRejectionCount;
    private long _hostWebSocketCloseCount;

    private readonly ILiveSessionStateDirectory _runtimes;
    private readonly Meter _observableMeter = new(MeterName);

    public OperationalMetrics(ILiveSessionStateDirectory runtimes)
    {
        _runtimes = runtimes;
        _observableMeter.CreateObservableGauge(
            "Kodosi.realtime.active_sessions",
            GetActiveSessionCount,
            description: "Currently active in-memory coding session runtimes.");
        _observableMeter.CreateObservableGauge(
            "Kodosi.realtime.active_participants",
            GetActiveParticipantCount,
            description: "Currently connected participants across all active session runtimes.");
    }

    public void Dispose() => _observableMeter.Dispose();

    public void RecordReconnect()
    {
        Interlocked.Increment(ref _reconnectCount);
        ReconnectCounter.Add(1);
    }

    public void RecordResync()
    {
        Interlocked.Increment(ref _resyncCount);
        ResyncCounter.Add(1);
    }

    public void RecordActionRejected()
    {
        Interlocked.Increment(ref _actionRejectCount);
        ActionRejectCounter.Add(1);
    }

    public void RecordQueueOverflow(QueueOverflowLane lane)
    {
        Interlocked.Increment(ref _queueOverflowCount);
        QueueOverflowCounter.Add(
            1,
            new KeyValuePair<string, object?>("lane", lane.ToTag()));
    }

    public void RecordCapacityRejected()
    {
        Interlocked.Increment(ref _capacityRejectCount);
        CapacityRejectCounter.Add(1);
    }

    public void RecordFrameAdmissionRejected()
    {
        Interlocked.Increment(ref _frameAdmissionRejectCount);
        FrameAdmissionRejectCounter.Add(1);
    }

    public void RecordControlQueueOverflowDisconnect()
    {
        RecordQueueOverflow(QueueOverflowLane.Control);
        Interlocked.Increment(ref _controlQueueOverflowDisconnectCount);
        ControlQueueOverflowDisconnectCounter.Add(1);
    }

    public void RecordJwtRevalidationTransient()
    {
        Interlocked.Increment(ref _jwtRevalidationTransientCount);
        JwtRevalidationTransientCounter.Add(1);
    }

    public void RecordJwtRevalidationInvalid()
    {
        Interlocked.Increment(ref _jwtRevalidationInvalidCount);
        JwtRevalidationInvalidCounter.Add(1);
    }

    public void RecordJwtRevalidationTransientThresholdReached()
    {
        Interlocked.Increment(ref _jwtRevalidationTransientThresholdCount);
        JwtRevalidationTransientThresholdCounter.Add(1);
    }

    public void RecordIdentityReset()
    {
        Interlocked.Increment(ref _identityResetCount);
        IdentityResetCounter.Add(1);
    }

    public void RecordHostSlotTimeoutRelease()
    {
        Interlocked.Increment(ref _hostSlotTimeoutReleaseCount);
        HostSlotTimeoutReleaseCounter.Add(1);
    }

    public void RecordKeyRotationRejection()
    {
        Interlocked.Increment(ref _keyRotationRejectionCount);
        KeyRotationRejectionCounter.Add(1);
    }

    public void RecordParticipantDecrementFailure()
    {
        Interlocked.Increment(ref _participantDecrementFailureCount);
        ParticipantDecrementFailureCounter.Add(1);
    }

    public void RecordAccessCascadeDisconnect()
    {
        Interlocked.Increment(ref _accessCascadeDisconnectCount);
        AccessCascadeDisconnectCounter.Add(1);
    }

    public void RecordFriendRequestThrottleRejection()
    {
        Interlocked.Increment(ref _friendRequestThrottleRejectionCount);
        FriendRequestThrottleRejectionCounter.Add(1);
    }

    public void RecordHeartbeatSweepDuration(double durationMs)
    {
        HeartbeatSweepDurationMs.Record(durationMs);
    }

    public void RecordDrainStarted()
    {
        Interlocked.Increment(ref _drainStartedCount);
        DrainStartedCounter.Add(1);
    }

    public void RecordDrainCompleted()
    {
        Interlocked.Increment(ref _drainCompletedCount);
        DrainCompletedCounter.Add(1);
    }

    public void RecordDrainDuration(double durationMs)
    {
        DrainDurationMs.Record(durationMs);
    }

    public void RecordHostWebSocketClose(CloseReason reason)
    {
        Interlocked.Increment(ref _hostWebSocketCloseCount);
        HostWebSocketCloseCounter.Add(
            1,
            new KeyValuePair<string, object?>("reason", reason.ToWire()));
    }

    public void RecordJwtRevalidationDuration(double durationMs)
    {
        JwtRevalidationDurationMs.Record(durationMs);
    }

    public void RecordPopFailure(string reason, string endpoint)
    {
        Interlocked.Increment(ref _popVerificationFailureCount);
        PopFailureCounter.Add(
            1,
            new KeyValuePair<string, object?>("reason", reason),
            new KeyValuePair<string, object?>("endpoint", endpoint));
    }

    public void RecordSessionKeySkewRejection()
    {
        Interlocked.Increment(ref _sessionKeySkewRejectionCount);
        SessionKeySkewRejectionCounter.Add(1);
    }

    public OperationalMetricsSnapshot Snapshot()
    {
        return new OperationalMetricsSnapshot(
            ActiveSessions: GetActiveSessionCount(),
            ActiveParticipants: GetActiveParticipantCount(),
            ReconnectCount: Interlocked.Read(ref _reconnectCount),
            ResyncCount: Interlocked.Read(ref _resyncCount),
            ActionRejectCount: Interlocked.Read(ref _actionRejectCount),
            QueueOverflowCount: Interlocked.Read(ref _queueOverflowCount),
            CapacityRejectCount: Interlocked.Read(ref _capacityRejectCount),
            FrameAdmissionRejectCount: Interlocked.Read(ref _frameAdmissionRejectCount),
            ControlQueueOverflowDisconnectCount: Interlocked.Read(ref _controlQueueOverflowDisconnectCount),
            JwtRevalidationTransientCount: Interlocked.Read(ref _jwtRevalidationTransientCount),
            JwtRevalidationInvalidCount: Interlocked.Read(ref _jwtRevalidationInvalidCount),
            JwtRevalidationTransientThresholdCount: Interlocked.Read(ref _jwtRevalidationTransientThresholdCount),
            IdentityResetCount: Interlocked.Read(ref _identityResetCount),
            HostSlotTimeoutReleaseCount: Interlocked.Read(ref _hostSlotTimeoutReleaseCount),
            KeyRotationRejectionCount: Interlocked.Read(ref _keyRotationRejectionCount),
            ParticipantDecrementFailureCount: Interlocked.Read(ref _participantDecrementFailureCount),
            AccessCascadeDisconnectCount: Interlocked.Read(ref _accessCascadeDisconnectCount),
            FriendRequestThrottleRejectionCount: Interlocked.Read(ref _friendRequestThrottleRejectionCount),
            DrainStartedCount: Interlocked.Read(ref _drainStartedCount),
            DrainCompletedCount: Interlocked.Read(ref _drainCompletedCount),
            PopVerificationFailureCount: Interlocked.Read(ref _popVerificationFailureCount),
            SessionKeySkewRejectionCount: Interlocked.Read(ref _sessionKeySkewRejectionCount),
            HostWebSocketCloseCount: Interlocked.Read(ref _hostWebSocketCloseCount));
    }

    private int GetActiveSessionCount() => _runtimes.GetActiveSessions().Count;

    private int GetActiveParticipantCount()
    {
        return _runtimes.GetActiveSessions()
            .Select(sessionId => _runtimes.TryGet(sessionId)?.Demand.GetStreamDemand().ParticipantCount ?? 0)
            .Sum();
    }
}

public sealed record OperationalMetricsSnapshot(
    int ActiveSessions,
    int ActiveParticipants,
    long ReconnectCount,
    long ResyncCount,
    long ActionRejectCount,
    long QueueOverflowCount,
    long CapacityRejectCount,
    long FrameAdmissionRejectCount,
    long ControlQueueOverflowDisconnectCount,
    long JwtRevalidationTransientCount,
    long JwtRevalidationInvalidCount,
    long JwtRevalidationTransientThresholdCount,
    long IdentityResetCount,
    long HostSlotTimeoutReleaseCount,
    long KeyRotationRejectionCount,
    long ParticipantDecrementFailureCount,
    long AccessCascadeDisconnectCount,
    long FriendRequestThrottleRejectionCount,
    long DrainStartedCount,
    long DrainCompletedCount,
    long PopVerificationFailureCount,
    long SessionKeySkewRejectionCount,
    long HostWebSocketCloseCount);

public enum QueueOverflowLane
{
    Host,
    ParticipantFrame,
    Control,
    UserEvents,
    Audit,
}

internal static class QueueOverflowLaneTags
{
    public static string ToTag(this QueueOverflowLane lane) =>
        lane switch
        {
            QueueOverflowLane.Host => "host",
            QueueOverflowLane.ParticipantFrame => "participant_frame",
            QueueOverflowLane.Control => "control",
            QueueOverflowLane.UserEvents => "user_events",
            QueueOverflowLane.Audit => "audit",
            _ => throw new ArgumentOutOfRangeException(nameof(lane), lane, "Unknown queue overflow lane."),
        };
}
