using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Endpoints;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Crypto;
using Kodosi.Infrastructure.Persistence;
using Kodosi.Infrastructure.Persistence.Repositories;
using Kodosi.Infrastructure.Realtime;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Infrastructure;
using Microsoft.EntityFrameworkCore.Migrations;
using Microsoft.Extensions.Logging.Abstractions;
using Npgsql;
using Testcontainers.PostgreSql;

namespace Kodosi.HostTests;

public sealed class PostgresPersistenceIntegrationTests
{
    [Fact]
    public async Task Session_Target_Adapters_Filter_Stale_Incarnations()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_session_target_resolver_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;
        await using (var setup = new KodosiDbContext(options))
        {
            await setup.Database.MigrateAsync(TestContext.Current.CancellationToken);
        }

        var userId = UserId.New();
        var currentSession = Session.Create(
            SessionId.New(),
            Guid.CreateVersion7(),
            1,
            Session.CurrentIncarnationProtocolVersion,
            userId,
            "current target",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            "secret-hash");
        await using (var context = new KodosiDbContext(options))
        {
            context.Users.Add(User.Create(
                userId,
                $"{userId.Value:N}@example.test",
                $"session-target-{userId.Value:N}",
                "Session target"));
            context.Sessions.Add(currentSession);
            await context.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var staleIncarnationId = Guid.CreateVersion7();
        var missingSessionId = SessionId.New();
        var resolver = new SessionIncarnationResolver(
            new IntegrationDbContextFactory(options));

        var deviceTargets = await ((IDeviceRevocationSessionResolver)resolver)
            .GetCurrentTargetsAsync(
                [
                    new(currentSession.Id, staleIncarnationId),
                    new(currentSession.Id, currentSession.IncarnationId),
                    new(currentSession.Id, currentSession.IncarnationId),
                    new(missingSessionId, Guid.CreateVersion7()),
                ],
                TestContext.Current.CancellationToken);
        var resetTargets = await ((IIdentityResetSessionResolver)resolver)
            .GetCurrentTargetsAsync(
                [
                    new(currentSession.Id, staleIncarnationId),
                    new(currentSession.Id, currentSession.IncarnationId),
                    new(currentSession.Id, currentSession.IncarnationId),
                    new(missingSessionId, Guid.CreateVersion7()),
                ],
                TestContext.Current.CancellationToken);

        Assert.Equal(
            [new DeviceRevocationSessionTarget(
                currentSession.Id,
                currentSession.IncarnationId)],
            deviceTargets);
        Assert.Equal(
            [new IdentityResetSessionTarget(
                currentSession.Id,
                currentSession.IncarnationId)],
            resetTargets);
    }

    [Fact]
    public async Task AccessOverrideExpiryMigration_Allows_Clean_BackfillOnly_Downgrade()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_access_expiry_clean_downgrade_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;
        var legacyAuditId = Guid.CreateVersion7();
        var legacyOccurredAt = new DateTimeOffset(2026, 8, 19, 12, 0, 0, TimeSpan.Zero);

        await using var context = new KodosiDbContext(options);
        var migrator = context.GetService<IMigrator>();
        await migrator.MigrateAsync(
            "20260820125920_CollapseUserDeviceCertificateAuthority",
            TestContext.Current.CancellationToken);
        await context.Database.ExecuteSqlInterpolatedAsync(
            $"""
            INSERT INTO access_override_audit (
                id, session_id, actor_user_id, grantee_user_id,
                action, reason, occurred_at)
            VALUES (
                {legacyAuditId}, {Guid.CreateVersion7()}, {Guid.CreateVersion7()},
                {Guid.CreateVersion7()}, {'G' + "ranted"}, {'E' + "xplicit"},
                {legacyOccurredAt})
            """,
            TestContext.Current.CancellationToken);
        await migrator.MigrateAsync(cancellationToken: TestContext.Current.CancellationToken);

        Assert.Equal(
            legacyOccurredAt,
            await context.Set<AccessOverrideAuditEntry>()
                .AsNoTracking()
                .Where(entry => entry.Id == legacyAuditId)
                .Select(entry => entry.RealtimeEnforcedAt)
                .SingleAsync(TestContext.Current.CancellationToken));

        await migrator.MigrateAsync(
            "20260820125920_CollapseUserDeviceCertificateAuthority",
            TestContext.Current.CancellationToken);

        Assert.DoesNotContain(
            "20260820153800_AddAccessOverrideExpiryEnforcement",
            await context.Database.GetAppliedMigrationsAsync(
                TestContext.Current.CancellationToken));
        Assert.Equal(
            1,
            await context.Database.SqlQueryRaw<int>(
                    "SELECT COUNT(*)::int AS \"Value\" FROM access_override_audit")
                .SingleAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task AccessOverrideExpiryMigration_Completed_Exact_Evidence_Blocks_Downgrade()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_access_expiry_evidence_downgrade_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;
        var occurredAt = new DateTimeOffset(2026, 8, 20, 12, 0, 0, TimeSpan.Zero);
        var evidenceId = Guid.CreateVersion7();

        await using var context = new KodosiDbContext(options);
        await context.Database.MigrateAsync(TestContext.Current.CancellationToken);
        await context.Database.ExecuteSqlInterpolatedAsync(
            $"""
            INSERT INTO access_override_audit (
                id, session_id, actor_user_id, grantee_user_id,
                action, reason, occurred_at, session_incarnation_id,
                session_started_at, expected_expires_at, expected_revoked_at,
                realtime_enforced_at)
            VALUES (
                {evidenceId}, {Guid.CreateVersion7()}, {Guid.CreateVersion7()},
                {Guid.CreateVersion7()}, {'R' + "evoked"}, {'E' + "xpired"},
                {occurredAt}, {Guid.CreateVersion7()}, {occurredAt.AddHours(-1)},
                {occurredAt.AddMinutes(-1)}, {occurredAt}, {occurredAt.AddSeconds(1)})
            """,
            TestContext.Current.CancellationToken);

        var exception = await Assert.ThrowsAsync<PostgresException>(() =>
            context.GetService<IMigrator>().MigrateAsync(
                "20260820125920_CollapseUserDeviceCertificateAuthority",
                TestContext.Current.CancellationToken));

        Assert.Contains("exact enforcement evidence", exception.MessageText, StringComparison.Ordinal);
        Assert.Contains(
            "20260820153800_AddAccessOverrideExpiryEnforcement",
            await context.Database.GetAppliedMigrationsAsync(
                TestContext.Current.CancellationToken));
        var retained = await context.Set<AccessOverrideAuditEntry>()
            .AsNoTracking()
            .SingleAsync(entry => entry.Id == evidenceId, TestContext.Current.CancellationToken);
        Assert.Equal(occurredAt.AddSeconds(1), retained.RealtimeEnforcedAt);
        Assert.Equal(occurredAt, retained.ExpectedRevokedAt);
    }

    [Fact]
    public async Task AccessOverrideExpiry_Work_Backfills_Fences_Regrant_And_Completes()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_access_expiry_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;
        var legacyAuditId = Guid.CreateVersion7();
        var legacyOccurredAt = new DateTimeOffset(2026, 8, 19, 12, 0, 0, TimeSpan.Zero);
        await using (var setup = new KodosiDbContext(options))
        {
            await setup.GetService<IMigrator>().MigrateAsync(
                "20260820125920_CollapseUserDeviceCertificateAuthority",
                TestContext.Current.CancellationToken);
            await setup.Database.ExecuteSqlInterpolatedAsync(
                $"""
                INSERT INTO access_override_audit (
                    id, session_id, actor_user_id, grantee_user_id,
                    action, reason, occurred_at)
                VALUES (
                    {legacyAuditId}, {Guid.CreateVersion7()}, {Guid.CreateVersion7()},
                    {Guid.CreateVersion7()}, {'G' + "ranted"}, {'E' + "xplicit"},
                    {legacyOccurredAt})
                """,
                TestContext.Current.CancellationToken);
            await setup.Database.MigrateAsync(TestContext.Current.CancellationToken);
        }

