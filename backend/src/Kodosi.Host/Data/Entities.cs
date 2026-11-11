namespace Kodosi.Data;

public sealed class User
{
    public Guid Id { get; set; }
    public string Issuer { get; set; } = "";
    public string Subject { get; set; } = "";
    public string Handle { get; set; } = "";
    public string DisplayName { get; set; } = "";
    public Guid? IdentityIncarnationId { get; set; }
    public string? Plan { get; set; }
    public DateTimeOffset? NameChecksUntil { get; set; }
    public DateTimeOffset? NameChecksRefusedAt { get; set; }
    public int NameChecksRefusedIntervals { get; set; }
    public DateTimeOffset? BannedAt { get; set; }
}

public sealed class DeletedAccount
{
    public string Issuer { get; set; } = "";
    public string Subject { get; set; } = "";
    public DateTimeOffset DeletedAt { get; set; }
}

public sealed class Device
{
    public string Id { get; set; } = "";
    public Guid UserId { get; set; }
    public string SignerDeviceId { get; set; } = "";
    public byte[] Certificate { get; set; } = [];
    public byte[] CertificateSignature { get; set; } = [];
    public byte[] SigningPublicKey { get; set; } = [];
    public byte[]? RecoveryBox { get; set; }
    public long IssuedAtMs { get; set; }
    public long? ExpiresAtMs { get; set; }
    public bool Revoked { get; set; }
}

public sealed class PushToken
{
    public string DeviceId { get; set; } = "";
    public Guid UserId { get; set; }
    public string Platform { get; set; } = "";
    public string Token { get; set; } = "";
    public string? Environment { get; set; }
    public DateTimeOffset UpdatedAt { get; set; }
}

public sealed class DeviceList
{
    public Guid UserId { get; set; }
    public long Generation { get; set; }
    public byte[] Body { get; set; } = [];
    public byte[] Signature { get; set; } = [];
    public long IssuedAtMs { get; set; }
    public long? ExpiresAtMs { get; set; }
}

public sealed class DeviceLink
{
    public Guid Id { get; set; }
    public Guid UserId { get; set; }
    public string DeviceCodeHash { get; set; } = "";
    public byte[] Nonce { get; set; } = [];
    public byte[] Proof { get; set; } = [];
    public byte[]? ApprovalProof { get; set; }
    public string DeviceId { get; set; } = "";
    public string Label { get; set; } = "";
    public byte[] SigningPublicKey { get; set; } = [];
    public DateTimeOffset ExpiresAt { get; set; }
    public string State { get; set; } = "pending";
}

public sealed class Block
{
    public Guid BlockerId { get; set; }
    public Guid BlockedId { get; set; }
    public DateTimeOffset CreatedAt { get; set; }
}

public sealed class IdentityReset
{
    public Guid UserId { get; set; }
    public Guid Id { get; set; }
    public string DeviceId { get; set; } = "";
    public string DeviceLabel { get; set; } = "";
    public byte[] SigningPublicKey { get; set; } = [];
    public DateTimeOffset RequestedAt { get; set; }
    public DateTimeOffset ReadyAt { get; set; }
}

public sealed class FriendList
{
    public Guid UserId { get; set; }
    public long Revision { get; set; }
    public byte[] Body { get; set; } = [];
    public byte[] Signature { get; set; } = [];
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
    public Guid? RestoredFrom { get; set; }
    public Guid? ProjectId { get; set; }
    public long ProjectSequence { get; set; }
    public string? ProjectDeviceId { get; set; }
    public byte[]? ProjectSignature { get; set; }
    public string? ProjectHistory { get; set; }
    public Guid? CloseUserId { get; set; }
    public string? CloseDeviceId { get; set; }
    public byte[]? CloseSignature { get; set; }
    public long AuthorizationRevision { get; set; } = 1;
    public bool Ended { get; set; }
    public DateTimeOffset ExpiresAt { get; set; }
    public DateTimeOffset CreatedAt { get; set; }
}

public sealed class SessionMember
{
    public Guid SessionId { get; set; }
    public Guid UserId { get; set; }
}

public sealed class HiddenSession
{
    public Guid UserId { get; set; }
    public Guid SessionId { get; set; }
}

public sealed class Project
{
    public Guid Id { get; set; }
    public string Name { get; set; } = "";
    public Guid OwnerUserId { get; set; }
    public bool Dosi { get; set; }
    public long KeyVersion { get; set; }
    public long ContentSequence { get; set; }
    public long ContentBytes { get; set; }
    public bool HistoryTrimmed { get; set; }
}

public sealed class ProjectMember
{
    public Guid ProjectId { get; set; }
    public Guid UserId { get; set; }
}

public sealed class ProjectInvitation
{
    public Guid Id { get; set; }
    public Guid ProjectId { get; set; }
    public Guid UserId { get; set; }
    public Guid InviterUserId { get; set; }
    public DateTimeOffset CreatedAt { get; set; }
}

public sealed class ProjectRecipientKey
{
    public string DeviceId { get; set; } = "";
    public Guid UserId { get; set; }
    public byte[] PublicKey { get; set; } = [];
    public byte[] Signature { get; set; } = [];
}

public sealed class ProjectKeyState
{
    public Guid ProjectId { get; set; }
    public long Version { get; set; }
    public long Epoch { get; set; }
    public byte[] Body { get; set; } = [];
    public byte[] Signature { get; set; } = [];
}

public sealed class ProjectItem
{
    public Guid ProjectId { get; set; }
    public Guid Id { get; set; }
    public string Kind { get; set; } = "";
    public long Version { get; set; }
    public long Sequence { get; set; }
    public long KeyVersion { get; set; }
    public byte[] Body { get; set; } = [];
    public byte[] Signature { get; set; } = [];
    public Guid UserId { get; set; }
    public string DeviceId { get; set; } = "";
    public DateTimeOffset CreatedAt { get; set; }
}
