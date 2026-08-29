using Kodosi.Domain;

namespace Kodosi.Application;

public sealed record SessionAccessMutationReceipt(
    Guid MutationId,
    Guid SessionId,
    Guid IncarnationId,
    string Kind,
    Guid? TargetUserId,
    AccessLevel? AccessLevel,
    DateTimeOffset? RequestedExpiresAt);

public sealed class SessionAccessMutationReceiptLookup(
    ISessionAccessMutationRepository mutations)
{
    public async Task<SessionAccessMutationReceipt?> FindAsync(
        UserId requesterUserId,
        SessionId sessionId,
        Guid incarnationId,
        Guid mutationId,
        CancellationToken ct = default)
    {
        var mutation = await mutations.GetAsync(requesterUserId, mutationId, ct);
        if (mutation is null
            || mutation.SessionId != sessionId
            || mutation.IncarnationId != incarnationId)
        {
            return null;
        }
        return new SessionAccessMutationReceipt(
            mutation.MutationId,
            mutation.SessionId.Value,
            mutation.IncarnationId,
            mutation.Kind switch
            {
                SessionAccessMutationKind.Grant => "grant",
                SessionAccessMutationKind.Revoke => "revoke",
                SessionAccessMutationKind.Leave => "leave",
                _ => throw new InvalidOperationException($"Unknown session access mutation kind: {mutation.Kind}."),
            },
            mutation.TargetUserId?.Value,
            mutation.AccessLevel,
            mutation.RequestedExpiresAt);
    }
}
