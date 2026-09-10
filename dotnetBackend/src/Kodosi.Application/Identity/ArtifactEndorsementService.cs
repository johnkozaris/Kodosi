using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class ArtifactEndorsementService(
    IArtifactEndorsementRepository endorsements,
    IUserRepository users,
    IUserDeviceRepository devices,
    IUserDeviceListRepository lists,
    IPopSignatureVerifier signatures,
    IUserLifecycleLock lifecycle,
    IUnitOfWork unitOfWork,
    UserIdentityBundleService identities,
    TimeProvider timeProvider)
{
    public const int MaximumPageSize = 100;

    public async Task PutAsync(
        UserId userId,
        Guid identityIncarnationId,
        string artifactDigest,
        string endorserDeviceId,
        byte[] signature,
        CancellationToken ct = default)
    {
        var incoming = ArtifactEndorsement.Create(
            userId, identityIncarnationId, artifactDigest, endorserDeviceId, signature);
        await using var transaction = await unitOfWork.BeginTransactionAsync(ct);
        await lifecycle.AcquireAsync(userId, ct);
        var user = await users.GetByIdAsync(userId, ct);
        if (user?.IdentityIncarnationId != identityIncarnationId)
        {
            throw new ConflictException("Artifact endorsement targets a retired identity incarnation.");
        }
        var device = await devices.GetByDeviceIdAsync(endorserDeviceId, ct);
        var list = await lists.GetLatestAsync(userId, ct);
        if (!ActiveDeviceAuthorization.IsAuthorized(device, list, userId, timeProvider.GetUtcNow())
            || !signatures.Verify(
                device!.SigningPublicKey,
                ArtifactEndorsement.CreatePreimage(userId, identityIncarnationId, artifactDigest, endorserDeviceId),
                signature))
        {
            throw new PolicyViolationException("Artifact endorsement requires a valid signature from an active device of this user.");
        }
        await endorsements.DeleteOtherIncarnationsAsync(userId, identityIncarnationId, ct);
        var existing = await endorsements.GetAsync(userId, identityIncarnationId, artifactDigest, ct);
        if (existing is null)
        {
            if (await endorsements.CountAsync(userId, identityIncarnationId, ct) >= ArtifactEndorsement.MaximumPerIdentity)
            {
                throw new ConflictException("Artifact endorsement retention limit reached.");
            }
            await endorsements.AddAsync(incoming, ct);
        }
        else
        {
            existing.ReplaceWith(incoming);
        }
        await unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
    }

    public async Task<IReadOnlyList<ArtifactEndorsement>?> GetAsync(
        UserId callerUserId,
        UserId targetUserId,
        string digest,
        CancellationToken ct = default)
    {
        ArtifactEndorsement.RequireDigest(digest);
        var identity = await identities.GetAsync(callerUserId, targetUserId, ct);
        if (identity is null)
        {
            return null;
        }
        var endorsement = await endorsements.GetAsync(targetUserId, identity.IdentityIncarnationId, digest, ct);
        return endorsement is null ? [] : [endorsement];
    }

    public async Task<ArtifactEndorsementPage> ListAsync(
        UserId userId, int offset, int limit, CancellationToken ct = default)
    {
        if (offset < 0 || offset > ArtifactEndorsement.MaximumPerIdentity || limit is < 1 or > MaximumPageSize)
        {
            throw new DomainException("Artifact endorsement pagination is outside the supported bounds.");
        }
        var user = await users.GetByIdAsync(userId, ct);
        if (user?.IdentityIncarnationId is not { } incarnationId)
        {
            return new ArtifactEndorsementPage([], false, null);
        }
        var rows = await endorsements.ListAsync(userId, incarnationId, offset, limit + 1, ct);
        var hasMore = rows.Count > limit;
        return new ArtifactEndorsementPage(rows.Take(limit).ToList(), hasMore, hasMore ? offset + limit : null);
    }
}

public sealed record ArtifactEndorsementPage(IReadOnlyList<ArtifactEndorsement> Items, bool HasMore, int? NextOffset);
