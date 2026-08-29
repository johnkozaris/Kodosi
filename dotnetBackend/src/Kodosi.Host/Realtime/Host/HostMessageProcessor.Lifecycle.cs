using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal sealed partial class HostMessageProcessor
{
    private static readonly TimeSpan HostEndPersistenceTimeout = TimeSpan.FromSeconds(30);

    private HostProcessOutcome HandleKeyRotation(
        byte[] rawBytes,
        HostMessageSource identity,
        ILiveSessionStreamCache stream)
    {
        var sessionId = identity.SessionId;
        using var document = JsonDocument.Parse(rawBytes);
        var root = document.RootElement;
        if (!HasMatchingSessionId(root, sessionId)
            || !TryGetRequiredInt32(root, "keyGeneration", out var keyGeneration)
            || keyGeneration < 0)
        {
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }



        var message = JsonSerializer.Deserialize(rawBytes, WsJsonContext.Default.KeyRotationMessage);

        if (message is null)
        {
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }

        if (message.KeyGeneration < 0)
        {
            _metrics.RecordKeyRotationRejection();
            _logger.LogWarning(
                "Rejected key.rotation for session {SessionId}: negative keyGeneration {KeyGen}",
                sessionId,
                message.KeyGeneration);
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }

        var rotationResult = stream.TryStoreKeyRotation((uint)message.KeyGeneration, rawBytes);
        if (rotationResult == LiveSessionKeyRotationStoreResult.Rejected)
        {

            _metrics.RecordKeyRotationRejection();
            _logger.LogWarning(
                "Rejected key.rotation for session {SessionId}: keyGeneration {KeyGen} is not strictly greater than current",
                sessionId,
                message.KeyGeneration);
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }

        if (rotationResult == LiveSessionKeyRotationStoreResult.DuplicateSameGeneration)
        {
            _logger.LogDebug(
                "Duplicate key.rotation for session {SessionId}: keyGeneration {KeyGen} already established",
                sessionId,
                message.KeyGeneration);
            return HostProcessOutcome.Continue;
        }

        if (!IsCurrent(identity)
            || !_broadcaster.BroadcastKeyRotationToAllDownstreamClientsIfSame(
                sessionId,
                identity.Queues,
                WireMessage.Json(rawBytes)))
        {
            return StaleHost();
        }
        _logger.LogInformation("Key rotation for session {SessionId}", sessionId);
        return HostProcessOutcome.Continue;
    }

    private async Task<HostProcessOutcome> HandleEndAsync(
        byte[] rawBytes,
        HostMessageSource identity)
    {
        var sessionId = identity.SessionId;
        if (!HasMatchingSessionId(rawBytes, sessionId))
        {
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }

        var end = JsonSerializer.Deserialize(rawBytes, WsJsonContext.Default.HostEndMessage);
        if (end is null || string.IsNullOrWhiteSpace(end.Reason))
        {
            _logger.LogWarning("Rejected malformed host.end for session {SessionId}: missing reason", sessionId);
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }

        var closeReason = CloseReasonWire.TryFromHostEndWire(end.Reason);
        if (closeReason is null)
        {
            _logger.LogWarning(
                "Rejected host.end for session {SessionId}: reason {Reason} is not host-authorable",
                sessionId,
                end.Reason);
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }

        _logger.LogInformation(
            "Host ended session {SessionId}: {Reason}",
            sessionId,
            end.Reason);

        SessionEndCoordinatorResult result;
        try
        {


            using var persistenceCts = new CancellationTokenSource(
                HostEndPersistenceTimeout);
            result = await _sessionEndCoordinator.EndAuthenticatedHostAsync(
                sessionId,
                identity.Runtime,
                identity.ConnectionId,
                identity.RuntimeIncarnationId,
                closeReason.Value,
                persistenceCts.Token);
            if (!result.Transition.Outcome.SatisfiesTargetState())
            {
                _logger.LogWarning(
                    "Host end for session {SessionId} did not persist cleanly because the outcome was {Outcome}",
                    sessionId,
                    result.Transition.Outcome);
            }
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Failed to end session {SessionId} in DB", sessionId);
            _logger.LogError(
                "Host end for session {SessionId} failed durable persistence; closing host socket without fanout",
                sessionId);
            return HostProcessOutcome.ProtocolViolation(CloseReason.ServerError);
        }

        if (!result.Transition.Outcome.SatisfiesTargetState())
        {
            _logger.LogError(
                "Host end for session {SessionId} failed durable persistence; closing host socket without fanout",
                sessionId);
            return HostProcessOutcome.ProtocolViolation(CloseReason.ServerError);
        }

        return HostProcessOutcome.End(closeReason.Value);
    }
}
