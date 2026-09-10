using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Infrastructure.Persistence;
using Kodosi.Infrastructure.Persistence.Repositories;
using Microsoft.EntityFrameworkCore;
using Testcontainers.PostgreSql;

namespace Kodosi.HostTests;

public sealed class ArtifactEndorsementPostgresTests
{
    [Fact]
    public async Task RemainingMemberCanVerifyFormerArtifactAuthorsWithoutRestoringRoomAccess()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_history_read_test").WithUsername("kodosi").WithPassword("kodosi-test-password").Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>().UseNpgsql(postgres.GetConnectionString()).Options;
        var reader = User.Create(UserId.New(), "reader@example.test", "history-reader", "Reader");
        var authors = Enumerable.Range(0, 4).Select(index =>
            User.Create(UserId.New(), $"author{index}@example.test", $"history-author-{index}", "Author")).ToArray();
        var room = Room.Create(RoomId.From(Guid.NewGuid()), reader.Id, "History", "history", 1, [1], [2], "reader-device");
        var otherRoom = Room.Create(RoomId.From(Guid.NewGuid()), authors[3].Id, "Other", "other", 1, [1], [2], "other-device");
        var now = DateTimeOffset.UtcNow.AddMinutes(-1);
        var digest = new string('a', 64);
        var chat = RoomChatMessage.Create(Guid.NewGuid(), room.Id, authors[0].Id, null, RoomChatAuthorKind.Human,
            [], [], "chat-ciphertext", 1);
        var titleTask = RoomTask.Create(Guid.NewGuid(), room.Id, authors[1].Id, "title-ciphertext", null, null, null, now);
        var resultTask = RoomTask.Create(Guid.NewGuid(), room.Id, reader.Id, "result-task-ciphertext", null, null, null, now);
        resultTask.ApplyOwnerOverride(RoomTaskStatus.Done, "result-ciphertext", authors[2].Id, now);

