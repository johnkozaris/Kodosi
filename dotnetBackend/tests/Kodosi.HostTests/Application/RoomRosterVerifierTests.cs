using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class RoomRosterVerifierTests
{
    [Fact]
    public async Task VerifyAsync_Rejects_Invalid_Signature()
    {
        var fixture = CreateFixture(signatureValid: false);
        var body = RoomRosterTestData.Build(
            fixture.Room,
            2,
            [fixture.Room.OwnerUserId, fixture.MemberId]);

        await Assert.ThrowsAsync<PolicyViolationException>(() =>
            fixture.Verifier.VerifyAsync(
                fixture.Room.Id,
                fixture.Room.OwnerUserId,
                2,
                body,
                [1],
                "owner-device",
                [fixture.Room.OwnerUserId, fixture.MemberId],
                TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task VerifyAsync_Rejects_Membership_That_Does_Not_Match_Mutation()
    {
        var fixture = CreateFixture(signatureValid: true);
        var body = RoomRosterTestData.Build(
            fixture.Room,
            2,
            [fixture.Room.OwnerUserId]);

        await Assert.ThrowsAsync<DomainException>(() =>
            fixture.Verifier.VerifyAsync(
                fixture.Room.Id,
                fixture.Room.OwnerUserId,
                2,
                body,
                [1],
                "owner-device",
                [fixture.Room.OwnerUserId, fixture.MemberId],
                TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task VerifyAsync_Accepts_Active_Signer_And_Matching_Roster()
    {
        var fixture = CreateFixture(signatureValid: true);
        var body = RoomRosterTestData.Build(
            fixture.Room,
            2,
            [fixture.Room.OwnerUserId, fixture.MemberId]);

        await fixture.Verifier.VerifyAsync(
            fixture.Room.Id,
            fixture.Room.OwnerUserId,
            2,
            body,
            [1],
            "owner-device",
            [fixture.Room.OwnerUserId, fixture.MemberId],
            TestContext.Current.CancellationToken);
    }

    [Fact]
    public async Task VerifyAsync_Rejects_Unrequested_Extra_Member()
    {
        var fixture = CreateFixture(signatureValid: true);
        var body = RoomRosterTestData.Build(
            fixture.Room,
            2,
            [fixture.Room.OwnerUserId, fixture.MemberId, UserId.New()]);

        await Assert.ThrowsAsync<DomainException>(() =>
            fixture.Verifier.VerifyAsync(
                fixture.Room.Id,
                fixture.Room.OwnerUserId,
                2,
                body,
                [1],
                "owner-device",
                [fixture.Room.OwnerUserId, fixture.MemberId],
                TestContext.Current.CancellationToken));
    }

    private static Fixture CreateFixture(bool signatureValid)
    {
        var ownerId = UserId.New();
        var memberId = UserId.New();
        var room = Room.Create(
            RoomId.From(Guid.NewGuid()),
            ownerId,
            "Room",
            "room",
            1,
            [1],
            [2],
            "owner-device");
        var device = TestDeviceCertificate.CreateDevice(
            ownerId,
            "owner-device",
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Owner device",
            signerDeviceId: "owner-device",
            issuedAt: DateTimeOffset.UtcNow.AddMinutes(-1),
            expiresAt: null);
        var list = TestDeviceList.Create(
            ownerId,
            1,
            """[{"deviceId":"owner-device","signerDeviceId":"owner-device"}]""",
            "owner-device",
            [2],
            DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds(),
            null);

        return new Fixture(
            room,
            memberId,
            new RoomRosterVerifier(
                new FakeUserDeviceRepository(device),
                new FakeUserDeviceListRepository(list),
                new ConfigurableSignatureVerifier(signatureValid),
                TimeProvider.System));
    }

    private sealed record Fixture(
        Room Room,
        UserId MemberId,
        RoomRosterVerifier Verifier);

    private sealed class ConfigurableSignatureVerifier(bool result) : IPopSignatureVerifier
    {
        public bool Verify(
            ReadOnlySpan<byte> publicKey,
            ReadOnlySpan<byte> message,
            ReadOnlySpan<byte> signature) => result;
    }
}
