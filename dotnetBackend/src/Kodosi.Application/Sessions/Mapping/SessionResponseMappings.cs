using Kodosi.Domain;

namespace Kodosi.Application;

internal static class SessionResponseMappings
{
    public static SessionDetailResponse ToDetailResponse(
        this Session session,
        AccessLevel effectiveAccess)
    {
        return new SessionDetailResponse(
            session.Id.Value,
            session.IncarnationId,
            session.IncarnationGeneration,
            session.IncarnationProtocolVersion,
            session.OwnerUserId.Value,
            session.Title,
            session.ToolKind,
            session.Scope,
            session.RoomId?.Value,
            session.DefaultAccess,
            effectiveAccess,
            session.Status,
            session.StartedAt,
            session.EndedAt,
            session.LastHeartbeatAt);
    }

    public static SessionCardResponse ToCardResponse(
        this SessionCardProjection projection,
        AccessLevel access,
        ILiveSessionStateDirectory runtimes)
    {
        var participantCount = runtimes
            .TryGet(SessionId.From(projection.Id))
            ?.Demand.GetStreamDemand().ParticipantCount ?? 0;

        return new SessionCardResponse(
            projection.Id.ToString(),
            projection.Title,
            projection.Scope,
            access,
            projection.Status,
            projection.OwnerUserId.ToString(),
            projection.OwnerDisplayName,
            projection.OwnerAvatarUrl,
            participantCount,
            projection.StartedAt,
            projection.ToolKind,
            projection.RoomId);
    }
}
