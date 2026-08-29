using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Identity;

namespace Kodosi.HostTests;

public sealed class DeviceEnrollmentVerifierTests
{
    private static readonly TimeSpan AllowedClockSkew = TimeSpan.FromMinutes(5);

    [Theory]
    [InlineData(0, true)]
    [InlineData(1, false)]
    public async Task Certificate_Future_IssuedAt_Uses_Exact_ReceivedAt_Skew_Boundary(
        long millisecondsPastBoundary,
        bool accepted)
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var fixture = CreateFixture(
            receivedAt,
            certificateIssuedAt: receivedAt.Add(AllowedClockSkew)
                .AddMilliseconds(millisecondsPastBoundary));

        var verification = VerifyAsync(fixture);

        if (accepted)
        {
            _ = await verification;
        }
        else
        {
            var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(() => verification);
            Assert.Equal(
                "Device certificate IssuedAtMs is too far in the future; check your system clock.",
                error.Message);
        }
        Assert.Equal(accepted ? 2 : 1, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(accepted ? 1 : 0, fixture.Lists.GetLatestCalls);
        Assert.Equal(accepted ? 1 : 0, fixture.Devices.GetByUserIdCalls);
    }

    [Theory]
    [InlineData(0, true)]
    [InlineData(1, false)]
    public async Task Signed_List_Future_IssuedAt_Uses_Exact_ReceivedAt_Skew_Boundary(
        long millisecondsPastBoundary,
        bool accepted)
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var fixture = CreateFixture(
            receivedAt,
            listIssuedAt: receivedAt.Add(AllowedClockSkew)
                .AddMilliseconds(millisecondsPastBoundary));

        var verification = VerifyAsync(fixture);

        if (accepted)
        {
            _ = await verification;
        }
        else
        {
            var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(() => verification);
            Assert.Equal(
                "Signed device list IssuedAtMs is too far in the future; check your system clock.",
                error.Message);
        }
        Assert.Equal(accepted ? 2 : 1, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(accepted ? 1 : 0, fixture.Lists.GetLatestCalls);
        Assert.Equal(accepted ? 1 : 0, fixture.Devices.GetByUserIdCalls);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task Certificate_Expiry_Outside_DateTimeOffset_Range_Rejects_Before_Repository_Reads(
        bool initialEnrollment)
    {
        var fixture = CreateFixture(
            DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000),
            certificateExpiresAtMs: long.MaxValue,
            initialEnrollment: initialEnrollment);

        var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(
            () => VerifyAsync(fixture));

        Assert.Equal(
            "Invalid device certificate: expires_at_ms exceeds the maximum representable Unix timestamp (9223372036854775807)",
            error.Message);
        AssertTemporalRejectionHasNoRepositoryIo(fixture);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task Signed_List_Expiry_Outside_DateTimeOffset_Range_Rejects_Before_Repository_Reads(
        bool initialEnrollment)
    {
        var fixture = CreateFixture(
            DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000),
            listExpiresAtMs: long.MaxValue,
            initialEnrollment: initialEnrollment);

        var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(
            () => VerifyAsync(fixture));

        Assert.Equal(
            "Invalid signed device list: expires_at_ms exceeds the maximum representable Unix timestamp (9223372036854775807)",
            error.Message);
        AssertTemporalRejectionHasNoRepositoryIo(fixture);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task Certificate_Issue_Time_Outside_DateTimeOffset_Range_Rejects_Before_Repository_Reads(
        bool initialEnrollment)
    {
        var fixture = CreateFixture(
            DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000),
            certificateIssuedAtMs: long.MaxValue,
            initialEnrollment: initialEnrollment);

        var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(
            () => VerifyAsync(fixture));

        Assert.Equal(
            "Invalid device certificate: issued_at_ms exceeds the maximum representable Unix timestamp (9223372036854775807)",
            error.Message);
        AssertTemporalRejectionHasNoRepositoryIo(fixture);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task Signed_List_Issue_Time_Outside_DateTimeOffset_Range_Rejects_Before_Repository_Reads(
        bool initialEnrollment)
    {
        var fixture = CreateFixture(
            DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000),
            listIssuedAtMs: long.MaxValue,
            initialEnrollment: initialEnrollment);

        var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(
            () => VerifyAsync(fixture));

        Assert.Equal(
            "Invalid signed device list: issued_at_ms exceeds the maximum representable Unix timestamp (9223372036854775807)",
            error.Message);
        AssertTemporalRejectionHasNoRepositoryIo(fixture);
    }

