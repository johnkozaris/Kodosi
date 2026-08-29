using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Infrastructure.Persistence;
using Kodosi.Infrastructure.Persistence.Repositories;
using Npgsql;

namespace Kodosi.Host.DependencyInjection;

public static class PersistenceInfrastructureModule
{
    public static IServiceCollection AddPersistenceInfrastructure(
        this IServiceCollection services,
        IConfiguration configuration)
    {
        var connectionString = configuration.GetConnectionString("kodosi");
        if (string.IsNullOrWhiteSpace(connectionString))
        {
            throw new InvalidOperationException("ConnectionStrings:kodosi must be configured.");
        }

        services.AddSingleton(_ => NpgsqlDataSource.Create(connectionString));





        services.AddSingleton<RelaySingletonLeaseHostedService>();
        services.AddHostedService(sp => sp.GetRequiredService<RelaySingletonLeaseHostedService>());
        services.AddDbContextFactory<KodosiDbContext>((serviceProvider, options) =>
            options.UseNpgsql(serviceProvider.GetRequiredService<NpgsqlDataSource>(), npgsql =>
            {



                npgsql.CommandTimeout(30);
            }));
        services.AddScoped(sp =>
            sp.GetRequiredService<IDbContextFactory<KodosiDbContext>>().CreateDbContext());

        services.AddScoped<IUserRepository, UserRepository>();
        services.AddScoped<IExternalIdentityRepository, ExternalIdentityRepository>();
        services.AddScoped<ISessionRepository, SessionRepository>();
        services.AddScoped<IFriendshipRepository, FriendshipRepository>();
        services.AddScoped<IRoomRepository, RoomRepository>();
        services.AddScoped<IRoomRosterTransitionRepository, RoomRosterTransitionRepository>();
        services.AddScoped<IRoomMemberRepository, RoomMemberRepository>();
        services.AddScoped<ISharedRoomAuthorizationRepository>(serviceProvider =>
            (RoomMemberRepository)serviceProvider.GetRequiredService<IRoomMemberRepository>());
        services.AddScoped<IRoomInvitationRepository, RoomInvitationRepository>();
        services.AddScoped<IRoomChatRepository, RoomChatRepository>();
        services.AddScoped<IRoomTaskRepository, RoomTaskRepository>();
        services.AddScoped<IRoomMutationReceiptRepository, RoomMutationReceiptRepository>();
        services.AddScoped<IAccessOverrideRepository, AccessOverrideRepository>();
        services.AddScoped<ISessionViewerDismissalRepository, SessionViewerDismissalRepository>();
        services.AddScoped<IInputAuditRepository, InputAuditRepository>();
        services.AddScoped<IIdentityResetAuditRepository, IdentityResetAuditRepository>();
        services.AddScoped<IIdentityExposureRepository, IdentityExposureRepository>();
        services.AddScoped<IFriendshipAuditRepository, FriendshipAuditRepository>();
        services.AddScoped<IRoomMemberAuditRepository, RoomMemberAuditRepository>();
        services.AddScoped<IDeviceRevocationAuditRepository, DeviceRevocationAuditRepository>();
        services.AddScoped<IAccessOverrideAuditRepository, AccessOverrideAuditRepository>();
        services.AddScoped<IUserDeviceRepository, UserDeviceRepository>();
        services.AddScoped<IUserDeviceListRepository, UserDeviceListRepository>();
        services.AddScoped<IDeviceRegistrationChallengeRepository, DeviceRegistrationChallengeRepository>();
        services.AddScoped<IDeviceLinkRequestRepository, DeviceLinkRequestRepository>();
        services.AddScoped<ISessionKeyBlobRepository, SessionKeyBlobRepository>();
        services.AddScoped<ISessionIncarnationRepository, SessionIncarnationRepository>();
        services.AddScoped<ISessionEndMutationRepository, SessionEndMutationRepository>();
        services.AddScoped<ISessionAccessMutationRepository, SessionAccessMutationRepository>();
        services.AddScoped<
            ISemanticRelayLifecycleRepository,
            SemanticRelayLifecycleRepository>();
        services.AddScoped<IUnitOfWork, UnitOfWork>();
        services.AddScoped<IUserLifecycleLock, PostgresUserLifecycleLock>();
        services.AddScoped<
            IRecipientDeviceLifecycleLock,
            PostgresRecipientDeviceLifecycleLock>();
        services.AddScoped<IRoomLifecycleLock, PostgresRoomLifecycleLock>();
        services.AddSingleton<
            IAccessOverrideExpiryDurabilityCoordinator,
            AccessOverrideExpiryDurabilityCoordinator>();
        services.AddSingleton<
            IIdentityResetDurabilityCoordinator,
            IdentityResetDurabilityCoordinator>();
        services.AddSingleton<SessionIncarnationResolver>();
        services.AddSingleton<IIdentityResetSessionResolver>(provider =>
            provider.GetRequiredService<SessionIncarnationResolver>());
        services.AddSingleton<
            IDeviceRevocationDurabilityCoordinator,
            DeviceRevocationDurabilityCoordinator>();
        services.AddSingleton<IDeviceRevocationSessionResolver>(provider =>
            provider.GetRequiredService<SessionIncarnationResolver>());

        services.AddSingleton(TimeProvider.System);

        return services;
    }
}
