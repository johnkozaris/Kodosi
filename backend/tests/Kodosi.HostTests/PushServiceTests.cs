using System.Net;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Kodosi.Data;
using Kodosi.Push;
using Kodosi.TerminalConnections;
using Microsoft.Extensions.Configuration;
using Xunit;

namespace Kodosi.HostTests;

public sealed class PushServiceTests
{
    private static readonly Guid TerminalId = Guid.Parse("0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d");
    private static readonly string AppleToken = new('a', 64);

    private static PushNotice Notice(TerminalStatus status, int blocked = 0) => new(TerminalId, Guid.CreateVersion7(), status, blocked);

    private static string Part(string jwt, int index) => Encoding.UTF8.GetString(System.Buffers.Text.Base64Url.DecodeFromChars(jwt.Split('.')[index]));

    private static bool Verify(string jwt, Func<byte[], byte[], bool> verify)
    {
        var parts = jwt.Split('.');
        return verify(Encoding.ASCII.GetBytes($"{parts[0]}.{parts[1]}"), System.Buffers.Text.Base64Url.DecodeFromChars(parts[2]));
    }

    [Fact]
    public async Task AnApplePushCarriesOnlyIdentifiersAndASentenceKeyWithAReusedProviderToken()
    {
        var ct = TestContext.Current.CancellationToken;
        var clock = new ManualClock();
        var apple = new PushHandler(_ => (HttpStatusCode.OK, ""));
        var apns = new Apns(PushKeys.Settings(fcm: false).Apns!, new PushClients(apple), clock);
        var now = clock.GetUtcNow().ToUnixTimeSeconds();

        Assert.Equal(Delivered.Sent, (await apns.SendAsync(new PushToken { Token = AppleToken, Environment = "production" },
            Notice(new TerminalStatus("claude", "blocked", "permission", 2), blocked: 2), ct)).Outcome);
        Assert.Equal(Delivered.Sent, (await apns.SendAsync(new PushToken { Token = AppleToken, Environment = "development" },
            Notice(new TerminalStatus("aider", "done")), ct)).Outcome);

        var (blocked, done) = (apple.Sent[0], apple.Sent[1]);
        Assert.Equal($"https://api.push.apple.com/3/device/{AppleToken}", blocked.Uri.ToString());
        Assert.Equal($"https://api.sandbox.push.apple.com/3/device/{AppleToken}", done.Uri.ToString());
        Assert.All(apple.Sent, sent => Assert.Equal((HttpMethod.Post, HttpVersion.Version20), (sent.Method, sent.Version)));
        Assert.Equal("com.kodosi.mobile", blocked.Headers["apns-topic"]);
        Assert.Equal("alert", blocked.Headers["apns-push-type"]);
        Assert.Equal(("10", "5"), (blocked.Headers["apns-priority"], done.Headers["apns-priority"]));
        Assert.Equal(TerminalId.ToString(), blocked.Headers["apns-collapse-id"]);
        Assert.Equal((now + 3600).ToString(System.Globalization.CultureInfo.InvariantCulture), blocked.Headers["apns-expiration"]);
        Assert.Equal(
            """{"aps":{"alert":{"loc-key":"%@ asks for permission","loc-args":["Claude Code"]},"sound":"default","mutable-content":1,"thread-id":"0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d","interruption-level":"time-sensitive","relevance-score":1.0,"category":"kodosi.terminal","badge":2},"kodosi":{"terminal":"0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d","agent":"claude","state":"blocked","kind":"permission"}}""",
            blocked.Body);
        Assert.Equal(
            """{"aps":{"alert":{"loc-key":"A terminal finished"},"sound":"default","mutable-content":1,"thread-id":"0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d","interruption-level":"active","relevance-score":0.5,"category":"kodosi.terminal","badge":0},"kodosi":{"terminal":"0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d","agent":"aider","state":"done","kind":null}}""",
            done.Body);

        var bearer = blocked.Headers["Authorization"];
        Assert.StartsWith("bearer ", bearer, StringComparison.Ordinal);
        var jwt = bearer["bearer ".Length..];
        Assert.Equal("""{"alg":"ES256","kid":"KEY1234567"}""", Part(jwt, 0));
        Assert.Equal($$"""{"iss":"TEAM123456","iat":{{now}}}""", Part(jwt, 1));
        Assert.True(Verify(jwt, (data, signature) => PushKeys.Apple.VerifyData(data, signature, HashAlgorithmName.SHA256)));
        Assert.Equal(bearer, done.Headers["Authorization"]);
        clock.Advance(Apns.TokenLife);
        await apns.SendAsync(new PushToken { Token = AppleToken, Environment = "production" }, Notice(new TerminalStatus(State: "error")), ct);
        Assert.NotEqual(bearer, apple.Sent[2].Headers["Authorization"]);
    }

