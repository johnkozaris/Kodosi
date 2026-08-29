using System.ComponentModel.DataAnnotations;
using System.Text.Json;
using Kodosi.Application;
using Kodosi.Host.Endpoints;
using Kodosi.Host.Serialization;

namespace Kodosi.HostTests;

public sealed class RoomMutationContractTests
{
    private static readonly Guid RequestId =
        Guid.Parse("0198f169-7c50-7000-8000-000000000001");

    [Fact]
    public void SixMutationRequestsRequireUuidV7RequestIdAndRevisionOrIncarnationBindings()
    {
        AssertRequestIdRequired<RoomInvitationDecisionRequest>();
        AssertRequestIdRequired<CancelRoomInvitationRequest>();
        AssertRequestIdRequired<ReplaceRoomRosterRequest>();
        AssertRequestIdRequired<UpdateRoomTaskStatusRequest>();
        AssertRequestIdRequired<AssignRoomTaskRequest>();

        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web);
        HostJsonSerializerOptions.Configure(options);
        var transition = JsonSerializer.Deserialize<UpdateRoomTaskStatusRequest>(
            $$"""
              {
                "requestId":"{{RequestId:D}}",
                "expectedTaskRevision":7,
                "status":"Done",
                "actorSessionId":"0198f169-7c50-7000-8000-000000000002",
                "actorSessionIncarnationId":"0198f169-7c50-7000-8000-000000000003",
                "result":"ciphertext"
              }
              """,
            options);
        Assert.NotNull(transition);
        Assert.Equal(RequestId, transition.RequestId);
        Assert.Equal(7, transition.ExpectedTaskRevision);
        Assert.NotNull(transition.ActorSessionIncarnationId);
    }

    [Fact]
    public void NonV7AndMissingRequestIdsFailAtTheHttpValidationBoundary()
    {
        var nonV7 = new AssignRoomTaskRequest(Guid.NewGuid(), 0, null, null);
        Assert.Contains(
            ValidatorFor(nonV7),
            failure => failure.MemberNames.Contains(nameof(AssignRoomTaskRequest.RequestId)));

        var missing = new CancelRoomInvitationRequest(Guid.Empty);
        Assert.Contains(
            ValidatorFor(missing),
            failure => failure.MemberNames.Contains(nameof(CancelRoomInvitationRequest.RequestId)));
    }

    private static void AssertRequestIdRequired<T>()
    {
        var property = typeof(T).GetProperty("RequestId");
        Assert.NotNull(property);
        Assert.NotNull(property.GetCustomAttributes(typeof(UuidV7Attribute), false).SingleOrDefault());
    }

    private static IReadOnlyList<ValidationResult> ValidatorFor(object value)
    {
        var failures = new List<ValidationResult>();
        Validator.TryValidateObject(value, new ValidationContext(value), failures, true);
        return failures;
    }
}
