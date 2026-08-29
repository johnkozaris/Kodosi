namespace Kodosi.Domain;

public static class DomainTags
{
    public static ReadOnlySpan<byte> DevicePopV1 => "kodosi-device-pop-v1"u8;
    public static ReadOnlySpan<byte> DeviceConnectionProofV1 =>
        "kodosi-device-connection-proof-v1"u8;
    public static ReadOnlySpan<byte> DeviceHttpRequestProofV1 =>
        "kodosi-device-http-request-proof-v1"u8;
    public static ReadOnlySpan<byte> DeviceCertV2 => "kodosi-device-cert-v2"u8;
    public static ReadOnlySpan<byte> DeviceListV1 => "kodosi-device-list-v1"u8;
    public static ReadOnlySpan<byte> SessionKeyV2 => "kodosi-session-key-v2"u8;
    public static ReadOnlySpan<byte> SessionKeyBlobV2 => "kodosi-session-key-blob-v2"u8;
    public static ReadOnlySpan<byte> ControlMessageV1 => "kodosi-control-message-v1"u8;
    public static ReadOnlySpan<byte> SemanticRequestV1 => "kodosi-semantic-request-v1"u8;
    public static ReadOnlySpan<byte> SemanticCancelV1 => "kodosi-semantic-cancel-v1"u8;
    public static ReadOnlySpan<byte> SemanticReceiptV1 => "kodosi-semantic-receipt-v1"u8;
    public static ReadOnlySpan<byte> SemanticReceiptAckV1 => "kodosi-semantic-receipt-ack-v1"u8;
    public static ReadOnlySpan<byte> RoomContentV2 => "kodosi-room-content-v2"u8;
    public static ReadOnlySpan<byte> RoomRosterV1 => "kodosi-room-roster-v1"u8;
    public static ReadOnlySpan<byte> RoomInvitationProposalV1 =>
        "kodosi-room-invitation-proposal-v1"u8;
    public static ReadOnlySpan<byte> RoomInvitationDecisionV1 =>
        "kodosi-room-invitation-decision-v1"u8;
}
