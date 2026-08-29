using Kodosi.Domain;

namespace Kodosi.DomainTests;

public sealed class SessionAccessMutationTests
{
    [Fact]
    public void Create_Accepts_Historical_UuidV4_Incarnation()
    {
        var incarnationId = Guid.NewGuid();

        var mutation = SessionAccessMutation.Create(
            UserId.New(),
            Guid.CreateVersion7(),
            SessionId.New(),
            incarnationId,
            SessionAccessMutationKind.Leave,
            targetUserId: null,
            accessLevel: null,
            requestedExpiresAt: null,
            DateTimeOffset.UtcNow);

        Assert.Equal(incarnationId, mutation.IncarnationId);
    }

    [Fact]
    public void Create_Requires_UuidV7_Mutation_Id()
    {
        Assert.Throws<DomainException>(() => SessionAccessMutation.Create(
            UserId.New(),
            Guid.NewGuid(),
            SessionId.New(),
            Guid.NewGuid(),
            SessionAccessMutationKind.Leave,
            targetUserId: null,
            accessLevel: null,
            requestedExpiresAt: null,
            DateTimeOffset.UtcNow));
    }

    [Theory]
    [InlineData(SessionAccessMutationKind.Grant)]
    [InlineData(SessionAccessMutationKind.Revoke)]
    [InlineData(SessionAccessMutationKind.Leave)]
    public void Create_Rejects_Fields_That_Do_Not_Match_Kind(
        SessionAccessMutationKind kind)
    {
        Assert.Throws<DomainException>(() => SessionAccessMutation.Create(
            UserId.New(),
            Guid.CreateVersion7(),
            SessionId.New(),
            Guid.NewGuid(),
            kind,
            targetUserId: null,
            accessLevel: AccessLevel.View,
            requestedExpiresAt: null,
            DateTimeOffset.UtcNow));
    }
}
