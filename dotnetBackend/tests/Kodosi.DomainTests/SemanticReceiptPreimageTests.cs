using Kodosi.Domain;

namespace Kodosi.DomainTests;

public sealed class SemanticReceiptPreimageTests
{
    [Fact]
    public void Create_Uses_Canonical_Field_Order_And_Utf8_Lengths()
    {
        var sessionId = SessionId.From(Guid.Parse("01900000-0000-7000-8000-000000000001"));
        var incarnationId = Guid.Parse("01900000-0000-7000-8000-000000000002");
        var requestId = Guid.Parse("01900000-0000-7000-8000-000000000003");
        var userId = UserId.From(Guid.Parse("01900000-0000-7000-8000-000000000004"));

        var preimage = SemanticReceiptPreimage.Create(
            sessionId,
            incarnationId,
            requestId,
            "stopAndSend",
            new string('a', 64),
            "deliveryUnknown",
            userId,
            "requester-设备",
            userId,
            "owner-设备");

        using var expected = new MemoryStream();
        expected.Write(DomainTags.SemanticReceiptV1);
        Span<byte> length = stackalloc byte[sizeof(uint)];
        foreach (var field in new[]
        {
            sessionId.Value.ToString("D").ToLowerInvariant(),
            incarnationId.ToString("D").ToLowerInvariant(),
            requestId.ToString("D").ToLowerInvariant(),
            "stopAndSend",
            new string('a', 64),
            "deliveryUnknown",
            userId.Value.ToString("D").ToLowerInvariant(),
            "requester-设备",
            userId.Value.ToString("D").ToLowerInvariant(),
            "owner-设备",
        })
        {
            var bytes = System.Text.Encoding.UTF8.GetBytes(field);
            System.Buffers.Binary.BinaryPrimitives.WriteUInt32BigEndian(
                length,
                checked((uint)bytes.Length));
            expected.Write(length);
            expected.Write(bytes);
        }
        Assert.Equal(expected.ToArray(), preimage);
    }

    [Theory]
    [InlineData("bad", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "injected")]
    [InlineData("queue", "not-a-sha256", "injected")]
    [InlineData("queue", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "unknown")]
    public void Create_Rejects_Invalid_Wire_Metadata(
        string mode,
        string payloadSha256,
        string outcome)
    {
        Assert.Throws<DomainException>(() => SemanticReceiptPreimage.Create(
            SessionId.New(),
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            mode,
            payloadSha256,
            outcome,
            UserId.New(),
            "requester-device",
            UserId.New(),
            "owner-device"));
    }

    [Fact]
    public void Create_Rejects_Empty_Session_And_User_Identifiers()
    {
        Assert.Throws<DomainException>(() => SemanticReceiptPreimage.Create(
            SessionId.From(Guid.Empty),
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            "queue",
            new string('a', 64),
            "injected",
            UserId.New(),
            "requester-device",
            UserId.New(),
            "owner-device"));
        Assert.Throws<DomainException>(() => SemanticReceiptPreimage.Create(
            SessionId.New(),
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            "queue",
            new string('a', 64),
            "injected",
            UserId.From(Guid.Empty),
            "requester-device",
            UserId.New(),
            "owner-device"));
    }
}
