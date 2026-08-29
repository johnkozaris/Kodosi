
namespace Kodosi.Domain;

public sealed class SessionAccessOverride
{
    public static readonly TimeSpan MaximumLifetime = TimeSpan.FromDays(7);

    public SessionId SessionId { get; private set; }
    public UserId ActorUserId { get; private set; }
    public AccessLevel AccessLevel { get; private set; }
    public UserId GrantedByUserId { get; private set; }
    public DateTimeOffset CreatedAt { get; private set; }
    public DateTimeOffset? RevokedAt { get; private set; }
    public DateTimeOffset? ExpiresAt { get; private set; }

    public bool IsActiveAt(DateTimeOffset now) =>
        RevokedAt is null && (ExpiresAt is null || ExpiresAt > now);

    private SessionAccessOverride() { }

    public static SessionAccessOverride Create(
        SessionId sessionId,
        UserId actorUserId,
        AccessLevel accessLevel,
        UserId grantedByUserId,
        DateTimeOffset expiresAt,
        DateTimeOffset createdAt)
    {
        ValidateExpiry(createdAt, expiresAt);
        return new SessionAccessOverride
        {
            SessionId = sessionId,
            ActorUserId = actorUserId,
            AccessLevel = accessLevel,
            GrantedByUserId = grantedByUserId,
            CreatedAt = createdAt,
            ExpiresAt = expiresAt,
        };
    }

    public void Revoke()
        => Revoke(DateTimeOffset.UtcNow);

    public void Revoke(DateTimeOffset revokedAt)
    {
        RevokedAt = revokedAt;
    }

    public void Grant(
        AccessLevel accessLevel,
        UserId grantedByUserId,
        DateTimeOffset expiresAt,
        DateTimeOffset grantedAt)
    {
        ValidateExpiry(grantedAt, expiresAt);
        AccessLevel = accessLevel;
        GrantedByUserId = grantedByUserId;
        CreatedAt = grantedAt;
        ExpiresAt = expiresAt;
        RevokedAt = null;
    }

    private static void ValidateExpiry(DateTimeOffset now, DateTimeOffset expiresAt)
    {
        if (expiresAt <= now)
        {
            throw new DomainException("Access expiry must be in the future.");
        }
        if (expiresAt > now + MaximumLifetime)
        {
            throw new DomainException("Access expiry cannot be more than 7 days in the future.");
        }
    }
}
