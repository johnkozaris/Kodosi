using Kodosi.Domain;

namespace Kodosi.Application;

public interface ILiveSessionHostState
{
    SessionStatus Status { get; }
    bool HostConnected { get; }
    bool HostReady { get; }
    string? HostConnectionId { get; }
    DateTimeOffset? SessionStartedAt { get; }
    Guid SessionIncarnationId { get; }

    void SetStatus(SessionStatus status);
    bool TryClaimHost(string connectionId, CancellationTokenSource hostLifetimeCts);
    bool IsHostHeartbeatPast(string connectionId, TimeSpan timeout);
    bool TryTimeoutHost(string connectionId, TimeSpan timeout);
    void ReleaseHost(string connectionId);
    bool IsDisconnectedPast(TimeSpan timeout);
    void SetHostReady(bool ready);
    void SetSessionStartedAt(DateTimeOffset startedAt);
    void SetSessionIncarnationId(Guid incarnationId);
    void RecordHostHeartbeat();
    bool HasPendingHostFences { get; }
    bool TryQueuePendingHostFence(PendingHostFence fence);
    PendingHostFence? PeekPendingHostFence();
    bool RemovePendingHostFence(PendingHostFence fence);
}
