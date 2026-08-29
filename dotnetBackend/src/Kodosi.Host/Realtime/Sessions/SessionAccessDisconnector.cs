using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;

namespace Kodosi.Host.Realtime;

internal sealed class SessionAccessDisconnector(
    SessionReader sessionReader,
    ISessionRepository sessions,
    IConnectionRegistry connectionRegistry,
    ILiveSessionStateDirectory runtimes,
    SessionBroadcaster broadcaster,
    SessionLifecycleGate lifecycleGate,
    OperationalMetrics metrics,
    ILogger<SessionAccessDisconnector> logger)
{
    private readonly SessionReader _sessionReader = sessionReader;
    private readonly ISessionRepository _sessions = sessions;
    private readonly IConnectionRegistry _connectionRegistry = connectionRegistry;
    private readonly ILiveSessionStateDirectory _runtimes = runtimes;
    private readonly SessionBroadcaster _broadcaster = broadcaster;
    private readonly SessionLifecycleGate _lifecycleGate = lifecycleGate;
    private readonly OperationalMetrics _metrics = metrics;
    private readonly ILogger<SessionAccessDisconnector> _logger = logger;



    public async Task DisconnectAfterScopeChangeAsync(
        SessionId sessionId,
        UserId ownerId,
        DateTimeOffset expectedStartedAt,
        CancellationToken ct = default,
        bool lifecycleAlreadyHeld = false)
    {
        var noCancellation = CancellationToken.None;
        IAsyncDisposable? lifecycle = null;
        if (!lifecycleAlreadyHeld)
        {
            lifecycle = await _lifecycleGate.AcquireAsync(
                sessionId,
                noCancellation);
        }
        try
        {
            var current = await _sessions.GetByIdAsync(
                sessionId,
                noCancellation);
            var runtime = _runtimes.TryGet(sessionId);
            var queues = _broadcaster.TryGetSession(sessionId);
            if (current is null
                || current.StartedAt != expectedStartedAt
                || current.Status == SessionStatus.Ended
                || runtime is null
                || runtime.Host.SessionIncarnationId
                    != current.IncarnationId
                || runtime.Host.SessionStartedAt != expectedStartedAt
                || queues is null)
            {
                return;
            }
            var activeParticipants = _connectionRegistry.GetActiveSharedParticipants(sessionId);
            if (activeParticipants.Count == 0)
            {
                return;
            }

            var deniedViewerIds = await _sessionReader.GetViewerIdsWithoutAccessAsync(
                sessionId,
                ownerId,
                activeParticipants
                    .Select(participant => participant.UserId)
                    .Distinct()
                    .ToList(),
                noCancellation);

            var participantQueues = activeParticipants
                .Select(participant => (
                    participant.ConnectionId,
                    participant.UserId,
                    Queue: queues.GetParticipantQueue(participant.ConnectionId)))
                .Where(static participant => participant.Queue is not null)
                .Select(static participant => (
                    participant.ConnectionId,
                    participant.UserId,
                    Queue: participant.Queue!))
                .ToList();

            foreach (var participant in participantQueues)
            {
                if (deniedViewerIds.Contains(participant.UserId))
                {
                    _metrics.RecordAccessCascadeDisconnect();
                    _ = _broadcaster.DisconnectParticipantIfSame(
                        sessionId,
                        participant.ConnectionId,
                        participant.Queue);
                    continue;
                }

                _ = _broadcaster.DisconnectParticipantForRefreshIfSame(
                    sessionId,
                    participant.ConnectionId,
                    participant.Queue);
            }

            foreach (var deniedViewerId in deniedViewerIds)
            {
                _broadcaster.NotifyHostAccessRevoked(sessionId, deniedViewerId);
            }
        }
        catch (Exception ex)
        {
            _logger.LogError(
                ex,
                "Failed to revalidate scope change for session {SessionId}; disconnecting all shared participants fail-closed",
                sessionId);
            FailClosedSharedParticipants(
                sessionId,
                null,
                expectedStartedAt);
        }
        finally
        {
            if (lifecycle is not null)
            {
                await lifecycle.DisposeAsync();
            }
        }
    }

    public async Task DisconnectRemovedRoomMemberAsync(
        UserId removedUserId,
        IReadOnlyList<RoomLiveSession> sessions,
        CancellationToken ct = default,
        bool lifecycleAlreadyHeld = false)
    {
        foreach (var session in sessions)
        {
            if (session.EndedByRemoval)
            {
                continue;
            }

            try
            {
                IAsyncDisposable? lifecycle = null;
                if (!lifecycleAlreadyHeld)
                {
                    lifecycle = await _lifecycleGate.AcquireAsync(
                        session.SessionId,
                        ct);
                }
                await using var lifecycleLease = lifecycle;
                var current = await _sessions.GetByIdAsync(session.SessionId, ct);
                var runtime = _runtimes.TryGet(session.SessionId);
                if (current is null
                    || current.StartedAt != session.StartedAt
                    || (session.IncarnationId != Guid.Empty
                        && current.IncarnationId != session.IncarnationId)
                    || current.Status == SessionStatus.Ended
                    || runtime is null
                    || (session.IncarnationId != Guid.Empty
                        && runtime.Host.SessionIncarnationId != Guid.Empty
                        && runtime.Host.SessionIncarnationId
                            != session.IncarnationId)
                    || runtime.Host.SessionStartedAt != session.StartedAt)
                {
                    continue;
                }

                await DisconnectDeniedViewerCurrentAsync(
                    session.SessionId,
                    session.OwnerId,
                    removedUserId,
                    notifyHost: true,
                    ct);



                var participants =
                    _connectionRegistry.GetActiveSharedParticipants(
                        session.SessionId);
                foreach (var (connectionId, userId) in participants)
                {
                    if (userId == removedUserId)
                    {
                        continue;
                    }
                    var expectedQueue = _broadcaster.TryGetParticipantQueue(
                        session.SessionId,
                        connectionId);
                    if (expectedQueue is not null)
                    {
                        _ = _broadcaster.DisconnectParticipantForRefreshIfSame(
                            session.SessionId,
                            connectionId,
                            expectedQueue);
                    }
                }
                _broadcaster.ForceDisconnectHost(
                    session.SessionId,
                    CloseReason.AccessRevoked);
            }
            catch (OperationCanceledException)
            {
                throw;
            }
            catch (Exception ex)
            {
                _logger.LogError(
                    ex,
                    "Room-member revocation fanout failed for session {SessionId}; disconnecting that incarnation fail-closed",
                    session.SessionId);
                FailClosedSession(session);
            }
        }
    }

    public async Task DisconnectFormerFriendAsync(
        UserId ownerId,
        UserId formerFriendId,
        IReadOnlyList<SessionAccessFanoutTarget> sessions,
        CancellationToken ct = default,
        bool lifecycleAlreadyHeld = false)
    {
        foreach (var session in sessions)
        {
            await TryDisconnectDeniedViewerAsync(
                session.SessionId,
                ownerId,
                formerFriendId,
                session.StartedAt,
                notifyHost: true,
                lifecycleAlreadyHeld,
                expectedIncarnationId: session.IncarnationId,
                ct: ct);
        }
    }

    public Task DisconnectExpiredOverrideAsync(
        SessionAccessFanoutTarget session,
        UserId viewerId,
        CancellationToken ct = default,
        bool lifecycleAlreadyHeld = false) =>
        TryDisconnectDeniedViewerAsync(
            session.SessionId,
            ownerId: null,
            viewerId,
            session.StartedAt,
            notifyHost: true,
            lifecycleAlreadyHeld,
            expectedIncarnationId: session.IncarnationId,
            ct: ct);

    public Task DisconnectDismissedViewerAsync(
        SessionId sessionId,
        Guid expectedIncarnationId,
        UserId viewerId,
        DateTimeOffset expectedStartedAt,
        CancellationToken ct = default) =>
        TryDisconnectDeniedViewerAsync(
            sessionId,
            ownerId: null,
            viewerId,
            expectedStartedAt,
            notifyHost: true,
            expectedIncarnationId: expectedIncarnationId,
            ct: ct);

    public async Task RefreshGrantedAccessAsync(
        SharingMutationResult mutation,
        CancellationToken ct,
        bool lifecycleAlreadyHeld = false)
    {
        IAsyncDisposable? lifecycle = null;
        if (!lifecycleAlreadyHeld)
        {
            lifecycle = await _lifecycleGate.AcquireAsync(
                mutation.SessionId,
                ct);
        }
        try
        {
            Session? current;
            try
            {
                current = await GetCurrentIncarnationAsync(
                    mutation.SessionId,
                    mutation.IncarnationId,
                    mutation.StartedAt,
                    ct);
            }
            catch (Exception ex) when (ex is not OperationCanceledException)
            {
                _logger.LogError(
                    ex,
                    "Failed to revalidate granted access for viewer {ViewerId} on session {SessionId}; disconnecting all shared participants fail-closed",
                    mutation.ActorUserId,
                    mutation.SessionId);
                FailClosedSharedParticipants(
                    mutation.SessionId,
                    mutation.IncarnationId,
                    mutation.StartedAt);
                return;
            }
            if (current is null)
            {
                return;
            }
            IReadOnlySet<UserId> denied;
            try
            {
                denied = await _sessionReader.GetViewerIdsWithoutAccessAsync(
                    mutation.SessionId,
                    mutation.OwnerUserId,
                    [mutation.ActorUserId],
                    ct);
            }
            catch (Exception ex) when (ex is not OperationCanceledException)
            {
                _logger.LogError(
                    ex,
                    "Failed to revalidate granted access for viewer {ViewerId} on session {SessionId}; disconnecting all shared participants fail-closed",
                    mutation.ActorUserId,
                    mutation.SessionId);
                FailClosedSharedParticipants(
                    mutation.SessionId,
                    mutation.IncarnationId,
                    mutation.StartedAt);
                return;
            }
            if (denied.Contains(mutation.ActorUserId))
            {
                return;
            }

            var connectionIds =
                _connectionRegistry.GetSharedParticipantConnectionIds(
                    mutation.SessionId,
                    mutation.ActorUserId);
            foreach (var connectionId in connectionIds)
            {
                var expectedQueue = _broadcaster.TryGetParticipantQueue(
                    mutation.SessionId,
                    connectionId);
                if (expectedQueue is not null)
                {
                    _ = _broadcaster.DisconnectParticipantForRefreshIfSame(
                        mutation.SessionId,
                        connectionId,
                        expectedQueue);
                }
            }
        }
        finally
        {
            if (lifecycle is not null)
            {
                await lifecycle.DisposeAsync();
            }
        }
    }

    public Task DisconnectRevokedAccessAsync(
        SharingMutationResult mutation,
        CancellationToken ct,
        bool lifecycleAlreadyHeld = false) =>
        TryDisconnectDeniedViewerAsync(
            mutation.SessionId,
            mutation.OwnerUserId,
            mutation.ActorUserId,
            mutation.StartedAt,
            notifyHost: true,
            lifecycleAlreadyHeld,
            expectedIncarnationId: mutation.IncarnationId,
            ct);

    private async Task TryDisconnectDeniedViewerAsync(
        SessionId sessionId,
        UserId? ownerId,
        UserId viewerId,
        DateTimeOffset expectedStartedAt,
        bool notifyHost,
        bool lifecycleAlreadyHeld = false,
        Guid? expectedIncarnationId = null,
        CancellationToken ct = default)
    {
        IAsyncDisposable? lifecycle = null;
        if (!lifecycleAlreadyHeld)
        {
            lifecycle = await _lifecycleGate.AcquireAsync(sessionId, ct);
        }
        try
        {
            var current = await GetCurrentIncarnationAsync(
                sessionId,
                expectedIncarnationId,
                expectedStartedAt,
                ct);
            if (current is null)
            {
                return;
            }
            await DisconnectDeniedViewerCurrentAsync(
                sessionId,
                ownerId ?? current.OwnerUserId,
                viewerId,
                notifyHost,
                ct);
        }
        catch (OperationCanceledException)
        {
            throw;
        }
        catch (Exception ex)
        {
            _logger.LogError(
                ex,
                "Failed to revalidate denied viewer {ViewerId} on session {SessionId}; disconnecting all shared participants fail-closed",
                viewerId,
                sessionId);
            FailClosedSharedParticipants(
                sessionId,
                expectedIncarnationId,
                expectedStartedAt);
            if (notifyHost)
            {
                _broadcaster.NotifyHostAccessRevoked(sessionId, viewerId);
            }
        }
        finally
        {
            if (lifecycle is not null)
            {
                await lifecycle.DisposeAsync();
            }
        }
    }

    private async Task DisconnectDeniedViewerCurrentAsync(
        SessionId sessionId,
        UserId ownerId,
        UserId viewerId,
        bool notifyHost,
        CancellationToken ct)
    {
        var effectiveAccess = await _sessionReader.ResolveViewerAccessAsync(
            sessionId,
            ownerId,
            viewerId,
            ct);

        var connectionIds =
            _connectionRegistry.GetSharedParticipantConnectionIds(sessionId, viewerId);
        var participantQueues = connectionIds
            .Select(connectionId => (
                ConnectionId: connectionId,
                Queue: _broadcaster.TryGetParticipantQueue(sessionId, connectionId)))
            .Where(static participant => participant.Queue is not null)
            .Select(static participant => (
                participant.ConnectionId,
                Queue: participant.Queue!))
            .ToList();
        foreach (var participant in participantQueues)
        {
            if (effectiveAccess is null)
            {
                _metrics.RecordAccessCascadeDisconnect();
                _ = _broadcaster.DisconnectParticipantIfSame(
                    sessionId,
                    participant.ConnectionId,
                    participant.Queue);
            }
            else if (participant.Queue.AccessLevel != effectiveAccess.Value)
            {
                _ = _broadcaster.DisconnectParticipantForRefreshIfSame(
                    sessionId,
                    participant.ConnectionId,
                    participant.Queue);
            }
        }

        if (notifyHost)
        {
            _broadcaster.NotifyHostAccessRevoked(sessionId, viewerId);
        }
    }

    private async Task<Session?> GetCurrentIncarnationAsync(
        SessionId sessionId,
        Guid? expectedIncarnationId,
        DateTimeOffset expectedStartedAt,
        CancellationToken ct)
    {
        var session = await _sessions.GetByIdAsync(sessionId, ct);
        var runtime = _runtimes.TryGet(sessionId);
        return session is not null
            && (expectedIncarnationId is null
                || session.IncarnationId == expectedIncarnationId)
            && session.StartedAt == expectedStartedAt
            && session.Status != SessionStatus.Ended
            && runtime is not null
            && (expectedIncarnationId is null
                || runtime.Host.SessionIncarnationId
                    == expectedIncarnationId.Value)
            && runtime.Host.SessionStartedAt == expectedStartedAt
            && _broadcaster.TryGetSession(sessionId) is not null
                ? session
                : null;
    }

    private void FailClosedSharedParticipants(
        SessionId sessionId,
        Guid? expectedIncarnationId,
        DateTimeOffset expectedStartedAt)
    {
        var runtime = _runtimes.TryGet(sessionId);
        if (runtime is null
            || (expectedIncarnationId is not null
                && runtime.Host.SessionIncarnationId
                    != expectedIncarnationId.Value)
            || runtime.Host.SessionStartedAt != expectedStartedAt
            || _broadcaster.TryGetSession(sessionId) is null)
        {
            return;
        }

        var participants =
            _connectionRegistry.GetActiveSharedParticipants(sessionId);
        var participantQueues = participants
            .Select(participant => (
                participant.ConnectionId,
                Queue: _broadcaster.TryGetParticipantQueue(
                    sessionId,
                    participant.ConnectionId)))
            .Where(static participant => participant.Queue is not null)
            .Select(static participant => (
                participant.ConnectionId,
                Queue: participant.Queue!))
            .ToList();
        foreach (var participant in participantQueues)
        {
            _metrics.RecordAccessCascadeDisconnect();
            _ = _broadcaster.DisconnectParticipantForRefreshIfSame(
                sessionId,
                participant.ConnectionId,
                participant.Queue);
        }
    }

    private void FailClosedSession(RoomLiveSession session)
    {
        var runtime = _runtimes.TryGet(session.SessionId);
        var queues = _broadcaster.TryGetSession(session.SessionId);
        if (runtime is null
            || queues is null
            || session.IncarnationId == Guid.Empty
            || runtime.Host.SessionIncarnationId != session.IncarnationId
            || runtime.Host.SessionStartedAt != session.StartedAt)
        {
            return;
        }

        _broadcaster.ForceDisconnectHost(
            session.SessionId,
            CloseReason.AccessRevoked);
        foreach (var (_, participantQueue) in
            queues.GetParticipantQueueSnapshot())
        {
            participantQueue.Complete(
                QueueCompletionCause.AccessRefresh,
                discardPending: true,
                closeReason: CloseReason.AccessRevoked);
        }
    }
}
