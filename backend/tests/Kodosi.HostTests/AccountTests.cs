using System.Security.Claims;
using System.Net.Http.Json;
using System.Text.Json;
using Kodosi.Accounts;
using Kodosi.Data;
using Microsoft.AspNetCore.Http;
using Microsoft.EntityFrameworkCore;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class AccountTests(PostgresFixture postgres)
{
    private const string Issuer = "https://accounts.test.invalid/";
    private static DefaultHttpContext Context(string subject, string name) => new()
    {
        User = new ClaimsPrincipal(new ClaimsIdentity([
            new Claim("iss", Issuer), new Claim("sub", subject), new Claim("preferred_username", name)
        ], "test"))
    };

    [Fact]
    public async Task AUserNameIsTheHandleOfOneAccountOnly()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var account = await new CurrentUser(store.Db, TimeProvider.System).GetAsync(Context("alice-subject", "Alice"), ct);
        Assert.Equal("alice", account.Handle);
        var again = await new CurrentUser(store.Db, TimeProvider.System).GetAsync(Context("alice-subject", "renamed"), ct);
        Assert.Equal((account.Id, "alice"), (again.Id, again.Handle));
        var taken = await Assert.ThrowsAsync<ApiException>(() => new CurrentUser(store.Db, TimeProvider.System).GetAsync(Context("other-subject", "alice"), ct));
        Assert.Equal(409, taken.Status);
        foreach (var name in new[] { "", "al", "alice smith", "älice", new string('a', 65) })
        {
            var refused = await Assert.ThrowsAsync<ApiException>(() => new CurrentUser(store.Db, TimeProvider.System).GetAsync(Context($"subject-{name.Length}", name), ct));
            Assert.Equal(403, refused.Status);
        }
        Assert.Equal(1, await store.Db.Users.CountAsync(ct));
    }

    [Fact]
    public async Task ConcurrentAccountCreationPreservesIssuerAndSubjectIdentity()
    {
        var connection = await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken);
        await using (var db = PostgresFixture.Context(connection))
            await DatabaseSetup.MigrateAsync(db, TestContext.Current.CancellationToken);
        var tasks = Enumerable.Range(0, 4).Select(async _ =>
        {
            await using var db = PostgresFixture.Context(connection);
            return (await new CurrentUser(db, TimeProvider.System).GetAsync(Context("same-subject", "same-name"), TestContext.Current.CancellationToken)).Id;
        });
        var ids = await Task.WhenAll(tasks);
        Assert.Single(ids.Distinct());
        await using var inspect = PostgresFixture.Context(connection);
        Assert.Single(await inspect.Users.ToListAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task WithoutTheSignInServiceTheAccountHasNoDeletionPage()
    {
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        using var actor = await app.EnrollAsync("profile-response");
        var response = await actor.Client.GetFromJsonAsync<JsonElement>("/api/me", TestContext.Current.CancellationToken);
        Assert.Equal(["id", "handle", "displayName", "deletionUri"], response.EnumerateObject().Select(x => x.Name));
        Assert.Equal(JsonValueKind.Null, response.GetProperty("deletionUri").ValueKind);
    }
}
