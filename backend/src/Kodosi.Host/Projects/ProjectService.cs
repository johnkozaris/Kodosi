using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Friends;
using Kodosi.Plans;
using Kodosi.Sessions;
using Kodosi.TerminalConnections;
using Microsoft.AspNetCore.Mvc;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Projects;

public sealed partial class ProjectService(KodosiDbContext db, ConnectionDirectory connections, FriendService friends, TimeProvider clock, DeviceService devices, PlanCatalog plans)
{
    private async Task<Project> RequireMemberAsync(Guid id, Guid userId, CancellationToken ct)
    {
        var project = await db.Projects.SingleOrDefaultAsync(x => x.Id == id, ct) ?? throw ApiException.Missing();
        return await IsMemberAsync(id, userId, ct) ? project : throw ApiException.Missing();
    }

    internal async Task<Project> RequireMemberForUpdateAsync(Guid id, Guid userId, CancellationToken ct)
    {
        var project = await LockAsync(id, ct);
        return await IsMemberAsync(id, userId, ct) ? project : throw ApiException.Missing();
    }

    private Task<bool> IsMemberAsync(Guid id, Guid userId, CancellationToken ct) =>
        ProjectMembership.People(db).AnyAsync(x => x.ProjectId == id && x.UserId == userId, ct);

    private async Task<Project> LockAsync(Guid id, CancellationToken ct)
    {
        if (db.Database.CurrentTransaction is null) throw new InvalidOperationException("A project change requires a transaction.");
        return await db.Projects.FromSqlInterpolated($"SELECT * FROM projects WHERE \"Id\" = {id} FOR UPDATE").SingleOrDefaultAsync(ct)
            ?? throw ApiException.Missing();
    }

    internal Task<List<Guid>> MemberIdsAsync(Guid id, CancellationToken ct, bool invited = false) =>
        ProjectMembership.PeopleOf(db, id, invited).Distinct().ToListAsync(ct);

    private IQueryable<Session> TerminalsOf(Guid id) => db.Sessions.Where(x => x.ProjectId == id && !x.Ended);

    private async Task<HashSet<Guid>> TerminalPeopleAsync(Guid id, CancellationToken ct)
    {
        var terminals = TerminalsOf(id).Select(x => x.Id);
        return (await TerminalAccess.People(db).Where(x => terminals.Contains(x.SessionId)).Select(x => x.UserId).Distinct().ToListAsync(ct)).ToHashSet();
    }

    public async Task<object> ListAsync(Guid userId, CancellationToken ct)
    {
        var joined = ProjectMembership.ProjectsOf(db, userId);
        var listed = await db.Projects.AsNoTracking().Where(x => joined.Contains(x.Id) && !x.Dosi)
            .OrderBy(x => x.Name).ThenBy(x => x.Id).Take(Limits.MaxVisibleProjects + 1).Select(x => new { x.Id, x.Name, x.OwnerUserId }).ToListAsync(ct);
        var dosi = await db.Projects.AsNoTracking().Where(x => x.OwnerUserId == userId && x.Dosi).Select(x => (Guid?)x.Id).SingleOrDefaultAsync(ct);
        var invitations = await (from i in db.ProjectInvitations.AsNoTracking()
                                 join project in db.Projects on i.ProjectId equals project.Id
                                 join u in db.Users on i.InviterUserId equals u.Id
                                 where i.UserId == userId
                                 orderby i.CreatedAt descending
                                 select new { i.Id, i.ProjectId, projectName = project.Name, inviterUserId = i.InviterUserId, inviterName = u.DisplayName, i.CreatedAt }).Take(Limits.MaxPendingProjectInvitations + 1).ToListAsync(ct);
        return new
        {
            projects = listed.Take(Limits.MaxVisibleProjects).ToArray(),
            invitations = invitations.Take(Limits.MaxPendingProjectInvitations).ToArray(),
            projectsTruncated = listed.Count > Limits.MaxVisibleProjects,
            invitationsTruncated = invitations.Count > Limits.MaxPendingProjectInvitations,
            dosi
        };
    }

    public async Task<object> OpenAsync(Guid id, Guid userId, CancellationToken ct)
    {
        var project = await RequireMemberAsync(id, userId, ct);
        var people = ProjectMembership.PeopleOf(db, id);
        var members = await db.Users.AsNoTracking().Where(u => people.Contains(u.Id))
            .OrderBy(u => u.Handle).Select(u => new { userId = u.Id, u.Handle, u.DisplayName, isOwner = u.Id == project.OwnerUserId }).ToListAsync(ct);
        return new { project = new { project.Id, project.Name, project.OwnerUserId }, members };
    }

