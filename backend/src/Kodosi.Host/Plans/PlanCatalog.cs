using System.Globalization;
using Kodosi.Data;

namespace Kodosi.Plans;

public sealed record PlanLimits(int Devices, int OwnedProjects, int ProjectPeople, int SharePeople, long? ProjectBytes,
    int? Friends, int SentFriendRequests, int? ReceivedFriendRequests)
{
    public static readonly PlanLimits Safety = new(Devices: 10, OwnedProjects: Limits.MaxProjectsPerUser,
        ProjectPeople: Limits.MaxProjectMembers + 1, SharePeople: Limits.MaxSessionMembers, ProjectBytes: null,
        Friends: null, SentFriendRequests: 64, ReceivedFriendRequests: null);
}

public sealed class PlanCatalog
{
    public const long LeastProjectBytes = 128 * 1024;
    private static readonly string[] LimitNames =
        ["Devices", "OwnedProjects", "ProjectPeople", "SharePeople", "ProjectBytes", "Friends", "SentFriendRequests", "ReceivedFriendRequests"];
    private readonly string? fallback;
    private readonly IReadOnlyDictionary<string, PlanLimits> plans;

    private PlanCatalog(string? fallback, IReadOnlyDictionary<string, PlanLimits> plans)
    {
        this.fallback = fallback; this.plans = plans;
    }

    public static PlanCatalog None { get; } = new(null, new Dictionary<string, PlanLimits>());

    public PlanLimits For(string? plan) =>
        fallback is null ? PlanLimits.Safety : plans[plan is not null && plans.ContainsKey(plan) ? plan : fallback];

    public async Task<PlanLimits> LimitsAsync(KodosiDbContext db, Guid userId, CancellationToken ct)
        => For((await db.Users.FindAsync([userId], ct))?.Plan);

    public static PlanCatalog Of(string fallback, IReadOnlyDictionary<string, PlanLimits> plans)
    {
        if (!plans.ContainsKey(fallback)) throw new InvalidOperationException($"Plans:Default names the plan '{fallback}', which Plans:Definitions does not define.");
        foreach (var (name, limits) in plans) Check(name, limits);
        return new PlanCatalog(fallback, plans);
    }

    public static PlanCatalog From(IConfiguration config)
    {
        var section = config.GetSection("Plans");
        var plans = section.GetSection("Definitions").GetChildren().ToDictionary(x => x.Key, Read, StringComparer.Ordinal);
        var fallback = section["Default"]?.Trim();
        if (string.IsNullOrEmpty(fallback))
        {
            if (plans.Count > 0) throw new InvalidOperationException("Plans:Default must name one of Plans:Definitions.");
            return None;
        }
        return Of(fallback, plans);
    }

    private static PlanLimits Read(IConfigurationSection plan)
    {
        foreach (var key in plan.GetChildren().Select(x => x.Key))
            if (!LimitNames.Contains(key)) throw new InvalidOperationException($"Plans:Definitions:{plan.Key}:{key} is not a plan limit.");
        long? Value(string key) => plan[key] is { Length: > 0 } text ? long.Parse(text, CultureInfo.InvariantCulture) : null;
        int? Count(string key) => Value(key) is { } value ? checked((int)value) : null;
        var safety = PlanLimits.Safety;
        return new PlanLimits(Count("Devices") ?? safety.Devices, Count("OwnedProjects") ?? safety.OwnedProjects,
            Count("ProjectPeople") ?? safety.ProjectPeople, Count("SharePeople") ?? safety.SharePeople, Value("ProjectBytes") ?? safety.ProjectBytes,
            Count("Friends") ?? safety.Friends, Count("SentFriendRequests") ?? safety.SentFriendRequests,
            Count("ReceivedFriendRequests") ?? safety.ReceivedFriendRequests);
    }

    private static void Check(string name, PlanLimits limits)
    {
        var safety = PlanLimits.Safety;
        void Within(string limit, long? value, long least, long most)
        {
            if (value < least || value > most)
                throw new InvalidOperationException($"Plans:Definitions:{name}:{limit} must be from {least} to {most}.");
        }
        Within("Devices", limits.Devices, 1, Limits.MaxDevicesPerUser);
        Within("OwnedProjects", limits.OwnedProjects, 1, safety.OwnedProjects);
        Within("ProjectPeople", limits.ProjectPeople, 1, safety.ProjectPeople);
        Within("SharePeople", limits.SharePeople, 1, safety.SharePeople);
        Within("ProjectBytes", limits.ProjectBytes, LeastProjectBytes, long.MaxValue);
        Within("Friends", limits.Friends, 1, int.MaxValue);
        Within("SentFriendRequests", limits.SentFriendRequests, 1, safety.SentFriendRequests);
        Within("ReceivedFriendRequests", limits.ReceivedFriendRequests, 1, int.MaxValue);
    }
}
