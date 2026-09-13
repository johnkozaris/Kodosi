using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Design;

namespace Kodosi.Data;

public sealed class DesignTimeDatabase : IDesignTimeDbContextFactory<KodosiDbContext>
{
    public KodosiDbContext CreateDbContext(string[] args)
        => new(new DbContextOptionsBuilder<KodosiDbContext>().UseNpgsql(
            Environment.GetEnvironmentVariable("KODOSI_TEST_DATABASE")
            ?? "Host=localhost;Database=kodosi_design_only;Username=kodosi;Password=unused").Options);
}
