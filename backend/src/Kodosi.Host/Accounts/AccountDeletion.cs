using System.Globalization;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Security.Cryptography;
using System.Text.Json;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Accounts;

public sealed class AccountDeletionSettings
{
    private AccountDeletionSettings(string issuer, Uri? admin, string? realm, string? clientId, string? clientSecret)
    {
        Issuer = issuer; Admin = admin; Realm = realm; ClientId = clientId; ClientSecret = clientSecret;
    }

    public string Issuer { get; }
    public Uri? Admin { get; }
    public string? Realm { get; }
    public string? ClientId { get; }
    public string? ClientSecret { get; }
    public bool ReadsSignInService => ClientId is not null;

    public static AccountDeletionSettings From(IConfiguration config)
    {
        var issuer = (config["Auth:Authority"] ?? "").Trim().TrimEnd('/');
        var section = config.GetSection("Auth:AccountDeletion");
        var clientId = section["ClientId"]?.Trim();
        if (string.IsNullOrEmpty(clientId)) return new(issuer, null, null, null, null);
        var secret = section["ClientSecret"]?.Trim();
        var issuerUri = new Uri(issuer);
        var segments = issuerUri.AbsolutePath.Trim('/').Split('/');
        if (string.IsNullOrEmpty(secret) || segments is not ["realms", { Length: > 0 } realm])
            throw new InvalidOperationException("Auth:AccountDeletion needs a ClientSecret and a Keycloak realm issuer (…/realms/<name>).");
        var admin = Uri.TryCreate(section["AdminUrl"] ?? issuerUri.GetLeftPart(UriPartial.Authority), UriKind.Absolute, out var parsed) ? parsed : null;
        if (admin is null || admin.Query.Length > 0 || !(admin.Scheme == "https" || admin.Scheme == "http" && admin.IsLoopback))
            throw new InvalidOperationException("Auth:AccountDeletion:AdminUrl must be HTTPS, or loopback HTTP.");
        return new(issuer, new Uri(admin.GetLeftPart(UriPartial.Path).TrimEnd('/') + "/"), realm, clientId, secret);
    }

    // Keycloak's delete_account action through its own account client. Nobody redeems the code.
    public string? ConfirmationUri()
    {
        if (!ReadsSignInService) return null;
        var challenge = System.Buffers.Text.Base64Url.EncodeToString(SHA256.HashData(RandomNumberGenerator.GetBytes(32)));
        var query = string.Join('&',
            "client_id=account-console",
            "redirect_uri=" + Uri.EscapeDataString(Issuer + "/account/"),
            "response_type=code",
            "scope=openid",
            "kc_action=delete_account",
            "code_challenge=" + challenge,
            "code_challenge_method=S256");
        return $"{Issuer}/protocol/openid-connect/auth?{query}";
    }
}

