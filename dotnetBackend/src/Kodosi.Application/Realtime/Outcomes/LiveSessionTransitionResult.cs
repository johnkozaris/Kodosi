
namespace Kodosi.Application;

public sealed record LiveSessionTransitionResult(
    SessionTransitionOutcome Outcome,
    SessionDiscoveryTarget? SharingState,
    DateTimeOffset? StartedAt = null);
