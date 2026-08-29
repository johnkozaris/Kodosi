using System.Net.WebSockets;
using Microsoft.AspNetCore.Http;
using Microsoft.AspNetCore.Http.Features;

using Kodosi.Host.Endpoints;
using Kodosi.Host.Realtime;

namespace Kodosi.HostTests;

public sealed class WebSocketAccessTokenResolverTests
{
    [Fact]
    public void Resolve_Returns_AccessToken_For_MeEvents_Subprotocol()
    {
        var context = new DefaultHttpContext();
        context.Features.Set<IHttpWebSocketFeature>(new TestWebSocketFeature(isWebSocketRequest: true));
        context.Request.Path = "/me/events";
        context.Request.Headers["Sec-WebSocket-Protocol"] = "bearer, test-token";

        var token = WebSocketAccessTokenResolver.Resolve(context.Request);

        Assert.Equal("test-token", token);
    }

    [Fact]
    public void Resolve_Returns_AccessToken_For_Participant_Subprotocol()
    {
        var context = new DefaultHttpContext();
        context.Features.Set<IHttpWebSocketFeature>(new TestWebSocketFeature(isWebSocketRequest: true));
        context.Request.Path = "/participants/session-123";
        context.Request.Headers["Sec-WebSocket-Protocol"] = "bearer, test-token";

        var token = WebSocketAccessTokenResolver.Resolve(context.Request);

        Assert.Equal("test-token", token);
    }

    [Fact]
    public void Resolve_Returns_Null_For_NonWebSocket_Request()
    {
        var context = new DefaultHttpContext();
        context.Features.Set<IHttpWebSocketFeature>(new TestWebSocketFeature(isWebSocketRequest: false));
        context.Request.Path = "/me/events";
        context.Request.QueryString = new QueryString("?access_token=test-token");
        context.Request.Headers["Sec-WebSocket-Protocol"] = "bearer, test-token";

        var token = WebSocketAccessTokenResolver.Resolve(context.Request);

        Assert.Null(token);
    }

    [Fact]
    public void Resolve_Returns_Token_For_Hosts_Path()
    {
        var context = new DefaultHttpContext();
        context.Features.Set<IHttpWebSocketFeature>(new TestWebSocketFeature(isWebSocketRequest: true));
        context.Request.Path = "/hosts/session-123";
        context.Request.Headers["Sec-WebSocket-Protocol"] = "bearer, test-token";

        var token = WebSocketAccessTokenResolver.Resolve(context.Request);

        Assert.Equal("test-token", token);
    }

    [Fact]
    public void Resolve_Returns_Null_For_Unknown_WebSocket_Path()
    {
        var context = new DefaultHttpContext();
        context.Features.Set<IHttpWebSocketFeature>(new TestWebSocketFeature(isWebSocketRequest: true));
        context.Request.Path = "/some-other-ws";
        context.Request.Headers["Sec-WebSocket-Protocol"] = "bearer, test-token";

        var token = WebSocketAccessTokenResolver.Resolve(context.Request);

        Assert.Null(token);
    }

    [Fact]
    public void Resolve_Ignores_QueryString_Token()
    {
        var context = new DefaultHttpContext();
        context.Features.Set<IHttpWebSocketFeature>(new TestWebSocketFeature(isWebSocketRequest: true));
        context.Request.Path = "/me/events";
        context.Request.QueryString = new QueryString("?access_token=query-string-token");

        var token = WebSocketAccessTokenResolver.Resolve(context.Request);

        Assert.Null(token);
    }

    [Fact]
    public void Resolve_Ignores_Bearer_SubProtocol_Without_Following_Token()
    {
        var context = new DefaultHttpContext();
        context.Features.Set<IHttpWebSocketFeature>(new TestWebSocketFeature(isWebSocketRequest: true));
        context.Request.Path = "/me/events";
        context.Request.Headers["Sec-WebSocket-Protocol"] = "bearer";
        context.Request.QueryString = new QueryString("?access_token=fallback-token");

        var token = WebSocketAccessTokenResolver.Resolve(context.Request);

        Assert.Null(token);
    }

