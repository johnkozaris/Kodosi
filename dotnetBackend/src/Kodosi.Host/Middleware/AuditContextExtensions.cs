namespace Kodosi.Host.Middleware;

internal static class AuditContextExtensions
{
    private const int UserAgentMaxLength = 512;

    public static (string? Ip, string? Ua) ExtractAuditContext(this HttpContext httpContext)
    {
        var ip = httpContext.Connection.RemoteIpAddress?.ToString();
        var ua = httpContext.Request.Headers.UserAgent.ToString();
        if (string.IsNullOrWhiteSpace(ua))
        {
            ua = null;
        }
        else if (ua.Length > UserAgentMaxLength)
        {
            ua = ua[..UserAgentMaxLength];
        }
        return (ip, ua);
    }
}
