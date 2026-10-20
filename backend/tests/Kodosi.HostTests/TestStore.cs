using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Friends;
using Kodosi.TerminalConnections;
using Kodosi.Missions;
using Kodosi.Plans;
using Kodosi.Security;
using Kodosi.Sessions;
using Microsoft.EntityFrameworkCore;
using Xunit;

namespace Kodosi.HostTests;

internal sealed class TestStore : IAsyncDisposable
{
    private TestStore(KodosiDbContext db) { Db = db; }
    public KodosiDbContext Db { get; }
    public ConnectionDirectory Connections { get; } = new();
    public DeviceSessions DeviceSessions { get; } = new(TimeProvider.System);
    public PlanCatalog Plans { get; set; } = PlanCatalog.None;
    public DeviceService Devices => new(Db, Connections, DeviceSessions, TimeProvider.System, Plans);
    public Kodosi.Accounts.AccountService Accounts => new(Db, Connections, DeviceSessions, Gate, TimeProvider.System);
    public Kodosi.Admission.AccountGate Gate { get; } = new();
    public FriendService Friends => new(Db, Connections, TimeProvider.System, Plans);
    public MissionService Missions => new(Db, Connections, Friends, TimeProvider.System, Devices, Plans);
    public SessionService Sessions => new(Db, Connections, Missions, TimeProvider.System, Plans);

    public static async Task<TestStore> CreateAsync(PostgresFixture postgres)
    {
        var connection = await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken);
        var store = new TestStore(PostgresFixture.Context(connection));
        await DatabaseSetup.MigrateAsync(store.Db, TestContext.Current.CancellationToken);
        return store;
    }
    public TestStore Fork(params Microsoft.EntityFrameworkCore.Diagnostics.IInterceptor[] interceptors) =>
        new(new KodosiDbContext(new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(Db.Database.GetConnectionString()).AddInterceptors(interceptors).Options)) { Plans = Plans };
    public async Task<TestUser> UserAsync(string handle)
    {
        var user = new User
        {
            Id = Guid.CreateVersion7(),
            Issuer = "https://test.example/",
            Subject = handle,
            Handle = handle,
            DisplayName = handle,
            IdentityIncarnationId = Guid.CreateVersion7()
        };
        var fixture = new DeviceFixture(user.Id, $"{handle}-device");
        var issued = DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds();
        var cert = fixture.CertificateBody(fixture.DeviceId, issued);
        var list = DeviceFixture.ListBody(user.Id, 1, [(fixture.DeviceId, fixture.DeviceId)], fixture.DeviceId, issued);
        var device = new Device
        {
            Id = fixture.DeviceId,
            UserId = user.Id,
            SignerDeviceId = fixture.DeviceId,
            SigningPublicKey = fixture.SigningKey,
            Certificate = cert,
            CertificateSignature = fixture.Sign(Proofs.Tagged(DomainTags.DeviceCertV3, cert)),
            IssuedAtMs = issued
        };
        Db.Users.Add(user); Db.Devices.Add(device); Db.DeviceLists.Add(new DeviceList
        {
            UserId = user.Id,
            Generation = 1,
            Body = list,
            Signature = fixture.Sign(Proofs.Tagged(DomainTags.DeviceListV1, list)),
            IssuedAtMs = issued
        });
        await Db.SaveChangesAsync(TestContext.Current.CancellationToken); return new(user, fixture, device);
    }
    public async ValueTask DisposeAsync()
    {
        Connections.StopAll(); await Db.DisposeAsync();
    }
}

internal sealed record TestUser(User User, DeviceFixture Fixture, Device Device)
{
    public ApprovedDevice Approved => ApprovedDevice.From(Device);

    public SessionService.RoomChoiceWrite RoomChoice(Guid terminal, Guid incarnation, Guid? room, long sequence) =>
        new(Device.Id, sequence, Convert.ToBase64String(Fixture.Sign(Proofs.TerminalRoom(User.Id, terminal, incarnation, room, sequence, Device.Id))));
}

internal sealed class ManualClock : TimeProvider
{
    private DateTimeOffset now = DateTimeOffset.UtcNow;
    public override DateTimeOffset GetUtcNow() => now;
    public void Advance(TimeSpan duration) => now += duration;
}