        await using (var setup = new KodosiDbContext(options))
        {
            await setup.Database.MigrateAsync(TestContext.Current.CancellationToken);
            foreach (var user in authors.Prepend(reader))
            {
                user.AdvanceIdentityLifecycle(Guid.NewGuid());
                setup.Users.Add(user);
                var deviceId = $"device-{user.Id.Value:N}";
                setup.UserDevices.Add(TestDeviceCertificate.CreateDevice(user.Id, deviceId, new byte[1184], new byte[1952],
                    "Device", deviceId, now, null));
                setup.UserDeviceLists.Add(TestDeviceList.Create(user.Id, 1,
                    $$"""[{"deviceId":"{{deviceId}}","signerDeviceId":"{{deviceId}}"}]""", deviceId,
                    new byte[3309], now.ToUnixTimeMilliseconds(), null));
                setup.ArtifactEndorsements.Add(ArtifactEndorsement.Create(user.Id, user.IdentityIncarnationId!.Value,
                    digest, deviceId, new byte[3309]));
            }
            setup.Rooms.AddRange(room, otherRoom);
            setup.RoomMembers.Add(RoomMember.CreateOwner(room.Id, reader.Id));
            foreach (var author in authors)
            {
                var member = DomainFixtureHydrator.RoomMember(room.Id, author.Id);
                member.Revoke();
                setup.RoomMembers.Add(member);
            }
            setup.RoomMembers.Add(RoomMember.CreateOwner(otherRoom.Id, authors[3].Id));
            setup.RoomChatMessages.AddRange(chat, RoomChatMessage.Create(Guid.NewGuid(), room.Id, reader.Id, null,
                RoomChatAuthorKind.Human, [], [], "reader-ciphertext", 2),
                RoomChatMessage.Create(Guid.NewGuid(), otherRoom.Id, authors[3].Id, null,
                    RoomChatAuthorKind.Human, [], [], "unrelated-ciphertext", 1));
            setup.RoomTasks.AddRange(titleTask, resultTask);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using (var read = new KodosiDbContext(options))
        {
            var identities = IdentityService(read);
            var endorsements = new ArtifactEndorsementService(new ArtifactEndorsementRepository(read),
                new UserRepository(read), new UserDeviceRepository(read), new UserDeviceListRepository(read),
                new RejectSignatureVerifier(), new PostgresUserLifecycleLock(read), new UnitOfWork(read), identities, TimeProvider.System);
            var members = new RoomMemberRepository(read);
            foreach (var author in authors.Take(3))
            {
                Assert.Empty(await members.GetSharedActiveRoomIdsAsync(reader.Id, author.Id, TestContext.Current.CancellationToken));
                Assert.Equal([room.Id], await members.GetHistoricalArtifactRoomIdsAsync(reader.Id, author.Id, TestContext.Current.CancellationToken));
                Assert.NotNull(await identities.GetAsync(reader.Id, author.Id, TestContext.Current.CancellationToken));
                Assert.Single((await endorsements.GetAsync(reader.Id, author.Id, digest, TestContext.Current.CancellationToken))!);
                Assert.Null(await identities.GetAsync(author.Id, reader.Id, TestContext.Current.CancellationToken));
                Assert.Null(await endorsements.GetAsync(author.Id, reader.Id, digest, TestContext.Current.CancellationToken));
                Assert.False(await members.IsMemberAsync(room.Id, author.Id, TestContext.Current.CancellationToken));
            }
            Assert.Null(await identities.GetAsync(reader.Id, authors[3].Id, TestContext.Current.CancellationToken));
            var chatService = new RoomChatService(new RoomRepository(read), members, new RoomChatRepository(read),
                new SessionRepository(read), new UnitOfWork(read));
            await Assert.ThrowsAsync<PolicyViolationException>(() => chatService.ListAsync(
                room.Id, authors[0].Id, 0, 100, TestContext.Current.CancellationToken));
        }

        await using (var removal = new KodosiDbContext(options))
        {
            removal.RoomChatMessages.Remove(await removal.RoomChatMessages.SingleAsync(message => message.Id == chat.Id,
                TestContext.Current.CancellationToken));
            await removal.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        await using (var read = new KodosiDbContext(options))
        {
            Assert.Null(await IdentityService(read).GetAsync(reader.Id, authors[0].Id, TestContext.Current.CancellationToken));
        }
        await using (var removal = new KodosiDbContext(options))
        {
            (await removal.RoomMembers.SingleAsync(member => member.RoomId == room.Id && member.UserId == reader.Id,
                TestContext.Current.CancellationToken)).Revoke();
            await removal.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        await using (var read = new KodosiDbContext(options))
        {
            Assert.Null(await IdentityService(read).GetAsync(reader.Id, authors[1].Id, TestContext.Current.CancellationToken));
        }
    }

    private static UserIdentityBundleService IdentityService(KodosiDbContext context) => new(
        new UserDeviceRepository(context), new UserDeviceListRepository(context), new UserRepository(context),
        new FriendshipRepository(context), new RoomMemberRepository(context), new AccessOverrideRepository(context, TimeProvider.System),
        new IdentityExposureRepository(context), new PostgresUserLifecycleLock(context), new PostgresRoomLifecycleLock(context),
        new UnitOfWork(context));

    private sealed class RejectSignatureVerifier : IPopSignatureVerifier
    {
        public bool Verify(ReadOnlySpan<byte> publicKey, ReadOnlySpan<byte> message, ReadOnlySpan<byte> signature) => false;
    }

    [Fact]
    public async Task MigrationPersistsBoundedProofsIsolatesResetAndCancelsOnlyRevokedPendingInvitations()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_endorsements_test").WithUsername("kodosi").WithPassword("kodosi-test-password").Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>().UseNpgsql(postgres.GetConnectionString()).Options;
        var ownerId = UserId.New();
        var owner = User.Create(ownerId, "owner@example.test", "endorsement-owner", "Owner");
        var incarnation = Guid.NewGuid();
        owner.AdvanceIdentityLifecycle(incarnation);
        var now = new DateTimeOffset(2026, 9, 6, 12, 0, 0, TimeSpan.Zero);
        var room = Room.Create(RoomId.From(Guid.NewGuid()), ownerId, "Mission", "mission", 1, [1], [2], "old-device");
        RoomInvitation Invitation(string proposalSigner, string rosterSigner) => RoomInvitation.Create(
            Guid.NewGuid(), room.Id, UserId.New(), ownerId,
            new RoomInvitationProposalProof(1, 2, [1], [2], rosterSigner, [3], [4], proposalSigner, [5], now, now.AddDays(7)), now);
        var proposalAffected = Invitation("old-device", "new-device");
        var rosterAffected = Invitation("new-device", "old-device");
        var unrelated = Invitation("new-device", "new-device");
        var accepted = Invitation("old-device", "old-device");
        accepted.Accept(accepted.InviteeUserId, new RoomInvitationDecisionProof([1], [2], "invitee-device", now), now);
        var member = RoomMember.CreateFromInvitation(accepted);
        var digest = new string('a', 64);

        await using (var setup = new KodosiDbContext(options))
        {
            await setup.Database.MigrateAsync(TestContext.Current.CancellationToken);
            setup.Users.Add(owner);
            foreach (var invitation in new[] { proposalAffected, rosterAffected, unrelated, accepted })
            {
                setup.Users.Add(User.Create(invitation.InviteeUserId, $"{invitation.Id:N}@example.test", $"invitee-{invitation.Id:N}", "Invitee"));
            }
            setup.Rooms.Add(room);
            setup.RoomInvitations.AddRange(proposalAffected, rosterAffected, unrelated, accepted);
            setup.RoomMembers.Add(member);
            setup.ArtifactEndorsements.Add(ArtifactEndorsement.Create(ownerId, incarnation, digest, "old-device", new byte[3309]));
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using (var rollback = new KodosiDbContext(options))
        {
            await using var transaction = await rollback.Database.BeginTransactionAsync(TestContext.Current.CancellationToken);
            await new PostgresUserLifecycleLock(rollback).AcquireAsync(ownerId, TestContext.Current.CancellationToken);
            var affected = await new RevokedDeviceInvitationCancellation(rollback).CancelPendingAsync(ownerId, ["old-device"], now, TestContext.Current.CancellationToken);
            Assert.Equal(2, affected.Count);
            await rollback.SaveChangesAsync(TestContext.Current.CancellationToken);
            await transaction.RollbackAsync(TestContext.Current.CancellationToken);
        }
        await using (var committed = new KodosiDbContext(options))
        {
            Assert.Equal(3, await committed.RoomInvitations.CountAsync(row => row.Status == RoomInvitationStatus.Pending, TestContext.Current.CancellationToken));
            await using var transaction = await committed.Database.BeginTransactionAsync(TestContext.Current.CancellationToken);
            await new PostgresUserLifecycleLock(committed).AcquireAsync(ownerId, TestContext.Current.CancellationToken);
            var affected = await new RevokedDeviceInvitationCancellation(committed).CancelPendingAsync(ownerId, ["old-device"], now, TestContext.Current.CancellationToken);
            Assert.Contains(proposalAffected.InviteeUserId, affected);
            Assert.Contains(rosterAffected.InviteeUserId, affected);
            var repository = new ArtifactEndorsementRepository(committed);
            var proof = await repository.GetAsync(ownerId, incarnation, digest, TestContext.Current.CancellationToken);
            Assert.NotNull(proof);
            proof.ReplaceWith(ArtifactEndorsement.Create(ownerId, incarnation, digest, "new-device", new byte[3309]));
            await committed.SaveChangesAsync(TestContext.Current.CancellationToken);
            await transaction.CommitAsync(TestContext.Current.CancellationToken);
        }
        await using (var verify = new KodosiDbContext(options))
        {
            var invitations = await verify.RoomInvitations.ToDictionaryAsync(row => row.Id, TestContext.Current.CancellationToken);
            Assert.Equal(RoomInvitationStatus.Cancelled, invitations[proposalAffected.Id].Status);
            Assert.Equal(RoomInvitationStatus.Cancelled, invitations[rosterAffected.Id].Status);
            Assert.Equal(RoomInvitationStatus.Pending, invitations[unrelated.Id].Status);
            Assert.Equal(RoomInvitationStatus.Accepted, invitations[accepted.Id].Status);
            Assert.True((await verify.RoomMembers.SingleAsync(TestContext.Current.CancellationToken)).IsActive);
            var repository = new ArtifactEndorsementRepository(verify);
            Assert.Equal("new-device", (await repository.GetAsync(ownerId, incarnation, digest, TestContext.Current.CancellationToken))!.EndorserDeviceId);
            Assert.Equal(1, await repository.CountAsync(ownerId, incarnation, TestContext.Current.CancellationToken));
            var resetOwner = await verify.Users.SingleAsync(user => user.Id == ownerId, TestContext.Current.CancellationToken);
            resetOwner.AdvanceIdentityLifecycle(Guid.NewGuid());
            await verify.SaveChangesAsync(TestContext.Current.CancellationToken);
            Assert.Null(await repository.GetAsync(ownerId, incarnation, digest, TestContext.Current.CancellationToken));
            Assert.Empty(await repository.ListAsync(ownerId, incarnation, 0, 100, TestContext.Current.CancellationToken));
            await repository.DeleteOtherIncarnationsAsync(ownerId, resetOwner.IdentityIncarnationId!.Value, TestContext.Current.CancellationToken);
            Assert.Empty(await verify.ArtifactEndorsements.AsNoTracking().ToListAsync(TestContext.Current.CancellationToken));
        }
    }
}
