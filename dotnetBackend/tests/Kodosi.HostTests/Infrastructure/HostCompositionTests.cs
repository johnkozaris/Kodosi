using System.Net;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host;
using Kodosi.Host.Configuration;
using Kodosi.Host.DependencyInjection;
using Kodosi.Host.Endpoints;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Persistence;
using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Hosting;
using Microsoft.AspNetCore.Hosting.Server;
using Microsoft.AspNetCore.Hosting.Server.Features;
using Microsoft.AspNetCore.Http;
using Microsoft.AspNetCore.Http.Metadata;
using Microsoft.AspNetCore.Mvc;
using Microsoft.AspNetCore.Routing;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;

namespace Kodosi.HostTests;

public sealed class HostCompositionTests
{
    [Fact]
    public void DbContext_Is_Composed_Without_A_Retrying_Execution_Strategy()
    {
        var builder = WebApplication.CreateBuilder(new WebApplicationOptions
        {
            EnvironmentName = Environments.Development,
        });
        builder.Configuration.AddInMemoryCollection(new Dictionary<string, string?>
        {
            ["ConnectionStrings:Kodosi"] = "Host=localhost;Database=composition_only",
        });
        builder.Services.AddInfrastructure(builder.Configuration);
        using var provider = builder.Services.BuildServiceProvider();
        using var scope = provider.CreateScope();

        var context = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
        var strategy = context.Database.CreateExecutionStrategy();

        Assert.False(strategy.RetriesOnFailure);
    }

    [Fact]
    public void Relay_Lease_Is_One_Instance_Shared_With_Hosted_Pipeline()
    {
        var builder = WebApplication.CreateBuilder(new WebApplicationOptions
        {
            EnvironmentName = Environments.Development,
        });
        builder.Configuration.AddInMemoryCollection(new Dictionary<string, string?>
        {
            ["ConnectionStrings:Kodosi"] = "Host=localhost;Database=composition_only",
        });
        builder.Services.AddInfrastructure(builder.Configuration);
        using var provider = builder.Services.BuildServiceProvider();

        var lease = provider.GetRequiredService<RelaySingletonLeaseHostedService>();
        var hostedLease = Assert.Single(
            provider.GetServices<IHostedService>(),
            service => service is RelaySingletonLeaseHostedService);

        Assert.Same(lease, hostedLease);
    }

    [Fact]
    public void Device_Workflow_Graphs_Resolve_From_One_Request_Scope()
    {
        var builder = WebApplication.CreateBuilder(new WebApplicationOptions
        {
            EnvironmentName = Environments.Development,
        });
        builder.Configuration.AddInMemoryCollection(new Dictionary<string, string?>
        {
            ["ConnectionStrings:Kodosi"] = "Host=localhost;Database=composition_only",
            ["Telemetry:ServiceName"] = "Kodosi.Tests",
        });
        builder.Services
            .AddApplication()
            .AddInfrastructure(builder.Configuration)
            .AddHostAdapters(builder.Configuration);
        using var provider = builder.Services.BuildServiceProvider(new ServiceProviderOptions
        {
            ValidateOnBuild = true,
            ValidateScopes = true,
        });
        using var scope = provider.CreateScope();

        Assert.NotNull(scope.ServiceProvider.GetRequiredService<DeviceRegistrationChallengeService>());
        Assert.NotNull(scope.ServiceProvider.GetRequiredService<DeviceEnrollmentService>());
        Assert.NotNull(scope.ServiceProvider.GetRequiredService<DeviceLinkRequestService>());
        Assert.NotNull(scope.ServiceProvider.GetRequiredService<DeviceLinkApprovalService>());
        Assert.NotNull(scope.ServiceProvider.GetRequiredService<DeviceLinkPollService>());
    }

