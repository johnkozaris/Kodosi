namespace Kodosi.Host.Realtime;



internal static class WebSocketAccessTokenResolver
{
    private const string MeEventsPath = "/me/events";
    private const string ParticipantsPathPrefix = "/participants/";
    private const string HostsPathPrefix = "/hosts/";
    private const string SecWebSocketProtocolHeader = "Sec-WebSocket-Protocol";


    public const string BearerSubProtocol = "bearer";

    public static string? Resolve(HttpRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        if (!request.HttpContext.WebSockets.IsWebSocketRequest)
        {
            return null;
        }

        if (!IsEligiblePath(request.Path.Value))
        {
            return null;
        }

        return TryExtractSubProtocolToken(request);
    }

    public static bool ClientAdvertisedBearerSubProtocol(HttpRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);
        return TryExtractSubProtocolToken(request) is not null;
    }



    private static bool IsEligiblePath(string? path)
        => string.Equals(path, MeEventsPath, StringComparison.OrdinalIgnoreCase)
            || (path?.StartsWith(ParticipantsPathPrefix, StringComparison.OrdinalIgnoreCase) ?? false)
            || (path?.StartsWith(HostsPathPrefix, StringComparison.OrdinalIgnoreCase) ?? false);

    private static string? TryExtractSubProtocolToken(HttpRequest request)
    {


        if (!request.Headers.TryGetValue(SecWebSocketProtocolHeader, out var headerValues))
        {
            return null;
        }

        string? previous = null;
        foreach (var rawValue in headerValues)
        {
            if (string.IsNullOrEmpty(rawValue))
            {
                continue;
            }

            foreach (var entry in rawValue.Split(',', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries))
            {
                if (string.Equals(previous, BearerSubProtocol, StringComparison.Ordinal)
                    && !string.Equals(entry, BearerSubProtocol, StringComparison.Ordinal))
                {
                    return entry;
                }

                previous = entry;
            }
        }

        return null;
    }
}
