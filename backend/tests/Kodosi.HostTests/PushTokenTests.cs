using System.Net;
using System.Net.Http.Json;
using Kodosi.Data;
using Kodosi.Push;
using Kodosi.Security;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class PushTokenTests(PostgresFixture postgres)
{
    private static readonly string AppleToken = new('b', 64);

    private static async Task<HttpStatusCode> SendAsync(ApiDevice actor, HttpMethod method, object? body = null)
    {
        using var request = await BackendApplication.SignedAsync(actor, method, "/api/me/push-token", body);
        using var response = await actor.Client.SendAsync(request, TestContext.Current.CancellationToken);
        return response.StatusCode;
    }

    private static async Task<List<PushToken>> TokensAsync(BackendApplication app)
    {
        await using var scope = app.Services.CreateAsyncScope();
        return await scope.ServiceProvider.GetRequiredService<KodosiDbContext>().PushTokens.AsNoTracking()
            .ToListAsync(TestContext.Current.CancellationToken);
    }

    [Fact]
    public async Task ADeviceRegistersReplacesAndRemovesItsOwnToken()
    {
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken), settings: PushKeys.Values());
        using var alice = await app.EnrollAsync("alice");

        Assert.Equal(HttpStatusCode.NoContent, await SendAsync(alice, HttpMethod.Put, new { platform = "apns", token = AppleToken, environment = "development" }));
        var apple = Assert.Single(await TokensAsync(app));
        Assert.Equal((alice.Fixture.DeviceId, alice.Fixture.UserId, "apns", AppleToken, "development"),
            (apple.DeviceId, apple.UserId, apple.Platform, apple.Token, apple.Environment));
        Assert.Equal(HttpStatusCode.NoContent, await SendAsync(alice, HttpMethod.Put, new { platform = "fcm", token = "fcm:token-1" }));
        var google = Assert.Single(await TokensAsync(app));
        Assert.Equal(("fcm", "fcm:token-1", (string?)null), (google.Platform, google.Token, google.Environment));
        Assert.Equal(HttpStatusCode.NoContent, await SendAsync(alice, HttpMethod.Delete));
        Assert.Equal(HttpStatusCode.NoContent, await SendAsync(alice, HttpMethod.Delete));
        Assert.Empty(await TokensAsync(app));
    }

    [Fact]
    public async Task ATokenNeedsItsFormAndTheDeviceThatGivesIt()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(ct), settings: PushKeys.Values());
        using var alice = await app.EnrollAsync("alice");
        foreach (var body in new object[]
                 {
                     new { platform = "gcm", token = "fcm:token-1" },
                     new { platform = "apns", token = AppleToken },
                     new { platform = "apns", token = AppleToken, environment = "staging" },
                     new { platform = "apns", token = new string('g', 64), environment = "production" },
                     new { platform = "apns", token = new string('a', 63), environment = "production" },
                     new { platform = "apns", token = new string('a', 201), environment = "production" },
                     new { platform = "fcm", token = "fcm:token-1", environment = "production" },
                     new { platform = "fcm", token = "" },
                     new { platform = "fcm", token = new string('f', 4097) },
                     new { platform = "fcm", token = "fcm\ttoken" },
                     new { platform = "fcm", token = "fcm:token-1", device = "other" },
                 })
            Assert.Equal(HttpStatusCode.BadRequest, await SendAsync(alice, HttpMethod.Put, body));
        using (var unproven = await alice.Client.PutAsJsonAsync("/api/me/push-token", new { platform = "fcm", token = "fcm:token-1" }, ct))
            Assert.Equal((HttpStatusCode)428, unproven.StatusCode);
        using var stranger = app.CreateClient();
        using (var anonymous = await stranger.PutAsJsonAsync("/api/me/push-token", new { platform = "fcm", token = "fcm:token-1" }, ct))
            Assert.Equal(HttpStatusCode.Unauthorized, anonymous.StatusCode);
        Assert.Empty(await TokensAsync(app));
    }

    [Fact]
    public async Task ATokenBelongsToTheLastDeviceThatGivesItAndAPlatformWithoutSettingsTakesNone()
    {
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken),
            settings: PushKeys.Values(apns: false));
        using var alice = await app.EnrollAsync("alice");
        using var bob = await app.EnrollAsync("bob");
        Assert.Equal(HttpStatusCode.NotFound, await SendAsync(alice, HttpMethod.Put, new { platform = "apns", token = AppleToken, environment = "production" }));
        foreach (var actor in new[] { alice, bob })
            Assert.Equal(HttpStatusCode.NoContent, await SendAsync(actor, HttpMethod.Put, new { platform = "fcm", token = "fcm:token-1" }));
        Assert.Equal(bob.Fixture.DeviceId, Assert.Single(await TokensAsync(app)).DeviceId);
    }

    [Fact]
    public async Task RemovingADeviceOrDeletingTheAccountRemovesItsToken()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner");
        var phone = new DeviceFixture(owner.User.Id, "phone-device");
        var issued = DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds();
        var certificate = phone.CertificateBody(owner.Device.Id, issued);
        store.Db.Devices.Add(new Device
        {
            Id = phone.DeviceId, UserId = owner.User.Id, SignerDeviceId = owner.Device.Id, SigningPublicKey = phone.SigningKey, IssuedAtMs = issued,
            Certificate = certificate, CertificateSignature = owner.Fixture.Sign(Proofs.Tagged(DomainTags.DeviceCertV3, certificate)),
        });
        var both = DeviceFixture.ListBody(owner.User.Id, 2, [(owner.Device.Id, owner.Device.Id), (phone.DeviceId, owner.Device.Id)], owner.Device.Id, issued + 1);
        var list = await store.Db.DeviceLists.SingleAsync(x => x.UserId == owner.User.Id, ct);
        (list.Generation, list.Body, list.Signature, list.IssuedAtMs) = (2, both, owner.Fixture.Sign(Proofs.Tagged(DomainTags.DeviceListV1, both)), issued + 1);
        await store.Db.SaveChangesAsync(ct);
        var tokens = new PushTokens(store.Db, PushKeys.Settings(), TimeProvider.System);
        await tokens.RegisterAsync(owner.User.Id, phone.DeviceId, new("apns", AppleToken, "production"), ct);
        await tokens.RegisterAsync(owner.User.Id, owner.Device.Id, new("fcm", "fcm:token-1"), ct);
        store.Db.ChangeTracker.Clear();

        var alone = DeviceFixture.ListBody(owner.User.Id, 3, [(owner.Device.Id, owner.Device.Id)], owner.Device.Id, issued + 2);
        await store.Devices.ReplaceListAsync(owner.User.Id, new(Convert.ToBase64String(alone), owner.Fixture.SignedList(alone)), ct);
        Assert.Equal(owner.Device.Id, Assert.Single(await store.Db.PushTokens.AsNoTracking().ToListAsync(ct)).DeviceId);
        await store.Accounts.DeleteSignedInAsync(owner.User, DateTimeOffset.UtcNow, ct);
        Assert.Empty(await store.Db.PushTokens.AsNoTracking().ToListAsync(ct));
    }

    [Fact]
    public async Task ADeliveryRemovesTheTokensThatAppleAndGoogleNoLongerTake()
    {
        var ct = TestContext.Current.CancellationToken;
        var kept = new string('c', 64);
        var apple = new PushHandler(request => request.Uri.AbsolutePath.EndsWith(kept, StringComparison.Ordinal)
            ? (HttpStatusCode.OK, "")
            : (HttpStatusCode.Gone, """{"reason":"Unregistered","timestamp":1}"""));
        var google = new PushHandler(request => request.Uri.Host == "oauth2.googleapis.com"
            ? (HttpStatusCode.OK, """{"access_token":"google-access","expires_in":3599}""")
            : (HttpStatusCode.NotFound, """{"error":{"code":404,"status":"NOT_FOUND","details":[{"@type":"type.googleapis.com/google.firebase.fcm.v1.FcmError","errorCode":"UNREGISTERED"}]}}"""));
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(ct), services =>
        {
            services.AddHttpClient(nameof(Apns)).ConfigurePrimaryHttpMessageHandler(() => apple);
            services.AddHttpClient(nameof(Fcm)).ConfigurePrimaryHttpMessageHandler(() => google);
        }, PushKeys.Values());
        using var alice = await app.EnrollAsync("alice");
        using var bob = await app.EnrollAsync("bob");
        using var carol = await app.EnrollAsync("carol");
        Assert.Equal(HttpStatusCode.NoContent, await SendAsync(alice, HttpMethod.Put, new { platform = "apns", token = AppleToken, environment = "production" }));
        Assert.Equal(HttpStatusCode.NoContent, await SendAsync(bob, HttpMethod.Put, new { platform = "fcm", token = "fcm:token-1" }));
        Assert.Equal(HttpStatusCode.NoContent, await SendAsync(carol, HttpMethod.Put, new { platform = "apns", token = kept, environment = "development" }));

        var delivery = app.Services.GetRequiredService<PushDelivery>();
        foreach (var actor in new[] { alice, bob, carol })
            await delivery.DeliverAsync(new PushNotice(Guid.CreateVersion7(), actor.Fixture.UserId, new TerminalStatus("claude", "done"), 0), ct);

        Assert.Equal(carol.Fixture.DeviceId, Assert.Single(await TokensAsync(app)).DeviceId);
        Assert.Equal(2, apple.Sent.Count);
        Assert.Single(google.Sent, sent => sent.Uri.Host == "fcm.googleapis.com");
    }
}
