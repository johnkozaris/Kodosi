using System.Net;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Security.Claims;
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
    private static DefaultHttpContext Context(string subject, string name, string? displayName = null) => new()
    {
        User = new ClaimsPrincipal(new ClaimsIdentity([
            new Claim("iss", Issuer), new Claim("sub", subject), new Claim("preferred_username", name),
            .. displayName is null ? Array.Empty<Claim>() : [new Claim("name", displayName)],
        ], "test"))
    };

    [Fact]
    public async Task NamesAndDeviceLabelsLoseTheInvisibleCharactersThatReorderOrHideLetters()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var account = await new CurrentUser(store.Db, TimeProvider.System).GetAsync(Context("bob-subject", "bob", "Al\u200Bice \u202Eecila\u202C 👨\u200D💻"), ct);
        Assert.Equal("Alice ecila 👨\u200D💻", account.DisplayName);
        Assert.Equal("Laptop", Limits.Text("\u2066Lap\uFEFFtop\u2069", "Device label", 128));
    }

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
        var lookalike = await Assert.ThrowsAsync<ApiException>(() => new CurrentUser(store.Db, TimeProvider.System).GetAsync(Context("lookalike-subject", "\u212Aodosi"), ct));
        Assert.Equal(403, lookalike.Status);
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
    public async Task WithoutTheSignInServiceTheAppDeletesTheAccountAfterARecentSignIn()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(ct));
        using var actor = await app.EnrollAsync("leaving");
        var me = await actor.Client.GetFromJsonAsync<JsonElement>("/api/me", ct);
        Assert.Equal(JsonValueKind.Null, me.GetProperty("deletionUri").ValueKind);
        using (var stale = await actor.Client.DeleteAsync("/api/me", ct))
            Assert.Equal(HttpStatusCode.Forbidden, stale.StatusCode);
        using var recent = app.CreateClient();
        recent.DefaultRequestHeaders.Authorization = new AuthenticationHeaderValue("Bearer", app.Token("leaving", authenticatedAt: DateTimeOffset.UtcNow));
        using (var deleted = await recent.DeleteAsync("/api/me", ct))
            Assert.Equal(HttpStatusCode.NoContent, deleted.StatusCode);
        using var gone = await actor.Client.GetAsync("/api/me", ct);
        Assert.Equal(HttpStatusCode.Gone, gone.StatusCode);
    }
}
