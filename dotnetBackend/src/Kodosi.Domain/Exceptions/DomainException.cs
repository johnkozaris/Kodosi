namespace Kodosi.Domain;

public class DomainException : Exception
{
    public DomainException(
        string message,
        string code = "DOMAIN_ERROR",
        Exception? innerException = null)
        : base(message, innerException)
    {
        Code = code;
    }

    public string Code { get; }
}

public sealed class NotFoundException(string entityName, object id) : DomainException($"{entityName} '{id}' not found.", "NOT_FOUND")
{
}

public sealed class PolicyViolationException(string message) : DomainException(message, "POLICY_VIOLATION")
{
}

public sealed class InvalidStateException(string message) : DomainException(message, "INVALID_STATE")
{
}

public class ConflictException : DomainException
{
    public ConflictException(string message)
        : base(message, "CONFLICT") { }

    public ConflictException(string message, Exception innerException)
        : base(message, "CONFLICT")
    {
        Data["InnerException"] = innerException;
    }

    protected ConflictException(string message, string code)
        : base(message, code) { }
}

public sealed class SessionEndMutationTargetConflictException : ConflictException
{
    public SessionEndMutationTargetConflictException()
        : base(
            "The session end mutation identifier was already used for a different target.",
            "SESSION_END_MUTATION_TARGET_CONFLICT")
    { }
}

public sealed class ConcurrentModificationException : ConflictException
{
    public ConcurrentModificationException(Exception? innerException = null)
        : base(
            "Another request modified the same record concurrently. Refresh and retry.",
            "CONCURRENT_MODIFICATION")
    {
        if (innerException is not null)
        {
            Data["InnerException"] = innerException;
        }
    }
}

public sealed class HandleAllocationExhaustedException : DomainException
{
    public string HandleSeed { get; }

    public HandleAllocationExhaustedException(string handleSeed, Exception? innerException = null)
        : base(
            $"Could not allocate a unique handle for seed '{handleSeed}' after retry budget exhausted.",
            "HANDLE_ALLOCATION_EXHAUSTED")
    {
        HandleSeed = handleSeed;
        if (innerException is not null)
        {
            Data["InnerException"] = innerException;
        }
    }
}

public sealed class HandleConcurrentlyTakenException : DomainException
{
    public HandleConcurrentlyTakenException(Exception? innerException = null)
        : base(
            "A concurrent write claimed the same user handle; retry with a fresh suffix.",
            "HANDLE_CONCURRENTLY_TAKEN")
    {
        if (innerException is not null)
        {
            Data["InnerException"] = innerException;
        }
    }
}

public sealed class FriendRequestThrottledException(TimeSpan retryAfter) : DomainException(
        "Friend request to this user was already sent recently; wait before retrying.",
        "FRIEND_REQUEST_THROTTLED")
{
    public TimeSpan RetryAfter { get; } = retryAfter;
}

public sealed class DeviceLinkPollThrottledException(TimeSpan retryAfter) : DomainException(
        "Device link polling is too frequent; wait before retrying.",
        "DEVICE_LINK_POLL_THROTTLED")
{
    public TimeSpan RetryAfter { get; } = retryAfter;
}

public sealed class RoomSlugConflictException(string slug) : ConflictException($"Room slug '{slug}' already exists.", "ROOM_SLUG_CONFLICT")
{
}

public sealed class RoomMemberAlreadyExistsException : ConflictException
{
    public RoomMemberAlreadyExistsException()
        : base("User is already a member of this room.", "ROOM_MEMBER_ALREADY_EXISTS") { }
}

public sealed class FriendRequestAlreadyExistsException : ConflictException
{
    public FriendRequestAlreadyExistsException()
        : base("A friend request between these users already exists.", "FRIEND_REQUEST_ALREADY_EXISTS") { }
}

public sealed class DeviceLinkUserCodeCollisionException : ConflictException
{
    public DeviceLinkUserCodeCollisionException(Exception? innerException = null)
        : base(
            "A concurrent device-link request claimed the generated user code; retry allocation.",
            "DEVICE_LINK_USER_CODE_COLLISION")
    {
        if (innerException is not null)
        {
            Data["InnerException"] = innerException;
        }
    }
}

public sealed class DeviceLinkReceiptInvalidatedException : ConflictException
{
    public DeviceLinkReceiptInvalidatedException()
        : base(
            "The device-link approval was invalidated by an identity reset.",
            "DEVICE_LINK_RECEIPT_INVALIDATED")
    { }
}

public sealed class DeviceAlreadyEnrolledException : ConflictException
{
    public DeviceAlreadyEnrolledException(Exception? innerException = null)
        : base(
            "Device already registered; refresh enrollment challenge and retry.",
            "DEVICE_ALREADY_ENROLLED")
    {
        if (innerException is not null)
        {
            Data["InnerException"] = innerException;
        }
    }
}

public sealed class DeviceListGenerationCollisionException : ConflictException
{
    public DeviceListGenerationCollisionException(Exception? innerException = null)
        : base(
            "Device list generation collision; refresh identity bundle and retry.",
            "DEVICE_LIST_GENERATION_COLLISION")
    {
        if (innerException is not null)
        {
            Data["InnerException"] = innerException;
        }
    }
}

public sealed class InvalidSessionKeyPayloadException(string message) : DomainException(message, "INVALID_SESSION_KEY_PAYLOAD")
{
}

public sealed class MissingRequiredParameterException(string parameterName) : DomainException($"'{parameterName}' is required.", "MISSING_REQUIRED_PARAMETER")
{
}

public sealed class InvalidParameterException(string parameterName, string requirement)
    : DomainException(
        $"'{parameterName}' {requirement}.",
        "INVALID_PARAMETER")
{
}
