using Kodosi.Domain;

namespace Kodosi.DomainTests;

public sealed class SemanticRelayRequestTests
{
    [Fact]
    public void Exact_Match_Includes_Account_Device_Session_Incarnation_Mode_And_Fingerprint()
    {
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var userId = UserId.New();
        var requestId = Guid.CreateVersion7();
        var request = SemanticRelayRequest.Create(
            sessionId,
            incarnationId,
            userId,
            "device-1",
            requestId,
            "steer",
            new string('a', 64),
            DateTimeOffset.UnixEpoch);

        Assert.True(request.Matches(
            sessionId,
            incarnationId,
            userId,
            "device-1",
            "steer",
            new string('a', 64)));
        Assert.False(request.Matches(
            SessionId.New(),
            incarnationId,
            userId,
            "device-1",
            "steer",
            new string('a', 64)));
        Assert.False(request.Matches(
            sessionId,
            Guid.CreateVersion7(),
            userId,
            "device-1",
            "steer",
            new string('a', 64)));
        Assert.False(request.Matches(
            sessionId,
            incarnationId,
            UserId.New(),
            "device-1",
            "steer",
            new string('a', 64)));
        Assert.False(request.Matches(
            sessionId,
            incarnationId,
            userId,
            "device-2",
            "steer",
            new string('a', 64)));
        Assert.False(request.Matches(
            sessionId,
            incarnationId,
            userId,
            "device-1",
            "queue",
            new string('a', 64)));
        Assert.False(request.Matches(
            sessionId,
            incarnationId,
            userId,
            "device-1",
            "steer",
            new string('b', 64)));
    }

    [Fact]
    public void Acknowledgement_Requires_Stored_Receipt_And_Is_Idempotent()
    {
        var request = SemanticRelayRequest.Create(
            SessionId.New(),
            Guid.CreateVersion7(),
            UserId.New(),
            "device-1",
            Guid.CreateVersion7(),
            "queue",
            new string('a', 64),
            DateTimeOffset.UnixEpoch);

        Assert.Throws<DomainException>(() =>
            request.MarkAcknowledged(DateTimeOffset.UnixEpoch.AddSeconds(1)));

        request.MarkReceiptStored(DateTimeOffset.UnixEpoch.AddSeconds(2));
        request.MarkAcknowledged(DateTimeOffset.UnixEpoch.AddSeconds(3));
        var acknowledgedAt = request.UpdatedAt;
        request.MarkAcknowledged(DateTimeOffset.UnixEpoch.AddSeconds(4));

        Assert.Equal(SemanticRequestState.Acknowledged, request.State);
        Assert.Equal(acknowledgedAt, request.UpdatedAt);
    }

    [Theory]
    [InlineData("injected")]
    [InlineData("cancelled")]
    [InlineData("deliveryUnknown")]
    public void Receipt_Accepts_Only_Terminal_Outcomes(string outcome)
    {
        var receipt = SemanticRelayReceipt.Create(
            Guid.NewGuid(),
            SessionId.New(),
            Guid.CreateVersion7(),
            UserId.New(),
            "requester-device",
            Guid.CreateVersion7(),
            "stopAndSend",
            new string('a', 64),
            outcome,
            UserId.New(),
            "owner-device",
            "signature",
            DateTimeOffset.UnixEpoch);

        Assert.Equal(outcome, receipt.Outcome);
    }

    [Fact]
    public void Receipt_Rejects_Nonterminal_Outcome()
    {
        Assert.Throws<DomainException>(() => SemanticRelayReceipt.Create(
            Guid.NewGuid(),
            SessionId.New(),
            Guid.CreateVersion7(),
            UserId.New(),
            "requester-device",
            Guid.CreateVersion7(),
            "steer",
            new string('a', 64),
            "queued",
            UserId.New(),
            "owner-device",
            "signature",
            DateTimeOffset.UnixEpoch));
    }
}
