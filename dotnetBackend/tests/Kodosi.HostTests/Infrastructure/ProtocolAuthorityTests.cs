using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization.Metadata;
using Kodosi.Domain;
using Kodosi.Host.DependencyInjection;
using Kodosi.Host.Realtime;

using Kodosi.Host.Serialization;

namespace Kodosi.HostTests;

public sealed class ProtocolAuthorityTests
{
    [Fact]
    public void BackendApiAuthorityMatchesReportedContractVersion()
    {
        using var document = JsonDocument.Parse(
            File.ReadAllText(ResolveAuthorityPath("backend-api-authority.json")));
        var root = document.RootElement;

        var versions = EndpointRegistration.BuildHealthStatus("test");
        Assert.Equal(
            versions.ApiContractVersion,
            root.GetProperty("apiContractVersion").GetInt32());
        Assert.Equal(
            versions.AuthContractVersion,
            root.GetProperty("authContractVersion").GetInt32());
        var creation = root.GetProperty("sessionCreationReconciliation");
        Assert.Equal(
            "/api/sessions/{sessionId}/creation-receipts/{createIdempotencyKey}",
            creation.GetProperty("path").GetString());
        Assert.Equal("ownerOnly", creation.GetProperty("authorization").GetString());
        Assert.Equal(
            ["sessionId", "createIdempotencyKey"],
            creation.GetProperty("targetBinding")
                .EnumerateArray()
                .Select(element => element.GetString()!)
                .ToArray());
        Assert.Equal(
            [
                "sessionId",
                "createIdempotencyKey",
                "incarnationId",
                "generation",
                "protocolVersion",
            ],
            creation.GetProperty("responseFields")
                .EnumerateArray()
                .Select(element => element.GetString()!)
                .ToArray());
        Assert.Equal(
            ["nonOwner", "missingKey", "missingSession"],
            creation.GetProperty("notFoundPolicy")
                .EnumerateArray()
                .Select(element => element.GetString()!)
                .ToArray());
        Assert.Equal(
            ["ownerUserId", "mutationId"],
            root.GetProperty("sessionEnd")
                .GetProperty("receiptIdentity")
                .EnumerateArray()
                .Select(element => element.GetString()!)
                .ToArray());
        Assert.Equal(
            ["sessionId", "incarnationId"],
            root.GetProperty("sessionEnd")
                .GetProperty("receiptTargetBinding")
                .EnumerateArray()
                .Select(element => element.GetString()!)
                .ToArray());

        var accessMutations = root.GetProperty("sessionAccessMutations");
        Assert.Equal(
            ["requesterUserId", "mutationId"],
            accessMutations.GetProperty("receiptIdentity")
                .EnumerateArray()
                .Select(element => element.GetString()!)
                .ToArray());
        Assert.Equal(
            [
                "sessionId",
                "incarnationId",
                "kind",
                "targetUserId",
                "accessLevel",
                "requestedExpiresAt",
            ],
            accessMutations.GetProperty("receiptTargetBinding")
                .EnumerateArray()
                .Select(element => element.GetString()!)
                .ToArray());
        Assert.Equal(
            "/api/sessions/{sessionId}/access/mutations/{mutationId}",
            accessMutations.GetProperty("reconciliation").GetProperty("path").GetString());
        Assert.Equal(
            "SESSION_ACCESS_MUTATION_TARGET_CONFLICT",
            accessMutations.GetProperty("conflictCode").GetString());

        var roomMutations = root.GetProperty("roomMutations");
        Assert.Equal(
            ["authenticatedActorUserId", "operation", "requestId"],
            roomMutations.GetProperty("receiptIdentity")
                .EnumerateArray()
                .Select(element => element.GetString()!)
                .ToArray());
        Assert.Equal(
            "ROOM_MUTATION_TARGET_CONFLICT",
            roomMutations.GetProperty("conflictCode").GetString());
        Assert.Equal(
            "a matching receipt proves a terminal outcome; operation-specific result values distinguish success from terminal failure",
            roomMutations.GetProperty("receiptOutcomeSemantics").GetString());
        var lifecycleFailures = roomMutations.GetProperty("invitationLifecycleFailureResults");
        Assert.Equal("INVITATION_EXPIRED", lifecycleFailures.GetProperty("Expired").GetString());
        Assert.Equal(
            "INVITATION_SUPERSEDED",
            lifecycleFailures.GetProperty("Superseded").GetString());
        var roomReconciliation = roomMutations.GetProperty("reconciliation");
        Assert.Equal(
            "/api/rooms/mutations/{operation}/{requestId}",
            roomReconciliation.GetProperty("path").GetString());
        Assert.Equal(
            ["otherActor", "missingRequest"],
            roomReconciliation.GetProperty("notFoundPolicy")
                .EnumerateArray()
                .Select(element => element.GetString()!)
                .ToArray());
        Assert.Equal("pointInTimeUnknown", roomReconciliation.GetProperty("notFoundSemantics").GetString());
        Assert.False(roomReconciliation.GetProperty("notFoundTerminal").GetBoolean());
        Assert.Equal(
            "retain the exact prepared mutation and retry receipt lookup; never infer non-commit or resend from 404 alone",
            roomReconciliation.GetProperty("notFoundClientPolicy").GetString());
        Assert.Equal(
            [
                new KeyValuePair<string, string>("acceptInvitation", "acceptInvitation"),
                new KeyValuePair<string, string>("assignTask", "tasks.assign"),
                new KeyValuePair<string, string>("cancelInvitation", "cancelInvitation"),
                new KeyValuePair<string, string>("declineInvitation", "declineInvitation"),
                new KeyValuePair<string, string>("removeMember", "removeMember"),
                new KeyValuePair<string, string>("transitionTask", "tasks.transition"),
            ],
            roomMutations.GetProperty("operations")
                .EnumerateObject()
                .OrderBy(property => property.Name)
                .Select(property => new KeyValuePair<string, string>(
                    property.Name,
                    property.Value.GetProperty("operationToken").GetString()!))
                .ToArray());
        Assert.Equal(
            [
                "acceptInvitation",
                "assignTask",
                "cancelInvitation",
                "declineInvitation",
                "removeMember",
                "transitionTask",
            ],
            roomMutations.GetProperty("operations")
                .EnumerateObject()
                .Select(property => property.Name)
                .Order()
                .ToArray());
    }

