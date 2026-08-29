using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Endpoints;
using Kodosi.Host.Serialization;
using Microsoft.AspNetCore.Http;

namespace Kodosi.HostTests;

public sealed class RoomChatContractTests
{
    private const int RustBackendClientMaxJsonResponseBytes = 16 * 1024 * 1024;

    [Fact]
    public void Api_V10_Preserves_Directed_Chat_Recipients()
    {
        var sessionId = Guid.NewGuid();
        var userId = Guid.NewGuid();
        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web);
        HostJsonSerializerOptions.Configure(options);
        var request = JsonSerializer.Deserialize<PostRoomChatRequest>(
            $$"""
              {
                "messageId": "{{Guid.NewGuid()}}",
                "body": "encrypted-envelope",
                "authorKind": "Human",
                "recipientSessionIds": ["{{sessionId}}"],
                "recipientUserIds": ["{{userId}}"]
              }
              """,
            options);

        Assert.NotNull(request);
        Assert.Equal([sessionId], request.RecipientSessionIds);
        Assert.Equal([userId], request.RecipientUserIds);
    }

    [Theory]
    [InlineData(null, RoomChatReadPolicy.DefaultLimit)]
    [InlineData(-1, 1)]
    [InlineData(0, 1)]
    [InlineData(1, 1)]
    [InlineData(RoomChatReadPolicy.MaxLimit, RoomChatReadPolicy.MaxLimit)]
    [InlineData(RoomChatReadPolicy.MaxLimit + 1, RoomChatReadPolicy.MaxLimit)]
    [InlineData(int.MaxValue, RoomChatReadPolicy.MaxLimit)]
    public void Chat_Read_Limit_Is_Bounded_And_Rust_Compatible(
        int? requested,
        int expected)
    {
        Assert.True(RoomChatReadPolicy.MaxLimit >= 1000);
        Assert.Equal(expected, RoomChatEndpoints.NormalizeReadLimit(requested));
    }

    [Fact]
    public void Chat_Read_Page_Bounds_Maximum_Bodies_And_Provides_Sequence_Continuation()
    {
        var roomId = RoomId.From(Guid.NewGuid());
        var authorId = UserId.New();
        var maxBody = new string('x', RoomInputRules.EncryptedContentMaxLength);
        var candidates = Enumerable.Range(1, 3)
            .Select(seq => RoomChatMessage.Create(
                Guid.NewGuid(),
                roomId,
                authorId,
                null,
                RoomChatAuthorKind.Human,
                [],
                [],
                maxBody,
                seq))
            .ToList();

        var page = RoomChatReadPolicy.CreatePage(
            candidates,
            RoomChatReadPolicy.MaxLimit);

        Assert.Equal(2, page.Items.Count);
        Assert.True(page.HasMore);
        Assert.Equal(2, page.NextSince);
        Assert.Equal(
            RoomChatReadPolicy.MaxPageBodyCharacters,
            page.Items.Sum(message => message.Body.Length));
    }

    [Fact]
    public void Chat_Max_Body_Page_Serializes_Within_Response_Budget()
    {
        var roomId = RoomId.From(Guid.NewGuid());
        var authorId = UserId.New();
        var escapedMaxBody = new string('\u0080', RoomInputRules.EncryptedContentMaxLength);
        var candidates = Enumerable.Range(1, 3)
            .Select(seq => RoomChatMessage.Create(
                Guid.NewGuid(),
                roomId,
                authorId,
                null,
                RoomChatAuthorKind.Human,
                [],
                [],
                escapedMaxBody,
                seq))
            .ToList();
        var page = RoomChatReadPolicy.CreatePage(
            candidates,
            RoomChatReadPolicy.MaxLimit);
        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web);
        HostJsonSerializerOptions.Configure(options);

        var responseBytes = JsonSerializer.SerializeToUtf8Bytes(
            page.Items.Select(RoomResponseMappers.MapChat),
            options);

        Assert.True(page.HasMore);
        Assert.InRange(
            responseBytes.Length,
            1,
            RustBackendClientMaxJsonResponseBytes);
    }

    [Fact]
    public void Chat_Tail_Page_Returns_Newest_Bounded_Window_In_Display_Order()
    {
        var roomId = RoomId.From(Guid.NewGuid());
        var authorId = UserId.New();
        var descending = Enumerable.Range(1, 6)
            .Reverse()
            .Select(seq => RoomChatMessage.Create(
                Guid.NewGuid(),
                roomId,
                authorId,
                null,
                RoomChatAuthorKind.Human,
                [],
                [],
                "ciphertext",
                seq))
            .ToList();

        var page = RoomChatReadPolicy.CreateTailPage(descending, 3);

        Assert.Equal([4L, 5L, 6L], page.Items.Select(message => message.Seq));
        Assert.True(page.HasMore);
        Assert.Null(page.NextSince);
        Assert.Equal(4, page.NextBefore);
    }

    [Fact]
    public void Chat_Read_Continuation_Uses_Headers_Without_Breaking_Array_Response()
    {
        var context = new DefaultHttpContext();
        var roomId = Guid.NewGuid();
        var page = new RoomChatReadPage([], 42, null, HasMore: true);

        RoomChatEndpoints.ApplyReadPageHeaders(
            context.Response,
            roomId,
            RoomChatReadPolicy.MaxLimit,
            page);

        Assert.Equal("true", context.Response.Headers["Kodosi-Has-More"]);
        Assert.Equal("42", context.Response.Headers["Kodosi-Next-Since"]);
        Assert.Equal(
            $"</api/rooms/{roomId:D}/chat?since=42&limit={RoomChatReadPolicy.MaxLimit}>; rel=\"next\"",
            context.Response.Headers.Link);
    }

    [Fact]
    public void Chat_Tail_Continuation_Uses_Before_Header()
    {
        var context = new DefaultHttpContext();
        var roomId = Guid.NewGuid();
        var page = new RoomChatReadPage([], null, 42, HasMore: true);

        RoomChatEndpoints.ApplyReadPageHeaders(
            context.Response,
            roomId,
            RoomChatReadPolicy.MaxLimit,
            page);

        Assert.Equal("true", context.Response.Headers["Kodosi-Has-More"]);
        Assert.Equal("42", context.Response.Headers["Kodosi-Next-Before"]);
        Assert.Equal(
            $"</api/rooms/{roomId:D}/chat?before=42&limit={RoomChatReadPolicy.MaxLimit}>; rel=\"next\"",
            context.Response.Headers.Link);
    }

    [Fact]
    public void Missing_Recipient_Fields_Map_To_Explicit_Broadcast_Arrays()
    {
        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web);
        HostJsonSerializerOptions.Configure(options);
        var messageId = Guid.NewGuid();
        var request = JsonSerializer.Deserialize<PostRoomChatRequest>(
            $$"""
              {
                "messageId": "{{messageId}}",
                "body": "encrypted-envelope",
                "authorKind": "Human"
              }
              """,
            options);

        Assert.NotNull(request);
        Assert.Null(request.RecipientSessionIds);
        Assert.Null(request.RecipientUserIds);

        var message = RoomChatMessage.Create(
            messageId,
            RoomId.From(Guid.NewGuid()),
            UserId.New(),
            null,
            RoomChatAuthorKind.Human,
            request.RecipientSessionIds,
            request.RecipientUserIds,
            request.Body,
            1);
        using var document = JsonDocument.Parse(
            JsonSerializer.Serialize(RoomResponseMappers.MapChat(message), options));

        Assert.Equal(JsonValueKind.Array, document.RootElement.GetProperty("recipientSessionIds").ValueKind);
        Assert.Equal(0, document.RootElement.GetProperty("recipientSessionIds").GetArrayLength());
        Assert.Equal(JsonValueKind.Array, document.RootElement.GetProperty("recipientUserIds").ValueKind);
        Assert.Equal(0, document.RootElement.GetProperty("recipientUserIds").GetArrayLength());
    }
}