    [Theory]
    [InlineData(HttpStatusCode.Gone, """{"reason":"Unregistered","timestamp":1}""", "Gone")]
    [InlineData(HttpStatusCode.BadRequest, """{"reason":"BadDeviceToken"}""", "Gone")]
    [InlineData(HttpStatusCode.BadRequest, """{"reason":"DeviceTokenNotForTopic"}""", "Gone")]
    [InlineData(HttpStatusCode.BadRequest, """{"reason":"BadTopic"}""", "Refused")]
    [InlineData(HttpStatusCode.TooManyRequests, """{"reason":"TooManyRequests"}""", "Retry")]
    [InlineData(HttpStatusCode.ServiceUnavailable, """{"reason":"ServiceUnavailable"}""", "Retry")]
    public async Task AppleSaysWhichTokensAreGone(HttpStatusCode status, string reason, string outcome)
    {
        var apns = new Apns(PushKeys.Settings(fcm: false).Apns!, new PushClients(new PushHandler(_ => (status, reason))), TimeProvider.System);
        var delivery = await apns.SendAsync(new PushToken { Token = AppleToken, Environment = "production" },
            Notice(new TerminalStatus("codex", "error")), TestContext.Current.CancellationToken);
        Assert.Equal(outcome, delivery.Outcome.ToString());
    }

    [Fact]
    public async Task AGooglePushIsADataMessageSentWithAnAccessTokenOfTheServiceAccount()
    {
        var ct = TestContext.Current.CancellationToken;
        var clock = new ManualClock();
        var google = new PushHandler(request => request.Uri.Host == "oauth2.googleapis.com"
            ? (HttpStatusCode.OK, """{"access_token":"google-access","expires_in":3599,"token_type":"Bearer"}""")
            : (HttpStatusCode.OK, """{"name":"projects/kodosi-test/messages/1"}"""));
        var fcm = new Fcm(PushKeys.Settings(apns: false).Fcm!, new PushClients(google), clock);
        var now = clock.GetUtcNow().ToUnixTimeSeconds();

        foreach (var status in new[] { new TerminalStatus("claude", "blocked", "permission", 2), new TerminalStatus(State: "done") })
            Assert.Equal(Delivered.Sent, (await fcm.SendAsync(new PushToken { Token = "fcm:token-1" }, Notice(status), ct)).Outcome);

        var (grant, blocked, done) = (Assert.Single(google.Sent, x => x.Uri.Host == "oauth2.googleapis.com"), google.Sent[1], google.Sent[2]);
        var form = System.Web.HttpUtility.ParseQueryString(grant.Body);
        Assert.Equal((HttpMethod.Post, "https://oauth2.googleapis.com/token"), (grant.Method, grant.Uri.ToString()));
        Assert.Equal("urn:ietf:params:oauth:grant-type:jwt-bearer", form["grant_type"]);
        var assertion = form["assertion"]!;
        Assert.Equal("""{"alg":"RS256","typ":"JWT","kid":"key-1"}""", Part(assertion, 0));
        Assert.Equal(
            $$"""{"iss":"push@kodosi-test.iam.gserviceaccount.com","scope":"https://www.googleapis.com/auth/firebase.messaging","aud":"https://oauth2.googleapis.com/token","iat":{{now}},"exp":{{now + 3600}}}""",
            Part(assertion, 1));
        Assert.True(Verify(assertion, (data, signature) => PushKeys.Google.VerifyData(data, signature, HashAlgorithmName.SHA256, RSASignaturePadding.Pkcs1)));

        Assert.Equal("https://fcm.googleapis.com/v1/projects/kodosi-test/messages:send", blocked.Uri.ToString());
        Assert.All(new[] { blocked, done }, sent => Assert.Equal("Bearer google-access", sent.Headers["Authorization"]));
        Assert.Equal(
            """{"message":{"token":"fcm:token-1","data":{"terminal":"0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d","agent":"claude","state":"blocked","kind":"permission"},"android":{"priority":"HIGH","collapse_key":"0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d","ttl":"3600s"}}}""",
            blocked.Body);
        Assert.Equal(
            """{"message":{"token":"fcm:token-1","data":{"terminal":"0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d","state":"done"},"android":{"priority":"NORMAL","collapse_key":"0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d","ttl":"3600s"}}}""",
            done.Body);

        clock.Advance(TimeSpan.FromSeconds(3599) - TimeSpan.FromMinutes(5));
        await fcm.SendAsync(new PushToken { Token = "fcm:token-1" }, Notice(new TerminalStatus(State: "error")), ct);
        Assert.Equal(2, google.Sent.Count(x => x.Uri.Host == "oauth2.googleapis.com"));
    }

