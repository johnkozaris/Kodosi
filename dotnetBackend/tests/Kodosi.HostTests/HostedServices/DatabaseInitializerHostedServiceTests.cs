using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host;
using Kodosi.Infrastructure.Persistence;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Infrastructure;
using Microsoft.EntityFrameworkCore.Migrations;
using Microsoft.Extensions.DependencyInjection;
using Testcontainers.PostgreSql;

namespace Kodosi.HostTests;

public sealed class DatabaseInitializerHostedServiceTests
{
    [Fact]
    public async Task Unknown_Applied_Migration_Refuses_To_Start()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_unknown_migration_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);

        var services = new ServiceCollection()
            .AddLogging()
            .AddDbContext<KodosiDbContext>(options =>
                options.UseNpgsql(postgres.GetConnectionString()))
            .BuildServiceProvider();
        await using var provider = services;
        await using (var setupScope = provider.CreateAsyncScope())
        {
            var setup = setupScope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            await setup.Database.ExecuteSqlRawAsync(
                """
                CREATE TABLE "__EFMigrationsHistory" (
                    "MigrationId" character varying(150) NOT NULL,
                    "ProductVersion" character varying(32) NOT NULL,
                    CONSTRAINT "PK___EFMigrationsHistory" PRIMARY KEY ("MigrationId")
                );
                INSERT INTO "__EFMigrationsHistory" ("MigrationId", "ProductVersion")
                VALUES ('99999999999999_FutureMigration', '10.0.10');
                """,
                TestContext.Current.CancellationToken);
        }

        var exception = await Assert.ThrowsAsync<InvalidOperationException>(() =>
            DatabaseInitializerHostedService.InitializeAsync(provider));

        Assert.Contains("Database schema is ahead of this build", exception.Message);
        Assert.Contains("99999999999999_FutureMigration", exception.Message);
    }

    [Fact]
    public async Task Certificate_Authority_Cutover_Runs_Cryptographic_Preflight_Before_Drop()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_certificate_preflight_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);

        var verifier = new RecordingSignatureVerifier(result: true);
        var services = new ServiceCollection()
            .AddLogging()
            .AddSingleton<IPopSignatureVerifier>(verifier)
            .AddDbContext<KodosiDbContext>(options =>
                options.UseNpgsql(postgres.GetConnectionString()))
            .BuildServiceProvider();
        await using var provider = services;
        var userId = UserId.New();
        const string deviceId = "device-1";
        var signingKey = Enumerable.Repeat((byte)4, 1952).ToArray();
        var body = TestDeviceCertificate.Body(
            userId,
            deviceId,
            "Test device",
            deviceId,
            new byte[1184],
            signingKey,
            1_700_000_000_000,
            null);
        var signature = TestDeviceCertificate.Signature(9);
        var listBody = TestDeviceList.Body(
            userId,
            1,
            TestDeviceList.Entries([(deviceId, deviceId)]),
            deviceId,
            1_700_000_000_001,
            null);
        var listSignature = TestDeviceCertificate.Signature(10);
        await using (var setupScope = provider.CreateAsyncScope())
        {
            var setup = setupScope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            await setup.GetService<IMigrator>().MigrateAsync(
                DeviceCertificateAuthorityMigrationPreflight.DeviceListCollapsePredecessor,
                TestContext.Current.CancellationToken);
            setup.Users.Add(User.Create(
                userId,
                $"{userId.Value:N}@example.test",
                "certificate-preflight",
                "Certificate preflight"));
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
            await InsertLegacyDeviceAsync(
                setup,
                userId,
                deviceId,
                signingKey,
                body,
                signature);
            await InsertLegacyDeviceListAsync(
                setup,
                userId,
                1,
                [(deviceId, deviceId)],
                deviceId,
                listBody,
                listSignature,
                1_700_000_000_001,
                null);
        }

        await DatabaseInitializerHostedService.InitializeAsync(provider);

        Assert.Collection(
            verifier.Calls,
            call =>
            {
                Assert.Equal(signingKey, call.PublicKey);
                Assert.Equal(signature, call.Signature);
                Assert.Equal(
                    DeviceCertificateParser.DomainTag.ToArray().Concat(body).ToArray(),
                    call.Message);
            },
            call =>
            {
                Assert.Equal(signingKey, call.PublicKey);
                Assert.Equal(listSignature, call.Signature);
                Assert.Equal(
                    SignedDeviceListParser.DomainTag.ToArray().Concat(listBody).ToArray(),
                    call.Message);
            });
        await using var checkScope = provider.CreateAsyncScope();
        Assert.Contains(
            DeviceCertificateAuthorityMigrationPreflight.Migration,
            await checkScope.ServiceProvider.GetRequiredService<KodosiDbContext>()
                .Database.GetAppliedMigrationsAsync(TestContext.Current.CancellationToken));
    }

    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(2)]
    public async Task Certificate_Authority_Cutover_Stops_On_Invalid_Cryptographic_State(
        int failureMode)
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_certificate_preflight_rejection_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);

        var services = new ServiceCollection()
            .AddLogging()
            .AddSingleton<IPopSignatureVerifier>(
                new RecordingSignatureVerifier(result: failureMode != 0))
            .AddDbContext<KodosiDbContext>(options =>
                options.UseNpgsql(postgres.GetConnectionString()))
            .BuildServiceProvider();
        await using var provider = services;
        var userId = UserId.New();
        const string deviceId = "device-1";
        var signingKey = new byte[1952];
        var body = TestDeviceCertificate.Body(
            userId,
            deviceId,
            "Test device",
            deviceId,
            new byte[1184],
            signingKey,
            1_700_000_000_000,
            null);
        await using (var setupScope = provider.CreateAsyncScope())
        {
            var setup = setupScope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            await setup.GetService<IMigrator>().MigrateAsync(
                DeviceCertificateAuthorityMigrationPreflight.DeviceListCollapsePredecessor,
                TestContext.Current.CancellationToken);
            setup.Users.Add(User.Create(
                userId,
                $"{userId.Value:N}@example.test",
                "certificate-preflight-rejection",
                "Certificate preflight rejection"));
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
            await InsertLegacyDeviceAsync(
                setup,
                userId,
                deviceId,
                signingKey,
                body,
                TestDeviceCertificate.Signature());
            if (failureMode == 1)
            {
                await InsertLegacyDeviceListAsync(
                    setup,
                    userId,
                    1,
                    [(deviceId, "missing-signer")],
                    deviceId,
                    TestDeviceList.Body(
                        userId,
                        1,
                        TestDeviceList.Entries([(deviceId, "missing-signer")]),
                        deviceId,
                        1_700_000_000_001,
                        null),
                    TestDeviceCertificate.Signature(),
                    1_700_000_000_001,
                    null);
            }
            else if (failureMode == 2)
            {
                await InsertLegacyDeviceListAsync(
                    setup,
                    userId,
                    1,
                    [("different-device", "different-device")],
                    deviceId,
                    TestDeviceList.Body(
                        userId,
                        1,
                        TestDeviceList.Entries([(deviceId, deviceId)]),
                        deviceId,
                        1_700_000_000_001,
                        null),
                    TestDeviceCertificate.Signature(),
                    1_700_000_000_001,
                    null);
            }
        }

        var exception = await Assert.ThrowsAsync<InvalidOperationException>(() =>
            DatabaseInitializerHostedService.InitializeAsync(provider));
        var expectedFailure = failureMode switch
        {
            0 => "signature verification failed",
            1 => "does not preserve its certificate signer",
            _ => "exact signed semantics differ from the legacy projections",
        };
        Assert.Contains(expectedFailure, exception.Message, StringComparison.Ordinal);

        await using var checkScope = provider.CreateAsyncScope();
        var applied = await checkScope.ServiceProvider.GetRequiredService<KodosiDbContext>()
            .Database.GetAppliedMigrationsAsync(TestContext.Current.CancellationToken);
        Assert.DoesNotContain(
            DeviceCertificateAuthorityMigrationPreflight.DeviceListCollapseMigration,
            applied);
        Assert.DoesNotContain(
            DeviceCertificateAuthorityMigrationPreflight.Migration,
            applied);
    }

    [Fact]
    public async Task Certificate_Preflight_Handles_Already_Collapsed_List_Schema()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_already_collapsed_list_preflight_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var services = new ServiceCollection()
            .AddLogging()
            .AddSingleton<IPopSignatureVerifier>(new RecordingSignatureVerifier(result: true))
            .AddDbContext<KodosiDbContext>(options =>
                options.UseNpgsql(postgres.GetConnectionString()))
            .BuildServiceProvider();
        await using var provider = services;
        await using (var setupScope = provider.CreateAsyncScope())
        {
            var setup = setupScope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            await setup.GetService<IMigrator>().MigrateAsync(
                "20260820010613_WidenDeviceRevocationGeneration",
                TestContext.Current.CancellationToken);
        }

        await DatabaseInitializerHostedService.InitializeAsync(provider);

        await using var checkScope = provider.CreateAsyncScope();
        Assert.Contains(
            DeviceCertificateAuthorityMigrationPreflight.Migration,
            await checkScope.ServiceProvider.GetRequiredService<KodosiDbContext>()
                .Database.GetAppliedMigrationsAsync(TestContext.Current.CancellationToken));
    }

    private static Task<int> InsertLegacyDeviceListAsync(
        KodosiDbContext context,
        UserId userId,
        long generation,
        (string DeviceId, string SignerDeviceId)[] legacyEntries,
        string signerDeviceId,
        byte[] body,
        byte[] signature,
        long issuedAtMs,
        long? expiresAtMs)
    {
        var entriesJson = JsonSerializer.Serialize(legacyEntries.Select(entry => new
        {
            deviceId = entry.DeviceId,
            signerDeviceId = entry.SignerDeviceId,
        }));
        return context.Database.ExecuteSqlInterpolatedAsync(
            $"""
            INSERT INTO user_device_lists (
                user_id, generation, device_ids, signer_device_id, body, signature,
                issued_at_ms, expires_at_ms, created_at)
            VALUES (
                {userId.Value}, {generation}, CAST({entriesJson} AS jsonb), {signerDeviceId},
                {body}, {signature}, {issuedAtMs}, {expiresAtMs}, NOW())
            """);
    }

    private static Task<int> InsertLegacyDeviceAsync(
        KodosiDbContext context,
        UserId userId,
        string deviceId,
        byte[] signingKey,
        byte[] body,
        byte[] signature) =>
        context.Database.ExecuteSqlInterpolatedAsync(
            $"""
            INSERT INTO user_devices (
                user_id, device_id, kem_public_key, signing_public_key, created_at,
                device_label, device_certificate, device_certificate_signature,
                cert_signer_device_id, cert_issued_at, cert_expires_at)
            VALUES (
                {userId.Value}, {deviceId}, {new byte[1184]}, {signingKey}, NOW(),
                {"Test device"}, {body}, {signature}, {deviceId},
                {DateTimeOffset.FromUnixTimeMilliseconds(1_700_000_000_000)}, NULL)
            """);

    private sealed class RecordingSignatureVerifier(bool result) : IPopSignatureVerifier
    {
        public List<(byte[] PublicKey, byte[] Message, byte[] Signature)> Calls { get; } = [];

        public bool Verify(
            ReadOnlySpan<byte> publicKey,
            ReadOnlySpan<byte> message,
            ReadOnlySpan<byte> signature)
        {
            Calls.Add((publicKey.ToArray(), message.ToArray(), signature.ToArray()));
            return result;
        }
    }
}
