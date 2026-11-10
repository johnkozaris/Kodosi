using System.Buffers;
using System.Buffers.Text;
using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
using Kodosi.Data;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Push;

internal enum Delivered { Sent, Gone, Retry, Refused }

internal readonly record struct Delivery(Delivered Outcome, string Detail = "");

internal interface IPushService
{
    Task<Delivery> SendAsync(PushToken token, PushNotice notice, CancellationToken ct);
}

internal sealed partial class PushDelivery(IServiceScopeFactory scopes, PushSettings settings, IHttpClientFactory http, TimeProvider clock,
    ILogger<PushDelivery> logger)
{
    internal const int Attempts = 3;
    private readonly Dictionary<string, IPushService> services = Services(settings, http, clock);

    public async Task DeliverAsync(PushNotice notice, CancellationToken ct)
    {
        List<PushToken> tokens;
        await using (var scope = scopes.CreateAsyncScope())
        {
            var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            tokens = await (from token in db.PushTokens.AsNoTracking()
                            join device in db.Devices on token.DeviceId equals device.Id
                            where token.UserId == notice.OwnerId && !device.Revoked
                            select token).ToListAsync(ct);
        }
        tokens.RemoveAll(token => !services.ContainsKey(token.Platform));
        var outcomes = await Task.WhenAll(tokens.Select(token => SendAsync(token, notice, ct)));
        var gone = tokens.Where((_, index) => outcomes[index] == Delivered.Gone).ToList();
        if (gone.Count == 0) return;
        await using var cleanup = scopes.CreateAsyncScope();
        var kept = cleanup.ServiceProvider.GetRequiredService<KodosiDbContext>().PushTokens;
        foreach (var token in gone)
            await kept.Where(x => x.DeviceId == token.DeviceId && x.Token == token.Token).ExecuteDeleteAsync(ct);
    }

    private async Task<Delivered> SendAsync(PushToken token, PushNotice notice, CancellationToken ct)
    {
        for (var attempt = 1; ; attempt++)
        {
            var delivery = await services[token.Platform].SendAsync(token, notice, ct);
            if (delivery.Outcome == Delivered.Refused) Refused(logger, token.Platform, delivery.Detail);
            if (delivery.Outcome != Delivered.Retry) return delivery.Outcome;
            if (attempt == Attempts)
            {
                NotTaken(logger, token.Platform, Attempts, delivery.Detail);
                return delivery.Outcome;
            }
            await Task.Delay(TimeSpan.FromSeconds(1 << (attempt - 1)), clock, ct);
        }
    }

    private static Dictionary<string, IPushService> Services(PushSettings settings, IHttpClientFactory http, TimeProvider clock)
    {
        var services = new Dictionary<string, IPushService>(StringComparer.Ordinal);
        if (settings.Apns is { } apple) services["apns"] = new Apns(apple, http, clock);
        if (settings.Fcm is { } google) services["fcm"] = new Fcm(google, http, clock);
        return services;
    }

    [LoggerMessage(Level = LogLevel.Warning, Message = "{Platform} refused a push notification: {Detail}")]
    private static partial void Refused(ILogger logger, string platform, string detail);

    [LoggerMessage(Level = LogLevel.Warning, Message = "{Platform} did not take a push notification in {Attempts} attempts: {Detail}")]
    private static partial void NotTaken(ILogger logger, string platform, int attempts, string detail);
}

internal static class PushJson
{
    public static byte[] Written(Action<Utf8JsonWriter> write)
    {
        var buffer = new ArrayBufferWriter<byte>();
        using (var json = new Utf8JsonWriter(buffer)) write(json);
        return buffer.WrittenSpan.ToArray();
    }

    public static ByteArrayContent Content(byte[] json) =>
        new(json) { Headers = { ContentType = new MediaTypeHeaderValue("application/json") } };

    public static string Signed(byte[] header, byte[] claims, Func<byte[], byte[]> sign)
    {
        var unsigned = $"{Base64Url.EncodeToString(header)}.{Base64Url.EncodeToString(claims)}";
        return $"{unsigned}.{Base64Url.EncodeToString(sign(Encoding.ASCII.GetBytes(unsigned)))}";
    }

    public static string? Text(JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object && value.TryGetProperty(name, out var field) && field.ValueKind == JsonValueKind.String
            ? field.GetString() : null;
}