    [Theory]
    [InlineData(false, -1, false)]
    [InlineData(false, 0, false)]
    [InlineData(false, 1, true)]
    [InlineData(true, -1, false)]
    [InlineData(true, 0, false)]
    [InlineData(true, 1, true)]
    public async Task Certificate_Expiry_Uses_Exact_Receipt_Time_Boundary(
        bool initialEnrollment,
        long millisecondsAfterReceipt,
        bool accepted)
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var fixture = CreateFixture(
            receivedAt,
            certificateIssuedAt: receivedAt.AddDays(-1),
            certificateExpiresAt: receivedAt.AddMilliseconds(millisecondsAfterReceipt),
            initialEnrollment: initialEnrollment);

        var verification = VerifyAsync(fixture);

        if (accepted)
        {
            _ = await verification;
        }
        else
        {
            var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(() => verification);
            Assert.Equal("Device certificate is expired at receipt time.", error.Message);
        }
        AssertTemporalBoundaryRepositoryCalls(fixture, initialEnrollment, accepted);
    }

    [Theory]
    [InlineData(false, -1, false)]
    [InlineData(false, 0, false)]
    [InlineData(false, 1, true)]
    [InlineData(true, -1, false)]
    [InlineData(true, 0, false)]
    [InlineData(true, 1, true)]
    public async Task Signed_List_Expiry_Uses_Exact_Receipt_Time_Boundary(
        bool initialEnrollment,
        long millisecondsAfterReceipt,
        bool accepted)
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var fixture = CreateFixture(
            receivedAt,
            listIssuedAt: receivedAt.AddDays(-1),
            listExpiresAt: receivedAt.AddMilliseconds(millisecondsAfterReceipt),
            initialEnrollment: initialEnrollment);

        var verification = VerifyAsync(fixture);

        if (accepted)
        {
            _ = await verification;
        }
        else
        {
            var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(() => verification);
            Assert.Equal("Signed device list is expired at receipt time.", error.Message);
        }
        AssertTemporalBoundaryRepositoryCalls(fixture, initialEnrollment, accepted);
    }

    [Theory]
    [InlineData(false, -1, false)]
    [InlineData(false, 0, false)]
    [InlineData(false, 1, true)]
    [InlineData(true, -1, false)]
    [InlineData(true, 0, false)]
    [InlineData(true, 1, true)]
    public async Task Certificate_Expiry_Must_Be_Strictly_After_Issue_Time(
        bool initialEnrollment,
        long millisecondsAfterIssue,
        bool accepted)
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var issuedAt = receivedAt.AddMinutes(1);
        var fixture = CreateFixture(
            receivedAt,
            certificateIssuedAt: issuedAt,
            certificateExpiresAt: issuedAt.AddMilliseconds(millisecondsAfterIssue),
            initialEnrollment: initialEnrollment);

        var verification = VerifyAsync(fixture);

        if (accepted)
        {
            _ = await verification;
        }
        else
        {
            var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(() => verification);
            Assert.Equal(
                "Invalid device certificate: expires_at_ms must be greater than issued_at_ms",
                error.Message);
        }
        if (accepted)
        {
            AssertTemporalBoundaryRepositoryCalls(fixture, initialEnrollment, accepted: true);
        }
        else
        {
            AssertTemporalRejectionHasNoRepositoryIo(fixture);
        }
    }

    [Theory]
    [InlineData(false, -1, false)]
    [InlineData(false, 0, false)]
    [InlineData(false, 1, true)]
    [InlineData(true, -1, false)]
    [InlineData(true, 0, false)]
    [InlineData(true, 1, true)]
    public async Task Signed_List_Expiry_Must_Be_Strictly_After_Issue_Time(
        bool initialEnrollment,
        long millisecondsAfterIssue,
        bool accepted)
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var issuedAt = receivedAt.AddMinutes(1);
        var fixture = CreateFixture(
            receivedAt,
            listIssuedAt: issuedAt,
            listExpiresAt: issuedAt.AddMilliseconds(millisecondsAfterIssue),
            initialEnrollment: initialEnrollment);

        var verification = VerifyAsync(fixture);

        if (accepted)
        {
            _ = await verification;
        }
        else
        {
            var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(() => verification);
            Assert.Equal(
                "Invalid signed device list: expires_at_ms must be greater than issued_at_ms",
                error.Message);
        }
        if (accepted)
        {
            AssertTemporalBoundaryRepositoryCalls(fixture, initialEnrollment, accepted: true);
        }
        else
        {
            AssertTemporalRejectionHasNoRepositoryIo(fixture);
        }
    }

    [Theory]
    [InlineData(0, false)]
    [InlineData(1, true)]
    public async Task Current_List_Uses_Exact_Expiry_Boundary(
        long millisecondsAfterNow,
        bool accepted)
    {
        var now = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var fixture = CreateFixture(
            now,
            currentListExpiresAt: now.AddMilliseconds(millisecondsAfterNow));

        var verification = VerifyAsync(fixture);

        if (accepted)
        {
            _ = await verification;
        }
        else
        {
            var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(() => verification);
            Assert.Equal(
                "Signer device is not an enrolled, active device of the authenticated user.",
                error.Message);
        }
        Assert.Equal(2, fixture.Clock.GetUtcNowCalls);
    }

    [Theory]
    [InlineData(0, false)]
    [InlineData(1, true)]
    public async Task Signer_Certificate_Uses_Exact_Expiry_Boundary(
        long millisecondsAfterNow,
        bool accepted)
    {
        var now = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var fixture = CreateFixture(
            now,
            signerCertificateExpiresAt: now.AddMilliseconds(millisecondsAfterNow));

        var verification = VerifyAsync(fixture);

        if (accepted)
        {
            _ = await verification;
        }
        else
        {
            var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(() => verification);
            Assert.Equal(
                "Signer device is not an enrolled, active device of the authenticated user.",
                error.Message);
        }
        Assert.Equal(2, fixture.Clock.GetUtcNowCalls);
    }

    [Fact]
    public async Task Successful_Subsequent_Verification_Captures_Receipt_And_Authorization_Time()
    {
        var now = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var fixture = CreateFixture(now);

        _ = await VerifyAsync(fixture);

        Assert.Equal(2, fixture.Clock.GetUtcNowCalls);
    }

    [Fact]
    public async Task Receipt_Time_Precedes_Awaited_List_Read_While_Authorization_Follows_It()
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var authorizationAt = receivedAt.AddMinutes(10);
        var fixture = CreateFixture(
            receivedAt,
            certificateIssuedAt: receivedAt.Add(AllowedClockSkew),
            listIssuedAt: receivedAt.Add(AllowedClockSkew),
            currentListExpiresAt: authorizationAt.AddMilliseconds(1),
            gateCurrentListRead: true);

        var verification = VerifyAsync(fixture);
        await fixture.Lists.ReadStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);
        Assert.Equal(1, fixture.Clock.GetUtcNowCalls);
        fixture.Clock.Advance(authorizationAt - receivedAt);
        fixture.Lists.CompleteRead();

        _ = await verification;

        Assert.Equal(2, fixture.Clock.GetUtcNowCalls);
    }

    [Theory]
    [InlineData(false, true)]
    [InlineData(false, false)]
    [InlineData(true, true)]
    [InlineData(true, false)]
    public async Task Submitted_Artifact_Expiry_During_Required_Read_Rejects_At_Authorization(
        bool initialEnrollment,
        bool certificateArtifact)
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var expiresAt = receivedAt.AddMilliseconds(1);
        var fixture = CreateFixture(
            receivedAt,
            certificateExpiresAt: certificateArtifact ? expiresAt : null,
            listExpiresAt: certificateArtifact ? null : expiresAt,
            currentListExpiresAt: receivedAt.AddMinutes(20),
            signerCertificateExpiresAt: receivedAt.AddMinutes(20),
            gateCurrentListRead: initialEnrollment,
            gateDeviceRead: !initialEnrollment,
            initialEnrollment: initialEnrollment);

        var verification = VerifyAsync(fixture);
        if (initialEnrollment)
        {
            await fixture.Lists.ReadStarted.Task.WaitAsync(
                TestContext.Current.CancellationToken);
        }
        else
        {
            await fixture.Devices.ReadStarted.Task.WaitAsync(
                TestContext.Current.CancellationToken);
        }
        Assert.Equal(1, fixture.Clock.GetUtcNowCalls);
        fixture.Clock.Advance(TimeSpan.FromMinutes(10));
        if (initialEnrollment)
        {
            fixture.Lists.CompleteRead();
        }
        else
        {
            fixture.Devices.CompleteRead();
        }

        var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(() => verification);

        Assert.Equal(
            certificateArtifact
                ? "Device certificate is expired at authorization time."
                : "Signed device list is expired at authorization time.",
            error.Message);
        Assert.Equal(2, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(1, fixture.Lists.GetLatestCalls);
        Assert.Equal(initialEnrollment ? 0 : 1, fixture.Devices.GetByUserIdCalls);
        Assert.Equal(0, fixture.Devices.GetByDeviceIdCalls);
    }

    [Fact]
    public async Task Invalid_Signature_Wins_When_Artifact_Expires_During_Required_Read()
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var fixture = CreateFixture(
            receivedAt,
            certificateExpiresAt: receivedAt.AddMilliseconds(1),
            currentListExpiresAt: receivedAt.AddMinutes(20),
            signerCertificateExpiresAt: receivedAt.AddMinutes(20),
            gateDeviceRead: true,
            signatureValid: false);

        var verification = VerifyAsync(fixture);
        await fixture.Devices.ReadStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);
        Assert.Equal(1, fixture.Clock.GetUtcNowCalls);
        fixture.Clock.Advance(TimeSpan.FromMinutes(10));
        fixture.Devices.CompleteRead();

        var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(() => verification);

        Assert.Equal("Device certificate signature is invalid.", error.Message);
        Assert.Equal(2, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(1, fixture.Lists.GetLatestCalls);
        Assert.Equal(1, fixture.Devices.GetByUserIdCalls);
        Assert.Equal(0, fixture.Devices.GetByDeviceIdCalls);
    }

    [Fact]
    public async Task Initial_Enrollment_Captures_Authorization_After_Current_List_Read()
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var fixture = CreateFixture(
            receivedAt,
            certificateIssuedAt: receivedAt.Add(AllowedClockSkew),
            listIssuedAt: receivedAt.Add(AllowedClockSkew),
            gateCurrentListRead: true,
            initialEnrollment: true);

        var verification = VerifyAsync(fixture);
        await fixture.Lists.ReadStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);
        Assert.Equal(1, fixture.Clock.GetUtcNowCalls);
        fixture.Clock.Advance(TimeSpan.FromMinutes(10));
        fixture.Lists.CompleteRead();

        _ = await verification;

        Assert.Equal(2, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(0, fixture.Devices.GetByUserIdCalls);
    }

    [Fact]
    public async Task Current_List_Expiry_During_Awaited_Device_Read_Rejects_Enrollment()
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var fixture = CreateFixture(
            receivedAt,
            currentListExpiresAt: receivedAt.AddMilliseconds(1),
            gateDeviceRead: true);

        var verification = VerifyAsync(fixture);
        await fixture.Devices.ReadStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);
        Assert.Equal(1, fixture.Clock.GetUtcNowCalls);
        fixture.Clock.Advance(TimeSpan.FromMinutes(10));
        fixture.Devices.CompleteRead();

        var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(() => verification);

        Assert.Equal(
            "Signer device is not an enrolled, active device of the authenticated user.",
            error.Message);
        Assert.Equal(2, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(1, fixture.Devices.GetByUserIdCalls);
        Assert.Equal(0, fixture.Devices.GetByDeviceIdCalls);
    }

    [Fact]
    public async Task Signer_Certificate_Expiry_During_Awaited_Device_Read_Rejects_Enrollment()
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var fixture = CreateFixture(
            receivedAt,
            signerCertificateExpiresAt: receivedAt.AddMilliseconds(1),
            gateDeviceRead: true);

        var verification = VerifyAsync(fixture);
        await fixture.Devices.ReadStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);
        Assert.Equal(1, fixture.Clock.GetUtcNowCalls);
        fixture.Clock.Advance(TimeSpan.FromMinutes(10));
        fixture.Devices.CompleteRead();

        var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(() => verification);

        Assert.Equal(
            "Signer device is not an enrolled, active device of the authenticated user.",
            error.Message);
        Assert.Equal(2, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(1, fixture.Devices.GetByUserIdCalls);
        Assert.Equal(0, fixture.Devices.GetByDeviceIdCalls);
    }

    [Fact]
    public async Task Bootstrap_Certificate_Future_IssuedAt_Rejects_Before_Repository_Reads()
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var fixture = CreateFixture(
            receivedAt,
            certificateIssuedAt: receivedAt.Add(AllowedClockSkew).AddMilliseconds(1),
            initialEnrollment: true);

        var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(
            () => VerifyAsync(fixture));

        Assert.Equal(
            "Device certificate IssuedAtMs is too far in the future; check your system clock.",
            error.Message);
        Assert.Equal(1, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(0, fixture.Lists.GetLatestCalls);
        Assert.Equal(0, fixture.Devices.GetByUserIdCalls);
        Assert.Equal(0, fixture.Devices.GetByDeviceIdCalls);
    }

    [Fact]
    public async Task Bootstrap_Signed_List_Future_IssuedAt_Rejects_Before_Repository_Reads()
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var fixture = CreateFixture(
            receivedAt,
            listIssuedAt: receivedAt.Add(AllowedClockSkew).AddMilliseconds(1),
            initialEnrollment: true);

        var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(
            () => VerifyAsync(fixture));

        Assert.Equal(
            "Signed device list IssuedAtMs is too far in the future; check your system clock.",
            error.Message);
        Assert.Equal(1, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(0, fixture.Lists.GetLatestCalls);
        Assert.Equal(0, fixture.Devices.GetByUserIdCalls);
        Assert.Equal(0, fixture.Devices.GetByDeviceIdCalls);
    }

    [Fact]
    public async Task Malformed_Certificate_Rejects_Before_Clock_Or_Repository_Reads()
    {
        var fixture = CreateFixture(
            DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000)) with
        {
            Certificate = [0],
        };

        var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(
            () => VerifyAsync(fixture));

        Assert.StartsWith("Invalid device certificate:", error.Message);
        Assert.Equal(0, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(0, fixture.Lists.GetLatestCalls);
        Assert.Equal(0, fixture.Devices.GetByUserIdCalls);
        Assert.Equal(0, fixture.Devices.GetByDeviceIdCalls);
    }

    [Fact]
    public async Task Request_Certificate_Mismatch_Rejects_Before_Clock_Or_Repository_Reads()
    {
        var fixture = CreateFixture(
            DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000)) with
        {
            DeviceId = "different-device",
        };

        var error = await Assert.ThrowsAsync<DeviceEnrollmentException>(
            () => VerifyAsync(fixture));

        Assert.Equal(
            "Certificate device_id does not match request device_id.",
            error.Message);
        Assert.Equal(0, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(0, fixture.Lists.GetLatestCalls);
        Assert.Equal(0, fixture.Devices.GetByUserIdCalls);
        Assert.Equal(0, fixture.Devices.GetByDeviceIdCalls);
    }

    [Fact]
    public async Task Valid_Initial_Enrollment_Reads_Only_Current_List_And_Captures_Two_Times()
    {
        var fixture = CreateFixture(
            DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000),
            initialEnrollment: true);

        _ = await VerifyAsync(fixture);

        Assert.Equal(2, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(1, fixture.Lists.GetLatestCalls);
        Assert.Equal(0, fixture.Devices.GetByUserIdCalls);
        Assert.Equal(0, fixture.Devices.GetByDeviceIdCalls);
    }

    [Fact]
    public async Task Cancellation_During_Current_List_Read_Stops_Before_Device_Read_Or_Authorization_Time()
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var fixture = CreateFixture(receivedAt, gateCurrentListRead: true);
        using var cancellation = CancellationTokenSource.CreateLinkedTokenSource(
            TestContext.Current.CancellationToken);

        var verification = VerifyWithCancellationAsync(fixture, cancellation.Token);
        await fixture.Lists.ReadStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);
        Assert.Equal(cancellation.Token, fixture.Lists.LastCancellationToken);
        cancellation.Cancel();

        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => verification);

        Assert.Equal(1, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(1, fixture.Lists.GetLatestCalls);
        Assert.Equal(0, fixture.Devices.GetByUserIdCalls);
        Assert.Equal(0, fixture.Devices.GetByDeviceIdCalls);
    }

    [Fact]
    public async Task Cancellation_During_Device_Read_Stops_Before_Authorization_Time()
    {
        var receivedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_800_000_000_000);
        var fixture = CreateFixture(receivedAt, gateDeviceRead: true);
        using var cancellation = CancellationTokenSource.CreateLinkedTokenSource(
            TestContext.Current.CancellationToken);

        var verification = VerifyWithCancellationAsync(fixture, cancellation.Token);
        await fixture.Devices.ReadStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);
        Assert.Equal(cancellation.Token, fixture.Lists.LastCancellationToken);
        Assert.Equal(cancellation.Token, fixture.Devices.LastCancellationToken);
        cancellation.Cancel();

        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => verification);

        Assert.Equal(1, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(1, fixture.Lists.GetLatestCalls);
        Assert.Equal(1, fixture.Devices.GetByUserIdCalls);
        Assert.Equal(0, fixture.Devices.GetByDeviceIdCalls);
    }

    private static EnrollmentFixture CreateFixture(
        DateTimeOffset now,
        DateTimeOffset? certificateIssuedAt = null,
        long? certificateIssuedAtMs = null,
        DateTimeOffset? certificateExpiresAt = null,
        long? certificateExpiresAtMs = null,
        DateTimeOffset? listIssuedAt = null,
        long? listIssuedAtMs = null,
        DateTimeOffset? listExpiresAt = null,
        long? listExpiresAtMs = null,
        DateTimeOffset? currentListExpiresAt = null,
        DateTimeOffset? signerCertificateExpiresAt = null,
        bool gateCurrentListRead = false,
        bool gateDeviceRead = false,
        bool initialEnrollment = false,
        bool signatureValid = true)
    {
        var userId = UserId.New();
        const string signerDeviceId = "signer-device";
        const string newDeviceId = "new-device";
        var signerKey = new byte[1952];
        signerKey[0] = 7;
        var newKemKey = new byte[1184];
        var newSigningKey = new byte[1952];
        newKemKey[0] = 11;
        newSigningKey[0] = 13;

        var signer = TestDeviceCertificate.CreateDevice(
            userId,
            signerDeviceId,
            kemPublicKey: new byte[1184],
            signingPublicKey: signerKey,
            deviceLabel: "Signer",
            signerDeviceId: signerDeviceId,
            issuedAt: now.AddDays(-2),
            expiresAt: signerCertificateExpiresAt ?? now.AddDays(2));

        var currentList = TestDeviceList.Create(
            userId,
            1,
            TestDeviceList.Entries([(signerDeviceId, signerDeviceId)]),
            signerDeviceId,
            [2],
            now.AddDays(-2).ToUnixTimeMilliseconds(),
            (currentListExpiresAt ?? now.AddDays(2)).ToUnixTimeMilliseconds());
        var certificateSignerDeviceId = initialEnrollment ? newDeviceId : signerDeviceId;
        var certificate = BuildCertificate(
            userId,
            newDeviceId,
            certificateSignerDeviceId,
            newKemKey,
            newSigningKey,
            certificateIssuedAtMs ?? (certificateIssuedAt ?? now).ToUnixTimeMilliseconds(),
            certificateExpiresAtMs
                ?? certificateExpiresAt?.ToUnixTimeMilliseconds());
        var signedList = BuildList(
            userId,
            signerDeviceId,
            newDeviceId,
            listIssuedAtMs ?? (listIssuedAt ?? now).ToUnixTimeMilliseconds(),
            listExpiresAtMs ?? listExpiresAt?.ToUnixTimeMilliseconds(),
            initialEnrollment);
        var lists = new GatedDeviceListRepository(
            initialEnrollment ? null : currentList,
            gateCurrentListRead);
        var devices = new GatedDeviceRepository(signer, gateDeviceRead);
        var clock = new CountingTimeProvider(now);
        var verifier = new DeviceEnrollmentVerifier(
            devices,
            lists,
            new ResultSignatureVerifier(signatureValid),
            new DeviceCertificateParser(),
            new SignedDeviceListParser(),
            clock);
        return new EnrollmentFixture(
            verifier,
            userId,
            newDeviceId,
            newKemKey,
            newSigningKey,
            certificate,
            signedList,
            clock,
            lists,
            devices);
    }

    private static Task<VerifiedDeviceLinkEnrollment> VerifyAsync(EnrollmentFixture fixture) =>
        fixture.Verifier.VerifyAsync(
            fixture.UserId,
            fixture.DeviceId,
            fixture.KemPublicKey,
            fixture.SigningPublicKey,
            fixture.Certificate,
            [1],
            fixture.SignedList,
            [2],
            TestContext.Current.CancellationToken);