    [Fact]
    public void SessionRelayAuthorityMatchesBackendWireContracts()
    {
        var authority = LoadSessionRelayAuthority();

        Assert.Equal(RelayProtocolVersions.Current, authority.RelayProtocolVersion);
        Assert.Equal("exactMatch", authority.CompatibilityPolicy);
        Assert.Equal("terminalClose", authority.UnknownMessagePolicy);
        Assert.Equal("terminalClose", authority.MalformedPayloadPolicy);
        Assert.Equal(authority.AccessLevels, Enum.GetNames<AccessLevel>());
        Assert.Equal(
            authority.SessionCapabilityBits.OrderBy(pair => pair.Key),
            BackendRelayCapabilityBitPositions().OrderBy(pair => pair.Key));
        Assert.Equal(
            authority.SessionCapabilityMasks.OrderBy(pair => pair.Key),
            BackendRelayCapabilityRoleMasks(pascalCase: true)
                .OrderBy(pair => pair.Key));
        Assert.Equal(
            authority.SessionStatuses,
            Enum.GetNames<SessionStatus>());
        Assert.Equal(
            authority.ParticipantActionStatuses,
            Enum.GetValues<RelayActionStatus>().Select(SerializeActionStatus).ToArray());
        Assert.Equal(
            BackendCloseReasons().OrderBy(static reason => reason),
            authority.CloseReasons.OrderBy(static reason => reason));
        Assert.Equal(
            BackendStreamDemandReasons().OrderBy(static reason => reason),
            authority.StreamDemandReasons.OrderBy(static reason => reason));
        Assert.All(
            authority.Messages.Values,
            message => Assert.Contains(message.Lane, authority.Lanes));
        Assert.Equal(
            authority.Lanes.OrderBy(static lane => lane),
            authority.Messages.Values
                .Select(static message => message.Lane)
                .Distinct(StringComparer.Ordinal)
                .OrderBy(static lane => lane));

        AssertMessages(
            authority,
            "participantToBackend",
            [
                (ExtractType(new ParticipantJoinMessage(7, 11, 13, 3, "device-1", Guid.NewGuid(), RelayProtocolVersions.Current), WsJsonContext.Default.ParticipantJoinMessage),
                    ExtractPropertyNames(new ParticipantJoinMessage(7, 11, 13, 3, "device-1", Guid.NewGuid(), RelayProtocolVersions.Current), WsJsonContext.Default.ParticipantJoinMessage)),
                ("participant.heartbeat", ["type"]),
                (ExtractType(new ParticipantSuggestMessage("action-123", "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantSuggestMessage),
                    ExtractPropertyNames(new ParticipantSuggestMessage("action-123", "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantSuggestMessage)),
                (ExtractType(new ParticipantInjectMessage("action-456", "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantInjectMessage),
                    ExtractPropertyNames(new ParticipantInjectMessage("action-456", "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantInjectMessage)),
                (ExtractType(new ParticipantResizeMessage("action-789", 24, 80, null, null, null, null, false, "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantResizeMessage),
                    ExtractPropertyNames(new ParticipantResizeMessage("action-789", 24, 80, null, null, null, null, false, "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantResizeMessage)),
                (ExtractType(new ParticipantFocusChangedMessage("action-321", true, "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantFocusChangedMessage),
                    ExtractPropertyNames(new ParticipantFocusChangedMessage("action-321", true, "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantFocusChangedMessage)),
                (ExtractType(new ParticipantPermissionDecisionMessage("action-654", "req-1", 7, "allow", "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantPermissionDecisionMessage),
                    ExtractPropertyNames(new ParticipantPermissionDecisionMessage("action-654", "req-1", 7, "allow", "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantPermissionDecisionMessage)),
                (ExtractType(new ParticipantStopMessage("action-stop", "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantStopMessage),
                    ExtractPropertyNames(new ParticipantStopMessage("action-stop", "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantStopMessage)),
                (ExtractType(new ParticipantInterruptMessage("action-interrupt", "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantInterruptMessage),
                    ExtractPropertyNames(new ParticipantInterruptMessage("action-interrupt", "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantInterruptMessage)),
                (ExtractType(new ParticipantSemanticSendMessage(Guid.Parse("01900000-0000-7000-8000-000000000001"), Guid.Parse("01900000-0000-7000-8000-000000000002"), RelaySemanticMode.Steer, new string('a', 64), "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantSemanticSendMessage),
                    ExtractPropertyNames(new ParticipantSemanticSendMessage(Guid.Parse("01900000-0000-7000-8000-000000000001"), Guid.Parse("01900000-0000-7000-8000-000000000002"), RelaySemanticMode.Steer, new string('a', 64), "nonce", "ciphertext", "signature"), WsJsonContext.Default.ParticipantSemanticSendMessage)),
                (ExtractType(new ParticipantSemanticCancelMessage(Guid.Parse("01900000-0000-7000-8000-000000000001"), Guid.Parse("01900000-0000-7000-8000-000000000002"), RelaySemanticMode.Steer, new string('a', 64), "signature"), WsJsonContext.Default.ParticipantSemanticCancelMessage),
                    ExtractPropertyNames(new ParticipantSemanticCancelMessage(Guid.Parse("01900000-0000-7000-8000-000000000001"), Guid.Parse("01900000-0000-7000-8000-000000000002"), RelaySemanticMode.Steer, new string('a', 64), "signature"), WsJsonContext.Default.ParticipantSemanticCancelMessage)),
                (ExtractType(new ParticipantSemanticReceiptAckMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000002"), Guid.Parse("01900000-0000-7000-8000-000000000001"), "user-1", "device-1", "signature"), WsJsonContext.Default.ParticipantSemanticReceiptAckMessage),
                    ExtractPropertyNames(new ParticipantSemanticReceiptAckMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000002"), Guid.Parse("01900000-0000-7000-8000-000000000001"), "user-1", "device-1", "signature"), WsJsonContext.Default.ParticipantSemanticReceiptAckMessage)),
            ]);

        AssertMessages(
            authority,
            "hostToBackend",
            [
                (ExtractType(new HostHelloMessage("session-123", "secret", "device-123", Guid.NewGuid(), RelayProtocolVersions.Current), WsJsonContext.Default.HostHelloMessage),
                    ExtractPropertyNames(new HostHelloMessage("session-123", "secret", "device-123", Guid.NewGuid(), RelayProtocolVersions.Current), WsJsonContext.Default.HostHelloMessage)),
                ("host.heartbeat", ["type", "sessionId"]),
                (ExtractType(new HostEndMessage("session-123", CloseReason.HostStopped.ToWire()), WsJsonContext.Default.HostEndMessage),
                    ExtractPropertyNames(new HostEndMessage("session-123", CloseReason.HostStopped.ToWire()), WsJsonContext.Default.HostEndMessage)),
                (ExtractType(new HostFenceAckMessage("session-123", "fence-1"), WsJsonContext.Default.HostFenceAckMessage),
                    ExtractPropertyNames(new HostFenceAckMessage("session-123", "fence-1"), WsJsonContext.Default.HostFenceAckMessage)),
                (ExtractType(new HostActionResultMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000002"), "action-1", "request-1", 7, "user-1", "device-1", RelayActionStatus.Accepted), WsJsonContext.Default.HostActionResultMessage),
                    ExtractPropertyNames(new HostActionResultMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000002"), "action-1", "request-1", 7, "user-1", "device-1", RelayActionStatus.Accepted), WsJsonContext.Default.HostActionResultMessage)),
                (ExtractType(new HostSemanticReceiptMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000002"), Guid.Parse("01900000-0000-7000-8000-000000000001"), RelaySemanticMode.Steer, new string('a', 64), RelaySemanticOutcome.Injected, "user-1", "device-1", "owner-1", "owner-device-1", "signature"), WsJsonContext.Default.HostSemanticReceiptMessage),
                    ExtractPropertyNames(new HostSemanticReceiptMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000002"), Guid.Parse("01900000-0000-7000-8000-000000000001"), RelaySemanticMode.Steer, new string('a', 64), RelaySemanticOutcome.Injected, "user-1", "device-1", "owner-1", "owner-device-1", "signature"), WsJsonContext.Default.HostSemanticReceiptMessage)),
            ]);

        AssertMessages(
            authority,
            "backendToHost",
            [
                (ExtractType(new HostAcceptedMessage("session-123", "epoch-1"), WsJsonContext.Default.HostAcceptedMessage),
                    ExtractPropertyNames(new HostAcceptedMessage("session-123", "epoch-1"), WsJsonContext.Default.HostAcceptedMessage)),
                (ExtractType(
                        new HostStreamDemandMessage("session-123", true, 1, 0, StreamDemandReason.SharedParticipantJoined.ToWire()),
                        WsJsonContext.Default.HostStreamDemandMessage),
                    ExtractPropertyNames(
                        new HostStreamDemandMessage("session-123", true, 1, 0, StreamDemandReason.SharedParticipantJoined.ToWire()),
                        WsJsonContext.Default.HostStreamDemandMessage)),
                (ExtractType(new HostKeyDistributionRequestedMessage("session-123", "fence-1"), WsJsonContext.Default.HostKeyDistributionRequestedMessage),
                    ExtractPropertyNames(new HostKeyDistributionRequestedMessage("session-123", "fence-1"), WsJsonContext.Default.HostKeyDistributionRequestedMessage)),
                (ExtractType(new HostStopMessage("session-123", "action-stop", "nonce", "ciphertext", "user-1", "device-1", "signature"), WsJsonContext.Default.HostStopMessage),
                    ExtractPropertyNames(new HostStopMessage("session-123", "action-stop", "nonce", "ciphertext", "user-1", "device-1", "signature"), WsJsonContext.Default.HostStopMessage)),
                (ExtractType(new HostInterruptMessage("session-123", "action-interrupt", "nonce", "ciphertext", "user-1", "device-1", "signature"), WsJsonContext.Default.HostInterruptMessage),
                    ExtractPropertyNames(new HostInterruptMessage("session-123", "action-interrupt", "nonce", "ciphertext", "user-1", "device-1", "signature"), WsJsonContext.Default.HostInterruptMessage)),
                (ExtractType(new HostInputMessage("session-123", "action-123", "nonce", "ciphertext", "user-1", "device-1", "signature"), WsJsonContext.Default.HostInputMessage),
                    ExtractPropertyNames(new HostInputMessage("session-123", "action-123", "nonce", "ciphertext", "user-1", "device-1", "signature"), WsJsonContext.Default.HostInputMessage)),
                (ExtractType(new HostResizeMessage("session-123", "action-789", 24, 80, null, null, null, null, false, "nonce", "ciphertext", "user-1", "device-1", "signature"), WsJsonContext.Default.HostResizeMessage),
                    ExtractPropertyNames(new HostResizeMessage("session-123", "action-789", 24, 80, null, null, null, null, false, "nonce", "ciphertext", "user-1", "device-1", "signature"), WsJsonContext.Default.HostResizeMessage)),
                (ExtractType(new HostFocusChangedMessage("session-123", "cmd-321", "client-7", false, "nonce", "ciphertext", "user-1", "device-1", "signature"), WsJsonContext.Default.HostFocusChangedMessage),
                    ExtractPropertyNames(new HostFocusChangedMessage("session-123", "cmd-321", "client-7", false, "nonce", "ciphertext", "user-1", "device-1", "signature"), WsJsonContext.Default.HostFocusChangedMessage)),
                (ExtractType(new HostParticipantDisconnectedMessage("session-123", "client-7"), WsJsonContext.Default.HostParticipantDisconnectedMessage),
                    ExtractPropertyNames(new HostParticipantDisconnectedMessage("session-123", "client-7"), WsJsonContext.Default.HostParticipantDisconnectedMessage)),
                (ExtractType(new HostSuggestionMessage("session-123", "suggestion-123", "nonce", "ciphertext", "user-1", "device-1", "signature"), WsJsonContext.Default.HostSuggestionMessage),
                    ExtractPropertyNames(new HostSuggestionMessage("session-123", "suggestion-123", "nonce", "ciphertext", "user-1", "device-1", "signature"), WsJsonContext.Default.HostSuggestionMessage)),
                (ExtractType(new HostParticipantChangedMessage("session-123", "fence-2", "user-123", 2, "joined"), WsJsonContext.Default.HostParticipantChangedMessage),
                    ExtractPropertyNames(new HostParticipantChangedMessage("session-123", "fence-2", "user-123", 2, "joined"), WsJsonContext.Default.HostParticipantChangedMessage)),
                (ExtractType(new HostAccessRevokedMessage("session-123", "fence-3", "user-456"), WsJsonContext.Default.HostAccessRevokedMessage),
                    ExtractPropertyNames(new HostAccessRevokedMessage("session-123", "fence-3", "user-456"), WsJsonContext.Default.HostAccessRevokedMessage)),
                (ExtractType(new HostPermissionDecisionMessage("session-123", "action-654", "req-1", 7, "allow", "user-9", "device-9", "nonce", "ciphertext", "signature"), WsJsonContext.Default.HostPermissionDecisionMessage),
                    ExtractPropertyNames(new HostPermissionDecisionMessage("session-123", "action-654", "req-1", 7, "allow", "user-9", "device-9", "nonce", "ciphertext", "signature"), WsJsonContext.Default.HostPermissionDecisionMessage)),
                (ExtractType(new HostSemanticSendMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000001"), Guid.Parse("01900000-0000-7000-8000-000000000002"), RelaySemanticMode.Steer, new string('a', 64), "nonce", "ciphertext", "user-1", "device-1", "signature"), WsJsonContext.Default.HostSemanticSendMessage),
                    ExtractPropertyNames(new HostSemanticSendMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000001"), Guid.Parse("01900000-0000-7000-8000-000000000002"), RelaySemanticMode.Steer, new string('a', 64), "nonce", "ciphertext", "user-1", "device-1", "signature"), WsJsonContext.Default.HostSemanticSendMessage)),
                (ExtractType(new HostSemanticCancelMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000001"), Guid.Parse("01900000-0000-7000-8000-000000000002"), RelaySemanticMode.Steer, new string('a', 64), "user-1", "device-1", "signature"), WsJsonContext.Default.HostSemanticCancelMessage),
                    ExtractPropertyNames(new HostSemanticCancelMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000001"), Guid.Parse("01900000-0000-7000-8000-000000000002"), RelaySemanticMode.Steer, new string('a', 64), "user-1", "device-1", "signature"), WsJsonContext.Default.HostSemanticCancelMessage)),
                (ExtractType(new HostActionResultAckMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000002"), "action-1", "user-1"), WsJsonContext.Default.HostActionResultAckMessage),
                    ExtractPropertyNames(new HostActionResultAckMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000002"), "action-1", "user-1"), WsJsonContext.Default.HostActionResultAckMessage)),
                (ExtractType(new HostSemanticReceiptAckMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000002"), Guid.Parse("01900000-0000-7000-8000-000000000001"), "user-1", "device-1"), WsJsonContext.Default.HostSemanticReceiptAckMessage),
                    ExtractPropertyNames(new HostSemanticReceiptAckMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000002"), Guid.Parse("01900000-0000-7000-8000-000000000001"), "user-1", "device-1"), WsJsonContext.Default.HostSemanticReceiptAckMessage)),
            ]);

        Assert.Equal(["requestId", "requestGeneration"], authority.Messages["action.result"].OptionalFields ?? []);

        AssertMessages(
            authority,
            "backendToParticipant",
            [
                (ExtractType(new ParticipantAcceptedMessage(
                        "session-123",
                        AccessLevel.View,
                        SessionCapabilities.FromAccess(AccessLevel.View).ToMask()),
                    WsJsonContext.Default.ParticipantAcceptedMessage),
                    ExtractPropertyNames(new ParticipantAcceptedMessage(
                        "session-123",
                        AccessLevel.View,
                        SessionCapabilities.FromAccess(AccessLevel.View).ToMask()),
                    WsJsonContext.Default.ParticipantAcceptedMessage)),
                (ExtractType(new ActionResultMessage("session-123", "action-123", RelayActionStatus.Accepted), WsJsonContext.Default.ActionResultMessage),
                    ExtractPropertyNames(new ActionResultMessage("session-123", "action-123", RelayActionStatus.Accepted), WsJsonContext.Default.ActionResultMessage)),
                (ExtractType(new SessionStatusMessage("session-123", SessionStatus.Live), WsJsonContext.Default.SessionStatusMessage),
                    ExtractPropertyNames(new SessionStatusMessage("session-123", SessionStatus.Live), WsJsonContext.Default.SessionStatusMessage)),
                (ExtractType(new SessionEndedMessage("session-123", CloseReason.SessionEnded.ToWire()), WsJsonContext.Default.SessionEndedMessage),
                    ExtractPropertyNames(new SessionEndedMessage("session-123", CloseReason.SessionEnded.ToWire()), WsJsonContext.Default.SessionEndedMessage)),
                (ExtractType(new SessionAccessRevokedMessage("session-123"), WsJsonContext.Default.SessionAccessRevokedMessage),
                    ExtractPropertyNames(new SessionAccessRevokedMessage("session-123"), WsJsonContext.Default.SessionAccessRevokedMessage)),
                (ExtractType(new ParticipantSemanticReceiptMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000002"), Guid.Parse("01900000-0000-7000-8000-000000000001"), RelaySemanticMode.Steer, new string('a', 64), RelaySemanticOutcome.Injected, "user-1", "device-1", "owner-1", "owner-device-1", "signature"), WsJsonContext.Default.ParticipantSemanticReceiptMessage),
                    ExtractPropertyNames(new ParticipantSemanticReceiptMessage("session-123", Guid.Parse("01900000-0000-7000-8000-000000000002"), Guid.Parse("01900000-0000-7000-8000-000000000001"), RelaySemanticMode.Steer, new string('a', 64), RelaySemanticOutcome.Injected, "user-1", "device-1", "owner-1", "owner-device-1", "signature"), WsJsonContext.Default.ParticipantSemanticReceiptMessage)),
            ]);

        AssertMessages(
            authority,
            "hostToBackendToParticipant",
            [
                (ExtractType(new KeyRotationMessage("session-123", 3), WsJsonContext.Default.KeyRotationMessage),
                    ExtractPropertyNames(new KeyRotationMessage("session-123", 3), WsJsonContext.Default.KeyRotationMessage)),
            ]);

        Assert.Equal(
            ["host_stopped"],
            authority.Messages["host.end"].ReasonValues ?? []);
        Assert.Equal("binary:0x03", authority.Messages["term.semanticCheckpoint"].Framing);
        Assert.Equal("binary:0x04", authority.Messages["term.rawBatch"].Framing);
        Assert.Equal("binary:0x05", authority.Messages["term.presentation"].Framing);
        AssertTerminalCounterAuthority(
            authority.Messages["term.semanticCheckpoint"],
            "semanticCheckpoint",
            "keyGeneration+semanticCheckpointCounterMonotonic; checkpointRevision+nextSequence remain monotonic across key generations");
        AssertTerminalCounterAuthority(
            authority.Messages["term.rawBatch"],
            "rawBatch",
            "keyGeneration+rawBatchCounterMonotonic; exactSequenceContiguous across key generations");
        AssertTerminalCounterAuthority(
            authority.Messages["term.presentation"],
            "presentation",
            "keyGeneration+presentationCounterMonotonic; presentationRevision remains monotonic across key generations");
    }

    [Fact]
    public void BackendInboundDispatchTablesMatchSessionRelayAuthority()
    {
        var authority = LoadSessionRelayAuthority();

        AssertDispatchTable(
            authority,
            [
                "key.rotation",
                "host.heartbeat",
                "host.end",
                "host.actionResult",
                "host.semanticReceipt",
                "host.fenceAck",
            ],
            ["hostToBackend", "hostToBackendToParticipant"],
            ["host.hello"]);
        AssertDispatchTable(
            authority,
            [
                "participant.semanticSend",
                "participant.semanticCancel",
                "participant.semanticReceiptAck",
                "participant.heartbeat",
                "participant.suggest",
                "participant.inject",
                "participant.resize",
                "participant.focusChanged",
                "participant.permissionDecision",
                "participant.stop",
                "participant.interrupt",
            ],
            ["participantToBackend"],
            ["participant.join"]);
    }

    [Fact]
    public void RelayMessageLimitsMatchSessionRelayAuthority()
    {
        var authority = LoadSessionRelayAuthority();
        var manifestLimits = authority.Messages.ToDictionary(
            pair => pair.Key,
            pair => pair.Value.MaxBytes,
            StringComparer.Ordinal);

        Assert.Equal(
            manifestLimits.OrderBy(static pair => pair.Key),
            RelayMessageLimits.MessageMaxBytes.OrderBy(static pair => pair.Key));
        foreach (var (messageType, maxBytes) in manifestLimits)
        {
            Assert.Equal(maxBytes, RelayMessageLimits.GetMaxBytes(messageType));
            Assert.True(RelayMessageLimits.IsWithinLimit(messageType, maxBytes));
            Assert.False(RelayMessageLimits.IsWithinLimit(messageType, maxBytes + 1));
        }
        Assert.Equal(
            authority.Messages["host.hello"].MaxBytes,
            RelayMessageLimits.GetMaxBytes("host.hello"));
        Assert.Equal(
            authority.Messages["participant.join"].MaxBytes,
            RelayMessageLimits.GetMaxBytes("participant.join"));
        Assert.Equal(
            RelayMessageLimits.HostReceiveMaxBytes,
            authority.Messages.Values.Max(message => message.MaxBytes));
    }

    [Fact]
    public void ParticipantToHostTransformLimitsUseDeclaredMessageAuthority()
    {
        var authority = LoadSessionRelayAuthority();

        var transforms = new Dictionary<string, string>(StringComparer.Ordinal)
        {
            ["participant.semanticSend"] = "host.semanticSend",
            ["participant.semanticCancel"] = "host.semanticCancel",
            ["participant.suggest"] = "host.suggestion",
            ["participant.inject"] = "host.input",
            ["participant.resize"] = "host.resize",
            ["participant.focusChanged"] = "host.focusChanged",
            ["participant.permissionDecision"] = "host.permissionDecision",
            ["participant.stop"] = "host.stop",
            ["participant.interrupt"] = "host.interrupt",
        };
        Assert.Equal(
            transforms.OrderBy(static pair => pair.Key),
            RelayMessageLimits.ParticipantToHostTransforms.OrderBy(static pair => pair.Key));
        Assert.Equal(
            authority.Messages
                .Where(pair => pair.Value.Direction == "participantToBackend")
                .Select(pair => pair.Key)
                .Except(["participant.join", "participant.heartbeat", "participant.semanticReceiptAck"])
                .OrderBy(type => type),
            transforms.Keys.OrderBy(type => type));
        foreach (var transform in transforms)
        {
            Assert.Equal(
                "participantToBackend",
                authority.Messages[transform.Key].Direction);
            Assert.Equal("backendToHost", authority.Messages[transform.Value].Direction);
            var hostLimit = authority.Messages[transform.Value].MaxBytes;
            Assert.True(RelayMessageLimits.IsTransformedParticipantOutputWithinLimit(
                transform.Key,
                hostLimit));
            Assert.False(RelayMessageLimits.IsTransformedParticipantOutputWithinLimit(
                transform.Key,
                hostLimit + 1));
        }
    }

    [Fact]
    public void PendingPermissionsUsesDedicatedEncryptedFrame()
    {
        var authority = LoadSessionRelayAuthority();
        Assert.DoesNotContain(authority.Messages.Keys, static k => k.StartsWith("hook.", StringComparison.Ordinal));

        var pending = authority.Messages["permission.pendingSnapshot"];
        Assert.Equal("pendingPermissions", pending.Lane);
        Assert.Equal("binary:0x06", pending.Framing);
        Assert.Contains("preserves the incarnation and snapshot-generation high-water mark", pending.Cache);
        Assert.Contains(
            "atomically purges queued prior-generation 0x03-0x06 ciphertext",
            authority.Messages["key.rotation"].Ordering);
    }

    [Fact]
    public void DeviceProofSecurityAuthorityIsConsumed()
    {
        var authority = LoadSessionRelayAuthority();

        Assert.Contains("fresh 32-byte server nonce", authority.Messages["device.proofChallenge"].Security);
        Assert.Contains("ML-DSA signature binds", authority.Messages["device.proof"].Security);
        Assert.Contains("relay protocol v10", authority.DeviceAuthentication.SessionRelays);
        Assert.Contains("purpose user-events", authority.DeviceAuthentication.UserEvents);
        Assert.Contains("exact request-body SHA-256", authority.DeviceAuthentication.DeviceScopedHttp);
        Assert.Contains("never sufficient actor authority", authority.DeviceAuthentication.CallerSelectedIdentity);
    }

    [Fact]
    public void CryptoDomainTagsMatchProtocolAuthority()
    {
        var authority = LoadDomainTagAuthority();
        Assert.Equal(7, authority.Version);
        Assert.Equal(16, authority.Tags.Count);

        Assert.Equal(
            authority.Tags["DEVICE_POP_V1"],
            Encoding.ASCII.GetString(DomainTags.DevicePopV1));
        Assert.Equal(
            authority.Tags["DEVICE_CERT_V2"],
            Encoding.ASCII.GetString(DomainTags.DeviceCertV2));
        Assert.Equal(
            authority.Tags["DEVICE_LIST_V1"],
            Encoding.ASCII.GetString(DomainTags.DeviceListV1));
        Assert.Equal(
            authority.Tags["DEVICE_CONNECTION_PROOF_V1"],
            Encoding.ASCII.GetString(DomainTags.DeviceConnectionProofV1));
        Assert.Equal(
            authority.Tags["DEVICE_HTTP_REQUEST_PROOF_V1"],
            Encoding.ASCII.GetString(DomainTags.DeviceHttpRequestProofV1));
        Assert.Equal(
            authority.Tags["SESSION_KEY_V2"],
            Encoding.ASCII.GetString(DomainTags.SessionKeyV2));
        Assert.Equal(
            authority.Tags["SESSION_KEY_BLOB_V2"],
            Encoding.ASCII.GetString(DomainTags.SessionKeyBlobV2));
        Assert.Equal(
            authority.Tags["CONTROL_MESSAGE_V1"],
            Encoding.ASCII.GetString(DomainTags.ControlMessageV1));
        Assert.Equal(
            authority.Tags["SEMANTIC_REQUEST_V1"],
            Encoding.ASCII.GetString(DomainTags.SemanticRequestV1));
        Assert.Equal(
            authority.Tags["SEMANTIC_CANCEL_V1"],
            Encoding.ASCII.GetString(DomainTags.SemanticCancelV1));
        Assert.Equal(
            authority.Tags["SEMANTIC_RECEIPT_V1"],
            Encoding.ASCII.GetString(DomainTags.SemanticReceiptV1));
        Assert.Equal(
            authority.Tags["SEMANTIC_RECEIPT_ACK_V1"],
            Encoding.ASCII.GetString(DomainTags.SemanticReceiptAckV1));
        Assert.Equal(
            authority.Tags["ROOM_CONTENT_V2"],
            Encoding.ASCII.GetString(DomainTags.RoomContentV2));
        Assert.Equal(
            authority.Tags["ROOM_ROSTER_V1"],
            Encoding.ASCII.GetString(DomainTags.RoomRosterV1));
        Assert.Equal(
            authority.Tags["ROOM_INVITATION_PROPOSAL_V1"],
            Encoding.ASCII.GetString(DomainTags.RoomInvitationProposalV1));
        Assert.Equal(
            authority.Tags["ROOM_INVITATION_DECISION_V1"],
            Encoding.ASCII.GetString(DomainTags.RoomInvitationDecisionV1));
    }

    [Fact]
    public void DeviceProofPreimagesMatchProtocolVectors()
    {
        var vectors = LoadDomainTagAuthority().DeviceProofPreimageVectors;
        var connection = vectors.ConnectionV1;
        var actualConnection = DeviceConnectionProofPreimage.Create(
            UserId.From(Guid.Parse(connection.UserId)),
            connection.DeviceId,
            connection.ConnectionId,
            connection.Purpose,
            connection.SessionId,
            Guid.Parse(connection.IncarnationId),
            Convert.FromHexString(connection.ChallengeHex));
        Assert.Equal(
            connection.PreimageHex,
            Convert.ToHexStringLower(actualConnection));

        var http = vectors.HttpV1;
        var actualHttp = DeviceHttpRequestProofPreimage.Create(
            UserId.From(Guid.Parse(http.UserId)),
            http.DeviceId,
            Guid.Parse(http.ChallengeId),
            http.Method,
            http.PathAndQuery,
            http.BodySha256,
            Convert.FromHexString(http.ChallengeHex));
        Assert.Equal(http.PreimageHex, Convert.ToHexStringLower(actualHttp));
    }

    [Fact]
    public void RoomInputRulesMatchProtocolAuthority()
    {
        var authority = LoadRoomInputRules();
        Assert.Equal(1, authority.Version);

        Assert.Equal(authority.TaskStatuses, Enum.GetNames<RoomTaskStatus>());
        Assert.Equal(RoomInputRules.RoomNameMaxLength, authority.Limits.RoomNameMaxLength);
        Assert.Equal(3, authority.Limits.RoomSlugMinLength);
        Assert.Equal(RoomInputRules.RoomSlugMaxLength, authority.Limits.RoomSlugMaxLength);
        Assert.Equal(RoomInputRules.RoomSlugPattern, authority.Limits.RoomSlugPattern);
        Assert.Equal(4000, authority.Limits.ChatBodyMaxLength);
        Assert.Equal(
            RoomInputRules.ChatRecipientMaxCount,
            authority.Limits.ChatRecipientMaxCount);
        Assert.Equal(RoomInputRules.TaskTitleMaxLength, authority.Limits.TaskTitleMaxLength);
        Assert.Equal(
            RoomInputRules.TaskDescriptionMaxLength,
            authority.Limits.TaskDescriptionMaxLength);
        Assert.Equal(RoomInputRules.TaskResultMaxLength, authority.Limits.TaskResultMaxLength);
    }

    [Fact]
    public void IdentityWireFormatMatchesProtocolAuthority()
    {
        var authority = LoadIdentityWireFormat();
        Assert.Equal(3, authority.Version);

        Assert.Equal(IdentityWireFormat.MaxFieldLength, authority.MaxFieldLength);
        Assert.Equal(IdentityWireFormat.MaxEntries, authority.MaxEntries);
        Assert.Equal(IdentityWireFormat.NoExpirySentinel, authority.NoExpirySentinel);
        Assert.Equal(
            IdentityWireFormat.MaxUnixTimeMilliseconds,
            authority.MaxUnixTimeMilliseconds);
        Assert.Equal(
            IdentityWireFormat.MaxDeviceCertificateBodyLength,
            authority.MaxDeviceCertificateBodyLength);
        Assert.Equal(
            IdentityWireFormat.MaxSignedDeviceListBodyLength,
            authority.MaxSignedDeviceListBodyLength);
        Assert.Equal("canonicalLowercaseUuid", authority.UserIdEncoding);
        Assert.Equal(IdentityWireFormat.UserIdLength, authority.UserIdLength);
        Assert.Equal(
            IdentityWireFormat.DeviceIdMaxUtf16CodeUnits,
            authority.DeviceIdMaxUtf16CodeUnits);
        Assert.Equal(
            IdentityWireFormat.DeviceLabelMaxUtf16CodeUnits,
            authority.DeviceLabelMaxUtf16CodeUnits);
        Assert.Equal(
            IdentityWireFormat.MlKem768PublicKeyLength,
            authority.MlKem768PublicKeyLength);
        Assert.Equal(
            IdentityWireFormat.MlDsa65PublicKeyLength,
            authority.MlDsa65PublicKeyLength);
        Assert.Equal(
            IdentityWireFormat.MlDsa65SignatureLength,
            authority.MlDsa65SignatureLength);
        Assert.Equal(
            "certificateProvenanceMayBeHistorical",
            authority.DeviceListEntrySignerSemantics);
        Assert.True(authority.DeviceListEnvelopeSignerMustBeActive);
    }

    [Fact]
    public void SessionScopesMatchDesktopRuntimeAuthority()
    {
        var runtimeScopes = LoadDesktopRuntimeSessionScopes();
        var backendScopesAsCamelCase = Enum.GetNames<SessionScope>()
            .Select(ToCamelCase)
            .ToArray();

        Assert.Equal(runtimeScopes, backendScopesAsCamelCase);
    }

    [Fact]
    public void RelayPermissionBitsAndRoleMasksMatchRustGeneratedAuthority()
    {
        var authority = LoadDesktopRuntimePermissionAuthority();
        Assert.Equal(
            authority.BitPositions.OrderBy(pair => pair.Key),
            BackendRelayCapabilityBitPositions().OrderBy(pair => pair.Key));
        Assert.Equal(
            authority.RoleMasks.OrderBy(pair => pair.Key),
            BackendRelayCapabilityRoleMasks().OrderBy(pair => pair.Key));
    }

    [Fact]
    public void UserEventsAuthorityMatchesBackendWireContracts()
    {
        var authority = LoadUserEventsAuthority();

        Assert.Equal("/me/events", authority.Channel);
        Assert.Equal("terminalClose", authority.UnknownMessagePolicy);
        Assert.Equal("terminalClose", authority.MalformedPayloadPolicy);

        string[] expectedOutgoingTypes =
        [
                ExtractType(
                    new DiscoveryInvalidatedMessage([DiscoverySurface.Friends]),
                    WsJsonContext.Default.DiscoveryInvalidatedMessage),
                ExtractType(
                    new UserDeviceListChangedMessage("user-123", 1),
                    WsJsonContext.Default.UserDeviceListChangedMessage),
                ExtractType(
                    new UserIdentityLifecycleChangedMessage(
                        "user-123",
                        2,
                        Guid.CreateVersion7(),
                        "enrolled",
                        1),
                    WsJsonContext.Default.UserIdentityLifecycleChangedMessage),
                ExtractType(
                    new UserDeviceLinkSnapshotMessage(
                    [new UserDeviceLinkSnapshotEntry(
                        "ABCD-EFGH",
                        "mbp",
                        DateTimeOffset.UnixEpoch)]),
                    WsJsonContext.Default.UserDeviceLinkSnapshotMessage),
                ExtractType(
                    new UserDeviceLinkRequestedMessage("ABCD-EFGH", "mbp", DateTimeOffset.UtcNow),
                    WsJsonContext.Default.UserDeviceLinkRequestedMessage),
                ExtractType(
                    new UserDeviceLinkResolvedMessage("ABCD-EFGH", "approved"),
                    WsJsonContext.Default.UserDeviceLinkResolvedMessage),
        ];
        Assert.Equal(expectedOutgoingTypes, authority.OutgoingTypes);

        Assert.Equal(
            authority.DiscoverySurfaces.OrderBy(static s => s),
            Enum.GetValues<DiscoverySurface>()
                .Select(SerializeDiscoverySurface)
                .OrderBy(static s => s));

        Assert.True(authority.MessageShapes.ContainsKey("discovery.invalidated"));
        Assert.True(authority.MessageShapes.ContainsKey("user.deviceListChanged"));
        Assert.True(authority.MessageShapes.ContainsKey("user.identityLifecycleChanged"));
        Assert.True(authority.MessageShapes.ContainsKey("user.deviceLinkSnapshot"));
        Assert.True(authority.MessageShapes.ContainsKey("user.deviceLinkRequested"));
        Assert.True(authority.MessageShapes.ContainsKey("user.deviceLinkResolved"));

        Assert.Equal(
            authority.MessageShapes["discovery.invalidated"].Fields.OrderBy(static f => f),
            ExtractPropertyNames(
                new DiscoveryInvalidatedMessage([DiscoverySurface.Friends], Guid.NewGuid().ToString()),
                WsJsonContext.Default.DiscoveryInvalidatedMessage)
                .OrderBy(static f => f));

        Assert.Equal(
            authority.MessageShapes["user.deviceListChanged"].Fields.OrderBy(static f => f),
            ExtractPropertyNames(
                new UserDeviceListChangedMessage("user-123", 1),
                WsJsonContext.Default.UserDeviceListChangedMessage)
                .OrderBy(static f => f));

        Assert.Equal(
            authority.MessageShapes["user.identityLifecycleChanged"].Fields.OrderBy(static f => f),
            ExtractPropertyNames(
                new UserIdentityLifecycleChangedMessage(
                    "user-123",
                    2,
                    Guid.CreateVersion7(),
                    "enrolled",
                    1),
                WsJsonContext.Default.UserIdentityLifecycleChangedMessage)
                .OrderBy(static f => f));

        var snapshot = new UserDeviceLinkSnapshotMessage(
            [new UserDeviceLinkSnapshotEntry("ABCD-EFGH", "mbp", DateTimeOffset.UnixEpoch)]);
        Assert.Equal(
            authority.MessageShapes["user.deviceLinkSnapshot"].Fields.OrderBy(static f => f),
            ExtractPropertyNames(snapshot, WsJsonContext.Default.UserDeviceLinkSnapshotMessage)
                .OrderBy(static f => f));
        var snapshotJson = SerializeToElement(
            snapshot,
            WsJsonContext.Default.UserDeviceLinkSnapshotMessage);
        Assert.Equal(
            authority.MessageShapes["user.deviceLinkSnapshot"].RequestFields!
                .OrderBy(static f => f),
            snapshotJson.GetProperty("requests")[0]
                .EnumerateObject()
                .Select(static property => property.Name)
                .OrderBy(static f => f));

        Assert.Equal(
            authority.MessageShapes["user.deviceLinkRequested"].Fields.OrderBy(static f => f),
            ExtractPropertyNames(
                new UserDeviceLinkRequestedMessage("ABCD-EFGH", "mbp", DateTimeOffset.UtcNow),
                WsJsonContext.Default.UserDeviceLinkRequestedMessage)
                .OrderBy(static f => f));

        Assert.Equal(
            authority.MessageShapes["user.deviceLinkResolved"].Fields.OrderBy(static f => f),
            ExtractPropertyNames(
                new UserDeviceLinkResolvedMessage("ABCD-EFGH", "approved"),
                WsJsonContext.Default.UserDeviceLinkResolvedMessage)
                .OrderBy(static f => f));
    }

    [Fact]
    public void HostEndDeclaresTheFailClosedDurabilityModeTheBackendImplements()
    {
        var authority = LoadSessionRelayAuthority();








        Assert.Equal(
            "closeHostWithoutFanout",
            authority.Messages["host.end"].DurabilityFailureMode);
    }

    [Fact]
    public void FocusChangedDeclaresTheCapabilityBothSidesEnforce()
    {
        var authority = LoadSessionRelayAuthority();

        Assert.Equal(
            "focus",
            authority.Messages["host.focusChanged"].RequiredCapability);
        Assert.Contains("focus", authority.SessionCapabilityBits.Keys);
    }

    [Fact]
    public void NoUnauthenticatedHostMessageCanAssertFocus()
    {
        var authority = LoadSessionRelayAuthority();

        var focusAsserting = authority.Messages
            .Where(pair => pair.Value.Direction == "backendToHost"
                && pair.Value.RequiredFields.Contains("focused"))
            .ToArray();

        Assert.NotEmpty(focusAsserting);
        foreach (var (type, message) in focusAsserting)
        {
            Assert.True(
                message.RequiredFields.Contains("senderUserId")
                    && message.RequiredFields.Contains("senderDeviceId")
                    && message.RequiredFields.Contains("signature"),
                $"{type} asserts focus but is not device-authenticated.");
        }


        Assert.DoesNotContain(
            "focused",
            authority.Messages["host.participantDisconnected"].RequiredFields);
    }

    private static SessionRelayAuthority LoadSessionRelayAuthority()
    {
        var json = File.ReadAllText(ResolveAuthorityPath("session-relay-authority.json"));
        return JsonSerializer.Deserialize<SessionRelayAuthority>(
            json,
            new JsonSerializerOptions
            {
                PropertyNameCaseInsensitive = true,
            })
            ?? throw new InvalidOperationException("session-relay-authority.json should deserialize.");
    }

    private static UserEventsAuthority LoadUserEventsAuthority()
    {
        var json = File.ReadAllText(ResolveAuthorityPath("user-events-authority.json"));
        return JsonSerializer.Deserialize<UserEventsAuthority>(
            json,
            new JsonSerializerOptions
            {
                PropertyNameCaseInsensitive = true,
            })
            ?? throw new InvalidOperationException("user-events-authority.json should deserialize.");
    }

    private static DomainTagAuthority LoadDomainTagAuthority()
    {
        var json = File.ReadAllText(ResolveAuthorityPath("crypto-domain-tags.json"));
        return JsonSerializer.Deserialize<DomainTagAuthority>(
            json,
            new JsonSerializerOptions
            {
                PropertyNameCaseInsensitive = true,
            })
            ?? throw new InvalidOperationException("crypto-domain-tags.json should deserialize.");
    }

    private static RoomInputRulesAuthority LoadRoomInputRules()
    {
        var json = File.ReadAllText(ResolveAuthorityPath("room-input-rules.json"));
        return JsonSerializer.Deserialize<RoomInputRulesAuthority>(
            json,
            new JsonSerializerOptions
            {
                PropertyNameCaseInsensitive = true,
            })
            ?? throw new InvalidOperationException("room-input-rules.json should deserialize.");
    }

    private static IdentityWireFormatAuthority LoadIdentityWireFormat()
    {
        var json = File.ReadAllText(ResolveAuthorityPath("identity-wire-format.json"));
        return JsonSerializer.Deserialize<IdentityWireFormatAuthority>(
            json,
            new JsonSerializerOptions
            {
                PropertyNameCaseInsensitive = true,
            })
            ?? throw new InvalidOperationException("identity-wire-format.json should deserialize.");
    }

    private static string[] LoadDesktopRuntimeSessionScopes()
    {
        var json = File.ReadAllText(ResolveAuthorityPath("desktop-runtime-authority.json"));
        using var doc = JsonDocument.Parse(json);
        return doc.RootElement
            .GetProperty("sessionScopes")
            .EnumerateArray()
            .Select(element => element.GetString()
                ?? throw new InvalidOperationException("sessionScopes entry should be a string."))
            .ToArray();
    }

    private static DesktopRuntimePermissionAuthority
        LoadDesktopRuntimePermissionAuthority()
    {
        var json = File.ReadAllText(
            ResolveAuthorityPath("desktop-runtime-authority.json"));
        using var document = JsonDocument.Parse(json);
        return new DesktopRuntimePermissionAuthority(
            ReadIntDictionary(
                document.RootElement.GetProperty(
                    "relayPermissionBitPositions")),
            ReadIntDictionary(
                document.RootElement.GetProperty(
                    "relayPermissionRoleMasks")));
    }

    private static Dictionary<string, int> ReadIntDictionary(
        JsonElement element) =>
        element.EnumerateObject().ToDictionary(
            property => property.Name,
            property => property.Value.GetInt32(),
            StringComparer.Ordinal);

    private static string ToCamelCase(string value)
        => string.IsNullOrEmpty(value)
            ? value
            : char.ToLowerInvariant(value[0]) + value[1..];

    private static string ResolveAuthorityPath(string fileName)
        => Path.GetFullPath(
            Path.Combine(
                AppContext.BaseDirectory,
                "..",
                "..",
                "..",
                "..",
                "..",
                "..",
                "protocol",
                fileName));

    private static string SerializeDiscoverySurface(DiscoverySurface surface)
    {
        var element = SerializeToElement(
            new DiscoveryInvalidatedMessage([surface]),
            WsJsonContext.Default.DiscoveryInvalidatedMessage);
        return element.GetProperty("surfaces")[0].GetString()
            ?? throw new InvalidOperationException("discovery surface should serialize.");
    }

    private static string SerializeActionStatus(RelayActionStatus status)
    {
        var element = SerializeToElement(
            new ActionResultMessage("session-123", "action-123", status),
            WsJsonContext.Default.ActionResultMessage);
        return element.GetProperty("status").GetString()
            ?? throw new InvalidOperationException("action.result.status should serialize.");
    }

    private static string ExtractType<T>(T payload, JsonTypeInfo<T> typeInfo)
    {
        var element = SerializeToElement(payload, typeInfo);
        return element.GetProperty("type").GetString()
            ?? throw new InvalidOperationException("Serialized payload should include type.");
    }

    private static string[] ExtractPropertyNames<T>(T payload, JsonTypeInfo<T> typeInfo)
    {
        var element = SerializeToElement(payload, typeInfo);
        return element.EnumerateObject().Select(static property => property.Name).ToArray();
    }

    private static string[] BackendCloseReasons()
        => Enum.GetValues<CloseReason>()
            .Select(static reason => reason.ToWire())
            .ToArray();

    private static string[] BackendStreamDemandReasons()
        => Enum.GetValues<StreamDemandReason>()
            .Select(static reason => reason.ToWire())
            .ToArray();

    private static Dictionary<string, int>
        BackendRelayCapabilityBitPositions() =>
        Enum.GetValues<SessionCapability>()
            .ToDictionary(
                capability => ToCamelCase(capability.ToString()),
                capability => (int)capability,
                StringComparer.Ordinal);

    private static Dictionary<string, int>
        BackendRelayCapabilityRoleMasks(bool pascalCase = false) =>
        new(StringComparer.Ordinal)
        {
            [pascalCase ? "View" : "view"] =
                SessionCapabilities.FromAccess(AccessLevel.View).ToMask(),
            [pascalCase ? "Suggest" : "suggest"] =
                SessionCapabilities.FromAccess(AccessLevel.Suggest).ToMask(),
            [pascalCase ? "Inject" : "inject"] =
                SessionCapabilities.FromAccess(AccessLevel.Inject).ToMask(),
            [pascalCase ? "Approve" : "approve"] =
                SessionCapabilities.FromAccess(AccessLevel.Approve).ToMask(),
            [pascalCase ? "Owner" : "owner"] = SessionCapabilities.FromAccess(
                AccessLevel.View,
                isOwnerParticipant: true).ToMask(),
        };

    private static void AssertTerminalCounterAuthority(
        RelayMessageAuthority message,
        string frameKind,
        string ordering)
    {
        Assert.Equal(ordering, message.Ordering);
        Assert.Equal(
            $"sessionId+keyGeneration+{frameKind}+counter",
            message.IdempotencyKey);
        Assert.Contains($"{frameKind} HKDF subkey", message.CounterDomain);
        Assert.Contains("independent", message.CounterDomain);
    }

    private static void AssertMessages(
        SessionRelayAuthority authority,
        string direction,
        (string Type, string[] Fields)[] contracts)
    {
        var manifestMessages = authority.Messages
            .Where(pair => pair.Value.Direction == direction && pair.Value.Framing == "json")
            .Select(pair => pair.Key)
            .ToArray();
        Assert.Equal(contracts.Select(static contract => contract.Type).ToArray(), manifestMessages);

        foreach (var (type, fields) in contracts)
        {
            Assert.Equal(
                authority.Messages[type].RequiredFields.OrderBy(static field => field),
                fields.OrderBy(static field => field));
        }
    }

    private static void AssertDispatchTable(
        SessionRelayAuthority authority,
        IEnumerable<string> dispatchTypes,
        string[] manifestDirections,
        string[] handshakeTypes)
    {
        var expected = authority.Messages
            .Where(pair => pair.Value.Framing == "json" && manifestDirections.Contains(pair.Value.Direction))
            .Select(pair => pair.Key)
            .Except(handshakeTypes)
            .OrderBy(static type => type)
            .ToArray();
        var actual = dispatchTypes.OrderBy(static type => type).ToArray();

        Assert.Equal(expected, actual);

        foreach (var handshakeType in handshakeTypes)
        {
            Assert.Contains(handshakeType, authority.Messages.Keys);
            Assert.DoesNotContain(handshakeType, actual);
        }
    }

    private static JsonElement SerializeToElement<T>(T payload, JsonTypeInfo<T> typeInfo)
    {
        using var document = JsonDocument.Parse(JsonSerializer.Serialize(payload, typeInfo));
        return document.RootElement.Clone();
    }

    public sealed record SessionRelayAuthority(
        int RelayProtocolVersion,
        string CompatibilityPolicy,
        string PayloadAuthority,
        string UnknownMessagePolicy,
        string MalformedPayloadPolicy,
        Dictionary<string, RelayMessageAuthority> Messages,
        string[] AccessLevels,
        Dictionary<string, int> SessionCapabilityBits,
        Dictionary<string, int> SessionCapabilityMasks,
        string[] SessionStatuses,
        string[] ParticipantActionStatuses,
        string[] Lanes,
        string[] CloseReasons,
        string[] StreamDemandReasons,
        DeviceAuthenticationAuthority DeviceAuthentication);

    public sealed record DeviceAuthenticationAuthority(
        string SessionRelays,
        string UserEvents,
        string DeviceScopedHttp,
        string CallerSelectedIdentity);

    public sealed record RelayMessageAuthority(
        string Direction,
        string Lane,
        string Framing,
        int MaxBytes,
        string[] RequiredFields,
        string Ordering,
        string? CounterDomain,
        string? IdempotencyKey,
        string? DurabilityFailureMode,
        bool Droppable,
        bool Coalesceable,
        string? Security = null,
        string[]? ReasonValues = null,
        string? RequiredCapability = null,
        string[]? OptionalFields = null,
        string? Cache = null);

    public sealed record UserEventsAuthority(
        string Channel,
        string Description,
        string UnknownMessagePolicy,
        string MalformedPayloadPolicy,
        string[] OutgoingTypes,
        string[] DiscoverySurfaces,
        Dictionary<string, UserEventMessageShape> MessageShapes);

    public sealed record UserEventMessageShape(
        string[] Fields,
        string Notes,
        string[]? RequestFields = null);

    public sealed record DomainTagAuthority(
        int Version,
        Dictionary<string, string> Tags,
        DeviceProofPreimageVectors DeviceProofPreimageVectors);

    public sealed record DeviceProofPreimageVectors(
        ConnectionProofVector ConnectionV1,
        HttpProofVector HttpV1);

    public sealed record ConnectionProofVector(
        string UserId,
        string DeviceId,
        string ConnectionId,
        string Purpose,
        string SessionId,
        string IncarnationId,
        string ChallengeHex,
        string PreimageHex);

    public sealed record HttpProofVector(
        string UserId,
        string DeviceId,
        string ChallengeId,
        string Method,
        string PathAndQuery,
        string BodySha256,
        string ChallengeHex,
        string PreimageHex);

    public sealed record RoomInputRulesAuthority(
        int Version,
        string[] TaskStatuses,
        RoomLimitsAuthority Limits);

    public sealed record RoomLimitsAuthority(
        int RoomNameMaxLength,
        int RoomSlugMinLength,
        int RoomSlugMaxLength,
        string RoomSlugPattern,
        int ChatBodyMaxLength,
        int ChatRecipientMaxCount,
        int TaskTitleMaxLength,
        int TaskDescriptionMaxLength,
        int TaskResultMaxLength);

    public sealed record IdentityWireFormatAuthority(
        int Version,
        uint MaxFieldLength,
        uint MaxEntries,
        ulong NoExpirySentinel,
        long MaxUnixTimeMilliseconds,
        int MaxDeviceCertificateBodyLength,
        int MaxSignedDeviceListBodyLength,
        string UserIdEncoding,
        int UserIdLength,
        int DeviceIdMaxUtf16CodeUnits,
        int DeviceLabelMaxUtf16CodeUnits,
        int MlKem768PublicKeyLength,
        int MlDsa65PublicKeyLength,
        int MlDsa65SignatureLength,
        string DeviceListEntrySignerSemantics,
        bool DeviceListEnvelopeSignerMustBeActive);

    private sealed record DesktopRuntimePermissionAuthority(
        Dictionary<string, int> BitPositions,
        Dictionary<string, int> RoleMasks);
}
