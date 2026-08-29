using Kodosi.Domain;

namespace Kodosi.DomainTests;

public sealed class SessionAccessOverrideExpiryTests
{
    [Fact]
    public void Create_Uses_The_Explicit_Time_Box()
    {
        var createdAt = DateTimeOffset.UtcNow;
        var expiresAt = createdAt.AddHours(24);

        var accessOverride = SessionAccessOverride.Create(
            SessionId.New(),
            UserId.New(),
            AccessLevel.View,
            UserId.New(),
            expiresAt,
            createdAt);

        Assert.Equal(createdAt, accessOverride.CreatedAt);
        Assert.Equal(expiresAt, accessOverride.ExpiresAt);
    }

    [Fact]
    public void IsActiveAt_Excludes_Expired_Grant()
    {
        var now = DateTimeOffset.UtcNow;
        var accessOverride = SessionAccessOverride.Create(
            SessionId.New(),
            UserId.New(),
            AccessLevel.Suggest,
            UserId.New(),
            now.AddHours(1),
            now);

        Assert.True(accessOverride.IsActiveAt(now));
        Assert.False(accessOverride.IsActiveAt(now.AddHours(1)));
    }
}
