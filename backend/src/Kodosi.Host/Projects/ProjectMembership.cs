using Kodosi.Data;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Projects;

internal sealed class ProjectPerson
{
    public Guid ProjectId { get; init; }
    public Guid UserId { get; init; }
}

internal static class ProjectMembership
{
    // One UNION ALL query: PostgreSQL pushes each filter into every branch, so each branch uses its index.
    public static IQueryable<ProjectPerson> People(KodosiDbContext db, bool invited = false)
    {
        var people = db.Projects.Select(x => new ProjectPerson { ProjectId = x.Id, UserId = x.OwnerUserId })
            .Concat(db.ProjectMembers.Select(x => new ProjectPerson { ProjectId = x.ProjectId, UserId = x.UserId }));
        return invited ? people.Concat(db.ProjectInvitations.Select(x => new ProjectPerson { ProjectId = x.ProjectId, UserId = x.UserId })) : people;
    }

    public static IQueryable<Guid> ProjectsOf(KodosiDbContext db, Guid userId, bool invited = false) =>
        People(db, invited).Where(x => x.UserId == userId).Select(x => x.ProjectId);

    public static IQueryable<Guid> PeopleOf(KodosiDbContext db, Guid projectId, bool invited = false) =>
        People(db, invited).Where(x => x.ProjectId == projectId).Select(x => x.UserId);

    public static async Task NotifyProjectsOfAsync(KodosiDbContext db, ConnectionDirectory connections, Guid userId, CancellationToken ct)
    {
        var projects = ProjectsOf(db, userId);
        var people = await People(db).Where(x => projects.Contains(x.ProjectId)).Select(x => new { x.ProjectId, x.UserId }).Distinct().ToListAsync(ct);
        foreach (var project in people.GroupBy(x => x.ProjectId)) connections.Notify(project.Select(x => x.UserId), "projects", project.Key);
    }
}
