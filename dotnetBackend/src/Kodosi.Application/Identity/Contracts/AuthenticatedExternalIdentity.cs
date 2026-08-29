using System.Text;

namespace Kodosi.Application;

public sealed record AuthenticatedExternalIdentity(
    string Provider,
    string Issuer,
    string Subject)
{
    public string IdentityKey => string.Join(
        ':',
        "v1",
        EncodeComponent(Provider.Trim().ToLowerInvariant()),
        EncodeComponent(Issuer.Trim()),
        EncodeComponent(Subject.Trim()));

    private static string EncodeComponent(string value) =>
        Convert.ToBase64String(Encoding.UTF8.GetBytes(value));
}
