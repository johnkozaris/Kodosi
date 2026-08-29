using Microsoft.AspNetCore.Identity;
using Kodosi.Application;

namespace Kodosi.Infrastructure.Crypto;

public sealed class OwnerSessionSecretHasher : IOwnerSessionSecretHasher
{
    private readonly object _userMarker = new();
    private readonly PasswordHasher<object> _passwordHasher = new();

    public string Hash(string secret)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(secret);
        return _passwordHasher.HashPassword(_userMarker, secret);
    }

    public bool Verify(string hashedSecret, string providedSecret)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(hashedSecret);
        ArgumentException.ThrowIfNullOrWhiteSpace(providedSecret);

        var result = _passwordHasher.VerifyHashedPassword(
            _userMarker,
            hashedPassword: hashedSecret,
            providedPassword: providedSecret);

        return result is PasswordVerificationResult.Success or PasswordVerificationResult.SuccessRehashNeeded;
    }
}
