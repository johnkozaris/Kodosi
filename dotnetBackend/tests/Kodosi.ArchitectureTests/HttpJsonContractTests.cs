using System.Globalization;
using System.ComponentModel.DataAnnotations;
using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Endpoints;
using Kodosi.Host.Serialization;

namespace Kodosi.ArchitectureTests;

public class HttpJsonContractTests
{
    private static JsonSerializerOptions CreateOptions()
    {
        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web);
        HostJsonSerializerOptions.Configure(options);
        return options;
    }

    [Fact]
    public void CreateSessionRequest_Allows_String_Enums()
    {
        const string json = """
            {
              "id": "33333333-3333-3333-3333-333333333333",
              "idempotencyKey": "44444444-4444-4444-4444-444444444444",
              "title": "Pairing Session",
              "scope": "JustMe",
              "toolKind": "Terminal",
              "defaultAccess": "Suggest",
              "ownerSecret": "0123456789abcdef",
              "roomId": null
            }
            """;

        var request = JsonSerializer.Deserialize<CreateSessionRequest>(json, CreateOptions());

        Assert.NotNull(request);
        Assert.Equal(Guid.Parse("33333333-3333-3333-3333-333333333333"), request.Id);
        Assert.Equal(
            Guid.Parse("44444444-4444-4444-4444-444444444444"),
            request.IdempotencyKey);
        Assert.Equal(SessionScope.JustMe, request.Scope);
        Assert.Equal(ToolKind.Terminal, request.ToolKind);
        Assert.Equal(DefaultAudienceAccess.Suggest, request.DefaultAccess);
    }

    [Fact]
    public void CreateSessionRequest_Requires_UuidV7_IdempotencyKey()
    {
        var valid = new CreateSessionRequest(
            Guid.NewGuid(),
            Guid.CreateVersion7(),
            "Session",
            SessionScope.JustMe,
            ToolKind.Terminal,
            DefaultAudienceAccess.Suggest,
            "owner-secret",
            null);
        var invalid = valid with { IdempotencyKey = Guid.NewGuid() };

        Assert.Empty(Validate(valid));
        var error = Assert.Single(Validate(invalid));
        Assert.Equal([nameof(CreateSessionRequest.IdempotencyKey)], error.MemberNames);
    }

    private static IReadOnlyList<ValidationResult> Validate(object value)
    {
        var results = new List<ValidationResult>();
        Validator.TryValidateObject(value, new ValidationContext(value), results, true);
        return results;
    }

    [Fact]
    public void SessionCreationReceiptResponse_Uses_Identifiers_Only_Contract()
    {
        var response = new SessionCreationReceiptResponse(
            Guid.Parse("11111111-1111-7111-8111-111111111111"),
            Guid.Parse("22222222-2222-7222-8222-222222222222"),
            Guid.Parse("33333333-3333-7333-8333-333333333333"),
            7,
            2);

        using var document = JsonDocument.Parse(
            JsonSerializer.Serialize(response, CreateOptions()));
        Assert.Equal(
            [
                "sessionId",
                "createIdempotencyKey",
                "incarnationId",
                "generation",
                "protocolVersion",
            ],
            document.RootElement
                .EnumerateObject()
                .Select(property => property.Name)
                .ToArray());
    }

    [Fact]
    public void UpdateSessionRequest_Allows_String_Enums()
    {
        const string json = """
            {
              "expectedIncarnationId": "44444444-4444-4444-4444-444444444444",
              "title": "Renamed Session",
              "scope": "Friends",
              "defaultAccess": "Suggest"
            }
            """;

        var request = JsonSerializer.Deserialize<UpdateSessionRequest>(json, CreateOptions());

        Assert.NotNull(request);
        Assert.Equal(
            Guid.Parse("44444444-4444-4444-4444-444444444444"),
            request.ExpectedIncarnationId);
        Assert.Equal("Renamed Session", request.Title);
        Assert.Equal(SessionScope.Friends, request.Scope);
        Assert.Equal(DefaultAudienceAccess.Suggest, request.DefaultAccess);
    }

    [Theory]
    [InlineData("Inject")]
    [InlineData("Approve")]
    public void Session_Request_DefaultAccess_Rejects_Owner_Only_Levels(string access)
    {
        var createJson = $$"""
            {
              "id": "33333333-3333-3333-3333-333333333333",
              "idempotencyKey": "01900000-0000-7000-8000-000000000001",
              "title": "Session",
              "scope": "JustMe",
              "toolKind": "Terminal",
              "defaultAccess": "{{access}}",
              "ownerSecret": "0123456789abcdef",
              "roomId": null
            }
            """;
        var updateJson = $$"""
            {
              "expectedIncarnationId": "44444444-4444-4444-4444-444444444444",
              "defaultAccess": "{{access}}"
            }
            """;

        Assert.Throws<JsonException>(() =>
            JsonSerializer.Deserialize<CreateSessionRequest>(createJson, CreateOptions()));
        Assert.Throws<JsonException>(() =>
            JsonSerializer.Deserialize<UpdateSessionRequest>(updateJson, CreateOptions()));
    }

    [Fact]
    public void GrantAccessRequest_Allows_String_Enums()
    {
        const string json = """
            {
              "expectedIncarnationId": "44444444-4444-4444-4444-444444444444",
              "mutationId": "01900000-0000-7000-8000-000000000001",
              "actorUserId": "b9a08740-b24a-42c2-b6fc-fcf53fdc3f54",
              "accessLevel": "Suggest",
              "expiresAt": "2026-08-14T00:00:00Z"
            }
            """;

        var request = JsonSerializer.Deserialize<GrantAccessRequest>(json, CreateOptions());

        Assert.NotNull(request);
        Assert.Equal(
            Guid.Parse("44444444-4444-4444-4444-444444444444"),
            request.ExpectedIncarnationId);
        Assert.Equal(
            Guid.Parse("01900000-0000-7000-8000-000000000001"),
            request.MutationId);
        Assert.Equal(AccessLevel.Suggest, request.AccessLevel);
        Assert.Equal(
            DateTimeOffset.Parse(
                "2026-08-14T00:00:00Z",
                CultureInfo.InvariantCulture),
            request.ExpiresAt);
    }

    [Fact]
    public void Enum_Contracts_Reject_Integer_Payloads()
    {
        const string json = """
            {
              "id": "33333333-3333-3333-3333-333333333333",
              "idempotencyKey": "44444444-4444-4444-4444-444444444444",
              "title": "Pairing Session",
              "scope": 1,
              "toolKind": "Terminal",
              "defaultAccess": "Suggest",
              "ownerSecret": "0123456789abcdef"
            }
            """;

        Assert.Throws<JsonException>(() =>
            JsonSerializer.Deserialize<CreateSessionRequest>(json, CreateOptions()));
    }

    [Fact]
    public void SessionCardResponse_Serializes_Enums_As_Strings()
    {
        var response = new SessionCardResponse(
            "11111111-1111-1111-1111-111111111111",
            "Pairing Session",
            SessionScope.Friends,
            AccessLevel.Suggest,
            SessionStatus.Live,
            "22222222-2222-2222-2222-222222222222",
            "Owner",
            null,
            3,
            DateTimeOffset.Parse("2026-04-06T12:00:00+00:00", CultureInfo.InvariantCulture),
            ToolKind.Terminal,
            null);

        var json = JsonSerializer.Serialize(response, CreateOptions());
        var roundTrip = JsonSerializer.Deserialize<SessionCardResponse>(json, CreateOptions());

        Assert.Contains("\"scope\":\"Friends\"", json);
        Assert.Contains("\"access\":\"Suggest\"", json);
        Assert.Contains("\"status\":\"Live\"", json);
        Assert.Contains("\"toolKind\":\"Terminal\"", json);
        Assert.NotNull(roundTrip);
        Assert.Equal(SessionScope.Friends, roundTrip.Scope);
        Assert.Equal(AccessLevel.Suggest, roundTrip.Access);
        Assert.Equal(SessionStatus.Live, roundTrip.Status);
        Assert.Equal(ToolKind.Terminal, roundTrip.ToolKind);
    }

    [Fact]
    public void SessionDetailResponse_Serializes_Enums_As_Strings()
    {
        var response = new SessionDetailResponse(
            Guid.Parse("11111111-1111-1111-1111-111111111111"),
            Guid.Parse("22222222-2222-2222-2222-222222222222"),
            7,
            Session.CurrentIncarnationProtocolVersion,
            Guid.Parse("44444444-4444-4444-8444-444444444444"),
            "Pairing Session",
            ToolKind.ClaudeCode,
            SessionScope.Room,
            Guid.Parse("33333333-3333-3333-3333-333333333333"),
            AccessLevel.Suggest,
            AccessLevel.Inject,
            SessionStatus.Reconnecting,
            DateTimeOffset.Parse("2026-04-06T12:00:00+00:00", CultureInfo.InvariantCulture),
            null,
            DateTimeOffset.Parse("2026-04-06T12:05:00+00:00", CultureInfo.InvariantCulture));

        var json = JsonSerializer.Serialize(response, CreateOptions());
        var roundTrip = JsonSerializer.Deserialize<SessionDetailResponse>(json, CreateOptions());

        Assert.Contains("\"toolKind\":\"ClaudeCode\"", json);
        Assert.Contains(
            "\"incarnationId\":\"22222222-2222-2222-2222-222222222222\"",
            json);
        Assert.Contains("\"scope\":\"Room\"", json);
        Assert.Contains("\"defaultAccess\":\"Suggest\"", json);
        Assert.Contains("\"effectiveAccess\":\"Inject\"", json);
        Assert.Contains("\"status\":\"Reconnecting\"", json);
        Assert.NotNull(roundTrip);
        Assert.Equal(ToolKind.ClaudeCode, roundTrip.ToolKind);
        Assert.Equal(SessionScope.Room, roundTrip.Scope);
        Assert.Equal(AccessLevel.Suggest, roundTrip.DefaultAccess);
        Assert.Equal(AccessLevel.Inject, roundTrip.EffectiveAccess);
        Assert.Equal(SessionStatus.Reconnecting, roundTrip.Status);
    }
}
