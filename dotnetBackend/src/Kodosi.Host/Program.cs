using Kodosi.Host;
using Kodosi.Host.Configuration;
using Kodosi.Host.DependencyInjection;
using Kodosi.Host.Health;
using Kodosi.Host.Middleware;

var builder = WebApplication.CreateBuilder(args);
var requestLimits = builder.Configuration.GetSection(RequestLimitsOptions.SectionName).Get<RequestLimitsOptions>() ?? new();
var webSocketSecurity = builder.Configuration.GetSection(WebSocketSecurityOptions.SectionName).Get<WebSocketSecurityOptions>() ?? new();
builder.Logging.AddJsonConsole(options => options.IncludeScopes = true);
builder.WebHost.ConfigureKestrel(options =>
{
    options.Limits.MaxRequestBodySize = requestLimits.MaxRequestBodySizeBytes;
});

builder.Services
    .AddApplication()
    .AddInfrastructure(builder.Configuration)
    .AddHostAdapters(builder.Configuration);

var app = builder.Build();

app.Services.GetRequiredService<PostQuantumCryptoReadiness>().RunOrThrow();

app.UseForwardedHeaders();
if (!app.Environment.IsDevelopment())
{
    app.UseHsts();
    app.UseHttpsRedirection();
}

app.UseMiddleware<ExceptionMappingMiddleware>();
app.UseAuthentication();
app.UseMiddleware<AuthenticatedUserSynchronizationMiddleware>();
app.UseRateLimiter();
app.UseAuthorization();
app.UseWebSockets(new WebSocketOptions
{
    KeepAliveInterval = TimeSpan.FromSeconds(webSocketSecurity.KeepAliveIntervalSeconds),
});

app.MapAllEndpoints();






await app.Services.GetRequiredService<RelaySingletonLeaseHostedService>()
    .StartAsync(app.Lifetime.ApplicationStopping);

await DatabaseInitializerHostedService.InitializeAsync(app.Services);

app.Run();