    [Fact]
    public void WebSocket_Handler_Graphs_Are_Singletons_Without_Scoped_Captures()
    {
        var builder = WebApplication.CreateBuilder(new WebApplicationOptions
        {
            EnvironmentName = Environments.Development,
        });
        builder.Configuration.AddInMemoryCollection(new Dictionary<string, string?>
        {
            ["ConnectionStrings:Kodosi"] = "Host=localhost;Database=composition_only",
            ["Auth:Providers:0:Enabled"] = "true",
            ["Auth:Providers:0:Scheme"] = "test",
            ["Auth:Providers:0:Provider"] = "test",
            ["Auth:Providers:0:Authority"] = "https://issuer.test",
            ["Auth:Providers:0:CanonicalIssuer"] = "https://issuer.test",
            ["Auth:Providers:0:Audiences:0"] = "test-audience",
            ["Auth:Providers:0:AllowedAlgorithms:0"] = "RS256",
            ["Telemetry:ServiceName"] = "Kodosi.Tests",
        });
        builder.Services
            .AddApplication()
            .AddInfrastructure(builder.Configuration)
            .AddHostAdapters(builder.Configuration);

        var handlerTypes = new[]
        {
            typeof(HostWebSocketHandler),
            typeof(ParticipantWebSocketHandler),
            typeof(UserEventsWebSocketHandler),
            typeof(SessionEndCoordinator),
            typeof(SessionLifecycleGate),
        };
        Assert.All(handlerTypes, handlerType =>
        {
            var descriptor = Assert.Single(
                builder.Services,
                service => service.ServiceType == handlerType);
            Assert.Equal(ServiceLifetime.Singleton, descriptor.Lifetime);
        });

        using var provider = builder.Services.BuildServiceProvider(
            new ServiceProviderOptions
            {
                ValidateOnBuild = true,
                ValidateScopes = true,
            });
        Assert.All(handlerTypes, handlerType =>
            Assert.NotNull(provider.GetRequiredService(handlerType)));
        Assert.Same(
            provider.GetRequiredService<SessionEndCoordinator>(),
            provider.GetRequiredService<ISessionEndAuthority>());
        Assert.Same(
            provider.GetRequiredService<IIdentityResetSessionResolver>(),
            provider.GetRequiredService<IDeviceRevocationSessionResolver>());
        Assert.NotNull(provider.GetRequiredService<IDeviceListRealtimeEffects>());
    }

