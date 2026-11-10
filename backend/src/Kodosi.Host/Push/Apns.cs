using System.Globalization;
using System.Net;
using System.Net.Http.Headers;
using System.Security.Cryptography;
using System.Text.Json;
using Kodosi.Data;
using Kodosi.TerminalConnections;

namespace Kodosi.Push;

internal sealed class Apns(ApnsSettings settings, IHttpClientFactory http, TimeProvider clock) : IPushService
{
    internal static readonly TimeSpan TokenLife = TimeSpan.FromMinutes(50);
    private static readonly TimeSpan Expiry = TimeSpan.FromHours(1);
    private readonly Lock sync = new();
    private (string Value, DateTimeOffset Made)? token;

    public async Task<Delivery> SendAsync(PushToken device, PushNotice notice, CancellationToken ct)
    {
        var host = device.Environment == "production" ? "api.push.apple.com" : "api.sandbox.push.apple.com";
        using var request = new HttpRequestMessage(HttpMethod.Post, new Uri($"https://{host}/3/device/{device.Token}"))
        {
            Version = HttpVersion.Version20,
            VersionPolicy = HttpVersionPolicy.RequestVersionExact,
            Content = PushJson.Content(Payload(notice)),
        };
        request.Headers.Authorization = new AuthenticationHeaderValue("bearer", Token());
        request.Headers.Add("apns-topic", settings.Topic);
        request.Headers.Add("apns-push-type", "alert");
        request.Headers.Add("apns-priority", notice.Status.State == "blocked" ? "10" : "5");
        request.Headers.Add("apns-collapse-id", notice.TerminalId.ToString());
        request.Headers.Add("apns-expiration", (clock.GetUtcNow() + Expiry).ToUnixTimeSeconds().ToString(CultureInfo.InvariantCulture));
        try
        {
            using var response = await http.CreateClient(nameof(Apns)).SendAsync(request, ct);
            if (response.IsSuccessStatusCode) return new(Delivered.Sent);
            var reason = await ReasonAsync(response, ct);
            var detail = $"{(int)response.StatusCode} {reason}";
            switch (response.StatusCode, reason)
            {
                case (HttpStatusCode.Gone, _) or (HttpStatusCode.BadRequest, "BadDeviceToken" or "DeviceTokenNotForTopic"):
                    return new(Delivered.Gone, detail);
                case (HttpStatusCode.Forbidden, "ExpiredProviderToken"):
                    lock (sync) token = null;
                    return new(Delivered.Retry, detail);
                case (HttpStatusCode.TooManyRequests or >= HttpStatusCode.InternalServerError, _):
                    return new(Delivered.Retry, detail);
                default:
                    return new(Delivered.Refused, detail);
            }
        }
        catch (Exception error) when (error is HttpRequestException or IOException || (error is TaskCanceledException && !ct.IsCancellationRequested))
        {
            return new(Delivered.Retry, error.GetType().Name);
        }
    }

    internal static byte[] Payload(PushNotice notice)
    {
        var status = notice.Status;
        var blocked = status.State == "blocked";
        var (key, agent) = Alert(status);
        return PushJson.Written(json =>
        {
            json.WriteStartObject();
            json.WriteStartObject("aps");
            json.WriteStartObject("alert");
            json.WriteString("loc-key", key);
            if (agent is not null)
            {
                json.WriteStartArray("loc-args");
                json.WriteStringValue(agent);
                json.WriteEndArray();
            }
            json.WriteEndObject();
            json.WriteString("sound", "default");
            json.WriteNumber("mutable-content", 1);
            json.WriteString("thread-id", notice.TerminalId);
            json.WriteString("interruption-level", blocked ? "time-sensitive" : "active");
            json.WritePropertyName("relevance-score");
            json.WriteRawValue(blocked ? "1.0" : "0.5");
            json.WriteString("category", "kodosi.terminal");
            json.WriteNumber("badge", notice.Blocked);
            json.WriteEndObject();
            json.WriteStartObject("kodosi");
            json.WriteString("terminal", notice.TerminalId);
            json.WriteString("agent", status.Program);
            json.WriteString("state", status.State);
            json.WriteString("kind", status.Kind);
            json.WriteEndObject();
            json.WriteEndObject();
        });
    }

    private static (string Key, string? Agent) Alert(TerminalStatus status)
    {
        var agent = status.Program switch
        {
            "claude" => "Claude Code",
            "codex" => "Codex",
            "copilot" => "Copilot CLI",
            "cursor" => "Cursor",
            "gemini" => "Gemini CLI",
            "opencode" => "OpenCode",
            _ => null,
        };
        var sentence = (status.State, status.Kind) switch
        {
            ("blocked", "permission") => "asks for permission",
            ("blocked", "question") => "asks you a question",
            ("blocked", "auth") => "needs you to sign in",
            ("blocked", _) => "waits for you",
            ("done", _) => "finished",
            _ => "failed",
        };
        return (agent is null ? $"A terminal {sentence}" : $"%@ {sentence}", agent);
    }

    private string Token()
    {
        lock (sync)
        {
            var now = clock.GetUtcNow();
            if (token is { } held && now - held.Made < TokenLife) return held.Value;
            var header = PushJson.Written(json =>
            {
                json.WriteStartObject();
                json.WriteString("alg", "ES256");
                json.WriteString("kid", settings.KeyId);
                json.WriteEndObject();
            });
            var claims = PushJson.Written(json =>
            {
                json.WriteStartObject();
                json.WriteString("iss", settings.TeamId);
                json.WriteNumber("iat", now.ToUnixTimeSeconds());
                json.WriteEndObject();
            });
            var value = PushJson.Signed(header, claims, bytes => settings.Key.SignData(bytes, HashAlgorithmName.SHA256));
            token = (value, now);
            return value;
        }
    }

    private static async Task<string> ReasonAsync(HttpResponseMessage response, CancellationToken ct)
    {
        try
        {
            using var json = JsonDocument.Parse(await response.Content.ReadAsByteArrayAsync(ct));
            return PushJson.Text(json.RootElement, "reason") ?? "";
        }
        catch (JsonException)
        {
            return "";
        }
    }
}
