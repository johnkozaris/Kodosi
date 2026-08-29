using Kodosi.Domain;
using Kodosi.Infrastructure.Persistence;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata;

namespace Kodosi.HostTests;

public sealed class DeviceLinkPersistenceModelTests
{
    [Fact]
    public void DeviceLinkRequest_Uses_Postgres_RowVersion()
    {
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql("Host=localhost;Database=metadata_only")
            .Options;
        using var context = new KodosiDbContext(options);

        var version = context.Model
            .FindEntityType(typeof(DeviceLinkRequest))!
            .FindProperty(nameof(DeviceLinkRequest.Version));

        Assert.NotNull(version);
        Assert.True(version!.IsConcurrencyToken);
        Assert.Equal(ValueGenerated.OnAddOrUpdate, version.ValueGenerated);
    }
}