public sealed class AccountDeletionFeed(
    IServiceScopeFactory scopes, IHttpClientFactory http, AccountDeletionSettings settings, TimeProvider clock, ILogger<AccountDeletionFeed> logger) : BackgroundService
{
    internal static readonly TimeSpan Period = TimeSpan.FromSeconds(15);
    private const int Page = 200;
    private readonly Dictionary<string, DateTimeOffset> handled = [];
    private DateTimeOffset? lastPass;
    private (string Value, DateTimeOffset Until)? token;

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        if (!settings.ReadsSignInService) return;
        using var timer = new PeriodicTimer(Period, clock);
        try
        {
            do
            {
                try { await PassAsync(stoppingToken); }
                catch (Exception error) when (error is HttpRequestException or JsonException or DbUpdateException or Npgsql.NpgsqlException
                                              or TaskCanceledException { InnerException: TimeoutException })
                { logger.LogWarning(error, "Account deletions from the sign-in service will be read again on the next pass."); }
            }
            while (await timer.WaitForNextTickAsync(stoppingToken));
        }
        catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested) { }
    }

    internal async Task PassAsync(CancellationToken ct)
    {
        var started = clock.GetUtcNow();
        var from = (lastPass ?? started - AccountService.DeletionMemory) - TimeSpan.FromDays(1);
        var day = from.UtcDateTime.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture);
        var client = http.CreateClient(nameof(AccountDeletionFeed));
        var deletions = new List<(string Id, string Subject, DateTimeOffset At)>();
        await foreach (var item in ReadAsync(client, $"events?type=DELETE_ACCOUNT&dateFrom={day}", ct))
            if (Text(item, "id") is { } id && Text(item, "userId") is { } subject && Time(item) is { } at)
                deletions.Add((id, subject, at));
        await foreach (var item in ReadAsync(client, $"admin-events?operationTypes=DELETE&resourceTypes=USER&dateFrom={day}", ct))
            if (Text(item, "resourcePath")?.Split('/') is ["users", { Length: > 0 } subject] && Time(item) is { } at)
                deletions.Add((Text(item, "id") ?? $"admin:{subject}", subject, at));
        foreach (var (id, subject, at) in deletions)
        {
            if (handled.ContainsKey(id)) continue;
            await using var scope = scopes.CreateAsyncScope();
            await scope.ServiceProvider.GetRequiredService<AccountService>().ForgetAsync(settings.Issuer, subject, at, ct);
            handled[id] = at;
        }
        foreach (var old in handled.Where(x => x.Value < from).Select(x => x.Key).ToList()) handled.Remove(old);
        lastPass = started;
    }

    private async IAsyncEnumerable<JsonElement> ReadAsync(HttpClient client, string resource, [System.Runtime.CompilerServices.EnumeratorCancellation] CancellationToken ct)
    {
        for (var first = 0; ; first += Page)
        {
            using var request = new HttpRequestMessage(HttpMethod.Get, new Uri(settings.Admin!, $"admin/realms/{settings.Realm}/{resource}&first={first}&max={Page}"));
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", await TokenAsync(client, ct));
            using var response = await client.SendAsync(request, ct);
            if (response.StatusCode is System.Net.HttpStatusCode.Unauthorized or System.Net.HttpStatusCode.Forbidden) token = null;
            response.EnsureSuccessStatusCode();
            var items = await response.Content.ReadFromJsonAsync<JsonElement>(ct);
            if (items.ValueKind != JsonValueKind.Array) throw new JsonException("The sign-in service did not answer with a list of events.");
            foreach (var item in items.EnumerateArray()) yield return item;
            if (items.GetArrayLength() < Page) yield break;
        }
    }

    private async Task<string> TokenAsync(HttpClient client, CancellationToken ct)
    {
        if (token is { } held && held.Until > clock.GetUtcNow()) return held.Value;
        using var body = new FormUrlEncodedContent(new Dictionary<string, string>
        {
            ["grant_type"] = "client_credentials",
            ["client_id"] = settings.ClientId!,
            ["client_secret"] = settings.ClientSecret!,
        });
        using var response = await client.PostAsync(new Uri(settings.Admin!, $"realms/{settings.Realm}/protocol/openid-connect/token"), body, ct);
        response.EnsureSuccessStatusCode();
        var answer = await response.Content.ReadFromJsonAsync<JsonElement>(ct);
        var value = answer.TryGetProperty("access_token", out var issued) && issued.ValueKind == JsonValueKind.String
            && issued.GetString() is { Length: > 0 } text ? text : throw new JsonException("The sign-in service gave no access token.");
        var lifetime = answer.TryGetProperty("expires_in", out var seconds) && seconds.TryGetInt32(out var s) ? s : 60;
        token = (value, clock.GetUtcNow().AddSeconds(Math.Max(lifetime - 30, 5)));
        return value;
    }

    private static string? Text(JsonElement item, string name) =>
        item.ValueKind == JsonValueKind.Object && item.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.String
            && value.GetString() is { Length: > 0 and <= 512 } text ? text : null;

    private static DateTimeOffset? Time(JsonElement item) =>
        item.ValueKind == JsonValueKind.Object && item.TryGetProperty("time", out var value) && value.TryGetInt64(out var ms) ? DateTimeOffset.FromUnixTimeMilliseconds(ms) : null;
}
