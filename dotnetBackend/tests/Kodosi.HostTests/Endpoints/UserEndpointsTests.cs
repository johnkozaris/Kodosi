using Kodosi.Domain;
using Kodosi.Host.Endpoints;

namespace Kodosi.HostTests;

public sealed class UserEndpointsTests
{
    [Theory]
    [InlineData("apple-1234567890abcdef123456@users.Kodosi.invalid", null)]
    [InlineData("person@example.com", "person@example.com")]
    public void ToProfileResponse_Hides_Apple_Placeholder_Email(string email, string? expected)
    {
        var user = User.Create(
            UserId.New(),
            email,
            "handle",
            "Display Name");

        var response = UserEndpoints.ToProfileResponse(user);

        Assert.Equal(expected, response.Email);
    }
}
