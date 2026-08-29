using Kodosi.Domain;

namespace Kodosi.DomainTests;

public class UserTests
{
    [Fact]
    public void Create_NormalizesHandleToLowerInvariant()
    {
        var user = User.Create(
            UserId.New(),
            "person@example.com",
            "MixedCaseUser",
            "Display Name");

        Assert.Equal("mixedcaseuser", user.Handle);
    }

    [Fact]
    public void SyncProfile_DoesNotChangeHandle()
    {
        var user = User.Create(
            UserId.New(),
            "person@example.com",
            "startuser",
            "Display Name");

        user.SyncProfile(
            "next@example.com",
            "Updated Name",
            "https://example.com/avatar.png");

        Assert.Equal("startuser", user.Handle);
        Assert.Equal("next@example.com", user.Email);
        Assert.Equal("Updated Name", user.DisplayName);
    }

}
