using System.Text.Json.Serialization;
using Kodosi;
using Kodosi.Accounts;
using Kodosi.Admission;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Friends;
using Kodosi.Missions;
using Kodosi.Plans;
using Kodosi.Security;
using Kodosi.Sessions;
using Kodosi.TerminalConnections;
using Microsoft.AspNetCore.HttpOverrides;
using Microsoft.EntityFrameworkCore;
using Npgsql;

var builder = WebApplication.CreateBuilder(args);
builder.Services.AddSingleton(services => DatabaseConnection.From(services.GetRequiredService<IConfiguration>()));
builder.Services.AddSingleton(services => NpgsqlDataSource.Create(services.GetRequiredService<DatabaseConnection>().ConnectionString));
builder.Services.AddDbContextPool<KodosiDbContext>((services, options) => options.UseNpgsql(services.GetRequiredService<NpgsqlDataSource>()));
builder.Services.AddSingleton(TimeProvider.System);
builder.Services.AddSingleton(services => PlanCatalog.From(services.GetRequiredService<IConfiguration>()));
builder.Services.AddSingleton(services => AccountDeletionSettings.From(services.GetRequiredService<IConfiguration>()));
builder.Services.AddSingleton<AccountGate>();
builder.Services.AddSingleton<DeviceSessions>();
builder.Services.AddSingleton<ConnectionDirectory>();
builder.Services.AddSingleton<ServerMetrics>();
builder.Services.AddSingleton<ConnectionLease>();
builder.Services.AddHostedService(services => services.GetRequiredService<ConnectionLease>());
builder.Services.AddHostedService<ConnectionWatchdog>();
builder.Services.AddHostedService<ConnectionMaintenance>();
builder.Services.AddHostedService<TerminalPresence>();
builder.Services.AddHostedService<DataCleanup>();
builder.Services.AddHostedService<ServerReport>();
builder.Services.AddHttpClient(nameof(AccountDeletionFeed), client => client.Timeout = TimeSpan.FromSeconds(10));
builder.Services.AddHostedService<AccountDeletionFeed>();
builder.Services.AddScoped<CurrentUser>();
builder.Services.AddScoped<AccountService>();
builder.Services.AddScoped<DeviceService>();
builder.Services.AddScoped<FriendService>();
builder.Services.AddScoped<MissionService>();
builder.Services.AddScoped<SessionService>();
builder.Services.ConfigureHttpJsonOptions(options =>
{
    options.SerializerOptions.Converters.Add(new CanonicalGuidConverter());
    options.SerializerOptions.RespectNullableAnnotations = true;
    options.SerializerOptions.RespectRequiredConstructorParameters = true;
    options.SerializerOptions.UnmappedMemberHandling = JsonUnmappedMemberHandling.Disallow;
    options.SerializerOptions.MaxDepth = 32;
});
builder.WebHost.ConfigureKestrel(options =>
{
    options.AddServerHeader = false;
    options.Limits.MaxRequestBodySize = Limits.HttpBodyBytes;
});
builder.Services.AddIdentityAuthentication(builder.Configuration);
builder.Services.AddApiRateLimits();
var proxies = builder.Configuration.GetSection("Proxy:Addresses").Get<string[]>() ?? [];
builder.Services.Configure<ForwardedHeadersOptions>(options =>
{
    options.ForwardedHeaders = ForwardedHeaders.XForwardedFor | ForwardedHeaders.XForwardedProto;
    foreach (var proxy in proxies) options.KnownProxies.Add(System.Net.IPAddress.Parse(proxy));
});
var app = builder.Build();
app.Services.GetRequiredService<PlanCatalog>();
if (args is ["migrate"])
{
    await using var migration = app.Services.CreateAsyncScope();
    await DatabaseSetup.MigrateAsync(migration.ServiceProvider.GetRequiredService<KodosiDbContext>(), CancellationToken.None);
    return;
}
if (proxies.Length > 0) app.UseForwardedHeaders();
await app.Services.GetRequiredService<ConnectionLease>().AcquireAsync(app.Lifetime.ApplicationStopping);
await using (var scope = app.Services.CreateAsyncScope())
    await DatabaseSetup.RequireCurrentAsync(scope.ServiceProvider.GetRequiredService<KodosiDbContext>(), app.Lifetime.ApplicationStopping);
app.UseMiddleware<ApiErrors>();
app.UseAuthentication();
app.UseAuthorization();
app.UseRateLimiter();
app.UseWebSockets(new WebSocketOptions { KeepAliveInterval = TimeSpan.FromSeconds(30) });
app.MapGet("/health/live", () => Results.Ok(ContractVersions.Report())).DisableRateLimiting();
app.MapGet("/health/ready", async (KodosiDbContext db, ConnectionLease lease, CancellationToken ct) =>
    lease.Held && await db.Database.CanConnectAsync(ct) ? Results.Ok(ContractVersions.Report()) : Results.StatusCode(StatusCodes.Status503ServiceUnavailable))
    .DisableRateLimiting();
var api = app.MapGroup("").AddEndpointFilter<AdmissionFilter>();
api.MapAccounts(); api.MapDevices(); api.MapFriends(); api.MapSessions(); api.MapMissions();
app.MapTerminalConnections();
app.Run();

public partial class Program;
