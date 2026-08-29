using Kodosi.Domain;

namespace Kodosi.Application;

public abstract record PendingHostFence
{
    public string FenceId { get; init; } = Guid.NewGuid().ToString("N");
    public virtual string? CoalesceKey => null;

    public sealed record KeyDistributionRequested : PendingHostFence
    {
        public override string CoalesceKey => "host.keyDistributionRequested";
    }

    public sealed record ParticipantChanged(
        UserId ParticipantUserId,
        int ParticipantCount,
        string Action) : PendingHostFence
    {
        public override string CoalesceKey => $"host.participantChanged:{ParticipantUserId.Value}:{Action}";
    }

    public sealed record AccessRevoked(UserId RevokedUserId) : PendingHostFence
    {
        public override string CoalesceKey => $"host.accessRevoked:{RevokedUserId.Value}";
    }
}
