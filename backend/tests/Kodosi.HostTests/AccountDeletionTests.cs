using System.Net;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Security.Claims;
using System.Text.Json;
using Kodosi.Accounts;
using Microsoft.AspNetCore.Http;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Microsoft.Extensions.Logging.Abstractions;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class AccountDeletionTests(PostgresFixture postgres)
{
    private static readonly Dictionary<string, string?> SignInService = new()
    {
        ["Auth:AccountDeletion:ClientId"] = "kodosi-backend",
        ["Auth:AccountDeletion:ClientSecret"] = "test-secret",
        ["Auth:AccountDeletion:AdminUrl"] = "http://127.0.0.1:18084",
    };

    private static DefaultHttpContext SignedIn(string subject, DateTimeOffset at) => new()
    {
        User = new ClaimsPrincipal(new ClaimsIdentity([
            new Claim("iss", BackendApplication.Issuer), new Claim("sub", subject), new Claim("preferred_username", subject),
            new Claim("auth_time", at.ToUnixTimeSeconds().ToString(), ClaimValueTypes.Integer64)
        ], "test"))
    };

    [Fact]
    public async Task ADeletedAccountRefusesEarlierSignInsAndALaterSignInStartsFresh()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var before = DateTimeOffset.UtcNow.AddMinutes(-2);
        var account = await new CurrentUser(store.Db, TimeProvider.System).GetAsync(SignedIn("leaving", before), ct);
        await store.Accounts.DeleteSignedInAsync(account, before, ct);
        store.Db.ChangeTracker.Clear();
        var refused = await Assert.ThrowsAsync<ApiException>(() => new CurrentUser(store.Db, TimeProvider.System).GetAsync(SignedIn("leaving", before), ct));
        Assert.Equal(410, refused.Status);
        Assert.Empty(await store.Db.Users.ToListAsync(ct));
        var fresh = await new CurrentUser(store.Db, TimeProvider.System).GetAsync(SignedIn("leaving", DateTimeOffset.UtcNow.AddSeconds(5)), ct);
        Assert.NotEqual(account.Id, fresh.Id);
        Assert.Empty(await store.Db.DeletedAccounts.ToListAsync(ct));
    }

    [Fact]
    public async Task ADeletedAccountIsForgottenAfterThirtyDays()
    {
        var ct = TestContext.Current.CancellationToken;
        var clock = new ManualClock();
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(ct), services =>
        {
            services.RemoveAll<TimeProvider>(); services.AddSingleton<TimeProvider>(clock);
        });
        var scopes = app.Services.GetRequiredService<IServiceScopeFactory>();
        await using (var scope = scopes.CreateAsyncScope())
            await scope.ServiceProvider.GetRequiredService<AccountService>().ForgetAsync(BackendApplication.Issuer, "gone", clock.GetUtcNow(), ct);
        using var cleanup = new Kodosi.Sessions.PublicationCleanup(scopes, clock, NullLogger<Kodosi.Sessions.PublicationCleanup>.Instance);
        await cleanup.SweepAsync(ct);
        await using (var scope = scopes.CreateAsyncScope())
            Assert.Single(await scope.ServiceProvider.GetRequiredService<Kodosi.Data.KodosiDbContext>().DeletedAccounts.ToListAsync(ct));
        clock.Advance(AccountService.DeletionMemory + TimeSpan.FromMinutes(1));
        await cleanup.SweepAsync(ct);
        await using (var scope = scopes.CreateAsyncScope())
            Assert.Empty(await scope.ServiceProvider.GetRequiredService<Kodosi.Data.KodosiDbContext>().DeletedAccounts.ToListAsync(ct));
    }

    [Fact]
    public async Task TheSignInServiceDeletesTheAccountAndTheAppOnlyGetsTheConfirmationPage()
    {
        var ct = TestContext.Current.CancellationToken;
        var keycloak = new FakeKeycloak();
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(ct),
            services => services.AddHttpClient(nameof(AccountDeletionFeed)).ConfigurePrimaryHttpMessageHandler(() => keycloak), SignInService);
        using var actor = await app.EnrollAsync("leaving");
        var me = await actor.Client.GetFromJsonAsync<JsonElement>("/api/me", ct);
        var page = new Uri(me.GetProperty("deletionUri").GetString()!);
        Assert.Equal(BackendApplication.Issuer + "/protocol/openid-connect/auth", page.GetLeftPart(UriPartial.Path));
        var query = System.Web.HttpUtility.ParseQueryString(page.Query);
        Assert.Equal("account-console", query["client_id"]);
        Assert.Equal("delete_account", query["kc_action"]);
        Assert.Equal(BackendApplication.Issuer + "/account/", query["redirect_uri"]);
        Assert.Equal("S256", query["code_challenge_method"]);
        Assert.Equal(43, query["code_challenge"]!.Length);
        var again = await actor.Client.GetFromJsonAsync<JsonElement>("/api/me", ct);
        Assert.NotEqual(me.GetProperty("deletionUri").GetString(), again.GetProperty("deletionUri").GetString());
        using var direct = await actor.Client.DeleteAsync("/api/me", ct);
        Assert.Equal(HttpStatusCode.Conflict, direct.StatusCode);
    }

    [Fact]
    public async Task TheFeedDeletesWhatTheSignInServiceDeletedAndOnlyThat()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(ct));
        using var leaving = await app.EnrollAsync("leaving");
        using var removed = await app.EnrollAsync("removed-by-admin");
        using var staying = await app.EnrollAsync("staying");
        var deletedAt = DateTimeOffset.UtcNow.AddSeconds(-5);
        var keycloak = new FakeKeycloak();
        keycloak.UserEvents.Add(new { id = "e1", time = deletedAt.ToUnixTimeMilliseconds(), type = "DELETE_ACCOUNT", userId = "leaving" });
        keycloak.AdminEvents.Add(new { id = "a1", time = deletedAt.ToUnixTimeMilliseconds(), operationType = "DELETE", resourceType = "USER", resourcePath = "users/removed-by-admin" });
        keycloak.AdminEvents.Add(new { id = "a2", time = deletedAt.ToUnixTimeMilliseconds(), operationType = "DELETE", resourceType = "USER", resourcePath = "users/staying/credentials/key" });
        var settings = AccountDeletionSettings.From(new ConfigurationBuilder().AddInMemoryCollection(
            new Dictionary<string, string?> { ["Auth:Authority"] = BackendApplication.Issuer }.Concat(SignInService)).Build());
        var scopes = app.Services.GetRequiredService<IServiceScopeFactory>();
        using var feed = new AccountDeletionFeed(scopes, new Clients(keycloak), settings, TimeProvider.System, NullLogger<AccountDeletionFeed>.Instance);

        await feed.PassAsync(ct);
        await feed.PassAsync(ct);

        Assert.Equal(new[] { "Bearer " + FakeKeycloak.AccessToken }, keycloak.Authorizations.Distinct());
        Assert.Equal(1, keycloak.TokenRequests);
        Assert.All(keycloak.Reads, read => Assert.StartsWith("http://127.0.0.1:18084/admin/realms/kodosi/", read));
        await using (var scope = scopes.CreateAsyncScope())
        {
            var db = scope.ServiceProvider.GetRequiredService<Kodosi.Data.KodosiDbContext>();
            Assert.Equal("staying", (await db.Users.SingleAsync(ct)).Subject);
            var remembered = await db.DeletedAccounts.OrderBy(x => x.Subject).ToListAsync(ct);
            Assert.Equal(new[] { "leaving", "removed-by-admin" }, remembered.Select(x => x.Subject));
            Assert.All(remembered, x => Assert.Equal(deletedAt.ToUnixTimeMilliseconds(), x.DeletedAt.ToUnixTimeMilliseconds()));
        }
        using var gone = await leaving.Client.GetAsync("/api/me", ct);
        Assert.Equal(HttpStatusCode.Gone, gone.StatusCode);
        using var kept = await staying.Client.GetAsync("/api/me", ct);
        Assert.Equal(HttpStatusCode.OK, kept.StatusCode);
    }

    private sealed class Clients(HttpMessageHandler handler) : IHttpClientFactory
    {
        public HttpClient CreateClient(string name) => new(handler, disposeHandler: false);
    }

    private sealed class FakeKeycloak : HttpMessageHandler
    {
        public static readonly string AccessToken = "eyJ" + new string('k', 1400);
        public List<object> UserEvents { get; } = [];
        public List<object> AdminEvents { get; } = [];
        public List<string> Authorizations { get; } = [];
        public List<string> Reads { get; } = [];
        public int TokenRequests { get; private set; }

        protected override async Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
        {
            var path = request.RequestUri!.AbsolutePath;
            if (path == "/realms/kodosi/protocol/openid-connect/token")
            {
                var form = System.Web.HttpUtility.ParseQueryString(await request.Content!.ReadAsStringAsync(cancellationToken));
                Assert.Equal(("client_credentials", "kodosi-backend", "test-secret"), (form["grant_type"], form["client_id"], form["client_secret"]));
                TokenRequests++;
                return JsonContent(new { access_token = AccessToken, expires_in = 300 });
            }
            Reads.Add(request.RequestUri.ToString());
            Authorizations.Add(request.Headers.Authorization?.ToString() ?? "");
            var query = System.Web.HttpUtility.ParseQueryString(request.RequestUri.Query);
            Assert.Matches(@"^\d{4}-\d{2}-\d{2}$", query["dateFrom"]);
            var items = query["first"] == "0"
                ? path.EndsWith("/admin-events", StringComparison.Ordinal) ? AdminEvents : UserEvents
                : [];
            return JsonContent(items);
        }

        private static HttpResponseMessage JsonContent(object body) => new(HttpStatusCode.OK)
        {
            Content = new StringContent(JsonSerializer.Serialize(body), new MediaTypeHeaderValue("application/json"))
        };
    }
}
