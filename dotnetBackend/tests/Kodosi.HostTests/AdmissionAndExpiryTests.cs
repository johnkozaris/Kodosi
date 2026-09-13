using System.Net;
using System.Text.Json;
using Kodosi.Data;
using Kodosi.Realtime;
using Kodosi.Sessions;
using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Hosting;
using Microsoft.AspNetCore.Http;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Microsoft.Extensions.Logging.Abstractions;
using Xunit;
using static Kodosi.HostTests.BackendApplication;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class AdmissionAndExpiryTests(PostgresFixture postgres)
{
    [Fact]
    public async Task StalledLargeResponseDoesNotHoldAdmissionForAnotherRequest()
    {
        var stall = new ResponseStall();
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken),
            services => services.AddSingleton<IStartupFilter>(stall));
        using var owner = await app.EnrollAsync("owner");
        await using (var scope = app.Services.CreateAsyncScope())
        {
            var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            for (var i = 0; i < 128; i++) db.Sessions.Add(new Session
            {
                Id = Guid.CreateVersion7(),
                IncarnationId = Guid.CreateVersion7(),
                OwnerUserId = owner.Fixture.UserId,
                HostDeviceId = owner.Fixture.DeviceId,
                HostName = new string('h', 128),
                Name = new string('n', 128),
                CreatedAt = DateTimeOffset.UtcNow,
                ExpiresAt = DateTimeOffset.UtcNow.AddMinutes(2)
            });
            await db.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        using var slowRequest = await SignedAsync(owner, HttpMethod.Get, "/api/sessions");
        slowRequest.Headers.Add("X-Test-Stall-Response", "1");
        using var otherRequest = await SignedAsync(owner, HttpMethod.Get, "/api/me/devices");
        var slow = owner.Client.SendAsync(slowRequest, TestContext.Current.CancellationToken);
        try
        {
            await stall.Started.Task.WaitAsync(TimeSpan.FromSeconds(10), TestContext.Current.CancellationToken);
            Assert.False(slow.IsCompleted);
            using var other = await owner.Client.SendAsync(otherRequest, TestContext.Current.CancellationToken)
                .WaitAsync(TimeSpan.FromSeconds(5), TestContext.Current.CancellationToken);
            Assert.Equal(HttpStatusCode.OK, other.StatusCode);
        }
        finally { stall.Release.TrySetResult(); }
        using var result = await slow;
        var bytes = await result.Content.ReadAsByteArrayAsync(TestContext.Current.CancellationToken);
        Assert.True(bytes.Length > 64 * 1024);
        Assert.Equal(128, JsonDocument.Parse(bytes).RootElement.GetArrayLength());
    }

    [Fact]
    public async Task OfflineExpiryIsBoundedAllowsRestartGraceAndPreservesLiveSessions()
    {
        var clock = new ManualClock();
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken), services =>
        {
            services.RemoveAll<TimeProvider>(); services.AddSingleton<TimeProvider>(clock);
        });
        using var owner = await app.EnrollAsync("owner");
        var scopes = app.Services.GetRequiredService<IServiceScopeFactory>();
        var gate = app.Services.GetRequiredService<AdmissionGate>();
        var relay = app.Services.GetRequiredService<RelayDirectory>();
        using var cleanup = new PublicationCleanup(scopes, gate, relay, clock, NullLogger<PublicationCleanup>.Instance);
        var abandoned = Enumerable.Range(0, 129).Select(_ => new Session
        {
            Id = Guid.CreateVersion7(),
            IncarnationId = Guid.CreateVersion7(),
            OwnerUserId = owner.Fixture.UserId,
            HostDeviceId = owner.Fixture.DeviceId,
            HostName = "Host",
            Name = "Terminal",
            CreatedAt = clock.GetUtcNow().AddMinutes(-5),
            ExpiresAt = clock.GetUtcNow().AddMinutes(-1)
        }).ToArray();
        var online = new Session
        {
            Id = Guid.CreateVersion7(),
            IncarnationId = Guid.CreateVersion7(),
            OwnerUserId = owner.Fixture.UserId,
            HostDeviceId = owner.Fixture.DeviceId,
            HostName = "Host",
            Name = "Live",
            ExpiresAt = clock.GetUtcNow().AddMinutes(-1),
            CreatedAt = clock.GetUtcNow()
        };
        var ended = new Session
        {
            Id = Guid.CreateVersion7(),
            IncarnationId = Guid.CreateVersion7(),
            OwnerUserId = owner.Fixture.UserId,
            HostDeviceId = owner.Fixture.DeviceId,
            HostName = "Host",
            Name = "Ended",
            Ended = true,
            ExpiresAt = clock.GetUtcNow().AddMinutes(-1),
            CreatedAt = clock.GetUtcNow()
        };
        await using (var scope = scopes.CreateAsyncScope())
        {
            var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            db.Sessions.AddRange(abandoned); db.Sessions.AddRange(online, ended);
            await db.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        await using var host = new SocketPeer(new FakeSocket(), owner.Fixture.UserId, owner.Fixture.DeviceId, Guid.CreateVersion7().ToString());
        relay.RegisterHost(online, host);
        await cleanup.SweepAsync(TestContext.Current.CancellationToken);
        await using (var scope = scopes.CreateAsyncScope())
            Assert.Equal(131, await scope.ServiceProvider.GetRequiredService<KodosiDbContext>().Sessions.CountAsync(TestContext.Current.CancellationToken));
        clock.Advance(TimeSpan.FromSeconds(121));
        await cleanup.SweepAsync(TestContext.Current.CancellationToken);
        await using (var scope = scopes.CreateAsyncScope())
        {
            var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            Assert.Equal(3, await db.Sessions.CountAsync(TestContext.Current.CancellationToken));
            Assert.True(await db.Sessions.AnyAsync(x => x.Id == online.Id, TestContext.Current.CancellationToken));
        }
        await cleanup.SweepAsync(TestContext.Current.CancellationToken);
        await using (var scope = scopes.CreateAsyncScope())
        {
            var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            Assert.Single(await db.Sessions.ToListAsync(TestContext.Current.CancellationToken));
            Assert.False(await db.Sessions.AnyAsync(x => x.Id == ended.Id, TestContext.Current.CancellationToken));
        }
        var lost = abandoned[0];
        var recreated = await CallAsync(owner, HttpMethod.Post, "/api/sessions", new
        {
            id = lost.Id,
            incarnationId = lost.IncarnationId,
            name = "Restored local terminal",
            hostDeviceId = owner.Fixture.DeviceId,
            hostName = "Host",
            roomId = (Guid?)null
        });
        Assert.False(recreated.GetProperty("ready").GetBoolean());
        Assert.Empty(recreated.GetProperty("sharedWith").EnumerateArray());
        Assert.True(relay.HostOnline(online.Id));
    }

    private sealed class ManualClock : TimeProvider
    {
        private DateTimeOffset now = DateTimeOffset.UtcNow;
        public override DateTimeOffset GetUtcNow() => now;
        public void Advance(TimeSpan duration) => now += duration;
    }

    private sealed class ResponseStall : IStartupFilter
    {
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public Action<IApplicationBuilder> Configure(Action<IApplicationBuilder> next) => app =>
        {
            app.Use(async (context, run) =>
            {
                if (context.Request.Headers.ContainsKey("X-Test-Stall-Response"))
                    context.Response.Body = new StalledStream(context.Response.Body, this);
                await run(context);
            });
            next(app);
        };
    }

    private sealed class StalledStream(Stream inner, ResponseStall stall) : Stream
    {
        public override bool CanRead => false;
        public override bool CanSeek => false;
        public override bool CanWrite => true;
        public override long Length => throw new NotSupportedException();
        public override long Position { get => throw new NotSupportedException(); set => throw new NotSupportedException(); }
        public override void Flush() => inner.Flush();
        public override Task FlushAsync(CancellationToken ct) => inner.FlushAsync(ct);
        public override int Read(byte[] buffer, int offset, int count) => throw new NotSupportedException();
        public override long Seek(long offset, SeekOrigin origin) => throw new NotSupportedException();
        public override void SetLength(long value) => throw new NotSupportedException();
        public override void Write(byte[] buffer, int offset, int count) => throw new NotSupportedException();
        public override Task WriteAsync(byte[] buffer, int offset, int count, CancellationToken ct) => WriteAsync(buffer.AsMemory(offset, count), ct).AsTask();
        public override async ValueTask WriteAsync(ReadOnlyMemory<byte> buffer, CancellationToken ct = default)
        {
            stall.Started.TrySetResult();
            await stall.Release.Task.WaitAsync(ct);
            await inner.WriteAsync(buffer, ct);
        }
    }
}
