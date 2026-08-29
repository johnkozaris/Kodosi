using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class RoomMutationPolicyTests
{
    private static readonly Guid RequestId =
        Guid.Parse("0198f169-7c50-7000-8000-000000000001");

    [Fact]
    public void FingerprintsBindTargetsButStoreOnlyDigestsOfOpaqueBodies()
    {
        var roomId = RoomId.From(Guid.NewGuid());
        var memberId = UserId.New();
        var first = RoomMutationTargetFingerprint.RemoveMember(
            roomId,
            memberId,
            7,
            8,
            [1, 2, 3],
            [4, 5],
            "device");
        var exact = RoomMutationTargetFingerprint.RemoveMember(
            roomId,
            memberId,
            7,
            8,
            [1, 2, 3],
            [4, 5],
            "device");
        var changedRevision = RoomMutationTargetFingerprint.RemoveMember(
            roomId,
            memberId,
            8,
            9,
            [1, 2, 3],
            [4, 5],
            "device");

        Assert.Equal(first.Value, exact.Value);
        Assert.NotEqual(first.Value, changedRevision.Value);
        Assert.Equal(RoomMutationReceipt.FingerprintLength, first.Value.Length);
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void UnauthorizedInvitationActorAndUnknownInvitationShareNotFoundShape(
        bool inviteeAction)
    {
        var invitation = RoomInvitation.Create(
            Guid.NewGuid(),
            RoomId.From(Guid.NewGuid()),
            UserId.New(),
            UserId.New(),
            new RoomInvitationProposalProof(
                1,
                2,
                [1],
                [2],
                "device",
                [3],
                [4],
                "device",
                [5],
                DateTimeOffset.UtcNow,
                DateTimeOffset.UtcNow.AddHours(1)),
            DateTimeOffset.UtcNow);
        var unknown = new NotFoundException("RoomInvitation", invitation.Id);
        var unauthorized = Assert.Throws<NotFoundException>(() =>
            RoomInvitationAuthorization.EnsureActor(
                invitation,
                UserId.New(),
                inviteeAction));

        Assert.Equal(unknown.Code, unauthorized.Code);
        Assert.Equal(unknown.Message, unauthorized.Message);
    }

    [Theory]
    [InlineData(RoomMutationOperation.RemoveMember, "removeMember")]
    [InlineData(RoomMutationOperation.AcceptInvitation, "acceptInvitation")]
    [InlineData(RoomMutationOperation.DeclineInvitation, "declineInvitation")]
    [InlineData(RoomMutationOperation.CancelInvitation, "cancelInvitation")]
    [InlineData(RoomMutationOperation.AssignTask, "tasks.assign")]
    [InlineData(RoomMutationOperation.TransitionTask, "tasks.transition")]
    public void ReceiptLookupUsesCanonicalDesktopOperationStrings(
        RoomMutationOperation operation,
        string expected)
    {
        Assert.Equal(expected, RoomMutationOperationWire.ToWireOperation(operation));
    }

    [Fact]
    public async Task ReceiptLookupIsActorScopedAndReadOnly()
    {
        var actor = UserId.New();
        var otherActor = UserId.New();
        var requestId = Guid.CreateVersion7();
        var roomId = RoomId.From(Guid.NewGuid());
        var fingerprint = RoomMutationTargetFingerprint.CancelInvitation(
            roomId,
            Guid.NewGuid(),
            1);
        var repository = new LookupReceiptRepository(RoomMutationReceipt.Create(
            actor,
            RoomMutationOperation.CancelInvitation,
            requestId,
            fingerprint.Value,
            roomId,
            Guid.NewGuid(),
            "Cancelled",
            null,
            null,
            1,
            DateTimeOffset.UtcNow));
        var lookup = new RoomMutationReceiptLookup(repository);

        var ct = TestContext.Current.CancellationToken;
        var found = await lookup.FindAsync(actor, RoomMutationOperation.CancelInvitation, requestId, ct);
        var concealed = await lookup.FindAsync(otherActor, RoomMutationOperation.CancelInvitation, requestId, ct);
        var unknown = await lookup.FindAsync(actor, RoomMutationOperation.CancelInvitation, Guid.CreateVersion7(), ct);

        Assert.NotNull(found);
        Assert.Equal("cancelInvitation", found.Operation);
        Assert.Equal(fingerprint.Value, found.TargetFingerprint);
        Assert.Null(concealed);
        Assert.Null(unknown);
        Assert.Equal(3, repository.ReadCalls);
        Assert.Equal(0, repository.WriteCalls);
    }

    [Fact]
    public void DuplicatePolicyReturnsStoredResultAndChangedFingerprintConflicts()
    {
        var actor = UserId.New();
        var room = RoomId.From(Guid.NewGuid());
        var entityId = Guid.NewGuid();
        var target = RoomMutationTargetFingerprint.AssignTask(room, entityId, 3, null, null);
        var receipt = RoomMutationReceipt.Create(
            actor,
            RoomMutationOperation.AssignTask,
            RequestId,
            target.Value,
            room,
            entityId,
            "Assigned",
            null,
            null,
            4,
            DateTimeOffset.UtcNow);

        var duplicate = RoomMutationReceiptPolicy.ResolveDuplicate(receipt, target);
        Assert.True(duplicate.IsDuplicate);
        Assert.Equal("Assigned", duplicate.Result);
        Assert.Equal(4, duplicate.Revision);

        var conflictTarget = RoomMutationTargetFingerprint.AssignTask(
            room,
            entityId,
            3,
            Guid.NewGuid(),
            Guid.CreateVersion7());
        var exception = Assert.Throws<RoomMutationReceiptTargetConflictException>(() =>
            RoomMutationReceiptPolicy.ResolveDuplicate(receipt, conflictTarget));
        Assert.Equal("ROOM_MUTATION_TARGET_CONFLICT", exception.Code);
    }

    private sealed class LookupReceiptRepository(RoomMutationReceipt receipt)
        : RoomMutationReceiptRepositoryStub
    {
        public int ReadCalls { get; private set; }
        public int WriteCalls { get; private set; }

        public override Task<RoomMutationReceipt?> FindAsync(
            UserId actorUserId,
            RoomMutationOperation operation,
            Guid requestId,
            CancellationToken ct = default)
        {
            ReadCalls++;
            return Task.FromResult<RoomMutationReceipt?>(
                receipt.ActorUserId == actorUserId
                    && receipt.Operation == operation
                    && receipt.RequestId == requestId
                    ? receipt
                    : null);
        }

        public override Task AddAsync(
            RoomMutationReceipt value,
            CancellationToken ct = default)
        {
            WriteCalls++;
            throw new NotSupportedException();
        }
    }
}
