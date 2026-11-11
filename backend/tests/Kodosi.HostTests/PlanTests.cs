using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Projects;
using Kodosi.Plans;
using Kodosi.Security;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.Configuration;
using Xunit;
using static Kodosi.HostTests.BackendApplication;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class PlanTests(PostgresFixture postgres)
{
    private static readonly PlanLimits Small = new(Devices: 1, OwnedProjects: 1, ProjectPeople: 2, SharePeople: 1,
        ProjectBytes: PlanCatalog.LeastProjectBytes, Friends: 1, SentFriendRequests: 1, ReceivedFriendRequests: 1);

    private static PlanCatalog Catalog() =>
        PlanCatalog.Of("free", new Dictionary<string, PlanLimits> { ["free"] = Small, ["unlimited"] = PlanLimits.Safety });

    private static async Task RefusedAsync(string message, Func<Task> action)
    {
        var refusal = await Assert.ThrowsAsync<ApiException>(action);
        Assert.Equal(403, refusal.Status);
        Assert.Equal(message, refusal.Message);
    }

    private static async Task BefriendAsync(TestStore store, Guid a, Guid b)
    {
        var (first, second) = Kodosi.Friends.FriendService.Pair(a, b);
        store.Db.Friendships.Add(new Friendship { FirstUserId = first, SecondUserId = second, RequestedBy = a, Accepted = true, CreatedAt = DateTimeOffset.UtcNow });
        await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
    }

    private static async Task<Guid> KeyedProjectAsync(TestStore store, TestUser owner, bool dosi = false)
    {
        var ct = TestContext.Current.CancellationToken;
        var project = Guid.CreateVersion7();
        await store.Projects.CreateAsync(owner.User.Id, new(project, "Workspace", dosi), ct);
        var state = JsonSerializer.SerializeToUtf8Bytes(new {
            roomId = project, ownerUserId = owner.User.Id, authorId = owner.User.Id, deviceId = owner.Device.Id,
            version = 1, epoch = 1, createdAtMs = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(), previousHash = "",
            members = new Dictionary<Guid, object> { [owner.User.Id] = new { } },
            recipients = new[] { new { userId = owner.User.Id, deviceId = owner.Device.Id, kemCiphertext = "AA==", @sealed = new { nonce = "AA==", ciphertext = "AA==" } } },
            previousKey = (object?)null
        }, TestJson.Options);
        await store.Projects.StoreKeysAsync(project, owner.User.Id, new(owner.Device.Id, Convert.ToBase64String(state),
            Convert.ToBase64String(owner.Fixture.Sign(Proofs.Tagged("kodosi-room-state-v1"u8, state)))), ct);
        return project;
    }

    private static ProjectService.ContentWrite Item(TestUser author, Guid project, Guid id, string kind)
    {
        var bytes = JsonSerializer.SerializeToUtf8Bytes(new { roomId = project, id, kind, version = 1, keyVersion = 1, epoch = 1,
            authorId = author.User.Id, deviceId = author.Device.Id, nonce = Convert.ToBase64String(new byte[12]),
            ciphertext = Convert.ToBase64String(new byte[40 * 1024]) }, TestJson.Options);
        return new(0, author.Device.Id, Convert.ToBase64String(bytes),
            Convert.ToBase64String(author.Fixture.Sign(Proofs.Tagged("kodosi-room-content-v1"u8, bytes))));
    }

    [Fact]
    public void TheSettingsDefineThePlansAndRefuseAWrongValue()
    {
        static PlanCatalog Read(Dictionary<string, string?> values) => PlanCatalog.From(new ConfigurationBuilder().AddInMemoryCollection(values).Build());
        Assert.Same(PlanCatalog.None, Read(new()));
        Assert.Equal(PlanLimits.Safety, PlanCatalog.None.For("free"));
        var catalog = Read(new()
        {
            ["Plans:Default"] = "free", ["Plans:Definitions:free:Devices"] = "4",
            ["Plans:Definitions:free:ProjectBytes"] = "10485760", ["Plans:Definitions:unlimited:Devices"] = "16",
        });
        Assert.Equal(PlanLimits.Safety with { Devices = 4, ProjectBytes = 10485760 }, catalog.For(null));
        Assert.Equal(16, catalog.For("unlimited").Devices);
        Assert.Same(catalog.For(null), catalog.For("gone"));
        Assert.Throws<InvalidOperationException>(() => Read(new() { ["Plans:Default"] = "pro", ["Plans:Definitions:free:Devices"] = "4" }));
        Assert.Throws<InvalidOperationException>(() => Read(new() { ["Plans:Definitions:free:Devices"] = "4" }));
        Assert.Throws<InvalidOperationException>(() => Read(new() { ["Plans:Default"] = "free", ["Plans:Definitions:free:Device"] = "4" }));
        Assert.Throws<InvalidOperationException>(() => Read(new() { ["Plans:Default"] = "free", ["Plans:Definitions:free:Devices"] = "17" }));
        Assert.Throws<InvalidOperationException>(() => Read(new() { ["Plans:Default"] = "free", ["Plans:Definitions:free:ProjectBytes"] = "1024" }));
    }

    [Fact]
    public async Task APlanLimitsProjectsPeopleAndSharesAndAnotherPlanLetsThemThrough()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        store.Plans = Catalog();
        var alice = await store.UserAsync("alice"); var bob = await store.UserAsync("bob"); var carol = await store.UserAsync("carol");
        await BefriendAsync(store, alice.User.Id, bob.User.Id); await BefriendAsync(store, alice.User.Id, carol.User.Id);
        var project = Guid.CreateVersion7();
        await store.Projects.CreateAsync(alice.User.Id, new(project, "Workspace"), ct);
        await RefusedAsync("You own 1 project, the most that you can own. Delete a project to make a new one.",
            () => store.Projects.CreateAsync(alice.User.Id, new(Guid.CreateVersion7(), "Second"), ct));
        await store.Projects.InviteAsync(project, alice.User.Id, new(Guid.CreateVersion7(), bob.User.Id), ct);
        await RefusedAsync("This project can have at most 2 people, with its open invitations.",
            () => store.Projects.InviteAsync(project, alice.User.Id, new(Guid.CreateVersion7(), carol.User.Id), ct));
        var terminal = Guid.CreateVersion7(); var incarnation = Guid.CreateVersion7();
        await store.Sessions.CreateAsync(alice.User.Id, alice.Approved, new(terminal, incarnation, "Shell", alice.Device.Id, "Mac", null), ct);
        await RefusedAsync("You can share a terminal with at most 1 person.",
            () => store.Sessions.ShareAsync(terminal, alice.User.Id, alice.Device.Id, new(incarnation, 1, [bob.User.Id, carol.User.Id]), ct));

        alice.User.Plan = "unlimited"; await store.Db.SaveChangesAsync(ct);
        await store.Projects.CreateAsync(alice.User.Id, new(Guid.CreateVersion7(), "Second"), ct);
        await store.Projects.InviteAsync(project, alice.User.Id, new(Guid.CreateVersion7(), carol.User.Id), ct);
        await store.Sessions.ShareAsync(terminal, alice.User.Id, alice.Device.Id, new(incarnation, 1, [bob.User.Id, carol.User.Id]), ct);
    }

    [Fact]
    public async Task FriendLimitsGiveTheReasonOfEachSide()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        store.Plans = Catalog();
        var alice = await store.UserAsync("alice"); var bob = await store.UserAsync("bob");
        var carol = await store.UserAsync("carol"); var dave = await store.UserAsync("dave");
        await store.Friends.MutateAsync(alice.User.Id, "bob", "send", ct);
        await RefusedAsync("You have 1 open friend request. Wait for an answer, or cancel a request.",
            () => store.Friends.MutateAsync(alice.User.Id, "carol", "send", ct));
        await RefusedAsync("This person has too many open friend requests. Try again later.",
            () => store.Friends.MutateAsync(carol.User.Id, "bob", "send", ct));
        await store.Friends.MutateAsync(bob.User.Id, "alice", "accept", ct);
        await RefusedAsync("You have 1 friend, the most that you can have. Remove a friend first.",
            () => store.Friends.MutateAsync(alice.User.Id, "dave", "send", ct));
        await RefusedAsync("This person has the most friends that they can have.",
            () => store.Friends.MutateAsync(dave.User.Id, "alice", "send", ct));
    }

    [Fact]
    public async Task TheDeviceLimitStopsALinkAndAnApprovalAndARemovedDeviceDoesNotCount()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        store.Plans = Catalog();
        var owner = await store.UserAsync("owner");
        DeviceService.LinkInit Init(DeviceFixture device) =>
            new(device.DeviceId, "New", Convert.ToBase64String(device.SigningKey), DeviceFixture.LinkNonce, DeviceFixture.LinkProof);
        var second = new DeviceFixture(owner.User.Id, "second-device");
        const string full = "This account has 1 device, the most that it can have. Remove a device in Settings on one of your devices, then try again.";
        await RefusedAsync(full, () => store.Devices.StartLinkAsync(owner.User.Id, Init(second), ct));

        owner.User.Plan = "unlimited"; await store.Db.SaveChangesAsync(ct);
        var link = JsonSerializer.SerializeToElement(await store.Devices.StartLinkAsync(owner.User.Id, Init(second), ct), TestJson.Options);
        owner.User.Plan = null; await store.Db.SaveChangesAsync(ct);
        var some = Convert.ToBase64String(new byte[32]);
        await RefusedAsync(full, () => store.Devices.ApproveLinkAsync(owner.User.Id, owner.Approved,
            new(link.GetProperty("requestId").GetGuid(), some, some, some, some, some), ct));

        owner.Device.Revoked = true; await store.Db.SaveChangesAsync(ct);
        await store.Devices.StartLinkAsync(owner.User.Id, Init(new DeviceFixture(owner.User.Id, "third-device")), ct);
    }

    [Fact]
    public async Task DosisProjectStaysWithinItsOwnSpaceOnAPlanWithoutALimit()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var alice = await store.UserAsync("alice");
        var project = await KeyedProjectAsync(store, alice, dosi: true);
        var records = Enumerable.Range(0, 80).Select(_ => Guid.CreateVersion7()).ToArray();
        foreach (var id in records) await store.Projects.PutContentAsync(project, id, alice.User.Id, Item(alice, project, id, "message"), ct);
        store.Db.ChangeTracker.Clear();
        var stored = await store.Db.Projects.SingleAsync(x => x.Id == project, ct);
        Assert.True(stored.HistoryTrimmed);
        Assert.InRange(stored.ContentBytes, 1, ProjectService.DosiProjectBytes);
        var kept = await store.Db.ProjectItems.Where(x => x.ProjectId == project).Select(x => x.Id).ToListAsync(ct);
        Assert.DoesNotContain(records[0], kept);
        Assert.Contains(records[^1], kept);
    }

    [Fact]
    public async Task ARoomKeepsItsNewestMessagesInItsSpaceAndRefusesTasksThatFillIt()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        store.Plans = Catalog();
        var alice = await store.UserAsync("alice"); var bob = await store.UserAsync("bob");
        var project = await KeyedProjectAsync(store, alice);
        Guid[] messages = [Guid.CreateVersion7(), Guid.CreateVersion7(), Guid.CreateVersion7()];
        foreach (var id in messages) await store.Projects.PutContentAsync(project, id, alice.User.Id, Item(alice, project, id, "message"), ct);
        store.Db.ChangeTracker.Clear();
        Assert.Equal(messages[1..], await store.Db.ProjectItems.Where(x => x.ProjectId == project).OrderBy(x => x.Sequence).Select(x => x.Id).ToListAsync(ct));
        var stored = await store.Db.Projects.SingleAsync(x => x.Id == project, ct);
        Assert.True(stored.HistoryTrimmed);
        Assert.Equal(await store.Db.ProjectItems.Where(x => x.ProjectId == project).SumAsync(x => (long)x.Body.Length + x.Signature.Length, ct), stored.ContentBytes);
        Assert.InRange(stored.ContentBytes, 1, PlanCatalog.LeastProjectBytes);
        var page = JsonSerializer.SerializeToElement(await store.Projects.ContentAsync(project, alice.User.Id, null, 0, null, null, ct), TestJson.Options);
        Assert.True(page.GetProperty("historyTrimmed").GetBoolean());

        var tasks = await KeyedProjectAsync(store, bob);
        foreach (var id in new[] { Guid.CreateVersion7(), Guid.CreateVersion7() })
            await store.Projects.PutContentAsync(tasks, id, bob.User.Id, Item(bob, tasks, id, "task"), ct);
        var full = Guid.CreateVersion7();
        await RefusedAsync("This project is full, because its tasks and repositories use all of its space. Make a new project to continue.",
            () => store.Projects.PutContentAsync(tasks, full, bob.User.Id, Item(bob, tasks, full, "task"), ct));
        store.Db.ChangeTracker.Clear();
        Assert.Equal(2, await store.Db.ProjectItems.CountAsync(x => x.ProjectId == tasks, ct));
        Assert.False((await store.Db.Projects.SingleAsync(x => x.Id == tasks, ct)).HistoryTrimmed);
    }

    [Fact]
    public async Task ThePlanSettingLimitsAnAccountAndTheRefusalSaysWhatToDo()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(ct), settings: new Dictionary<string, string?>
        {
            ["Plans:Default"] = "free", ["Plans:Definitions:free:OwnedProjects"] = "1",
        });
        using var owner = await app.EnrollAsync("owner");
        await CallAsync(owner, HttpMethod.Post, "/api/projects", new { id = Guid.CreateVersion7(), name = "Project" });
        using var second = await SignedAsync(owner, HttpMethod.Post, "/api/projects", new { id = Guid.CreateVersion7(), name = "Second" });
        using var response = await owner.Client.SendAsync(second, ct);
        Assert.Equal(HttpStatusCode.Forbidden, response.StatusCode);
        var body = await response.Content.ReadFromJsonAsync<JsonElement>(ct);
        Assert.Equal(["error", "limit", "most"], body.EnumerateObject().Select(x => x.Name));
        Assert.Equal("You own 1 project, the most that you can own. Delete a project to make a new one.", body.GetProperty("error").GetString());
        Assert.Equal("projects", body.GetProperty("limit").GetString());
        Assert.Equal(1, body.GetProperty("most").GetInt64());
    }
}