#pragma warning disable xUnit1051
    private static Task<VerifiedDeviceLinkEnrollment> VerifyWithCancellationAsync(
        EnrollmentFixture fixture,
        CancellationToken ct) =>
        fixture.Verifier.VerifyAsync(
            fixture.UserId,
            fixture.DeviceId,
            fixture.KemPublicKey,
            fixture.SigningPublicKey,
            fixture.Certificate,
            [1],
            fixture.SignedList,
            [2],
            ct);
#pragma warning restore xUnit1051

    private static byte[] BuildCertificate(
        UserId userId,
        string deviceId,
        string signerDeviceId,
        byte[] kemPublicKey,
        byte[] signingPublicKey,
        long issuedAtMs,
        long? expiresAtMs = null)
    {
        using var body = new MemoryStream();
        IdentityWireTestWriter.WriteString(body, userId.Value.ToString());
        IdentityWireTestWriter.WriteString(body, deviceId);
        IdentityWireTestWriter.WriteString(body, "New device");
        IdentityWireTestWriter.WriteString(body, signerDeviceId);
        IdentityWireTestWriter.WriteBytes(body, kemPublicKey);
        IdentityWireTestWriter.WriteBytes(body, signingPublicKey);
        IdentityWireTestWriter.WriteUInt64(body, (ulong)issuedAtMs);
        IdentityWireTestWriter.WriteUInt64(body, (ulong)(expiresAtMs ?? 0));
        return body.ToArray();
    }

    private static byte[] BuildList(
        UserId userId,
        string signerDeviceId,
        string newDeviceId,
        long issuedAtMs,
        long? expiresAtMs = null,
        bool initialEnrollment = false)
    {
        using var body = new MemoryStream();
        IdentityWireTestWriter.WriteString(body, userId.Value.ToString());
        IdentityWireTestWriter.WriteUInt64(body, initialEnrollment ? 1UL : 2UL);
        IdentityWireTestWriter.WriteUInt32(body, initialEnrollment ? 1U : 2U);
        if (!initialEnrollment)
        {
            IdentityWireTestWriter.WriteString(body, signerDeviceId);
            IdentityWireTestWriter.WriteString(body, signerDeviceId);
        }
        IdentityWireTestWriter.WriteString(body, newDeviceId);
        IdentityWireTestWriter.WriteString(
            body,
            initialEnrollment ? newDeviceId : signerDeviceId);
        IdentityWireTestWriter.WriteString(
            body,
            initialEnrollment ? newDeviceId : signerDeviceId);
        IdentityWireTestWriter.WriteUInt64(body, (ulong)issuedAtMs);
        IdentityWireTestWriter.WriteUInt64(body, (ulong)(expiresAtMs ?? 0));
        return body.ToArray();
    }

    private static void AssertTemporalRejectionHasNoRepositoryIo(EnrollmentFixture fixture)
    {
        Assert.Equal(0, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(0, fixture.Lists.GetLatestCalls);
        Assert.Equal(0, fixture.Devices.GetByUserIdCalls);
        Assert.Equal(0, fixture.Devices.GetByDeviceIdCalls);
    }

    private static void AssertTemporalBoundaryRepositoryCalls(
        EnrollmentFixture fixture,
        bool initialEnrollment,
        bool accepted)
    {
        Assert.Equal(accepted ? 2 : 1, fixture.Clock.GetUtcNowCalls);
        Assert.Equal(accepted ? 1 : 0, fixture.Lists.GetLatestCalls);
        Assert.Equal(accepted && !initialEnrollment ? 1 : 0, fixture.Devices.GetByUserIdCalls);
        Assert.Equal(0, fixture.Devices.GetByDeviceIdCalls);
    }

    private sealed record EnrollmentFixture(
        DeviceEnrollmentVerifier Verifier,
        UserId UserId,
        string DeviceId,
        byte[] KemPublicKey,
        byte[] SigningPublicKey,
        byte[] Certificate,
        byte[] SignedList,
        CountingTimeProvider Clock,
        GatedDeviceListRepository Lists,
        GatedDeviceRepository Devices);

    private sealed class ResultSignatureVerifier(bool result) : IPopSignatureVerifier
    {
        public bool Verify(
            ReadOnlySpan<byte> publicKey,
            ReadOnlySpan<byte> message,
            ReadOnlySpan<byte> signature) => result;
    }

    private sealed class CountingTimeProvider(DateTimeOffset now) : TimeProvider
    {
        private DateTimeOffset _now = now;

        public int GetUtcNowCalls { get; private set; }

        public override DateTimeOffset GetUtcNow()
        {
            GetUtcNowCalls++;
            return _now;
        }

        public void Advance(TimeSpan delta) => _now = _now.Add(delta);
    }

    private sealed class GatedDeviceRepository(
        UserDevice device,
        bool gateRead) : IUserDeviceRepository
    {
        private readonly TaskCompletionSource<IReadOnlyList<UserDevice>> _readCompletion =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public TaskCompletionSource ReadStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public int GetByUserIdCalls { get; private set; }
        public int GetByDeviceIdCalls { get; private set; }
        public CancellationToken LastCancellationToken { get; private set; }

        public Task<IReadOnlyList<UserDevice>> GetByUserIdAsync(
            UserId userId,
            CancellationToken ct = default)
        {
            GetByUserIdCalls++;
            LastCancellationToken = ct;
            Assert.Equal(device.UserId, userId);
            ReadStarted.TrySetResult();
            return gateRead
                ? _readCompletion.Task.WaitAsync(ct)
                : Task.FromResult<IReadOnlyList<UserDevice>>([device]);
        }

        public void CompleteRead() => _readCompletion.TrySetResult([device]);

        public Task<UserDevice?> GetByDeviceIdAsync(
            string deviceId,
            CancellationToken ct = default)
        {
            GetByDeviceIdCalls++;
            throw new NotSupportedException();
        }

        public Task<IReadOnlyDictionary<string, UserId>> GetUserIdsByDeviceIdsAsync(
            IReadOnlyCollection<string> deviceIds,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task<IReadOnlyList<UserDevice>> GetByUserIdsAsync(
            IReadOnlyList<UserId> userIds,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task AddAsync(UserDevice addedDevice, CancellationToken ct = default) =>
            throw new NotSupportedException();

        public void Update(UserDevice updatedDevice) => throw new NotSupportedException();

        public Task<int> RemoveAllForUserAsync(
            UserId userId,
            CancellationToken ct = default) => throw new NotSupportedException();
    }

    private sealed class GatedDeviceListRepository(
        UserDeviceList? currentList,
        bool gateRead) : IUserDeviceListRepository
    {
        private readonly TaskCompletionSource<UserDeviceList?> _readCompletion =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public TaskCompletionSource ReadStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public int GetLatestCalls { get; private set; }
        public CancellationToken LastCancellationToken { get; private set; }

        public Task<UserDeviceList?> GetLatestAsync(
            UserId userId,
            CancellationToken ct = default)
        {
            GetLatestCalls++;
            LastCancellationToken = ct;
            Assert.True(currentList is null || currentList.UserId == userId);
            ReadStarted.TrySetResult();
            return gateRead
                ? _readCompletion.Task.WaitAsync(ct)
                : Task.FromResult<UserDeviceList?>(currentList);
        }

        public void CompleteRead() => _readCompletion.TrySetResult(currentList);

        public Task<UserDeviceList?> GetGenerationAsync(
            UserId userId,
            long generation,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task<IReadOnlyList<UserDeviceList>> GetLatestByUserIdsAsync(
            IReadOnlyCollection<UserId> userIds,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task AddAsync(UserDeviceList list, CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<int> RemoveAllForUserAsync(
            UserId userId,
            CancellationToken ct = default) => throw new NotSupportedException();
    }
}
