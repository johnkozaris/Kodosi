using Kodosi.Domain;
using Kodosi.Infrastructure.Persistence;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata;

namespace Kodosi.HostTests;

public sealed class SessionViewerDismissalPersistenceModelTests
{
    [Fact]
    public void Dismissal_Uses_Durable_Composite_Unique_Key_And_Restrictive_Foreign_Keys()
    {
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql("Host=localhost;Database=kodosi_model_test")
            .Options;
        using var context = new KodosiDbContext(options);

        var dismissal = context.Model.FindEntityType(typeof(SessionViewerDismissal))!;

        Assert.Equal("session_viewer_dismissals", dismissal.GetTableName());
        Assert.Equal(
            [nameof(SessionViewerDismissal.SessionId), nameof(SessionViewerDismissal.ViewerUserId)],
            dismissal.FindPrimaryKey()!.Properties.Select(property => property.Name));
        Assert.All(
            dismissal.GetForeignKeys(),
            foreignKey => Assert.Equal(DeleteBehavior.Restrict, foreignKey.DeleteBehavior));
    }
}
