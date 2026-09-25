using System.Security.Cryptography;
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
builder.Services.AddSingleton<AdmissionGate>();
builder.Services.AddSingleton<ConnectionDirectory>();
builder.Services.AddHostedService<ConnectionLease>();
builder.Services.AddHostedService<ConnectionMaintenance>();
builder.Services.AddHostedService<PublicationCleanup>();
builder.Services.AddScoped<CurrentUser>();
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
var app = builder.Build();
await using (var scope = app.Services.CreateAsyncScope())
    await DatabaseSetup.InitializeAsync(scope.ServiceProvider.GetRequiredService<KodosiDbContext>(), app.Lifetime.ApplicationStopping);
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
app.Use(async (context, next) =>
{
    if (context.Request.Path.StartsWithSegments("/api"))
    {
        context.Request.EnableBuffering(bufferThreshold: 32 * 1024, bufferLimit: Limits.HttpBodyBytes + 16 * 1024);
        using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        var buffer = new byte[16 * 1024];
        var total = 0;
        int count;
        while ((count = await context.Request.Body.ReadAsync(buffer, context.RequestAborted)) > 0)
        {
            total += count;
            if (total > Limits.HttpBodyBytes) throw new ApiException(413, "Request body is too large.");
            hash.AppendData(buffer, 0, count);
        }
        context.Items["bodySha256"] = Convert.ToHexStringLower(hash.GetHashAndReset());
        context.Request.Body.Position = 0;
    }
    await next(context);
});
var websocket = new WebSocketOptions { KeepAliveInterval = TimeSpan.FromSeconds(30) };
foreach (var origin in builder.Configuration.GetSection("WebSockets:AllowedOrigins").Get<string[]>() ?? []) websocket.AllowedOrigins.Add(origin);
app.UseWebSockets(websocket);
app.MapGet("/health/live", () => Results.Ok(new { status = "ok", apiContractVersion = 16, authContractVersion = 1 }));
app.MapGet("/health/ready", async (KodosiDbContext db, CancellationToken ct) =>
    await db.Database.CanConnectAsync(ct) ? Results.Ok(new { status = "ok", apiContractVersion = 16, authContractVersion = 1 }) : Results.StatusCode(503));
var api = app.MapGroup("").AddEndpointFilter<AdmissionFilter>();
api.MapAccounts(); api.MapDevices(); api.MapFriends(); api.MapSessions(); api.MapMissions();
app.MapTerminalConnections();
app.Lifetime.ApplicationStopping.Register(() => app.Services.GetRequiredService<ConnectionDirectory>().StopAll());
app.Run();

public partial class Program;
