using System.Reflection;
using System.Text.Json;
using System.Text.RegularExpressions;
using Kodosi.Accounts;
using Kodosi.Missions;
using Kodosi.Plans;
using Kodosi.TerminalConnections;
using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.Routing;
using Microsoft.Extensions.DependencyInjection;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed partial class ContractTests(PostgresFixture postgres)
{
    private static readonly HashSet<string> SignInOnly =
    [
        "GET /api/me", "DELETE /api/me", "POST /api/me/devices/challenge", "POST /api/me/device-sessions", "POST /api/me/devices",
        "GET /api/me/identity", "POST /api/me/identity/reset", "POST /api/devices/link/init", "POST /api/devices/link/poll",
        "POST /api/devices/link/ack", "DELETE /api/devices/link/requests/{requestId}", "GET /api/me/recovery-key", "POST /api/me/devices/recover",
    ];

    [GeneratedRegex(@"\{(\w+):[^}]+\}")]
    private static partial Regex RouteConstraint();

    private static JsonElement Contract(string file)
    {
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null && !File.Exists(Path.Combine(directory.FullName, "protocol", file))) directory = directory.Parent;
        Assert.NotNull(directory);
        using var json = JsonDocument.Parse(File.ReadAllBytes(Path.Combine(directory.FullName, "protocol", file)));
        return json.RootElement.Clone();
    }

    private static List<(string Route, RouteEndpoint Endpoint)> Routes(BackendApplication app) =>
        app.Services.GetRequiredService<EndpointDataSource>().Endpoints.OfType<RouteEndpoint>()
            .SelectMany(endpoint => (endpoint.Metadata.GetMetadata<IHttpMethodMetadata>()?.HttpMethods ?? []).Select(method =>
                ($"{method} {RouteConstraint().Replace(endpoint.RoutePattern.RawText!, "{$1}").TrimEnd('/')}", endpoint)))
            .ToList();

    [Fact]
    public async Task TheServerHasTheVersionsLimitsAndRoutesOfItsContracts()
    {
        var api = Contract("backend-api.json");
        var connections = Contract("terminal-connections.json");
        Assert.Equal(ContractVersions.Api, api.GetProperty("apiContractVersion").GetInt32());
        Assert.Equal(ContractVersions.OldestApi, api.GetProperty("oldestApiContractVersion").GetInt32());
        Assert.Equal(ContractVersions.Auth, api.GetProperty("authContractVersion").GetInt32());
        Assert.Equal(ContractVersions.TerminalConnection, connections.GetProperty("terminalConnectionProtocolVersion").GetInt32());
        Assert.Equal(ContractVersions.OldestTerminalConnection, connections.GetProperty("oldestTerminalConnectionProtocolVersion").GetInt32());
        var room = api.GetProperty("roomContent").GetProperty("limits");
        Assert.Equal(PlanLimits.Safety.RoomPeople, room.GetProperty("people").GetInt32());
        Assert.Equal(MissionService.MaxRoomRecipients, room.GetProperty("recipients").GetInt32());
        Assert.Equal(PlanLimits.Safety.RoomPeople * (PlanCatalog.MostDevices + 1), room.GetProperty("recipients").GetInt32());
        var pipes = connections.GetProperty("pipes");
        Assert.Equal(ConnectionEndpoints.MessageBytes, pipes.GetProperty("messageMaxBytes").GetInt32());
        Assert.Equal(ConnectionDirectory.MaximumSessionPipes, pipes.GetProperty("viewersMax").GetInt32());
        Assert.Equal(ConnectionDirectory.MaximumDevicePipes, pipes.GetProperty("deviceMax").GetInt32());

        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        var routes = Routes(app).Select(x => x.Route).ToList();
        var promised = api.GetProperty("routes").EnumerateObject().SelectMany(x => x.Value.EnumerateArray()).Select(x => x.GetString()!);
        Assert.Equal(promised.Order(), routes.Where(x => x.Contains(" /api/", StringComparison.Ordinal)).Order());
        Assert.Contains("GET " + connections.GetProperty("routes").GetProperty("device").GetString(), routes);
        Assert.Equal(routes.Count, routes.Distinct().Count());
    }

    [Fact]
    public async Task EachRequestProvesAnApprovedDeviceExceptTheRequestsThatSetUpADevice()
    {
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        var checkedRoutes = 0;
        foreach (var (route, endpoint) in Routes(app))
        {
            var parameters = endpoint.Metadata.OfType<MethodInfo>().Single().GetParameters().Select(x => x.ParameterType).ToArray();
            var device = parameters.Contains(typeof(DeviceCaller)) || parameters.Contains(typeof(RenewingDevice));
            var signedIn = parameters.Contains(typeof(SignedIn));
            var authorized = endpoint.Metadata.GetMetadata<IAuthorizeData>() is not null;
            if (route.StartsWith("GET /health/", StringComparison.Ordinal))
            {
                Assert.False(device || signedIn || authorized, route);
                continue;
            }
            Assert.True(authorized, route);
            if (route == "GET /ws/device") continue;
            Assert.True(device != signedIn, route);
            Assert.Equal(SignInOnly.Contains(route), signedIn);
            checkedRoutes++;
        }
        Assert.True(checkedRoutes > SignInOnly.Count);
    }
}
