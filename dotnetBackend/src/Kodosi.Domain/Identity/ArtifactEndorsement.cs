namespace Kodosi.Domain;

public sealed class ArtifactEndorsement
{
    public const int MaximumPerIdentity = 50_000;
    public static ReadOnlySpan<byte> SignatureDomain => DomainTags.ArtifactEndorsementV1;

    public UserId UserId { get; private set; }
    public Guid IdentityIncarnationId { get; private set; }
    public string ArtifactDigest { get; private set; } = string.Empty;
    public string EndorserDeviceId { get; private set; } = string.Empty;
    public byte[] Signature { get; private set; } = [];

    private ArtifactEndorsement() { }

    public static ArtifactEndorsement Create(
        UserId userId,
        Guid identityIncarnationId,
        string artifactDigest,
        string endorserDeviceId,
        byte[] signature)
    {
        _ = CreatePreimage(userId, identityIncarnationId, artifactDigest, endorserDeviceId);
        if (signature.Length != IdentityWireFormat.MlDsa65SignatureLength)
        {
            throw new DomainException("Artifact endorsement signature has an invalid length.");
        }
        return new ArtifactEndorsement
        {
            UserId = userId,
            IdentityIncarnationId = identityIncarnationId,
            ArtifactDigest = artifactDigest,
            EndorserDeviceId = endorserDeviceId,
            Signature = signature.ToArray(),
        };
    }

    public void ReplaceWith(ArtifactEndorsement endorsement)
    {
        if (UserId != endorsement.UserId
            || IdentityIncarnationId != endorsement.IdentityIncarnationId
            || ArtifactDigest != endorsement.ArtifactDigest)
        {
            throw new DomainException("Artifact endorsement target cannot change.");
        }
        EndorserDeviceId = endorsement.EndorserDeviceId;
        Signature = endorsement.Signature.ToArray();
    }

    public static void RequireDigest(string digest)
    {
        if (digest.Length != 64 || digest.Any(character => character is not (>= '0' and <= '9' or >= 'a' and <= 'f')))
        {
            throw new DomainException("Artifact digest must be a lowercase hexadecimal SHA-256 digest.");
        }
    }

    public static byte[] CreatePreimage(
        UserId userId,
        Guid identityIncarnationId,
        string artifactDigest,
        string endorserDeviceId)
    {
        if (userId.Value == Guid.Empty || identityIncarnationId == Guid.Empty)
        {
            throw new DomainException("Artifact endorsement requires a user and identity incarnation.");
        }
        RequireDigest(artifactDigest);
        if (DeviceIdRules.Require(endorserDeviceId) != endorserDeviceId)
        {
            throw new DomainException("Artifact endorser device ID must be canonical.");
        }
        using var stream = new MemoryStream();
        stream.Write(SignatureDomain);
        CanonicalLengthPrefixedUtf8.Write(stream, userId.Value.ToString("D"));
        stream.Write(identityIncarnationId.ToByteArray(bigEndian: true));
        CanonicalLengthPrefixedUtf8.Write(stream, artifactDigest);
        CanonicalLengthPrefixedUtf8.Write(stream, endorserDeviceId);
        return stream.ToArray();
    }
}