    [Fact]
    public async Task Every_Api_And_WebSocket_Endpoint_Is_Authorized_And_RateLimited()
    {
        var builder = WebApplication.CreateBuilder(new WebApplicationOptions
        {
            EnvironmentName = Environments.Development,
        });
        builder.Configuration.AddInMemoryCollection(new Dictionary<string, string?>
        {
            ["ConnectionStrings:Kodosi"] = "Host=localhost;Database=composition_only",
            ["Auth:Providers:0:Enabled"] = "true",
            ["Auth:Providers:0:Scheme"] = "test",
            ["Auth:Providers:0:Provider"] = "test",
            ["Auth:Providers:0:Authority"] = "https://issuer.test",
            ["Auth:Providers:0:CanonicalIssuer"] = "https://issuer.test",
            ["Auth:Providers:0:Audiences:0"] = "test-audience",
            ["Auth:Providers:0:AllowedAlgorithms:0"] = "RS256",
            ["Telemetry:ServiceName"] = "Kodosi.Tests",
        });
        builder.Services
            .AddApplication()
            .AddInfrastructure(builder.Configuration)
            .AddHostAdapters(builder.Configuration);
        await using var app = builder.Build();
        app.MapAllEndpoints();

        var endpoints = ((IEndpointRouteBuilder)app).DataSources
            .SelectMany(source => source.Endpoints)
            .OfType<RouteEndpoint>()
            .Where(endpoint =>
                endpoint.RoutePattern.RawText?.StartsWith("/api", StringComparison.Ordinal) == true
                || endpoint.RoutePattern.RawText is "/hosts/{sessionId}"
                    or "/participants/{sessionId}"
                    or "/me/events")
            .ToList();

        Assert.NotEmpty(endpoints);
        Assert.All(endpoints, endpoint =>
        {
            Assert.NotNull(endpoint.Metadata.GetMetadata<IAuthorizeData>());
            Assert.Contains(
                endpoint.Metadata,
                metadata => metadata.GetType().Name.Contains(
                    "EnableRateLimiting",
                    StringComparison.Ordinal));
        });

        var deviceProofRoutes = new HashSet<string>(StringComparer.Ordinal)
        {
            "/api/me/device-proofs/challenge",
            "/api/me/semantic-receipts/",
            "/api/me/semantic-receipts/{requestId:guid}/ack",
            "/api/sessions/{id:guid}/keys/mine",
        };
        var protectedDeviceRoutes = endpoints
            .Where(endpoint => deviceProofRoutes.Contains(endpoint.RoutePattern.RawText!))
            .ToList();
        Assert.Equal(5, protectedDeviceRoutes.Count);
        Assert.All(protectedDeviceRoutes, endpoint => Assert.Equal(
            RateLimitPolicyNames.DeviceProof,
            endpoint.Metadata
                .GetOrderedMetadata<Microsoft.AspNetCore.RateLimiting.EnableRateLimitingAttribute>()
                .Last()
                .PolicyName));

        var roomFeedEndpoint = Assert.Single(endpoints, endpoint =>
            endpoint.RoutePattern.RawText == "/api/feed/room/{roomId:guid}");
        Assert.Contains(
            roomFeedEndpoint.Metadata.GetOrderedMetadata<IProducesResponseTypeMetadata>(),
            metadata =>
                metadata.StatusCode == StatusCodes.Status400BadRequest
                && metadata.Type == typeof(ProblemDetails));

        var leaveSessionEndpoint = Assert.Single(endpoints, endpoint =>
            endpoint.RoutePattern.RawText == "/api/sessions/{id:guid}/access/me"
            && endpoint.Metadata.GetMetadata<HttpMethodMetadata>()
                ?.HttpMethods.Contains(HttpMethods.Delete) == true);
        Assert.NotNull(leaveSessionEndpoint.Metadata.GetMetadata<IAuthorizeData>());

        var encryptedRoomWrites = endpoints.Where(endpoint =>
            endpoint.Metadata.GetMetadata<HttpMethodMetadata>()
                ?.HttpMethods.Contains(HttpMethods.Post) == true
            && endpoint.RoutePattern.RawText is
                "/api/rooms/{roomId:guid}/chat"
                or "/api/rooms/{roomId:guid}/tasks"
                or "/api/rooms/{roomId:guid}/tasks/{taskId:guid}/status");
        Assert.All(encryptedRoomWrites, endpoint =>
            Assert.True(
                endpoint.Metadata.GetMetadata<IRequestSizeLimitMetadata>()
                    ?.MaxRequestBodySize >= RoomRequestLimits.EncryptedContentBodyBytes));

        var rosterWrites = endpoints.Where(endpoint =>
            endpoint.RoutePattern.RawText is
                "/api/rooms/"
                or "/api/rooms/{roomId:guid}/invitations"
                or "/api/rooms/{roomId:guid}/members/{userId:guid}"
            && endpoint.Metadata.GetMetadata<HttpMethodMetadata>()?.HttpMethods.Any(
                method => string.Equals(method, HttpMethods.Post, StringComparison.Ordinal)
                    || string.Equals(method, HttpMethods.Delete, StringComparison.Ordinal)) == true)
            .ToList();
        Assert.Equal(3, rosterWrites.Count);
        Assert.All(rosterWrites, endpoint =>
        {
            var expected = endpoint.RoutePattern.RawText
                == "/api/rooms/{roomId:guid}/invitations"
                ? RoomRequestLimits.InvitationWithRosterBodyBytes
                : RoomRequestLimits.SingleRosterBodyBytes;
            Assert.Equal(
                expected,
                endpoint.Metadata.GetMetadata<IRequestSizeLimitMetadata>()
                    ?.MaxRequestBodySize);
            Assert.True(expected > 64 * 1024);
            Assert.True(expected < 4 * 1024 * 1024);
        });

        var identityProofWrites = endpoints.Where(endpoint =>
            endpoint.Metadata.GetMetadata<HttpMethodMetadata>()
                ?.HttpMethods.Contains(HttpMethods.Post) == true
            && endpoint.RoutePattern.RawText is
                "/api/me/devices"
                or "/api/me/identity/device-list"
                or "/api/devices/link/approve")
            .ToList();
        Assert.Equal(3, identityProofWrites.Count);
        Assert.All(identityProofWrites, endpoint =>
            Assert.Equal(
                IdentityRequestLimits.MutationBodyBytes,
                endpoint.Metadata.GetMetadata<IRequestSizeLimitMetadata>()
                    ?.MaxRequestBodySize));
        Assert.True(
            IdentityRequestLimits.MutationBodyBytes
            > ((IdentityWireFormat.MaxSignedDeviceListBodyLength + 2L) / 3L) * 4L);
        Assert.True(IdentityRequestLimits.MutationBodyBytes < 2 * 1024 * 1024);

        var invitationActions = endpoints.Where(endpoint =>
            endpoint.Metadata.GetMetadata<HttpMethodMetadata>()
                ?.HttpMethods.Contains(HttpMethods.Post) == true
            && endpoint.RoutePattern.RawText is
                "/api/rooms/{roomId:guid}/invitations"
                or "/api/rooms/invitations/{invitationId:guid}/accept"
                or "/api/rooms/invitations/{invitationId:guid}/decline")
            .ToList();
        Assert.Equal(3, invitationActions.Count);
        Assert.All(invitationActions, endpoint =>
            Assert.Contains(
                endpoint.Metadata.GetOrderedMetadata<IProducesResponseTypeMetadata>(),
                metadata => metadata.Type == (endpoint.RoutePattern.RawText
                    == "/api/rooms/{roomId:guid}/invitations"
                        ? typeof(RoomInvitationResponse)
                        : typeof(RoomMutationResponse))));

        var roomReceiptLookup = Assert.Single(endpoints, endpoint =>
            endpoint.RoutePattern.RawText == "/api/rooms/mutations/{operation}/{requestId}"
            && endpoint.Metadata.GetMetadata<HttpMethodMetadata>()
                ?.HttpMethods.Contains(HttpMethods.Get) == true);
        Assert.Contains(
            roomReceiptLookup.Metadata.GetOrderedMetadata<IProducesResponseTypeMetadata>(),
            metadata => metadata.Type == typeof(RoomMutationReceiptResponse));

        var semanticMailbox = endpoints.Where(endpoint =>
            endpoint.RoutePattern.RawText?.StartsWith(
                "/api/me/semantic-receipts",
                StringComparison.Ordinal) == true)
            .ToList();
        Assert.Equal(3, semanticMailbox.Count);
        Assert.Contains(semanticMailbox, endpoint =>
            endpoint.RoutePattern.RawText == "/api/me/semantic-receipts/"
            && endpoint.Metadata.GetMetadata<HttpMethodMetadata>()
                ?.HttpMethods.Contains(HttpMethods.Get) == true);
        Assert.Contains(semanticMailbox, endpoint =>
            endpoint.RoutePattern.RawText == "/api/me/semantic-receipts/"
            && endpoint.Metadata.GetMetadata<HttpMethodMetadata>()
                ?.HttpMethods.Contains(HttpMethods.Post) == true);
        Assert.Contains(semanticMailbox, endpoint =>
            endpoint.RoutePattern.RawText == "/api/me/semantic-receipts/{requestId:guid}/ack"
            && endpoint.Metadata.GetMetadata<HttpMethodMetadata>()
                ?.HttpMethods.Contains(HttpMethods.Post) == true);

        Assert.DoesNotContain(endpoints, endpoint =>
            endpoint.RoutePattern.RawText == "/api/rooms/{roomId:guid}/members"
            && endpoint.Metadata.GetMetadata<HttpMethodMetadata>()
                ?.HttpMethods.Contains(HttpMethods.Post) == true);
    }

