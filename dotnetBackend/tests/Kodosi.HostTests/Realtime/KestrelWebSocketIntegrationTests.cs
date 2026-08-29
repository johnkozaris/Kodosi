using System.Net.WebSockets;
using Kodosi.Host.Realtime;
using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Hosting;
using Microsoft.AspNetCore.Hosting.Server;
using Microsoft.AspNetCore.Hosting.Server.Features;
using Microsoft.Extensions.DependencyInjection;

namespace Kodosi.HostTests;

public sealed class KestrelWebSocketIntegrationTests
{
    [Fact]
    public async Task Idle_WebSocket_Is_Closed_When_Handshake_Deadline_Expires()
    {
        var builder = WebApplication.CreateBuilder();
        builder.WebHost
            .UseKestrel()
            .UseUrls("http://127.0.0.1:0");
        await using var app = builder.Build();
        app.UseWebSockets();
        app.Map("/ws", async context =>
        {
            using var socket = await context.WebSockets.AcceptWebSocketAsync();
            var handshake = await WebSocketHandshakeReader.ReceiveAsync(
                socket,
                maxMessageBytes: 64,
                timeout: TimeSpan.FromMilliseconds(50),
                context.RequestAborted);
            if (handshake is null)
            {
                socket.Abort();
            }
        });

        await app.StartAsync(TestContext.Current.CancellationToken);
        try
        {
            var server = app.Services.GetRequiredService<IServer>();
            var address = Assert.Single(
                server.Features.Get<IServerAddressesFeature>()!.Addresses);
            var uri = new UriBuilder(address)
            {
                Scheme = "ws",
                Path = "/ws",
            }.Uri;
            using var client = new ClientWebSocket();
            await client.ConnectAsync(uri, TestContext.Current.CancellationToken);

            await Assert.ThrowsAsync<WebSocketException>(() =>
                client.ReceiveAsync(
                    new ArraySegment<byte>(new byte[64]),
                    TestContext.Current.CancellationToken));
        }
        finally
        {
            await app.StopAsync(TestContext.Current.CancellationToken);
        }
    }
}
