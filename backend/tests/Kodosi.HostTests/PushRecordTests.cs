using Kodosi.Data;
using Kodosi.Push;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging.Abstractions;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class PushRecordTests(PostgresFixture postgres)
{
    [Fact]
    public async Task APushStaysSavedForATerminalThatHasNotEndedUntilItsStateChangesOrItIsTakenBack()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("ana");
        var now = DateTimeOffset.FromUnixTimeMilliseconds(DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());
        Session Terminal(bool ended) => new()
        {
            Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), OwnerUserId = owner.User.Id, HostDeviceId = owner.Device.Id,
            HostName = "Studio", Name = "Terminal", Ended = ended, CreatedAt = now, ExpiresAt = now.AddDays(1),
        };
        var (waiting, ended) = (Terminal(ended: false), Terminal(ended: true));
        store.Db.Sessions.AddRange(waiting, ended);
        await store.Db.SaveChangesAsync(ct);
        await using var services = new ServiceCollection()
            .AddDbContext<KodosiDbContext>(options => options.UseNpgsql(store.Db.Database.GetConnectionString())).BuildServiceProvider();
        var records = new PushRecords(services.GetRequiredService<IServiceScopeFactory>(), NullLogger<PushRecords>.Instance);
        PushRecord Record(Guid terminal, string state) =>
            new(owner.User.Id, terminal, new TerminalStatus("claude", state, state == "blocked" ? "question" : null, 1, true), now);
        var gone = Guid.CreateVersion7();

        Assert.True(await records.SaveAsync(new Dictionary<Guid, PushRecord?>
        {
            [waiting.Id] = Record(waiting.Id, "blocked"), [ended.Id] = Record(ended.Id, "done"), [gone] = Record(gone, "error"),
        }, ct));
        Assert.Equal([Record(waiting.Id, "blocked")], await records.LoadAsync(ct));
        Assert.True(await records.SaveAsync(new Dictionary<Guid, PushRecord?> { [waiting.Id] = Record(waiting.Id, "done") }, ct));
        Assert.Equal([Record(waiting.Id, "done")], await records.LoadAsync(ct));
        Assert.True(await records.SaveAsync(new Dictionary<Guid, PushRecord?> { [waiting.Id] = null }, ct));
        Assert.Empty(await records.LoadAsync(ct));
    }
}
