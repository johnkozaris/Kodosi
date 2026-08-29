
namespace Kodosi.Domain;

public sealed class User
{
    private const int EmailMaxLength = 320;
    private const int HandleMaxLength = 64;
    private const int DisplayNameMaxLength = 128;
    private const int AvatarUrlMaxLength = 2048;

    public UserId Id { get; private set; }
    public string Email { get; private set; } = string.Empty;
    public string Handle { get; private set; } = string.Empty;
    public string DisplayName { get; private set; } = string.Empty;
    public string? AvatarUrl { get; private set; }
    public DateTimeOffset CreatedAt { get; private set; }
    public long IdentityRevision { get; private set; }
    public Guid? IdentityIncarnationId { get; private set; }

    private User() { }

    public static User Create(
        UserId id,
        string email,
        string handle,
        string displayName,
        string? avatarUrl = null)
    {
        var normalized = NormalizeProfile(email, handle, displayName, avatarUrl);

        return new User
        {
            Id = id,
            Email = normalized.Email,
            Handle = normalized.Handle,
            DisplayName = normalized.DisplayName,
            AvatarUrl = normalized.AvatarUrl,
            CreatedAt = DateTimeOffset.UtcNow,
            IdentityRevision = 0,
            IdentityIncarnationId = null,
        };
    }

    public bool SyncProfile(
        string email,
        string displayName,
        string? avatarUrl = null)
    {
        var normalizedEmail = email.Trim();
        var normalizedDisplayName = displayName.Trim();
        var normalizedAvatarUrl = string.IsNullOrWhiteSpace(avatarUrl) ? null : avatarUrl.Trim();
        var changed = false;

        if (string.IsNullOrWhiteSpace(normalizedEmail))
            throw new ArgumentException("Email is required.", nameof(email));

        if (string.IsNullOrWhiteSpace(normalizedDisplayName))
            throw new ArgumentException("Display name is required.", nameof(displayName));

        if (normalizedEmail.Length > EmailMaxLength)
            throw new ArgumentOutOfRangeException(nameof(email), $"Email must be {EmailMaxLength} characters or fewer.");

        if (normalizedDisplayName.Length > DisplayNameMaxLength)
            throw new ArgumentOutOfRangeException(nameof(displayName), $"Display name must be {DisplayNameMaxLength} characters or fewer.");

        if (normalizedAvatarUrl is not null && normalizedAvatarUrl.Length > AvatarUrlMaxLength)
            throw new ArgumentOutOfRangeException(nameof(avatarUrl), $"Avatar URL must be {AvatarUrlMaxLength} characters or fewer.");

        if (Email != normalizedEmail)
        {
            Email = normalizedEmail;
            changed = true;
        }

        if (DisplayName != normalizedDisplayName)
        {
            DisplayName = normalizedDisplayName;
            changed = true;
        }

        if (AvatarUrl != normalizedAvatarUrl)
        {
            AvatarUrl = normalizedAvatarUrl;
            changed = true;
        }

        return changed;
    }

    public long AdvanceIdentityLifecycle(Guid? incarnationId)
    {
        IdentityRevision = checked(IdentityRevision + 1);
        IdentityIncarnationId = incarnationId;
        return IdentityRevision;
    }

    public void ReassignHandleForCollisionRetry(string handle)
    {
        var normalized = handle.Trim().ToLowerInvariant();
        if (string.IsNullOrWhiteSpace(normalized))
            throw new ArgumentException("Handle is required.", nameof(handle));
        if (normalized.Length > HandleMaxLength)
            throw new ArgumentOutOfRangeException(
                nameof(handle),
                $"Handle must be {HandleMaxLength} characters or fewer.");
        Handle = normalized;
    }

    private static (string Email, string Handle, string DisplayName, string? AvatarUrl)
        NormalizeProfile(
            string email,
            string handle,
            string displayName,
            string? avatarUrl)
    {
        var normalizedEmail = email.Trim();
        var normalizedHandle = handle.Trim().ToLowerInvariant();
        var normalizedDisplayName = displayName.Trim();
        var normalizedAvatarUrl = string.IsNullOrWhiteSpace(avatarUrl) ? null : avatarUrl.Trim();

        if (string.IsNullOrWhiteSpace(normalizedEmail))
            throw new ArgumentException("Email is required.", nameof(email));

        if (string.IsNullOrWhiteSpace(normalizedHandle))
            throw new ArgumentException("Handle is required.", nameof(handle));

        if (string.IsNullOrWhiteSpace(normalizedDisplayName))
            throw new ArgumentException("Display name is required.", nameof(displayName));

        if (normalizedEmail.Length > EmailMaxLength)
            throw new ArgumentOutOfRangeException(nameof(email), $"Email must be {EmailMaxLength} characters or fewer.");

        if (normalizedHandle.Length > HandleMaxLength)
            throw new ArgumentOutOfRangeException(nameof(handle), $"Handle must be {HandleMaxLength} characters or fewer.");

        if (normalizedDisplayName.Length > DisplayNameMaxLength)
            throw new ArgumentOutOfRangeException(nameof(displayName), $"Display name must be {DisplayNameMaxLength} characters or fewer.");

        if (normalizedAvatarUrl is not null && normalizedAvatarUrl.Length > AvatarUrlMaxLength)
            throw new ArgumentOutOfRangeException(nameof(avatarUrl), $"Avatar URL must be {AvatarUrlMaxLength} characters or fewer.");

        return (
            normalizedEmail,
            normalizedHandle,
            normalizedDisplayName,
            normalizedAvatarUrl);
    }
}
