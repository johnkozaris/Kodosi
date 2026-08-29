using Kodosi.Domain;

namespace Kodosi.DomainTests;

public sealed class SemanticReceiptAckPreimageTests
{
    [Fact]
    public void Matches_Rust_Parity_Vector()
    {
        var preimage = SemanticReceiptAckPreimage.Create(
            SessionId.From(Guid.Parse("01900000-0000-7000-8000-000000000001")),
            Guid.Parse("01900000-0000-7000-8000-000000000002"),
            Guid.Parse("01900000-0000-7000-8000-000000000003"),
            UserId.From(Guid.Parse("01900000-0000-7000-8000-000000000004")),
            "device-α");

        Assert.Equal(
            "6b6f646f73692d73656d616e7469632d726563656970742d61636b2d76310000002430313930303030302d303030302d373030302d383030302d3030303030303030303030310000002430313930303030302d303030302d373030302d383030302d3030303030303030303030320000002430313930303030302d303030302d373030302d383030302d3030303030303030303030330000002430313930303030302d303030302d373030302d383030302d303030303030303030303034000000096465766963652dceb1",
            Convert.ToHexStringLower(preimage));
    }

    [Fact]
    public void Rejects_Empty_Identity_Fields()
    {
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var requestId = Guid.CreateVersion7();
        var userId = UserId.New();

        Assert.Throws<DomainException>(() => SemanticReceiptAckPreimage.Create(
            SessionId.From(Guid.Empty),
            incarnationId,
            requestId,
            userId,
            "device"));
        Assert.Throws<DomainException>(() => SemanticReceiptAckPreimage.Create(
            sessionId,
            incarnationId,
            requestId,
            UserId.From(Guid.Empty),
            "device"));
        Assert.Throws<DomainException>(() => SemanticReceiptAckPreimage.Create(
            sessionId,
            Guid.Empty,
            requestId,
            userId,
            "device"));
        Assert.Throws<DomainException>(() => SemanticReceiptAckPreimage.Create(
            sessionId,
            incarnationId,
            Guid.Empty,
            userId,
            "device"));
        Assert.Throws<DomainException>(() => SemanticReceiptAckPreimage.Create(
            sessionId,
            incarnationId,
            requestId,
            userId,
            " "));
    }
}