    public async Task CreateAsync(Guid userId, CreateProject body, CancellationToken ct)
    {
        Limits.Id(body.Id, "Project ID");
        var name = Limits.Text(body.Name, "Project name", 128);
        var existing = await db.Projects.SingleOrDefaultAsync(x => x.Id == body.Id, ct);
        if (existing is not null)
        {
            if (existing.OwnerUserId != userId || existing.Name != name || existing.Dosi != body.Dosi) throw ApiException.Conflict("This project ID is already in use.");
            return;
        }
        if (body.Dosi)
        {
            if (await db.Projects.AnyAsync(x => x.OwnerUserId == userId && x.Dosi, ct))
                throw ApiException.Conflict("Dosi already has a conversation.");
        }
        else
        {
            var owned = await db.Projects.CountAsync(x => x.OwnerUserId == userId && !x.Dosi, ct);
            var mostProjects = (await plans.LimitsAsync(db, userId, ct)).OwnedProjects;
            if (owned >= mostProjects)
                throw ApiException.OverLimit("projects", mostProjects, $"You own {Limits.Count(owned, "project", "projects")}, the most that you can own. Delete a project to make a new one.");
            await RequireProjectCapacityAsync(userId, ct);
        }
        db.Projects.Add(new Project { Id = body.Id, Name = name, OwnerUserId = userId, Dosi = body.Dosi });
        await db.SaveChangesAsync(ct); connections.Notify([userId], "projects", body.Id);
    }

    private static void RequireProject(Project project)
    {
        if (project.Dosi) throw ApiException.Forbidden("Dosi's conversation belongs to you alone.");
    }

    public async Task RenameAsync(Guid id, Guid userId, string name, CancellationToken ct)
    {
        var project = await RequireMemberAsync(id, userId, ct);
        if (project.OwnerUserId != userId) throw ApiException.Forbidden();
        RequireProject(project);
        project.Name = Limits.Text(name, "Project name", 128);
        await db.SaveChangesAsync(ct);
        await NotifyAsync(id, ct);
        connections.Notify(await TerminalPeopleAsync(id, ct), "sessions");
    }

    public async Task DeleteAsync(Guid id, Guid userId, CancellationToken ct)
    {
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        if (!await db.Projects.AnyAsync(x => x.Id == id, ct)) return;
        var project = await RequireMemberForUpdateAsync(id, userId, ct);
        if (project.OwnerUserId != userId) throw ApiException.Forbidden();
        var people = await MemberIdsAsync(id, ct, invited: true);
        var viewers = await TerminalPeopleAsync(id, ct);
        var terminals = await TerminalsOf(id).ToListAsync(ct);
        foreach (var terminal in terminals)
        {
            SessionService.LeaveProject(terminal);
            terminal.AuthorizationRevision = checked(terminal.AuthorizationRevision + 1);
        }
        db.Projects.Remove(project); await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        await TerminalAccess.RevokeAsync(db, connections, terminals, ct);
        connections.Notify(people, "projects", id);
        connections.Notify(viewers, "sessions");
    }

