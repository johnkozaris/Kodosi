using System.Collections;
using System.Reflection;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Realtime;
using Microsoft.Extensions.DependencyInjection;

namespace Kodosi.HostTests;

internal sealed class UnsupportedServiceScopeFactory : IServiceScopeFactory
{
    public static UnsupportedServiceScopeFactory Instance { get; } = new();

    public IServiceScope CreateScope() =>
        throw new InvalidOperationException("This test fixture does not support scoped protocol work.");
}

internal sealed class UnsupportedPermissionDecisionAuditStore : IPermissionDecisionAuditStore
{
    public static UnsupportedPermissionDecisionAuditStore Instance { get; } = new();

    public Task<PermissionDecisionAdmissionResult> AdmitAsync(
        SessionId sessionId,
        UserId requesterUserId,
        string actionId,
        string auditPayload,
        PermissionDecisionPendingTuple pendingTuple,
        CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture does not support permission decisions.");

    public Task<bool> MarkDispatchFailedAsync(
        SessionId sessionId,
        UserId requesterUserId,
        string actionId,
        PermissionDecisionPendingTuple pendingTuple,
        CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture does not support permission decisions.");

    public Task<HostActionCompletionResult> CompleteHostActionAsync(
        SessionId sessionId,
        UserId assertedRequesterUserId,
        string assertedActionId,
        PermissionDecisionPendingTuple assertedTuple,
        InputAuditStatus terminalStatus,
        CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture does not support permission decisions.");
}

internal sealed class EmptySemanticRelayRepository : ISemanticRelayRepository
{
    public static EmptySemanticRelayRepository Instance { get; } = new();

    public Task<SemanticRequestClaim> ClaimRequestAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        Guid requestId,
        string mode,
        string payloadSha256,
        CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture supports semantic receipt reads only.");

    public Task<SemanticRequestClaim?> FindExactRequestAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        Guid requestId,
        string mode,
        string payloadSha256,
        CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture supports semantic receipt reads only.");

    public Task MarkDispatchedAsync(Guid requestRowId, CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture supports semantic receipt reads only.");

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
        CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture supports semantic receipt reads only.");

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
        CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture supports semantic receipt reads only.");

    public Task<int> DeleteAcknowledgedBeforeAsync(
        DateTimeOffset cutoff,
        int limit,
        CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture supports semantic receipt reads only.");
}

internal sealed class UnsupportedSemanticRelayRepository : ISemanticRelayRepository
{
    public static UnsupportedSemanticRelayRepository Instance { get; } = new();

    public Task<SemanticRequestClaim> ClaimRequestAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        Guid requestId,
        string mode,
        string payloadSha256,
        CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture does not support semantic relay.");

    public Task<SemanticRequestClaim?> FindExactRequestAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        Guid requestId,
        string mode,
        string payloadSha256,
        CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture does not support semantic relay.");

    public Task MarkDispatchedAsync(Guid requestRowId, CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture does not support semantic relay.");

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
        CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture does not support semantic relay.");

    public Task<IReadOnlyList<SemanticRelayReceipt>> ListPendingReceiptsForDeviceAsync(
        UserId requesterUserId,
        string requesterDeviceId,
        int limit,
        SemanticReceiptCursor? cursor = null,
        CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture does not support semantic relay.");

    public Task<IReadOnlyList<SemanticRelayReceipt>> ListPendingReceiptsAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        int limit,
        CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture does not support semantic relay.");

    public Task<bool> AcknowledgeReceiptAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        Guid requestId,
        CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture does not support semantic relay.");

    public Task<int> DeleteAcknowledgedBeforeAsync(
        DateTimeOffset cutoff,
        int limit,
        CancellationToken ct = default) =>
        throw new InvalidOperationException("This test fixture does not support semantic relay.");
}

internal static class SessionLifecycleGateTestExtensions
{
    private static readonly FieldInfo EntriesField = typeof(SessionLifecycleGate)
        .GetField("_entries", BindingFlags.Instance | BindingFlags.NonPublic)
        ?? throw new InvalidOperationException(
            "Session lifecycle gate entries field should exist for test synchronization.");

    public static int TestActiveSessionCount(this SessionLifecycleGate gate) =>
        ((ICollection)EntriesField.GetValue(gate)!).Count;

    public static int TestReferenceCount(
        this SessionLifecycleGate gate,
        SessionId sessionId)
    {
        var entries = EntriesField.GetValue(gate)
            ?? throw new InvalidOperationException(
                "Session lifecycle gate entries should be initialized.");
        var tryGetValue = entries.GetType().GetMethod("TryGetValue")
            ?? throw new InvalidOperationException(
                "Session lifecycle gate dictionary should expose TryGetValue.");
        var arguments = new object?[] { sessionId, null };
        if (!(bool)tryGetValue.Invoke(entries, arguments)!)
        {
            return 0;
        }

        var entry = arguments[1]
            ?? throw new InvalidOperationException(
                "Session lifecycle gate entry should be returned.");
        var entryType = entry.GetType();
        var stateLock = (Lock)(entryType.GetProperty(
            "StateLock",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)
            ?.GetValue(entry)
            ?? throw new InvalidOperationException(
                "Session lifecycle gate entry lock should exist."));
        lock (stateLock)
        {
            return (int)(entryType.GetField(
                    "ReferenceCount",
                    BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)
                ?.GetValue(entry)
                ?? throw new InvalidOperationException(
                    "Session lifecycle gate reference count should exist."));
        }
    }
}

internal static class RealtimeTestExtensions
{
    public static LiveSessionPorts CreateRuntime(
        this ILiveSessionStateDirectory directory,
        SessionId sessionId) =>
        directory.CreateRuntime(sessionId, out _);

    public static LiveSessionPorts CreateRuntime(
        this ILiveSessionStateDirectory directory,
        SessionId sessionId,
        out LiveSessionCreationOwnership? creationOwnership)
    {
        var connectionId = $"test-runtime-bootstrap-{Guid.NewGuid():N}";
        using var hostLifetime = new CancellationTokenSource();
        if (!directory.TryClaimHost(
                sessionId,
                connectionId,
                hostLifetime,
                out var ports,
                out creationOwnership)
            || ports is null)
        {
            throw new InvalidOperationException(
                "Could not initialize test runtime through a host claim.");
        }

        ports.Host.ReleaseHost(connectionId);
        return ports;
    }

    public static ChannelByteSendQueue SetHostQueue(
        this SessionSendQueues queues,
        string connectionId = "test-host")
    {
        var queue = queues.TrySetHostQueue(connectionId, [])
            ?? throw new InvalidOperationException("Could not initialize test host queue.");
        _ = queue.ReadAsync(CancellationToken.None).GetAwaiter().GetResult()
            ?? throw new InvalidOperationException("Test host queue did not retain its seed.");
        return queue;
    }

    public static RelayClientSendQueue AddParticipantQueue(
        this SessionSendQueues queues,
        string connectionId,
        AccessLevel accessLevel = AccessLevel.View) =>
        queues.AddPreparedParticipantQueue(connectionId, accessLevel, static _ => { });

    public static StreamDemandTransition AddOwnerParticipant(
        this ILiveParticipantRoster roster,
        string connectionId)
    {
        if (!roster.TryAddOwnerParticipant(
                connectionId,
                int.MaxValue,
                out var transition))
        {
            throw new InvalidOperationException("Could not initialize test owner participant.");
        }
        return transition;
    }

    public static async Task<WireMessage?> ReadAsync(
        this RelayClientSendQueue queue,
        CancellationToken ct)
    {
        var delivery = await queue.ReadForSendAsync(ct);
        return delivery?.Message;
    }

    public static ChannelByteSendQueue Register(
        this UserEventBroadcaster broadcaster,
        string connectionId,
        UserId userId,
        string deviceId)
    {
        var registration = broadcaster.RegisterPrimed(
            connectionId,
            userId,
            deviceId);
        var outcome = registration.CompletePriming();
        if (outcome != ChannelByteSendQueueWriteOutcome.Enqueued)
        {
            throw new InvalidOperationException(
                $"Could not complete test user-event registration: {outcome}.");
        }
        return registration.Queue;
    }
}