    [Theory]
    [InlineData(HttpStatusCode.NotFound, """{"error":{"code":404,"message":"Requested entity was not found.","status":"NOT_FOUND","details":[{"@type":"type.googleapis.com/google.firebase.fcm.v1.FcmError","errorCode":"UNREGISTERED"}]}}""", "Gone")]
    [InlineData(HttpStatusCode.BadRequest, """{"error":{"code":400,"message":"The registration token is not a valid FCM registration token","status":"INVALID_ARGUMENT","details":[{"@type":"type.googleapis.com/google.firebase.fcm.v1.FcmError","errorCode":"INVALID_ARGUMENT"}]}}""", "Gone")]
    [InlineData(HttpStatusCode.BadRequest, """{"error":{"code":400,"message":"Invalid value","status":"INVALID_ARGUMENT","details":[{"@type":"type.googleapis.com/google.rpc.BadRequest","fieldViolations":[{"field":"message.token","description":"Invalid registration token"}]}]}}""", "Gone")]
    [InlineData(HttpStatusCode.BadRequest, """{"error":{"code":400,"message":"Invalid value at 'message.data[0].value'","status":"INVALID_ARGUMENT","details":[{"@type":"type.googleapis.com/google.rpc.BadRequest","fieldViolations":[{"field":"message.data[0].value"}]}]}}""", "Refused")]
    [InlineData(HttpStatusCode.ServiceUnavailable, """{"error":{"code":503,"status":"UNAVAILABLE"}}""", "Retry")]
    public async Task GoogleSaysWhichTokensAreGone(HttpStatusCode status, string error, string outcome)
    {
        var google = new PushHandler(request => request.Uri.Host == "oauth2.googleapis.com"
            ? (HttpStatusCode.OK, """{"access_token":"google-access","expires_in":3599}""")
            : (status, error));
        var fcm = new Fcm(PushKeys.Settings(apns: false).Fcm!, new PushClients(google), TimeProvider.System);
        var delivery = await fcm.SendAsync(new PushToken { Token = "fcm:token-1" }, Notice(new TerminalStatus("gemini", "done")),
            TestContext.Current.CancellationToken);
        Assert.Equal(outcome, delivery.Outcome.ToString());
    }

    [Fact]
    public void PushStaysOffWithoutSettingsAndRefusesHalfASetting()
    {
        static PushSettings Read(Dictionary<string, string?> values) =>
            PushSettings.From(new ConfigurationBuilder().AddInMemoryCollection(values).Build());
        Assert.False(Read(new()).Sends);
        Assert.True(Read(PushKeys.Values(fcm: false)).Sends);
        foreach (var (key, value) in new Dictionary<string, string?>
                 {
                     ["Push:Apns:Topic"] = null, ["Push:Apns:PrivateKey"] = "not a key", ["Push:Apns:TeamId"] = "TEAM 1",
                     ["Push:Fcm:ServiceAccount"] = "{}", ["Push:Fcm:ProjectId"] = null,
                 })
        {
            var values = PushKeys.Values();
            values[key] = value;
            Assert.Throws<InvalidOperationException>(() => Read(values));
        }
    }
}

internal static class PushKeys
{
    public static readonly ECDsa Apple = ECDsa.Create(ECCurve.NamedCurves.nistP256);
    public static readonly RSA Google = RSA.Create(2048);

    public static Dictionary<string, string?> Values(bool apns = true, bool fcm = true)
    {
        var values = new Dictionary<string, string?>();
        if (apns)
        {
            values["Push:Apns:TeamId"] = "TEAM123456";
            values["Push:Apns:KeyId"] = "KEY1234567";
            values["Push:Apns:PrivateKey"] = Apple.ExportPkcs8PrivateKeyPem();
            values["Push:Apns:Topic"] = "com.kodosi.mobile";
        }
        if (fcm)
        {
            values["Push:Fcm:ProjectId"] = "kodosi-test";
            values["Push:Fcm:ServiceAccount"] = JsonSerializer.Serialize(new Dictionary<string, string>
            {
                ["type"] = "service_account",
                ["project_id"] = "kodosi-test",
                ["private_key_id"] = "key-1",
                ["private_key"] = Google.ExportPkcs8PrivateKeyPem(),
                ["client_email"] = "push@kodosi-test.iam.gserviceaccount.com",
                ["token_uri"] = "https://oauth2.googleapis.com/token",
            });
        }
        return values;
    }

    public static PushSettings Settings(bool apns = true, bool fcm = true) =>
        PushSettings.From(new ConfigurationBuilder().AddInMemoryCollection(Values(apns, fcm)).Build());
}

internal sealed record PushRequest(HttpMethod Method, Uri Uri, Version Version, Dictionary<string, string> Headers, string Body);

internal sealed class PushHandler(Func<PushRequest, (HttpStatusCode Status, string Body)> answer) : HttpMessageHandler
{
    private readonly Lock sync = new();
    private readonly List<PushRequest> sent = [];

    public IReadOnlyList<PushRequest> Sent
    {
        get { lock (sync) return sent.ToList(); }
    }

    protected override async Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
    {
        var body = request.Content is null ? "" : await request.Content.ReadAsStringAsync(cancellationToken);
        var headers = request.Headers.ToDictionary(x => x.Key, x => string.Join(",", x.Value), StringComparer.OrdinalIgnoreCase);
        var recorded = new PushRequest(request.Method, request.RequestUri!, request.Version, headers, body);
        lock (sync) sent.Add(recorded);
        var (status, content) = answer(recorded);
        return new HttpResponseMessage(status) { Content = new StringContent(content, Encoding.UTF8, "application/json") };
    }
}

internal sealed class PushClients(HttpMessageHandler handler) : IHttpClientFactory
{
    public HttpClient CreateClient(string name) => new(handler, disposeHandler: false);
}
