using System.IdentityModel.Tokens.Jwt;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Net.WebSockets;
using System.Security.Claims;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Kodosi.TerminalConnections;
using Kodosi.Security;
using Microsoft.AspNetCore.Authentication.JwtBearer;
using Microsoft.AspNetCore.Hosting;
using Microsoft.AspNetCore.Mvc.Testing;
using Microsoft.AspNetCore.TestHost;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.IdentityModel.Protocols.OpenIdConnect;
using Microsoft.IdentityModel.Tokens;
using Xunit;

namespace Kodosi.HostTests;

internal sealed class BackendApplication(string connection, Action<IServiceCollection>? configure = null) : WebApplicationFactory<Program>
{
    private const string Issuer = "https://auth.kodosi.com/realms/kodosi";
    private readonly RSA key = RSA.Create(2048);
    protected override void ConfigureWebHost(IWebHostBuilder builder)
    {
        builder.ConfigureAppConfiguration((_, config) => config.AddInMemoryCollection(new Dictionary<string, string?>
        {
            ["ConnectionStrings:Kodosi"] = connection,
            ["Logging:LogLevel:Default"] = "Warning",
        }));
        builder.ConfigureTestServices(services => services.PostConfigure<JwtBearerOptions>(JwtBearerDefaults.AuthenticationScheme, options =>
        {
            var config = new OpenIdConnectConfiguration { Issuer = Issuer };
            config.SigningKeys.Add(new RsaSecurityKey(key));
            options.ConfigurationManager = new Microsoft.IdentityModel.Protocols.StaticConfigurationManager<OpenIdConnectConfiguration>(config);
        }));
        if (configure is not null) builder.ConfigureTestServices(configure);
    }

    protected override Microsoft.Extensions.Hosting.IHost CreateHost(Microsoft.Extensions.Hosting.IHostBuilder builder)
    {
        using (var db = PostgresFixture.Context(connection))
            Kodosi.Data.DatabaseSetup.MigrateAsync(db, CancellationToken.None).GetAwaiter().GetResult();
        return base.CreateHost(builder);
    }

    public string Token(string subject, string audience = "kodosi-app", DateTimeOffset? authenticatedAt = null, TimeSpan? lifetime = null)
    {
        List<Claim> claims = [new("sub", subject), new("preferred_username", subject)];
        if (authenticatedAt is { } at) claims.Add(new Claim("auth_time", at.ToUnixTimeSeconds().ToString(), ClaimValueTypes.Integer64));
        return new JwtSecurityTokenHandler().WriteToken(new JwtSecurityToken(Issuer, audience, claims,
            DateTime.UtcNow.AddMinutes(-1), DateTime.UtcNow.Add(lifetime ?? TimeSpan.FromMinutes(10)), new SigningCredentials(new RsaSecurityKey(key), SecurityAlgorithms.RsaSha256)));
    }

    public async Task<ApiDevice> EnrollAsync(string subject)
    {
        var client = CreateClient(); var token = Token(subject);
        client.DefaultRequestHeaders.Authorization = new AuthenticationHeaderValue("Bearer", token);
        var me = await client.GetFromJsonAsync<JsonElement>("/api/me", TestContext.Current.CancellationToken);
        var user = me.GetProperty("id").GetGuid();
        var device = new DeviceFixture(user, $"{subject}-device");
        var challenge = await ChallengeAsync(client, "/api/me/devices/challenge");
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var cert = device.CertificateBody(device.DeviceId, now);
        var list = DeviceFixture.ListBody(user, 1, [(device.DeviceId, device.DeviceId)], device.DeviceId, now);
        using var enrolled = await client.PostAsJsonAsync("/api/me/devices", new
        {
            deviceId = device.DeviceId,
            signingPublicKey = Convert.ToBase64String(device.SigningKey),
            challengeId = challenge.Id,
            popSignature = Convert.ToBase64String(device.Sign(Proofs.Tagged(DomainTags.DevicePopV1, challenge.Bytes))),
            deviceCertificate = Convert.ToBase64String(cert),
            deviceCertificateSignature = device.SignedCertificate(cert),
            signedDeviceList = Convert.ToBase64String(list),
            signedDeviceListSignature = device.SignedList(list),
        }, TestContext.Current.CancellationToken);
        await SuccessAsync(enrolled);
        return new ApiDevice(client, token, device, await OpenSessionAsync(client, device));
    }

