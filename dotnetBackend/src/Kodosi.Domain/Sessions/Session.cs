
namespace Kodosi.Domain;

public sealed class Session
{
    public const int LegacyIncarnationProtocolVersion = 1;
    public const int CurrentIncarnationProtocolVersion = 2;

    public SessionId Id { get; private set; }
    public Guid IncarnationId { get; private set; }
    public long IncarnationGeneration { get; private set; }
    public int IncarnationProtocolVersion { get; private set; }
    public UserId OwnerUserId { get; private set; }
    public ToolKind ToolKind { get; private set; }
    public string Title { get; private set; } = string.Empty;
    public SessionScope Scope { get; private set; }
    public RoomId? RoomId { get; private set; }
    public AccessLevel DefaultAccess { get; private set; }
    public SessionStatus Status { get; private set; }
    public string OwnerSessionSecretHash { get; private set; } = string.Empty;
    public DateTimeOffset StartedAt { get; private set; }
    public DateTimeOffset? EndedAt { get; private set; }
    public DateTimeOffset LastHeartbeatAt { get; private set; }
    public uint Version { get; private set; }

    public string? HostConnectionSlot { get; private set; }
    public DateTimeOffset? HostClaimedAt { get; private set; }
    public DateTimeOffset? HostReleasedAt { get; private set; }
    public int LiveParticipantCount { get; private set; }



    public int CurrentKeyGeneration { get; private set; }

    private Session() { }

    public static Session Create(
        SessionId id,
        Guid incarnationId,
        long incarnationGeneration,
        int incarnationProtocolVersion,
        UserId ownerUserId,
        string title,
        SessionScope scope,
        ToolKind toolKind,
        AccessLevel defaultAccess,
        string ownerSecretHash,
        RoomId? roomId = null)
    {
        if (id.Value == Guid.Empty)
        {
            throw new DomainException("Session ID cannot be empty.");
        }
        if (incarnationId == Guid.Empty)
        {
            throw new DomainException("Session incarnation ID cannot be empty.");
        }
        if (incarnationGeneration <= 0)
        {
            throw new DomainException("Session incarnation generation must be positive.");
        }
        if (incarnationProtocolVersion is not (
            LegacyIncarnationProtocolVersion or
            CurrentIncarnationProtocolVersion))
        {
            throw new DomainException("Unsupported session incarnation protocol version.");
        }
        if (scope == SessionScope.Room && roomId is null)
        {
            throw new DomainException("Room scope requires a room ID.");
        }

        ValidateRequestedDefaultAccess(defaultAccess);

        var now = DateTimeOffset.UtcNow;
        return new Session
        {
            Id = id,
            IncarnationId = incarnationId,
            IncarnationGeneration = incarnationGeneration,
            IncarnationProtocolVersion = incarnationProtocolVersion,
            OwnerUserId = ownerUserId,
            ToolKind = toolKind,
            Title = title,
            Scope = scope,
            RoomId = roomId,
            DefaultAccess = NormalizeDefaultAccess(defaultAccess),
            Status = SessionStatus.Pending,
            OwnerSessionSecretHash = ownerSecretHash,
            StartedAt = now,
            LastHeartbeatAt = now,
        };
    }

    public void ActivateHost(string connectionId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(connectionId);
        EnsureNotEnded();
        if (HostConnectionSlot is not null)
        {
            throw new DomainException("Session already has a host.");
        }
        if (Status is not (
            SessionStatus.Pending or
            SessionStatus.Reconnecting or
            SessionStatus.Live))
        {
            throw new DomainException($"Cannot activate a host from {Status} state.");
        }

        var now = DateTimeOffset.UtcNow;
        Status = SessionStatus.Live;
        LastHeartbeatAt = now;
        HostConnectionSlot = connectionId;
        HostClaimedAt = now;
        HostReleasedAt = null;
    }

    public void ReleaseHostSlot(string? expectedConnectionId)
    {
        if (HostConnectionSlot is null
            || (expectedConnectionId is not null
                && !string.Equals(
                    HostConnectionSlot,
                    expectedConnectionId,
                    StringComparison.Ordinal)))
        {
            return;
        }

        HostConnectionSlot = null;
        HostClaimedAt = null;
        HostReleasedAt = DateTimeOffset.UtcNow;
    }