    [Theory]
    [InlineData("/single-roster", RoomRequestLimits.SingleRosterBodyBytes)]
    [InlineData("/invitation-roster", RoomRequestLimits.InvitationWithRosterBodyBytes)]
    public async Task Roster_Request_Limits_Override_Global_Limit_And_Return_413_Above_Boundary(
        string path,
        long requestLimit)
    {
        var builder = WebApplication.CreateBuilder();
        builder.WebHost.ConfigureKestrel(options =>
        {
            options.Listen(IPAddress.Loopback, 0);
            options.Limits.MaxRequestBodySize = 64 * 1024;
        });
        await using var app = builder.Build();
        app.MapPost(path, async (HttpContext context) =>
        {
            await context.Request.Body.CopyToAsync(
                Stream.Null,
                context.RequestAborted);
            return Results.NoContent();
        }).WithMetadata(new RequestSizeLimitAttribute(requestLimit));
        await app.StartAsync(TestContext.Current.CancellationToken);

        var address = app.Services.GetRequiredService<IServer>()
            .Features.Get<IServerAddressesFeature>()!
            .Addresses.Single();
        using var client = new HttpClient { BaseAddress = new Uri(address) };

        using var accepted = await client.PostAsync(
            path,
            new ByteArrayContent(new byte[requestLimit]),
            TestContext.Current.CancellationToken);
        Assert.Equal(HttpStatusCode.NoContent, accepted.StatusCode);

        using var rejectedRequest = new HttpRequestMessage(HttpMethod.Post, path)
        {
            Content = new ByteArrayContent(new byte[requestLimit + 1])
        };


        rejectedRequest.Headers.ExpectContinue = true;
        using var rejected = await client.SendAsync(
            rejectedRequest,
            TestContext.Current.CancellationToken);
        Assert.Equal(HttpStatusCode.RequestEntityTooLarge, rejected.StatusCode);
    }
}
