namespace Kodosi.Data;

public sealed class User
{
    public Guid Id { get; set; }
    public string Issuer { get; set; } = "";
    public string Subject { get; set; } = "";
    public string Handle { get; set; } = "";
    public string DisplayName { get; set; } = "";
    public string? Email { get; set; }
    public string? AvatarUrl { get; set; }
    public Guid? IdentityIncarnationId { get; set; }
    public long IdentityRevision { get; set; }
}

public sealed class Device
{
    public string Id { get; set; } = "";
    public Guid UserId { get; set; }
    public string Label { get; set; } = "";
    public string SignerDeviceId { get; set; } = "";
    public byte[] Certificate { get; set; } = [];
    public byte[] CertificateSignature { get; set; } = [];
    public byte[] SigningPublicKey { get; set; } = [];
    public byte[] KemPublicKey { get; set; } = [];
    public long IssuedAtMs { get; set; }
    public long? ExpiresAtMs { get; set; }
    public bool Revoked { get; set; }
}

public sealed class DeviceList
{
    public Guid UserId { get; set; }
    public long Generation { get; set; }
    public string SignerDeviceId { get; set; } = "";
    public byte[] Body { get; set; } = [];
    public byte[] Signature { get; set; } = [];
    public long IssuedAtMs { get; set; }
    public long? ExpiresAtMs { get; set; }
}

public sealed class DeviceChallenge
{
    public Guid Id { get; set; }
    public Guid UserId { get; set; }
    public byte[] Bytes { get; set; } = [];
    public DateTimeOffset ExpiresAt { get; set; }
}

public sealed class DeviceLink
{
    public Guid Id { get; set; }
    public Guid UserId { get; set; }
    public string DeviceCodeHash { get; set; } = "";
    public string UserCode { get; set; } = "";
    public string DeviceId { get; set; } = "";
    public string Label { get; set; } = "";
    public byte[] SigningPublicKey { get; set; } = [];
    public byte[] KemPublicKey { get; set; } = [];
    public DateTimeOffset ExpiresAt { get; set; }
    public string State { get; set; } = "pending";
    public long? ApprovedGeneration { get; set; }
}

public sealed class Friendship
{
    public Guid FirstUserId { get; set; }
    public Guid SecondUserId { get; set; }
    public Guid RequestedBy { get; set; }
    public bool Accepted { get; set; }
    public DateTimeOffset CreatedAt { get; set; }
}

public sealed class Session
{
    public Guid Id { get; set; }
    public Guid IncarnationId { get; set; }
    public Guid OwnerUserId { get; set; }
    public string HostDeviceId { get; set; } = "";
    public string HostName { get; set; } = "";
    public string Name { get; set; } = "";
    public Guid? RoomId { get; set; }
    public long AuthorizationRevision { get; set; } = 1;
    public int KeyGeneration { get; set; }
    public bool Ready { get; set; }
    public bool Ended { get; set; }
    public DateTimeOffset ExpiresAt { get; set; }
    public DateTimeOffset CreatedAt { get; set; }
}

public sealed class SessionMember
{
    public Guid SessionId { get; set; }
    public Guid UserId { get; set; }
}

public sealed class SessionKeyEnvelope
{
    public Guid SessionId { get; set; }
    public string RecipientDeviceId { get; set; } = "";
    public Guid RecipientUserId { get; set; }
    public string SenderDeviceId { get; set; } = "";
    public int KeyGeneration { get; set; }
    public long IssuedAtMs { get; set; }
    public byte[] EncryptedKey { get; set; } = [];
    public byte[] Signature { get; set; } = [];
}

public sealed class Room
{
    public Guid Id { get; set; }
    public string Name { get; set; } = "";
    public Guid OwnerUserId { get; set; }
}

public sealed class RoomMember
{
    public Guid RoomId { get; set; }
    public Guid UserId { get; set; }
}

public sealed class RoomInvitation
{
    public Guid Id { get; set; }
    public Guid RoomId { get; set; }
    public Guid UserId { get; set; }
    public Guid InviterUserId { get; set; }
    public DateTimeOffset CreatedAt { get; set; }
}
