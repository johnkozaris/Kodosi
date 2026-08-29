using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class UserIdentityServiceTests
{
    [Fact]
    public void IdentityKey_Is_Canonical_And_Collision_Free()
    {
        var canonical = new AuthenticatedExternalIdentity("a", "b", "c\nd");
        var formerlyAliased = new AuthenticatedExternalIdentity("a\nb", "c", "d");
        var normalized = new AuthenticatedExternalIdentity(" A ", " b ", " c\nd ");
        var unicode = new AuthenticatedExternalIdentity("é", "", "");

        Assert.NotEqual(canonical.IdentityKey, formerlyAliased.IdentityKey);
        Assert.Equal(canonical.IdentityKey, normalized.IdentityKey);
        Assert.Equal("v1:w6k=::", unicode.IdentityKey);
    }

    [Fact]
    public async Task ResolveOrProvisionAsync_Rejects_Formerly_Aliased_Identity_Components()
    {
        var firstUser = User.Create(UserId.New(), "first@example.com", "first", "First");
        var secondUser = User.Create(UserId.New(), "second@example.com", "second", "Second");
        var users = new InMemoryUserRepository(firstUser, secondUser);
        var identities = new InMemoryExternalIdentityRepository(
            ExternalIdentity.Create(firstUser.Id, "a", "b", "c\nd"),
            ExternalIdentity.Create(secondUser.Id, "a\nb", "c", "d"));
        var service = new UserIdentityService(users, identities, new TestUnitOfWork());

        var error = await Assert.ThrowsAsync<ConflictException>(() => service.ResolveOrProvisionAsync(
            new AuthenticatedUserProfile(
                new AuthenticatedExternalIdentity("a", "b", "c\nd"),
                [
                    new AuthenticatedExternalIdentity("a", "b", "c\nd"),
                    new AuthenticatedExternalIdentity("a\nb", "c", "d"),
                ],
                "conflict",
                null,
                null,
                null),
            TestContext.Current.CancellationToken));

        Assert.Equal("Authenticated identities are already linked to different users.", error.Message);
    }

    [Fact]
    public async Task ResolveOrProvisionAsync_Creates_User_From_Primary_Identity()
    {
        var users = new InMemoryUserRepository();
        var identities = new InMemoryExternalIdentityRepository();
        var service = new UserIdentityService(users, identities, new TestUnitOfWork());

        var user = await service.ResolveOrProvisionAsync(new AuthenticatedUserProfile(
            new AuthenticatedExternalIdentity("google", "https://accounts.google.com", "google-sub-1"),
            [
                new AuthenticatedExternalIdentity("google", "https://accounts.google.com", "google-sub-1"),
            ],
            "John Example",
            "[email protected]",
            "John Example",
            "https://example.com/avatar.png"),
            TestContext.Current.CancellationToken);

        Assert.Equal("john-example", user.Handle);

        var storedIdentity = await identities.GetAsync(
            "google",
            "https://accounts.google.com",
            "google-sub-1",
            TestContext.Current.CancellationToken);
        Assert.NotNull(storedIdentity);
        Assert.Equal(user.Id, storedIdentity!.UserId);
    }

    [Fact]
    public async Task ResolveOrProvisionAsync_Links_Broker_Identity_To_Existing_Native_User()
    {
        var existingUser = User.Create(
            UserId.New(),
            "[email protected]",
            "john-example",
            "John Example");
        var users = new InMemoryUserRepository(existingUser);
        var identities = new InMemoryExternalIdentityRepository(
            ExternalIdentity.Create(
                existingUser.Id,
                "google",
                "https://accounts.google.com",
                "google-sub-1"));
        var service = new UserIdentityService(users, identities, new TestUnitOfWork());

        var resolved = await service.ResolveOrProvisionAsync(new AuthenticatedUserProfile(
            new AuthenticatedExternalIdentity("google", "https://accounts.google.com", "google-sub-1"),
            [
                new AuthenticatedExternalIdentity("google", "https://accounts.google.com", "google-sub-1"),
                new AuthenticatedExternalIdentity("authentik", "https://auth.kodosi.com/application/o/kodosi/", "authentik-sub-1"),
            ],
            "john",
            "[email protected]",
            "John Example",
            null),
            TestContext.Current.CancellationToken);

        Assert.Equal(existingUser.Id, resolved.Id);

        var brokerIdentity = await identities.GetAsync(
            "authentik",
            "https://auth.kodosi.com/application/o/kodosi/",
            "authentik-sub-1",
            TestContext.Current.CancellationToken);
        Assert.NotNull(brokerIdentity);
        Assert.Equal(existingUser.Id, brokerIdentity!.UserId);
    }

    [Fact]
    public async Task ResolveOrProvisionAsync_Rejects_Cross_User_Identity_Conflicts()
    {
        var googleUser = User.Create(UserId.New(), "[email protected]", "google-user", "Google User");
        var appleUser = User.Create(UserId.New(), "[email protected]", "apple-user", "Apple User");
        var users = new InMemoryUserRepository(googleUser, appleUser);
        var identities = new InMemoryExternalIdentityRepository(
            ExternalIdentity.Create(googleUser.Id, "google", "https://accounts.google.com", "google-sub"),
            ExternalIdentity.Create(appleUser.Id, "apple", "https://appleid.apple.com", "apple-sub"));
        var service = new UserIdentityService(users, identities, new TestUnitOfWork());

        var error = await Assert.ThrowsAsync<ConflictException>(() => service.ResolveOrProvisionAsync(
            new AuthenticatedUserProfile(
                new AuthenticatedExternalIdentity("google", "https://accounts.google.com", "google-sub"),
                [
                    new AuthenticatedExternalIdentity("google", "https://accounts.google.com", "google-sub"),
                    new AuthenticatedExternalIdentity("apple", "https://appleid.apple.com", "apple-sub"),
                ],
                "conflict",
                null,
                "Conflict",
                null),
            TestContext.Current.CancellationToken));

        Assert.Equal("Authenticated identities are already linked to different users.", error.Message);
    }

    [Fact]
    public async Task ResolveOrProvisionAsync_Separates_Same_Subject_Across_Providers()
    {
        var users = new InMemoryUserRepository();
        var identities = new InMemoryExternalIdentityRepository();
        var service = new UserIdentityService(users, identities, new TestUnitOfWork());

        var googleUser = await service.ResolveOrProvisionAsync(new AuthenticatedUserProfile(
            new AuthenticatedExternalIdentity("google", "https://accounts.google.com", "shared-subject"),
            [
                new AuthenticatedExternalIdentity("google", "https://accounts.google.com", "shared-subject"),
            ],
            "Google Example",
            "google@example.com",
            "Google Example",
            null),
            TestContext.Current.CancellationToken);

        var appleUser = await service.ResolveOrProvisionAsync(new AuthenticatedUserProfile(
            new AuthenticatedExternalIdentity("apple", "https://appleid.apple.com", "shared-subject"),
            [
                new AuthenticatedExternalIdentity("apple", "https://appleid.apple.com", "shared-subject"),
            ],
            "Apple Example",
            "apple@example.com",
            "Apple Example",
            null),
            TestContext.Current.CancellationToken);

        var googleIdentity = await identities.GetAsync(
            "google",
            "https://accounts.google.com",
            "shared-subject",
            TestContext.Current.CancellationToken);
        var appleIdentity = await identities.GetAsync(
            "apple",
            "https://appleid.apple.com",
            "shared-subject",
            TestContext.Current.CancellationToken);

        Assert.Equal(googleUser.Id, googleIdentity!.UserId);
        Assert.Equal(appleUser.Id, appleIdentity!.UserId);
        Assert.NotEqual(googleIdentity.UserId, appleIdentity.UserId);
    }

    [Fact]
    public async Task ResolveOrProvisionAsync_Uses_Placeholder_Canonical_Email_Without_Profile_Email()
    {
        var users = new InMemoryUserRepository();
        var identities = new InMemoryExternalIdentityRepository();
        var service = new UserIdentityService(users, identities, new TestUnitOfWork());

        var user = await service.ResolveOrProvisionAsync(new AuthenticatedUserProfile(
            new AuthenticatedExternalIdentity("apple", "https://appleid.apple.com", "apple-sub"),
            [
                new AuthenticatedExternalIdentity("apple", "https://appleid.apple.com", "apple-sub"),
            ],
            "apple-user",
            null,
            null,
            null),
            TestContext.Current.CancellationToken);

        Assert.EndsWith("@users.Kodosi.invalid", user.Email);
        Assert.Equal("apple-user", user.DisplayName);

        var storedIdentity = await identities.GetAsync(
            "apple",
            "https://appleid.apple.com",
            "apple-sub",
            TestContext.Current.CancellationToken);
        Assert.NotNull(storedIdentity);
        Assert.Equal(user.Id, storedIdentity!.UserId);
    }

    [Fact]
    public async Task ResolveOrProvisionAsync_Resolves_Existing_Persisted_Apple_Identity()
    {
        var existingUser = User.Create(
            UserId.New(),
            "stable@example.com",
            "apple-user",
            "Stable Name");
        var users = new InMemoryUserRepository(existingUser);
        var identities = new InMemoryExternalIdentityRepository(
            ExternalIdentity.Create(
                existingUser.Id,
                "apple",
                "https://appleid.apple.com",
                "apple-sub"));
        var service = new UserIdentityService(users, identities, new TestUnitOfWork());

        var resolved = await service.ResolveOrProvisionAsync(new AuthenticatedUserProfile(
            new AuthenticatedExternalIdentity("apple", "https://appleid.apple.com", "apple-sub"),
            [
                new AuthenticatedExternalIdentity("apple", "https://appleid.apple.com", "apple-sub"),
                new AuthenticatedExternalIdentity(
                    "authentik",
                    "https://auth.kodosi.com/application/o/kodosi/",
                    "authentik-sub"),
            ],
            "updated-handle-seed",
            null,
            null,
            null),
            TestContext.Current.CancellationToken);

        Assert.Equal(existingUser.Id, resolved.Id);
        Assert.Equal("stable@example.com", resolved.Email);
        Assert.Equal("Stable Name", resolved.DisplayName);

        var brokerIdentity = await identities.GetAsync(
            "authentik",
            "https://auth.kodosi.com/application/o/kodosi/",
            "authentik-sub",
            TestContext.Current.CancellationToken);
        Assert.NotNull(brokerIdentity);
        Assert.Equal(existingUser.Id, brokerIdentity!.UserId);
    }

    [Fact]
    public async Task ResolveOrProvisionAsync_Batches_Handle_Family_Lookup()
    {
        var existingRoot = User.Create(
            UserId.New(),
            "root@example.com",
            "john-example",
            "Root");
        var existingSecond = User.Create(
            UserId.New(),
            "second@example.com",
            "john-example-2",
            "Second");
        var users = new InMemoryUserRepository(existingRoot, existingSecond);
        var identities = new InMemoryExternalIdentityRepository();
        var service = new UserIdentityService(users, identities, new TestUnitOfWork());

        var user = await service.ResolveOrProvisionAsync(new AuthenticatedUserProfile(
            new AuthenticatedExternalIdentity("google", "https://accounts.google.com", "google-sub-2"),
            [
                new AuthenticatedExternalIdentity("google", "https://accounts.google.com", "google-sub-2"),
            ],
            "John Example",
            "john2@example.com",
            "John Example",
            null),
            TestContext.Current.CancellationToken);

        Assert.Equal("john-example-3", user.Handle);
        Assert.Equal(0, users.GetByHandleAsyncCalls);
        Assert.Equal(1, users.GetHandlesByPrefixAsyncCalls);
    }

    private sealed class InMemoryUserRepository(params User[] users) : IUserRepository
    {
        private readonly Dictionary<UserId, User> _usersById = users.ToDictionary(user => user.Id);
        private readonly Dictionary<string, User> _usersByHandle = users.ToDictionary(
            user => user.Handle,
            user => user,
            StringComparer.Ordinal);

        public int GetByHandleAsyncCalls { get; private set; }
        public int GetHandlesByPrefixAsyncCalls { get; private set; }

        public Task<User?> GetByIdAsync(UserId id, CancellationToken ct = default)
            => Task.FromResult(_usersById.GetValueOrDefault(id));

        public Task<User?> GetByHandleAsync(string handle, CancellationToken ct = default)
        {
            GetByHandleAsyncCalls++;
            return Task.FromResult(_usersByHandle.GetValueOrDefault(handle.Trim().ToLowerInvariant()));
        }

        public Task<IReadOnlyList<string>> GetHandlesByPrefixAsync(string prefix, CancellationToken ct = default)
        {
            GetHandlesByPrefixAsyncCalls++;
            var normalizedPrefix = prefix.Trim().ToLowerInvariant();
            return Task.FromResult<IReadOnlyList<string>>(_usersByHandle.Keys
                .Where(handle =>
                    string.Equals(handle, normalizedPrefix, StringComparison.Ordinal)
                    || handle.StartsWith(normalizedPrefix + "-", StringComparison.Ordinal))
                .OrderBy(handle => handle, StringComparer.Ordinal)
                .ToList());
        }

        public Task<IReadOnlyList<User>> GetByIdsAsync(IReadOnlyList<UserId> ids, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<User>>(ids
                .Select(id => _usersById.GetValueOrDefault(id))
                .Where(user => user is not null)
                .Cast<User>()
                .ToList());

        public Task AddAsync(User user, CancellationToken ct = default)
        {
            _usersById[user.Id] = user;
            _usersByHandle[user.Handle] = user;
            return Task.CompletedTask;
        }
    }

    private sealed class InMemoryExternalIdentityRepository(params ExternalIdentity[] identities)
        : IExternalIdentityRepository
    {
        private readonly Dictionary<(string Provider, string Issuer, string Subject), ExternalIdentity> _identities =
            identities.ToDictionary(BuildKey, identity => identity);

        public Task<ExternalIdentity?> GetAsync(string provider, string issuer, string subject, CancellationToken ct = default)
            => Task.FromResult(_identities.GetValueOrDefault(BuildKey(provider, issuer, subject)));

        public Task<IReadOnlyList<ExternalIdentity>> GetByUserIdAsync(UserId userId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<ExternalIdentity>>(_identities.Values
                .Where(identity => identity.UserId == userId)
                .ToList());

        public Task AddAsync(ExternalIdentity identity, CancellationToken ct = default)
        {
            _identities[BuildKey(identity)] = identity;
            return Task.CompletedTask;
        }

        private static (string Provider, string Issuer, string Subject) BuildKey(ExternalIdentity identity)
            => BuildKey(identity.Provider, identity.Issuer, identity.Subject);

        private static (string Provider, string Issuer, string Subject) BuildKey(
            string provider,
            string issuer,
            string subject)
            => (provider.Trim().ToLowerInvariant(), issuer.Trim(), subject.Trim());
    }

    private sealed class TestUnitOfWork : UnitOfWorkStub
    {
        public override Task SaveChangesAsync(CancellationToken ct = default) => Task.CompletedTask;
    }
}
