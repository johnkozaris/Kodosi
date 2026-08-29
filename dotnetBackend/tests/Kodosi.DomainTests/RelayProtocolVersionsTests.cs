using Kodosi.Domain;

namespace Kodosi.DomainTests;

public sealed class RelayProtocolVersionsTests
{
    [Fact]
    public void Current_Is_Exact_Protocol()
    {
        Assert.Equal(10, RelayProtocolVersions.Current);
    }

    [Theory]
    [InlineData(null)]
    [InlineData(int.MinValue)]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(2)]
    [InlineData(3)]
    [InlineData(4)]
    [InlineData(5)]
    [InlineData(6)]
    [InlineData(7)]
    [InlineData(8)]
    [InlineData(int.MaxValue)]
    public void Admission_Rejects_Every_Version_Except_Current(int? advertised)
    {
        Assert.False(RelayProtocolVersions.IsAccepted(advertised));
    }

    [Fact]
    public void Admission_Accepts_Current_Version()
    {
        Assert.True(RelayProtocolVersions.IsAccepted(RelayProtocolVersions.Current));
    }
}
