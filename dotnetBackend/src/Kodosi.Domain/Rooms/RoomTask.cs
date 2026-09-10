namespace Kodosi.Domain;

public sealed class RoomTask
{
    public Guid Id { get; private set; }
    public RoomId RoomId { get; private set; } = default!;
    public UserId CreatedByUserId { get; private set; } = default!;
    public string Title { get; private set; } = string.Empty;
    public string? Description { get; private set; }
    public RoomTaskStatus Status { get; private set; }
    public Guid? AssignedSessionId { get; private set; }
    public Guid? AssignedSessionIncarnationId { get; private set; }
    public DateTimeOffset? DueAt { get; private set; }
    public DateTimeOffset CreatedAt { get; private set; }
    public DateTimeOffset UpdatedAt { get; private set; }
    public DateTimeOffset? CompletedAt { get; private set; }
    public string? Result { get; private set; }
    public UserId? ResultAuthorUserId { get; private set; }
    public long Revision { get; private set; }
    public uint Version { get; private set; }

    private RoomTask() { }

    public static RoomTask Create(
        Guid id,
        RoomId roomId,
        UserId createdByUserId,
        string title,
        string? description,
        Guid? assignedSessionId,
        DateTimeOffset? dueAt,
        DateTimeOffset now)
    {
        if (id == Guid.Empty)
        {
            throw new DomainException("Task ID is required.");
        }
        if (string.IsNullOrWhiteSpace(title))
        {
            throw new DomainException("Task title cannot be empty.");
        }

        if (title.Length > RoomInputRules.EncryptedContentMaxLength)
        {
            throw new DomainException(
                $"Encrypted task payload exceeds {RoomInputRules.EncryptedContentMaxLength} chars.");
        }
        if (description is not null)
        {
            throw new DomainException(
                "Task description must be encrypted together with the title, not stored separately.");
        }

        return new RoomTask
        {
            Id = id,
            RoomId = roomId,
            CreatedByUserId = createdByUserId,
            Title = title,
            Description = description,
            Status = RoomTaskStatus.Open,
            Revision = 0,
            AssignedSessionId = assignedSessionId,
            DueAt = dueAt,
            CreatedAt = now,
            UpdatedAt = now,
        };
    }

    public void Claim(DateTimeOffset now)
    {
        Require(RoomTaskStatus.Open, "claim");
        Status = RoomTaskStatus.InProgress;
        Touch(now);
    }

    public void Submit(DateTimeOffset now)
    {
        Require(RoomTaskStatus.InProgress, "submit");
        Status = RoomTaskStatus.Review;
        Touch(now);
    }

    public void Complete(string? result, UserId resultAuthorUserId, DateTimeOffset now)
    {
        ValidateResult(result);
        if (Status != RoomTaskStatus.InProgress && Status != RoomTaskStatus.Review)
        {
            throw new InvalidStateException(
                $"Cannot complete a task that is {Status}; must be InProgress or Review.");
        }

        Status = RoomTaskStatus.Done;
        Result = result;
        ResultAuthorUserId = result is null ? null : resultAuthorUserId;
        CompletedAt = now;
        Touch(now);
    }

    public void Archive(DateTimeOffset now)
    {
        Status = RoomTaskStatus.Archived;
        Touch(now);
    }


    public void ApplyOwnerOverride(
        RoomTaskStatus to,
        string? result,
        UserId resultAuthorUserId,
        DateTimeOffset now)
    {
        ValidateResult(result);
        Status = to;
        if (to == RoomTaskStatus.Done)
        {
            CompletedAt ??= now;
        }
        else if (to is RoomTaskStatus.Open or RoomTaskStatus.InProgress or RoomTaskStatus.Review)
        {
            CompletedAt = null;
        }

        if (result is not null)
        {
            Result = result;
            ResultAuthorUserId = resultAuthorUserId;
        }

        Touch(now);
    }

    public void AssignInitialSession(
        Guid assignedSessionId,
        Guid assignedSessionIncarnationId)
    {
        if (assignedSessionId == Guid.Empty || assignedSessionIncarnationId == Guid.Empty)
        {
            throw new DomainException("Initial task assignment requires a session incarnation.");
        }
        AssignedSessionId = assignedSessionId;
        AssignedSessionIncarnationId = assignedSessionIncarnationId;
    }

    public void Assign(
        Guid? assignedSessionId,
        Guid? assignedSessionIncarnationId,
        DateTimeOffset now)
    {
        if (assignedSessionId.HasValue != assignedSessionIncarnationId.HasValue)
        {
            throw new DomainException(
                "Assigned session and incarnation must be present together.");
        }
        AssignedSessionId = assignedSessionId;
        AssignedSessionIncarnationId = assignedSessionIncarnationId;
        Touch(now);
    }

    private static void ValidateResult(string? result)
    {
        if (result?.Length > RoomInputRules.EncryptedContentMaxLength)
        {
            throw new DomainException(
                $"Encrypted task result exceeds {RoomInputRules.EncryptedContentMaxLength} chars.");
        }
    }

    private void Require(RoomTaskStatus expected, string verb)
    {
        if (Status != expected)
        {
            throw new InvalidStateException(
                $"Cannot {verb} a task that is {Status}; must be {expected}.");
        }
    }

    private void Touch(DateTimeOffset now)
    {
        if (Revision == long.MaxValue)
        {
            throw new InvalidStateException("Task revision is exhausted.");
        }
        Revision++;
        UpdatedAt = now;
    }
}
