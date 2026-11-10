using System.Net;
using System.Net.Http.Headers;
using System.Security.Cryptography;
using System.Text.Json;
using Kodosi.Data;

namespace Kodosi.Push;

internal sealed class Fcm(FcmSettings settings, IHttpClientFactory http, TimeProvider clock) : IPushService
{
    private const string Scope = "https://www.googleapis.com/auth/firebase.messaging";
    private static readonly TimeSpan AssertionLife = TimeSpan.FromHours(1);
    private static readonly TimeSpan Margin = TimeSpan.FromMinutes(5);
    private readonly Uri messages = new($"https://fcm.googleapis.com/v1/projects/{settings.ProjectId}/messages:send");
    private readonly SemaphoreSlim gate = new(1, 1);
    private Access? access;

    public async Task<Delivery> SendAsync(PushToken device, PushNotice notice, CancellationToken ct)
    {
        try
        {
            var client = http.CreateClient(nameof(Fcm));
            using var request = new HttpRequestMessage(HttpMethod.Post, messages) { Content = PushJson.Content(Payload(device.Token, notice)) };
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", await AccessAsync(client, ct));
            using var response = await client.SendAsync(request, ct);
            if (response.IsSuccessStatusCode) return new(Delivered.Sent);
            var (code, aboutToken) = await ErrorAsync(response, ct);
            var detail = $"{(int)response.StatusCode} {code}";
            switch (response.StatusCode)
            {
                case HttpStatusCode.NotFound when code == "UNREGISTERED":
                case HttpStatusCode.BadRequest when aboutToken:
                    return new(Delivered.Gone, detail);
                case HttpStatusCode.Unauthorized:
                    access = null;
                    return new(Delivered.Retry, detail);
                case HttpStatusCode.TooManyRequests or >= HttpStatusCode.InternalServerError:
                    return new(Delivered.Retry, detail);
                default:
                    return new(Delivered.Refused, detail);
            }
        }
        catch (HttpRequestException error) when (error.StatusCode is { } status && status != HttpStatusCode.TooManyRequests && (int)status < 500)
        {
            return new(Delivered.Refused, $"the access token: {(int)status}");
        }
        catch (Exception error) when (error is HttpRequestException or IOException or JsonException
                                      || (error is TaskCanceledException && !ct.IsCancellationRequested))
        {
            return new(Delivered.Retry, error.GetType().Name);
        }
    }

    internal static byte[] Payload(string token, PushNotice notice)
    {
        var status = notice.Status;
        return PushJson.Written(json =>
        {
            json.WriteStartObject();
            json.WriteStartObject("message");
            json.WriteString("token", token);
            json.WriteStartObject("data");
            json.WriteString("terminal", notice.TerminalId);
            if (status.Program is { } agent) json.WriteString("agent", agent);
            json.WriteString("state", status.State);
            if (status.Kind is { } kind) json.WriteString("kind", kind);
            json.WriteEndObject();
            json.WriteStartObject("android");
            json.WriteString("priority", status.State == "blocked" ? "HIGH" : "NORMAL");
            json.WriteString("collapse_key", notice.TerminalId);
            json.WriteString("ttl", "3600s");
            json.WriteEndObject();
            json.WriteEndObject();
            json.WriteEndObject();
        });
    }

    private async Task<string> AccessAsync(HttpClient client, CancellationToken ct)
    {
        await gate.WaitAsync(ct);
        try
        {
            var now = clock.GetUtcNow();
            if (access is { } held && held.Until > now) return held.Value;
            using var form = new FormUrlEncodedContent(new Dictionary<string, string>
            {
                ["grant_type"] = "urn:ietf:params:oauth:grant-type:jwt-bearer",
                ["assertion"] = Assertion(now),
            });
            using var response = await client.PostAsync(settings.TokenUri, form, ct);
            if (!response.IsSuccessStatusCode) throw new HttpRequestException("Google gave no access token.", null, response.StatusCode);
            using var answer = JsonDocument.Parse(await response.Content.ReadAsByteArrayAsync(ct));
            var value = PushJson.Text(answer.RootElement, "access_token") ?? throw new JsonException("Google gave no access token.");
            var lifetime = answer.RootElement.TryGetProperty("expires_in", out var seconds) && seconds.TryGetInt32(out var s) ? s : 3600;
            access = new Access(value, now + TimeSpan.FromSeconds(lifetime) - Margin);
            return value;
        }
        finally
        {
            gate.Release();
        }
    }

    private string Assertion(DateTimeOffset now)
    {
        var header = PushJson.Written(json =>
        {
            json.WriteStartObject();
            json.WriteString("alg", "RS256");
            json.WriteString("typ", "JWT");
            if (settings.KeyId is { } keyId) json.WriteString("kid", keyId);
            json.WriteEndObject();
        });
        var claims = PushJson.Written(json =>
        {
            json.WriteStartObject();
            json.WriteString("iss", settings.ClientEmail);
            json.WriteString("scope", Scope);
            json.WriteString("aud", settings.TokenUri.AbsoluteUri);
            json.WriteNumber("iat", now.ToUnixTimeSeconds());
            json.WriteNumber("exp", (now + AssertionLife).ToUnixTimeSeconds());
            json.WriteEndObject();
        });
        return PushJson.Signed(header, claims, bytes => settings.Key.SignData(bytes, HashAlgorithmName.SHA256, RSASignaturePadding.Pkcs1));
    }

    private static async Task<(string Code, bool AboutToken)> ErrorAsync(HttpResponseMessage response, CancellationToken ct)
    {
        try
        {
            using var json = JsonDocument.Parse(await response.Content.ReadAsByteArrayAsync(ct));
            if (!json.RootElement.TryGetProperty("error", out var error) || error.ValueKind != JsonValueKind.Object) return ("", false);
            var code = PushJson.Text(error, "status") ?? "";
            var field = false;
            if (error.TryGetProperty("details", out var details) && details.ValueKind == JsonValueKind.Array)
                foreach (var detail in details.EnumerateArray())
                {
                    if (PushJson.Text(detail, "errorCode") is { } fcmCode) code = fcmCode;
                    if (detail.ValueKind == JsonValueKind.Object && detail.TryGetProperty("fieldViolations", out var violations)
                        && violations.ValueKind == JsonValueKind.Array)
                        field |= violations.EnumerateArray().Any(violation => PushJson.Text(violation, "field") == "message.token");
                }
            var aboutToken = code == "INVALID_ARGUMENT"
                && (field || PushJson.Text(error, "message")?.Contains("registration token", StringComparison.OrdinalIgnoreCase) == true);
            return (code, aboutToken);
        }
        catch (JsonException)
        {
            return ("", false);
        }
    }

    private sealed record Access(string Value, DateTimeOffset Until);
}