    [Fact]
    public void Resolve_Parses_Repeated_SubProtocol_Headers()
    {
        var context = new DefaultHttpContext();
        context.Features.Set<IHttpWebSocketFeature>(new TestWebSocketFeature(isWebSocketRequest: true));
        context.Request.Path = "/me/events";
        context.Request.Headers["Sec-WebSocket-Protocol"] = new[] { "chat", "bearer", "repeated-token" };

        var token = WebSocketAccessTokenResolver.Resolve(context.Request);

        Assert.Equal("repeated-token", token);
    }

    [Fact]
    public void ClientAdvertisedBearerSubProtocol_True_When_Bearer_Followed_By_Token()
    {
        var context = new DefaultHttpContext();
        context.Features.Set<IHttpWebSocketFeature>(new TestWebSocketFeature(isWebSocketRequest: true));
        context.Request.Headers["Sec-WebSocket-Protocol"] = "bearer, some-jwt";

        Assert.True(WebSocketAccessTokenResolver.ClientAdvertisedBearerSubProtocol(context.Request));
    }

    [Fact]
    public void ClientAdvertisedBearerSubProtocol_False_When_No_Subprotocol()
    {
        var context = new DefaultHttpContext();
        context.Features.Set<IHttpWebSocketFeature>(new TestWebSocketFeature(isWebSocketRequest: true));

        Assert.False(WebSocketAccessTokenResolver.ClientAdvertisedBearerSubProtocol(context.Request));
    }

    [Fact]
    public void ClientAdvertisedBearerSubProtocol_False_When_Bearer_Has_No_Token()
    {
        var context = new DefaultHttpContext();
        context.Features.Set<IHttpWebSocketFeature>(new TestWebSocketFeature(isWebSocketRequest: true));
        context.Request.Headers["Sec-WebSocket-Protocol"] = "bearer";

        Assert.False(WebSocketAccessTokenResolver.ClientAdvertisedBearerSubProtocol(context.Request));
    }

    [Fact]
    public void IsAllowedOrigin_Accepts_Configured_Browser_Origin()
    {
        var context = new DefaultHttpContext();
        context.Request.Headers.Origin = "https://app.kodosi.com";

        Assert.True(WebSocketEndpoints.IsAllowedOrigin(context, ["https://app.kodosi.com"]));
    }

    [Fact]
    public void IsAllowedOrigin_Rejects_Unconfigured_Browser_Origin()
    {
        var context = new DefaultHttpContext();
        context.Request.Headers.Origin = "https://evil.example";

        Assert.False(WebSocketEndpoints.IsAllowedOrigin(context, ["https://app.kodosi.com"]));
    }

    [Fact]
    public void IsAllowedOrigin_Empty_List_Allows_Originless_Native_Client()
    {
        var context = new DefaultHttpContext();
        context.Request.Headers.Authorization = $"Bearer {new string('x', 32)}";

        Assert.True(WebSocketEndpoints.IsAllowedOrigin(context, []));
    }

    [Fact]
    public void IsAllowedOrigin_Empty_List_Rejects_Any_Origin_Header()
    {
        var context = new DefaultHttpContext();
        context.Request.Headers.Authorization = $"Bearer {new string('x', 32)}";
        context.Request.Headers.Origin = "https://app.kodosi.com";

        Assert.False(WebSocketEndpoints.IsAllowedOrigin(context, []));
    }

    [Fact]
    public void IsAllowedOrigin_Allows_Native_Authorization_Client_Without_Origin()
    {
        var context = new DefaultHttpContext();
        context.Request.Headers.Authorization = "Bearer native-token";

        Assert.True(WebSocketEndpoints.IsAllowedOrigin(context, ["https://app.kodosi.com"]));
    }

    [Fact]
    public void IsAllowedOrigin_Rejects_Bearer_Subprotocol_Client_Without_Origin()
    {
        var context = new DefaultHttpContext();
        context.Features.Set<IHttpWebSocketFeature>(new TestWebSocketFeature(isWebSocketRequest: true));
        context.Request.Path = "/me/events";
        context.Request.Headers["Sec-WebSocket-Protocol"] = "bearer, browser-token";

        Assert.False(WebSocketEndpoints.IsAllowedOrigin(context, ["https://app.kodosi.com"]));
    }

    private sealed class TestWebSocketFeature(bool isWebSocketRequest) : IHttpWebSocketFeature
    {
        public bool IsWebSocketRequest => isWebSocketRequest;

        public Task<WebSocket> AcceptAsync(WebSocketAcceptContext context)
            => throw new NotSupportedException();
    }
}
