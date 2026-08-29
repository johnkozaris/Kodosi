namespace Kodosi.Application;

public interface IOwnerSessionSecretHasher
{
    string Hash(string secret);
    bool Verify(string hashedSecret, string providedSecret);
}
