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
    private static readonly Guid OtherId = Guid.Parse("0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0e");
    private static readonly PushedTerminal Other = new(OtherId, new TerminalStatus(State: "done", Attention: true));
    private static readonly string AppleToken = new('a', 64);

    private static PushNotice Notice(TerminalStatus status, bool sound = false, int waits = 0) =>
        new(Guid.CreateVersion7(), [new PushedTerminal(TerminalId, status), Other], new PushAlert(TerminalId, status, sound, waits));

    private static string Part(string jwt, int index) => Encoding.UTF8.GetString(System.Buffers.Text.Base64Url.DecodeFromChars(jwt.Split('.')[index]));

    private static bool Verify(string jwt, Func<byte[], byte[], bool> verify)
    {
        var parts = jwt.Split('.');
        return verify(Encoding.ASCII.GetBytes($"{parts[0]}.{parts[1]}"), System.Buffers.Text.Base64Url.DecodeFromChars(parts[2]));
    }

    [Fact]
    public async Task AnApplePushCarriesOnlyIdentifiersASentenceKeyAndTheListOfPushesWithAReusedProviderToken()
    {
        var ct = TestContext.Current.CancellationToken;
        var clock = new ManualClock();
        var apple = new PushHandler(_ => (HttpStatusCode.OK, ""));
        var apns = new Apns(PushKeys.Settings(fcm: false).Apns!, new PushClients(apple), clock);
        var now = clock.GetUtcNow().ToUnixTimeSeconds();
        var production = new PushToken { Token = AppleToken, Environment = "production" };

        foreach (var notice in new[]
                 {
                     Notice(new TerminalStatus("claude", "blocked", "permission", 2, true), sound: true, waits: 2),
                     Notice(new TerminalStatus("aider", "done", Attention: true)),
                     Notice(new TerminalStatus("codex", "error", Attention: true), waits: 1),
                     new PushNotice(Guid.CreateVersion7(), [Other]),
                 })
            Assert.Equal(Delivered.Sent, (await apns.SendAsync(production, notice, ct)).Outcome);
        Assert.Equal(Delivered.Sent, (await apns.SendAsync(new PushToken { Token = AppleToken, Environment = "development" },
            Notice(new TerminalStatus(State: "done", Attention: true)), ct)).Outcome);

        var (blocked, done, failed, background, development) = (apple.Sent[0], apple.Sent[1], apple.Sent[2], apple.Sent[3], apple.Sent[4]);
        Assert.Equal($"https://api.push.apple.com/3/device/{AppleToken}", blocked.Uri.ToString());
        Assert.Equal($"https://api.sandbox.push.apple.com/3/device/{AppleToken}", development.Uri.ToString());
        Assert.All(apple.Sent, sent => Assert.Equal((HttpMethod.Post, HttpVersion.Version20, "com.kodosi.mobile"),
            (sent.Method, sent.Version, sent.Headers["apns-topic"])));
        foreach (var (alert, priority) in new[] { (blocked, "10"), (done, "5"), (failed, "5") })
        {
            Assert.Equal(("alert", priority, TerminalId.ToString()), (alert.Headers["apns-push-type"], alert.Headers["apns-priority"], alert.Headers["apns-collapse-id"]));
            Assert.Equal((now + 3600).ToString(System.Globalization.CultureInfo.InvariantCulture), alert.Headers["apns-expiration"]);
        }
        Assert.Equal(("background", "5", "0"), (background.Headers["apns-push-type"], background.Headers["apns-priority"], background.Headers["apns-expiration"]));
        Assert.False(background.Headers.ContainsKey("apns-collapse-id"));
        Assert.Equal(
            """{"aps":{"alert":{"loc-key":"%@ asks for permission","loc-args":["Claude Code"]},"sound":"default","mutable-content":1,"interruption-level":"active","relevance-score":1.0,"badge":2},"kodosi":{"terminal":"0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d","agent":"claude","state":"blocked","kind":"permission","pushed":["0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d","0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0e"]}}""",
            blocked.Body);
        Assert.Equal(
            """{"aps":{"alert":{"loc-key":"A terminal finished"},"mutable-content":1,"interruption-level":"passive","relevance-score":0.3},"kodosi":{"terminal":"0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d","agent":"aider","state":"done","kind":null,"pushed":["0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d","0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0e"]}}""",
            done.Body);
        Assert.Equal(
            """{"aps":{"alert":{"loc-key":"%@ failed","loc-args":["Codex"]},"mutable-content":1,"interruption-level":"passive","relevance-score":0.6,"badge":1},"kodosi":{"terminal":"0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d","agent":"codex","state":"error","kind":null,"pushed":["0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d","0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0e"]}}""",
            failed.Body);
        Assert.Equal("""{"aps":{"content-available":1},"kodosi":{"pushed":["0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0e"]}}""", background.Body);

        var bearer = blocked.Headers["Authorization"];
        Assert.StartsWith("bearer ", bearer, StringComparison.Ordinal);
        var jwt = bearer["bearer ".Length..];
        Assert.Equal("""{"alg":"ES256","kid":"KEY1234567"}""", Part(jwt, 0));
        Assert.Equal($$"""{"iss":"TEAM123456","iat":{{now}}}""", Part(jwt, 1));
        Assert.True(Verify(jwt, (data, signature) => PushKeys.Apple.VerifyData(data, signature, HashAlgorithmName.SHA256)));
        Assert.All(apple.Sent, sent => Assert.Equal(bearer, sent.Headers["Authorization"]));
        clock.Advance(Apns.TokenLife);
        await apns.SendAsync(production, Notice(new TerminalStatus(State: "error", Attention: true)), ct);
        Assert.NotEqual(bearer, apple.Sent[5].Headers["Authorization"]);
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
            Notice(new TerminalStatus("codex", "error", Attention: true)), TestContext.Current.CancellationToken);
        Assert.Equal(outcome, delivery.Outcome.ToString());
    }

    [Fact]
    public async Task AGooglePushIsADataMessageOfTheListOfPushesSentWithAnAccessTokenOfTheServiceAccount()
    {
        var ct = TestContext.Current.CancellationToken;
        var clock = new ManualClock();
        var google = new PushHandler(request => request.Uri.Host == "oauth2.googleapis.com"
            ? (HttpStatusCode.OK, """{"access_token":"google-access","expires_in":3599,"token_type":"Bearer"}""")
            : (HttpStatusCode.OK, """{"name":"projects/kodosi-test/messages/1"}"""));
        var fcm = new Fcm(PushKeys.Settings(apns: false).Fcm!, new PushClients(google), clock);
        var now = clock.GetUtcNow().ToUnixTimeSeconds();
        var device = new PushToken { Token = "fcm:token-1" };

        foreach (var notice in new[]
                 {
                     Notice(new TerminalStatus("claude", "blocked", "permission", 2, true), sound: true),
                     Notice(new TerminalStatus("claude", "blocked", Attention: true)),
                     Notice(new TerminalStatus(State: "error", Attention: true)),
                     new PushNotice(Guid.CreateVersion7(), []),
                 })
            Assert.Equal(Delivered.Sent, (await fcm.SendAsync(device, notice, ct)).Outcome);

        var grant = Assert.Single(google.Sent, x => x.Uri.Host == "oauth2.googleapis.com");
        var form = System.Web.HttpUtility.ParseQueryString(grant.Body);
        Assert.Equal((HttpMethod.Post, "https://oauth2.googleapis.com/token"), (grant.Method, grant.Uri.ToString()));
        Assert.Equal("urn:ietf:params:oauth:grant-type:jwt-bearer", form["grant_type"]);
        var assertion = form["assertion"]!;
        Assert.Equal("""{"alg":"RS256","typ":"JWT","kid":"key-1"}""", Part(assertion, 0));
        Assert.Equal(
            $$"""{"iss":"push@kodosi-test.iam.gserviceaccount.com","scope":"https://www.googleapis.com/auth/firebase.messaging","aud":"https://oauth2.googleapis.com/token","iat":{{now}},"exp":{{now + 3600}}}""",
            Part(assertion, 1));
        Assert.True(Verify(assertion, (data, signature) => PushKeys.Google.VerifyData(data, signature, HashAlgorithmName.SHA256, RSASignaturePadding.Pkcs1)));

        var messages = google.Sent.Where(x => x.Uri.Host == "fcm.googleapis.com").ToList();
        Assert.All(messages, sent => Assert.Equal(("https://fcm.googleapis.com/v1/projects/kodosi-test/messages:send", "Bearer google-access"),
            (sent.Uri.ToString(), sent.Headers["Authorization"])));
        Assert.Equal(
            """{"message":{"token":"fcm:token-1","data":{"pushed":"0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d:claude:blocked:permission:1 0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0e::done::0"},"android":{"priority":"HIGH","collapse_key":"pushed","ttl":"3600s"}}}""",
            messages[0].Body);
        Assert.Equal(
            """{"message":{"token":"fcm:token-1","data":{"pushed":"0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d:claude:blocked::0 0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0e::done::0"},"android":{"priority":"HIGH","collapse_key":"pushed","ttl":"3600s"}}}""",
            messages[1].Body);
        Assert.Equal(
            """{"message":{"token":"fcm:token-1","data":{"pushed":"0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0d::error::0 0196d0a1-8f3e-7c2a-9b4d-5e6f7a8b9c0e::done::0"},"android":{"priority":"NORMAL","collapse_key":"pushed","ttl":"3600s"}}}""",
            messages[2].Body);
        Assert.Equal(
            """{"message":{"token":"fcm:token-1","data":{"pushed":""},"android":{"priority":"NORMAL","collapse_key":"pushed","ttl":"3600s"}}}""",
            messages[3].Body);

        clock.Advance(TimeSpan.FromSeconds(3599) - TimeSpan.FromMinutes(5));
        await fcm.SendAsync(device, Notice(new TerminalStatus(State: "error", Attention: true)), ct);
        Assert.Equal(2, google.Sent.Count(x => x.Uri.Host == "oauth2.googleapis.com"));
    }

    [Fact]
    public void TheLongestListOfAGooglePushStaysWithinItsDataLimit()
    {
        var widest = new TerminalStatus(new string('a', 32), "blocked", "permission", 255, true);
        var pushed = Enumerable.Range(0, TerminalAlerts.MostListed).Select(_ => new PushedTerminal(Guid.CreateVersion7(), widest)).ToList();
        using var json = JsonDocument.Parse(Fcm.Payload("fcm:token-1", new PushNotice(Guid.CreateVersion7(), pushed, new PushAlert(pushed[0].TerminalId, widest, true, 32))));
        var data = json.RootElement.GetProperty("message").GetProperty("data");
        Assert.Equal(TerminalAlerts.MostListed, data.GetProperty("pushed").GetString()!.Split(' ').Length);
        Assert.InRange(data.EnumerateObject().Sum(x => Encoding.UTF8.GetByteCount(x.Name) + Encoding.UTF8.GetByteCount(x.Value.GetString()!)), 1, 4095);
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
        var delivery = await fcm.SendAsync(new PushToken { Token = "fcm:token-1" }, Notice(new TerminalStatus("gemini", "done", Attention: true)),
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
