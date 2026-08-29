namespace Kodosi.Application;

public sealed record AuthenticatedUserProfile(
    AuthenticatedExternalIdentity PrimaryIdentity,
    IReadOnlyList<AuthenticatedExternalIdentity> Identities,
    string HandleSeed,
    string? Email,
    string? DisplayName,
    string? AvatarUrl);
