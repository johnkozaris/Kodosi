using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Friends;
using Kodosi.Realtime;
using Kodosi.Rooms;
using Kodosi.Security;
using Kodosi.Sessions;
using Microsoft.EntityFrameworkCore;
using Xunit;

namespace Kodosi.HostTests;

internal sealed class TestStore : IAsyncDisposable
{
    private TestStore(KodosiDbContext db) { Db = db; }
    public KodosiDbContext Db { get; }
    public RelayDirectory Relay { get; } = new(TimeProvider.System);
    public DeviceService Devices => new(Db, new SignatureVerifier(), new DeviceCertificateParser(), new SignedDeviceListParser(), Relay, TimeProvider.System);
    public FriendService Friends => new(Db, Relay, TimeProvider.System);
    public RoomService Rooms => new(Db, Relay, Friends, TimeProvider.System);
    public SessionService Sessions => new(Db, Relay, Devices, Rooms, new SignatureVerifier(), TimeProvider.System);

    public static async Task<TestStore> CreateAsync(PostgresFixture postgres)
    {
        var connection = await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken);
        var store = new TestStore(PostgresFixture.Context(connection));
        await DatabaseSetup.InitializeAsync(store.Db, TestContext.Current.CancellationToken);
        return store;
    }
    public async Task<(User User, DeviceFixture Fixture, Device Device)> UserAsync(string handle)
    {
        var user = new User
        {
            Id = Guid.CreateVersion7(),
            Issuer = "https://test.example/",
            Subject = handle,
            Handle = handle,
            DisplayName = handle,
            IdentityIncarnationId = Guid.CreateVersion7(),
            IdentityRevision = 1
        };
        var fixture = new DeviceFixture(user.Id, $"{handle}-device");
        var issued = DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds();
        var cert = fixture.CertificateBody(fixture.DeviceId, issued);
        var list = DeviceFixture.ListBody(user.Id, 1, [(fixture.DeviceId, fixture.DeviceId)], fixture.DeviceId, issued);
        var device = new Device
        {
            Id = fixture.DeviceId,
            UserId = user.Id,
            Label = "Test device",
            SignerDeviceId = fixture.DeviceId,
            SigningPublicKey = fixture.SigningKey,
            KemPublicKey = fixture.KemKey,
            Certificate = cert,
            CertificateSignature = fixture.Sign(Proofs.Tagged(DomainTags.DeviceCertV2, cert)),
            IssuedAtMs = issued
        };
        Db.Users.Add(user); Db.Devices.Add(device); Db.DeviceLists.Add(new DeviceList
        {
            UserId = user.Id,
            Generation = 1,
            SignerDeviceId = fixture.DeviceId,
            Body = list,
            Signature = fixture.Sign(Proofs.Tagged(DomainTags.DeviceListV1, list)),
            IssuedAtMs = issued
        });
        await Db.SaveChangesAsync(TestContext.Current.CancellationToken); return (user, fixture, device);
    }
    public async ValueTask DisposeAsync()
    {
        Relay.StopAll(); await Db.DisposeAsync();
    }
}
