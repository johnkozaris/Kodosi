using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Endpoints;
using Microsoft.AspNetCore.Http;

namespace Kodosi.HostTests;

public sealed class RoomFeedEndpointTests
{
    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData("  ")]
    public void ToolKindParser_Treats_Missing_Or_Blank_As_Unfiltered(string? value)
    {
        var parsed = FeedQueryParsers.TryParseToolKind(value, out var toolKind);

        Assert.True(parsed);
        Assert.Null(toolKind);
    }

    [Theory]
    [InlineData("Terminal", ToolKind.Terminal)]
    [InlineData("claudecode", ToolKind.ClaudeCode)]
    public void ToolKindParser_Accepts_Defined_Values(
        string value,
        ToolKind expected)
    {
        var parsed = FeedQueryParsers.TryParseToolKind(value, out var toolKind);

        Assert.True(parsed);
        Assert.Equal(expected, toolKind);
    }

    [Theory]
    [InlineData("unknown")]
    [InlineData("999")]
    public void ToolKindParser_Rejects_Invalid_Nonblank_Values(string value)
    {
        var parsed = FeedQueryParsers.TryParseToolKind(value, out var toolKind);

        Assert.False(parsed);
        Assert.Null(toolKind);
    }

    [Fact]
    public async Task RoomFeedEndpoint_Returns_400_For_Invalid_Nonblank_ToolKind()
    {
        var result = await RoomFeedEndpoints.GetRoomFeedAsync(
            Guid.NewGuid(),
            cursor: null,
            limit: null,
            toolKind: "unknown",
            sort: null,
            since: null,
            CreateService(),
            UserId.New(),
            TestContext.Current.CancellationToken);

        Assert.Equal(
            StatusCodes.Status400BadRequest,
            Assert.IsAssignableFrom<IStatusCodeHttpResult>(result).StatusCode);
    }

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData("  ")]
    public async Task RoomFeedEndpoint_Leaves_Missing_Or_Blank_ToolKind_Unfiltered(
        string? toolKind)
    {
        var roomId = RoomId.From(Guid.NewGuid());
        var actorId = UserId.New();
        var result = await RoomFeedEndpoints.GetRoomFeedAsync(
            roomId.Value,
            cursor: null,
            limit: null,
            toolKind,
            sort: null,
            since: null,
            CreateService(roomId, actorId),
            actorId,
            TestContext.Current.CancellationToken);

        Assert.Equal(
            StatusCodes.Status200OK,
            Assert.IsAssignableFrom<IStatusCodeHttpResult>(result).StatusCode);
    }

    private static RoomSessionFeedService CreateService(
        RoomId? roomId = null,
        UserId? actorId = null) =>
        new(
            new FakeSessionRepository(),
            roomId is not null && actorId is not null
                ? new FakeRoomMemberRepository((roomId.Value, actorId.Value))
                : new FakeRoomMemberRepository(),
            new FakeAccessOverrideRepository(),
            new FakeSessionViewerDismissalRepository(),
            new FakeRuntimeDirectory());
}
