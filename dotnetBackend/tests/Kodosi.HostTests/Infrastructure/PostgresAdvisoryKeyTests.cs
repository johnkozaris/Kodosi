using Kodosi.Infrastructure.Persistence;

namespace Kodosi.HostTests;

public sealed class PostgresAdvisoryKeyTests
{
    [Fact]
    public void EqualIdentifiersRemainSeparatedAcrossLockFamilies()
    {
        var id = Guid.Parse("01900000-0000-7000-8000-000000000001");
        var keys = new[]
        {
            PostgresAdvisoryKey.Derive("kodosi-user-lifecycle"u8, id),
            PostgresAdvisoryKey.Derive("kodosi-room-lifecycle"u8, id),
            PostgresAdvisoryKey.Derive("kodosi-room-create"u8, id),
            PostgresAdvisoryKey.Derive("kodosi-room-chat-message"u8, id),
        };

        Assert.Equal(keys.Length, keys.Distinct().Count());
    }

    [Fact]
    public void EmptyLockDomainIsRejected()
    {
        Assert.Throws<ArgumentException>(() =>
            PostgresAdvisoryKey.Derive([], Guid.NewGuid()));
    }
}