    public static async Task<string> OpenSessionAsync(HttpClient client, DeviceFixture device)
    {
        var challenge = await ChallengeAsync(client, "/api/me/devices/challenge");
        using var opened = await client.PostAsJsonAsync("/api/me/device-sessions", new
        {
            deviceId = device.DeviceId,
            challengeId = challenge.Id,
            signature = Convert.ToBase64String(device.Sign(Proofs.DeviceSession(device.UserId, device.DeviceId, challenge.Id, challenge.Bytes))),
        }, TestContext.Current.CancellationToken);
        await SuccessAsync(opened);
        return (await opened.Content.ReadFromJsonAsync<JsonElement>(TestContext.Current.CancellationToken)).GetProperty("session").GetString()!;
    }

    private static async Task<(Guid Id, byte[] Bytes)> ChallengeAsync(HttpClient client, string path)
    {
        using var response = await client.PostAsync(path, null, TestContext.Current.CancellationToken); await SuccessAsync(response);
        var json = await response.Content.ReadFromJsonAsync<JsonElement>(TestContext.Current.CancellationToken);
        return (json.GetProperty("challengeId").GetGuid(), Convert.FromBase64String(json.GetProperty("challengeBytes").GetString()!));
    }

    public static Task<HttpRequestMessage> SignedAsync(ApiDevice actor, HttpMethod method, string path, object? body = null)
    {
        var request = new HttpRequestMessage(method, path);
        if (body is not null)
        {
            request.Content = new ByteArrayContent(JsonSerializer.SerializeToUtf8Bytes(body, Wire.Json));
            request.Content.Headers.ContentType = new MediaTypeHeaderValue("application/json");
        }
        request.Headers.Add("X-Kodosi-Device-Id", actor.Fixture.DeviceId);
        request.Headers.Add("X-Kodosi-Device-Session", actor.Session);
        return Task.FromResult(request);
    }

    public static async Task<JsonElement> CallAsync(ApiDevice actor, HttpMethod method, string path, object? body = null)
    {
        using var request = await SignedAsync(actor, method, path, body);
        using var response = await actor.Client.SendAsync(request, TestContext.Current.CancellationToken); await SuccessAsync(response);
        return response.Content.Headers.ContentLength == 0 || response.StatusCode == System.Net.HttpStatusCode.NoContent
            ? default : await response.Content.ReadFromJsonAsync<JsonElement>(TestContext.Current.CancellationToken);
    }

    public async Task<WebSocket> ConnectAsync(ApiDevice actor)
    {
        var client = Server.CreateWebSocketClient(); client.ConfigureRequest = request => request.Headers.Authorization = "Bearer " + actor.Token;
        var socket = await client.ConnectAsync(new Uri("ws://localhost/ws/device"), TestContext.Current.CancellationToken);
        await SendAsync(socket, new { type = "hello", protocolVersion = 18, deviceId = actor.Fixture.DeviceId, deviceSession = actor.Session });
        Assert.Equal("ready", (await JsonAsync(socket)).GetProperty("type").GetString());
        return socket;
    }

    public static byte[] PipeFrame(Guid pipe, byte[] payload) => [.. pipe.ToByteArray(bigEndian: true), .. payload];

    public static async Task SuccessAsync(HttpResponseMessage response) => Assert.True(response.IsSuccessStatusCode,
        $"{response.StatusCode}: {await response.Content.ReadAsStringAsync(TestContext.Current.CancellationToken)}");
    public static Task SendAsync(WebSocket socket, object body) => socket.SendAsync(new ArraySegment<byte>(Wire.Encode(body)), WebSocketMessageType.Text, true, TestContext.Current.CancellationToken);
    public static async Task<(WebSocketMessageType Type, byte[] Bytes)> FrameAsync(WebSocket socket)
    {
        using var deadline = CancellationTokenSource.CreateLinkedTokenSource(TestContext.Current.CancellationToken); deadline.CancelAfter(TimeSpan.FromSeconds(10));
        return await Wire.ReadAsync(socket, 512 * 1024, deadline.Token) ?? throw new InvalidOperationException("Socket closed before expected frame.");
    }
    public static async Task<JsonElement> JsonAsync(WebSocket socket)
    {
        while (true)
        {
            var frame = await FrameAsync(socket); Assert.Equal(WebSocketMessageType.Text, frame.Type);
            using var json = JsonDocument.Parse(frame.Bytes);
            if (json.RootElement.GetProperty("type").GetString() == "ping") { await SendAsync(socket, new { type = "pong" }); continue; }
            return json.RootElement.Clone();
        }
    }
    public override async ValueTask DisposeAsync() { await base.DisposeAsync(); key.Dispose(); }
}

internal sealed record ApiDevice(HttpClient Client, string Token, DeviceFixture Fixture, string Session) : IDisposable
{
    public void Dispose() => Client.Dispose();
}