    public void Republish(
        Guid incarnationId,
        long incarnationGeneration,
        UserId ownerUserId,
        string title,
        SessionScope scope,
        ToolKind toolKind,
        AccessLevel defaultAccess,
        string ownerSecretHash,
        RoomId? roomId)
    {
        if (Status != SessionStatus.Ended || OwnerUserId != ownerUserId)
        {
            throw new DomainException("Only an ended session may be republished by its owner.");
        }
        if (incarnationId == Guid.Empty)
        {
            throw new DomainException("Session incarnation ID cannot be empty.");
        }
        if (incarnationGeneration != IncarnationGeneration + 1)
        {
            throw new DomainException("Session incarnation generation must advance by one.");
        }
        if (scope == SessionScope.Room && roomId is null)
        {
            throw new DomainException("Room scope requires a room ID.");
        }
        ValidateRequestedDefaultAccess(defaultAccess);

        var now = DateTimeOffset.UtcNow;
        if (CurrentKeyGeneration == int.MaxValue)
        {
            throw new DomainException("Session key generation is exhausted.");
        }
        IncarnationId = incarnationId;
        IncarnationGeneration = incarnationGeneration;
        IncarnationProtocolVersion = CurrentIncarnationProtocolVersion;
        Title = title;
        Scope = scope;
        ToolKind = toolKind;
        DefaultAccess = NormalizeDefaultAccess(defaultAccess);
        OwnerSessionSecretHash = ownerSecretHash;
        RoomId = roomId;
        Status = SessionStatus.Pending;
        StartedAt = now;
        EndedAt = null;
        LastHeartbeatAt = now;
        HostConnectionSlot = null;
        HostClaimedAt = null;
        HostReleasedAt = null;
        LiveParticipantCount = 0;
        CurrentKeyGeneration++;
    }

    public bool AcceptsIncarnationHandshake(Guid? expectedIncarnationId) =>
        expectedIncarnationId == IncarnationId;

    public void MarkReconnecting()
    {
        EnsureNotEnded();
        if (Status != SessionStatus.Live)
        {
            throw new DomainException($"Cannot mark reconnecting from {Status} state.");
        }

        Status = SessionStatus.Reconnecting;
    }

    public void End()
    {
        Status = SessionStatus.Ended;
        EndedAt = DateTimeOffset.UtcNow;
    }

    public void FenceKeyPublication()
    {
        EnsureNotEnded();
        if (CurrentKeyGeneration == int.MaxValue)
        {
            throw new DomainException("Session key generation is exhausted.");
        }

        CurrentKeyGeneration++;
    }

    public void RecordHostHeartbeat()
    {
        EnsureNotEnded();
        LastHeartbeatAt = DateTimeOffset.UtcNow;
    }

    public void UpdateTitle(string title)
    {
        EnsureNotEnded();

        if (string.IsNullOrWhiteSpace(title))
        {
            throw new DomainException("Session title cannot be empty.");
        }

        Title = title.Trim();
    }

    public void UpdateScope(SessionScope scope, RoomId? roomId)
    {
        EnsureNotEnded();

        if (scope == SessionScope.Room && roomId is null)
        {
            throw new DomainException("Room scope requires a room ID.");
        }

        Scope = scope;
        RoomId = roomId;
        DefaultAccess = NormalizeDefaultAccess(DefaultAccess);
    }

    public void UpdateDefaultAccess(AccessLevel access)
    {
        EnsureNotEnded();
        ValidateRequestedDefaultAccess(access);
        DefaultAccess = NormalizeDefaultAccess(access);
    }

    public bool IsOwner(UserId userId) => OwnerUserId == userId;

    private static AccessLevel NormalizeDefaultAccess(AccessLevel access) =>
        access is AccessLevel.Inject or AccessLevel.Approve ? AccessLevel.Suggest : access;

    private static void ValidateRequestedDefaultAccess(AccessLevel access)
    {
        if (access is AccessLevel.Inject or AccessLevel.Approve)
        {
            throw new DomainException($"Default access cannot be {access}.");
        }
    }

    private void EnsureNotEnded()
    {
        if (Status == SessionStatus.Ended)
        {
            throw new DomainException("Session has already ended.");
        }
    }
}
