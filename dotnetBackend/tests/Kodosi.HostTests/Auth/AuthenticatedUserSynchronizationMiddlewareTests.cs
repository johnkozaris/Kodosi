using System.Security.Claims;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Auth;
using Kodosi.Host.Configuration;
using Kodosi.Host.Middleware;
using Microsoft.AspNetCore.Http;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Options;

namespace Kodosi.HostTests;

public sealed class AuthenticatedUserSynchronizationMiddlewareTests
{
    [Fact]
    public async Task Synchronizes_Complete_Typed_Profile_And_Overwrites_Canonical_Claims()
    {
        var primaryIdentity = new AuthenticatedExternalIdentity(
            "apple",
            "https://appleid.apple.com",
            "apple-subject");
        var brokerIdentity = new AuthenticatedExternalIdentity(
            "authentik",
            "https://auth.kodosi.com/application/o/kodosi/",
            "authentik-subject");
        var profile = new AuthenticatedUserProfile(
            primaryIdentity,
            [primaryIdentity, brokerIdentity],
            "John Example",
            "john@example.test",
            "John Example",
            "https://cdn.example.test/avatar.png");
        var users = new CapturingUserRepository();
        var externalIdentities = new CapturingExternalIdentityRepository();
        using var services = BuildServices(users, externalIdentities);
        var downstreamCalled = false;
        var middleware = new AuthenticatedUserSynchronizationMiddleware(
            _ =>
            {
                downstreamCalled = true;
                return Task.CompletedTask;
            },
            services.GetRequiredService<IServiceScopeFactory>());
        var staleUserId = Guid.NewGuid();
        var context = CreateContext(
            profile,
            new Claim(AuthClaimTypes.UserId, staleUserId.ToString()),
            new Claim(AuthClaimTypes.UserId, Guid.NewGuid().ToString()),
            new Claim(ClaimTypes.Name, "broker-name"),
            new Claim(ClaimTypes.Name, "duplicate-name"));

        await middleware.InvokeAsync(
            context,
            new AuthenticatedUserSyncCache(),
            new AuthenticatedUserProvisioningLock(),
            SyncOptions());

        Assert.True(downstreamCalled);
        Assert.Equal(profile.Identities, externalIdentities.LinkedIdentities);
        Assert.Equal("john-example", users.AddedUser!.Handle);
        Assert.Equal("john@example.test", users.AddedUser.Email);
        Assert.Equal("John Example", users.AddedUser.DisplayName);
        Assert.Equal("https://cdn.example.test/avatar.png", users.AddedUser.AvatarUrl);
        Assert.Null(context.Features.Get<AuthenticatedProfileFeature>());
        var userIdClaim = Assert.Single(context.User.FindAll(AuthClaimTypes.UserId));
        Assert.Equal(users.AddedUser.Id.Value.ToString(), userIdClaim.Value);
        var nameClaim = Assert.Single(context.User.FindAll(ClaimTypes.Name));
        Assert.Equal("john-example", nameClaim.Value);
    }

    [Fact]
    public async Task Removes_Profile_Feature_Before_Long_Lived_Continuation()
    {
        var scopedLifetime = new ScopedLifetimeProbe();
        using var services = new ServiceCollection()
            .AddScoped(_ => scopedLifetime.CreateLease())
            .AddScoped<UserIdentityService>(serviceProvider =>
            {
                _ = serviceProvider.GetRequiredService<ScopedLifetimeLease>();
                return new UserIdentityService(
                    new CapturingUserRepository(),
                    new CapturingExternalIdentityRepository(),
                    new ProvisioningUnitOfWork());
            })
            .BuildServiceProvider();
        var nextEntered = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var releaseNext = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        AuthenticatedProfileFeature? downstreamFeature = null;
        var middleware = new AuthenticatedUserSynchronizationMiddleware(
            async context =>
            {
                downstreamFeature = context.Features.Get<AuthenticatedProfileFeature>();
                nextEntered.TrySetResult();
                await releaseNext.Task;
            },
            services.GetRequiredService<IServiceScopeFactory>());
        var context = CreateContext(CreateProfile());

        var invokeTask = middleware.InvokeAsync(
            context,
            new AuthenticatedUserSyncCache(),
            new AuthenticatedUserProvisioningLock(),
            SyncOptions());
        await nextEntered.Task.WaitAsync(TestContext.Current.CancellationToken);

        try
        {
            Assert.False(invokeTask.IsCompleted);
            Assert.Null(downstreamFeature);
            Assert.Null(context.Features.Get<AuthenticatedProfileFeature>());
            Assert.NotNull(context.User.FindFirstValue(AuthClaimTypes.UserId));
            Assert.Equal(1, scopedLifetime.Created);
            Assert.Equal(scopedLifetime.Created, scopedLifetime.Disposed);
            Assert.Equal(0, scopedLifetime.Active);
        }
        finally
        {
            releaseNext.TrySetResult();
            await invokeTask;
        }
    }

