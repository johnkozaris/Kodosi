
namespace Kodosi.Domain;

public sealed class ExternalIdentity
{
    private const int ProviderMaxLength = 64;
    private const int IssuerMaxLength = 512;
    private const int SubjectMaxLength = 512;

    public Guid Id { get; private set; }
    public UserId UserId { get; private set; }
    public string Provider { get; private set; } = string.Empty;
    public string Issuer { get; private set; } = string.Empty;
    public string Subject { get; private set; } = string.Empty;
    public DateTimeOffset LinkedAt { get; private set; }

    private ExternalIdentity() { }

    public static ExternalIdentity Create(
        UserId userId,
        string provider,
        string issuer,
        string subject,
        DateTimeOffset? linkedAt = null)
    {
        var normalized = Normalize(provider, issuer, subject);

        return new ExternalIdentity
        {
            Id = Guid.NewGuid(),
            UserId = userId,
            Provider = normalized.Provider,
            Issuer = normalized.Issuer,
            Subject = normalized.Subject,
            LinkedAt = linkedAt ?? DateTimeOffset.UtcNow,
        };
    }

    private static (string Provider, string Issuer, string Subject) Normalize(
        string provider,
        string issuer,
        string subject)
    {
        var normalizedProvider = provider.Trim().ToLowerInvariant();
        var normalizedIssuer = issuer.Trim();
        var normalizedSubject = subject.Trim();

        if (string.IsNullOrWhiteSpace(normalizedProvider))
            throw new ArgumentException("Provider is required.", nameof(provider));

        if (string.IsNullOrWhiteSpace(normalizedIssuer))
            throw new ArgumentException("Issuer is required.", nameof(issuer));

        if (string.IsNullOrWhiteSpace(normalizedSubject))
            throw new ArgumentException("Subject is required.", nameof(subject));

        if (normalizedProvider.Length > ProviderMaxLength)
            throw new ArgumentOutOfRangeException(nameof(provider), $"Provider must be {ProviderMaxLength} characters or fewer.");

        if (normalizedIssuer.Length > IssuerMaxLength)
            throw new ArgumentOutOfRangeException(nameof(issuer), $"Issuer must be {IssuerMaxLength} characters or fewer.");

        if (normalizedSubject.Length > SubjectMaxLength)
            throw new ArgumentOutOfRangeException(nameof(subject), $"Subject must be {SubjectMaxLength} characters or fewer.");

        return (normalizedProvider, normalizedIssuer, normalizedSubject);
    }
}
