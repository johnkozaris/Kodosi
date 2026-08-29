using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal static class SemanticReceiptWire
{
    public static RelayClientSendQueue.SemanticReceiptDelivery Delivery(
        SemanticRelayReceipt receipt) =>
        new(receipt.RequestId, WireMessage.Json(Encode(receipt)));

    public static RelayClientSendQueue.SemanticReceiptDelivery Delivery(
        ParticipantSemanticReceiptMessage receipt) =>
        new(receipt.RequestId, WireMessage.Json(RelayOutbound.Encode(receipt)));

    public static byte[] Encode(SemanticRelayReceipt receipt) =>
        RelayOutbound.Encode(new ParticipantSemanticReceiptMessage(
            receipt.SessionId.Value.ToString(),
            receipt.IncarnationId,
            receipt.RequestId,
            receipt.Mode switch
            {
                "queue" => RelaySemanticMode.Queue,
                "steer" => RelaySemanticMode.Steer,
                "stopAndSend" => RelaySemanticMode.StopAndSend,
                _ => throw new InvalidOperationException(
                    $"Stored semantic mode {receipt.Mode} is invalid."),
            },
            receipt.PayloadSha256,
            receipt.Outcome switch
            {
                "injected" => RelaySemanticOutcome.Injected,
                "cancelled" => RelaySemanticOutcome.Cancelled,
                "deliveryUnknown" => RelaySemanticOutcome.DeliveryUnknown,
                _ => throw new InvalidOperationException(
                    $"Stored semantic outcome {receipt.Outcome} is invalid."),
            },
            receipt.RequesterUserId.Value.ToString(),
            receipt.RequesterDeviceId,
            receipt.OwnerUserId.Value.ToString(),
            receipt.OwnerDeviceId,
            receipt.Signature));
}
