namespace Kodosi.Domain;

public sealed class IdentityExposure
{
    public UserId IdentityOwnerUserId { get; private set; }
    public UserId RecipientUserId { get; private set; }
    public DateTimeOffset FirstExposedAt { get; private set; }
    public DateTimeOffset LastExposedAt { get; private set; }

    private IdentityExposure() { }

    public static IdentityExposure Create(
        UserId identityOwnerUserId,
        UserId recipientUserId,
        DateTimeOffset exposedAt)
    {
        if (identityOwnerUserId == recipientUserId)
        {
            throw new DomainException("Identity exposure must reference two distinct users.");
        }
        return new IdentityExposure
        {
            IdentityOwnerUserId = identityOwnerUserId,
            RecipientUserId = recipientUserId,
            FirstExposedAt = exposedAt,
            LastExposedAt = exposedAt,
        };
    }

    public void MarkExposed(DateTimeOffset exposedAt)
    {
        if (exposedAt > LastExposedAt)
        {
            LastExposedAt = exposedAt;
        }
    }
}
