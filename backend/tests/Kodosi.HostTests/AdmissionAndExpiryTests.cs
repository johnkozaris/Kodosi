using System.Net;
using System.Text.Json;
using Kodosi.Admission;
using Kodosi.Data;
using Kodosi.TerminalConnections;
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
        using var otherRequest = await SignedAsync(owner, HttpMethod.Get, "/api/devices/link/requests");
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
    public async Task AChangeWaitsOnlyForAnotherChangeOfTheSameAccount()
    {
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        using var owner = await app.EnrollAsync("owner");
        using var other = await app.EnrollAsync("other");
        using var read = await SignedAsync(owner, HttpMethod.Get, "/api/sessions");
        using var change = await SignedAsync(owner, HttpMethod.Post, "/api/missions", new { id = Guid.CreateVersion7(), name = "Project" });
        using var separate = await SignedAsync(other, HttpMethod.Post, "/api/missions", new { id = Guid.CreateVersion7(), name = "Other" });
        var changing = await app.Services.GetRequiredService<AccountGate>().EnterAsync(owner.Fixture.UserId, TestContext.Current.CancellationToken);
        Task<HttpResponseMessage> pending;
        try
        {
            using var listed = await owner.Client.SendAsync(read, TestContext.Current.CancellationToken)
                .WaitAsync(TimeSpan.FromSeconds(5), TestContext.Current.CancellationToken);
            Assert.Equal(HttpStatusCode.OK, listed.StatusCode);
            pending = owner.Client.SendAsync(change, TestContext.Current.CancellationToken);
            using var made = await other.Client.SendAsync(separate, TestContext.Current.CancellationToken)
                .WaitAsync(TimeSpan.FromSeconds(5), TestContext.Current.CancellationToken);
            Assert.Equal(HttpStatusCode.OK, made.StatusCode);
            Assert.False(pending.IsCompleted);
        }
        finally { changing.Dispose(); }
        using var changed = await pending;
        Assert.Equal(HttpStatusCode.OK, changed.StatusCode);
    }

    [Fact]
    public async Task OfflineTerminalsKeepTheirRecordAndSharingAndOnlyEndedOnesAreRemoved()
    {
        var clock = new ManualClock();
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken), services =>
        {
            services.RemoveAll<TimeProvider>(); services.AddSingleton<TimeProvider>(clock);
        });
        using var owner = await app.EnrollAsync("owner");
        using var friend = await app.EnrollAsync("friend");
        var scopes = app.Services.GetRequiredService<IServiceScopeFactory>();
        using var cleanup = new PublicationCleanup(scopes, clock, NullLogger<PublicationCleanup>.Instance);
        Session Terminal(string name, bool ended) => new()
        {
            Id = Guid.CreateVersion7(),
            IncarnationId = Guid.CreateVersion7(),
            OwnerUserId = owner.Fixture.UserId,
            HostDeviceId = owner.Fixture.DeviceId,
            HostName = "Host",
            Name = name,
            Ended = ended,
            ExpiresAt = clock.GetUtcNow().AddMinutes(2),
            CreatedAt = clock.GetUtcNow()
        };
        var offline = Terminal("Offline", ended: false);
        var ended = Terminal("Ended", ended: true);
        await using (var scope = scopes.CreateAsyncScope())
        {
            var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            db.Sessions.AddRange(offline, ended);
            db.SessionMembers.Add(new SessionMember { SessionId = offline.Id, UserId = friend.Fixture.UserId });
            await db.SaveChangesAsync(TestContext.Current.CancellationToken);
        }
        await cleanup.SweepAsync(TestContext.Current.CancellationToken);
        await using (var scope = scopes.CreateAsyncScope())
            Assert.Equal(2, await scope.ServiceProvider.GetRequiredService<KodosiDbContext>().Sessions.CountAsync(TestContext.Current.CancellationToken));
        clock.Advance(TimeSpan.FromDays(30));
        await cleanup.SweepAsync(TestContext.Current.CancellationToken);
        await using (var scope = scopes.CreateAsyncScope())
        {
            var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            Assert.Equal(offline.Id, (await db.Sessions.SingleAsync(TestContext.Current.CancellationToken)).Id);
            Assert.Equal(friend.Fixture.UserId, (await db.SessionMembers.SingleAsync(TestContext.Current.CancellationToken)).UserId);
        }
    }

    [Fact]
    public async Task RequestsWithNoTokenCannotBlockTheHealthCheckOrMakeTheServerReadALargeBody()
    {
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        using var client = app.CreateClient();
        for (var i = 0; i < 1250; i++)
        {
            using var probe = await client.GetAsync("/health/ready", TestContext.Current.CancellationToken);
            Assert.Equal(System.Net.HttpStatusCode.OK, probe.StatusCode);
        }
        using var body = new ByteArrayContent(new byte[Limits.HttpBodyBytes + 64 * 1024]);
        body.Headers.ContentType = new System.Net.Http.Headers.MediaTypeHeaderValue("application/json");
        using var unknown = await client.PostAsync("/api/not-a-route", body, TestContext.Current.CancellationToken);
        Assert.Equal(System.Net.HttpStatusCode.NotFound, unknown.StatusCode);
    }

    [Fact]
    public async Task SocketEndsWithItsTokenUnlessTheDeviceRenewsItWithANewerOne()
    {
        await using var app = new BackendApplication(await postgres.CreateDatabaseAsync(TestContext.Current.CancellationToken));
        using var renewing = await app.EnrollAsync("renewing");
        using var silent = await app.EnrollAsync("silent");
        var brief = TimeSpan.FromSeconds(5);
        var (kept, _) = await app.ConnectAsync(renewing with { Token = app.Token("renewing", lifetime: brief) }, "events");
        var (ended, _) = await app.ConnectAsync(silent with { Token = app.Token("silent", lifetime: brief) }, "events");
        await CallAsync(renewing, HttpMethod.Post, "/api/me/connections/renew");
        await CallAsync(silent, HttpMethod.Get, "/api/sessions");
        await Task.Delay(brief + TimeSpan.FromSeconds(1), TestContext.Current.CancellationToken);
        await SendAsync(kept, new { type = "ping" });
        Assert.Equal("pong", (await JsonAsync(kept)).GetProperty("type").GetString());
        await Assert.ThrowsAnyAsync<Exception>(() => JsonAsync(ended));
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
