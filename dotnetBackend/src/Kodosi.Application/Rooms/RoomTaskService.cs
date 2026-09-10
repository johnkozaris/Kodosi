using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class RoomTaskService(
    IRoomRepository rooms,
    IRoomMemberRepository roomMembers,
    IRoomTaskRepository tasks,
    ISessionRepository sessions,
    IRoomMutationReceiptRepository mutationReceipts,
    IRoomLifecycleLock roomLifecycleLock,
    IUnitOfWork unitOfWork,
    TimeProvider? timeProvider = null)
{
    private const int MaxPageSize = 500;
    private readonly IRoomRepository _rooms = rooms;
    private readonly IRoomMemberRepository _roomMembers = roomMembers;
    private readonly IRoomTaskRepository _tasks = tasks;
    private readonly ISessionRepository _sessions = sessions;
    private readonly IRoomMutationReceiptRepository _mutationReceipts = mutationReceipts;
    private readonly IRoomLifecycleLock _roomLifecycleLock = roomLifecycleLock;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;

    public async Task<RoomTask> CreateAsync(
        RoomId roomId,
        Guid taskId,
        UserId requesterUserId,
        string title,
        string? description,
        Guid? assignedSessionId,
        Guid? assignedSessionIncarnationId,
        DateTimeOffset? dueAt,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _roomLifecycleLock.AcquireAsync(roomId, ct);
        var task = await CreateCoreAsync(
            roomId,
            taskId,
            requesterUserId,
            title,
            description,
            assignedSessionId,
            assignedSessionIncarnationId,
            dueAt,
            ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return task;
    }

    private async Task<RoomTask> CreateCoreAsync(
        RoomId roomId,
        Guid taskId,
        UserId requesterUserId,
        string title,
        string? description,
        Guid? assignedSessionId,
        Guid? assignedSessionIncarnationId,
        DateTimeOffset? dueAt,
        CancellationToken ct)
    {
        var room = await EnsureMemberAsync(roomId, requesterUserId, ct);
        await EnsureAssignableRoomSessionAsync(
            roomId,
            assignedSessionId,
            assignedSessionIncarnationId,
            ct);
        var now = _timeProvider.GetUtcNow();
        var task = RoomTask.Create(
            taskId,
            roomId,
            requesterUserId,
            title,
            description,
            assignedSessionId,
            dueAt,
            now);
        if (assignedSessionId is not null)
        {
            task.AssignInitialSession(
                assignedSessionId.Value,
                assignedSessionIncarnationId!.Value);
        }
        var creation = await _tasks.AddIdempotentAsync(task, ct);
        if (creation.Created)
        {
            room.AdvanceTaskRevision();
        }
        return creation.Task;
    }

    public async Task<RoomTaskMutationResult> TransitionIdempotentlyAsync(
        Guid requestId,
        RoomId roomId,
        Guid taskId,
        long expectedTaskRevision,
        UserId requesterUserId,
        Guid? requesterSessionId,
        Guid? requesterSessionIncarnationId,
        RoomTaskStatus to,
        string? result,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _mutationReceipts.AcquireAsync(
            requesterUserId,
            RoomMutationOperation.TransitionTask,
            requestId,
            ct);
        await _roomLifecycleLock.AcquireAsync(roomId, ct);
        var fingerprint = RoomMutationTargetFingerprint.TransitionTask(
            roomId,
            taskId,
            expectedTaskRevision,
            to,
            requesterSessionId,
            requesterSessionIncarnationId,
            result);
        var existingReceipt = await _mutationReceipts.GetAsync(
            requesterUserId,
            RoomMutationOperation.TransitionTask,
            requestId,
            ct);
        if (existingReceipt is not null)
        {
            RoomMutationReceiptPolicy.EnsureOperation(
                existingReceipt,
                RoomMutationOperation.TransitionTask);
            var duplicate = RoomMutationReceiptPolicy.ResolveDuplicate(
                existingReceipt,
                fingerprint);
            await transaction.CommitAsync(ct);
            return new RoomTaskMutationResult(duplicate, null);
        }

        var task = await _tasks.GetByIdForUpdateAsync(taskId, ct)
            ?? throw new NotFoundException("RoomTask", taskId);
        EnsureTaskTarget(task, roomId, expectedTaskRevision);
        await EnsureSessionIncarnationAsync(
            requesterSessionId,
            requesterSessionIncarnationId,
            ct);
        if (requesterSessionId is not null
            && (task.AssignedSessionId != requesterSessionId
                || task.AssignedSessionIncarnationId != requesterSessionIncarnationId))
        {
            throw new PolicyViolationException(
                "Actor session incarnation does not match the task assignment.");
        }
        var now = _timeProvider.GetUtcNow();
        await ApplyAuthorizedTransitionAsync(
            task,
            requesterUserId,
            requesterSessionId,
            to,
            result,
            now,
            ct);
        var receipt = RoomMutationReceiptPolicy.Create(
            requesterUserId,
            RoomMutationOperation.TransitionTask,
            requestId,
            fingerprint,
            roomId,
            task.Id,
            task.Status.ToString(),
            task.Revision,
            task.AssignedSessionId,
            task.AssignedSessionIncarnationId,
            now);
        await _mutationReceipts.AddAsync(receipt, ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return new RoomTaskMutationResult(ToResult(receipt), task);
    }

    private async Task ApplyAuthorizedTransitionAsync(
        RoomTask task,
        UserId requesterUserId,
        Guid? requesterSessionId,
        RoomTaskStatus to,
        string? result,
        DateTimeOffset now,
        CancellationToken ct)
    {
        var room = await _rooms.GetByIdAsync(task.RoomId, ct)
            ?? throw new NotFoundException(nameof(Room), task.RoomId);
        if (!await _roomMembers.IsMemberAsync(task.RoomId, requesterUserId, ct)
            && !room.IsOwner(requesterUserId))
        {
            throw new PolicyViolationException("Only room members may manage tasks.");
        }



        var isAgentOnAssignedSession = requesterSessionId.HasValue
            && task.AssignedSessionId == requesterSessionId.Value
            && await IsOwnedRoomSessionAsync(
                task.RoomId,
                requesterSessionId.Value,
                requesterUserId,
                ct);
        var isOwner = room.IsOwner(requesterUserId);

        if (isAgentOnAssignedSession)
        {
            switch (to)
            {
                case RoomTaskStatus.InProgress: task.Claim(now); break;
                case RoomTaskStatus.Review: task.Submit(now); break;
                case RoomTaskStatus.Done: task.Complete(result, requesterUserId, now); break;
                case RoomTaskStatus.Archived: task.Archive(now); break;
                default:
                    throw new DomainException(
                        $"Agent cannot transition assigned task to {to}.");
            }
        }
        else if (isOwner)
        {
            task.ApplyOwnerOverride(to, result, requesterUserId, now);
        }
        else
        {
            throw new PolicyViolationException(
                "Only the assigned agent session or the room owner may transition this task.");
        }
        room.AdvanceTaskRevision();
    }

    public async Task<RoomTaskMutationResult> AssignIdempotentlyAsync(
        Guid requestId,
        RoomId roomId,
        Guid taskId,
        long expectedTaskRevision,
        UserId requesterUserId,
        Guid? sessionId,
        Guid? sessionIncarnationId,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _mutationReceipts.AcquireAsync(
            requesterUserId,
            RoomMutationOperation.AssignTask,
            requestId,
            ct);
        await _roomLifecycleLock.AcquireAsync(roomId, ct);
        var fingerprint = RoomMutationTargetFingerprint.AssignTask(
            roomId,
            taskId,
            expectedTaskRevision,
            sessionId,
            sessionIncarnationId);
        var existingReceipt = await _mutationReceipts.GetAsync(
            requesterUserId,
            RoomMutationOperation.AssignTask,
            requestId,
            ct);
        if (existingReceipt is not null)
        {
            RoomMutationReceiptPolicy.EnsureOperation(
                existingReceipt,
                RoomMutationOperation.AssignTask);
            var duplicate = RoomMutationReceiptPolicy.ResolveDuplicate(
                existingReceipt,
                fingerprint);
            await transaction.CommitAsync(ct);
            return new RoomTaskMutationResult(duplicate, null);
        }

        var task = await _tasks.GetByIdForUpdateAsync(taskId, ct)
            ?? throw new NotFoundException("RoomTask", taskId);
        EnsureTaskTarget(task, roomId, expectedTaskRevision);
        await EnsureAssignableRoomSessionAsync(
            roomId,
            sessionId,
            sessionIncarnationId,
            ct);
        var room = await EnsureMemberAsync(task.RoomId, requesterUserId, ct);
        var now = _timeProvider.GetUtcNow();
        task.Assign(sessionId, sessionIncarnationId, now);
        room.AdvanceTaskRevision();
        var receipt = RoomMutationReceiptPolicy.Create(
            requesterUserId,
            RoomMutationOperation.AssignTask,
            requestId,
            fingerprint,
            roomId,
            task.Id,
            "Assigned",
            task.Revision,
            sessionId,
            sessionIncarnationId,
            now);
        await _mutationReceipts.AddAsync(receipt, ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return new RoomTaskMutationResult(ToResult(receipt), task);
    }

    public async Task<RoomTask?> GetByIdAsync(
        RoomId roomId,
        Guid taskId,
        UserId requesterUserId,
        CancellationToken ct = default)
    {
        await EnsureMemberAsync(roomId, requesterUserId, ct);
        var task = await _tasks.GetByIdAsync(taskId, ct);
        return task?.RoomId == roomId ? task : null;
    }

    public async Task<RoomTaskReadPage> ListAsync(
        RoomId roomId,
        UserId requesterUserId,
        RoomTaskStatus? statusFilter,
        Guid? assigneeFilter,
        int offset,
        int limit,
        CancellationToken ct = default,
        string? snapshot = null)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _roomLifecycleLock.AcquireAsync(roomId, ct);
        var room = await EnsureMemberAsync(roomId, requesterUserId, ct);
        var currentSnapshot = TaskSnapshot(room, requesterUserId, statusFilter, assigneeFilter);
        if (snapshot is not null && !string.Equals(snapshot, currentSnapshot, StringComparison.Ordinal))
        {
            throw new ConcurrentModificationException();
        }
        var normalizedOffset = Math.Max(0, offset);
        var normalizedLimit = Math.Clamp(limit, 1, MaxPageSize);
        var candidates = await _tasks.GetByRoomPageAsync(
            roomId,
            statusFilter,
            assigneeFilter,
            normalizedOffset,
            normalizedLimit + 1,
            ct);
        var hasMore = candidates.Count > normalizedLimit;
        var items = hasMore ? candidates.Take(normalizedLimit).ToList() : candidates;
        await transaction.CommitAsync(ct);
        return new RoomTaskReadPage(
            items,
            hasMore,
            hasMore ? normalizedOffset + items.Count : null,
            currentSnapshot);
    }

    private static string TaskSnapshot(
        Room room,
        UserId requesterUserId,
        RoomTaskStatus? statusFilter,
        Guid? assigneeFilter)
    {
        var identity = FormattableString.Invariant(
            $"kodosi:room-task-snapshot:v1\n{room.Id.Value:D}\n{requesterUserId.Value:D}\n{room.TaskRevision}\n{statusFilter}\n{assigneeFilter:D}");
        return Convert.ToHexStringLower(System.Security.Cryptography.SHA256.HashData(
            System.Text.Encoding.UTF8.GetBytes(identity)));
    }

    private static void EnsureTaskTarget(
        RoomTask task,
        RoomId roomId,
        long expectedTaskRevision)
    {
        if (task.RoomId != roomId)
        {
            throw new NotFoundException("RoomTask", task.Id);
        }
        if (expectedTaskRevision < 0 || task.Revision != expectedTaskRevision)
        {
            throw new ConcurrentModificationException();
        }
    }

    private async Task EnsureAssignableRoomSessionAsync(
        RoomId roomId,
        Guid? sessionId,
        Guid? expectedIncarnationId,
        CancellationToken ct)
    {
        if (sessionId.HasValue != expectedIncarnationId.HasValue)
        {
            throw new InvalidParameterException(
                "sessionIncarnationId",
                "must be supplied exactly when a session ID is supplied");
        }
        if (sessionId is not { } id)
        {
            return;
        }
        var session = await _sessions.GetByIdAsync(SessionId.From(id), ct);
        if (session is null
            || session.IncarnationId != expectedIncarnationId
            || session.Status is not (SessionStatus.Live or SessionStatus.Reconnecting)
            || session.Scope != SessionScope.Room
            || session.RoomId != roomId)
        {
            throw new PolicyViolationException(
                "Assigned session must be the requested active room-session incarnation.");
        }
    }

    private async Task EnsureSessionIncarnationAsync(
        Guid? sessionId,
        Guid? expectedIncarnationId,
        CancellationToken ct)
    {
        if (sessionId.HasValue != expectedIncarnationId.HasValue)
        {
            throw new InvalidParameterException(
                "sessionIncarnationId",
                "must be supplied exactly when a session ID is supplied");
        }
        if (sessionId is not { } id)
        {
            return;
        }
        var session = await _sessions.GetByIdAsync(SessionId.From(id), ct);
        if (session is null || session.IncarnationId != expectedIncarnationId)
        {
            throw new NotFoundException(nameof(Session), id);
        }
    }

    private static RoomMutationReceiptResult ToResult(RoomMutationReceipt receipt) =>
        new(
            false,
            receipt.RoomId,
            receipt.EntityId,
            receipt.Result,
            receipt.Revision,
            receipt.AssigneeSessionId,
            receipt.AssigneeSessionIncarnationId);

    private async Task<Room> EnsureMemberAsync(RoomId roomId, UserId userId, CancellationToken ct)
    {
        var room = await _rooms.GetByIdAsync(roomId, ct)
            ?? throw new NotFoundException(nameof(Room), roomId);
        if (!await _roomMembers.IsMemberAsync(roomId, userId, ct)
            && !room.IsOwner(userId))
        {
            throw new PolicyViolationException("Only room members may manage tasks.");
        }
        return room;
    }

    private async Task<bool> IsOwnedRoomSessionAsync(
            RoomId roomId,
            Guid sessionId,
            UserId? ownerUserId,
            CancellationToken ct)
    {
        var session = await _sessions.GetByIdAsync(SessionId.From(sessionId), ct);
        return session is not null
            && session.Status is SessionStatus.Live or SessionStatus.Reconnecting
            && session.Scope == SessionScope.Room
            && session.RoomId == roomId
            && (ownerUserId is null || session.IsOwner(ownerUserId.Value));
    }
}

public sealed record RoomTaskReadPage(
    IReadOnlyList<RoomTask> Items,
    bool HasMore,
    int? NextOffset,
    string Snapshot);

public sealed record RoomTaskMutationResult(
    RoomMutationReceiptResult Receipt,
    RoomTask? Task);
