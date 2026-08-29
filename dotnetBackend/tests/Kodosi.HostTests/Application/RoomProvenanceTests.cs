using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class RoomProvenanceTests
{
    [Fact]
    public async Task ChatPost_Rejects_Session_Not_Owned_By_Claimed_Author()
    {
        var roomOwner = UserId.New();
        var author = UserId.New();
        var otherUser = UserId.New();
        var room = TestRoom(roomOwner);
        var foreignSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            otherUser,
            "foreign",
            SessionScope.Room,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret",
            room.Id);
        var chat = new RecordingRoomChatRepository();
        var service = CreateChatService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository((room.Id, author)),
            chat,
            new FakeSessionRepository(foreignSession));

        await Assert.ThrowsAsync<PolicyViolationException>(() =>
            service.PostAsync(
                room.Id,
                Guid.NewGuid(),
                author,
                foreignSession.Id.Value,
                RoomChatAuthorKind.Agent,
                "encrypted-envelope",
                null,
                null,
                TestContext.Current.CancellationToken));
        Assert.Equal(0, chat.AddCalls);
    }

    [Fact]
    public async Task ChatPost_Rejects_Human_Attribution_With_Agent_Session()
    {
        var owner = UserId.New();
        var room = TestRoom(owner);
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            owner,
            "agent",
            SessionScope.Room,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret",
            room.Id);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var service = CreateChatService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository((room.Id, owner)),
            new RecordingRoomChatRepository(),
            new FakeSessionRepository(session));

        await Assert.ThrowsAsync<PolicyViolationException>(() =>
            service.PostAsync(
                room.Id,
                Guid.NewGuid(),
                owner,
                session.Id.Value,
                RoomChatAuthorKind.Human,
                "encrypted-envelope",
                null,
                null,
                TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task ChatPost_Accepts_And_Normalizes_Multiple_Typed_Recipients()
    {
        var owner = UserId.New();
        var author = UserId.New();
        var member = UserId.New();
        var room = TestRoom(owner);
        var liveSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            owner, "live", SessionScope.Room, ToolKind.Terminal, AccessLevel.View, "secret", room.Id);
        liveSession.ActivateHost("test-host");
        liveSession.ReleaseHostSlot("test-host");
        var reconnectingSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            member, "reconnecting", SessionScope.Room, ToolKind.Terminal, AccessLevel.View, "secret", room.Id);
        reconnectingSession.ActivateHost("test-host");
        reconnectingSession.ReleaseHostSlot("test-host");
        reconnectingSession.MarkReconnecting();
        var repository = new RecordingRoomChatRepository();
        var sessionRepository = new FakeSessionRepository(liveSession, reconnectingSession);
        var service = CreateChatService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository((room.Id, author), (room.Id, member)),
            repository,
            sessionRepository);

        var message = await service.PostAsync(
            room.Id,
            Guid.NewGuid(),
            author,
            null,
            RoomChatAuthorKind.Human,
            "encrypted-envelope",
            [reconnectingSession.Id.Value, liveSession.Id.Value],
            [member.Value, owner.Value],
            TestContext.Current.CancellationToken);

        Assert.Equal(
            new[] { liveSession.Id.Value, reconnectingSession.Id.Value }.Order(),
            message.RecipientSessionIds);
        Assert.Equal(
            new[] { owner.Value, member.Value }.Order(),
            message.RecipientUserIds);
        Assert.Equal(2, sessionRepository.GetByIdForUpdateCalls);
    }

    [Fact]
    public async Task ChatPost_Rejects_Invalid_Session_And_User_Targets()
    {
        var owner = UserId.New();
        var author = UserId.New();
        var outsider = UserId.New();
        var room = TestRoom(owner);
        var pendingSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            owner, "pending", SessionScope.Room, ToolKind.Terminal, AccessLevel.View, "secret", room.Id);
        var repository = new RecordingRoomChatRepository();
        var service = CreateChatService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository((room.Id, author)),
            repository,
            new FakeSessionRepository(pendingSession));

        await Assert.ThrowsAsync<PolicyViolationException>(() => service.PostAsync(
            room.Id,
            Guid.NewGuid(),
            author,
            null,
            RoomChatAuthorKind.Human,
            "encrypted-envelope",
            [pendingSession.Id.Value],
            null,
            TestContext.Current.CancellationToken));
        await Assert.ThrowsAsync<PolicyViolationException>(() => service.PostAsync(
            room.Id,
            Guid.NewGuid(),
            author,
            null,
            RoomChatAuthorKind.Human,
            "encrypted-envelope",
            null,
            [outsider.Value],
            TestContext.Current.CancellationToken));
        Assert.Equal(0, repository.AddCalls);
    }

    [Fact]
    public async Task ChatPost_Rejects_Empty_Duplicate_And_Too_Many_Recipients()
    {
        var owner = UserId.New();
        var room = TestRoom(owner);
        var repository = new RecordingRoomChatRepository();
        var service = CreateChatService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository(),
            repository,
            new FakeSessionRepository());
        var duplicate = Guid.NewGuid();
        var tooMany = Enumerable.Range(0, RoomInputRules.ChatRecipientMaxCount + 1)
            .Select(_ => Guid.NewGuid())
            .ToArray();

        await Assert.ThrowsAsync<PolicyViolationException>(() => service.PostAsync(
            room.Id, Guid.NewGuid(), owner, null, RoomChatAuthorKind.Human, "ciphertext",
            [Guid.Empty], null, TestContext.Current.CancellationToken));
        await Assert.ThrowsAsync<PolicyViolationException>(() => service.PostAsync(
            room.Id, Guid.NewGuid(), owner, null, RoomChatAuthorKind.Human, "ciphertext",
            [duplicate, duplicate], null, TestContext.Current.CancellationToken));
        await Assert.ThrowsAsync<PolicyViolationException>(() => service.PostAsync(
            room.Id, Guid.NewGuid(), owner, null, RoomChatAuthorKind.Human, "ciphertext",
            null, [duplicate, duplicate], TestContext.Current.CancellationToken));
        await Assert.ThrowsAsync<PolicyViolationException>(() => service.PostAsync(
            room.Id, Guid.NewGuid(), owner, null, RoomChatAuthorKind.Human, "ciphertext",
            tooMany, null, TestContext.Current.CancellationToken));
        Assert.Equal(0, repository.AddCalls);
    }

    [Fact]
    public async Task ChatPost_Rejects_Targeting_Author_Session()
    {
        var owner = UserId.New();
        var room = TestRoom(owner);
        var authorSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            owner, "author", SessionScope.Room, ToolKind.Terminal, AccessLevel.View, "secret", room.Id);
        authorSession.ActivateHost("test-host");
        authorSession.ReleaseHostSlot("test-host");
        var repository = new RecordingRoomChatRepository();
        var service = CreateChatService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository(),
            repository,
            new FakeSessionRepository(authorSession));

        await Assert.ThrowsAsync<PolicyViolationException>(() => service.PostAsync(
            room.Id,
            Guid.NewGuid(),
            owner,
            authorSession.Id.Value,
            RoomChatAuthorKind.Agent,
            "ciphertext",
            [authorSession.Id.Value],
            null,
            TestContext.Current.CancellationToken));
        Assert.Equal(0, repository.AddCalls);
    }

    [Fact]
    public async Task ChatPost_Exact_Replay_Precedes_Mutable_Recipient_Eligibility()
    {
        var owner = UserId.New();
        var author = UserId.New();
        var recipientOwner = UserId.New();
        var room = TestRoom(owner);
        var recipientSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            recipientOwner,
            "recipient",
            SessionScope.Room,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret",
            room.Id);
        recipientSession.ActivateHost("test-host");
        recipientSession.ReleaseHostSlot("test-host");
        var messageId = Guid.NewGuid();
        var repository = new RecordingRoomChatRepository();
        var members = new FakeRoomMemberRepository(
            (room.Id, author),
            (room.Id, recipientOwner));
        var service = CreateChatService(
            new FakeRoomRepository(room),
            members,
            repository,
            new FakeSessionRepository(recipientSession));

        var posted = await service.PostAsync(
            room.Id,
            messageId,
            author,
            null,
            RoomChatAuthorKind.Human,
            "ciphertext",
            [recipientSession.Id.Value],
            null,
            TestContext.Current.CancellationToken);
        recipientSession.End();
        var recipientMembership = await members.GetAsync(
            room.Id,
            recipientOwner,
            TestContext.Current.CancellationToken);
        Assert.NotNull(recipientMembership);
        recipientMembership.Revoke();

        var replay = await service.PostAsync(
            room.Id,
            messageId,
            author,
            null,
            RoomChatAuthorKind.Human,
            "ciphertext",
            [recipientSession.Id.Value],
            null,
            TestContext.Current.CancellationToken);

        Assert.Same(posted, replay);
        Assert.Equal(1, repository.AddCalls);
    }

    [Fact]
    public async Task ChatPost_Exact_Replay_Rejects_Removed_Author()
    {
        var owner = UserId.New();
        var author = UserId.New();
        var room = TestRoom(owner);
        var members = new FakeRoomMemberRepository((room.Id, author));
        var repository = new RecordingRoomChatRepository();
        var service = CreateChatService(
            new FakeRoomRepository(room),
            members,
            repository,
            new FakeSessionRepository());
        var messageId = Guid.NewGuid();
        _ = await service.PostAsync(
            room.Id,
            messageId,
            author,
            null,
            RoomChatAuthorKind.Human,
            "ciphertext",
            ct: TestContext.Current.CancellationToken);
        var membership = await members.GetAsync(
            room.Id,
            author,
            TestContext.Current.CancellationToken);
        Assert.NotNull(membership);
        membership.Revoke();

        await Assert.ThrowsAsync<PolicyViolationException>(() => service.PostAsync(
            room.Id,
            messageId,
            author,
            null,
            RoomChatAuthorKind.Human,
            "ciphertext",
            ct: TestContext.Current.CancellationToken));

        Assert.Equal(1, repository.AddCalls);
    }

    [Fact]
    public async Task ChatPost_Replay_With_Changed_Ciphertext_Conflicts()
    {
        var owner = UserId.New();
        var room = TestRoom(owner);
        var messageId = Guid.NewGuid();
        var repository = new RecordingRoomChatRepository();
        var service = CreateChatService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository(),
            repository,
            new FakeSessionRepository());
        var posted = await service.PostAsync(
            room.Id,
            messageId,
            owner,
            null,
            RoomChatAuthorKind.Human,
            "ciphertext",
            null,
            null,
            TestContext.Current.CancellationToken);

        await Assert.ThrowsAsync<ConflictException>(() => service.PostAsync(
            room.Id,
            messageId,
            owner,
            null,
            RoomChatAuthorKind.Human,
            "changed-ciphertext",
            null,
            null,
            TestContext.Current.CancellationToken));

        Assert.Equal("ciphertext", posted.Body);
        Assert.Equal(1, repository.AddCalls);
    }

    [Fact]
    public async Task ChatPost_Replay_With_Different_Member_Attribution_Conflicts()
    {
        var owner = UserId.New();
        var otherMember = UserId.New();
        var room = TestRoom(owner);
        var messageId = Guid.NewGuid();
        var repository = new RecordingRoomChatRepository();
        var service = CreateChatService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository((room.Id, otherMember)),
            repository,
            new FakeSessionRepository());
        _ = await service.PostAsync(
            room.Id,
            messageId,
            owner,
            null,
            RoomChatAuthorKind.Human,
            "ciphertext",
            null,
            null,
            TestContext.Current.CancellationToken);

        await Assert.ThrowsAsync<ConflictException>(() => service.PostAsync(
            room.Id,
            messageId,
            otherMember,
            null,
            RoomChatAuthorKind.Human,
            "changed-ciphertext",
            null,
            null,
            TestContext.Current.CancellationToken));

        Assert.Equal(1, repository.AddCalls);
    }

    [Fact]
    public async Task ChatPost_Rejects_Session_Owned_By_Inactive_Room_User()
    {
        var owner = UserId.New();
        var author = UserId.New();
        var inactiveOwner = UserId.New();
        var room = TestRoom(owner);
        var recipientSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            inactiveOwner,
            "inactive-owner",
            SessionScope.Room,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret",
            room.Id);
        recipientSession.ActivateHost("test-host");
        recipientSession.ReleaseHostSlot("test-host");
        var repository = new RecordingRoomChatRepository();
        var service = CreateChatService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository((room.Id, author)),
            repository,
            new FakeSessionRepository(recipientSession));

        await Assert.ThrowsAsync<PolicyViolationException>(() => service.PostAsync(
            room.Id,
            Guid.NewGuid(),
            author,
            null,
            RoomChatAuthorKind.Human,
            "ciphertext",
            [recipientSession.Id.Value],
            null,
            TestContext.Current.CancellationToken));

        Assert.Equal(0, repository.AddCalls);
    }

    [Fact]
    public async Task ChatPost_Validates_Membership_After_Acquiring_Room_Lock()
    {
        var owner = UserId.New();
        var author = UserId.New();
        var room = TestRoom(owner);
        var members = new FakeRoomMemberRepository((room.Id, author));
        var repository = new RecordingRoomChatRepository(blockRoomLock: true);
        var service = CreateChatService(
            new FakeRoomRepository(room),
            members,
            repository,
            new FakeSessionRepository());

        var posting = service.PostAsync(
            room.Id,
            Guid.NewGuid(),
            author,
            null,
            RoomChatAuthorKind.Human,
            "ciphertext",
            null,
            null,
            TestContext.Current.CancellationToken);
        await repository.RoomLockReached.Task.WaitAsync(
            TestContext.Current.CancellationToken);
        var membership = await members.GetAsync(
            room.Id,
            author,
            TestContext.Current.CancellationToken);
        Assert.NotNull(membership);
        membership.Revoke();
        repository.ReleaseRoomLock.TrySetResult();

        await Assert.ThrowsAsync<PolicyViolationException>(() => posting);
        Assert.Equal(0, repository.AddCalls);
    }

    [Fact]
    public async Task ChatPost_Acquires_Room_Lock_Before_Session_Locks()
    {
        var owner = UserId.New();
        var author = UserId.New();
        var room = TestRoom(owner);
        var recipientSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            owner,
            "recipient",
            SessionScope.Room,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret",
            room.Id);
        recipientSession.ActivateHost("test-host");
        recipientSession.ReleaseHostSlot("test-host");
        var repository = new RecordingRoomChatRepository(blockRoomLock: true);
        var sessions = new FakeSessionRepository(recipientSession);
        var service = CreateChatService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository((room.Id, author)),
            repository,
            sessions);

        var posting = service.PostAsync(
            room.Id,
            Guid.NewGuid(),
            author,
            null,
            RoomChatAuthorKind.Human,
            "ciphertext",
            [recipientSession.Id.Value],
            null,
            TestContext.Current.CancellationToken);
        await repository.RoomLockReached.Task.WaitAsync(
            TestContext.Current.CancellationToken);

        Assert.Equal(0, sessions.GetByIdForUpdateCalls);

        repository.ReleaseRoomLock.TrySetResult();
        await posting;
        Assert.Equal(1, sessions.GetByIdForUpdateCalls);
    }

    [Fact]
    public async Task ChatPost_Rejects_Oversized_Recipients_Without_Enumeration()
    {
        var owner = UserId.New();
        var room = TestRoom(owner);
        var repository = new RecordingRoomChatRepository();
        var service = CreateChatService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository(),
            repository,
            new FakeSessionRepository());

        await Assert.ThrowsAsync<PolicyViolationException>(() => service.PostAsync(
            room.Id,
            Guid.NewGuid(),
            owner,
            null,
            RoomChatAuthorKind.Human,
            "ciphertext",
            new NonEnumeratingGuidList(RoomInputRules.ChatRecipientMaxCount + 1),
            null,
            TestContext.Current.CancellationToken));

        Assert.Equal(0, repository.MessageLockCalls);
    }

    [Fact]
    public void Task_Description_And_Result_Enforce_Manifest_UTF16_Bounds()
    {
        var owner = UserId.New();
        var room = TestRoom(owner);
        Assert.Throws<DomainException>(() => RoomTask.Create(
            Guid.NewGuid(),
            room.Id,
            owner,
            "title",
            new string('d', RoomInputRules.TaskDescriptionMaxLength + 1),
            null,
            null, DateTimeOffset.UtcNow));

        var oversizedResultTask = RoomTask.Create(
            Guid.NewGuid(),
            room.Id,
            owner,
            "title",
            null,
            null,
            null, DateTimeOffset.UtcNow);
        oversizedResultTask.Claim(DateTimeOffset.UtcNow);
        Assert.Throws<DomainException>(() => oversizedResultTask.Complete(
            new string('r', RoomInputRules.TaskResultMaxLength + 1),
            owner,
            DateTimeOffset.UtcNow));

        var task = RoomTask.Create(
            Guid.NewGuid(),
            room.Id,
            owner,
            "title",
            new string('d', RoomInputRules.TaskDescriptionMaxLength),
            null,
            null, DateTimeOffset.UtcNow);
        task.Claim(DateTimeOffset.UtcNow);
        task.Complete(
            new string('r', RoomInputRules.TaskResultMaxLength),
            owner,
            DateTimeOffset.UtcNow);
        Assert.Equal(RoomInputRules.TaskResultMaxLength, task.Result?.Length);
    }

    [Fact]
    public void OwnerOverride_Reopening_Done_Task_Clears_CompletedAt()
    {
        var owner = UserId.New();
        var room = TestRoom(owner);
        var task = RoomTask.Create(
            Guid.NewGuid(),
            room.Id,
            owner,
            "encrypted-envelope",
            null,
            null,
            null, DateTimeOffset.UtcNow);
        task.Claim(DateTimeOffset.UtcNow);
        task.Complete(null, owner, DateTimeOffset.UtcNow);

        task.ApplyOwnerOverride(RoomTaskStatus.Open, null, owner, DateTimeOffset.UtcNow);

        Assert.Null(task.CompletedAt);
    }

    [Fact]
    public async Task TaskList_Clamps_Offset_And_PageSize()
    {
        var owner = UserId.New();
        var room = TestRoom(owner);
        var task = RoomTask.Create(
            Guid.NewGuid(),
            room.Id,
            owner,
            "encrypted-envelope",
            null,
            null,
            null, DateTimeOffset.UtcNow);
        var tasks = new SingleRoomTaskRepository(task);
        var service = CreateTaskService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository((room.Id, owner)),
            tasks,
            new FakeSessionRepository());

        _ = await service.ListAsync(
            room.Id,
            owner,
            null,
            null,
            offset: -10,
            limit: 5_000,
            TestContext.Current.CancellationToken);

        Assert.Equal(0, tasks.LastOffset);
        Assert.Equal(501, tasks.LastLimit);
    }

    [Fact]
    public async Task TaskCreate_Persists_Exact_Active_RoomSession_Incarnation()
    {
        var owner = UserId.New();
        var room = TestRoom(owner);
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            owner,
            "assigned",
            SessionScope.Room,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret",
            room.Id);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var tasks = new SingleRoomTaskRepository(RoomTask.Create(
            Guid.NewGuid(), room.Id, owner, "placeholder", null, null, null, DateTimeOffset.UtcNow));
        var service = CreateTaskService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository((room.Id, owner)),
            tasks,
            new FakeSessionRepository(session));

        var task = await service.CreateAsync(
            room.Id,
            Guid.NewGuid(),
            owner,
            "ciphertext",
            null,
            session.Id.Value,
            session.IncarnationId,
            null,
            TestContext.Current.CancellationToken);

        Assert.Equal(session.Id.Value, task.AssignedSessionId);
        Assert.Equal(session.IncarnationId, task.AssignedSessionIncarnationId);
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public async Task TaskAssignment_Rejects_Ended_Or_CrossRoom_Session(bool ended)
    {
        var owner = UserId.New();
        var room = TestRoom(owner);
        var otherRoom = TestRoom(owner);
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            owner,
            "invalid-assignee",
            SessionScope.Room,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret",
            ended ? room.Id : otherRoom.Id);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        if (ended)
        {
            session.End();
        }
        var task = RoomTask.Create(
            Guid.NewGuid(), room.Id, owner, "ciphertext", null, null, null, DateTimeOffset.UtcNow);
        var service = CreateTaskService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository((room.Id, owner)),
            new SingleRoomTaskRepository(task),
            new FakeSessionRepository(session));

        await Assert.ThrowsAsync<PolicyViolationException>(() =>
            service.CreateAsync(
                room.Id,
                Guid.NewGuid(),
                owner,
                "ciphertext",
                null,
                session.Id.Value,
                session.IncarnationId,
                null,
                TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task TaskTransition_Rejects_Forged_AssignedSession_Provenance()
    {
        var roomOwner = UserId.New();
        var actor = UserId.New();
        var otherUser = UserId.New();
        var room = TestRoom(roomOwner);
        var foreignSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            otherUser,
            "foreign",
            SessionScope.Room,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret",
            room.Id);
        var task = RoomTask.Create(
            Guid.NewGuid(),
            room.Id,
            roomOwner,
            "encrypted-envelope",
            null,
            foreignSession.Id.Value,
            null, DateTimeOffset.UtcNow);
        var service = CreateTaskService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository((room.Id, actor)),
            new SingleRoomTaskRepository(task),
            new FakeSessionRepository(foreignSession),
            new RecordingRoomMutationReceiptRepository());

        await Assert.ThrowsAsync<PolicyViolationException>(() =>
            service.TransitionIdempotentlyAsync(
                Guid.CreateVersion7(),
                room.Id,
                task.Id,
                task.Revision,
                actor,
                foreignSession.Id.Value,
                foreignSession.IncarnationId,
                RoomTaskStatus.InProgress,
                null,
                TestContext.Current.CancellationToken));
        Assert.Equal(RoomTaskStatus.Open, task.Status);
    }

    [Fact]
    public async Task TaskOwnerOverride_WithResult_AdvancesOneRevisionAndReceipt()
    {
        var owner = UserId.New();
        var room = TestRoom(owner);
        var task = RoomTask.Create(
            Guid.NewGuid(),
            room.Id,
            owner,
            "encrypted-envelope",
            null,
            null,
            null, DateTimeOffset.UtcNow);
        var receipts = new RecordingRoomMutationReceiptRepository();
        var service = CreateTaskService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository(),
            new SingleRoomTaskRepository(task),
            new FakeSessionRepository(),
            receipts);

        var mutation = await service.TransitionIdempotentlyAsync(
            Guid.CreateVersion7(),
            room.Id,
            task.Id,
            expectedTaskRevision: 0,
            owner,
            requesterSessionId: null,
            requesterSessionIncarnationId: null,
            RoomTaskStatus.Review,
            "evidence",
            TestContext.Current.CancellationToken);

        Assert.Equal(RoomTaskStatus.Review, task.Status);
        Assert.Equal("evidence", task.Result);
        Assert.Equal(owner, task.ResultAuthorUserId);
        Assert.Equal(1, task.Revision);
        Assert.Equal(1, mutation.Receipt.Revision);
        Assert.Equal(1, receipts.AddCalls);
    }

    [Fact]
    public async Task ChatPost_Owns_Transaction_Through_Commit()
    {
        var owner = UserId.New();
        var room = TestRoom(owner);
        var unitOfWork = new RecordingUnitOfWork();
        var service = new RoomChatService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository(),
            new RecordingRoomChatRepository(),
            new FakeSessionRepository(),
            unitOfWork);

        _ = await service.PostAsync(
            room.Id,
            Guid.NewGuid(),
            owner,
            null,
            RoomChatAuthorKind.Human,
            "ciphertext",
            ct: TestContext.Current.CancellationToken);

        Assert.Equal(["begin", "save", "commit", "dispose"], unitOfWork.Events);
    }

    [Fact]
    public async Task RoomCreate_Owns_Lifecycle_Lock_Through_Commit()
    {
        var owner = UserId.New();
        var room = TestRoom(owner);
        var rosterBody = RoomRosterTestData.Build(room, 1, [owner]);
        var unitOfWork = new RecordingUnitOfWork();
        var roomLock = new RecordingRoomLifecycleLock(unitOfWork.Events);
        var rooms = new CreateRecordingRoomRepository();
        var device = TestDeviceCertificate.CreateDevice(
            owner,
            "owner-device",
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Owner device",
            signerDeviceId: "owner-device",
            issuedAt: DateTimeOffset.UtcNow.AddMinutes(-1),
            expiresAt: null);
        var deviceList = TestDeviceList.Create(
            owner,
            1,
            """[{"deviceId":"owner-device","signerDeviceId":"owner-device"}]""",
            "owner-device",
            [2],
            DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds(),
            null);
        var service = new RoomService(
            rooms,
            new FakeRoomMemberRepository(),
            new FakeUserRepository(),
            new RoomRosterVerifier(
                new FakeUserDeviceRepository(device),
                new FakeUserDeviceListRepository(deviceList),
                new AlwaysValidSignatureVerifier(),
                TimeProvider.System),
            new RecordingRoomRosterTransitionRepository(),
            unitOfWork,
            roomLock);

        _ = await service.CreateAsync(
            room.Id,
            owner,
            room.Name,
            room.Slug,
            room.RosterGeneration,
            rosterBody,
            room.RosterSignature,
            room.RosterSignerDeviceId,
            TestContext.Current.CancellationToken);

        Assert.Equal(["begin", "room-lock", "save", "commit", "dispose"], unitOfWork.Events);
    }

    [Fact]
    public async Task TaskCreate_Owns_Transaction_Through_Commit()
    {
        var owner = UserId.New();
        var room = TestRoom(owner);
        var unitOfWork = new RecordingUnitOfWork();
        var roomLock = new RecordingRoomLifecycleLock(unitOfWork.Events);
        var service = new RoomTaskService(
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository(),
            new SingleRoomTaskRepository(RoomTask.Create(
                Guid.NewGuid(), room.Id, owner, "placeholder", null, null, null, DateTimeOffset.UtcNow)),
            new FakeSessionRepository(),
            new UnconfiguredRoomMutationReceiptRepository(),
            roomLock,
            unitOfWork);

        _ = await service.CreateAsync(
            room.Id,
            Guid.NewGuid(),
            owner,
            "ciphertext",
            null,
            null,
            null,
            null,
            TestContext.Current.CancellationToken);

        Assert.Equal(["begin", "room-lock", "save", "commit", "dispose"], unitOfWork.Events);
    }

    private sealed class RecordingRoomLifecycleLock(List<string> events) : IRoomLifecycleLock
    {
        public Task AcquireAsync(RoomId roomId, CancellationToken ct = default)
        {
            events.Add("room-lock");
            return Task.CompletedTask;
        }
    }

    private static RoomChatService CreateChatService(
        IRoomRepository rooms,
        IRoomMemberRepository roomMembers,
        IRoomChatRepository chat,
        ISessionRepository sessions) =>
        new(rooms, roomMembers, chat, sessions, new CompletedUnitOfWork());

    private static RoomTaskService CreateTaskService(
        IRoomRepository rooms,
        IRoomMemberRepository roomMembers,
        IRoomTaskRepository tasks,
        ISessionRepository sessions,
        IRoomMutationReceiptRepository? mutationReceipts = null) =>
        new(
            rooms,
            roomMembers,
            tasks,
            sessions,
            mutationReceipts ?? new UnconfiguredRoomMutationReceiptRepository(),
            new FakeRoomLifecycleLock(),
            new CompletedUnitOfWork());

    private static Room TestRoom(UserId ownerId) =>
        Room.Create(
            RoomId.From(Guid.NewGuid()),
            ownerId,
            "Room",
            "room",
            1,
            [1],
            [2],
            "owner-device");

    private sealed class CreateRecordingRoomRepository : IRoomRepository
    {
        private Room? _room;

        public Task<Room?> GetByIdAsync(RoomId id, CancellationToken ct = default) =>
            Task.FromResult(_room?.Id == id ? _room : null);


        public Task AddAsync(Room room, CancellationToken ct = default)
        {
            _room = room;
            return Task.CompletedTask;
        }

        public Task<IReadOnlyList<Room>> GetByMemberPageAsync(
            UserId userId,
            FeedCursor? cursor,
            int limit,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<Room>>([]);

        public Task<IReadOnlyList<Room>> GetByIdsAsync(
            IReadOnlyCollection<RoomId> ids,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<Room>>(
                _room is not null && ids.Contains(_room.Id) ? [_room] : []);
    }

    private sealed class RecordingRoomMutationReceiptRepository
        : RoomMutationReceiptRepositoryStub
    {
        public int AddCalls { get; private set; }

        public override Task AcquireAsync(
            UserId actorUserId,
            RoomMutationOperation operation,
            Guid requestId,
            CancellationToken ct = default) => Task.CompletedTask;

        public override Task<RoomMutationReceipt?> GetAsync(
            UserId actorUserId,
            RoomMutationOperation operation,
            Guid requestId,
            CancellationToken ct = default) =>
            Task.FromResult<RoomMutationReceipt?>(null);

        public override Task AddAsync(
            RoomMutationReceipt receipt,
            CancellationToken ct = default)
        {
            AddCalls++;
            return Task.CompletedTask;
        }
    }

    private sealed class RecordingUnitOfWork : UnitOfWorkStub
    {
        public List<string> Events { get; } = [];

        public override Task<ITransactionScope> BeginTransactionAsync(
            CancellationToken ct = default)
        {
            Events.Add("begin");
            return Task.FromResult<ITransactionScope>(new Transaction(Events));
        }

        public override Task SaveChangesAsync(CancellationToken ct = default)
        {
            Events.Add("save");
            return Task.CompletedTask;
        }

        private sealed class Transaction(List<string> events) : TransactionScopeStub
        {
            public override Task CommitAsync(CancellationToken ct = default)
            {
                events.Add("commit");
                return Task.CompletedTask;
            }

            public override ValueTask DisposeAsync()
            {
                events.Add("dispose");
                return ValueTask.CompletedTask;
            }
        }
    }

    private sealed class RecordingRoomChatRepository(bool blockRoomLock = false)
        : IRoomChatRepository
    {
        private readonly Dictionary<Guid, RoomChatMessage> _messages = [];
        public int AddCalls { get; private set; }
        public int MessageLockCalls { get; private set; }
        public TaskCompletionSource RoomLockReached { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource ReleaseRoomLock { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public Task AcquireMessageLockAsync(
            Guid messageId,
            CancellationToken ct = default)
        {
            MessageLockCalls++;
            return Task.CompletedTask;
        }

        public async Task AcquireRoomLockAsync(
            RoomId roomId,
            CancellationToken ct = default)
        {
            if (!blockRoomLock)
            {
                return;
            }

            RoomLockReached.TrySetResult();
            await ReleaseRoomLock.Task.WaitAsync(ct);
        }

        public Task<RoomChatMessage?> GetByIdAsync(
            Guid messageId,
            CancellationToken ct = default)
        {
            _messages.TryGetValue(messageId, out var message);
            return Task.FromResult<RoomChatMessage?>(message);
        }

        public Task<RoomChatMessage> AddWithNextSeqAsync(
            RoomId roomId,
            Guid messageId,
            UserId authorUserId,
            Guid? authorSessionId,
            RoomChatAuthorKind authorKind,
            IReadOnlyList<Guid> recipientSessionIds,
            IReadOnlyList<Guid> recipientUserIds,
            string body,
            CancellationToken ct = default)
        {
            AddCalls++;
            var message = RoomChatMessage.Create(
                messageId,
                roomId,
                authorUserId,
                authorSessionId,
                authorKind,
                recipientSessionIds,
                recipientUserIds,
                body,
                _messages.Count + 1);
            _messages.Add(messageId, message);
            return Task.FromResult(message);
        }

        public Task<IReadOnlyList<RoomChatMessage>> GetPageCandidatesSinceAsync(
            RoomId roomId,
            long sinceSeq,
            int limit,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<RoomChatMessage>>([]);

        public Task<IReadOnlyList<RoomChatMessage>> GetTailCandidatesAsync(
            RoomId roomId,
            long? beforeSeq,
            int limit,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<RoomChatMessage>>([]);
    }

    private sealed class NonEnumeratingGuidList(int count) : IReadOnlyList<Guid>
    {
        public int Count { get; } = count;
        public Guid this[int index] => throw new InvalidOperationException("Must not enumerate.");
        public IEnumerator<Guid> GetEnumerator() =>
            throw new InvalidOperationException("Must not enumerate.");
        System.Collections.IEnumerator System.Collections.IEnumerable.GetEnumerator() =>
            GetEnumerator();
    }

    private sealed class SingleRoomTaskRepository(RoomTask task) : IRoomTaskRepository
    {
        public int? LastOffset { get; private set; }
        public int? LastLimit { get; private set; }
        public Task<RoomTask?> GetByIdAsync(Guid id, CancellationToken ct = default) =>
            Task.FromResult<RoomTask?>(id == task.Id ? task : null);

        public Task<RoomTask?> GetByIdForUpdateAsync(
            Guid id,
            CancellationToken ct = default) => GetByIdAsync(id, ct);

        public Task<IReadOnlyList<RoomTask>> GetByRoomPageAsync(
            RoomId roomId,
            RoomTaskStatus? statusFilter,
            Guid? assigneeSessionFilter,
            int offset,
            int candidateLimit,
            CancellationToken ct = default)
        {
            LastOffset = offset;
            LastLimit = candidateLimit;
            return Task.FromResult<IReadOnlyList<RoomTask>>([task]);
        }

        public Task<RoomTask> AddIdempotentAsync(
            RoomTask roomTask,
            CancellationToken ct = default) => Task.FromResult(roomTask);
    }
}