        await using (var verifyBackfill = new KodosiDbContext(options))
        {
            var backfilledAt = await verifyBackfill.Set<AccessOverrideAuditEntry>()
                .AsNoTracking()
                .Where(entry => entry.Id == legacyAuditId)
                .Select(entry => entry.RealtimeEnforcedAt)
                .SingleAsync(TestContext.Current.CancellationToken);
            Assert.Equal(legacyOccurredAt, backfilledAt);
        }

        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var owner = User.Create(
            ownerId,
            $"{ownerId.Value:N}@example.test",
            $"expiry-owner-{ownerId.Value:N}",
            "Expiry owner");
        var viewer = User.Create(
            viewerId,
            $"{viewerId.Value:N}@example.test",
            $"expiry-viewer-{viewerId.Value:N}",
            "Expiry viewer");
        var session = Session.Create(
            sessionId,
            incarnationId,
            1,
            Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "access expiry",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret-hash");
        var revokedAt = new DateTimeOffset(2026, 8, 20, 12, 0, 0, TimeSpan.Zero);
        var expiresAt = revokedAt.AddMinutes(-1);
        var accessOverride = SessionAccessOverride.Create(
            sessionId,
            viewerId,
            AccessLevel.Suggest,
            ownerId,
            expiresAt,
            expiresAt.AddMinutes(-1));
        accessOverride.Revoke(revokedAt);
        var audit = AccessOverrideAuditEntry.CreateExpiredRevocation(
            sessionId,
            ownerId,
            viewerId,
            incarnationId,
            session.StartedAt,
            expiresAt,
            revokedAt);
        await using (var context = new KodosiDbContext(options))
        {
            context.Users.AddRange(owner, viewer);
            context.Sessions.Add(session);
            context.SessionAccessOverrides.Add(accessOverride);
            context.Set<AccessOverrideAuditEntry>().Add(audit);
            await context.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var coordinator = new AccessOverrideExpiryDurabilityCoordinator(
            new IntegrationDbContextFactory(options),
            TimeProvider.System);
        var work = Assert.Single(await coordinator.GetPendingEnforcementAsync(
            10,
            TestContext.Current.CancellationToken));
        Assert.Equal(audit.Id, work.AuditEntryId);
        Assert.Equal(incarnationId, work.SessionIncarnationId);
        Assert.Equal(session.StartedAt, work.SessionStartedAt);
        Assert.Equal(expiresAt, work.ExpectedExpiresAt);
        Assert.Equal(revokedAt, work.ExpectedRevokedAt);

        var enforced = false;
        Assert.True(await coordinator.ExecuteIfCurrentAsync(
            work,
            _ =>
            {
                enforced = true;
                return Task.CompletedTask;
            },
            TestContext.Current.CancellationToken));
        Assert.True(enforced);

        await using (var regrant = new KodosiDbContext(options))
        {
            var current = await regrant.SessionAccessOverrides.SingleAsync(
                row => row.SessionId == sessionId && row.ActorUserId == viewerId,
                TestContext.Current.CancellationToken);
            var grantedAt = revokedAt.AddMinutes(1);
            current.Grant(
                AccessLevel.Approve,
                ownerId,
                grantedAt.AddMinutes(5),
                grantedAt);
            await regrant.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        enforced = false;
        Assert.False(await coordinator.ExecuteIfCurrentAsync(
            work,
            _ =>
            {
                enforced = true;
                return Task.CompletedTask;
            },
            TestContext.Current.CancellationToken));
        Assert.False(enforced);

        await coordinator.CompleteEnforcementAsync(
            work.AuditEntryId,
            TestContext.Current.CancellationToken);
        await coordinator.CompleteEnforcementAsync(
            work.AuditEntryId,
            TestContext.Current.CancellationToken);
        Assert.Empty(await coordinator.GetPendingEnforcementAsync(
            10,
            TestContext.Current.CancellationToken));

        var pending = AccessOverrideAuditEntry.CreateExpiredRevocation(
            sessionId,
            ownerId,
            viewerId,
            incarnationId,
            session.StartedAt,
            expiresAt,
            revokedAt);
        await using (var context = new KodosiDbContext(options))
        {
            context.Set<AccessOverrideAuditEntry>().Add(pending);
            await context.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        await using var downgrade = new KodosiDbContext(options);
        var downgradeException = await Assert.ThrowsAsync<PostgresException>(() =>
            downgrade.GetService<IMigrator>().MigrateAsync(
                "20260820125920_CollapseUserDeviceCertificateAuthority",
                TestContext.Current.CancellationToken));
        Assert.Contains(
            "access override expiry enforcement",
            downgradeException.MessageText,
            StringComparison.OrdinalIgnoreCase);
        Assert.Contains(
            "20260820153800_AddAccessOverrideExpiryEnforcement",
            await downgrade.Database.GetAppliedMigrationsAsync(
                TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task IdentityReset_Work_Persists_Exact_Session_Targets_And_Fails_Closed()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_reset_target_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;
        await using (var setup = new KodosiDbContext(options))
        {
            await setup.Database.MigrateAsync(TestContext.Current.CancellationToken);
        }

        var userId = UserId.New();
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var resetId = Guid.CreateVersion7();
        var user = User.Create(
            userId,
            $"{userId.Value:N}@example.test",
            $"reset-target-{userId.Value:N}",
            "Reset target");
        Assert.Equal(1, user.AdvanceIdentityLifecycle(incarnationId: null));
        var session = Session.Create(
            sessionId,
            incarnationId,
            1,
            Session.CurrentIncarnationProtocolVersion,
            userId,
            "reset target",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            "secret-hash");
        session.End();
        var audit = IdentityResetAuditEntry.Create(
            resetId,
            userId,
            1,
            1,
            0,
            0,
            null,
            null,
            [],
            [sessionId],
            [
                new IdentityResetEndedSessionTarget(
                    sessionId,
                    incarnationId,
                    userId,
                    SessionScope.JustMe,
                    null,
                    session.StartedAt),
            ],
            [sessionId],
            [new IdentityResetSessionTarget(sessionId, incarnationId)],
            [],
            1);
        await using (var context = new KodosiDbContext(options))
        {
            context.Users.Add(user);
            context.Sessions.Add(session);
            context.IdentityResetAuditEntries.Add(audit);
            await context.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var coordinator = new IdentityResetDurabilityCoordinator(
            new IntegrationDbContextFactory(options));
        var work = Assert.Single(await coordinator.GetPendingEnforcementAsync(
            10,
            TestContext.Current.CancellationToken));
        Assert.Equal(
            [new IdentityResetSessionTarget(sessionId, incarnationId)],
            work.SessionTargets);
        var ended = Assert.Single(work.EndedSessions);
        Assert.Equal(incarnationId, ended.Target.IncarnationId);
        Assert.Equal(session.StartedAt, ended.StartedAt);

        var malformedResetId = Guid.CreateVersion7();
        var malformed = IdentityResetAuditEntry.Create(
            malformedResetId,
            userId,
            0,
            0,
            0,
            0,
            null,
            null,
            [],
            [],
            [],
            [],
            [],
            [],
            1);
        DomainFixtureHydrator.SetIdentityResetTargetEvidence(
            malformed,
            "null",
            "[]",
            [],
            [sessionId.Value]);
        await using (var context = new KodosiDbContext(options))
        {
            context.IdentityResetAuditEntries.Add(malformed);
            await context.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        await Assert.ThrowsAsync<DomainException>(() =>
            coordinator.GetPendingEnforcementAsync(
                10,
                TestContext.Current.CancellationToken));
        await using (var context = new KodosiDbContext(options))
        {
            Assert.True(await context.IdentityResetAuditEntries
                .AsNoTracking()
                .Where(entry => entry.Id == malformedResetId)
                .Select(entry => entry.RealtimeEnforcedAt == null)
                .SingleAsync(TestContext.Current.CancellationToken));
        }

        await using var downgradeContext = new KodosiDbContext(options);
        var downgradeException = await Assert.ThrowsAsync<PostgresException>(() =>
            downgradeContext.GetService<IMigrator>().MigrateAsync(
                "20260818034840_AddDeviceRevocationSessionTargets",
                TestContext.Current.CancellationToken));
        Assert.Contains(
            "identity reset session targets",
            downgradeException.MessageText,
            StringComparison.OrdinalIgnoreCase);
        Assert.Contains(
            "20260818074703_AddIdentityResetSessionTargets",
            await downgradeContext.Database.GetAppliedMigrationsAsync(
                TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task CertificateAuthorityCollapse_Validates_Exact_Semantics_And_Refuses_Fabricated_Downgrade()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_certificate_authority_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;
        var userId = UserId.New();
        var deviceId = $"device-{Guid.NewGuid():N}";
        var certificate = TestDeviceCertificate.Body(
            userId,
            deviceId,
            "Certificate authority device",
            deviceId,
            new byte[1184],
            new byte[1952],
            1_700_000_000_000,
            null);
        var signature = TestDeviceCertificate.Signature(7);
        await using (var setup = new KodosiDbContext(options))
        {
            await setup.GetService<IMigrator>().MigrateAsync(
                "20260820010613_WidenDeviceRevocationGeneration",
                TestContext.Current.CancellationToken);
            setup.Users.Add(User.Create(
                userId,
                $"{userId.Value:N}@example.test",
                $"certificate-authority-{userId.Value:N}",
                "Certificate authority"));
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
            await setup.Database.ExecuteSqlInterpolatedAsync(
                $"""
                INSERT INTO user_devices (
                    user_id, device_id, kem_public_key, signing_public_key, created_at,
                    device_label, device_certificate, device_certificate_signature,
                    cert_signer_device_id, cert_issued_at, cert_expires_at)
                VALUES (
                    {userId.Value}, {deviceId}, {new byte[1184]}, {new byte[1952]}, NOW(),
                    {'X' + "Certificate authority device"}, {certificate}, {signature},
                    {deviceId}, {DateTimeOffset.FromUnixTimeMilliseconds(1_700_000_000_000)}, NULL)
                """,
                TestContext.Current.CancellationToken);
        }

        await using (var rejected = new KodosiDbContext(options))
        {
            var exception = await Assert.ThrowsAsync<PostgresException>(() =>
                rejected.Database.MigrateAsync(TestContext.Current.CancellationToken));
            Assert.True(
                exception.MessageText.Contains(
                    "exact certificate semantics differ from the legacy projections",
                    StringComparison.Ordinal),
                exception.ToString());
            Assert.DoesNotContain(
                "20260820125920_CollapseUserDeviceCertificateAuthority",
                await rejected.Database.GetAppliedMigrationsAsync(
                    TestContext.Current.CancellationToken));
            await rejected.Database.ExecuteSqlInterpolatedAsync(
                $"UPDATE user_devices SET device_label = {'C' + "ertificate authority device"} WHERE user_id = {userId.Value} AND device_id = {deviceId}",
                TestContext.Current.CancellationToken);
            await rejected.Database.MigrateAsync(TestContext.Current.CancellationToken);
        }

        await using (var reloaded = new KodosiDbContext(options))
        {
            var device = await reloaded.UserDevices
                .AsNoTracking()
                .SingleAsync(
                    row => row.UserId == userId && row.DeviceId == deviceId,
                    TestContext.Current.CancellationToken);
            Assert.Equal(certificate, device.DeviceCertificate);
            Assert.Equal(signature, device.DeviceCertificateSignature);
            Assert.Equal("Certificate authority device", device.DeviceLabel);
            Assert.Equal(DateTimeOffset.FromUnixTimeMilliseconds(1_700_000_000_000), device.CertIssuedAt);
        }

        await using var downgrade = new KodosiDbContext(options);
        var downgradeException = await Assert.ThrowsAsync<PostgresException>(() =>
            downgrade.GetService<IMigrator>().MigrateAsync(
                "20260820010613_WidenDeviceRevocationGeneration",
                TestContext.Current.CancellationToken));

        Assert.Contains(
            "without fabricating signed semantics",
            downgradeException.MessageText,
            StringComparison.Ordinal);
        Assert.Contains(
            "20260820125920_CollapseUserDeviceCertificateAuthority",
            await downgrade.Database.GetAppliedMigrationsAsync(
                TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task Committed_Device_Revocation_Gates_Participant_Dispatch()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_revocation_dispatch_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;
        await using (var setup = new KodosiDbContext(options))
        {
            await setup.Database.MigrateAsync(TestContext.Current.CancellationToken);
        }

        await AssertCommittedDeviceRevocationGatesParticipantDispatchAsync(options);
    }

    [Fact]
    public async Task DeviceRevocation_Work_Persists_Exact_Session_Target_Tuple()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_revocation_target_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;
        await using (var setup = new KodosiDbContext(options))
        {
            await setup.Database.MigrateAsync(TestContext.Current.CancellationToken);
        }

        var userId = UserId.New();
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var revocationId = Guid.CreateVersion7();
        var user = User.Create(
            userId,
            $"{userId.Value:N}@example.test",
            $"revocation-{userId.Value:N}",
            "Revocation User");
        user.AdvanceIdentityLifecycle(Guid.CreateVersion7());
        var session = Session.Create(
            sessionId,
            incarnationId,
            1,
            Session.CurrentIncarnationProtocolVersion,
            userId,
            "revocation tuple",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            "secret-hash");
        var signer = CreateCertifiedDevice(userId, "tuple-signer");
        var revoked = CreateCertifiedDevice(userId, "tuple-revoked");
        revoked.Revoke(signer.DeviceId);
        var deviceList = TestDeviceList.Create(
            userId,
            2,
            TestDeviceList.Entries([(signer.DeviceId, signer.DeviceId)]),
            signer.DeviceId,
            [2],
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
            null);
        var target = new DeviceRevocationSessionTarget(sessionId, incarnationId);
        var audit = DeviceRevocationAuditEntry.Create(
            revocationId,
            userId,
            revoked.DeviceId,
            signer.DeviceId,
            2,
            user.IdentityRevision,
            1,
            [target],
            null,
            null);

        await using (var context = new KodosiDbContext(options))
        {
            context.Users.Add(user);
            context.Sessions.Add(session);
            context.UserDevices.AddRange(signer, revoked);
            context.UserDeviceLists.Add(deviceList);
            context.Set<DeviceRevocationAuditEntry>().Add(audit);
            await context.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var coordinator = new DeviceRevocationDurabilityCoordinator(
            new IntegrationDbContextFactory(options));
        var work = Assert.Single(await coordinator.GetPendingEnforcementAsync(
            10,
            TestContext.Current.CancellationToken));
        Assert.Equal([target], work.AffectedSessionTargets);

        var malformedRevocationId = Guid.CreateVersion7();
        var malformed = DeviceRevocationAuditEntry.Create(
            malformedRevocationId,
            userId,
            "malformed-revoked",
            signer.DeviceId,
            2,
            user.IdentityRevision,
            0,
            [target],
            null,
            null);
        DomainFixtureHydrator.SetDeviceRevocationTargetEvidence(
            malformed,
            "null",
            [sessionId.Value]);
        await using (var context = new KodosiDbContext(options))
        {
            context.Set<DeviceRevocationAuditEntry>().Add(malformed);
            await context.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await Assert.ThrowsAsync<DomainException>(() =>
            coordinator.GetPendingEnforcementAsync(
                10,
                TestContext.Current.CancellationToken));
        await using (var context = new KodosiDbContext(options))
        {
            var malformedStillPending = await context.Set<DeviceRevocationAuditEntry>()
                .AsNoTracking()
                .Where(entry => entry.RevocationId == malformedRevocationId)
                .Select(entry => entry.RealtimeEnforcedAt == null)
                .SingleAsync(TestContext.Current.CancellationToken);
            Assert.True(malformedStillPending);
        }

        await using var downgradeContext = new KodosiDbContext(options);
        await downgradeContext.UserDeviceLists.ExecuteDeleteAsync(
            TestContext.Current.CancellationToken);
        await downgradeContext.UserDevices.ExecuteDeleteAsync(
            TestContext.Current.CancellationToken);
        var downgradeException = await Assert.ThrowsAsync<PostgresException>(() =>
            downgradeContext.GetService<IMigrator>().MigrateAsync(
                "20260817112447_DropUnusedPersistenceIndexes",
                TestContext.Current.CancellationToken));
        Assert.Contains(
            "exact target audit evidence",
            downgradeException.MessageText,
            StringComparison.Ordinal);
        Assert.Contains(
            "20260818034840_AddDeviceRevocationSessionTargets",
            await downgradeContext.Database.GetAppliedMigrationsAsync(
                TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task SemanticRelayMailbox_Enforces_Exact_Concurrent_Idempotency_And_DeviceScoped_Acknowledgement()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_semantic_mailbox_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;
        await using (var setup = new KodosiDbContext(options))
        {
            await setup.Database.MigrateAsync(TestContext.Current.CancellationToken);
        }
        var factory = new IntegrationDbContextFactory(options);
        var clock = new TestTimeProvider(
            new DateTimeOffset(2026, 8, 16, 12, 0, 0, TimeSpan.Zero));
        var repository = new SemanticRelayRepository(factory, clock);
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var requesterUserId = UserId.New();
        var requestId = Guid.CreateVersion7();
        const string requesterDeviceId = "requester-device";
        const string mode = "steer";
        var fingerprint = new string('a', 64);

        var claims = await Task.WhenAll(
            repository.ClaimRequestAsync(
                sessionId,
                incarnationId,
                requesterUserId,
                requesterDeviceId,
                requestId,
                mode,
                fingerprint,
                TestContext.Current.CancellationToken),
            repository.ClaimRequestAsync(
                sessionId,
                incarnationId,
                requesterUserId,
                requesterDeviceId,
                requestId,
                mode,
                fingerprint,
                TestContext.Current.CancellationToken));
        Assert.Single(claims, claim => claim.Kind == SemanticRequestClaimKind.Created);
        Assert.Single(claims, claim => claim.Kind == SemanticRequestClaimKind.ExactDuplicate);
        Assert.Equal(claims[0].Request.Id, claims[1].Request.Id);

        var conflict = await repository.ClaimRequestAsync(
            sessionId,
            incarnationId,
            requesterUserId,
            requesterDeviceId,
            requestId,
            "queue",
            fingerprint,
            TestContext.Current.CancellationToken);
        Assert.Equal(SemanticRequestClaimKind.Conflict, conflict.Kind);

        await repository.MarkDispatchedAsync(
            claims[0].Request.Id,
            TestContext.Current.CancellationToken);
        var ownerUserId = requesterUserId;
        Assert.True(await repository.StoreReceiptAsync(
            sessionId,
            incarnationId,
            requesterUserId,
            requesterDeviceId,
            requestId,
            mode,
            fingerprint,
            "injected",
            ownerUserId,
            "owner-device",
            "owner-signature",
            TestContext.Current.CancellationToken));
        Assert.True(await repository.StoreReceiptAsync(
            sessionId,
            incarnationId,
            requesterUserId,
            requesterDeviceId,
            requestId,
            mode,
            fingerprint,
            "injected",
            ownerUserId,
            "owner-device",
            "owner-signature",
            TestContext.Current.CancellationToken));
        Assert.False(await repository.StoreReceiptAsync(
            sessionId,
            incarnationId,
            requesterUserId,
            requesterDeviceId,
            requestId,
            mode,
            fingerprint,
            "cancelled",
            ownerUserId,
            "owner-device",
            "owner-signature",
            TestContext.Current.CancellationToken));

        var pending = await repository.ListPendingReceiptsAsync(
            sessionId,
            incarnationId,
            requesterUserId,
            requesterDeviceId,
            32,
            TestContext.Current.CancellationToken);
        var receipt = Assert.Single(pending);
        Assert.Null(receipt.AcknowledgedAt);
        Assert.Empty(await repository.ListPendingReceiptsAsync(
            sessionId,
            incarnationId,
            requesterUserId,
            "other-device",
            32,
            TestContext.Current.CancellationToken));
        Assert.Empty(await repository.ListPendingReceiptsAsync(
            sessionId,
            Guid.CreateVersion7(),
            requesterUserId,
            requesterDeviceId,
            32,
            TestContext.Current.CancellationToken));

        Assert.True(await repository.AcknowledgeReceiptAsync(
            sessionId,
            incarnationId,
            requesterUserId,
            requesterDeviceId,
            requestId,
            TestContext.Current.CancellationToken));
        Assert.True(await repository.AcknowledgeReceiptAsync(
            sessionId,
            incarnationId,
            requesterUserId,
            requesterDeviceId,
            requestId,
            TestContext.Current.CancellationToken));
        Assert.False(await repository.AcknowledgeReceiptAsync(
            sessionId,
            incarnationId,
            requesterUserId,
            "other-device",
            requestId,
            TestContext.Current.CancellationToken));
        Assert.Empty(await repository.ListPendingReceiptsAsync(
            sessionId,
            incarnationId,
            requesterUserId,
            requesterDeviceId,
            32,
            TestContext.Current.CancellationToken));

        await using var verify = new KodosiDbContext(options);
        var storedRequest = await verify.SemanticRelayRequests.SingleAsync(
            value => value.RequesterUserId == requesterUserId
                && value.RequestId == requestId,
            TestContext.Current.CancellationToken);
        var storedReceipt = await verify.SemanticRelayReceipts.SingleAsync(
            value => value.RequestRowId == storedRequest.Id,
            TestContext.Current.CancellationToken);
        Assert.Equal(SemanticRequestState.Acknowledged, storedRequest.State);
        Assert.NotNull(storedReceipt.AcknowledgedAt);

        await repository.MarkDispatchedAsync(
            storedRequest.Id,
            TestContext.Current.CancellationToken);
        await verify.Entry(storedRequest).ReloadAsync(TestContext.Current.CancellationToken);
        Assert.Equal(SemanticRequestState.Acknowledged, storedRequest.State);

        Assert.Equal(
            0,
            await repository.DeleteAcknowledgedBeforeAsync(
                clock.GetUtcNow().AddMilliseconds(-1),
                100,
                TestContext.Current.CancellationToken));
        Assert.Equal(
            1,
            await repository.DeleteAcknowledgedBeforeAsync(
                clock.GetUtcNow(),
                100,
                TestContext.Current.CancellationToken));
        await using var compacted = new KodosiDbContext(options);
        Assert.False(await compacted.SemanticRelayRequests.AnyAsync(
            value => value.Id == storedRequest.Id,
            TestContext.Current.CancellationToken));
        Assert.False(await compacted.SemanticRelayReceipts.AnyAsync(
            value => value.RequestRowId == storedRequest.Id,
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task SemanticRelayDispatch_CannotOverwriteReceiptStored()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_semantic_dispatch_race_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;
        await using (var setup = new KodosiDbContext(options))
        {
            await setup.Database.MigrateAsync(TestContext.Current.CancellationToken);
        }
        var factory = new IntegrationDbContextFactory(options);
        var repository = new SemanticRelayRepository(factory, TimeProvider.System);
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var requesterUserId = UserId.New();
        var requestId = Guid.CreateVersion7();
        var claim = await repository.ClaimRequestAsync(
            sessionId,
            incarnationId,
            requesterUserId,
            "requester-device",
            requestId,
            "steer",
            new string('a', 64),
            TestContext.Current.CancellationToken);

        Assert.True(await repository.StoreReceiptAsync(
            sessionId,
            incarnationId,
            requesterUserId,
            "requester-device",
            requestId,
            "steer",
            new string('a', 64),
            "injected",
            requesterUserId,
            "owner-device",
            "owner-signature",
            TestContext.Current.CancellationToken));
        await repository.MarkDispatchedAsync(
            claim.Request.Id,
            TestContext.Current.CancellationToken);

        await using var verify = new KodosiDbContext(options);
        var request = await verify.SemanticRelayRequests.SingleAsync(
            value => value.Id == claim.Request.Id,
            TestContext.Current.CancellationToken);
        Assert.Equal(SemanticRequestState.ReceiptStored, request.State);
    }

    [Fact]
    public async Task DeviceRegistrationChallengeCleanupDeletesOnlyExpiredRowsWithinLimit()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_challenge_cleanup_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;
        var userId = UserId.New();
        var first = DeviceRegistrationChallenge.Create(userId, new byte[32], TimeSpan.FromMinutes(1));
        var second = DeviceRegistrationChallenge.Create(userId, new byte[32], TimeSpan.FromMinutes(1));
        var active = DeviceRegistrationChallenge.Create(userId, new byte[32], TimeSpan.FromDays(1));

        await using (var setup = new KodosiDbContext(options))
        {
            await setup.Database.MigrateAsync(TestContext.Current.CancellationToken);
            setup.Users.Add(User.Create(
                userId,
                $"{userId.Value:N}@example.test",
                $"challenge-{userId.Value:N}",
                "Challenge cleanup"));
            setup.DeviceRegistrationChallenges.AddRange(first, second, active);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using var context = new KodosiDbContext(options);
        var repository = new DeviceRegistrationChallengeRepository(context);
        var removed = await repository.DeleteExpiredAsync(
            DateTimeOffset.UtcNow.AddMinutes(2),
            limit: 1,
            TestContext.Current.CancellationToken);

        Assert.Equal(1, removed);
        Assert.Equal(2, await context.DeviceRegistrationChallenges.CountAsync(
            TestContext.Current.CancellationToken));
        Assert.True(await context.DeviceRegistrationChallenges.AnyAsync(
            challenge => challenge.Id == active.Id,
            TestContext.Current.CancellationToken));

        await using var firstConsumeContext = new KodosiDbContext(options);
        await using var secondConsumeContext = new KodosiDbContext(options);
        var firstRepository = new DeviceRegistrationChallengeRepository(firstConsumeContext);
        var secondRepository = new DeviceRegistrationChallengeRepository(secondConsumeContext);
        var firstCandidate = await firstRepository.GetByIdAsync(
            active.Id,
            TestContext.Current.CancellationToken);
        var secondCandidate = await secondRepository.GetByIdAsync(
            active.Id,
            TestContext.Current.CancellationToken);
        Assert.NotNull(firstCandidate);
        Assert.NotNull(secondCandidate);

        var consumed = await Task.WhenAll(
            firstRepository.TryConsumeAsync(
                firstCandidate,
                TestContext.Current.CancellationToken),
            secondRepository.TryConsumeAsync(
                secondCandidate,
                TestContext.Current.CancellationToken));

        Assert.Single(consumed, value => value);
        await using var verify = new KodosiDbContext(options);
        Assert.False(await verify.DeviceRegistrationChallenges.AnyAsync(
            challenge => challenge.Id == active.Id,
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task ActiveOverrideAudienceQueryIsBidirectionalAndExcludesExpiredOrRevokedGrants()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_override_audience_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;
        var owner = UserId.New();
        var actor = UserId.New();
        var expiredActor = UserId.New();
        var revokedActor = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            owner,
            "override-audience",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        var now = DateTimeOffset.UtcNow;
        var active = SessionAccessOverride.Create(
            session.Id,
            actor,
            AccessLevel.View,
            owner,
            now.AddHours(1),
            now);
        var expired = SessionAccessOverride.Create(
            session.Id,
            expiredActor,
            AccessLevel.View,
            owner,
            now.AddMinutes(-1),
            now.AddHours(-1));
        var revoked = SessionAccessOverride.Create(
            session.Id,
            revokedActor,
            AccessLevel.View,
            owner,
            now.AddHours(1),
            now);
        revoked.Revoke(now);

        await using (var setup = new KodosiDbContext(options))
        {
            await setup.Database.MigrateAsync(TestContext.Current.CancellationToken);
            foreach (var userId in new[] { owner, actor, expiredActor, revokedActor })
            {
                setup.Users.Add(User.Create(
                    userId,
                    $"{userId.Value:N}@example.test",
                    $"override-{userId.Value:N}",
                    "Override audience"));
            }
            setup.Sessions.Add(session);
            setup.SessionAccessOverrides.AddRange(active, expired, revoked);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using var query = new KodosiDbContext(options);
        var repository = new AccessOverrideRepository(query, TimeProvider.System);

        Assert.Equal(
            [actor],
            await repository.GetActiveRelatedUserIdsAsync(
                owner,
                TestContext.Current.CancellationToken));
        Assert.Equal(
            [owner],
            await repository.GetActiveRelatedUserIdsAsync(
                actor,
                TestContext.Current.CancellationToken));
        Assert.Empty(await repository.GetActiveRelatedUserIdsAsync(
            expiredActor,
            TestContext.Current.CancellationToken));
        Assert.Empty(await repository.GetActiveRelatedUserIdsAsync(
            revokedActor,
            TestContext.Current.CancellationToken));
        Assert.True(await repository.HasActiveRelationshipAsync(
            owner,
            actor,
            TestContext.Current.CancellationToken));

        var persistedSession = await query.Sessions.SingleAsync(
            value => value.Id == session.Id,
            TestContext.Current.CancellationToken);
        persistedSession.End();
        await query.SaveChangesAsync(TestContext.Current.CancellationToken);

        Assert.Empty(await repository.GetActiveRelatedUserIdsAsync(
            owner,
            TestContext.Current.CancellationToken));
        Assert.False(await repository.HasActiveRelationshipAsync(
            owner,
            actor,
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task IdentityResetAudienceMigrationAndRecipientQueryWorkOnPostgres()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_identity_reset_audience_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;
        var resetUser = UserId.New();
        var recipient = UserId.New();
        var unrelated = UserId.New();

        await using (var setup = new KodosiDbContext(options))
        {
            await setup.Database.MigrateAsync(TestContext.Current.CancellationToken);
            var resetOwner = User.Create(
                resetUser,
                $"{resetUser.Value:N}@example.test",
                $"reset-{resetUser.Value:N}",
                "Reset audience");
            Assert.Equal(1, resetOwner.AdvanceIdentityLifecycle(incarnationId: null));
            setup.Users.Add(resetOwner);
            var repository = new IdentityResetAuditRepository(setup);
            await repository.AddAsync(
                IdentityResetAuditEntry.Create(
                    Guid.CreateVersion7(),
                    resetUser,
                    sessionsAttempted: 0,
                    sessionsEnded: 0,
                    devicesRemoved: 0,
                    deviceListsRemoved: 0,
                    clientIp: null,
                    userAgent: null,
                    removedDeviceIds: [],
                    endedSessionIds: [],
                    endedSessionTargets: [],
                    sessionsWithRevokedKeys: [],
                    sessionTargets: [],
                    audienceUserIds: [recipient],
                    identityRevision: 1),
                TestContext.Current.CancellationToken);
            await new FriendshipAuditRepository(setup).AddAsync(
                FriendshipAuditEntry.Create(
                    resetUser,
                    recipient,
                    FriendshipAuditAction.RequestAccepted,
                    clientIp: null,
                    userAgent: null),
                TestContext.Current.CancellationToken);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using var query = new KodosiDbContext(options);
        var exposureRepository = new IdentityExposureRepository(query);
        var historicalPeers = await exposureRepository.GetHistoricalPeerUserIdsAsync(
            resetUser,
            TestContext.Current.CancellationToken);
        var withdrawnSnapshot = await exposureRepository.GetLifecycleSnapshotForRecipientAsync(
            recipient,
            TestContext.Current.CancellationToken);

        Assert.Equal([recipient], historicalPeers);
        var withdrawn = Assert.Single(withdrawnSnapshot);
        Assert.Equal(resetUser, withdrawn.UserId);
        Assert.Equal(1, withdrawn.IdentityRevision);
        Assert.Null(withdrawn.IdentityIncarnationId);
        Assert.Equal(0, withdrawn.Generation);

        var incarnationId = Guid.CreateVersion7();
        await using (var reenroll = new KodosiDbContext(options))
        {
            var owner = await reenroll.Users.SingleAsync(
                user => user.Id == resetUser,
                TestContext.Current.CancellationToken);
            Assert.Equal(2, owner.AdvanceIdentityLifecycle(incarnationId));
            reenroll.UserDeviceLists.Add(TestDeviceList.Create(
                resetUser,
                1,
                "[]",
                "new-device",
                [2],
                DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                null));
            await reenroll.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        await using var supersededQuery = new KodosiDbContext(options);
        var enrolledSnapshot = await new IdentityExposureRepository(supersededQuery)
            .GetLifecycleSnapshotForRecipientAsync(
                recipient,
                TestContext.Current.CancellationToken);
        var enrolled = Assert.Single(enrolledSnapshot);
        Assert.Equal(2, enrolled.IdentityRevision);
        Assert.Equal(incarnationId, enrolled.IdentityIncarnationId);
        Assert.Equal(1, enrolled.Generation);

        var columnType = query.Model
            .FindEntityType(typeof(IdentityResetAuditEntry))!
            .FindProperty(nameof(IdentityResetAuditEntry.AudienceUserIds))!
            .GetColumnType();
        Assert.Equal("uuid[]", columnType);
    }

    [Fact]
    public async Task IdentityLifecycleMigration_Backfills_UuidV7_On_Postgres16()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_identity_lifecycle_migration_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;
        var userId = UserId.New();

        await using var context = new KodosiDbContext(options);
        var migrator = context.GetService<IMigrator>();
        await migrator.MigrateAsync(
            "20260812150418_AddIdentityResetAudience",
            TestContext.Current.CancellationToken);
        await context.Database.ExecuteSqlInterpolatedAsync(
            $"""
            INSERT INTO users (
                id, auth_subject, email, handle, display_name, created_at
            ) VALUES (
                {userId.Value}, 'migration-user', 'migration@example.test',
                'migration-user', 'Migration User', now()
            );

            INSERT INTO user_device_lists (
                user_id, generation, device_ids, signer_device_id,
                body, signature, issued_at_ms, expires_at_ms, created_at
            ) VALUES (
                {userId.Value}, 1, '[]'::jsonb, 'migration-device',
                decode('01', 'hex'), decode(repeat('02', 3309), 'hex'), 1, NULL, now()
            );
            """,
            TestContext.Current.CancellationToken);

        await migrator.MigrateAsync(
            cancellationToken: TestContext.Current.CancellationToken);
        var projection = await context.Database.SqlQueryRaw<IdentityMigrationProjection>(
                """
                SELECT
                    identity_revision AS "IdentityRevision",
                    identity_incarnation_id::text AS "IdentityIncarnationId"
                FROM users
                WHERE id = {0}
                """,
                userId.Value)
            .SingleAsync(TestContext.Current.CancellationToken);

        Assert.Equal(1, projection.IdentityRevision);
        Assert.NotNull(projection.IdentityIncarnationId);
        Assert.Equal('7', projection.IdentityIncarnationId![14]);
    }

    private sealed record IdentityMigrationProjection(
        long IdentityRevision,
        string? IdentityIncarnationId);

    [Fact]
    public async Task RecipientLifecycleLock_Uses_Transaction_Connection_With_SingleConnectionPool()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_recipient_lock_pool_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var connectionString = new NpgsqlConnectionStringBuilder(
            postgres.GetConnectionString())
        {
            MaxPoolSize = 1,
            Timeout = 2,
        }.ConnectionString;
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(connectionString)
            .Options;

        await using var context = new KodosiDbContext(options);
        await context.Database.MigrateAsync(TestContext.Current.CancellationToken);
        await using var transaction = await new UnitOfWork(context)
            .BeginTransactionAsync(TestContext.Current.CancellationToken);
        await new PostgresRecipientDeviceLifecycleLock(context)
            .AcquireAsync(
                [UserId.New(), UserId.New()],
                TestContext.Current.CancellationToken);

        Assert.Equal(
            1,
            await context.Database.SqlQueryRaw<int>("SELECT 1 AS \"Value\"")
                .SingleAsync(TestContext.Current.CancellationToken));
        await transaction.CommitAsync(TestContext.Current.CancellationToken);
    }

    [Fact]
    public async Task SessionCreate_Idempotency_History_Prevents_Incarnation_ABA_OnPostgres()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_incarnation_history_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;
        var ownerId = UserId.New();
        var sessionId = SessionId.New();
        var firstKey = Guid.CreateVersion7();
        var secondKey = Guid.CreateVersion7();
        var firstRequest = new CreateSessionRequest(
            sessionId.Value,
            firstKey,
            "first incarnation",
            SessionScope.JustMe,
            ToolKind.Terminal,
            DefaultAudienceAccess.View,
            "first-owner-secret",
            null);

        await using (var setup = new KodosiDbContext(options))
        {
            await setup.Database.MigrateAsync(TestContext.Current.CancellationToken);
            setup.Users.Add(User.Create(
                ownerId,
                $"{ownerId.Value:N}@example.test",
                $"inc-{ownerId.Value:N}",
                "Incarnation Owner"));
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        SessionDetailResponse first;
        await using (var create = new KodosiDbContext(options))
        {
            first = (await CreateSessionCreator(
                    create,
                    new GateOnlySessionEndAuthority(new SessionLifecycleGate()))
                .CreateOwnedAsync(
                    ownerId,
                    firstRequest,
                    TestContext.Current.CancellationToken)).Response;
        }
        await using (var retry = new KodosiDbContext(options))
        {
            var retried = (await CreateSessionCreator(
                    retry,
                    new GateOnlySessionEndAuthority(new SessionLifecycleGate()))
                .CreateOwnedAsync(
                    ownerId,
                    firstRequest,
                    TestContext.Current.CancellationToken)).Response;
            Assert.Equal(first.IncarnationId, retried.IncarnationId);
            Assert.Equal(1, retried.IncarnationGeneration);
        }

        await EndPersistedSessionAsync(options, sessionId);
        SessionDetailResponse second;
        await using (var republish = new KodosiDbContext(options))
        {
            second = (await CreateSessionCreator(
                    republish,
                    new GateOnlySessionEndAuthority(new SessionLifecycleGate()))
                .CreateOwnedAsync(
                    ownerId,
                    firstRequest with
                    {
                        IdempotencyKey = secondKey,
                        Title = "second incarnation",
                        OwnerSecret = "second-owner-secret",
                    },
                    TestContext.Current.CancellationToken)).Response;
        }
        Assert.NotEqual(first.IncarnationId, second.IncarnationId);
        Assert.Equal(2, second.IncarnationGeneration);

        await EndPersistedSessionAsync(options, sessionId);
        await using (var staleReplay = new KodosiDbContext(options))
        {
            await Assert.ThrowsAsync<ConflictException>(() =>
                CreateSessionCreator(
                        staleReplay,
                        new GateOnlySessionEndAuthority(new SessionLifecycleGate()))
                    .CreateOwnedAsync(
                        ownerId,
                        firstRequest,
                        TestContext.Current.CancellationToken));
        }

        await using var verify = new KodosiDbContext(options);
        var history = await verify.Database
            .SqlQueryRaw<IncarnationHistoryProjection>(
                """
                SELECT
                    generation AS "Generation",
                    incarnation_id AS "IncarnationId",
                    idempotency_key AS "IdempotencyKey"
                FROM session_incarnations
                WHERE session_id = {0}
                ORDER BY generation
                """,
                sessionId.Value)
            .ToListAsync(TestContext.Current.CancellationToken);
        Assert.Equal([1L, 2L], history.Select(row => row.Generation));
        Assert.Equal([firstKey, secondKey], history.Select(row => row.IdempotencyKey));
        Assert.Equal(2, history.Select(row => row.IncarnationId).Distinct().Count());

        var racingSessionId = SessionId.New();
        var racingKey = Guid.CreateVersion7();
        using var raceDeadline = CancellationTokenSource.CreateLinkedTokenSource(
            TestContext.Current.CancellationToken);
        raceDeadline.CancelAfter(TimeSpan.FromSeconds(15));
        var raceToken = raceDeadline.Token;
        var commitReached = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var allowCommit = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        await using var createContext = new KodosiDbContext(options);
        var creating = CreateSessionCreator(
                createContext,
                new GateOnlySessionEndAuthority(new SessionLifecycleGate()),
                new FaultInjectingUnitOfWork(
                    new UnitOfWork(createContext),
                    CommitFaultMode.PauseBeforeCommit,
                    commitReached,
                    allowCommit))
            .CreateOwnedAsync(
                ownerId,
                new CreateSessionRequest(
                    racingSessionId.Value,
                    racingKey,
                    "receipt commit race",
                    SessionScope.JustMe,
                    ToolKind.Terminal,
                    DefaultAudienceAccess.View,
                    "receipt-race-owner-secret",
                    null),
                raceToken);
        await using var lookupContext = new KodosiDbContext(options);
        Task<SessionCreationReceiptResponse>? readingReceipt = null;
        try
        {
            await commitReached.Task.WaitAsync(TimeSpan.FromSeconds(5), raceToken);

            var lookupLockAttempted = new TaskCompletionSource<bool>(
                TaskCreationOptions.RunContinuationsAsynchronously);
            var lookup = new SessionCreationReceiptLookup(
                new SessionRepository(lookupContext),
                new SessionIncarnationRepository(lookupContext),
                new SignalingUserLifecycleLock(
                    new PostgresUserLifecycleLock(lookupContext),
                    lookupLockAttempted),
                new GateOnlySessionEndAuthority(new SessionLifecycleGate()),
                new UnitOfWork(lookupContext));
            readingReceipt = lookup.GetOwnedAsync(
                racingSessionId,
                racingKey,
                ownerId,
                raceToken);
            await lookupLockAttempted.Task.WaitAsync(TimeSpan.FromSeconds(5), raceToken);
            var lookupConnection = Assert.IsType<NpgsqlConnection>(
                lookupContext.Database.GetDbConnection());
            await WaitForBlockedAdvisoryLockAsync(
                postgres.GetConnectionString(),
                lookupConnection.ProcessID,
                raceToken);
            Assert.False(
                readingReceipt.IsCompleted,
                "receipt lookup must wait for an in-flight CREATE instead of returning a terminal 404");

            allowCommit.TrySetResult();
            var created = await creating.WaitAsync(TimeSpan.FromSeconds(5), raceToken);
            var receipt = await readingReceipt.WaitAsync(TimeSpan.FromSeconds(5), raceToken);
            Assert.Equal(created.Response.Id, receipt.SessionId);
            Assert.Equal(racingKey, receipt.CreateIdempotencyKey);
            Assert.Equal(created.Response.IncarnationId, receipt.IncarnationId);
            Assert.Equal(created.Response.IncarnationGeneration, receipt.Generation);
            Assert.Equal(created.Response.IncarnationProtocolVersion, receipt.ProtocolVersion);
        }
        finally
        {
            allowCommit.TrySetResult();
            try
            {
                _ = await creating.WaitAsync(
                    TimeSpan.FromSeconds(5),
                    TestContext.Current.CancellationToken);
            }
            catch
            {

            }
            if (readingReceipt is not null)
            {
                try
                {
                    _ = await readingReceipt.WaitAsync(
                        TimeSpan.FromSeconds(5),
                        TestContext.Current.CancellationToken);
                }
                catch
                {

                }
            }
        }
    }

    [Fact]
    public async Task SessionIncarnationMigration_Preserves_Current_Dismissals_And_Removes_Old_OnPostgres()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_incarnation_migration_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;

        var ownerId = UserId.New();
        var currentViewerId = UserId.New();
        var historicalViewerId = UserId.New();
        var sessionId = SessionId.New();
        var startedAt = new DateTimeOffset(
            2026,
            8,
            6,
            12,
            0,
            0,
            TimeSpan.Zero);

        await using var context = new KodosiDbContext(options);
        var migrator = context.GetService<IMigrator>();
        await migrator.MigrateAsync(
            "20260726105849_BoundRoomChatPayloads",
            TestContext.Current.CancellationToken);
        await context.Database.ExecuteSqlInterpolatedAsync(
            $"""
            INSERT INTO users (
                id,
                auth_subject,
                email,
                handle,
                display_name,
                created_at
            ) VALUES
                ({ownerId.Value}, 'migration-owner', 'owner@example.test', 'migration-owner', 'Owner', {startedAt.AddDays(-1)}),
                ({currentViewerId.Value}, 'migration-current', 'current@example.test', 'migration-current', 'Current', {startedAt.AddDays(-1)}),
                ({historicalViewerId.Value}, 'migration-old', 'old@example.test', 'migration-old', 'Old', {startedAt.AddDays(-1)});

            INSERT INTO sessions (
                id,
                owner_user_id,
                tool_kind,
                title,
                scope,
                default_access,
                status,
                owner_session_secret_hash,
                started_at,
                last_heartbeat_at
            ) VALUES (
                {sessionId.Value},
                {ownerId.Value},
                'Terminal',
                'migration-session',
                'Friends',
                'View',
                'Live',
                'secret-hash',
                {startedAt},
                {startedAt}
            );

            INSERT INTO session_viewer_dismissals (
                session_id,
                viewer_user_id,
                created_at
            ) VALUES
                ({sessionId.Value}, {historicalViewerId.Value}, {startedAt.AddSeconds(-1)}),
                ({sessionId.Value}, {currentViewerId.Value}, {startedAt.AddSeconds(1)});
            """,
            TestContext.Current.CancellationToken);

        await migrator.MigrateAsync(
            "20260806042353_AddSessionIncarnationId",
            TestContext.Current.CancellationToken);

        var dismissal = Assert.Single(
            await context.SessionViewerDismissals
                .AsNoTracking()
                .ToListAsync(TestContext.Current.CancellationToken));
        Assert.Equal(currentViewerId, dismissal.ViewerUserId);
        var session = await context.Sessions
            .AsNoTracking()
            .SingleAsync(
                stored => stored.Id == sessionId,
                TestContext.Current.CancellationToken);
        Assert.NotEqual(Guid.Empty, session.IncarnationId);
        Assert.Equal(1, session.IncarnationGeneration);
        Assert.Equal(Session.LegacyIncarnationProtocolVersion, session.IncarnationProtocolVersion);
        var history = await context.Database
            .SqlQueryRaw<IncarnationHistoryProjection>(
                """
                SELECT
                    generation AS "Generation",
                    incarnation_id AS "IncarnationId",
                    idempotency_key AS "IdempotencyKey"
                FROM session_incarnations
                WHERE session_id = {0}
                """,
                sessionId.Value)
            .SingleAsync(TestContext.Current.CancellationToken);
        Assert.Equal(session.IncarnationId, history.IncarnationId);
        Assert.Equal(session.IncarnationId, history.IdempotencyKey);
    }

    [Fact]
    public async Task ObsoleteDeviceLinkTimestampDowngradePreservesConsumedState()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_device_link_downgrade_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;

        await using var context = new KodosiDbContext(options);
        var migrator = context.GetService<IMigrator>();
        await migrator.MigrateAsync(
            cancellationToken: TestContext.Current.CancellationToken);
        await context.Database.ExecuteSqlRawAsync(
            """
            SET session_replication_role = replica;
            INSERT INTO device_link_requests (
                id, user_id, device_code, user_code, device_id,
                kem_public_key, signing_public_key, expires_at,
                approved_at, acknowledged_at, device_list_generation, created_at
            ) VALUES (
                '00000000-0000-0000-0000-000000000011',
                '00000000-0000-0000-0000-000000000012',
                'device-code', 'USER-CODE', 'device-id',
                '\\x01', '\\x02', '2030-01-01T00:00:00Z',
                '2026-08-19T10:00:00Z', '2026-08-19T10:05:00Z', 1,
                '2026-08-19T09:00:00Z'
            );
            SET session_replication_role = DEFAULT;
            """,
            TestContext.Current.CancellationToken);

        await migrator.MigrateAsync(
            "20260712092051_HardenBackendLifecycleAndExpiry",
            TestContext.Current.CancellationToken);

        var consumedAt = await context.Database
            .SqlQueryRaw<DateTimeOffset>(
                """
                SELECT consumed_at AS "Value"
                FROM device_link_requests
                WHERE id = '00000000-0000-0000-0000-000000000011'
                """)
            .SingleAsync(TestContext.Current.CancellationToken);
        Assert.Equal(
            new DateTimeOffset(2026, 8, 19, 10, 5, 0, TimeSpan.Zero),
            consumedAt);
    }

    [Fact]
    public async Task RoomChatMigrationDowngradesFailClosedOnPostgres()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_migration_guard_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;

        await using (var authorContext = new KodosiDbContext(options))
        {
            var migrator = authorContext.GetService<IMigrator>();
            await migrator.MigrateAsync(
                "20260718104455_DropRoomChatAuthorKindDefault",
                TestContext.Current.CancellationToken);
            await authorContext.Database.ExecuteSqlRawAsync(
                """
                SET session_replication_role = replica;
                INSERT INTO room_chat_messages (
                    id,
                    room_id,
                    author_user_id,
                    author_session_id,
                    author_kind,
                    body,
                    seq,
                    posted_at
                ) VALUES (
                    '00000000-0000-0000-0000-000000000001',
                    '00000000-0000-0000-0000-000000000002',
                    '00000000-0000-0000-0000-000000000003',
                    '00000000-0000-0000-0000-000000000004',
                    'Agent',
                    'ciphertext',
                    1,
                    now()
                );
                SET session_replication_role = DEFAULT;
                """,
                TestContext.Current.CancellationToken);

            var authorException = await Assert.ThrowsAsync<PostgresException>(() =>
                migrator.MigrateAsync(
                    "20260717182112_AddRoomChatRecipients",
                    TestContext.Current.CancellationToken));
            Assert.Contains(
                "cannot discard Agent attribution",
                authorException.MessageText,
                StringComparison.Ordinal);
        }

        await using (var recipientContext = new KodosiDbContext(options))
        {
            await recipientContext.Database.ExecuteSqlRawAsync(
                """
                UPDATE room_chat_messages
                SET author_kind = 'Human',
                    author_session_id = NULL
                """,
                TestContext.Current.CancellationToken);
            var migrator = recipientContext.GetService<IMigrator>();
            await migrator.MigrateAsync(cancellationToken: TestContext.Current.CancellationToken);
            await recipientContext.Database.ExecuteSqlRawAsync(
                """
                UPDATE room_chat_messages
                SET recipient_session_ids =
                    ARRAY['00000000-0000-0000-0000-000000000005'::uuid]
                """,
                TestContext.Current.CancellationToken);

            var recipientException = await Assert.ThrowsAsync<PostgresException>(() =>
                migrator.MigrateAsync(
                    "20260714195220_AddRoomRosterTransitionHistory",
                    TestContext.Current.CancellationToken));
            Assert.Contains(
                "cannot discard targeted recipient routing",
                recipientException.MessageText,
                StringComparison.Ordinal);
            Assert.Equal(
                [
                    "20260718104455_DropRoomChatAuthorKindDefault",
                    "20260726105849_BoundRoomChatPayloads",
                    "20260806042353_AddSessionIncarnationId",
                    "20260806143552_AddIdentityResetEnforcementMarker",
                    "20260812043926_AddSessionEndMutationReceipts",
                    "20260812150418_AddIdentityResetAudience",
                    "20260812180638_AddIdentityLifecycleRevision",
                    "20260813001222_AddSemanticRelayMailbox",
                    "20260813092740_AddDeviceRevocationEnforcement",
                    "20260813100948_AddSessionAccessMutationReceipts",
                    "20260813153702_AddPermissionDecisionPendingTuple",
                    "20260813164141_AddRoomMutationReceipts",
                    "20260814032457_FenceDelayedIdentityEnforcement",
                    "20260816062619_EnforceUserDeviceListProofPayload",
                    "20260816093107_AddPermissionRequestGenerationFence",
                    "20260816202428_RepairCurrentKeyGeneration",
                    "20260816235234_DropRedundantIdentityObservations",
                    "20260817094400_DropSessionViewerDismissalCreatedAt",
                    "20260817112447_DropUnusedPersistenceIndexes",
                    "20260818034840_AddDeviceRevocationSessionTargets",
                    "20260818074703_AddIdentityResetSessionTargets",
                    "20260819023532_DropUnusedReceiptIndexes",
                    "20260819031336_DropDuplicateDeviceLinkCertificateReceipt",
                    "20260819104548_DropObsoleteConsumedTimestamps",
                    "20260820000539_CollapseUserDeviceListAuthority",
                    "20260820010613_WidenDeviceRevocationGeneration",
                    "20260820125920_CollapseUserDeviceCertificateAuthority",
                    "20260820153800_AddAccessOverrideExpiryEnforcement",
                    "20260906132920_AddArtifactEndorsements",
                    "20260906164733_AddRoomTaskSnapshotRevision",
                ],
                await recipientContext.Database.GetPendingMigrationsAsync(
                    TestContext.Current.CancellationToken));
            await migrator.MigrateAsync(
                cancellationToken: TestContext.Current.CancellationToken);
            Assert.Empty(await recipientContext.Database.GetPendingMigrationsAsync(
                TestContext.Current.CancellationToken));
            await AssertRoomChatAuthorKindHasNoDefaultAsync(recipientContext);
        }
    }

    [Fact]
    public async Task Migrations_And_Critical_Concurrency_Gates_Work_On_Postgres()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql(postgres.GetConnectionString())
            .Options;

        var ownerId = UserId.New();
        var inviteeId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var sessionIds = Enumerable.Range(0, 501)
            .Select(_ => SessionId.New())
            .ToArray();
        var linkRequestId = Guid.NewGuid();
        await using (var setup = new KodosiDbContext(options))
        {
            await setup.Database.MigrateAsync(TestContext.Current.CancellationToken);
            await AssertRoomChatAuthorKindHasNoDefaultAsync(setup);
            var owner = User.Create(
                ownerId,
                "owner@example.test",
                "owner",
                "Owner");
            var invitee = User.Create(
                inviteeId,
                "invitee@example.test",
                "invitee",
                "Invitee");
            var room = Room.Create(
                roomId,
                ownerId,
                "Room",
                "room",
                1,
                [1],
                [2],
                "owner-device");
            setup.Users.Add(owner);
            setup.Users.Add(invitee);
            setup.Rooms.Add(room);
            setup.RoomMembers.Add(DomainFixtureHydrator.RoomMember(roomId, ownerId));
            for (var index = 0; index < sessionIds.Length; index++)
            {
                setup.Sessions.Add(Session.Create(
                    sessionIds[index], Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
                    ownerId,
                    $"session-{index}",
                    SessionScope.Room,
                    ToolKind.Terminal,
                    AccessLevel.View,
                    "secret-hash",
                    roomId));
            }

            var linkRequest = DeviceLinkRequest.Create(
                ownerId,
                "device-code",
                "ABCD-EFGH",
                "new-device",
                "Laptop",
                new byte[1184],
                new byte[1952],
                TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow);
            typeof(DeviceLinkRequest)
                .GetProperty(nameof(DeviceLinkRequest.Id))!
                .SetValue(linkRequest, linkRequestId);
            setup.DeviceLinkRequests.Add(linkRequest);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
            Assert.Empty(await setup.Database.GetPendingMigrationsAsync(
                TestContext.Current.CancellationToken));
        }

        await AssertDeviceLinkConcurrencyAsync(options, linkRequestId);
        await AssertExpectedUniqueConstraintClassificationAsync(
            options,
            ownerId,
            inviteeId);
        await AssertRoomConcurrencyAsync(options, roomId);
        await AssertUserLifecycleLockAsync(options, ownerId);
        await AssertDeviceLinkCancellationSerializesWithApprovalAsync(options, ownerId);
        await AssertCreateIdentityResetLockOrderAsync(options, ownerId);
        await AssertFriendshipRemovalSerializesFriendPublicationAsync(
            options,
            ownerId,
            inviteeId);
        await AssertRoomRemovalSerializesSessionPublicationAsync(
            options,
            ownerId,
            inviteeId,
            republish: false);
        await AssertRoomRemovalSerializesSessionPublicationAsync(
            options,
            ownerId,
            inviteeId,
            republish: true);
        await AssertRoomRemovalSerializesScopeMoveAsync(
            options,
            inviteeId,
            moveIntoRoom: true);
        await AssertRoomRemovalSerializesScopeMoveAsync(
            options,
            inviteeId,
            moveIntoRoom: false);
        await AssertSessionKeyGenerationConcurrencyAsync(
            options,
            sessionIds[0],
            ownerId);
        await AssertSessionKeyBlobReplacementRollbackAsync(options, ownerId);
        await AssertSessionKeyBlobReplacementWaitsForRevocationAsync(options);
        await AssertSessionKeyPublicationRejectsExpiredRecipientAsync(options);
        await AssertCommittedDeviceRevocationGatesParticipantDispatchAsync(options);
        await AssertNonRoomSessionCreateRetryNormalizesRoomIdAsync(
            options,
            ownerId);
        await AssertSessionKeyGenerationExhaustionAsync(options, ownerId);
        await AssertSessionEndClaimSerializationAsync(options, ownerId);
        await AssertSessionEndMutationDurabilityAsync(options, ownerId);
        await AssertSessionAccessMutationDurabilityAsync(
            options,
            ownerId,
            inviteeId);
        await AssertHostActivationRejectsOldSecretAfterRepublishAsync(
            options,
            ownerId);
        await AssertParticipantCountIncarnationAsync(options, ownerId);
        await AssertOrphanMarkerRevalidationAsync(options, ownerId);
        await AssertIdentityResetRecipientSessionsCapturedAsync(
            options,
            ownerId);
        await AssertIdentityResetCommitFaultAsync(
            options,
            CommitFaultMode.BeforeCommit);
        await AssertIdentityResetCommitFaultAsync(
            options,
            CommitFaultMode.AfterCommit);
        await AssertIdentityResetFencesStaleKeyPublicationAsync(options);
        await AssertIdentityResetSerializesFirstKeyPublicationAsync(options);
        await AssertStaleSharingGrantRejectedAsync(options, ownerId);
        await AssertRoomForUpdateProjectsConcurrencyTokenAsync(options, roomId, sessionIds[1]);
        await AssertRoomEntityIdempotencyAsync(options, roomId, ownerId);
        await AssertRoomChatCrossRoomMessageIdConcurrencyAsync(options, roomId, ownerId);
        await AssertRoomChatReadsBeyondLegacyCapAsync(options, roomId, ownerId);
        await AssertRoomChatTailReturnsNewestWindowAsync(options, roomId, ownerId);
        await AssertRoomChatMaxBodyPageAsync(options, roomId, ownerId);
        await AssertRoomCatalogAndTransitionPagesAsync(options, ownerId);
        await AssertRoomFeedCursorPrecisionAsync(options, roomId, ownerId);
        await AssertInvitationLifecyclePersistenceAsync(
            options,
            roomId,
            ownerId,
            inviteeId);
        await AssertAccessOverrideExpiryAsync(
            options,
            sessionIds[0],
            ownerId,
            inviteeId);
        await AssertDeviceLinkGenerationInvalidationAsync(options, ownerId, inviteeId);
        await AssertDeviceLinkAcknowledgementAsync(options, ownerId);
        await AssertDeviceLinkTerminalRetentionAsync(options, ownerId);

        await using var queryContext = new KodosiDbContext(options);
        var sessionIdsInRoom = await new SessionRepository(queryContext)
            .GetAllNonEndedByRoomIdsAsync(
                roomId,
                TestContext.Current.CancellationToken);
        Assert.Equal(501, sessionIdsInRoom.Count);
    }

    private static async Task AssertRoomConcurrencyAsync(
        DbContextOptions<KodosiDbContext> options,
        RoomId roomId)
    {
        await using var firstContext = new KodosiDbContext(options);
        await using var secondContext = new KodosiDbContext(options);
        var first = await firstContext.Rooms.SingleAsync(
            room => room.Id == roomId,
            TestContext.Current.CancellationToken);
        var second = await secondContext.Rooms.SingleAsync(
            room => room.Id == roomId,
            TestContext.Current.CancellationToken);
        first.ReplaceRosterForRemoval(2, [3], [4], "owner-device");
        second.ReplaceRosterForRemoval(2, [5], [6], "owner-device");

        await new UnitOfWork(firstContext).SaveChangesAsync(
            TestContext.Current.CancellationToken);
        var conflict = await Assert.ThrowsAsync<ConcurrentModificationException>(() =>
            new UnitOfWork(secondContext).SaveChangesAsync(
                TestContext.Current.CancellationToken));
        Assert.Equal("CONCURRENT_MODIFICATION", conflict.Code);
    }

    private static async Task AssertExpectedUniqueConstraintClassificationAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId,
        UserId inviteeId)
    {
        await using (var roomContext = new KodosiDbContext(options))
        {
            roomContext.Rooms.Add(Room.Create(
                RoomId.From(Guid.NewGuid()),
                ownerId,
                "Duplicate slug",
                "room",
                1,
                [1],
                [2],
                "owner-device"));
            var exception = await Assert.ThrowsAsync<DbUpdateException>(() =>
                roomContext.SaveChangesAsync(TestContext.Current.CancellationToken));
            var postgres = Assert.IsType<PostgresException>(exception.InnerException);
            Assert.Equal(PostgresErrorCodes.UniqueViolation, postgres.SqlState);
            Assert.Equal(
                "IX_rooms_slug",
                postgres.ConstraintName);
            Assert.True(EndpointPostgresExceptionHelpers.IsRoomSlugConflict(exception));
            Assert.False(EndpointPostgresExceptionHelpers.IsFriendshipConflict(exception));
        }

        await using (var friendshipContext = new KodosiDbContext(options))
        {
            friendshipContext.Friendships.Add(
                Friendship.CreateRequest(ownerId, inviteeId));
            await friendshipContext.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using (var duplicateContext = new KodosiDbContext(options))
        {
            duplicateContext.Friendships.Add(
                Friendship.CreateRequest(inviteeId, ownerId));
            var exception = await Assert.ThrowsAsync<DbUpdateException>(() =>
                duplicateContext.SaveChangesAsync(TestContext.Current.CancellationToken));
            var postgres = Assert.IsType<PostgresException>(exception.InnerException);
            Assert.Equal(PostgresErrorCodes.UniqueViolation, postgres.SqlState);
            Assert.Equal(
                "PK_friendships",
                postgres.ConstraintName);
            Assert.True(EndpointPostgresExceptionHelpers.IsFriendshipConflict(exception));
            Assert.False(EndpointPostgresExceptionHelpers.IsRoomSlugConflict(exception));
        }
    }

    private static async Task AssertRoomChatReadsBeyondLegacyCapAsync(
        DbContextOptions<KodosiDbContext> options,
        RoomId roomId,
        UserId ownerId)
    {
        await using var context = new KodosiDbContext(options);
        var existingMax = await context.RoomChatMessages
            .Where(message => message.RoomId == roomId)
            .MaxAsync(
                message => (long?)message.Seq,
                TestContext.Current.CancellationToken)
            ?? 0;
        var messages = Enumerable.Range(1, 600)
            .Select(offset => RoomChatMessage.Create(
                Guid.NewGuid(),
                roomId,
                ownerId,
                null,
                RoomChatAuthorKind.Human,
                [],
                [],
                $"ciphertext-{offset}",
                existingMax + offset))
            .ToList();
        context.RoomChatMessages.AddRange(messages);
        await context.SaveChangesAsync(TestContext.Current.CancellationToken);

        var loaded = await new RoomChatRepository(context).GetPageCandidatesSinceAsync(
            roomId,
            existingMax,
            RoomChatReadPolicy.MaxLimit,
            TestContext.Current.CancellationToken);

        Assert.Equal(600, loaded.Count);
    }

    private static async Task AssertRoomChatTailReturnsNewestWindowAsync(
        DbContextOptions<KodosiDbContext> options,
        RoomId roomId,
        UserId ownerId)
    {
        await using var context = new KodosiDbContext(options);
        var existingMax = await context.RoomChatMessages
            .Where(message => message.RoomId == roomId)
            .MaxAsync(
                message => (long?)message.Seq,
                TestContext.Current.CancellationToken)
            ?? 0;
        var messages = Enumerable.Range(1, 6)
            .Select(offset => RoomChatMessage.Create(
                Guid.NewGuid(),
                roomId,
                ownerId,
                null,
                RoomChatAuthorKind.Human,
                [],
                [],
                $"tail-{offset}",
                existingMax + offset))
            .ToList();
        context.RoomChatMessages.AddRange(messages);
        await context.SaveChangesAsync(TestContext.Current.CancellationToken);

        var candidates = await new RoomChatRepository(context).GetTailCandidatesAsync(
            roomId,
            beforeSeq: null,
            limit: 3,
            TestContext.Current.CancellationToken);
        var page = RoomChatReadPolicy.CreateTailPage(candidates, 3);

        Assert.Equal(
            [existingMax + 4, existingMax + 5, existingMax + 6],
            page.Items.Select(message => message.Seq));
        Assert.True(page.HasMore);
        Assert.Equal(existingMax + 4, page.NextBefore);
    }

    private static async Task AssertRoomChatMaxBodyPageAsync(
        DbContextOptions<KodosiDbContext> options,
        RoomId roomId,
        UserId ownerId)
    {
        await using var context = new KodosiDbContext(options);
        var existingMax = await context.RoomChatMessages
            .Where(message => message.RoomId == roomId)
            .MaxAsync(
                message => (long?)message.Seq,
                TestContext.Current.CancellationToken)
            ?? 0;
        var maxBody = new string('x', RoomInputRules.EncryptedContentMaxLength);
        var messages = Enumerable.Range(1, 3)
            .Select(offset => RoomChatMessage.Create(
                Guid.NewGuid(),
                roomId,
                ownerId,
                null,
                RoomChatAuthorKind.Human,
                [],
                [],
                maxBody,
                existingMax + offset))
            .ToList();
        context.RoomChatMessages.AddRange(messages);
        await context.SaveChangesAsync(TestContext.Current.CancellationToken);

        var candidates = await new RoomChatRepository(context).GetPageCandidatesSinceAsync(
            roomId,
            existingMax,
            RoomChatReadPolicy.MaxLimit,
            TestContext.Current.CancellationToken);
        var page = RoomChatReadPolicy.CreatePage(
            candidates,
            RoomChatReadPolicy.MaxLimit);

        Assert.Equal(3, candidates.Count);
        Assert.Equal(2, page.Items.Count);
        Assert.True(page.HasMore);
        Assert.Equal(existingMax + 2, page.NextSince);
    }

    private static async Task AssertRoomCatalogAndTransitionPagesAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId)
    {
        await using var context = new KodosiDbContext(options);
        var rooms = await new RoomRepository(context).GetByMemberPageAsync(
            ownerId,
            cursor: null,
            limit: 2,
            TestContext.Current.CancellationToken);
        Assert.NotEmpty(rooms);
        var first = rooms[0];
        var afterFirst = await new RoomRepository(context).GetByMemberPageAsync(
            ownerId,
            new FeedCursor(first.CreatedAt, first.Id.Value),
            limit: 2,
            TestContext.Current.CancellationToken);
        Assert.DoesNotContain(afterFirst, room => room.Id == first.Id);

        var transitions = await new RoomRosterTransitionRepository(context).GetPageAfterAsync(
            first.Id,
            afterGeneration: 0,
            limit: 2,
            TestContext.Current.CancellationToken);
        Assert.All(transitions, transition => Assert.True(transition.Generation > 0));
        Assert.True(transitions.SequenceEqual(transitions.OrderBy(item => item.Generation)));
    }

    private static async Task AssertRoomFeedCursorPrecisionAsync(
        DbContextOptions<KodosiDbContext> options,
        RoomId roomId,
        UserId ownerId)
    {
        var newerId = SessionId.New();
        var olderId = SessionId.New();
        await using (var setup = new KodosiDbContext(options))
        {
            var newer = Session.Create(
                newerId, Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
                ownerId,
                "same-ms-newer",
                SessionScope.Room,
                ToolKind.Terminal,
                AccessLevel.View,
                "secret-hash",
                roomId);
            var older = Session.Create(
                olderId, Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
                ownerId,
                "same-ms-older",
                SessionScope.Room,
                ToolKind.Terminal,
                AccessLevel.View,
                "secret-hash",
                roomId);
            newer.ActivateHost("test-host");
            newer.ReleaseHostSlot("test-host");
            older.ActivateHost("test-host");
            older.ReleaseHostSlot("test-host");
            setup.Sessions.AddRange(newer, older);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);

            var millisecond = DateTimeOffset.FromUnixTimeMilliseconds(1_752_806_523_123);
            var newerStartedAt = millisecond.AddTicks(9000);
            var olderStartedAt = millisecond.AddTicks(4000);
            await setup.Database.ExecuteSqlInterpolatedAsync(
                $"UPDATE sessions SET started_at = {newerStartedAt} WHERE id = {newerId.Value}",
                TestContext.Current.CancellationToken);
            await setup.Database.ExecuteSqlInterpolatedAsync(
                $"UPDATE sessions SET started_at = {olderStartedAt} WHERE id = {olderId.Value}",
                TestContext.Current.CancellationToken);
        }

        await using (var query = new KodosiDbContext(options))
        {
            var repository = new SessionRepository(query);
            var firstPage = await repository.GetLiveByRoomProjectedAsync(
                roomId,
                limit: 1,
                ct: TestContext.Current.CancellationToken);
            Assert.Equal(newerId.Value, firstPage[0].Id);

            var secondPage = await repository.GetLiveByRoomProjectedAsync(
                roomId,
                new FeedCursor(firstPage[0].StartedAt, firstPage[0].Id),
                limit: 1,
                ct: TestContext.Current.CancellationToken);
            Assert.Equal(olderId.Value, secondPage[0].Id);
        }

        await using var cleanup = new KodosiDbContext(options);
        _ = await cleanup.Sessions
            .Where(session => session.Id == newerId || session.Id == olderId)
            .ExecuteDeleteAsync(TestContext.Current.CancellationToken);
    }

    private static async Task AssertDeviceLinkConcurrencyAsync(
        DbContextOptions<KodosiDbContext> options,
        Guid linkRequestId)
    {
        await using var firstContext = new KodosiDbContext(options);
        await using var secondContext = new KodosiDbContext(options);
        var first = await firstContext.DeviceLinkRequests.SingleAsync(
            request => request.Id == linkRequestId,
            TestContext.Current.CancellationToken);
        var second = await secondContext.DeviceLinkRequests.SingleAsync(
            request => request.Id == linkRequestId,
            TestContext.Current.CancellationToken);
        DomainFixtureHydrator.CancelDeviceLink(first, DateTimeOffset.UtcNow);
        DomainFixtureHydrator.CancelDeviceLink(second, DateTimeOffset.UtcNow);
        new DeviceLinkRequestRepository(firstContext).Update(first);
        new DeviceLinkRequestRepository(secondContext).Update(second);

        await new UnitOfWork(firstContext).SaveChangesAsync(
            TestContext.Current.CancellationToken);
        var conflict = await Assert.ThrowsAsync<ConcurrentModificationException>(() =>
            new UnitOfWork(secondContext).SaveChangesAsync(
                TestContext.Current.CancellationToken));
        Assert.Equal("CONCURRENT_MODIFICATION", conflict.Code);
    }

    private static async Task AssertUserLifecycleLockAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId userId)
    {
        await using var firstContext = new KodosiDbContext(options);
        await using var secondContext = new KodosiDbContext(options);
        var firstUnitOfWork = new UnitOfWork(firstContext);
        var secondUnitOfWork = new UnitOfWork(secondContext);
        await using var firstTransaction = await firstUnitOfWork.BeginTransactionAsync(
            TestContext.Current.CancellationToken);
        await new PostgresUserLifecycleLock(firstContext).AcquireAsync(
            userId,
            TestContext.Current.CancellationToken);
        await using var secondTransaction = await secondUnitOfWork.BeginTransactionAsync(
            TestContext.Current.CancellationToken);
        var waiting = new PostgresUserLifecycleLock(secondContext).AcquireAsync(
            userId,
            TestContext.Current.CancellationToken);

        await Task.Delay(50, TestContext.Current.CancellationToken);
        Assert.False(waiting.IsCompleted);
        await firstTransaction.CommitAsync(TestContext.Current.CancellationToken);
        await waiting;
        await secondTransaction.CommitAsync(TestContext.Current.CancellationToken);
    }

    private static async Task AssertDeviceLinkCancellationSerializesWithApprovalAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId userId)
    {
        var request = DeviceLinkRequest.Create(
            userId,
            $"cancel-race-device-{Guid.NewGuid():N}",
            $"CR-{Guid.NewGuid():N}"[..9],
            $"cancel-race-device-{Guid.NewGuid():N}",
            "Cancellation race",
            new byte[1184],
            new byte[1952],
            TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow);
        await using (var setup = new KodosiDbContext(options))
        {
            setup.DeviceLinkRequests.Add(request);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using var approvalContext = new KodosiDbContext(options);
        var approvalUnitOfWork = new UnitOfWork(approvalContext);
        await using var approvalTransaction = await approvalUnitOfWork.BeginTransactionAsync(
            TestContext.Current.CancellationToken);
        await new PostgresUserLifecycleLock(approvalContext).AcquireAsync(
            userId,
            TestContext.Current.CancellationToken);
        var approvalRepository = new DeviceLinkRequestRepository(approvalContext);
        var approving = await approvalRepository.GetByUserCodeAsync(
            request.UserCode,
            TestContext.Current.CancellationToken);
        Assert.NotNull(approving);
        approving!.Approve(2, DateTimeOffset.UtcNow);
        approvalRepository.Update(approving);
        await approvalUnitOfWork.SaveChangesAsync(TestContext.Current.CancellationToken);

        var cancellationStarted = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var cancelling = Task.Run(async () =>
        {
            await using var cancellationContext = new KodosiDbContext(options);
            var unitOfWork = new UnitOfWork(cancellationContext);
            await using var transaction = await unitOfWork.BeginTransactionAsync(
                TestContext.Current.CancellationToken);
            cancellationStarted.TrySetResult();
            await new PostgresUserLifecycleLock(cancellationContext).AcquireAsync(
                userId,
                TestContext.Current.CancellationToken);
            var outcome = await new DeviceLinkRequestRepository(cancellationContext)
                .CancelPendingAsync(
                    request.UserCode,
                    userId,
                    DateTimeOffset.UtcNow,
                    TestContext.Current.CancellationToken);
            await transaction.CommitAsync(TestContext.Current.CancellationToken);
            return outcome;
        });
        await cancellationStarted.Task.WaitAsync(TestContext.Current.CancellationToken);
        await Task.Delay(50, TestContext.Current.CancellationToken);
        Assert.False(cancelling.IsCompleted);

        await approvalTransaction.CommitAsync(TestContext.Current.CancellationToken);
        Assert.Equal(DeviceLinkCancelOutcome.Approved, await cancelling);

        await using var verify = new KodosiDbContext(options);
        var stored = await verify.DeviceLinkRequests.SingleAsync(
            item => item.Id == request.Id,
            TestContext.Current.CancellationToken);
        Assert.NotNull(stored.ApprovedAt);
        Assert.Null(stored.CancelledAt);

        var cancelledFirst = CreateDeviceLinkRequest(userId, "cancel-wins-race");
        await using (var setup = new KodosiDbContext(options))
        {
            setup.DeviceLinkRequests.Add(cancelledFirst);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        await using var cancellationContext = new KodosiDbContext(options);
        var cancellationUnitOfWork = new UnitOfWork(cancellationContext);
        await using var cancellationTransaction = await cancellationUnitOfWork.BeginTransactionAsync(
            TestContext.Current.CancellationToken);
        await new PostgresUserLifecycleLock(cancellationContext).AcquireAsync(
            userId,
            TestContext.Current.CancellationToken);
        Assert.Equal(
            DeviceLinkCancelOutcome.Cancelled,
            await new DeviceLinkRequestRepository(cancellationContext).CancelPendingAsync(
                cancelledFirst.UserCode,
                userId,
                DateTimeOffset.UtcNow,
                TestContext.Current.CancellationToken));

        var approvalObserverStarted = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var approvalObserver = Task.Run(async () =>
        {
            await using var observationContext = new KodosiDbContext(options);
            var unitOfWork = new UnitOfWork(observationContext);
            await using var transaction = await unitOfWork.BeginTransactionAsync(
                TestContext.Current.CancellationToken);
            approvalObserverStarted.TrySetResult();
            await new PostgresUserLifecycleLock(observationContext).AcquireAsync(
                userId,
                TestContext.Current.CancellationToken);
            var row = await new DeviceLinkRequestRepository(observationContext)
                .GetByUserCodeAsync(
                    cancelledFirst.UserCode,
                    TestContext.Current.CancellationToken);
            await transaction.CommitAsync(TestContext.Current.CancellationToken);
            return row?.IsPending(DateTimeOffset.UtcNow) == true;
        });
        await approvalObserverStarted.Task.WaitAsync(TestContext.Current.CancellationToken);
        await Task.Delay(50, TestContext.Current.CancellationToken);
        Assert.False(approvalObserver.IsCompleted);

        await cancellationTransaction.CommitAsync(TestContext.Current.CancellationToken);
        Assert.False(await approvalObserver);
        await using var cancelledVerify = new KodosiDbContext(options);
        var cancelledStored = await cancelledVerify.DeviceLinkRequests
            .AsNoTracking()
            .SingleAsync(
                item => item.Id == cancelledFirst.Id,
                TestContext.Current.CancellationToken);
        Assert.Null(cancelledStored.ApprovedAt);
        Assert.NotNull(cancelledStored.CancelledAt);
    }

    private static async Task AssertCreateIdentityResetLockOrderAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId)
    {
        var sessionId = SessionId.New();
        var lifecycleGate = new SessionLifecycleGate();
        var authority = new GateOnlySessionEndAuthority(lifecycleGate);
        var externalGate = await lifecycleGate.AcquireAsync(
            sessionId,
            TestContext.Current.CancellationToken);
        var userLockAcquired = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);

        var identityReset = Task.Run(async () =>
        {
            await using var context = new KodosiDbContext(options);
            await using var transaction = await new UnitOfWork(context)
                .BeginTransactionAsync(TestContext.Current.CancellationToken);
            await new PostgresUserLifecycleLock(context).AcquireAsync(
                ownerId,
                TestContext.Current.CancellationToken);
            userLockAcquired.TrySetResult();
            await using var sessionGates = await authority.AcquireAsync(
                [sessionId],
                TestContext.Current.CancellationToken);
            await transaction.CommitAsync(TestContext.Current.CancellationToken);
        });
        await userLockAcquired.Task.WaitAsync(
            TestContext.Current.CancellationToken);

        await using var createContext = new KodosiDbContext(options);
        var creating = CreateSessionCreator(createContext, authority).CreateOwnedAsync(
            ownerId,
            new CreateSessionRequest(
                sessionId.Value,
                Guid.CreateVersion7(),
                "create-vs-reset",
                SessionScope.JustMe,
                ToolKind.Terminal,
                DefaultAudienceAccess.View,
                "owner-secret",
                null),
            TestContext.Current.CancellationToken);

        await Task.Delay(50, TestContext.Current.CancellationToken);
        Assert.False(creating.IsCompleted);
        Assert.Equal(2, lifecycleGate.TestReferenceCount(sessionId));

        await externalGate.DisposeAsync();
        await identityReset;
        var created = await creating;
        Assert.Equal(sessionId.Value, created.Response.Id);
    }

    private static async Task AssertRoomRemovalSerializesSessionPublicationAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId roomOwnerId,
        UserId memberUserId,
        bool republish)
    {
        _ = roomOwnerId;
        var roomId = RoomId.From(Guid.NewGuid());
        var sessionId = SessionId.New();
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Rooms.Add(Room.Create(
                roomId,
                memberUserId,
                republish ? "Republish lock room" : "Create lock room",
                $"room-lock-{Guid.NewGuid():N}",
                1,
                [1],
                [2],
                "owner-device"));
            setup.RoomMembers.Add(
                DomainFixtureHydrator.RoomMember(roomId, memberUserId));
            if (republish)
            {
                var ended = Session.Create(
                    sessionId, Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
                    memberUserId,
                    "ended-room-session",
                    SessionScope.Room,
                    ToolKind.Terminal,
                    AccessLevel.View,
                    "old-secret",
                    roomId);
                ended.End();
                setup.Sessions.Add(ended);
            }
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var roomLockAcquired = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var allowRemovalCommit = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var removing = Task.Run(async () =>
        {
            await using var context = new KodosiDbContext(options);
            await using var transaction = await new UnitOfWork(context)
                .BeginTransactionAsync(TestContext.Current.CancellationToken);
            await new PostgresRoomLifecycleLock(context).AcquireAsync(
                roomId,
                TestContext.Current.CancellationToken);
            roomLockAcquired.TrySetResult();
            await allowRemovalCommit.Task.WaitAsync(
                TestContext.Current.CancellationToken);
            var member = await context.RoomMembers.SingleAsync(
                candidate => candidate.RoomId == roomId
                    && candidate.UserId == memberUserId,
                TestContext.Current.CancellationToken);
            member.Revoke();
            await context.SaveChangesAsync(TestContext.Current.CancellationToken);
            await transaction.CommitAsync(TestContext.Current.CancellationToken);
        });
        await roomLockAcquired.Task.WaitAsync(
            TestContext.Current.CancellationToken);

        await using var publishContext = new KodosiDbContext(options);
        var publishing = CreateSessionCreator(
            publishContext,
            new GateOnlySessionEndAuthority(new SessionLifecycleGate()))
            .CreateOwnedAsync(
                memberUserId,
                new CreateSessionRequest(
                    sessionId.Value,
                    Guid.CreateVersion7(),
                    republish ? "republished-room-session" : "new-room-session",
                    SessionScope.Room,
                    ToolKind.Terminal,
                    DefaultAudienceAccess.View,
                    "new-secret",
                    roomId.Value),
                TestContext.Current.CancellationToken);

        await Task.Delay(50, TestContext.Current.CancellationToken);
        Assert.False(publishing.IsCompleted);
        allowRemovalCommit.TrySetResult();
        await removing;
        await Assert.ThrowsAsync<PolicyViolationException>(() => publishing);

        if (republish)
        {
            await using var verify = new KodosiDbContext(options);
            var persisted = await verify.Sessions
                .AsNoTracking()
                .SingleAsync(
                    session => session.Id == sessionId,
                    TestContext.Current.CancellationToken);
            Assert.Equal(SessionStatus.Ended, persisted.Status);
        }
    }

    private static async Task AssertFriendshipRemovalSerializesFriendPublicationAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId firstUserId,
        UserId secondUserId)
    {
        var updateSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            secondUserId,
            "friend-scope-update",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Sessions.Add(updateSession);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using var removalContext = new KodosiDbContext(options);
        await using var removalTransaction = await new UnitOfWork(removalContext)
            .BeginTransactionAsync(TestContext.Current.CancellationToken);
        foreach (var userId in new[] { firstUserId, secondUserId }
            .OrderBy(userId => userId.Value))
        {
            await new PostgresUserLifecycleLock(removalContext).AcquireAsync(
                userId,
                TestContext.Current.CancellationToken);
        }

        var createSessionId = SessionId.New();
        await using var createContext = new KodosiDbContext(options);
        var creating = CreateSessionCreator(
            createContext,
            new GateOnlySessionEndAuthority(new SessionLifecycleGate()))
            .CreateOwnedAsync(
                firstUserId,
                new CreateSessionRequest(
                    createSessionId.Value,
                    Guid.CreateVersion7(),
                    "friend-publication",
                    SessionScope.Friends,
                    ToolKind.Terminal,
                    DefaultAudienceAccess.View,
                    "secret",
                    null),
                TestContext.Current.CancellationToken);

        await using var updateContext = new KodosiDbContext(options);
        var updateRepository = new SessionRepository(updateContext);
        var updating = new SessionUpdater(
            updateRepository,
            new SessionRoomResolver(new RoomMemberRepository(updateContext)),
            new AccessOverrideRepository(updateContext, TimeProvider.System),
            new AccessOverrideAuditRepository(updateContext),
            new FriendshipRepository(updateContext),
            new RoomMemberRepository(updateContext),
            new PostgresUserLifecycleLock(updateContext),
            new PostgresRoomLifecycleLock(updateContext),
            new GateOnlySessionEndAuthority(new SessionLifecycleGate()),
            new UnitOfWork(updateContext))
            .UpdateOwnedAsync(
                updateSession.Id,
                secondUserId,
                new UpdateSessionRequest(
                    updateSession.IncarnationId,
                    null,
                    SessionScope.Friends,
                    null,
                    null),
                TestContext.Current.CancellationToken);

        await Task.Delay(50, TestContext.Current.CancellationToken);
        Assert.False(creating.IsCompleted);
        Assert.False(updating.IsCompleted);
        await removalTransaction.CommitAsync(
            TestContext.Current.CancellationToken);

        Assert.Equal(createSessionId.Value, (await creating).Response.Id);
        Assert.Equal(SessionScope.Friends, (await updating).Response.Scope);
    }

    private static async Task WaitForBlockedAdvisoryLockAsync(
        string connectionString,
        int backendProcessId,
        CancellationToken ct)
    {
        await using var observer = new NpgsqlConnection(connectionString);
        await observer.OpenAsync(ct);
        while (true)
        {
            await using var command = new NpgsqlCommand(
                """
                SELECT EXISTS (
                    SELECT 1
                    FROM pg_locks
                    WHERE pid = @pid
                      AND locktype = 'advisory'
                      AND NOT granted)
                """,
                observer);
            command.Parameters.AddWithValue("pid", backendProcessId);
            if (await command.ExecuteScalarAsync(ct) is true)
            {
                return;
            }
            await Task.Delay(10, ct);
        }
    }

    private static SessionCreator CreateSessionCreator(
        KodosiDbContext context,
        ISessionEndAuthority authority,
        IUnitOfWork? unitOfWork = null)
    {
        var sessions = new SessionRepository(context);
        return new SessionCreator(
            sessions,
            new SessionIncarnationRepository(context),
            new OwnerSessionSecretHasher(),
            new SessionRoomResolver(new RoomMemberRepository(context)),
            new SessionAccessOverrideRevoker(
                sessions,
                new AccessOverrideRepository(context, TimeProvider.System),
                new SessionKeyBlobRepository(context)),
            new SessionViewerDismissalRepository(context),
            new SessionKeyBlobRepository(context),
            new PostgresUserLifecycleLock(context),
            new PostgresRoomLifecycleLock(context),
            authority,
            unitOfWork ?? new UnitOfWork(context));
    }

    private static async Task EndPersistedSessionAsync(
        DbContextOptions<KodosiDbContext> options,
        SessionId sessionId)
    {
        await using var context = new KodosiDbContext(options);
        var session = await context.Sessions.SingleAsync(
            candidate => candidate.Id == sessionId,
            TestContext.Current.CancellationToken);
        session.End();
        await context.SaveChangesAsync(TestContext.Current.CancellationToken);
    }

    private sealed record IncarnationHistoryProjection(
        long Generation,
        Guid IncarnationId,
        Guid IdempotencyKey);

    private static async Task AssertRoomRemovalSerializesScopeMoveAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId,
        bool moveIntoRoom)
    {
        var roomId = RoomId.From(Guid.NewGuid());
        var sessionId = SessionId.New();
        Guid sessionIncarnationId;
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Rooms.Add(Room.Create(
                roomId,
                ownerId,
                moveIntoRoom ? "Move in room" : "Move out room",
                $"scope-lock-{Guid.NewGuid():N}",
                1,
                [1],
                [2],
                "owner-device"));
            setup.RoomMembers.Add(
                DomainFixtureHydrator.RoomMember(roomId, ownerId));
            var session = Session.Create(
                sessionId, Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
                ownerId,
                "scope-move-session",
                moveIntoRoom ? SessionScope.Friends : SessionScope.Room,
                ToolKind.Terminal,
                AccessLevel.View,
                "secret",
                moveIntoRoom ? null : roomId);
            sessionIncarnationId = session.IncarnationId;
            setup.Sessions.Add(session);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var roomLockAcquired = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var allowRemovalCommit = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var removalObservedSession = false;
        var removing = Task.Run(async () =>
        {
            await using var context = new KodosiDbContext(options);
            await using var transaction = await new UnitOfWork(context)
                .BeginTransactionAsync(TestContext.Current.CancellationToken);
            await new PostgresRoomLifecycleLock(context).AcquireAsync(
                roomId,
                TestContext.Current.CancellationToken);
            var ids = await new SessionRepository(context)
                .GetAllNonEndedByRoomIdsAsync(
                    roomId,
                    TestContext.Current.CancellationToken);
            removalObservedSession = ids.Contains(sessionId);
            roomLockAcquired.TrySetResult();
            await allowRemovalCommit.Task.WaitAsync(
                TestContext.Current.CancellationToken);
            var member = await context.RoomMembers.SingleAsync(
                candidate => candidate.RoomId == roomId
                    && candidate.UserId == ownerId,
                TestContext.Current.CancellationToken);
            member.Revoke();
            await context.SaveChangesAsync(TestContext.Current.CancellationToken);
            await transaction.CommitAsync(TestContext.Current.CancellationToken);
        });
        await roomLockAcquired.Task.WaitAsync(
            TestContext.Current.CancellationToken);

        await using var updateContext = new KodosiDbContext(options);
        var repository = new SessionRepository(updateContext);
        var roomMembers = new RoomMemberRepository(updateContext);
        var authority =
            new GateOnlySessionEndAuthority(new SessionLifecycleGate());
        var updater = new SessionUpdater(
            repository,
            new SessionRoomResolver(roomMembers),
            new FakeAccessOverrideRepository(),
            new FakeAccessOverrideAuditRepository(),
            new FakeFriendshipRepository(),
            roomMembers,
            new PostgresUserLifecycleLock(updateContext),
            new PostgresRoomLifecycleLock(updateContext),
            authority,
            new UnitOfWork(updateContext));
        var updating = updater.UpdateOwnedAsync(
            sessionId,
            ownerId,
            new UpdateSessionRequest(
                sessionIncarnationId,
                null,
                moveIntoRoom ? SessionScope.Room : SessionScope.Friends,
                null,
                moveIntoRoom ? roomId.Value : null),
            TestContext.Current.CancellationToken);

        await Task.Delay(50, TestContext.Current.CancellationToken);
        Assert.False(updating.IsCompleted);
        allowRemovalCommit.TrySetResult();
        await removing;

        if (moveIntoRoom)
        {
            await Assert.ThrowsAsync<PolicyViolationException>(() => updating);
            Assert.False(removalObservedSession);
        }
        else
        {
            var updated = await updating;
            Assert.Equal(SessionScope.Friends, updated.Response.Scope);
            Assert.True(removalObservedSession);
        }
    }

    private static async Task AssertSessionKeyGenerationConcurrencyAsync(
        DbContextOptions<KodosiDbContext> options,
        SessionId sessionId,
        UserId ownerId)
    {
        await using var lockingContext = new KodosiDbContext(options);
        await using var claimingContext = new KodosiDbContext(options);
        await using var transaction = await new UnitOfWork(lockingContext)
            .BeginTransactionAsync(TestContext.Current.CancellationToken);
        var lockedSession = await new SessionRepository(lockingContext)
            .GetByIdForUpdateAsync(
                sessionId,
                TestContext.Current.CancellationToken)
            ?? throw new InvalidOperationException("session should exist");
        var claiming = new SessionKeyGenerationClaimer(
            new SessionRepository(claimingContext),
            new UnitOfWork(claimingContext))
            .ClaimNextAsync(
                sessionId,
                ownerId,
                lockedSession.IncarnationId,
                0,
                TestContext.Current.CancellationToken);

        await Task.Delay(50, TestContext.Current.CancellationToken);
        Assert.False(claiming.IsCompleted);
        await transaction.CommitAsync(TestContext.Current.CancellationToken);
        var first = await claiming;
        Assert.Equal(SessionKeyGenerationClaimState.Claimed, first.State);
        Assert.Equal(1, first.Generation);

        await using var firstContext = new KodosiDbContext(options);
        await using var secondContext = new KodosiDbContext(options);
        var firstConcurrent = new SessionKeyGenerationClaimer(
            new SessionRepository(firstContext),
            new UnitOfWork(firstContext))
            .ClaimNextAsync(
                sessionId,
                ownerId,
                lockedSession.IncarnationId,
                1,
                TestContext.Current.CancellationToken);
        var secondConcurrent = new SessionKeyGenerationClaimer(
            new SessionRepository(secondContext),
            new UnitOfWork(secondContext))
            .ClaimNextAsync(
                sessionId,
                ownerId,
                lockedSession.IncarnationId,
                1,
                TestContext.Current.CancellationToken);

        var concurrent = await Task.WhenAll(firstConcurrent, secondConcurrent);
        Assert.Equal(
            [SessionKeyGenerationClaimState.Claimed, SessionKeyGenerationClaimState.GenerationChanged],
            concurrent.Select(result => result.State).Order().ToArray());
        Assert.All(concurrent, result => Assert.Equal(2, result.Generation));

        await using var verifyContext = new KodosiDbContext(options);
        var persisted = await verifyContext.Sessions
            .AsNoTracking()
            .SingleAsync(
                session => session.Id == sessionId,
                TestContext.Current.CancellationToken);
        Assert.Equal(2, persisted.CurrentKeyGeneration);
    }

    private static async Task AssertSessionKeyBlobReplacementRollbackAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId)
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "key-blob-rollback",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret-hash");
        var senderDeviceId = $"key-rollback-{Guid.NewGuid():N}";
        var senderDevice = TestDeviceCertificate.CreateDevice(
            ownerId,
            senderDeviceId,
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Rollback test device",
            signerDeviceId: senderDeviceId,
            issuedAt: DateTimeOffset.UtcNow.AddMinutes(-1),
            expiresAt: null);
        var deviceList = TestDeviceList.Create(
            ownerId,
            1,
            TestDeviceList.Entries([(senderDeviceId, senderDeviceId)]),
            senderDeviceId,
            [2],
            DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds(),
            null);
        var existingBlob = SessionKeyBlob.Create(
            session.Id,
            "existing-recipient",
            [1, 2, 3],
            senderDeviceId,
            senderDevice.KemPublicKey,
            session.CurrentKeyGeneration,
            SessionKeyBlobSignatureDigest.LegacyVersion,
            [4, 5, 6],
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());

        await using (var setupContext = new KodosiDbContext(options))
        {
            setupContext.Sessions.Add(session);
            setupContext.UserDevices.Add(senderDevice);
            setupContext.UserDeviceLists.Add(deviceList);
            setupContext.SessionKeyBlobs.Add(existingBlob);
            await setupContext.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using (var operationContext = new KodosiDbContext(options))
        {
            var repository = new ThrowingAddSessionKeyBlobRepository(
                new SessionKeyBlobRepository(operationContext));
            var service = new SessionKeyDistributionService(
                new SessionRepository(operationContext),
                repository,
                new UserDeviceRepository(operationContext),
                new UserDeviceListRepository(operationContext),
                new AlwaysValidSignatureVerifier(),
                new PostgresUserLifecycleLock(operationContext),
                new PostgresRecipientDeviceLifecycleLock(operationContext),
                new GateOnlySessionEndAuthority(new SessionLifecycleGate()),
                CreateSessionAccessService(operationContext),
                new UnitOfWork(operationContext),
                new IntegrationNoOpSharingMetrics(),
                TimeProvider.System);

            await Assert.ThrowsAsync<InvalidOperationException>(() =>
                service.ReplaceKeyBlobsAsync(
                    session.Id,
                    ownerId,
                    session.IncarnationId,
                    [
                        new SessionKeyBlobSubmission(
                            senderDeviceId,
                            Convert.ToBase64String([7, 8, 9]),
                            senderDeviceId,
                            session.CurrentKeyGeneration,
                            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                            Convert.ToBase64String([10, 11, 12]),
                            SessionKeyBlobSignatureDigest.CurrentVersion)
                    ],
                    TestContext.Current.CancellationToken));
        }

        await using var verifyContext = new KodosiDbContext(options);
        var persisted = await verifyContext.SessionKeyBlobs
            .AsNoTracking()
            .SingleAsync(
                blob => blob.SessionId == session.Id,
                TestContext.Current.CancellationToken);
        Assert.Equal(existingBlob.RecipientDeviceId, persisted.RecipientDeviceId);
        Assert.Equal(existingBlob.EncryptedSessionKey, persisted.EncryptedSessionKey);
    }

    private static async Task AssertSessionKeyPublicationRejectsExpiredRecipientAsync(
        DbContextOptions<KodosiDbContext> options)
    {
        var ownerId = UserId.New();
        var recipientId = UserId.New();
        var ownerSuffix = ownerId.Value.ToString("N");
        var recipientSuffix = recipientId.Value.ToString("N");
        var senderDeviceId = $"expiry-race-sender-{ownerSuffix}";
        var recipientDeviceId = $"expiry-race-recipient-{recipientSuffix}";
        var session = Session.Create(
            SessionId.New(),
            Guid.CreateVersion7(),
            1,
            Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "key-publication-expiry-race",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret-hash");
        var senderDevice = CreateCertifiedDevice(ownerId, senderDeviceId);
        var recipientDevice = CreateCertifiedDevice(recipientId, recipientDeviceId);
        var senderList = TestDeviceList.Create(
            ownerId,
            1,
            TestDeviceList.Entries([(senderDeviceId, senderDeviceId)]),
            senderDeviceId,
            [2],
            DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds(),
            null);
        var recipientList = TestDeviceList.Create(
            recipientId,
            1,
            TestDeviceList.Entries([(recipientDeviceId, recipientDeviceId)]),
            recipientDeviceId,
            [2],
            DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds(),
            null);
        var expiry = DateTimeOffset.UtcNow.AddMinutes(5);
        var accessOverride = SessionAccessOverride.Create(
            session.Id,
            recipientId,
            AccessLevel.View,
            ownerId,
            expiry,
            DateTimeOffset.UtcNow);

        await using (var setup = new KodosiDbContext(options))
        {
            setup.Users.AddRange(
                User.Create(
                    ownerId,
                    $"expiry-owner-{ownerSuffix}@example.test",
                    $"expiry-owner-{ownerSuffix[..10]}",
                    "Expiry Owner"),
                User.Create(
                    recipientId,
                    $"expiry-recipient-{recipientSuffix}@example.test",
                    $"expiry-recipient-{recipientSuffix[..10]}",
                    "Expiry Recipient"));
            setup.Sessions.Add(session);
            setup.UserDevices.AddRange(senderDevice, recipientDevice);
            setup.UserDeviceLists.AddRange(senderList, recipientList);
            setup.SessionAccessOverrides.Add(accessOverride);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var gate = new SessionLifecycleGate();
        var held = await gate.AcquireAsync(
            session.Id,
            TestContext.Current.CancellationToken);
        await using var publicationContext = new KodosiDbContext(options);
        var publishing = new SessionKeyDistributionService(
            new SessionRepository(publicationContext),
            new SessionKeyBlobRepository(publicationContext),
            new UserDeviceRepository(publicationContext),
            new UserDeviceListRepository(publicationContext),
            new AlwaysValidSignatureVerifier(),
            new PostgresUserLifecycleLock(publicationContext),
            new PostgresRecipientDeviceLifecycleLock(publicationContext),
            new GateOnlySessionEndAuthority(gate),
            CreateSessionAccessService(publicationContext),
            new UnitOfWork(publicationContext),
            new IntegrationNoOpSharingMetrics(),
            TimeProvider.System)
            .ReplaceKeyBlobsAsync(
                session.Id,
                ownerId,
                session.IncarnationId,
                [
                    new SessionKeyBlobSubmission(
                        recipientDeviceId,
                        Convert.ToBase64String([7, 8, 9]),
                        senderDeviceId,
                        session.CurrentKeyGeneration,
                        DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                        Convert.ToBase64String([10, 11, 12]),
                        SessionKeyBlobSignatureDigest.CurrentVersion)
                ],
                TestContext.Current.CancellationToken);
        while (gate.TestReferenceCount(session.Id) < 2)
        {
            await Task.Delay(1, TestContext.Current.CancellationToken);
        }

        await using (var expiryContext = new KodosiDbContext(options))
        {
            Assert.True(await new AccessOverrideRepository(
                    expiryContext,
                    TimeProvider.System)
                .TryRevokeExpiredAsync(
                    session.Id,
                    recipientId,
                    expiry,
                    expiry,
                    TestContext.Current.CancellationToken));
        }
        await held.DisposeAsync();

        var result = await publishing;
        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Contains("not authorized", result.ErrorMessage, StringComparison.Ordinal);

        await using var verify = new KodosiDbContext(options);
        Assert.False(await verify.SessionKeyBlobs
            .AsNoTracking()
            .AnyAsync(
                blob => blob.SessionId == session.Id,
                TestContext.Current.CancellationToken));
    }

    private static async Task AssertSessionKeyBlobReplacementWaitsForRevocationAsync(
        DbContextOptions<KodosiDbContext> options)
    {
        var ownerId = UserId.New();
        var ownerSuffix = ownerId.Value.ToString("N");
        var senderDeviceId = $"race-sender-{ownerSuffix}";
        var revokerDeviceId = $"race-revoker-{ownerSuffix}";
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "key-blob-revocation-race",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret-hash");
        var senderDevice = CreateCertifiedDevice(ownerId, senderDeviceId);
        var revokerDevice = CreateCertifiedDevice(ownerId, revokerDeviceId);
        var initialDeviceList = TestDeviceList.Create(
            ownerId,
            1,
            TestDeviceList.Entries(
                [
                    (senderDeviceId, revokerDeviceId),
                    (revokerDeviceId, revokerDeviceId),
                ]),
            revokerDeviceId,
            [2],
            DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds(),
            null);
        var existingBlob = SessionKeyBlob.Create(
            session.Id,
            "existing-recipient",
            [1, 2, 3],
            senderDeviceId,
            senderDevice.KemPublicKey,
            session.CurrentKeyGeneration,
            SessionKeyBlobSignatureDigest.LegacyVersion,
            [4, 5, 6],
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());

        await using (var setupContext = new KodosiDbContext(options))
        {
            setupContext.Users.Add(User.Create(
                ownerId,
                $"{ownerSuffix}@example.test",
                $"race-{ownerSuffix}",
                "Race Owner"));
            setupContext.Sessions.Add(session);
            setupContext.UserDevices.AddRange(senderDevice, revokerDevice);
            setupContext.UserDeviceLists.Add(initialDeviceList);
            setupContext.SessionKeyBlobs.Add(existingBlob);
            await setupContext.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using var revocationContext = new KodosiDbContext(options);
        var revocationUnitOfWork = new UnitOfWork(revocationContext);
        await using var revocationTransaction = await revocationUnitOfWork
            .BeginTransactionAsync(TestContext.Current.CancellationToken);
        await new PostgresUserLifecycleLock(revocationContext).AcquireAsync(
            ownerId,
            TestContext.Current.CancellationToken);
        var revocationDevices = new UserDeviceRepository(revocationContext);
        var persistedSender = await revocationDevices.GetByDeviceIdAsync(
            senderDeviceId,
            TestContext.Current.CancellationToken);
        Assert.NotNull(persistedSender);
        persistedSender!.Revoke(revokerDeviceId);
        revocationDevices.Update(persistedSender);
        var revocationLists = new UserDeviceListRepository(revocationContext);
        await revocationLists.AddAsync(
            TestDeviceList.Create(
                ownerId,
                2,
                TestDeviceList.Entries([(revokerDeviceId, revokerDeviceId)]),
                revokerDeviceId,
                [4],
                initialDeviceList.ParseBody().IssuedAtMs + 1,
                null),
            TestContext.Current.CancellationToken);
        await revocationUnitOfWork.SaveChangesAsync(TestContext.Current.CancellationToken);

        await using var replacementContext = new KodosiDbContext(options);
        var lifecycleLockAttempted = new TaskCompletionSource<bool>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var service = new SessionKeyDistributionService(
            new SessionRepository(replacementContext),
            new SessionKeyBlobRepository(replacementContext),
            new UserDeviceRepository(replacementContext),
            new UserDeviceListRepository(replacementContext),
            new AlwaysValidSignatureVerifier(),
            new SignalingUserLifecycleLock(
                new PostgresUserLifecycleLock(replacementContext),
                lifecycleLockAttempted),
            new PostgresRecipientDeviceLifecycleLock(replacementContext),
            new GateOnlySessionEndAuthority(new SessionLifecycleGate()),
            CreateSessionAccessService(replacementContext),
            new UnitOfWork(replacementContext),
            new IntegrationNoOpSharingMetrics(),
            TimeProvider.System);
        var replacement = service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    revokerDeviceId,
                    Convert.ToBase64String([7, 8, 9]),
                    senderDeviceId,
                    session.CurrentKeyGeneration,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([10, 11, 12]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        await lifecycleLockAttempted.Task.WaitAsync(
            TimeSpan.FromSeconds(5),
            TestContext.Current.CancellationToken);
        Assert.False(replacement.IsCompleted);
        await revocationTransaction.CommitAsync(TestContext.Current.CancellationToken);

        var result = await replacement;
        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Equal($"Sender device {senderDeviceId} is not active.", result.ErrorMessage);

        await using var verifyContext = new KodosiDbContext(options);
        var persistedBlob = await verifyContext.SessionKeyBlobs
            .AsNoTracking()
            .SingleAsync(
                blob => blob.SessionId == session.Id,
                TestContext.Current.CancellationToken);
        Assert.Equal(existingBlob.RecipientDeviceId, persistedBlob.RecipientDeviceId);
        Assert.True(await verifyContext.UserDevices
            .AsNoTracking()
            .Where(device =>
                device.UserId == ownerId
                && device.DeviceId == senderDeviceId)
            .Select(device => device.RevokedAt != null)
            .SingleAsync(TestContext.Current.CancellationToken));
    }

    private static async Task AssertCommittedDeviceRevocationGatesParticipantDispatchAsync(
        DbContextOptions<KodosiDbContext> options)
    {
        var ownerId = UserId.New();
        var ownerSuffix = ownerId.Value.ToString("N");
        var revokedDeviceId = $"dispatch-revoked-{ownerSuffix}";
        var revokerDeviceId = $"dispatch-revoker-{ownerSuffix}";
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "committed-revocation-dispatch",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            "secret-hash");
        var revokedDevice = CreateCertifiedDevice(ownerId, revokedDeviceId);
        var revokerDevice = CreateCertifiedDevice(ownerId, revokerDeviceId);
        var initialDeviceList = TestDeviceList.Create(
            ownerId,
            1,
            TestDeviceList.Entries(
                [
                    (revokedDeviceId, revokerDeviceId),
                    (revokerDeviceId, revokerDeviceId),
                ]),
            revokerDeviceId,
            [2],
            DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds(),
            null);
        var blob = SessionKeyBlob.Create(
            session.Id,
            revokedDeviceId,
            [1, 2, 3],
            revokerDeviceId,
            revokerDevice.KemPublicKey,
            session.CurrentKeyGeneration,
            SessionKeyBlobSignatureDigest.LegacyVersion,
            [4, 5, 6],
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());
        await using (var setupContext = new KodosiDbContext(options))
        {
            setupContext.Users.Add(User.Create(
                ownerId,
                $"{ownerSuffix}@example.test",
                $"dispatch-{ownerSuffix}",
                "Dispatch Owner"));
            setupContext.Sessions.Add(session);
            setupContext.UserDevices.AddRange(revokedDevice, revokerDevice);
            setupContext.UserDeviceLists.Add(initialDeviceList);
            setupContext.SessionKeyBlobs.Add(blob);
            await setupContext.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var connections = new ConnectionRegistry();
        var runtime = runtimes.CreateRuntime(session.Id);
        runtime.Host.SetSessionIncarnationId(session.IncarnationId);
        runtime.Host.SetStatus(SessionStatus.Live);
        runtime.Host.SetHostReady(true);
        var queues = broadcaster.GetOrCreateSession(session.Id);
        queues.SetHostQueue("host");
        var participantQueue = queues.AddParticipantQueue(
            "revoked-participant",
            AccessLevel.Suggest);
        connections.RegisterSharedParticipant(
            "revoked-participant",
            ownerId,
            revokedDeviceId,
            session.Id);
        var lifecycleGate = new SessionLifecycleGate();
        var authority = new GateOnlySessionEndAuthority(lifecycleGate);
        var committed = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var allowInvalidation = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);

        await using var revocationContext = new KodosiDbContext(options);
        var unitOfWork = new UnitOfWork(revocationContext);
        await using var transaction = await unitOfWork.BeginTransactionAsync(
            TestContext.Current.CancellationToken);
        await new PostgresUserLifecycleLock(revocationContext).AcquireAsync(
            ownerId,
            TestContext.Current.CancellationToken);
        var devices = new UserDeviceRepository(revocationContext);
        var persistedDevice = await devices.GetByDeviceIdAsync(
            revokedDeviceId,
            TestContext.Current.CancellationToken);
        Assert.NotNull(persistedDevice);
        persistedDevice!.Revoke(revokerDeviceId);
        devices.Update(persistedDevice);
        await new UserDeviceListRepository(revocationContext).AddAsync(
            TestDeviceList.Create(
                ownerId,
                2,
                TestDeviceList.Entries(
                    [(revokerDeviceId, revokerDeviceId)]),
                revokerDeviceId,
                [4],
                initialDeviceList.ParseBody().IssuedAtMs + 1,
                null),
            TestContext.Current.CancellationToken);
        var blobs = new SessionKeyBlobRepository(revocationContext);
        var affectedSessions = await blobs.GetSessionTargetsForRecipientDevicesAsync(
            [revokedDeviceId],
            TestContext.Current.CancellationToken);

        var effects = DeviceListRealtimeEffectsFactory.Create(
            connections,
            broadcaster,
            new UserEventBroadcaster(metrics, NullLogger<UserEventBroadcaster>.Instance),
            authority,
            new SessionIncarnationResolver(
                new IntegrationDbContextFactory(options)),
            runtimes,
            NullLogger<DeviceListRealtimeEffects>.Instance);
        var enforcing = effects.PersistAndEnforceAsync(
                ownerId,
                [revokedDeviceId],
                affectedSessions,
                async ct =>
                {
                    await blobs.DeleteForRecipientDevicesAsync(
                        [revokedDeviceId],
                        ct);
                    await unitOfWork.SaveChangesAsync(ct);
                    await transaction.CommitAsync(ct);
                    committed.TrySetResult();
                    await allowInvalidation.Task.WaitAsync(ct);
                },
                TestContext.Current.CancellationToken);
        await committed.Task.WaitAsync(TestContext.Current.CancellationToken);

        var dispatchCalls = 0;
        var actionAuthority = new ParticipantActionAuthority(
            session.Id,
            "revoked-participant",
            ownerId,
            revokedDeviceId,
            new ParticipantAccessDecision(
                IsOwnerParticipant: false,
                AccessLevel: AccessLevel.Suggest,
                SessionStartedAt: session.StartedAt),
            runtime,
            queues,
            participantQueue,
            runtimes,
            connections,
            broadcaster,
            lifecycleGate);
        var dispatching = actionAuthority.TryDispatchAsync(
                SessionCapability.Suggest,
                () =>
                {
                    Interlocked.Increment(ref dispatchCalls);
                    return broadcaster.SendToHost(session.Id, [1]);
                },
                TestContext.Current.CancellationToken)
            .AsTask();
        using (var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5)))
        {
            while (lifecycleGate.TestReferenceCount(session.Id) < 2
                && !dispatching.IsCompleted)
            {
                await Task.Delay(1, timeout.Token);
            }
        }
        Assert.False(dispatching.IsCompleted);

        allowInvalidation.TrySetResult();
        await enforcing;
        var dispatch = await dispatching;

        Assert.False(dispatch.Authorized);
        Assert.Equal(0, dispatchCalls);
        Assert.Equal(
            QueueCompletionCause.AccessRevokedCascade,
            participantQueue.CompletionCause);
        await using var verifyContext = new KodosiDbContext(options);
        Assert.True(await verifyContext.UserDevices
            .AsNoTracking()
            .Where(device =>
                device.UserId == ownerId
                && device.DeviceId == revokedDeviceId)
            .Select(device => device.RevokedAt != null)
            .SingleAsync(TestContext.Current.CancellationToken));
        Assert.False(await verifyContext.SessionKeyBlobs
            .AsNoTracking()
            .AnyAsync(
                candidate => candidate.RecipientDeviceId == revokedDeviceId,
                TestContext.Current.CancellationToken));
    }

    private static async Task AssertNonRoomSessionCreateRetryNormalizesRoomIdAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId)
    {
        var request = new CreateSessionRequest(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            "postgres-non-room-idempotency",
            SessionScope.MyDevices,
            ToolKind.Terminal,
            DefaultAudienceAccess.View,
            "owner-secret",
            Guid.CreateVersion7());

        OwnedSessionCreateResult first;
        await using (var firstContext = new KodosiDbContext(options))
        {
            first = await CreateSessionCreator(
                    firstContext,
                    new GateOnlySessionEndAuthority(
                        new SessionLifecycleGate()))
                .CreateOwnedAsync(
                    ownerId,
                    request,
                    TestContext.Current.CancellationToken);
        }

        OwnedSessionCreateResult retry;
        await using (var retryContext = new KodosiDbContext(options))
        {
            retry = await CreateSessionCreator(
                    retryContext,
                    new GateOnlySessionEndAuthority(
                        new SessionLifecycleGate()))
                .CreateOwnedAsync(
                    ownerId,
                    request,
                    TestContext.Current.CancellationToken);
        }

        Assert.Null(first.Response.RoomId);
        Assert.Equal(first.Response, retry.Response);
        await using var verifyContext = new KodosiDbContext(options);
        var persisted = await verifyContext.Sessions
            .AsNoTracking()
            .SingleAsync(
                candidate => candidate.Id == SessionId.From(request.Id),
                TestContext.Current.CancellationToken);
        Assert.Null(persisted.RoomId);
    }

    private static UserDevice CreateCertifiedDevice(UserId userId, string deviceId)
    {
        var device = TestDeviceCertificate.CreateDevice(
            userId,
            deviceId,
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Postgres test device",
            signerDeviceId: deviceId,
            issuedAt: DateTimeOffset.UtcNow.AddMinutes(-1),
            expiresAt: null);
        return device;
    }

    private static async Task AssertSessionKeyGenerationExhaustionAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId)
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "exhausted-session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret-hash");
        typeof(Session)
            .GetProperty(nameof(Session.CurrentKeyGeneration))!
            .SetValue(session, int.MaxValue);
        await using (var setupContext = new KodosiDbContext(options))
        {
            setupContext.Sessions.Add(session);
            await setupContext.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using var claimContext = new KodosiDbContext(options);
        var result = await new SessionKeyGenerationClaimer(
            new SessionRepository(claimContext),
            new UnitOfWork(claimContext))
            .ClaimNextAsync(
                session.Id,
                ownerId,
                session.IncarnationId,
                session.CurrentKeyGeneration,
                TestContext.Current.CancellationToken);

        Assert.Equal(SessionKeyGenerationClaimState.Exhausted, result.State);
        await using var verifyContext = new KodosiDbContext(options);
        var persisted = await verifyContext.Sessions
            .AsNoTracking()
            .SingleAsync(
                candidate => candidate.Id == session.Id,
                TestContext.Current.CancellationToken);
        Assert.Equal(int.MaxValue, persisted.CurrentKeyGeneration);
    }

    private static async Task AssertSessionEndClaimSerializationAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId)
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "end-claim-session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret-hash");
        await using (var setupContext = new KodosiDbContext(options))
        {
            setupContext.Sessions.Add(session);
            await setupContext.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using var lockingContext = new KodosiDbContext(options);
        await using var endContext = new KodosiDbContext(options);
        await using var claimContext = new KodosiDbContext(options);
        await using var transaction = await new UnitOfWork(lockingContext)
            .BeginTransactionAsync(TestContext.Current.CancellationToken);
        _ = await new SessionRepository(lockingContext)
            .GetByIdForUpdateAsync(
                session.Id,
                TestContext.Current.CancellationToken);

        var ending = new LiveSessionTerminator(
            new SessionRepository(endContext),
            new SessionKeyBlobRepository(endContext),
            new UnitOfWork(endContext),
            new SessionEndMutationRepository(endContext))
            .EndOwnedSessionIdempotentlyAsync(
                session.Id,
                ownerId,
                session.IncarnationId,
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
                TestContext.Current.CancellationToken);
        await Task.Delay(50, TestContext.Current.CancellationToken);
        Assert.False(ending.IsCompleted);

        var claiming = new SessionKeyGenerationClaimer(
            new SessionRepository(claimContext),
            new UnitOfWork(claimContext))
            .ClaimNextAsync(
                session.Id,
                ownerId,
                session.IncarnationId,
                session.CurrentKeyGeneration,
                TestContext.Current.CancellationToken);
        await Task.Delay(50, TestContext.Current.CancellationToken);
        Assert.False(claiming.IsCompleted);

        await transaction.CommitAsync(TestContext.Current.CancellationToken);
        var endResult = await ending;
        var claimResult = await claiming;

        Assert.Equal(SessionTransitionOutcome.Applied, endResult.Outcome);
        Assert.Equal(SessionKeyGenerationClaimState.Ended, claimResult.State);
        await using var verifyContext = new KodosiDbContext(options);
        var persisted = await verifyContext.Sessions
            .AsNoTracking()
            .SingleAsync(
                candidate => candidate.Id == session.Id,
                TestContext.Current.CancellationToken);
        Assert.Equal(SessionStatus.Ended, persisted.Status);
        Assert.Equal(0, persisted.CurrentKeyGeneration);
    }

    private static async Task AssertSessionEndMutationDurabilityAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId)
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "idempotent-end-session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret-hash");
        var recipientDeviceId = $"end-receipt-{Guid.NewGuid():N}";
        var blob = SessionKeyBlob.Create(
            session.Id,
            recipientDeviceId,
            [1, 2, 3],
            "sender-device",
            [4, 5, 6],
            1,
            SessionKeyBlobSignatureDigest.LegacyVersion,
            [7, 8, 9],
            issuedAtMs: 1_000);
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Sessions.Add(session);
            setup.SessionKeyBlobs.Add(blob);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var mutationId = Guid.CreateVersion7();
        var firstAttemptId = Guid.CreateVersion7();
        var competingAttemptId = Guid.CreateVersion7();
        var ends = new[] { firstAttemptId, competingAttemptId }
            .Select(async attemptId =>
            {
                await using var context = new KodosiDbContext(options);
                return await CreateIdempotentTerminator(context)
                    .EndOwnedSessionIdempotentlyAsync(
                        session.Id,
                        ownerId,
                        session.IncarnationId,
                        mutationId,
                        attemptId,
                        TestContext.Current.CancellationToken);
            })
            .ToArray();
        var results = await Task.WhenAll(ends);

        Assert.Single(
            results,
            result => result.Outcome == SessionTransitionOutcome.Applied);
        Assert.Single(
            results,
            result => result.Outcome
                == SessionTransitionOutcome.AlreadyInTargetState);
        await using (var verify = new KodosiDbContext(options))
        {
            var receipt = await verify.SessionEndMutations
                .AsNoTracking()
                .SingleAsync(
                    candidate => candidate.OwnerUserId == ownerId
                        && candidate.MutationId == mutationId,
                    TestContext.Current.CancellationToken);
            Assert.Contains(
                receipt.FirstAttemptId,
                new[] { firstAttemptId, competingAttemptId });
            Assert.Equal(session.Id, receipt.SessionId);
            Assert.Equal(session.IncarnationId, receipt.IncarnationId);
            Assert.Equal(
                SessionStatus.Ended,
                await verify.Sessions
                    .Where(candidate => candidate.Id == session.Id)
                    .Select(candidate => candidate.Status)
                    .SingleAsync(TestContext.Current.CancellationToken));
            Assert.False(await verify.SessionKeyBlobs.AnyAsync(
                candidate => candidate.SessionId == session.Id,
                TestContext.Current.CancellationToken));
        }

        await using (var mismatch = new KodosiDbContext(options))
        {
            var conflict = await Assert.ThrowsAsync<SessionEndMutationTargetConflictException>(() =>
                CreateIdempotentTerminator(mismatch)
                    .EndOwnedSessionIdempotentlyAsync(
                        session.Id,
                        ownerId,
                        Guid.CreateVersion7(),
                        mutationId,
                        Guid.CreateVersion7(),
                        TestContext.Current.CancellationToken));
            Assert.Equal("SESSION_END_MUTATION_TARGET_CONFLICT", conflict.Code);
        }

        var rollbackSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "rollback-end-session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret-hash");
        var rollbackBlob = SessionKeyBlob.Create(
            rollbackSession.Id,
            $"rollback-end-{Guid.NewGuid():N}",
            [1, 2, 3],
            "sender-device",
            [4, 5, 6],
            1,
            SessionKeyBlobSignatureDigest.LegacyVersion,
            [7, 8, 9],
            issuedAtMs: 1_000);
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Sessions.Add(rollbackSession);
            setup.SessionKeyBlobs.Add(rollbackBlob);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        var rollbackMutationId = Guid.CreateVersion7();
        await using (var rollback = new KodosiDbContext(options))
        {
            var inner = new UnitOfWork(rollback);
            var faulting = new FaultInjectingUnitOfWork(
                inner,
                CommitFaultMode.BeforeCommit);
            var terminator = new LiveSessionTerminator(
                new SessionRepository(rollback),
                new SessionKeyBlobRepository(rollback),
                faulting,
                new SessionEndMutationRepository(rollback));
            await Assert.ThrowsAsync<InvalidOperationException>(() =>
                terminator.EndOwnedSessionIdempotentlyAsync(
                    rollbackSession.Id,
                    ownerId,
                    rollbackSession.IncarnationId,
                    rollbackMutationId,
                    Guid.CreateVersion7(),
                    TestContext.Current.CancellationToken));
        }

        await using (var verify = new KodosiDbContext(options))
        {
            Assert.Equal(
                SessionStatus.Pending,
                await verify.Sessions
                    .Where(candidate => candidate.Id == rollbackSession.Id)
                    .Select(candidate => candidate.Status)
                    .SingleAsync(TestContext.Current.CancellationToken));
            Assert.True(await verify.SessionKeyBlobs.AnyAsync(
                candidate => candidate.SessionId == rollbackSession.Id,
                TestContext.Current.CancellationToken));
            Assert.False(await verify.SessionEndMutations.AnyAsync(
                candidate => candidate.OwnerUserId == ownerId
                    && candidate.MutationId == rollbackMutationId,
                TestContext.Current.CancellationToken));
        }
    }

    private static LiveSessionTerminator CreateIdempotentTerminator(
        KodosiDbContext context)
    {
        return new LiveSessionTerminator(
            new SessionRepository(context),
            new SessionKeyBlobRepository(context),
            new UnitOfWork(context),
            new SessionEndMutationRepository(context));
    }

    private static async Task AssertHostActivationRejectsOldSecretAfterRepublishAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId)
    {
        var hasher = new OwnerSessionSecretHasher();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "host-secret-incarnation",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            hasher.Hash("old-secret"));
        session.End();
        await Task.Delay(1, TestContext.Current.CancellationToken);
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            ownerId,
            "republished-host-secret",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            hasher.Hash("new-secret"),
            roomId: null);
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Sessions.Add(session);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using (var oldSecretContext = new KodosiDbContext(options))
        {
            var unitOfWork = new UnitOfWork(oldSecretContext);
            await using var transaction = await unitOfWork.BeginTransactionAsync(
                TestContext.Current.CancellationToken);
            var result = await new HostSessionActivator(
                new SessionRepository(oldSecretContext),
                hasher,
                unitOfWork)
                .ActivateInsideExistingTransactionAsync(
                    session.Id,
                    ownerId,
                    "old-secret",
                    session.IncarnationId,
                    "old-secret-host",
                    static () => true,
                    TestContext.Current.CancellationToken);
            Assert.Equal(
                HostSessionActivationOutcome.InvalidSessionOrSecret,
                result.Outcome);
        }

        await using (var newSecretContext = new KodosiDbContext(options))
        {
            var unitOfWork = new UnitOfWork(newSecretContext);
            await using var transaction = await unitOfWork.BeginTransactionAsync(
                TestContext.Current.CancellationToken);
            var result = await new HostSessionActivator(
                new SessionRepository(newSecretContext),
                hasher,
                unitOfWork)
                .ActivateInsideExistingTransactionAsync(
                    session.Id,
                    ownerId,
                    "new-secret",
                    session.IncarnationId,
                    "new-secret-host",
                    static () => true,
                    TestContext.Current.CancellationToken);
            Assert.Equal(HostSessionActivationOutcome.Applied, result.Outcome);
            Assert.Equal(session.StartedAt, result.SessionStartedAt);
            await transaction.CommitAsync(TestContext.Current.CancellationToken);
        }
    }

    private static async Task AssertParticipantCountIncarnationAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId)
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "participant-incarnation",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret-hash");
        var oldStartedAt = session.StartedAt;
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Sessions.Add(session);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using (var oldContext = new KodosiDbContext(options))
        {
            var oldRepository = new SessionRepository(oldContext);
            Assert.True(await oldRepository.TryIncrementParticipantCountAsync(
                session.Id,
                oldStartedAt,
                50,
                TestContext.Current.CancellationToken));
            Assert.True(await oldRepository.TryClearParticipantCountAsync(
                session.Id,
                oldStartedAt,
                TestContext.Current.CancellationToken));
            Assert.True(await oldRepository.TryIncrementParticipantCountAsync(
                session.Id,
                oldStartedAt,
                50,
                TestContext.Current.CancellationToken));
        }

        await using (var republishContext = new KodosiDbContext(options))
        {
            var persisted = await republishContext.Sessions.SingleAsync(
                candidate => candidate.Id == session.Id,
                TestContext.Current.CancellationToken);
            persisted.End();
            await Task.Delay(1, TestContext.Current.CancellationToken);
            persisted.Republish(Guid.CreateVersion7(), checked(persisted.IncarnationGeneration + 1),
                ownerId,
                "republished-participant-incarnation",
                SessionScope.Friends,
                ToolKind.Terminal,
                AccessLevel.View,
                "new-secret",
                roomId: null);
            await republishContext.SaveChangesAsync(
                TestContext.Current.CancellationToken);
            session = persisted;
        }

        await using var verify = new KodosiDbContext(options);
        var repository = new SessionRepository(verify);
        Assert.True(await repository.TrySetParticipantCountAsync(
            session.Id,
            session.StartedAt,
            50,
            TestContext.Current.CancellationToken));
        Assert.False(await repository.TryDecrementParticipantCountAsync(
            session.Id,
            oldStartedAt,
            TestContext.Current.CancellationToken));
        Assert.False(await repository.TryClearParticipantCountAsync(
            session.Id,
            oldStartedAt,
            TestContext.Current.CancellationToken));
        Assert.False(await repository.TryIncrementParticipantCountAsync(
            session.Id,
            session.StartedAt,
            50,
            TestContext.Current.CancellationToken));
        Assert.Equal(
            50,
            await verify.Sessions
                .Where(candidate => candidate.Id == session.Id)
                .Select(candidate => candidate.LiveParticipantCount)
                .SingleAsync(TestContext.Current.CancellationToken));
    }

    private static async Task AssertOrphanMarkerRevalidationAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId)
    {
        var releasedSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
                ownerId,
                "orphan-release-marker",
                SessionScope.Friends,
                ToolKind.Terminal,
                AccessLevel.View,
                "secret-hash");
        releasedSession.ActivateHost("old-host");
        releasedSession.ReleaseHostSlot("old-host");
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Sessions.Add(releasedSession);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        OrphanedSessionCandidate releasedCandidate;
        await using (var query = new KodosiDbContext(options))
        {
            releasedCandidate = Assert.Single(
                await new SessionRepository(query).GetOrphanedLiveSessionsAsync(
                    DateTimeOffset.UtcNow.AddMinutes(1),
                    TestContext.Current.CancellationToken),
                candidate => candidate.SessionId == releasedSession.Id);
        }

        await Task.Delay(1, TestContext.Current.CancellationToken);
        await using (var reconnect = new KodosiDbContext(options))
        {
            var persisted = await reconnect.Sessions.SingleAsync(
                session => session.Id == releasedSession.Id,
                TestContext.Current.CancellationToken);
            persisted.ActivateHost("replacement-host");
            persisted.ReleaseHostSlot("replacement-host");
            await reconnect.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using (var end = new KodosiDbContext(options))
        {
            var result = await new LiveSessionTerminator(
                new SessionRepository(end),
                new SessionKeyBlobRepository(end),
                new UnitOfWork(end),
                new SessionEndMutationRepository(end))
                .EndOrphanedSessionAsync(
                    releasedCandidate,
                    TestContext.Current.CancellationToken);
            Assert.Equal(SessionTransitionOutcome.Rejected, result.Outcome);
        }

        var heartbeatSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
                ownerId,
                "orphan-heartbeat-marker",
                SessionScope.Friends,
                ToolKind.Terminal,
                AccessLevel.View,
                "secret-hash");
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Sessions.Add(heartbeatSession);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        OrphanedSessionCandidate heartbeatCandidate;
        await using (var query = new KodosiDbContext(options))
        {
            heartbeatCandidate = Assert.Single(
                await new SessionRepository(query).GetOrphanedLiveSessionsAsync(
                    DateTimeOffset.UtcNow.AddMinutes(1),
                    TestContext.Current.CancellationToken),
                candidate => candidate.SessionId == heartbeatSession.Id);
        }

        await Task.Delay(1, TestContext.Current.CancellationToken);
        await using (var heartbeat = new KodosiDbContext(options))
        {
            var persisted = await heartbeat.Sessions.SingleAsync(
                session => session.Id == heartbeatSession.Id,
                TestContext.Current.CancellationToken);
            persisted.RecordHostHeartbeat();
            await heartbeat.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using (var end = new KodosiDbContext(options))
        {
            var result = await new LiveSessionTerminator(
                new SessionRepository(end),
                new SessionKeyBlobRepository(end),
                new UnitOfWork(end),
                new SessionEndMutationRepository(end))
                .EndOrphanedSessionAsync(
                    heartbeatCandidate,
                    TestContext.Current.CancellationToken);
            Assert.Equal(SessionTransitionOutcome.Rejected, result.Outcome);
        }

        await using var verify = new KodosiDbContext(options);
        Assert.All(
                await verify.Sessions
                    .Where(session =>
                        session.Id == releasedSession.Id
                        || session.Id == heartbeatSession.Id)
                    .ToListAsync(TestContext.Current.CancellationToken),
                session => Assert.NotEqual(SessionStatus.Ended, session.Status));
    }

    private static async Task AssertIdentityResetRecipientSessionsCapturedAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId)
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "identity-reset-recipient-session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret-hash");
        var recipientDeviceId = $"reset-device-{Guid.NewGuid():N}";
        var blob = SessionKeyBlob.Create(
            session.Id,
            recipientDeviceId,
            [1, 2, 3],
            "sender-device",
            [4, 5, 6],
            1,
            SessionKeyBlobSignatureDigest.LegacyVersion,
            [7, 8, 9],
            issuedAtMs: 1_000);
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Sessions.Add(session);
            setup.SessionKeyBlobs.Add(blob);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using var context = new KodosiDbContext(options);
        var repository = new SessionKeyBlobRepository(context);
        var affected = await repository.GetSessionIdsForRecipientDevicesAsync(
            [recipientDeviceId],
            TestContext.Current.CancellationToken);
        var deleted = await repository.DeleteForRecipientDevicesAsync(
            [recipientDeviceId],
            TestContext.Current.CancellationToken);

        Assert.Equal([session.Id], affected);
        Assert.Equal(1, deleted);
        Assert.False(await context.SessionKeyBlobs.AnyAsync(
            candidate => candidate.SessionId == session.Id,
            TestContext.Current.CancellationToken));
    }

    private static async Task AssertIdentityResetCommitFaultAsync(
        DbContextOptions<KodosiDbContext> options,
        CommitFaultMode faultMode)
    {
        var userId = UserId.New();
        var suffix = userId.Value.ToString("N");
        var deviceId = $"reset-commit-{suffix}";
        var device = CreateCertifiedDevice(userId, deviceId);
        var issuedAtMs = DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds();
        var deviceList = TestDeviceList.Create(
            userId,
            1,
            TestDeviceList.Entries([(deviceId, deviceId)]),
            deviceId,
            [2],
            issuedAtMs,
            null);
        var challenge = DeviceRegistrationChallenge.Create(
            userId,
            Enumerable.Range(0, 32).Select(index => (byte)index).ToArray(),
            TimeSpan.FromMinutes(5));
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Users.Add(User.Create(
                userId,
                $"reset-{suffix}@example.test",
                $"reset-{suffix[..12]}",
                "Reset Commit"));
            setup.UserDevices.Add(device);
            setup.UserDeviceLists.Add(deviceList);
            setup.DeviceRegistrationChallenges.Add(challenge);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var contextFactory = new IntegrationDbContextFactory(options);
        var authority = new RecordingResetSessionEndAuthority();
        var enforcer = new RecordingResetRealtimeEnforcer(authority);
        var durability = new RecordingResetDurabilityCoordinator(
            new IdentityResetDurabilityCoordinator(contextFactory),
            authority,
            enforcer);
        await using (var operationContext = new KodosiDbContext(options))
        {
            var unitOfWork = new FaultInjectingUnitOfWork(
                new UnitOfWork(operationContext),
                faultMode);
            var service = CreateIdentityResetService(
                operationContext,
                unitOfWork,
                durability,
                authority,
                enforcer);
            var reset = service.ResetIdentityAsync(
                userId,
                new IdentityResetPopPayload(
                    challenge.Id,
                    deviceId,
                    Convert.ToBase64String([1, 2, 3])),
                new IdentityResetAuditContext("127.0.0.1", "postgres-test"),
                TestContext.Current.CancellationToken);

            if (faultMode == CommitFaultMode.BeforeCommit)
            {
                var exception = await Assert.ThrowsAsync<InvalidOperationException>(
                    () => reset);
                Assert.Contains(
                    "before commit",
                    exception.Message,
                    StringComparison.Ordinal);
            }
            else
            {
                var result = await reset;
                Assert.Equal(1, result.DevicesRemoved);
            }
        }

        await using var verify = new KodosiDbContext(options);
        var audit = await verify.IdentityResetAuditEntries
            .AsNoTracking()
            .SingleOrDefaultAsync(
                entry => entry.UserId == userId,
                TestContext.Current.CancellationToken);
        var persistedDevice = await verify.UserDevices
            .AsNoTracking()
            .SingleOrDefaultAsync(
                candidate => candidate.UserId == userId,
                TestContext.Current.CancellationToken);
        var persistedChallenge = await verify.DeviceRegistrationChallenges
            .AsNoTracking()
            .SingleOrDefaultAsync(
                candidate => candidate.Id == challenge.Id,
                TestContext.Current.CancellationToken);
        Assert.Equal(1, durability.ReconciliationCount);
        Assert.True(durability.ReconciledWithFence);
        Assert.True(durability.ReconciledWithSessionLease);

        if (faultMode == CommitFaultMode.BeforeCommit)
        {
            Assert.Null(audit);
            Assert.NotNull(persistedDevice);
            Assert.NotNull(persistedChallenge);
            Assert.Equal(0, enforcer.EnforcementCount);
        }
        else
        {
            Assert.NotNull(audit);
            Assert.Null(persistedDevice);
            Assert.Null(persistedChallenge);
            Assert.Equal([deviceId], audit!.RemovedDeviceIds);
            Assert.NotNull(audit.RealtimeEnforcedAt);
            Assert.Equal(1, enforcer.EnforcementCount);
            Assert.True(enforcer.EnforcedWithFence);
            Assert.True(enforcer.EnforcedWithSessionLease);
        }
    }

    private static async Task AssertIdentityResetFencesStaleKeyPublicationAsync(
        DbContextOptions<KodosiDbContext> options)
    {
        var resetUserId = UserId.New();
        var foreignOwnerId = UserId.New();
        var resetSuffix = resetUserId.Value.ToString("N");
        var foreignSuffix = foreignOwnerId.Value.ToString("N");
        var removedDeviceId = $"reset-race-{resetSuffix}";
        var senderDeviceId = $"reset-race-sender-{foreignSuffix}";
        var removedDevice = CreateCertifiedDevice(resetUserId, removedDeviceId);
        var senderDevice = CreateCertifiedDevice(foreignOwnerId, senderDeviceId);
        var challenge = DeviceRegistrationChallenge.Create(
            resetUserId,
            Enumerable.Repeat((byte)7, 32).ToArray(),
            TimeSpan.FromMinutes(5));
        var foreignSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            foreignOwnerId,
            "identity-reset-key-publication-race",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret-hash");
        var senderList = TestDeviceList.Create(
            foreignOwnerId,
            1,
            TestDeviceList.Entries([(senderDeviceId, senderDeviceId)]),
            senderDeviceId,
            [2],
            DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds(),
            null);
        var resetList = TestDeviceList.Create(
            resetUserId,
            1,
            TestDeviceList.Entries(
                [(removedDeviceId, removedDeviceId)]),
            removedDeviceId,
            [2],
            DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds(),
            null);
        var existingBlob = SessionKeyBlob.Create(
            foreignSession.Id,
            removedDeviceId,
            [1, 2, 3],
            senderDeviceId,
            senderDevice.KemPublicKey,
            foreignSession.CurrentKeyGeneration,
            SessionKeyBlobSignatureDigest.LegacyVersion,
            [4, 5, 6],
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Users.AddRange(
                User.Create(
                    resetUserId,
                    $"reset-race-{resetSuffix}@example.test",
                    $"reset-race-{resetSuffix[..10]}",
                    "Reset Race"),
                User.Create(
                    foreignOwnerId,
                    $"foreign-race-{foreignSuffix}@example.test",
                    $"foreign-race-{foreignSuffix[..10]}",
                    "Foreign Race"));
            setup.UserDevices.AddRange(removedDevice, senderDevice);
            setup.UserDeviceLists.AddRange(resetList, senderList);
            setup.DeviceRegistrationChallenges.Add(challenge);
            setup.Sessions.Add(foreignSession);
            setup.SessionKeyBlobs.Add(existingBlob);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var commitReached = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var allowCommit = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var contextFactory = new IntegrationDbContextFactory(options);
        var durability = new IdentityResetDurabilityCoordinator(contextFactory);
        var authority = new RecordingResetSessionEndAuthority(
            new SessionLifecycleGate());
        var enforcer = new RecordingResetRealtimeEnforcer(authority);

        await using var resetContext = new KodosiDbContext(options);
        var resetUnitOfWork = new FaultInjectingUnitOfWork(
            new UnitOfWork(resetContext),
            CommitFaultMode.PauseBeforeCommit,
            commitReached,
            allowCommit);
        var resetService = CreateIdentityResetService(
            resetContext,
            resetUnitOfWork,
            durability,
            authority,
            enforcer);
        var resetting = resetService.ResetIdentityAsync(
            resetUserId,
            new IdentityResetPopPayload(
                challenge.Id,
                removedDeviceId,
                Convert.ToBase64String([1, 2, 3])),
            new IdentityResetAuditContext("127.0.0.1", "postgres-race"),
            TestContext.Current.CancellationToken);
        await commitReached.Task.WaitAsync(
            TimeSpan.FromSeconds(5),
            TestContext.Current.CancellationToken);

        await using var publicationContext = new KodosiDbContext(options);
        var publishing = CreateSessionKeyDistributionService(publicationContext)
            .ReplaceKeyBlobsAsync(
                foreignSession.Id,
                foreignOwnerId,
                foreignSession.IncarnationId,
                [
                    new SessionKeyBlobSubmission(
                        removedDeviceId,
                        Convert.ToBase64String([8, 9, 10]),
                        senderDeviceId,
                        foreignSession.CurrentKeyGeneration,
                        DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                        Convert.ToBase64String([11, 12, 13]),
                        SessionKeyBlobSignatureDigest.CurrentVersion)
                ],
                TestContext.Current.CancellationToken);

        await Task.Delay(50, TestContext.Current.CancellationToken);
        Assert.False(publishing.IsCompleted);
        allowCommit.TrySetResult();
        await resetting;

        var staleResult = await publishing;
        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, staleResult.State);
        Assert.Contains(
            "not authorized",
            staleResult.ErrorMessage,
            StringComparison.Ordinal);

        await using (var freshPublicationContext = new KodosiDbContext(options))
        {
            var freshResult = await CreateSessionKeyDistributionService(
                    freshPublicationContext)
                .ReplaceKeyBlobsAsync(
                    foreignSession.Id,
                    foreignOwnerId,
                    foreignSession.IncarnationId,
                    [
                        new SessionKeyBlobSubmission(
                            senderDeviceId,
                            Convert.ToBase64String([14, 15, 16]),
                            senderDeviceId,
                            1,
                            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                            Convert.ToBase64String([17, 18, 19]),
                            SessionKeyBlobSignatureDigest.CurrentVersion)
                    ],
                    TestContext.Current.CancellationToken);
            Assert.Equal(StoreSessionKeyBlobsState.Stored, freshResult.State);
        }

        await using var verify = new KodosiDbContext(options);
        var persistedSession = await verify.Sessions
            .AsNoTracking()
            .SingleAsync(
                candidate => candidate.Id == foreignSession.Id,
                TestContext.Current.CancellationToken);
        var persistedBlob = await verify.SessionKeyBlobs
            .AsNoTracking()
            .SingleAsync(
                candidate => candidate.SessionId == foreignSession.Id,
                TestContext.Current.CancellationToken);
        Assert.Equal(1, persistedSession.CurrentKeyGeneration);
        Assert.Equal(senderDeviceId, persistedBlob.RecipientDeviceId);
        Assert.DoesNotContain(
            removedDeviceId,
            await verify.SessionKeyBlobs
                .Where(candidate => candidate.SessionId == foreignSession.Id)
                .Select(candidate => candidate.RecipientDeviceId)
                .ToListAsync(TestContext.Current.CancellationToken));
    }

    private static async Task AssertIdentityResetSerializesFirstKeyPublicationAsync(
        DbContextOptions<KodosiDbContext> options)
    {
        var resetUserId = UserId.New();
        var foreignOwnerId = UserId.New();
        var resetSuffix = resetUserId.Value.ToString("N");
        var foreignSuffix = foreignOwnerId.Value.ToString("N");
        var removedDeviceId = $"reset-first-{resetSuffix}";
        var senderDeviceId = $"reset-first-sender-{foreignSuffix}";
        var removedDevice = CreateCertifiedDevice(resetUserId, removedDeviceId);
        var senderDevice = CreateCertifiedDevice(foreignOwnerId, senderDeviceId);
        var challenge = DeviceRegistrationChallenge.Create(
            resetUserId,
            Enumerable.Repeat((byte)8, 32).ToArray(),
            TimeSpan.FromMinutes(5));
        var foreignSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            foreignOwnerId,
            "identity-reset-first-key-publication-race",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret-hash");
        var resetList = TestDeviceList.Create(
            resetUserId,
            1,
            TestDeviceList.Entries(
                [(removedDeviceId, removedDeviceId)]),
            removedDeviceId,
            [2],
            DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds(),
            null);
        var senderList = TestDeviceList.Create(
            foreignOwnerId,
            1,
            TestDeviceList.Entries(
                [(senderDeviceId, senderDeviceId)]),
            senderDeviceId,
            [2],
            DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds(),
            null);
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Users.AddRange(
                User.Create(
                    resetUserId,
                    $"reset-first-{resetSuffix}@example.test",
                    $"reset-first-{resetSuffix[..10]}",
                    "Reset First"),
                User.Create(
                    foreignOwnerId,
                    $"foreign-first-{foreignSuffix}@example.test",
                    $"foreign-first-{foreignSuffix[..10]}",
                    "Foreign First"));
            setup.UserDevices.AddRange(removedDevice, senderDevice);
            setup.UserDeviceLists.AddRange(resetList, senderList);
            setup.DeviceRegistrationChallenges.Add(challenge);
            setup.Sessions.Add(foreignSession);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var commitReached = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var allowCommit = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var enforcementReached = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var allowEnforcement = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var contextFactory = new IntegrationDbContextFactory(options);
        var authority = new RecordingResetSessionEndAuthority(
            new SessionLifecycleGate());
        var enforcer = new RecordingResetRealtimeEnforcer(
            authority,
            enforcementReached,
            allowEnforcement.Task);

        await using var resetContext = new KodosiDbContext(options);
        var resetService = CreateIdentityResetService(
            resetContext,
            new FaultInjectingUnitOfWork(
                new UnitOfWork(resetContext),
                CommitFaultMode.PauseBeforeCommit,
                commitReached,
                allowCommit),
            new IdentityResetDurabilityCoordinator(contextFactory),
            authority,
            enforcer);
        var resetting = resetService.ResetIdentityAsync(
            resetUserId,
            new IdentityResetPopPayload(
                challenge.Id,
                removedDeviceId,
                Convert.ToBase64String([1, 2, 3])),
            new IdentityResetAuditContext("127.0.0.1", "postgres-first-race"),
            TestContext.Current.CancellationToken);
        await commitReached.Task.WaitAsync(
            TimeSpan.FromSeconds(5),
            TestContext.Current.CancellationToken);

        await using var publicationContext = new KodosiDbContext(options);
        var lifecycleLockAttempted = new TaskCompletionSource<bool>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var publishingService = new SessionKeyDistributionService(
            new SessionRepository(publicationContext),
            new SessionKeyBlobRepository(publicationContext),
            new UserDeviceRepository(publicationContext),
            new UserDeviceListRepository(publicationContext),
            new AlwaysValidSignatureVerifier(),
            new PostgresUserLifecycleLock(publicationContext),
            new SignalingRecipientDeviceLifecycleLock(
                new PostgresRecipientDeviceLifecycleLock(publicationContext),
                lifecycleLockAttempted),
            new GateOnlySessionEndAuthority(new SessionLifecycleGate()),
            CreateSessionAccessService(publicationContext),
            new UnitOfWork(publicationContext),
            new IntegrationNoOpSharingMetrics(),
            TimeProvider.System);
        var publishing = publishingService.ReplaceKeyBlobsAsync(
            foreignSession.Id,
            foreignOwnerId,
            foreignSession.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    removedDeviceId,
                    Convert.ToBase64String([8, 9, 10]),
                    senderDeviceId,
                    foreignSession.CurrentKeyGeneration,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([11, 12, 13]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);
        await lifecycleLockAttempted.Task.WaitAsync(
            TimeSpan.FromSeconds(5),
            TestContext.Current.CancellationToken);
        Assert.False(publishing.IsCompleted);

        allowCommit.TrySetResult();
        await enforcementReached.Task.WaitAsync(
            TimeSpan.FromSeconds(5),
            TestContext.Current.CancellationToken);
        Assert.False(publishing.IsCompleted);
        allowEnforcement.TrySetResult();
        await resetting;
        var publicationResult = await publishing;
        Assert.Equal(
            StoreSessionKeyBlobsState.InvalidRequest,
            publicationResult.State);
        Assert.Equal(
            $"Recipient device {removedDeviceId} is not authorized for the session.",
            publicationResult.ErrorMessage);

        await using var verify = new KodosiDbContext(options);
        Assert.False(await verify.SessionKeyBlobs
            .AsNoTracking()
            .AnyAsync(
                blob => blob.SessionId == foreignSession.Id,
                TestContext.Current.CancellationToken));
        Assert.Equal(
            0,
            await verify.Sessions
                .AsNoTracking()
                .Where(session => session.Id == foreignSession.Id)
                .Select(session => session.CurrentKeyGeneration)
                .SingleAsync(TestContext.Current.CancellationToken));
        Assert.False(await verify.UserDevices
            .AsNoTracking()
            .AnyAsync(
                device => device.DeviceId == removedDeviceId,
                TestContext.Current.CancellationToken));
    }

    private static async Task AssertSessionAccessMutationDurabilityAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId,
        UserId actorId)
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "durable-access-mutation",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Sessions.Add(session);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var mutationId = Guid.CreateVersion7();
        var expiresAt = DateTimeOffset.FromUnixTimeMilliseconds(
            DateTimeOffset.UtcNow.AddHours(1).ToUnixTimeMilliseconds());
        await ApplyGrantAsync(
            options,
            session,
            ownerId,
            actorId,
            mutationId,
            expiresAt);
        await ApplyGrantAsync(
            options,
            session,
            ownerId,
            actorId,
            mutationId,
            expiresAt);

        await using var verify = new KodosiDbContext(options);
        Assert.Equal(
            1,
            await verify.SessionAccessOverrides.CountAsync(
                access => access.SessionId == session.Id
                    && access.ActorUserId == actorId,
                TestContext.Current.CancellationToken));
        Assert.Equal(
            1,
            await verify.Set<AccessOverrideAuditEntry>().CountAsync(
                entry => entry.SessionId == session.Id
                    && entry.GranteeUserId == actorId
                    && entry.Action == AccessOverrideAuditAction.Granted,
                TestContext.Current.CancellationToken));
        Assert.Equal(
            1,
            await verify.SessionAccessMutations.CountAsync(
                mutation => mutation.RequesterUserId == ownerId
                    && mutation.MutationId == mutationId,
                TestContext.Current.CancellationToken));

        var receipt = await new SessionAccessMutationReceiptLookup(
                new SessionAccessMutationRepository(verify))
            .FindAsync(
                ownerId,
                session.Id,
                session.IncarnationId,
                mutationId,
                TestContext.Current.CancellationToken);
        Assert.NotNull(receipt);
        Assert.Equal("grant", receipt.Kind);
        Assert.Equal(actorId.Value, receipt.TargetUserId);
        Assert.Equal(AccessLevel.Inject, receipt.AccessLevel);
        Assert.Equal(expiresAt, receipt.RequestedExpiresAt);

        await using (var conflictContext = new KodosiDbContext(options))
        {
            var conflictService = CreateSharingService(conflictContext);
            var conflict = await Assert.ThrowsAsync<SessionAccessMutationTargetConflictException>(() =>
                conflictService.GrantAccessAsync(
                    session.Id,
                    session.IncarnationId,
                    mutationId,
                    actorId,
                    AccessLevel.View,
                    ownerId,
                    expiresAt,
                    TestContext.Current.CancellationToken));
            Assert.Equal("SESSION_ACCESS_MUTATION_TARGET_CONFLICT", conflict.Code);
        }

        var predecessorIncarnation = session.IncarnationId;
        await using (var republish = new KodosiDbContext(options))
        {
            var persisted = await republish.Sessions.SingleAsync(
                candidate => candidate.Id == session.Id,
                TestContext.Current.CancellationToken);
            persisted.End();
            persisted.Republish(Guid.CreateVersion7(), checked(persisted.IncarnationGeneration + 1),
                ownerId,
                "durable-access-mutation-replacement",
                SessionScope.Friends,
                ToolKind.Terminal,
                AccessLevel.View,
                "replacement-secret",
                roomId: null);
            await republish.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using (var afterRepublish = new KodosiDbContext(options))
        {
            var lookup = new SessionAccessMutationReceiptLookup(
                new SessionAccessMutationRepository(afterRepublish));
            Assert.NotNull(await lookup.FindAsync(
                ownerId,
                session.Id,
                predecessorIncarnation,
                mutationId,
                TestContext.Current.CancellationToken));
            var replacementIncarnation = await afterRepublish.Sessions
                .Where(candidate => candidate.Id == session.Id)
                .Select(candidate => candidate.IncarnationId)
                .SingleAsync(TestContext.Current.CancellationToken);
            Assert.NotEqual(predecessorIncarnation, replacementIncarnation);
            Assert.Null(await lookup.FindAsync(
                ownerId,
                session.Id,
                replacementIncarnation,
                mutationId,
                TestContext.Current.CancellationToken));
            Assert.Null(await lookup.FindAsync(
                actorId,
                session.Id,
                predecessorIncarnation,
                mutationId,
                TestContext.Current.CancellationToken));
        }
    }

    private static async Task ApplyGrantAsync(
        DbContextOptions<KodosiDbContext> options,
        Session session,
        UserId ownerId,
        UserId actorId,
        Guid mutationId,
        DateTimeOffset expiresAt)
    {
        await using var context = new KodosiDbContext(options);
        await using var result = await CreateSharingService(context).GrantAccessAsync(
            session.Id,
            session.IncarnationId,
            mutationId,
            actorId,
            AccessLevel.Inject,
            ownerId,
            expiresAt,
            TestContext.Current.CancellationToken);
    }

    private static SharingService CreateSharingService(KodosiDbContext context) =>
        new(
            new SessionRepository(context),
            new UserRepository(context),
            new AccessOverrideRepository(context, TimeProvider.System),
            new AccessOverrideAuditRepository(context),
            new SessionAccessMutationRepository(context),
            new PostgresUserLifecycleLock(context),
            new GateOnlySessionEndAuthority(new SessionLifecycleGate()),
            new UnitOfWork(context));

    private static async Task AssertStaleSharingGrantRejectedAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId)
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "stale-sharing-grant",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Sessions.Add(session);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var gate = new SessionLifecycleGate();
        var held = await gate.AcquireAsync(
            session.Id,
            TestContext.Current.CancellationToken);
        await using var grantContext = new KodosiDbContext(options);
        var grantService = new SharingService(
            new SessionRepository(grantContext),
            new UserRepository(grantContext),
            new AccessOverrideRepository(grantContext, TimeProvider.System),
            new AccessOverrideAuditRepository(grantContext),
            new SessionAccessMutationRepository(grantContext),
            new PostgresUserLifecycleLock(grantContext),
            new GateOnlySessionEndAuthority(gate),
            new UnitOfWork(grantContext));
        var granting = grantService.GrantAccessAsync(
            session.Id,
            session.IncarnationId,
            Guid.CreateVersion7(),
            UserId.New(),
            AccessLevel.View,
            ownerId,
            expiresAt: DateTimeOffset.UtcNow.AddHours(1),
            TestContext.Current.CancellationToken);
        while (gate.TestReferenceCount(session.Id) < 2)
        {
            await Task.Delay(1, TestContext.Current.CancellationToken);
        }

        await using (var republish = new KodosiDbContext(options))
        {
            var persisted = await republish.Sessions.SingleAsync(
                candidate => candidate.Id == session.Id,
                TestContext.Current.CancellationToken);
            persisted.End();
            await Task.Delay(1, TestContext.Current.CancellationToken);
            persisted.Republish(Guid.CreateVersion7(), checked(persisted.IncarnationGeneration + 1),
                ownerId,
                "replacement-sharing-grant",
                SessionScope.Friends,
                ToolKind.Terminal,
                AccessLevel.View,
                "replacement-secret",
                roomId: null);
            await republish.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        await held.DisposeAsync();

        await Assert.ThrowsAsync<NotFoundException>(() => granting);
    }

    private static async Task AssertRoomForUpdateProjectsConcurrencyTokenAsync(
    DbContextOptions<KodosiDbContext> options,
    RoomId roomId,
    SessionId sessionId)
    {
        await using var context = new KodosiDbContext(options);
        var unitOfWork = new UnitOfWork(context);
        var repository = new SessionRepository(context);
        await using var transaction = await unitOfWork.BeginTransactionAsync(
            TestContext.Current.CancellationToken);

        var locked = await repository.GetNonEndedByRoomIdsForUpdateAsync(
            roomId,
            [sessionId],
            TestContext.Current.CancellationToken);

        Assert.NotEmpty(locked);
        Assert.All(locked, session => Assert.NotEqual(0u, session.Version));



        var target = locked.Single(session => session.Id == sessionId);
        target.FenceKeyPublication();
        await repository.UpdateAsync(target, TestContext.Current.CancellationToken);
        await unitOfWork.SaveChangesAsync(TestContext.Current.CancellationToken);
        await transaction.CommitAsync(TestContext.Current.CancellationToken);

        await using var verifyContext = new KodosiDbContext(options);
        var persisted = await verifyContext.Sessions
            .AsNoTracking()
            .SingleAsync(session => session.Id == sessionId, TestContext.Current.CancellationToken);
        Assert.Equal(1, persisted.CurrentKeyGeneration);
    }

    private static IdentityLifecycleService CreateIdentityResetService(
        KodosiDbContext context,
        IUnitOfWork unitOfWork,
        IIdentityResetDurabilityCoordinator durabilityCoordinator,
        ISessionEndAuthority sessionEndAuthority,
        IIdentityResetRealtimeEnforcer realtimeEnforcer)
    {
        var devices = new UserDeviceRepository(context);
        var sessions = new SessionRepository(context);
        var blobs = new SessionKeyBlobRepository(context);
        var authMetrics = new IntegrationNoOpAuthMetrics();
        var popVerifier = new IdentityResetPopVerifier(
            new PopChallengeConsumer(
                new DeviceRegistrationChallengeRepository(context),
                new AlwaysValidSignatureVerifier(),
                authMetrics),
            authMetrics);
        var cascade = new IdentityResetCascade(
            devices,
            new UserDeviceListRepository(context),
            new DeviceLinkRequestRepository(context),
            new SemanticRelayLifecycleRepository(context),
            blobs,
            sessions,
            new IdentityResetAuditRepository(context),
            unitOfWork,
            new LiveSessionTerminator(
                sessions,
                blobs,
                unitOfWork,
                new SessionEndMutationRepository(context)));
        return new IdentityLifecycleService(
            devices,
            new UserDeviceListRepository(context),
            new UserRepository(context),
            new FriendshipRepository(context),
            new RoomMemberRepository(context),
            new AccessOverrideRepository(context, TimeProvider.System),
            new IdentityExposureRepository(context),
            popVerifier,
            cascade,
            unitOfWork,
            new PostgresUserLifecycleLock(context),
            new PostgresRecipientDeviceLifecycleLock(context),
            sessionEndAuthority,
            realtimeEnforcer,
            durabilityCoordinator,
            NullLogger<IdentityLifecycleService>.Instance);
    }

    private static SessionAccessService CreateSessionAccessService(
        KodosiDbContext context) =>
        new(
            new FriendshipRepository(context),
            new RoomMemberRepository(context),
            new AccessOverrideRepository(context, TimeProvider.System),
            new SessionViewerDismissalRepository(context));

    private static SessionKeyDistributionService
        CreateSessionKeyDistributionService(KodosiDbContext context) =>
        new(
            new SessionRepository(context),
            new SessionKeyBlobRepository(context),
            new UserDeviceRepository(context),
            new UserDeviceListRepository(context),
            new AlwaysValidSignatureVerifier(),
            new PostgresUserLifecycleLock(context),
            new PostgresRecipientDeviceLifecycleLock(context),
            new GateOnlySessionEndAuthority(new SessionLifecycleGate()),
            CreateSessionAccessService(context),
            new UnitOfWork(context),
            new IntegrationNoOpSharingMetrics(),
            TimeProvider.System);

    private enum CommitFaultMode
    {
        BeforeCommit,
        AfterCommit,
        PauseBeforeCommit,
    }

    private sealed class FaultInjectingUnitOfWork(
        IUnitOfWork inner,
        CommitFaultMode faultMode,
        TaskCompletionSource? commitReached = null,
        TaskCompletionSource? allowCommit = null) : IUnitOfWork
    {
        private readonly IUnitOfWork _inner = inner;
        private readonly CommitFaultMode _faultMode = faultMode;
        private readonly TaskCompletionSource? _commitReached = commitReached;
        private readonly TaskCompletionSource? _allowCommit = allowCommit;

        public Task SaveChangesAsync(CancellationToken ct = default) =>
            _inner.SaveChangesAsync(ct);

        public async Task<ITransactionScope> BeginTransactionAsync(
            CancellationToken ct = default) =>
            new FaultInjectingTransaction(
                await _inner.BeginTransactionAsync(ct),
                _faultMode,
                _commitReached,
                _allowCommit);
    }

    private sealed class FaultInjectingTransaction(
        ITransactionScope inner,
        CommitFaultMode faultMode,
        TaskCompletionSource? commitReached,
        TaskCompletionSource? allowCommit) : ITransactionScope
    {
        private readonly ITransactionScope _inner = inner;
        private readonly CommitFaultMode _faultMode = faultMode;
        private readonly TaskCompletionSource? _commitReached = commitReached;
        private readonly TaskCompletionSource? _allowCommit = allowCommit;

        public async Task CommitAsync(CancellationToken ct = default)
        {
            if (_faultMode == CommitFaultMode.BeforeCommit)
            {
                throw new InvalidOperationException(
                    "Injected identity reset failure before commit.");
            }
            if (_faultMode == CommitFaultMode.PauseBeforeCommit)
            {
                _commitReached!.TrySetResult();
                await _allowCommit!.Task.WaitAsync(ct);
            }

            await _inner.CommitAsync(ct);
            if (_faultMode == CommitFaultMode.AfterCommit)
            {
                throw new InvalidOperationException(
                    "Injected identity reset failure after commit.");
            }
        }

        public Task CreateSavepointAsync(
            string name,
            CancellationToken ct = default) =>
            _inner.CreateSavepointAsync(name, ct);

        public Task RollbackToSavepointAsync(
            string name,
            CancellationToken ct = default) =>
            _inner.RollbackToSavepointAsync(name, ct);

        public ValueTask DisposeAsync() => _inner.DisposeAsync();
    }

    private sealed class IntegrationDbContextFactory(
        DbContextOptions<KodosiDbContext> options)
        : IDbContextFactory<KodosiDbContext>
    {
        private readonly DbContextOptions<KodosiDbContext> _options = options;

        public KodosiDbContext CreateDbContext() => new(_options);
    }

    private sealed class RecordingResetSessionEndAuthority(
        SessionLifecycleGate? lifecycleGate = null) : ISessionEndAuthority
    {
        private readonly SessionLifecycleGate? _lifecycleGate = lifecycleGate;

        public bool LeaseHeld { get; private set; }

        public async ValueTask<IAsyncDisposable> AcquireAsync(
            IReadOnlyCollection<SessionId> sessionIds,
            CancellationToken ct = default)
        {
            LeaseHeld = true;
            var inner = _lifecycleGate is null
                ? null
                : await _lifecycleGate.AcquireAsync(sessionIds, ct);
            return new RecordingLease(this, inner);
        }

        public Task ProjectCommittedAsync(
            IReadOnlyCollection<CommittedSessionEnd> sessionEnds,
            CancellationToken ct = default) =>
            Task.CompletedTask;

        public Task RetireEndedIncarnationAsync(
            SessionDiscoveryTarget endedSession,
            DateTimeOffset startedAt,
            CancellationToken ct = default) =>
            Task.CompletedTask;

        private sealed class RecordingLease(
            RecordingResetSessionEndAuthority owner,
            IAsyncDisposable? inner) : IAsyncDisposable
        {
            public async ValueTask DisposeAsync()
            {
                if (inner is not null)
                {
                    await inner.DisposeAsync();
                }
                owner.LeaseHeld = false;
            }
        }
    }

    private sealed class RecordingResetRealtimeEnforcer(
        RecordingResetSessionEndAuthority sessionEndAuthority,
        TaskCompletionSource? enforcementReached = null,
        Task? allowEnforcement = null)
        : IIdentityResetRealtimeEnforcer
    {
        private readonly RecordingResetSessionEndAuthority _sessionEndAuthority =
            sessionEndAuthority;
        private bool _fenceHeld;

        public int EnforcementCount { get; private set; }
        public bool FenceHeld => _fenceHeld;
        public bool EnforcedWithFence { get; private set; }
        public bool EnforcedWithSessionLease { get; private set; }

        public IDisposable FenceNewConnections(
            UserId userId,
            IReadOnlyCollection<string> removedDeviceIds)
        {
            _fenceHeld = true;
            return new CallbackDisposable(() => _fenceHeld = false);
        }

        public IReadOnlyList<SessionId> GetAffectedSessionIds(
            UserId userId,
            IReadOnlyCollection<string> removedDeviceIds,
            IReadOnlyCollection<SessionId> sessionsWithRevokedKeys) =>
            sessionsWithRevokedKeys
                .Distinct()
                .OrderBy(sessionId => sessionId.Value)
                .ToList();

        public async Task EnforceCommittedAsync(
            UserId userId,
            IReadOnlyCollection<string> removedDeviceIds,
            IReadOnlyCollection<SessionId> sessionsWithRevokedKeys,
            CancellationToken ct = default)
        {
            EnforcementCount++;
            EnforcedWithFence = _fenceHeld;
            EnforcedWithSessionLease = _sessionEndAuthority.LeaseHeld;
            enforcementReached?.TrySetResult();
            if (allowEnforcement is not null)
            {
                await allowEnforcement.WaitAsync(ct);
            }
        }
    }

    private sealed class RecordingResetDurabilityCoordinator(
        IIdentityResetDurabilityCoordinator inner,
        RecordingResetSessionEndAuthority sessionEndAuthority,
        RecordingResetRealtimeEnforcer realtimeEnforcer)
        : IIdentityResetDurabilityCoordinator
    {
        private readonly IIdentityResetDurabilityCoordinator _inner = inner;
        private readonly RecordingResetSessionEndAuthority _sessionEndAuthority =
            sessionEndAuthority;
        private readonly RecordingResetRealtimeEnforcer _realtimeEnforcer =
            realtimeEnforcer;

        public int ReconciliationCount { get; private set; }
        public bool ReconciledWithFence { get; private set; }
        public bool ReconciledWithSessionLease { get; private set; }

        public Task<IdentityResetCommitReconciliation> ReconcileCommitAsync(
            Guid resetId,
            UserId userId,
            IReadOnlyCollection<string> expectedRemovedDeviceIds,
            CancellationToken ct = default)
        {
            ReconciliationCount++;
            ReconciledWithFence = _realtimeEnforcer.FenceHeld;
            ReconciledWithSessionLease = _sessionEndAuthority.LeaseHeld;
            return _inner.ReconcileCommitAsync(
                resetId,
                userId,
                expectedRemovedDeviceIds,
                ct);
        }

        public Task<IReadOnlyList<IdentityResetEnforcementWork>>
            GetPendingEnforcementAsync(
                int limit,
                CancellationToken ct = default) =>
            _inner.GetPendingEnforcementAsync(limit, ct);

        public Task<bool> ExecuteIfCurrentAsync(
            IdentityResetEnforcementWork work,
            Func<CancellationToken, Task> enforce,
            CancellationToken ct = default) =>
            _inner.ExecuteIfCurrentAsync(work, enforce, ct);

        public Task CompleteEnforcementAsync(
            Guid resetId,
            CancellationToken ct = default) =>
            _inner.CompleteEnforcementAsync(resetId, ct);
    }

    private sealed class CallbackDisposable(Action dispose) : IDisposable
    {
        private Action? _dispose = dispose;

        public void Dispose() =>
            Interlocked.Exchange(ref _dispose, null)?.Invoke();
    }

    private sealed class GateOnlySessionEndAuthority(
        SessionLifecycleGate lifecycleGate) : ISessionEndAuthority
    {
        public ValueTask<IAsyncDisposable> AcquireAsync(
            IReadOnlyCollection<SessionId> sessionIds,
            CancellationToken ct = default) =>
            lifecycleGate.AcquireAsync(sessionIds, ct);

        public Task ProjectCommittedAsync(
            IReadOnlyCollection<CommittedSessionEnd> sessionEnds,
            CancellationToken ct = default) =>
            Task.CompletedTask;

        public Task RetireEndedIncarnationAsync(
            SessionDiscoveryTarget endedSession,
            DateTimeOffset startedAt,
            CancellationToken ct = default) =>
            Task.CompletedTask;
    }

    private sealed class ThrowingAddSessionKeyBlobRepository(
        ISessionKeyBlobRepository inner) : ISessionKeyBlobRepository
    {
        public Task<SessionKeyBlob?> GetForDeviceAsync(
            SessionId sessionId,
            string recipientDeviceId,
            CancellationToken ct = default) =>
            inner.GetForDeviceAsync(sessionId, recipientDeviceId, ct);

        public Task AddRangeAsync(
            IReadOnlyList<SessionKeyBlob> blobs,
            CancellationToken ct = default) =>
            throw new InvalidOperationException("Injected key blob add failure.");

        public Task DeleteForSessionAsync(
            SessionId sessionId,
            CancellationToken ct = default) =>
            inner.DeleteForSessionAsync(sessionId, ct);

        public Task<IReadOnlyList<DeviceRevocationSessionTarget>>
            GetSessionTargetsForRecipientDevicesAsync(
                IReadOnlyCollection<string> recipientDeviceIds,
                CancellationToken ct = default) =>
            inner.GetSessionTargetsForRecipientDevicesAsync(recipientDeviceIds, ct);

        public Task<IReadOnlyList<SessionId>> GetSessionIdsForRecipientDevicesAsync(
            IReadOnlyCollection<string> recipientDeviceIds,
            CancellationToken ct = default) =>
            inner.GetSessionIdsForRecipientDevicesAsync(recipientDeviceIds, ct);

        public Task<int> DeleteForRecipientDevicesAsync(
            IReadOnlyCollection<string> recipientDeviceIds,
            CancellationToken ct = default) =>
            inner.DeleteForRecipientDevicesAsync(recipientDeviceIds, ct);
    }

    private sealed class SignalingUserLifecycleLock(
        IUserLifecycleLock inner,
        TaskCompletionSource<bool> attempted) : IUserLifecycleLock
    {
        public async Task AcquireAsync(UserId userId, CancellationToken ct = default)
        {
            attempted.TrySetResult(true);
            await inner.AcquireAsync(userId, ct);
        }
    }

    private sealed class SignalingRecipientDeviceLifecycleLock(
        IRecipientDeviceLifecycleLock inner,
        TaskCompletionSource<bool> attempted)
        : IRecipientDeviceLifecycleLock
    {
        public Task AcquireAsync(
            IReadOnlyCollection<UserId> recipientUserIds,
            CancellationToken ct = default)
        {
            attempted.TrySetResult(true);
            return inner.AcquireAsync(recipientUserIds, ct);
        }
    }

    private sealed class IntegrationNoOpSharingMetrics : ISharingMetrics
    {
        public void RecordSessionKeySkewRejection()
        {
        }
    }

    private sealed class IntegrationNoOpAuthMetrics : IAuthMetrics
    {
        public void RecordPopFailure(PopFailureReason reason, string endpoint)
        {
        }
    }

    private static async Task AssertRoomEntityIdempotencyAsync(
        DbContextOptions<KodosiDbContext> options,
        RoomId roomId,
        UserId ownerId)
    {
        var messageId = Guid.NewGuid();
        var recipientSessionIds = new[] { Guid.NewGuid(), Guid.NewGuid() };
        var recipientUserIds = new[] { Guid.NewGuid(), Guid.NewGuid() };
        await using (var firstContext = new KodosiDbContext(options))
        {
            var unitOfWork = new UnitOfWork(firstContext);
            await using var transaction = await unitOfWork.BeginTransactionAsync(
                TestContext.Current.CancellationToken);
            _ = await new RoomChatRepository(firstContext).AddWithNextSeqAsync(
                roomId,
                messageId,
                ownerId,
                null,
                RoomChatAuthorKind.Human,
                recipientSessionIds.Reverse().ToArray(),
                recipientUserIds.Reverse().ToArray(),
                "original-ciphertext",
                TestContext.Current.CancellationToken);
            await unitOfWork.SaveChangesAsync(TestContext.Current.CancellationToken);
            await transaction.CommitAsync(TestContext.Current.CancellationToken);
        }

        var taskId = Guid.NewGuid();
        await using (var firstContext = new KodosiDbContext(options))
        {
            var unitOfWork = new UnitOfWork(firstContext);
            await using var transaction = await unitOfWork.BeginTransactionAsync(
                TestContext.Current.CancellationToken);
            var task = RoomTask.Create(
                taskId,
                roomId,
                ownerId,
                "original-task",
                null,
                null,
                null, DateTimeOffset.UtcNow);
            _ = await new RoomTaskRepository(firstContext).AddIdempotentAsync(
                task,
                TestContext.Current.CancellationToken);
            await unitOfWork.SaveChangesAsync(TestContext.Current.CancellationToken);
            await transaction.CommitAsync(TestContext.Current.CancellationToken);
        }

        await using (var retryContext = new KodosiDbContext(options))
        {
            var unitOfWork = new UnitOfWork(retryContext);
            await using var transaction = await unitOfWork.BeginTransactionAsync(
                TestContext.Current.CancellationToken);
            var retry = RoomTask.Create(
                taskId,
                roomId,
                ownerId,
                "different-task",
                null,
                null,
                null, DateTimeOffset.UtcNow);
            await Assert.ThrowsAsync<ConflictException>(() =>
                new RoomTaskRepository(retryContext).AddIdempotentAsync(
                    retry,
                    TestContext.Current.CancellationToken));
        }

        var assignedTaskId = Guid.NewGuid();
        var assignedSessionId = Guid.NewGuid();
        var firstIncarnationId = Guid.NewGuid();
        await using (var assignedContext = new KodosiDbContext(options))
        {
            var unitOfWork = new UnitOfWork(assignedContext);
            await using var transaction = await unitOfWork.BeginTransactionAsync(
                TestContext.Current.CancellationToken);
            var assignedTask = RoomTask.Create(
                assignedTaskId,
                roomId,
                ownerId,
                "assigned-task",
                null,
                assignedSessionId,
                null, DateTimeOffset.UtcNow);
            assignedTask.AssignInitialSession(assignedSessionId, firstIncarnationId);
            _ = await new RoomTaskRepository(assignedContext).AddIdempotentAsync(
                assignedTask,
                TestContext.Current.CancellationToken);
            await unitOfWork.SaveChangesAsync(TestContext.Current.CancellationToken);
            await transaction.CommitAsync(TestContext.Current.CancellationToken);
        }

        await using (var staleReplayContext = new KodosiDbContext(options))
        {
            var unitOfWork = new UnitOfWork(staleReplayContext);
            await using var transaction = await unitOfWork.BeginTransactionAsync(
                TestContext.Current.CancellationToken);
            var staleReplay = RoomTask.Create(
                assignedTaskId,
                roomId,
                ownerId,
                "assigned-task",
                null,
                assignedSessionId,
                null, DateTimeOffset.UtcNow);
            staleReplay.AssignInitialSession(assignedSessionId, Guid.NewGuid());
            await Assert.ThrowsAsync<ConflictException>(() =>
                new RoomTaskRepository(staleReplayContext).AddIdempotentAsync(
                    staleReplay,
                    TestContext.Current.CancellationToken));
        }

        await using var verifyContext = new KodosiDbContext(options);
        var persistedMessage = await verifyContext.RoomChatMessages.SingleAsync(
            message => message.Id == messageId,
            TestContext.Current.CancellationToken);
        Assert.Equal(recipientSessionIds.Order(), persistedMessage.RecipientSessionIds);
        Assert.Equal(recipientUserIds.Order(), persistedMessage.RecipientUserIds);
        Assert.Equal(
            1,
            await verifyContext.RoomChatMessages.CountAsync(
                message => message.Id == messageId,
                TestContext.Current.CancellationToken));
        Assert.Equal(
            1,
            await verifyContext.RoomTasks.CountAsync(
                task => task.Id == taskId,
                TestContext.Current.CancellationToken));
    }

    private static async Task AssertRoomChatCrossRoomMessageIdConcurrencyAsync(
        DbContextOptions<KodosiDbContext> options,
        RoomId firstRoomId,
        UserId ownerId)
    {
        var secondRoomId = RoomId.From(Guid.NewGuid());
        await using (var setup = new KodosiDbContext(options))
        {
            setup.Rooms.Add(Room.Create(
                secondRoomId,
                ownerId,
                "Second room",
                $"second-room-{secondRoomId.Value:N}",
                1,
                [1],
                [2],
                "owner-device"));
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var messageId = Guid.NewGuid();
        await using var firstContext = new KodosiDbContext(options);
        await using var secondContext = new KodosiDbContext(options);
        var firstUnitOfWork = new UnitOfWork(firstContext);
        var secondUnitOfWork = new UnitOfWork(secondContext);
        await using var firstTransaction = await firstUnitOfWork.BeginTransactionAsync(
            TestContext.Current.CancellationToken);
        await using var secondTransaction = await secondUnitOfWork.BeginTransactionAsync(
            TestContext.Current.CancellationToken);

        var firstRepository = new RoomChatRepository(firstContext);
        var secondRepository = new RoomChatRepository(secondContext);
        await firstRepository.AcquireMessageLockAsync(
            messageId,
            TestContext.Current.CancellationToken);
        await firstRepository.AcquireRoomLockAsync(
            firstRoomId,
            TestContext.Current.CancellationToken);
        _ = await firstRepository.AddWithNextSeqAsync(
            firstRoomId,
            messageId,
            ownerId,
            null,
            RoomChatAuthorKind.Human,
            [],
            [],
            "first-ciphertext",
            TestContext.Current.CancellationToken);
        var competingLock = secondRepository.AcquireMessageLockAsync(
            messageId,
            TestContext.Current.CancellationToken);

        await Task.Delay(50, TestContext.Current.CancellationToken);
        Assert.False(competingLock.IsCompleted);
        await firstUnitOfWork.SaveChangesAsync(TestContext.Current.CancellationToken);
        await firstTransaction.CommitAsync(TestContext.Current.CancellationToken);
        await competingLock;
        var existing = await secondRepository.GetByIdAsync(
            messageId,
            TestContext.Current.CancellationToken);
        Assert.NotNull(existing);
        Assert.Equal(firstRoomId, existing.RoomId);
        Assert.NotEqual(secondRoomId, existing.RoomId);
    }

    private static async Task AssertRoomChatAuthorKindHasNoDefaultAsync(
        KodosiDbContext context)
    {
        await context.Database.OpenConnectionAsync(TestContext.Current.CancellationToken);
        await using var command = context.Database.GetDbConnection().CreateCommand();
        command.CommandText =
            """
            SELECT column_default IS NULL
            FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND table_name = 'room_chat_messages'
              AND column_name = 'author_kind'
            """;
        var result = await command.ExecuteScalarAsync(TestContext.Current.CancellationToken);
        Assert.Equal(true, result);
    }

    private static async Task AssertInvitationLifecyclePersistenceAsync(
        DbContextOptions<KodosiDbContext> options,
        RoomId roomId,
        UserId ownerId,
        UserId inviteeId)
    {
        var now = DateTimeOffset.UtcNow;
        var stale = RoomInvitation.Create(
            Guid.NewGuid(),
            roomId,
            inviteeId,
            ownerId,
            Proposal(baseGeneration: 2, issuedAt: now.AddDays(-2), expiresAt: now.AddDays(-1)),
            now.AddDays(-2));
        await using (var context = new KodosiDbContext(options))
        {
            context.RoomInvitations.Add(stale);
            await new UnitOfWork(context).SaveChangesAsync(
                TestContext.Current.CancellationToken);
        }

        var replacement = RoomInvitation.Create(
            Guid.NewGuid(),
            roomId,
            inviteeId,
            ownerId,
            Proposal(baseGeneration: 2, issuedAt: now, expiresAt: now.AddDays(1)),
            now);
        await using (var context = new KodosiDbContext(options))
        {
            await using var transaction = await new UnitOfWork(context)
                .BeginTransactionAsync(TestContext.Current.CancellationToken);
            var trackedStale = await context.RoomInvitations.SingleAsync(
                invitation => invitation.Id == stale.Id,
                TestContext.Current.CancellationToken);
            trackedStale.Expire(now);
            context.RoomInvitations.Add(replacement);
            await new UnitOfWork(context).SaveChangesAsync(
                TestContext.Current.CancellationToken);
            await transaction.CommitAsync(TestContext.Current.CancellationToken);
        }

        await using (var conflictContext = new KodosiDbContext(options))
        {
            conflictContext.RoomInvitations.Add(RoomInvitation.Create(
                Guid.NewGuid(),
                roomId,
                inviteeId,
                ownerId,
                Proposal(baseGeneration: 2, issuedAt: now, expiresAt: now.AddDays(1)),
                now));
            await Assert.ThrowsAsync<ConflictException>(() =>
                new UnitOfWork(conflictContext).SaveChangesAsync(
                    TestContext.Current.CancellationToken));
        }

        await using var verifyContext = new KodosiDbContext(options);
        Assert.Equal(
            RoomInvitationStatus.Expired,
            (await verifyContext.RoomInvitations.SingleAsync(
                invitation => invitation.Id == stale.Id,
                TestContext.Current.CancellationToken)).Status);
        Assert.Equal(
            RoomInvitationStatus.Pending,
            (await verifyContext.RoomInvitations.SingleAsync(
                invitation => invitation.Id == replacement.Id,
                TestContext.Current.CancellationToken)).Status);
    }

    private static RoomInvitationProposalProof Proposal(
        long baseGeneration,
        DateTimeOffset issuedAt,
        DateTimeOffset expiresAt) =>
        new(
            baseGeneration,
            baseGeneration + 1,
            [1],
            [2],
            "owner-device",
            [3],
            [4],
            "owner-device",
            [5],
            issuedAt,
            expiresAt);

    private static async Task AssertAccessOverrideExpiryAsync(
        DbContextOptions<KodosiDbContext> options,
        SessionId sessionId,
        UserId ownerId,
        UserId inviteeId)
    {
        var now = DateTimeOffset.UtcNow;
        var accessOverride = SessionAccessOverride.Create(
            sessionId,
            inviteeId,
            AccessLevel.View,
            ownerId,
            now.AddMilliseconds(50),
            now);
        await using (var context = new KodosiDbContext(options))
        {
            context.SessionAccessOverrides.Add(accessOverride);
            await context.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await Task.Delay(75, TestContext.Current.CancellationToken);
        await using var queryContext = new KodosiDbContext(options);
        var repository = new AccessOverrideRepository(queryContext, TimeProvider.System);
        Assert.Null(await repository.GetActiveAsync(
            sessionId,
            inviteeId,
            TestContext.Current.CancellationToken));
        Assert.Single(await repository.GetExpiredUnrevokedAsync(
            DateTimeOffset.UtcNow,
            10,
            TestContext.Current.CancellationToken));
    }

    private static async Task AssertDeviceLinkGenerationInvalidationAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId,
        UserId otherUserId)
    {
        var eligible = CreateDeviceLinkRequest(ownerId, "eligible");
        eligible.Approve(1, DateTimeOffset.UtcNow);
        var excluded = CreateDeviceLinkRequest(ownerId, "excluded");
        excluded.Approve(1, DateTimeOffset.UtcNow);
        var acknowledged = CreateDeviceLinkRequest(ownerId, "acknowledged");
        acknowledged.Approve(1, DateTimeOffset.UtcNow);
        var sameGeneration = CreateDeviceLinkRequest(ownerId, "same-generation");
        sameGeneration.Approve(3, DateTimeOffset.UtcNow);
        var newerGeneration = CreateDeviceLinkRequest(ownerId, "newer-generation");
        newerGeneration.Approve(4, DateTimeOffset.UtcNow);
        var pending = CreateDeviceLinkRequest(ownerId, "pending");
        var alreadyInvalidated = CreateDeviceLinkRequest(ownerId, "already-invalidated");
        alreadyInvalidated.Approve(1, DateTimeOffset.UtcNow);
        var firstInvalidatedAt = DateTimeOffset.FromUnixTimeMilliseconds(
            DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds());
        DomainFixtureHydrator.CancelDeviceLink(alreadyInvalidated, firstInvalidatedAt);
        var foreign = CreateDeviceLinkRequest(otherUserId, "foreign");
        foreign.Approve(1, DateTimeOffset.UtcNow);
        await using (var setup = new KodosiDbContext(options))
        {
            setup.DeviceLinkRequests.AddRange(
                eligible,
                excluded,
                acknowledged,
                sameGeneration,
                newerGeneration,
                pending,
                alreadyInvalidated,
                foreign);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        var acknowledgedAt = DateTimeOffset.UtcNow;
        await using (var acknowledgeContext = new KodosiDbContext(options))
        {
            Assert.Equal(
                DeviceLinkAcknowledgeOutcome.Acknowledged,
                await new DeviceLinkRequestRepository(acknowledgeContext)
                    .AcknowledgeApprovedAsync(
                        acknowledged.Id,
                        ownerId,
                        acknowledged.DeviceId,
                        acknowledgedAt,
                        TestContext.Current.CancellationToken));
        }

        var invalidatedAt = DateTimeOffset.FromUnixTimeMilliseconds(
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());
        await using (var invalidateContext = new KodosiDbContext(options))
        {
            Assert.True(
                await new DeviceLinkRequestRepository(invalidateContext)
                    .InvalidateApprovedBeforeGenerationAsync(
                        ownerId,
                        3,
                        excluded.Id,
                        invalidatedAt,
                        TestContext.Current.CancellationToken) >= 1);
        }

        await using var verify = new KodosiDbContext(options);
        var rows = await verify.DeviceLinkRequests
            .AsNoTracking()
            .Where(request => new[]
            {
                eligible.Id,
                excluded.Id,
                acknowledged.Id,
                sameGeneration.Id,
                newerGeneration.Id,
                pending.Id,
                alreadyInvalidated.Id,
                foreign.Id,
            }.Contains(request.Id))
            .ToDictionaryAsync(
                request => request.Id,
                TestContext.Current.CancellationToken);
        Assert.Equal(invalidatedAt, rows[eligible.Id].CancelledAt);
        Assert.Null(rows[excluded.Id].CancelledAt);
        Assert.NotNull(rows[acknowledged.Id].AcknowledgedAt);
        Assert.Null(rows[acknowledged.Id].CancelledAt);
        Assert.Null(rows[sameGeneration.Id].CancelledAt);
        Assert.Null(rows[newerGeneration.Id].CancelledAt);
        Assert.Null(rows[pending.Id].CancelledAt);
        Assert.Equal(firstInvalidatedAt, rows[alreadyInvalidated.Id].CancelledAt);
        Assert.Null(rows[foreign.Id].CancelledAt);

        var noExclusion = CreateDeviceLinkRequest(ownerId, "no-exclusion");
        noExclusion.Approve(1, DateTimeOffset.UtcNow);
        await using (var setup = new KodosiDbContext(options))
        {
            setup.DeviceLinkRequests.Add(noExclusion);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        var nullExclusionInvalidatedAt = DateTimeOffset.FromUnixTimeMilliseconds(
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());
        await using (var invalidateContext = new KodosiDbContext(options))
        {
            Assert.True(
                await new DeviceLinkRequestRepository(invalidateContext)
                    .InvalidateApprovedBeforeGenerationAsync(
                        ownerId,
                        2,
                        null,
                        nullExclusionInvalidatedAt,
                        TestContext.Current.CancellationToken) >= 1);
        }
        await using var nullExclusionVerify = new KodosiDbContext(options);
        Assert.Equal(
            nullExclusionInvalidatedAt,
            (await nullExclusionVerify.DeviceLinkRequests
                .AsNoTracking()
                .SingleAsync(
                    request => request.Id == noExclusion.Id,
                    TestContext.Current.CancellationToken))
                .CancelledAt);
    }

    private static async Task AssertDeviceLinkTerminalRetentionAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId)
    {
        var cancelled = CreateDeviceLinkRequest(ownerId, "retained-cancelled");
        DomainFixtureHydrator.CancelDeviceLink(cancelled, DateTimeOffset.UtcNow);
        await AssertDeviceLinkRetainedUntilAsync(
            options,
            cancelled,
            static request => request.CancelledAt!.Value);

        var naturallyExpired = DeviceLinkRequest.Create(
            ownerId,
            $"retention-expired-{Guid.NewGuid():N}",
            $"RE-{Guid.NewGuid():N}"[..9],
            $"retention-expired-{Guid.NewGuid():N}",
            "Naturally expired",
            new byte[1184],
            new byte[1952],
            TimeSpan.FromMilliseconds(1), DateTimeOffset.UtcNow);
        await Task.Delay(5, TestContext.Current.CancellationToken);
        await AssertDeviceLinkRetainedUntilAsync(
            options,
            naturallyExpired,
            static request => request.ExpiresAt);

        var approvedExpired = CreateDeviceLinkRequest(ownerId, "retained-approved");
        approvedExpired.Approve(2, DateTimeOffset.UtcNow);
        await AssertDeviceLinkRetainedUntilAsync(
            options,
            approvedExpired,
            static request => request.ResultExpiresAt!.Value);

        var acknowledged = CreateDeviceLinkRequest(ownerId, "retained-acknowledged");
        acknowledged.Approve(2, DateTimeOffset.UtcNow);
        await using (var setup = new KodosiDbContext(options))
        {
            setup.DeviceLinkRequests.Add(acknowledged);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        var acknowledgedAt = DateTimeOffset.FromUnixTimeMilliseconds(
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());
        await using (var acknowledgeContext = new KodosiDbContext(options))
        {
            Assert.Equal(
                DeviceLinkAcknowledgeOutcome.Acknowledged,
                await new DeviceLinkRequestRepository(acknowledgeContext)
                    .AcknowledgeApprovedAsync(
                        acknowledged.Id,
                        ownerId,
                        acknowledged.DeviceId,
                        acknowledgedAt,
                        TestContext.Current.CancellationToken));
        }
        await AssertPersistedDeviceLinkRetainedUntilAsync(
            options,
            acknowledged.Id,
            acknowledged.DeviceCode,
            acknowledgedAt);
    }

    private static async Task AssertDeviceLinkRetainedUntilAsync(
        DbContextOptions<KodosiDbContext> options,
        DeviceLinkRequest request,
        Func<DeviceLinkRequest, DateTimeOffset> terminalAt)
    {
        await using (var setup = new KodosiDbContext(options))
        {
            setup.DeviceLinkRequests.Add(request);
            await setup.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        await using var reload = new KodosiDbContext(options);
        var persisted = await reload.DeviceLinkRequests
            .AsNoTracking()
            .SingleAsync(
                item => item.Id == request.Id,
                TestContext.Current.CancellationToken);
        await AssertPersistedDeviceLinkRetainedUntilAsync(
            options,
            persisted.Id,
            persisted.DeviceCode,
            terminalAt(persisted));
    }

    private static async Task AssertPersistedDeviceLinkRetainedUntilAsync(
        DbContextOptions<KodosiDbContext> options,
        Guid requestId,
        string deviceCode,
        DateTimeOffset terminalAt)
    {
        await using (var retainedContext = new KodosiDbContext(options))
        {
            var repository = new DeviceLinkRequestRepository(retainedContext);
            _ = await repository.DeleteStaleAsync(
                terminalAt.AddHours(24).AddTicks(-10),
                TestContext.Current.CancellationToken);
            Assert.NotNull(await repository.GetByDeviceCodeAsync(
                deviceCode,
                TestContext.Current.CancellationToken));
        }
        await using (var boundaryContext = new KodosiDbContext(options))
        {
            var repository = new DeviceLinkRequestRepository(boundaryContext);
            _ = await repository.DeleteStaleAsync(
                terminalAt.AddHours(24),
                TestContext.Current.CancellationToken);
            Assert.Null(await boundaryContext.DeviceLinkRequests
                .AsNoTracking()
                .SingleOrDefaultAsync(
                    request => request.Id == requestId,
                    TestContext.Current.CancellationToken));
        }
    }

    private static DeviceLinkRequest CreateDeviceLinkRequest(
        UserId userId,
        string label) =>
        DeviceLinkRequest.Create(
            userId,
            $"{label}-device-code-{Guid.NewGuid():N}",
            $"DL-{Guid.NewGuid():N}"[..9],
            $"{label}-device-{Guid.NewGuid():N}",
            label,
            new byte[1184],
            new byte[1952],
            TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow);

    private static async Task AssertDeviceLinkAcknowledgementAsync(
        DbContextOptions<KodosiDbContext> options,
        UserId ownerId)
    {
        var request = DeviceLinkRequest.Create(
            ownerId,
            $"device-code-{Guid.NewGuid()}",
            $"ACK-{Guid.NewGuid():N}"[..12],
            $"device-{Guid.NewGuid()}",
            "Laptop",
            new byte[1184],
            new byte[1952],
            TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow);
        request.Approve(2, DateTimeOffset.UtcNow);
        await using (var context = new KodosiDbContext(options))
        {
            context.DeviceLinkRequests.Add(request);
            await context.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using var acknowledgeContext = new KodosiDbContext(options);
        var repository = new DeviceLinkRequestRepository(acknowledgeContext);
        Assert.Equal(
            DeviceLinkAcknowledgeOutcome.Acknowledged,
            await repository.AcknowledgeApprovedAsync(
                request.Id,
                ownerId,
                request.DeviceId,
                DateTimeOffset.UtcNow,
                TestContext.Current.CancellationToken));
        Assert.Equal(
            DeviceLinkAcknowledgeOutcome.AlreadyAcknowledged,
            await repository.AcknowledgeApprovedAsync(
                request.Id,
                ownerId,
                request.DeviceId,
                DateTimeOffset.UtcNow,
                TestContext.Current.CancellationToken));
        Assert.Equal(
            DeviceLinkAcknowledgeOutcome.Unavailable,
            await repository.AcknowledgeApprovedAsync(
                request.Id,
                UserId.New(),
                request.DeviceId,
                DateTimeOffset.UtcNow,
                TestContext.Current.CancellationToken));

        var invalidatedAt = new DateTimeOffset(2026, 8, 20, 12, 0, 0, TimeSpan.Zero);
        var invalidated = DeviceLinkRequest.Create(
            ownerId,
            $"device-code-{Guid.NewGuid()}",
            $"INV-{Guid.NewGuid():N}"[..12],
            $"device-{Guid.NewGuid()}",
            "Tablet",
            new byte[1184],
            new byte[1952],
            TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow);
        invalidated.Approve(3, DateTimeOffset.UtcNow);
        DomainFixtureHydrator.CancelDeviceLink(invalidated, invalidatedAt);
        await using (var insertContext = new KodosiDbContext(options))
        {
            insertContext.DeviceLinkRequests.Add(invalidated);
            await insertContext.SaveChangesAsync(TestContext.Current.CancellationToken);
        }

        await using (var retainedContext = new KodosiDbContext(options))
        {
            var retainedRepository = new DeviceLinkRequestRepository(retainedContext);
            _ = await retainedRepository.DeleteStaleAsync(
                invalidatedAt.AddHours(24).AddTicks(-TimeSpan.TicksPerMicrosecond),
                TestContext.Current.CancellationToken);
            Assert.NotNull(await retainedRepository.GetByDeviceCodeAsync(
                invalidated.DeviceCode,
                TestContext.Current.CancellationToken));
            Assert.Equal(
                DeviceLinkAcknowledgeOutcome.Invalidated,
                await retainedRepository.AcknowledgeApprovedAsync(
                    invalidated.Id,
                    ownerId,
                    invalidated.DeviceId,
                    invalidatedAt.AddHours(23),
                    TestContext.Current.CancellationToken));
        }

        await using (var expiredContext = new KodosiDbContext(options))
        {
            var expiredRepository = new DeviceLinkRequestRepository(expiredContext);
            _ = await expiredRepository.DeleteStaleAsync(
                invalidatedAt.AddHours(24),
                TestContext.Current.CancellationToken);
            Assert.Null(await expiredRepository.GetByDeviceCodeAsync(
                invalidated.DeviceCode,
                TestContext.Current.CancellationToken));
        }
    }
}
