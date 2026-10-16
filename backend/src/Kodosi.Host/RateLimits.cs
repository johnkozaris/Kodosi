using System.Globalization;
using System.Threading.RateLimiting;
using Microsoft.AspNetCore.RateLimiting;

namespace Kodosi;

internal static class RateLimits
{
    public const string Challenge = "challenge";
    public const string Enrollment = "enrollment";
    public const string Socket = "socket";

    public static void AddApiRateLimits(this IServiceCollection services) => services.AddRateLimiter(options =>
    {
        options.RejectionStatusCode = StatusCodes.Status429TooManyRequests;
        options.OnRejected = RejectAsync;
        options.GlobalLimiter = PartitionedRateLimiter.Create<HttpContext, (bool, string)>(context =>
            Bucket(context, burst: 1200, tokens: 20, TimeSpan.FromSeconds(1)));
        options.AddPolicy(Challenge, context => Bucket(context, burst: 600, tokens: 10, TimeSpan.FromSeconds(1)));
        options.AddPolicy(Enrollment, context => Bucket(context, burst: 20, tokens: 1, TimeSpan.FromSeconds(3)));
        options.AddPolicy(Socket, context => Bucket(context, burst: 60, tokens: 1, TimeSpan.FromSeconds(1)));
    });

    private static RateLimitPartition<(bool, string)> Bucket(HttpContext context, int burst, int tokens, TimeSpan period) =>
        RateLimitPartition.GetTokenBucketLimiter(Partition(context), _ => new TokenBucketRateLimiterOptions
        {
            TokenLimit = burst,
            TokensPerPeriod = tokens,
            ReplenishmentPeriod = period,
            QueueLimit = 0,
            AutoReplenishment = true,
        });

    private static (bool Account, string Key) Partition(HttpContext context) =>
        context.User.FindFirst("sub")?.Value is { } subject
            ? (true, subject)
            : (false, context.Connection.RemoteIpAddress?.ToString() ?? "");

    private static async ValueTask RejectAsync(OnRejectedContext rejected, CancellationToken ct)
    {
        var context = rejected.HttpContext;
        if (rejected.Lease.TryGetMetadata(MetadataName.RetryAfter, out var wait))
            context.Response.Headers.RetryAfter = Math.Max(1, (int)Math.Ceiling(wait.TotalSeconds)).ToString(CultureInfo.InvariantCulture);
        context.RequestServices.GetRequiredService<ServerMetrics>().Limited();
        await context.Response.WriteAsJsonAsync(new ErrorBody("Too many requests. Wait a moment and try again."), ct);
    }
}
