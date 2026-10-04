using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading.RateLimiting;
using Kodosi;
using Kodosi.Accounts;
using Kodosi.Admission;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Friends;
using Kodosi.Missions;
using Kodosi.TerminalConnections;
using Kodosi.Security;
using Kodosi.Sessions;
using Microsoft.AspNetCore.Http.Features;
using Microsoft.AspNetCore.HttpOverrides;
using Microsoft.AspNetCore.RateLimiting;
using Microsoft.EntityFrameworkCore;
using Npgsql;

var builder = WebApplication.CreateBuilder(args);
builder.Services.AddSingleton(services => NpgsqlDataSource.Create(services.GetRequiredService<IConfiguration>().GetConnectionString("Kodosi")
    ?? throw new InvalidOperationException("ConnectionStrings:Kodosi is required.")));
builder.Services.AddDbContext<KodosiDbContext>((services, options) => options.UseNpgsql(services.GetRequiredService<NpgsqlDataSource>()));
builder.Services.AddSingleton(TimeProvider.System);
builder.Services.AddSingleton<SignatureVerifier>();
builder.Services.AddSingleton<DeviceCertificateParser>();
builder.Services.AddSingleton<SignedDeviceListParser>();
builder.Services.AddSingleton<AccountGate>();
builder.Services.AddSingleton<DeviceSessions>();
builder.Services.AddSingleton<ServerMetrics>();
builder.Services.AddSingleton<ConnectionDirectory>();
builder.Services.AddSingleton<ConnectionLease>();
builder.Services.AddHostedService(services => services.GetRequiredService<ConnectionLease>());
builder.Services.AddHostedService<ConnectionMaintenance>();
builder.Services.AddHostedService<PublicationCleanup>();
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
builder.WebHost.ConfigureKestrel(options => options.Limits.MaxRequestBodySize = Limits.HttpBodyBytes);
builder.Services.AddIdentityAuthentication(builder.Configuration);
builder.Services.AddRateLimiter(options =>
{
    options.RejectionStatusCode = 429;
    options.GlobalLimiter = PartitionedRateLimiter.Create<HttpContext, string>(context => RateLimitPartition.GetFixedWindowLimiter(
        context.User.FindFirst("sub")?.Value ?? context.Connection.RemoteIpAddress?.ToString() ?? "unknown",
        _ => new FixedWindowRateLimiterOptions { PermitLimit = 1200, Window = TimeSpan.FromMinutes(1), QueueLimit = 0 }));
    options.AddPolicy("challenge", context => RateLimitPartition.GetFixedWindowLimiter(
        context.User.FindFirst("sub")?.Value ?? "unknown", _ => new FixedWindowRateLimiterOptions { PermitLimit = 600, Window = TimeSpan.FromMinutes(1), QueueLimit = 0 }));
    options.AddPolicy("enrollment", context => RateLimitPartition.GetFixedWindowLimiter(
        context.User.FindFirst("sub")?.Value ?? "unknown", _ => new FixedWindowRateLimiterOptions { PermitLimit = 20, Window = TimeSpan.FromMinutes(1), QueueLimit = 0 }));
    options.AddPolicy("socket", context => RateLimitPartition.GetFixedWindowLimiter(
        context.User.FindFirst("sub")?.Value ?? "unknown", _ => new FixedWindowRateLimiterOptions { PermitLimit = 60, Window = TimeSpan.FromMinutes(1), QueueLimit = 0 }));
});
var proxies = builder.Configuration.GetSection("Proxy:Addresses").Get<string[]>() ?? [];
builder.Services.Configure<ForwardedHeadersOptions>(options =>
{
    options.ForwardedHeaders = ForwardedHeaders.XForwardedFor | ForwardedHeaders.XForwardedProto;
    foreach (var proxy in proxies) options.KnownProxies.Add(System.Net.IPAddress.Parse(proxy));
});
var app = builder.Build();
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
app.Services.GetRequiredService<ServerMetrics>().Observe(app.Services.GetRequiredService<ConnectionDirectory>().Count);
app.Use(async (context, next) =>
{
    try { await next(context); }
    catch (ApiException error) when (!context.Response.HasStarted)
    { context.Response.StatusCode = error.Status; await context.Response.WriteAsJsonAsync(new { error = error.Message }); }
    catch (Exception error) when (!context.Response.HasStarted && error is DeviceCertificateFormatException or SignedDeviceListFormatException or JsonException)
    { context.Response.StatusCode = 400; await context.Response.WriteAsJsonAsync(new { error = "Invalid request encoding." }); }
    catch (DbUpdateConcurrencyException) when (!context.Response.HasStarted)
    { context.Response.StatusCode = 409; await context.Response.WriteAsJsonAsync(new { error = "The current state changed; refresh before retrying." }); }
    catch (DbUpdateException) when (!context.Response.HasStarted)
    { context.Response.StatusCode = 409; await context.Response.WriteAsJsonAsync(new { error = "The requested change conflicts with current state." }); }
});
app.UseAuthentication();
app.UseAuthorization();
app.UseRateLimiter();
var websocket = new WebSocketOptions { KeepAliveInterval = TimeSpan.FromSeconds(30) };
foreach (var origin in builder.Configuration.GetSection("WebSockets:AllowedOrigins").Get<string[]>() ?? []) websocket.AllowedOrigins.Add(origin);
app.UseWebSockets(websocket);
app.MapGet("/health/live", () => Results.Ok(new { status = "ok", apiContractVersion = 20, authContractVersion = 1 })).DisableRateLimiting();
app.MapGet("/health/ready", async (KodosiDbContext db, CancellationToken ct) =>
    await db.Database.CanConnectAsync(ct) ? Results.Ok(new { status = "ok", apiContractVersion = 20, authContractVersion = 1 }) : Results.StatusCode(503))
    .DisableRateLimiting();
var api = app.MapGroup("").AddEndpointFilter<AdmissionFilter>();
api.MapAccounts(); api.MapDevices(); api.MapFriends(); api.MapSessions(); api.MapMissions();
app.MapTerminalConnections();
app.Lifetime.ApplicationStopping.Register(() => app.Services.GetRequiredService<ConnectionDirectory>().StopAll());
app.Run();

public partial class Program;
