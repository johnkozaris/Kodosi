using Kodosi.Host.Realtime;
using Microsoft.Extensions.Options;
using Kodosi.Application;
using Kodosi.Host.Auth;
using Kodosi.Host.Configuration;

namespace Kodosi.Host.Endpoints;

public static class WebSocketEndpoints
{

    public static void MapWebSocketEndpoints(this IEndpointRouteBuilder app)
    {
        app.Map("/hosts/{sessionId}", async (
            HttpContext context,
            string sessionId,
            ICurrentUser currentUser,
            IOptions<WebSocketSecurityOptions> webSocketSecurity,
            GracefulShutdownHostedService shutdownService,
            HostWebSocketHandler handler) =>
        {
            if (shutdownService.IsDraining)
            {
                context.Response.StatusCode = StatusCodes.Status503ServiceUnavailable;
                return;
            }

            if (!context.WebSockets.IsWebSocketRequest)
            {
                context.Response.StatusCode = StatusCodes.Status400BadRequest;
                return;
            }

            if (!IsAllowedOrigin(context, webSocketSecurity.Value.AllowedOrigins))
            {
                context.Response.StatusCode = StatusCodes.Status403Forbidden;
                return;
            }


            var accessToken = BearerTokenResolver.Resolve(context.Request);
            using var ws = await AcceptWithBearerSubProtocolAsync(context);
            await handler.HandleAsync(ws, sessionId, currentUser.UserId, accessToken, context.RequestAborted);
        })
        .RequireAuthorization()
        .RequireRateLimiting(RateLimitPolicyNames.WebSocket);

        app.Map("/participants/{sessionId}", async (
            HttpContext context,
            string sessionId,
            ICurrentUser currentUser,
            IOptions<WebSocketSecurityOptions> webSocketSecurity,
            GracefulShutdownHostedService shutdownService,
            ParticipantWebSocketHandler handler) =>
        {
            if (shutdownService.IsDraining)
            {
                context.Response.StatusCode = StatusCodes.Status503ServiceUnavailable;
                return;
            }

            if (!context.WebSockets.IsWebSocketRequest)
            {
                context.Response.StatusCode = StatusCodes.Status400BadRequest;
                return;
            }

            if (!IsAllowedOrigin(context, webSocketSecurity.Value.AllowedOrigins))
            {
                context.Response.StatusCode = StatusCodes.Status403Forbidden;
                return;
            }


            var accessToken = BearerTokenResolver.Resolve(context.Request);
            using var ws = await AcceptWithBearerSubProtocolAsync(context);
            await handler.HandleAsync(ws, sessionId, currentUser.UserId, accessToken, context.RequestAborted);
        })
        .RequireAuthorization()
        .RequireRateLimiting(RateLimitPolicyNames.WebSocket);

        app.Map("/me/events", async (
            HttpContext context,
            ICurrentUser currentUser,
            IOptions<WebSocketSecurityOptions> webSocketSecurity,
            GracefulShutdownHostedService shutdownService,
            UserEventsWebSocketHandler handler) =>
        {
            if (shutdownService.IsDraining)
            {
                context.Response.StatusCode = StatusCodes.Status503ServiceUnavailable;
                return;
            }

            if (!context.WebSockets.IsWebSocketRequest)
            {
                context.Response.StatusCode = StatusCodes.Status400BadRequest;
                return;
            }

            if (!IsAllowedOrigin(context, webSocketSecurity.Value.AllowedOrigins))
            {
                context.Response.StatusCode = StatusCodes.Status403Forbidden;
                return;
            }

            var accessToken = BearerTokenResolver.Resolve(context.Request);
            using var ws = await AcceptWithBearerSubProtocolAsync(context);
            await handler.HandleAsync(
                ws,
                currentUser.UserId,
                accessToken,
                context.RequestAborted);
        })
        .RequireAuthorization()
        .RequireRateLimiting(RateLimitPolicyNames.WebSocket);
    }

    private static async Task<System.Net.WebSockets.WebSocket> AcceptWithBearerSubProtocolAsync(HttpContext context)
    {
        if (!WebSocketAccessTokenResolver.ClientAdvertisedBearerSubProtocol(context.Request))
        {
            return await context.WebSockets.AcceptWebSocketAsync();
        }

        return await context.WebSockets.AcceptWebSocketAsync(new WebSocketAcceptContext
        {
            SubProtocol = WebSocketAccessTokenResolver.BearerSubProtocol,
        });
    }

    internal static bool IsAllowedOrigin(HttpContext context, IReadOnlyCollection<string> allowedOrigins)
    {


        if (allowedOrigins.Count == 0)
        {
            return !context.Request.Headers.ContainsKey("Origin")
                && HasAuthorizationBearer(context.Request)
                && !WebSocketAccessTokenResolver.ClientAdvertisedBearerSubProtocol(
                    context.Request);
        }




        if (!context.Request.Headers.TryGetValue("Origin", out var originValues))
        {
            return HasAuthorizationBearer(context.Request)
                && !WebSocketAccessTokenResolver.ClientAdvertisedBearerSubProtocol(context.Request);
        }

        var origin = originValues.ToString().Trim().TrimEnd('/');
        if (origin.Length == 0)
        {
            return false;
        }

        return allowedOrigins.Any(allowedOrigin =>
            string.Equals(
                allowedOrigin.Trim().TrimEnd('/'),
                origin,
                StringComparison.OrdinalIgnoreCase));
    }

    private static bool HasAuthorizationBearer(HttpRequest request)
        => request.Headers.Authorization.ToString()
                .StartsWith("Bearer ", StringComparison.OrdinalIgnoreCase);
}
