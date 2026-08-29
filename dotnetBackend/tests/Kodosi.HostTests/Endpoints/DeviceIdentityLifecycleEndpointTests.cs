using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Endpoints;

namespace Kodosi.HostTests;

public sealed class DeviceIdentityLifecycleEndpointTests
{
    [Fact]
    public void IdentityReset_Response_Reports_Attempted_And_Ended_Separately()
    {
        var response = new IdentityResetEndpoints.IdentityResetResponse(
            DevicesRemoved: 0,
            ListsRemoved: 0,
            SessionsEnded: 0,
            SessionsAttempted: 1);
        var json = JsonSerializer.SerializeToElement(
            response,
            new JsonSerializerOptions(JsonSerializerDefaults.Web));

        Assert.Equal(1, json.GetProperty("sessionsAttempted").GetInt32());
        Assert.Equal(0, json.GetProperty("sessionsEnded").GetInt32());
    }
}
