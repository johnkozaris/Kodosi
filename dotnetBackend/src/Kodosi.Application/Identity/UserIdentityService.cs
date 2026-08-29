using System.Security.Cryptography;
using System.Text;
using System.Globalization;
using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class UserIdentityService(
    IUserRepository users,
    IExternalIdentityRepository externalIdentities,
    IUnitOfWork unitOfWork)
{
    private readonly IUserRepository _users = users;
    private readonly IExternalIdentityRepository _externalIdentities = externalIdentities;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;

    public async Task<User> ResolveOrProvisionAsync(
        AuthenticatedUserProfile profile,
        CancellationToken ct = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(
            profile.PrimaryIdentity.Provider, nameof(profile));
        ArgumentException.ThrowIfNullOrWhiteSpace(
            profile.PrimaryIdentity.Issuer, nameof(profile));
        ArgumentException.ThrowIfNullOrWhiteSpace(
            profile.PrimaryIdentity.Subject, nameof(profile));
        ArgumentException.ThrowIfNullOrWhiteSpace(profile.HandleSeed, nameof(profile));

        var identityMatches = await ResolveIdentityMatchesAsync(profile.Identities, ct);
        var matchedUsers = identityMatches.Values
            .Select(identity => identity.UserId)
            .Distinct()
            .ToArray();

        if (matchedUsers.Length > 1)
        {
            throw new ConflictException(
                "Authenticated identities are already linked to different users.");
        }

        User user;
        var changed = false;

        if (matchedUsers.Length == 0)
        {
            var userId = UserId.New();
            var email = profile.Email ?? BuildPlaceholderEmail(profile.PrimaryIdentity);
            var displayName = profile.DisplayName ?? profile.HandleSeed;
            var handle = await GenerateUniqueHandleAsync(profile.HandleSeed, ct);

            user = User.Create(
                userId,
                email,
                handle,
                displayName,
                profile.AvatarUrl);

            await _users.AddAsync(user, ct);
            changed = true;
        }
        else
        {
            user = await _users.GetByIdAsync(matchedUsers[0], ct)
                ?? throw new ConflictException("Authenticated identities resolved to a missing user.");
        }

        foreach (var identity in profile.Identities)
        {
            changed |= await LinkIdentityAsync(
                user.Id,
                identity,
                identityMatches,
                ct);
        }

        var nextEmail = profile.Email ?? user.Email;
        var nextDisplayName = profile.DisplayName ?? user.DisplayName;
        var nextAvatarUrl = profile.AvatarUrl ?? user.AvatarUrl;

        if (user.SyncProfile(nextEmail, nextDisplayName, nextAvatarUrl))
        {
            changed = true;
        }

        if (changed)
        {
            await SaveWithHandleCollisionRetryAsync(user, profile.HandleSeed, ct);
        }

        return user;
    }

    private async Task SaveWithHandleCollisionRetryAsync(
        User user,
        string handleSeed,
        CancellationToken ct)
    {
        const int MaxAttempts = 5;
        for (var attempt = 1; attempt <= MaxAttempts; attempt++)
        {
            try
            {
                await _unitOfWork.SaveChangesAsync(ct);
                return;
            }
            catch (HandleConcurrentlyTakenException ex)
            {
                if (attempt == MaxAttempts)
                {
                    throw new HandleAllocationExhaustedException(handleSeed, ex);
                }
                var freshHandle = await GenerateUniqueHandleAsync(handleSeed, ct);
                user.ReassignHandleForCollisionRetry(freshHandle);
            }
        }
    }

    private async Task<Dictionary<string, ExternalIdentity>> ResolveIdentityMatchesAsync(
        IReadOnlyList<AuthenticatedExternalIdentity> identities,
        CancellationToken ct)
    {
        var matches = new Dictionary<string, ExternalIdentity>(StringComparer.Ordinal);

        foreach (var identity in identities)
        {
            var existing = await _externalIdentities.GetAsync(identity.Provider, identity.Issuer, identity.Subject, ct);
            if (existing is not null)
            {
                matches[identity.IdentityKey] = existing;
            }
        }

        return matches;
    }

    private async Task<bool> LinkIdentityAsync(
        UserId userId,
        AuthenticatedExternalIdentity identity,
        IReadOnlyDictionary<string, ExternalIdentity> existingIdentities,
        CancellationToken ct)
    {
        if (existingIdentities.TryGetValue(identity.IdentityKey, out var existing))
        {
            if (existing.UserId != userId)
            {
                throw new ConflictException(
                    "Authenticated identity is already linked to a different user.");
            }

            return false;
        }

        await _externalIdentities.AddAsync(
            ExternalIdentity.Create(
                userId,
                identity.Provider,
                identity.Issuer,
                identity.Subject),
            ct);

        return true;
    }

    private async Task<string> GenerateUniqueHandleAsync(string seed, CancellationToken ct)
    {
        var stem = BuildHandleStem(seed);
        var occupiedHandles = await _users.GetHandlesByPrefixAsync(stem, ct);
        var occupiedSuffixes = new HashSet<int>();
        foreach (var occupiedHandle in occupiedHandles)
        {
            var normalizedHandle = occupiedHandle.Trim().ToLowerInvariant();
            if (string.Equals(normalizedHandle, stem, StringComparison.Ordinal))
            {
                occupiedSuffixes.Add(1);
                continue;
            }

            if (!normalizedHandle.StartsWith(stem, StringComparison.Ordinal)
                || normalizedHandle.Length <= stem.Length
                || normalizedHandle[stem.Length] != '-')
            {
                continue;
            }

            var suffixText = normalizedHandle[(stem.Length + 1)..];
            if (int.TryParse(suffixText, NumberStyles.None, CultureInfo.InvariantCulture, out var suffix)
                && suffix >= 2
                && string.Equals(suffixText, suffix.ToString(CultureInfo.InvariantCulture), StringComparison.Ordinal))
            {
                occupiedSuffixes.Add(suffix);
            }
        }

        for (var suffix = 1; suffix <= 1000; suffix++)
        {
            if (!occupiedSuffixes.Contains(suffix))
            {
                return suffix == 1 ? stem : AppendSuffix(stem, suffix);
            }
        }

        throw new ConflictException("Unable to provision a unique handle for the authenticated user.");
    }

    private static string BuildHandleStem(string seed)
    {
        var builder = new StringBuilder(capacity: Math.Min(seed.Length, 48));
        var lastWasSeparator = false;

        foreach (var character in seed.Trim().ToLowerInvariant())
        {
            if (char.IsAsciiLetterOrDigit(character))
            {
                builder.Append(character);
                lastWasSeparator = false;
                continue;
            }

            if (!lastWasSeparator && builder.Length > 0)
            {
                builder.Append('-');
                lastWasSeparator = true;
            }
        }

        var stem = builder.ToString().Trim('-');
        if (string.IsNullOrWhiteSpace(stem))
        {
            return "user";
        }

        return stem.Length <= 48 ? stem : stem[..48];
    }

    private static string AppendSuffix(string stem, int suffix)
    {
        var suffixText = suffix.ToString(System.Globalization.CultureInfo.InvariantCulture);
        var maxStemLength = Math.Max(1, 64 - suffixText.Length - 1);
        var truncatedStem = stem.Length <= maxStemLength ? stem : stem[..maxStemLength];
        return $"{truncatedStem}-{suffixText}";
    }

    private static string BuildPlaceholderEmail(AuthenticatedExternalIdentity identity)
    {
        var hash = Convert.ToHexStringLower(
            SHA256.HashData(Encoding.UTF8.GetBytes(identity.IdentityKey)))[..24];
        return $"{identity.Provider}-{hash}@users.Kodosi.invalid";
    }

}
