using System.ComponentModel.DataAnnotations;
using System.Security.Cryptography;
using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
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

    [Theory]
    [InlineData(1)]
    [InlineData(8)]
    public void TaskRequestsAcceptPostQuantumEnvelopeSizesAtHttpAndDomainBoundaries(int recipientCount)
    {
        var taskEnvelope = SizedRoomEnvelope("task", recipientCount);
        var resultEnvelope = SizedRoomEnvelope("taskResult", recipientCount);
        Assert.True(taskEnvelope.Length > RoomInputRules.TaskTitleMaxLength);
        Assert.True(resultEnvelope.Length > RoomInputRules.TaskResultMaxLength);

        var create = new CreateRoomTaskRequest(
            Guid.NewGuid(), taskEnvelope, null, null, null, null);
        var transition = new UpdateRoomTaskStatusRequest(
            RequestId, 1, "Done", null, null, resultEnvelope);
        Assert.Empty(ValidatorFor(create));
        Assert.Empty(ValidatorFor(transition));
        Assert.True(JsonSerializer.SerializeToUtf8Bytes(create).LongLength
            < RoomRequestLimits.EncryptedContentBodyBytes);
        Assert.True(JsonSerializer.SerializeToUtf8Bytes(transition).LongLength
            < RoomRequestLimits.EncryptedContentBodyBytes);

        var owner = UserId.New();
        var task = RoomTask.Create(
            create.TaskId,
            RoomId.From(Guid.NewGuid()),
            owner,
            create.Title,
            create.Description,
            null,
            null,
            DateTimeOffset.UtcNow);
        task.Claim(DateTimeOffset.UtcNow);
        task.Complete(transition.Result, owner, DateTimeOffset.UtcNow);
        var response = RoomResponseMappers.MapTask(task);
        Assert.Equal(taskEnvelope, response.Title);
        Assert.Equal(resultEnvelope, response.Result);
        Assert.Null(response.Description);
        Assert.Equal(owner.Value, response.ResultAuthorUserId);
    }

    [Fact]
    public void TaskRequestsEnforceEncryptedEnvelopeSizeLimitsAtHttpBoundary()
    {
        var maximum = new string('x', RoomInputRules.EncryptedContentMaxLength);
        var create = new CreateRoomTaskRequest(
            Guid.NewGuid(), maximum, null, null, null, null);
        var transition = new UpdateRoomTaskStatusRequest(
            RequestId, 1, "Done", null, null, maximum);
        Assert.Empty(ValidatorFor(create));
        Assert.Empty(ValidatorFor(transition));
        Assert.Empty(ValidatorFor(transition with { Result = null }));
        Assert.Contains(
            ValidatorFor(create with { Title = maximum + "x" }),
            failure => failure.MemberNames.Contains(nameof(CreateRoomTaskRequest.Title)));
        Assert.Contains(
            ValidatorFor(transition with { Result = maximum + "x" }),
            failure => failure.MemberNames.Contains(nameof(UpdateRoomTaskStatusRequest.Result)));
    }

    [Theory]
    [InlineData("")]
    [InlineData(" ")]
    public void TaskTitleStillRequiresAnEnvelope(string title)
    {
        var create = new CreateRoomTaskRequest(
            Guid.NewGuid(), title, null, null, null, null);
        Assert.Contains(
            ValidatorFor(create),
            failure => failure.MemberNames.Contains(nameof(CreateRoomTaskRequest.Title)));
        Assert.Throws<DomainException>(() => RoomTask.Create(
            create.TaskId,
            RoomId.From(Guid.NewGuid()),
            UserId.New(),
            create.Title,
            null,
            null,
            null,
            DateTimeOffset.UtcNow));
    }

    [Fact]
    public void TaskPagesExposeSnapshotAndPreserveFiltersInContinuationLink()
    {
        var response = new Microsoft.AspNetCore.Http.DefaultHttpContext().Response;
        var roomId = Guid.NewGuid();
        var assignee = Guid.NewGuid();
        var snapshot = new string('a', 64);
        RoomTaskEndpoints.ApplyReadPageHeaders(response, roomId,
            new RoomTaskReadPage([], true, 2, snapshot), RoomTaskStatus.Open, assignee, 2);

        Assert.Equal(snapshot, response.Headers["Kodosi-Task-Snapshot"]);
        Assert.Equal("true", response.Headers["Kodosi-Has-More"]);
        Assert.Equal("2", response.Headers["Kodosi-Next-Offset"]);
        Assert.Equal($"</api/rooms/{roomId:D}/tasks?offset=2&limit=2&snapshot={snapshot}&status=Open&assignee={assignee:D}>; rel=\"next\"",
            response.Headers.Link);

        var terminal = new Microsoft.AspNetCore.Http.DefaultHttpContext().Response;
        RoomTaskEndpoints.ApplyReadPageHeaders(terminal, roomId, new RoomTaskReadPage([], false, null, snapshot));
        Assert.Equal(snapshot, terminal.Headers["Kodosi-Task-Snapshot"]);
        Assert.Equal("false", terminal.Headers["Kodosi-Has-More"]);
        Assert.False(terminal.Headers.ContainsKey("Kodosi-Next-Offset"));
        Assert.False(terminal.Headers.ContainsKey("Link"));
    }

    private static string SizedRoomEnvelope(string contentKind, int recipientCount)
    {
        // The backend treats this payload as opaque. Preserve native crypto field sizes
        // without requiring platform ML-DSA support just to test transport validation.
        const int WrappedKeySize = 1 + 1088 + 12 + 32 + 16;
        var userId = Guid.NewGuid().ToString("D");
        var recipients = Enumerable.Range(0, recipientCount).Select(index => new
        {
            userId,
            deviceId = "device-" + index.ToString(System.Globalization.CultureInfo.InvariantCulture),
            wrappedKey = Convert.ToBase64String(new byte[WrappedKeySize]),
        }).ToArray();
        return JsonSerializer.Serialize(new
        {
            version = 2,
            roomId = Guid.NewGuid().ToString("D"),
            objectId = Guid.NewGuid().ToString("D"),
            contentKind,
            senderUserId = userId,
            senderDeviceId = "device-0",
            rosterGeneration = 1,
            issuedAtMs = 1_780_000_000_000L,
            counter = 1,
            recipients,
            ciphertext = Convert.ToBase64String(new byte[64 + 16]),
            signature = Convert.ToBase64String(new byte[MLDsaAlgorithm.MLDsa65.SignatureSizeInBytes]),
        });
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
