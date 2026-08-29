using NetArchTest.Rules;

namespace Kodosi.ArchitectureTests;

public class HexagonalArchitectureTests
{
    private const string DomainNamespace = "Kodosi.Domain";
    private const string ApplicationNamespace = "Kodosi.Application";
    private const string InfrastructureNamespace = "Kodosi.Infrastructure";
    private const string HostNamespace = "Kodosi.Host";

    [Fact]
    public void Domain_Should_Not_Depend_On_Application()
    {
        var result = Types.InAssembly(typeof(Domain.User).Assembly)
            .ShouldNot()
            .HaveDependencyOn(ApplicationNamespace)
            .GetResult();

        Assert.True(result.IsSuccessful, "Domain must not depend on Application.");
    }

    [Fact]
    public void Domain_Should_Not_Depend_On_Infrastructure()
    {
        var result = Types.InAssembly(typeof(Domain.User).Assembly)
            .ShouldNot()
            .HaveDependencyOn(InfrastructureNamespace)
            .GetResult();

        Assert.True(result.IsSuccessful, "Domain must not depend on Infrastructure.");
    }

    [Fact]
    public void Domain_Should_Not_Depend_On_Host()
    {
        var result = Types.InAssembly(typeof(Domain.User).Assembly)
            .ShouldNot()
            .HaveDependencyOn(HostNamespace)
            .GetResult();

        Assert.True(result.IsSuccessful, "Domain must not depend on Host.");
    }

    [Fact]
    public void Application_Should_Not_Depend_On_Infrastructure()
    {
        var result = Types.InAssembly(typeof(Application.IUserRepository).Assembly)
            .ShouldNot()
            .HaveDependencyOn(InfrastructureNamespace)
            .GetResult();

        Assert.True(result.IsSuccessful, "Application must not depend on Infrastructure.");
    }

    [Fact]
    public void Application_Should_Not_Depend_On_Host()
    {
        var result = Types.InAssembly(typeof(Application.IUserRepository).Assembly)
            .ShouldNot()
            .HaveDependencyOn(HostNamespace)
            .GetResult();

        Assert.True(result.IsSuccessful, "Application must not depend on Host.");
    }

    [Fact]
    public void Infrastructure_Should_Not_Depend_On_Host()
    {
        var result = Types.InAssembly(typeof(Infrastructure.Persistence.KodosiDbContext).Assembly)
            .ShouldNot()
            .HaveDependencyOn(HostNamespace)
            .GetResult();

        Assert.True(result.IsSuccessful, "Infrastructure must not depend on Host.");
    }

    [Fact]
    public void CurrentUser_Should_Be_A_Host_Request_Contract()
    {
        var applicationCurrentUser = typeof(Application.IUserRepository).Assembly
            .GetType("Kodosi.Application.ICurrentUser", throwOnError: false);
        var hostCurrentUser = typeof(Host.Auth.ICurrentUser);

        Assert.Null(applicationCurrentUser);
        Assert.Equal("Kodosi.Host.Auth", hostCurrentUser.Namespace);
        Assert.True(hostCurrentUser.IsInterface);
    }

    [Fact]
    public void Endpoints_Should_Not_Depend_On_Persistence_Ports()
    {
        var applicationAssembly = typeof(Application.IUserRepository).Assembly;
        var persistencePorts = applicationAssembly.GetTypes()
            .Where(type => type == typeof(Application.IUnitOfWork)
                || (type.IsInterface && type.Name.EndsWith(
                    "Repository",
                    StringComparison.Ordinal)))
            .Select(type => type.FullName!)
            .ToArray();

        var result = Types.InAssembly(typeof(Host.Endpoints.UserEndpoints).Assembly)
            .That()
            .ResideInNamespace("Kodosi.Host.Endpoints")
            .ShouldNot()
            .HaveDependencyOnAny(persistencePorts)
            .GetResult();

        Assert.True(
            result.IsSuccessful,
            $"Endpoints must call Application use cases rather than persistence ports: {string.Join(", ", result.FailingTypeNames ?? [])}");
    }
}
