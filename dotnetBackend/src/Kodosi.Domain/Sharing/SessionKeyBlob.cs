
namespace Kodosi.Domain;

public sealed class SessionKeyBlob
{
    public SessionId SessionId { get; private set; }
    public string RecipientDeviceId { get; private set; } = string.Empty;
    public byte[] EncryptedSessionKey { get; private set; } = [];
    public string SenderDeviceId { get; private set; } = string.Empty;
    public byte[] SenderKemPublicKey { get; private set; } = [];
    public int KeyGeneration { get; private set; }
    public int SignatureVersion { get; private set; }
    public byte[] Signature { get; private set; } = [];
    public DateTimeOffset CreatedAt { get; private set; }

    public long IssuedAtMs { get; private set; }

    private SessionKeyBlob() { }

    public static SessionKeyBlob Create(
        SessionId sessionId,
        string recipientDeviceId,
        byte[] encryptedSessionKey,
        string senderDeviceId,
        byte[] senderKemPublicKey,
        int keyGeneration,
        int signatureVersion,
        byte[] signature,
        long issuedAtMs)
    {
        if (string.IsNullOrWhiteSpace(recipientDeviceId))
            throw new DomainException("Recipient device ID is required.");

        if (string.IsNullOrWhiteSpace(senderDeviceId))
            throw new DomainException("Sender device ID is required.");



        if (encryptedSessionKey is not { Length: > 0 })
            throw new DomainException("Encrypted session key is required.");

        if (senderKemPublicKey is not { Length: > 0 })
            throw new DomainException("Sender KEM public key is required.");

        if (signature is not { Length: > 0 })
            throw new DomainException("Key blob signature is required.");



        if (keyGeneration < 0)
            throw new DomainException("Key generation must be non-negative.");
        if (signatureVersion is not (
            SessionKeyBlobSignatureDigest.LegacyVersion or
            SessionKeyBlobSignatureDigest.CurrentVersion))
        {
            throw new DomainException("Unsupported key blob signature version.");
        }

        if (issuedAtMs <= 0)
            throw new DomainException("IssuedAtMs must be a positive Unix epoch milliseconds value.");

        return new SessionKeyBlob
        {
            SessionId = sessionId,
            RecipientDeviceId = recipientDeviceId.Trim(),
            EncryptedSessionKey = encryptedSessionKey,
            SenderDeviceId = senderDeviceId.Trim(),
            SenderKemPublicKey = [.. senderKemPublicKey],
            KeyGeneration = keyGeneration,
            SignatureVersion = signatureVersion,
            Signature = [.. signature],
            CreatedAt = DateTimeOffset.UtcNow,
            IssuedAtMs = issuedAtMs,
        };
    }
}