    [Fact]
    public async Task Removes_Profile_Feature_When_Synchronization_Fails()
    {
        using var services = new ServiceCollection()
            .AddScoped(_ => new UserIdentityService(
                new CapturingUserRepository(),
                new ThrowingExternalIdentityRepository(),
                new ProvisioningUnitOfWork()))
            .BuildServiceProvider();
        var downstreamCalled = false;
        var middleware = new AuthenticatedUserSynchronizationMiddleware(
            _ =>
            {
                downstreamCalled = true;
                return Task.CompletedTask;
            },
            services.GetRequiredService<IServiceScopeFactory>());
        var context = CreateContext(CreateProfile());

        await Assert.ThrowsAsync<InvalidOperationException>(() => middleware.InvokeAsync(
            context,
            new AuthenticatedUserSyncCache(),
            new AuthenticatedUserProvisioningLock(),
            SyncOptions()));

        Assert.False(downstreamCalled);
        Assert.Null(context.Features.Get<AuthenticatedProfileFeature>());
    }

    [Fact]
    public async Task Removes_Profile_Feature_When_Principal_Is_Not_Authenticated()
    {
        using var services = BuildServices(
            new CapturingUserRepository(),
            new CapturingExternalIdentityRepository());
        AuthenticatedProfileFeature? downstreamFeature = null;
        var middleware = new AuthenticatedUserSynchronizationMiddleware(
            context =>
            {
                downstreamFeature = context.Features.Get<AuthenticatedProfileFeature>();
                return Task.CompletedTask;
            },
            services.GetRequiredService<IServiceScopeFactory>());
        var context = new DefaultHttpContext
        {
            User = new ClaimsPrincipal(new ClaimsIdentity()),
        };
        context.Features.Set(new AuthenticatedProfileFeature(CreateProfile()));

        await middleware.InvokeAsync(
            context,
            new AuthenticatedUserSyncCache(),
            new AuthenticatedUserProvisioningLock(),
            SyncOptions());

        Assert.Null(downstreamFeature);
        Assert.Null(context.Features.Get<AuthenticatedProfileFeature>());
    }

    private static ServiceProvider BuildServices(
        CapturingUserRepository users,
        CapturingExternalIdentityRepository externalIdentities) =>
        new ServiceCollection()
            .AddScoped(_ => new UserIdentityService(
                users,
                externalIdentities,
                new ProvisioningUnitOfWork()))
            .BuildServiceProvider();

    private static DefaultHttpContext CreateContext(
        AuthenticatedUserProfile profile,
        params Claim[] extraClaims)
    {
        var context = new DefaultHttpContext
        {
            User = new ClaimsPrincipal(new ClaimsIdentity(
                extraClaims,
                authenticationType: "broker")),
        };
        context.Features.Set(new AuthenticatedProfileFeature(profile));
        return context;
    }

    private static AuthenticatedUserProfile CreateProfile()
    {
        var identity = new AuthenticatedExternalIdentity(
            "test",
            "https://issuer.test",
            "subject-1");
        return new AuthenticatedUserProfile(
            identity,
            [identity],
            "john",
            "john@example.test",
            "John",
            null);
    }

    private static IOptions<OidcAuthOptions> SyncOptions() =>
        Options.Create(new OidcAuthOptions
        {
            LocalProfileSyncTtlSeconds = 60,
        });

    private sealed class CapturingUserRepository : IUserRepository
    {
        public User? AddedUser { get; private set; }

        public Task<User?> GetByIdAsync(
            UserId id,
            CancellationToken ct = default) =>
            Task.FromResult<User?>(null);

        public Task<User?> GetByHandleAsync(
            string handle,
            CancellationToken ct = default) =>
            Task.FromResult<User?>(null);

        public Task<IReadOnlyList<string>> GetHandlesByPrefixAsync(
            string prefix,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<string>>([]);

        public Task<IReadOnlyList<User>> GetByIdsAsync(
            IReadOnlyList<UserId> ids,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<User>>([]);

        public Task AddAsync(
            User user,
            CancellationToken ct = default)
        {
            AddedUser = user;
            return Task.CompletedTask;
        }
    }

    private sealed class CapturingExternalIdentityRepository : IExternalIdentityRepository
    {
        private readonly List<AuthenticatedExternalIdentity> _linkedIdentities = [];

        public IReadOnlyList<AuthenticatedExternalIdentity> LinkedIdentities => _linkedIdentities;

        public Task<ExternalIdentity?> GetAsync(
            string provider,
            string issuer,
            string subject,
            CancellationToken ct = default) =>
            Task.FromResult<ExternalIdentity?>(null);

        public Task AddAsync(
            ExternalIdentity identity,
            CancellationToken ct = default)
        {
            _linkedIdentities.Add(new AuthenticatedExternalIdentity(
                identity.Provider,
                identity.Issuer,
                identity.Subject));
            return Task.CompletedTask;
        }
    }

    private sealed class ThrowingExternalIdentityRepository : IExternalIdentityRepository
    {
        public Task<ExternalIdentity?> GetAsync(
            string provider,
            string issuer,
            string subject,
            CancellationToken ct = default) =>
            throw new InvalidOperationException("Synchronization failed.");

        public Task AddAsync(
            ExternalIdentity identity,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
    }

    private sealed class ProvisioningUnitOfWork : IUnitOfWork
    {
        public Task SaveChangesAsync(CancellationToken ct = default) =>
            Task.CompletedTask;

        public Task<ITransactionScope> BeginTransactionAsync(
            CancellationToken ct = default) =>
            throw new NotSupportedException();
    }
}