    public async Task InviteAsync(Guid id, Guid userId, Invite body, CancellationToken ct)
    {
        var keys = await PrepareKeysAsync(userId, body.Keys, ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var project = await RequireMemberForUpdateAsync(id, userId, ct);
        if (project.OwnerUserId != userId) throw ApiException.Forbidden();
        RequireProject(project);
        if (!await friends.AreFriendsAsync(userId, body.UserId, ct)) throw ApiException.Forbidden("Invite an accepted friend.");
        if (await db.ProjectMembers.AnyAsync(x => x.ProjectId == id && x.UserId == body.UserId, ct)) return;
        if (await db.ProjectInvitations.AnyAsync(x => x.ProjectId == id && x.UserId == body.UserId, ct)) return;
        if (await db.ProjectInvitations.CountAsync(x => x.UserId == body.UserId, ct) >= Limits.MaxPendingProjectInvitations)
            throw ApiException.Conflict("This person has too many open project invitations.");
        await RequireProjectCapacityAsync(body.UserId, ct);
        var people = await ProjectMembership.PeopleOf(db, id, invited: true).CountAsync(ct);
        var most = (await plans.LimitsAsync(db, project.OwnerUserId, ct)).ProjectPeople;
        if (people >= most) throw TooManyPeople(most);
        db.ProjectInvitations.Add(new ProjectInvitation { Id = Limits.Id(body.Id, "Invitation ID"), ProjectId = id, UserId = body.UserId, InviterUserId = userId, CreatedAt = clock.GetUtcNow() });
        await db.SaveChangesAsync(ct);
        await AppendKeysAsync(project, keys, ct);
        await transaction.CommitAsync(ct);
        connections.Notify([body.UserId], "projects", id);
        await NotifyAsync(id, ct);
    }

    public async Task ResolveInvitationAsync(Guid id, Guid userId, bool accept, CancellationToken ct, ProjectKeyWrite? keys = null)
    {
        var leaving = accept ? null : await PrepareKeysAsync(userId, keys, ct);
        var projectId = await db.ProjectInvitations.Where(x => x.Id == id && x.UserId == userId).Select(x => x.ProjectId).SingleOrDefaultAsync(ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var project = await LockAsync(projectId, ct);
        var invitation = await db.ProjectInvitations.SingleOrDefaultAsync(x => x.Id == id && x.UserId == userId, ct) ?? throw ApiException.Missing();
        if (accept)
        {
            if (!await friends.AreFriendsAsync(project.OwnerUserId, userId, ct)) throw ApiException.Forbidden("This invitation is no longer available.");
            if (!await db.ProjectMembers.AnyAsync(x => x.ProjectId == project.Id && x.UserId == userId, ct))
            {
                await RequireProjectCapacityAsync(userId, ct);
                db.ProjectMembers.Add(new ProjectMember { ProjectId = project.Id, UserId = userId });
            }
        }
        db.ProjectInvitations.Remove(invitation); await db.SaveChangesAsync(ct);
        if (!accept) await AppendKeysAsync(project, leaving, ct);
        await TerminalsOf(project.Id).ExecuteUpdateAsync(set => set.SetProperty(x => x.AuthorizationRevision, x => x.AuthorizationRevision + 1), ct);
        await transaction.CommitAsync(ct);
        await NotifyAsync(project.Id, ct); connections.Notify([userId], "projects", project.Id);
        await TerminalAccess.RevokeAsync(db, connections, await TerminalsOf(project.Id).AsNoTracking().ToListAsync(ct), ct);
        connections.Notify((await TerminalPeopleAsync(project.Id, ct)).Append(userId), "sessions");
    }

    public async Task RemoveMemberAsync(Guid id, Guid actor, Guid userId, CancellationToken ct, ProjectKeyWrite? keys = null)
    {
        var prepared = await PrepareKeysAsync(actor, keys, ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var project = await RequireMemberForUpdateAsync(id, actor, ct);
        if (userId == project.OwnerUserId) throw ApiException.Invalid("The owner cannot leave the project. Delete it instead.");
        if (actor != userId && project.OwnerUserId != actor) throw ApiException.Forbidden();
        var viewers = await TerminalPeopleAsync(id, ct);
        var terminals = await TerminalsOf(id).ToListAsync(ct);
        await db.ProjectMembers.Where(x => x.ProjectId == id && x.UserId == userId).ExecuteDeleteAsync(ct);
        await db.ProjectInvitations.Where(x => x.ProjectId == id && x.UserId == userId).ExecuteDeleteAsync(ct);
        foreach (var terminal in terminals)
        {
            if (terminal.OwnerUserId == userId) SessionService.LeaveProject(terminal);
            terminal.AuthorizationRevision = checked(terminal.AuthorizationRevision + 1);
        }
        await db.SaveChangesAsync(ct);
        await AppendKeysAsync(project, prepared, ct);
        await transaction.CommitAsync(ct);
        await TerminalAccess.RevokeAsync(db, connections, terminals, ct);
        await NotifyAsync(id, ct); connections.Notify([userId], "projects", id);
        connections.Notify(viewers.Append(userId), "sessions");
    }

    internal async Task RequireSharedProjectAsync(Guid id, CancellationToken ct)
    {
        if (await db.Projects.AnyAsync(x => x.Id == id && x.Dosi, ct))
            throw ApiException.Invalid("Terminals do not go into Dosi's conversation.");
    }

    private async Task RequireProjectCapacityAsync(Guid userId, CancellationToken ct)
    {
        var projects = ProjectMembership.ProjectsOf(db, userId);
        if (await db.Projects.CountAsync(x => projects.Contains(x.Id) && !x.Dosi, ct) >= Limits.MaxVisibleProjects)
            throw ApiException.Conflict("Leave a project before you join or make another.");
    }

    private async Task NotifyAsync(Guid id, CancellationToken ct) => connections.Notify(await MemberIdsAsync(id, ct), "projects", id);

    private static ApiException TooManyPeople(int most) =>
        ApiException.OverLimit("projectPeople", most, $"This project can have at most {Limits.Count(most, "person", "people")}, with its open invitations.");

    public sealed record CreateProject(Guid Id, string Name, bool Dosi = false);
    public sealed record RenameProject(string Name);
    public sealed record Invite(Guid Id, Guid UserId, ProjectKeyWrite? Keys = null);
}

internal static class ProjectEndpoints
{
    public static void MapProjects(this IEndpointRouteBuilder app)
    {
        app.MapPut("/api/me/project-key", async (ProjectService.RecipientKeyWrite body, DeviceCaller caller, ProjectService projects, CancellationToken ct) =>
        {
            await projects.RegisterProjectKeyAsync(caller.User.Id, caller.Device, body, ct);
            return Results.NoContent();
        }).RequireAuthorization();
        app.MapGet("/api/users/{userId:guid}/project-keys", async (Guid userId, DeviceCaller caller, ProjectService projects, CancellationToken ct) =>
            Results.Ok(await projects.RecipientKeysAsync(caller.User.Id, userId, ct))).RequireAuthorization();
        var group = app.MapGroup("/api/projects").RequireAuthorization();
        group.MapGet("/{id:guid}/keys", async (Guid id, long? afterVersion, DeviceCaller caller, ProjectService projects, CancellationToken ct) =>
            Results.Ok(await projects.KeyHistoryAsync(id, caller.User.Id, afterVersion ?? 0, ct)));
        group.MapPut("/{id:guid}/keys", async (Guid id, ProjectService.ProjectKeyWrite body, DeviceCaller caller, ProjectService projects, CancellationToken ct) =>
        {
            await projects.StoreKeysAsync(id, caller.User.Id, body, ct);
            return Results.NoContent();
        });
        group.MapGet("/{id:guid}/content", async (Guid id, string? kind, long? after, long? before, int? limit, DeviceCaller caller, ProjectService projects,
            CancellationToken ct) => Results.Ok(await projects.ContentAsync(id, caller.User.Id, kind, after ?? 0, before, limit, ct)));
        group.MapPut("/{id:guid}/content/{itemId:guid}", async (Guid id, Guid itemId, ProjectService.ContentWrite body, DeviceCaller caller, ProjectService projects,
            CancellationToken ct) => Results.Ok(await projects.PutContentAsync(id, itemId, caller.User.Id, body, ct, caller.Device)));
        group.MapGet("/", async (DeviceCaller caller, ProjectService projects, CancellationToken ct) =>
            Results.Ok(await projects.ListAsync(caller.User.Id, ct)));
        group.MapPost("/", async (ProjectService.CreateProject body, DeviceCaller caller, ProjectService projects, CancellationToken ct) =>
        {
            await projects.CreateAsync(caller.User.Id, body, ct);
            return Results.NoContent();
        });
        group.MapGet("/{id:guid}", async (Guid id, DeviceCaller caller, ProjectService projects, CancellationToken ct) =>
            Results.Ok(await projects.OpenAsync(id, caller.User.Id, ct)));
        group.MapPatch("/{id:guid}", async (Guid id, ProjectService.RenameProject body, DeviceCaller caller, ProjectService projects, CancellationToken ct) =>
        {
            await projects.RenameAsync(id, caller.User.Id, body.Name, ct);
            return Results.NoContent();
        });
        group.MapDelete("/{id:guid}", async (Guid id, DeviceCaller caller, ProjectService projects, CancellationToken ct) =>
        {
            await projects.DeleteAsync(id, caller.User.Id, ct);
            return Results.NoContent();
        });
        group.MapPost("/{id:guid}/invitations", async (Guid id, ProjectService.Invite body, DeviceCaller caller, ProjectService projects, CancellationToken ct) =>
        {
            await projects.InviteAsync(id, caller.User.Id, body, ct);
            return Results.NoContent();
        });
        foreach (var accept in new[] { true, false })
            group.MapPost($"/invitations/{{id:guid}}/{(accept ? "accept" : "reject")}", async (Guid id, [FromBody] ProjectService.MembershipWrite? body,
                DeviceCaller caller, ProjectService projects, CancellationToken ct) =>
            {
                await projects.ResolveInvitationAsync(id, caller.User.Id, accept, ct, body?.Keys);
                return Results.NoContent();
            });
        group.MapDelete("/{id:guid}/members/{userId:guid}", async (Guid id, Guid userId, [FromBody] ProjectService.MembershipWrite? body, DeviceCaller caller,
            ProjectService projects, CancellationToken ct) =>
        {
            await projects.RemoveMemberAsync(id, caller.User.Id, userId, ct, body?.Keys);
            return Results.NoContent();
        });
    }
}
