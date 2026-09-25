using System.Security.Claims;
using System.Security.Cryptography;
using System.Text;
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
            new Claim("iss", Issuer), new Claim("sub", subject), new Claim("preferred_username", name),
            new Claim("email", "unused@example.invalid"), new Claim("picture", "https://example.invalid/avatar.png")
        ], "test"))
    };

    [Fact]
    public async Task RepeatedHandleCollisionsKeepOneCanonicalAccount()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        const string subject = "new-alice";
        var suffix = Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(Issuer + "\n" + subject)))[..10];
        foreach (var handle in new[] { "alice", $"alice-{suffix}" })
            store.Db.Users.Add(new User { Id = Guid.CreateVersion7(), Issuer = Issuer, Subject = handle, Handle = handle, DisplayName = handle });
        await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        var account = await new CurrentUser(store.Db).GetAsync(Context(subject, "Alice"), TestContext.Current.CancellationToken);
        Assert.StartsWith("alice-", account.Handle);
        Assert.NotEqual($"alice-{suffix}", account.Handle);
        Assert.InRange(account.Handle.Length, 3, 64);
        var again = await new CurrentUser(store.Db).GetAsync(Context(subject, "Changed name"), TestContext.Current.CancellationToken);
        Assert.Equal(account.Id, again.Id);
        Assert.Equal(account.Handle, again.Handle);
        Assert.Equal(3, await store.Db.Users.CountAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task ConcurrentAccountCreationPreservesIssuerAndSubjectIdentity()
    {
        var connection = await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken);
        await using (var db = PostgresFixture.Context(connection))
            await DatabaseSetup.InitializeAsync(db, TestContext.Current.CancellationToken);
        var tasks = Enumerable.Range(0, 4).Select(async _ =>
        {
            await using var db = PostgresFixture.Context(connection);
            return (await new CurrentUser(db).GetAsync(Context("same-subject", "same-name"), TestContext.Current.CancellationToken)).Id;
        });
        var ids = await Task.WhenAll(tasks);
        Assert.Single(ids.Distinct());
        await using var inspect = PostgresFixture.Context(connection);
        Assert.Single(await inspect.Users.ToListAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task UnusedProfileClaimsAreNotCollectedAndExistingValuesArePreserved()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var context = Context("profile", "profile");
        var account = await new CurrentUser(store.Db).GetAsync(context, TestContext.Current.CancellationToken);
        Assert.Null(account.Email);
        Assert.Null(account.AvatarUrl);
        account.Email = "saved@example.invalid";
        account.AvatarUrl = "https://saved.invalid/avatar.png";
        await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        store.Db.ChangeTracker.Clear();
        var retained = await new CurrentUser(store.Db).GetAsync(context, TestContext.Current.CancellationToken);
        Assert.Equal("saved@example.invalid", retained.Email);
        Assert.Equal("https://saved.invalid/avatar.png", retained.AvatarUrl);
    }

    [Fact]
    public async Task AccountResponseOmitsUnusedProfileFieldsAndDuplicateDeviceRouteIsAbsent()
    {
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        using var actor = await app.EnrollAsync("profile-response");
        var response = await actor.Client.GetFromJsonAsync<JsonElement>("/api/me", TestContext.Current.CancellationToken);
        Assert.Equal(3, response.EnumerateObject().Count());
        Assert.True(response.TryGetProperty("id", out _));
        Assert.True(response.TryGetProperty("handle", out _));
        Assert.True(response.TryGetProperty("displayName", out _));
        Assert.False(response.TryGetProperty("email", out _));
        Assert.False(response.TryGetProperty("avatarUrl", out _));
        using var removed = await actor.Client.GetAsync("/api/me/devices", TestContext.Current.CancellationToken);
        Assert.Equal(System.Net.HttpStatusCode.MethodNotAllowed, removed.StatusCode);
    }
}
