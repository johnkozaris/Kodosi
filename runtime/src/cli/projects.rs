use super::{
    Context, Duration, Error, PathBuf, Result, Subcommand, Uuid, Value, friend_id, friends, json,
    local_host, only, output, print_json, project_id, projects, render, request,
};
use crate::{
    projects::{Action, Issue, Snapshot, TaskChange},
    protocol::{GitHubState, RepositoryList, refers_to, short_id},
};

#[derive(clap::Args)]
pub(super) struct Arguments {
    #[arg(
        long = "project",
        value_name = "PROJECT",
        global = true,
        help = "The project: its name, its ID or the end of its ID that `kodosi project list` shows. The default is the project of this terminal"
    )]
    project: Option<String>,
    #[command(subcommand)]
    action: ProjectAction,
}
#[derive(Subcommand)]
enum ProjectAction {
    /// List your projects and your invitations to projects.
    List,
    /// Make a project.
    Create { name: String },
    /// Give the project a new name.
    Rename { name: String },
    /// Delete the project. Its terminals keep running.
    Delete,
    /// Invite a friend to the project.
    Invite { friend: String },
    /// Accept the invitation to a project.
    Accept {
        #[arg(help = INVITATION)]
        name: String,
    },
    /// Reject the invitation to a project.
    Reject {
        #[arg(help = INVITATION)]
        name: String,
    },
    /// Remove a member from the project.
    RemoveMember {
        /// The member: the username of a friend, or the user ID that `kodosi project context`
        /// shows.
        member: String,
    },
    /// Leave the project.
    Leave,
    /// Show the project, its members, terminals, tasks and repositories.
    Context,
    /// Open the project in the Kodosi app.
    Show,
    /// Read the conversation.
    Read {
        /// Print only the messages after this sequence number.
        #[arg(long)]
        since: Option<u64>,
        /// Print the messages before this sequence number.
        #[arg(long)]
        before: Option<u64>,
    },
    /// Post a message.
    Post {
        text: String,
        /// The name of the agent that writes, for example Claude Code.
        #[arg(long)]
        agent: Option<String>,
    },
    /// Wait for activity in the project.
    Wait {
        /// The longest wait, in seconds.
        #[arg(long, default_value_t = 60)]
        timeout: u64,
        /// Wait for activity after this sequence number.
        #[arg(long)]
        after: Option<u64>,
    },
    /// Give every member full control of a terminal, by default this one.
    Share {
        #[arg(help = super::TERMINAL)]
        terminal: Option<String>,
    },
    /// Take a terminal out of its project, by default this one.
    Unshare {
        #[arg(help = super::TERMINAL)]
        terminal: Option<String>,
    },
    /// Work with the project's tasks.
    #[command(subcommand)]
    Task(TaskAction),
    /// Work with the project's repositories.
    #[command(subcommand)]
    Repo(RepoAction),
}
#[derive(Subcommand)]
enum TaskAction {
    /// List the tasks.
    List {
        /// List only the open tasks that nobody claimed.
        #[arg(long)]
        available: bool,
    },
    /// Put a task up for grabs.
    Create {
        title: String,
        /// What the task needs.
        #[arg(long, default_value = "")]
        description: String,
        /// Link a repository of the project, by the name or address that `kodosi project repo list`
        /// shows. Repeat it for more.
        #[arg(long = "repo")]
        repositories: Vec<String>,
        /// Also open the task as an issue in this repository of the project. A change of the
        /// task then changes the issue.
        #[arg(long)]
        issue: Option<String>,
    },
    /// Take a task.
    Claim {
        #[arg(help = TASK)]
        task: String,
    },
    /// Give a task back.
    Release {
        #[arg(help = TASK)]
        task: String,
    },
    /// Close a task.
    Close {
        #[arg(help = TASK)]
        task: String,
        /// The result or a pull request link.
        #[arg(long)]
        note: Option<String>,
    },
    /// Open a closed task again.
    Reopen {
        #[arg(help = TASK)]
        task: String,
    },
    /// Make a task of a GitHub or Gitea issue. A change of the task changes the issue.
    Import {
        #[arg(help = REPOSITORY)]
        repository: String,
        number: u64,
    },
}
#[derive(Subcommand)]
enum RepoAction {
    /// List the repositories.
    List {
        /// List the repositories that you can add: the origins of this computer's terminals,
        /// then your GitHub repositories.
        #[arg(long)]
        available: bool,
    },
    /// Link a repository, by default the origin of the current folder's checkout.
    Add {
        /// A repository address, or a name that `kodosi project repo list --available` shows.
        repository: Option<String>,
        /// Use the origin of the checkout in this folder.
        #[arg(long)]
        directory: Option<PathBuf>,
        /// github or gitea, when the address does not show it.
        #[arg(long)]
        provider: Option<String>,
    },
    /// List the open issues of a repository.
    Issues {
        #[arg(help = REPOSITORY)]
        repository: String,
    },
}

const INVITATION: &str =
    "The project of the invitation, by the name that `kodosi project list` shows";
const TASK: &str =
    "The task: its title, its ID or the end of its ID that `kodosi project task list` shows";
const REPOSITORY: &str =
    "The repository: its name or address that `kodosi project repo list` shows";

fn terminal() -> Option<String> {
    super::this_terminal()
}

fn this_agent(context: &Context) -> Option<String> {
    let id = Uuid::parse_str(&terminal()?).ok()?;
    let program = context.session(id).ok()?["program"].as_str()?;
    crate::agents::Agent::named(program).map(|agent| agent.label().to_owned())
}

pub(super) async fn run(
    args: Arguments,
    client: &mut local_host::Client,
    context: &mut Context,
    json_output: bool,
) -> Result<()> {
    let catalog = projects(client, context).await?;
    let command = match &args.action {
        ProjectAction::List => {
            return output(&catalog, json_output, &render::projects(&catalog));
        }
        ProjectAction::Unshare { terminal: chosen } => {
            return share_terminal(client, context, None, chosen.clone(), json_output).await;
        }
        ProjectAction::Create { name } => Some(json!({"type":"project.create","name":name})),
        ProjectAction::Accept { name } => {
            Some(answer("project.invitation.accept", &catalog, name)?)
        }
        ProjectAction::Reject { name } => {
            Some(answer("project.invitation.reject", &catalog, name)?)
        }
        _ => None,
    };
    if let Some(command) = command {
        return change(client, context, command, json_output).await;
    }
    let project = resolve_project(args.project, &catalog, context)?;
    let command = match &args.action {
        ProjectAction::Context => return show_context(client, context, &project).await,
        ProjectAction::Share { terminal: chosen } => {
            return share_terminal(client, context, Some(&project), chosen.clone(), json_output)
                .await;
        }
        ProjectAction::Rename { name } => Some(json!({"type":"project.rename","name":name})),
        ProjectAction::Show => Some(json!({"type":"project.show"})),
        ProjectAction::Delete => Some(json!({"type":"project.delete"})),
        ProjectAction::Leave => Some(json!({"type":"project.leave"})),
        ProjectAction::Invite { friend } => {
            let user = friend_id(&friends(client, context).await?, friend)?;
            Some(json!({"type":"project.invite","userId":user}))
        }
        ProjectAction::RemoveMember { member } => {
            let user = if Uuid::parse_str(member).is_ok() {
                member.clone()
            } else {
                friend_id(&friends(client, context).await?, member)?
            };
            Some(json!({"type":"project.removeMember","userId":user}))
        }
        _ => None,
    };
    if let Some(mut command) = command {
        command["projectId"] = json!(project);
        return change(client, context, command, json_output).await;
    }
    if let ProjectAction::Post { text, agent } = args.action {
        let action = Action::Post {
            text,
            agent: agent.or_else(|| this_agent(context)),
            terminal_id: terminal(),
        };
        return done(&call(client, context, &project, action).await?, json_output);
    }
    let before = match &args.action {
        ProjectAction::Read { before, .. } => *before,
        _ => None,
    };
    let initial = call(client, context, &project, Action::Read { before }).await?;
    let mut snapshot: Snapshot = serde_json::from_value(initial["content"].clone())?;
    match args.action {
        ProjectAction::Read { since, .. } => {
            if let Some(after) = since {
                snapshot.messages.retain(|message| message.sequence > after);
            }
            output(
                &serde_json::to_value(&snapshot)?,
                json_output,
                &conversation(&snapshot),
            )
        }
        ProjectAction::Wait { timeout, after } => {
            wait(client, context, &project, &snapshot, after, timeout).await
        }
        ProjectAction::Task(TaskAction::List { available }) => {
            show_tasks(&snapshot, available, json_output)
        }
        ProjectAction::Task(action) => {
            let action = task_action(action, &snapshot)?;
            done(&call(client, context, &project, action).await?, json_output)
        }
        ProjectAction::Repo(action) => {
            repository(action, client, context, &project, &snapshot, json_output).await
        }
        _ => Err(Error::Invalid("Unsupported project action.".into())),
    }
}

async fn repository(
    action: RepoAction,
    client: &mut local_host::Client,
    context: &mut Context,
    project: &str,
    snapshot: &Snapshot,
    json_output: bool,
) -> Result<()> {
    let action = match action {
        RepoAction::List { available: false } => {
            return output(
                &json!({"repositories":snapshot.repositories}),
                json_output,
                &snapshot
                    .repositories
                    .iter()
                    .map(|repo| format!("{}  {}", repo.name, repo.url))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        RepoAction::List { available: true } => {
            let choices = repository_choices(client, context, project).await?;
            return show_choices(&addable(choices, snapshot), json_output);
        }
        RepoAction::Add {
            repository: Some(name),
            ..
        } if !name.contains(':') => {
            chosen(&repository_choices(client, context, project).await?, &name)?
        }
        action => repository_action(action, snapshot)?,
    };
    done(&call(client, context, project, action).await?, json_output)
}

fn done(result: &Value, json_output: bool) -> Result<()> {
    if !result["issues"].is_null() {
        let issues: Vec<Issue> = serde_json::from_value(result["issues"].clone())?;
        return output(&result["issues"], json_output, &issue_lines(&issues));
    }
    let suffix = result["result"]["itemId"]
        .as_str()
        .map_or_else(String::new, |id| format!(" {}", short_id(id)));
    output(result, json_output, &format!("Done.{suffix}"))
}

fn resolve_project(
    reference: Option<String>,
    catalog: &Value,
    context: &Context,
) -> Result<String> {
    if let Some(reference) = reference {
        return project_id(catalog, &reference);
    }
    if let Some(id) = terminal().and_then(|id| {
        context
            .sessions
            .iter()
            .find(|session| session["id"].as_str() == Some(&id))
            .and_then(|session| session["projectId"].as_str())
            .map(str::to_owned)
    }) {
        return Ok(id);
    }
    let list = catalog["projects"]
        .as_array()
        .ok_or_else(|| Error::Invalid("Create or join a project first.".into()))?;
    if list.len() != 1 {
        return Err(Error::Invalid(
            "Choose a project with --project, or run this command inside one of its terminals."
                .into(),
        ));
    }
    list[0]["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or(Error::Stale)
}

async fn show_context(
    client: &mut local_host::Client,
    context: &mut Context,
    project: &str,
) -> Result<()> {
    let event = request(
        client,
        context,
        json!({"type":"project.open","requestId":Uuid::now_v7(),"projectId":project}),
        false,
    )
    .await?;
    let read = call(client, context, project, Action::Read { before: None }).await?;
    let snapshot: Snapshot = serde_json::from_value(read["content"].clone())?;
    let terminals = context
        .sessions
        .iter()
        .filter(|session| session["projectId"].as_str() == Some(project))
        .collect::<Vec<_>>();
    print_json(&json!({
        "project":event["project"],"members":event["members"],"terminals":terminals,
        "currentTerminalId":terminal(),"tasks":snapshot.tasks,
        "repositories":snapshot.repositories,"sequence":snapshot.sequence
    }))
}

async fn share_terminal(
    client: &mut local_host::Client,
    context: &mut Context,
    project: Option<&str>,
    chosen: Option<String>,
    json_output: bool,
) -> Result<()> {
    let reference = chosen
        .or_else(terminal)
        .ok_or_else(|| Error::Invalid("Choose a terminal.".into()))?;
    let id = context.session_id(&reference)?;
    let event = request(
        client,
        context,
        json!({"type":"session.attachProject","requestId":Uuid::now_v7(),"sessionId":id,
        "expectedRuntimeIncarnationId":context.incarnation(id)?,"projectId":project}),
        json_output,
    )
    .await?;
    let text = if project.is_some() {
        "The terminal is shared with the project."
    } else {
        "The terminal is no longer shared with a project."
    };
    output(&event, json_output, text)
}

async fn change(
    client: &mut local_host::Client,
    context: &mut Context,
    command: Value,
    json_output: bool,
) -> Result<()> {
    let event = request(client, context, command, json_output).await?;
    output(&event, json_output, &render::result(&event, context))
}

fn answer(operation: &str, catalog: &Value, reference: &str) -> Result<Value> {
    let matches = catalog["invitations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|invitation| {
            let text = |key: &str| invitation[key].as_str().unwrap_or_default();
            invitation["id"] == reference
                || refers_to(reference, text("projectId"), text("projectName"))
        })
        .filter_map(|invitation| invitation["id"].as_str())
        .collect::<Vec<_>>();
    let id = only(&matches, "invitation", reference, "kodosi project list")?;
    Ok(json!({"type":operation,"invitationId":id}))
}

async fn wait(
    client: &mut local_host::Client,
    context: &mut Context,
    project: &str,
    snapshot: &Snapshot,
    after: Option<u64>,
    timeout: u64,
) -> Result<()> {
    if timeout == 0 || timeout > 3600 {
        return Err(Error::Invalid(
            "Use a wait timeout from 1 to 3600 seconds.".into(),
        ));
    }
    let after = after.unwrap_or(snapshot.sequence);
    if snapshot.sequence > after {
        return print_json(&serde_json::to_value(snapshot)?);
    }
    let next = tokio::time::timeout(Duration::from_secs(timeout), async {
        loop {
            let frame = client.next().await?;
            if let Some(event) = frame.get("event")
                && context.observe(event)
                && event["type"] == "project.snapshot"
                && event["content"]["projectId"] == project
                && event["content"]["sequence"]
                    .as_u64()
                    .is_some_and(|sequence| sequence > after)
            {
                return Ok::<_, Error>(event["content"].clone());
            }
        }
    })
    .await;
    match next {
        Ok(value) => print_json(&value?),
        Err(_) => print_json(&json!({"projectId":project,"sequence":after,"timedOut":true})),
    }
}

fn issue_lines(issues: &[Issue]) -> String {
    issues
        .iter()
        .map(|issue| {
            let state = if issue.closed { "closed" } else { "open" };
            format!("#{}  {state}  {}", issue.number, issue.title)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn show_tasks(snapshot: &Snapshot, available: bool, json_output: bool) -> Result<()> {
    let tasks = snapshot
        .tasks
        .iter()
        .filter(|task| !available || (!task.closed && task.assigned_to.is_none()))
        .collect::<Vec<_>>();
    let text = tasks
        .iter()
        .map(|task| {
            format!(
                "{}  {}  {}",
                short_id(&task.id),
                if task.closed {
                    "done"
                } else if task.assigned_to.is_some() {
                    "in progress"
                } else {
                    "up for grabs"
                },
                task.title
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    output(&json!({"tasks":tasks}), json_output, &text)
}

fn task_action(action: TaskAction, snapshot: &Snapshot) -> Result<Action> {
    let (reference, change, note) = match action {
        TaskAction::Create {
            title,
            description,
            repositories,
            issue,
        } => {
            return Ok(Action::CreateTask {
                title,
                description,
                repository_ids: repositories
                    .iter()
                    .map(|reference| repo_id(snapshot, reference))
                    .collect::<Result<_>>()?,
                terminal_id: terminal(),
                issue_repository_id: issue
                    .map(|reference| repo_id(snapshot, &reference))
                    .transpose()?,
            });
        }
        TaskAction::Import { repository, number } => {
            return Ok(Action::ImportIssue {
                repository_id: repo_id(snapshot, &repository)?,
                number,
            });
        }
        TaskAction::Claim { task } => (task, TaskChange::Claim, None),
        TaskAction::Release { task } => (task, TaskChange::Release, None),
        TaskAction::Close { task, note } => (task, TaskChange::Close, note),
        TaskAction::Reopen { task } => (task, TaskChange::Reopen, None),
        TaskAction::List { .. } => return Err(Error::Invalid("Expected a task change.".into())),
    };
    let tasks = snapshot
        .tasks
        .iter()
        .map(|task| (task.id.as_str(), task.title.as_str()));
    let matching = crate::protocol::referred(&reference, tasks);
    Ok(Action::UpdateTask {
        task_id: only(&matching, "task", &reference, "kodosi project task list")?.to_owned(),
        change,
        note,
        terminal_id: terminal(),
    })
}

fn repository_action(action: RepoAction, snapshot: &Snapshot) -> Result<Action> {
    match action {
        RepoAction::Issues { repository } => Ok(Action::Issues {
            repository_id: repo_id(snapshot, &repository)?,
        }),
        RepoAction::Add {
            repository,
            directory,
            provider,
        } => {
            let url = if let Some(url) = repository {
                url
            } else {
                let mut command = std::process::Command::new("git");
                if let Some(directory) = directory {
                    command.arg("-C").arg(directory);
                }
                let result = command.args(["remote", "get-url", "origin"]).output()?;
                if !result.status.success() {
                    return Err(Error::Invalid(
                        "Choose a repository URL or a working folder with an origin remote.".into(),
                    ));
                }
                String::from_utf8(result.stdout)
                    .map_err(|_| Error::Invalid("Repository address is not UTF-8.".into()))?
                    .trim()
                    .to_owned()
            };
            Ok(Action::AddRepository { url, provider })
        }
        RepoAction::List { .. } => Err(Error::Invalid("Expected a repository action.".into())),
    }
}

async fn repository_choices(
    client: &mut local_host::Client,
    context: &mut Context,
    project: &str,
) -> Result<RepositoryList> {
    let answer = request(
        client,
        context,
        json!({"type":"repositories.list","projectId":project}),
        false,
    )
    .await?;
    Ok(serde_json::from_value(answer["repositories"].clone())?)
}

fn addable(mut list: RepositoryList, snapshot: &Snapshot) -> RepositoryList {
    list.repositories.retain(|choice| {
        !snapshot
            .repositories
            .iter()
            .any(|linked| linked.url.eq_ignore_ascii_case(&choice.url))
    });
    list
}

fn show_choices(list: &RepositoryList, json_output: bool) -> Result<()> {
    let mut lines = list
        .repositories
        .iter()
        .map(|choice| {
            let private = if choice.private { "  private" } else { "" };
            format!("{}  {}{private}", choice.name, choice.url)
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push("No repositories to add.".to_owned());
    }
    match list.github {
        GitHubState::Ready => {}
        GitHubState::SignedOut => {
            lines.push("Run `gh auth login` to list your GitHub repositories too.".to_owned());
        }
        GitHubState::Missing => {
            lines.push("Install GitHub CLI to list your GitHub repositories too.".to_owned());
        }
    }
    output(&serde_json::to_value(list)?, json_output, &lines.join("\n"))
}

fn chosen(list: &RepositoryList, name: &str) -> Result<Action> {
    let matching = list
        .repositories
        .iter()
        .filter(|choice| choice.name.eq_ignore_ascii_case(name))
        .map(|choice| choice.url.as_str())
        .collect::<Vec<_>>();
    let url = only(
        &matching,
        "repository",
        name,
        "kodosi project repo list --available",
    )?;
    Ok(Action::AddRepository {
        url: url.to_owned(),
        provider: None,
    })
}

fn repo_id(snapshot: &Snapshot, reference: &str) -> Result<String> {
    let matching = snapshot
        .repositories
        .iter()
        .filter(|repo| refers_to(reference, &repo.id, &repo.name) || repo.url == reference)
        .map(|repo| repo.id.as_str())
        .collect::<Vec<_>>();
    only(
        &matching,
        "repository",
        reference,
        "kodosi project repo list",
    )
    .map(str::to_owned)
}
fn conversation(snapshot: &Snapshot) -> String {
    snapshot
        .messages
        .iter()
        .map(|message| {
            format!(
                "{}{}: {}",
                message.author_name,
                message
                    .agent
                    .as_ref()
                    .map_or_else(String::new, |agent| format!(" · {agent}")),
                message.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
async fn call(
    client: &mut local_host::Client,
    context: &mut Context,
    project: &str,
    action: Action,
) -> Result<Value> {
    let result = request(
        client,
        context,
        json!({"type":"project.act","projectId":project,"action":action}),
        true,
    )
    .await?;
    let snapshot = match result.get("content") {
        Some(page) => page.clone(),
        None => context.contents.get(project).cloned().unwrap_or_default(),
    };
    Ok(
        json!({"content":snapshot,"issues":result.get("issues").cloned().unwrap_or_default(),"result":result}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        serde_json::from_value(json!({
            "projectId":Uuid::now_v7(),"messages":[],"sequence":0,"hasOlder":false,"historyTrimmed":false,
            "repositories":[
                {"id":"019f0000-0000-7000-8000-000000000001","name":"team/api","url":"https://github.com/team/api","host":"github.com","owner":"team","repository":"api","provider":"github"},
                {"id":"019f0000-0000-7000-8000-000000000002","name":"team/ui","url":"https://forge.example/team/ui","host":"forge.example","owner":"team","repository":"ui","provider":"gitea"}
            ],
            "tasks":[
                {"id":"019f0000-0000-7000-8000-000000000003","version":1,"title":"Review","description":"","closed":false,"assignedTo":null,"assignedName":null,"terminalId":null,"repositoryIds":[],"note":null,"issue":null}
            ]
        })).unwrap()
    }

    #[test]
    fn commands_resolve_work_across_repositories_without_a_shared_checkout() {
        let mut project = snapshot();
        let create = task_action(
            TaskAction::Create {
                title: "Join API and UI".into(),
                description: "Shared context".into(),
                repositories: vec!["team/api".into(), "000002".into()],
                issue: Some("team/ui".into()),
            },
            &project,
        )
        .unwrap();
        let Action::CreateTask {
            repository_ids,
            issue_repository_id,
            ..
        } = create
        else {
            panic!()
        };
        assert_eq!(
            issue_repository_id.as_ref(),
            Some(&project.repositories[1].id)
        );
        assert_eq!(
            repository_ids,
            project
                .repositories
                .iter()
                .map(|r| r.id.clone())
                .collect::<Vec<_>>()
        );
        for command in [
            TaskAction::Claim {
                task: "Review".into(),
            },
            TaskAction::Release {
                task: "000003".into(),
            },
            TaskAction::Close {
                task: project.tasks[0].id.clone(),
                note: Some("PR #27".into()),
            },
            TaskAction::Reopen {
                task: "Review".into(),
            },
        ] {
            let Action::UpdateTask { task_id, .. } = task_action(command, &project).unwrap() else {
                panic!()
            };
            assert_eq!(task_id, project.tasks[0].id);
        }
        let imported = task_action(
            TaskAction::Import {
                repository: project.repositories[1].url.clone(),
                number: 4,
            },
            &project,
        )
        .unwrap();
        assert!(matches!(imported, Action::ImportIssue { number: 4, .. }));
        let issues = repository_action(
            RepoAction::Issues {
                repository: "team/api".into(),
            },
            &project,
        )
        .unwrap();
        assert!(
            matches!(issues,Action::Issues { repository_id } if repository_id == project.repositories[0].id)
        );
        let add = repository_action(
            RepoAction::Add {
                repository: Some("https://github.com/another/repo".into()),
                directory: None,
                provider: None,
            },
            &project,
        )
        .unwrap();
        assert!(matches!(add, Action::AddRepository { .. }));
        assert!(repo_id(&project, "unknown").is_err());
        assert!(repo_id(&project, "00000").is_err());
        let mut duplicate = project.tasks[0].clone();
        duplicate.id = Uuid::now_v7().to_string();
        project.tasks.push(duplicate);
        for task in ["Review", "unknown"] {
            assert!(task_action(TaskAction::Claim { task: task.into() }, &project).is_err());
        }
    }

    #[test]
    fn a_repository_can_be_added_by_its_name_from_the_choices_that_the_project_lacks() {
        let project = snapshot();
        let choice = |name: &str, url: &str| crate::protocol::RepositoryChoice {
            name: name.into(),
            url: url.into(),
            provider: "github".into(),
            folder: None,
            private: false,
        };
        let choices = RepositoryList {
            github: GitHubState::Ready,
            repositories: vec![
                choice("team/api", "https://GitHub.com/team/api"),
                choice("ana/notes", "https://github.com/ana/notes"),
                choice("team/web", "https://github.com/team/web"),
                choice("team/web", "https://forge.example/team/web"),
            ],
        };
        let open = addable(choices.clone(), &project);
        assert_eq!(
            open.repositories
                .iter()
                .map(|choice| choice.url.as_str())
                .collect::<Vec<_>>(),
            [
                "https://github.com/ana/notes",
                "https://github.com/team/web",
                "https://forge.example/team/web"
            ]
        );
        let Action::AddRepository { url, .. } = chosen(&choices, "ANA/notes").unwrap() else {
            panic!()
        };
        assert_eq!(url, "https://github.com/ana/notes");
        assert!(chosen(&choices, "team/web").is_err());
        assert!(chosen(&choices, "ana/unknown").is_err());
    }

    #[test]
    fn project_names_must_be_unambiguous_and_agent_authorship_is_visible() {
        let context = Context::from_events(&[]);
        let id = Uuid::now_v7().to_string();
        let projects = json!({"projects":[{"id":id,"name":"Workspace"}]});
        assert_eq!(
            resolve_project(Some("Workspace".into()), &projects, &context).unwrap(),
            id
        );
        assert_eq!(resolve_project(None, &projects, &context).unwrap(), id);
        assert!(resolve_project(None, &json!({"projects":[]}), &context).is_err());
        assert!(
            resolve_project(
                None,
                &json!({"projects":[{"id":id},{"id":"second"}]}),
                &context
            )
            .is_err()
        );
        assert!(resolve_project(None, &json!({}), &context).is_err());
        let mut project = snapshot();
        project.messages = serde_json::from_value(json!([
            {"id":"1","sequence":1,"authorId":id,"authorName":"Alice","agent":null,"terminalId":null,"text":"Can someone review?","createdAt":"now"},
            {"id":"2","sequence":2,"authorId":id,"authorName":"Alice","agent":"Claude","terminalId":null,"text":"I can pick it up.","createdAt":"now"}
        ])).unwrap();
        assert_eq!(
            conversation(&project),
            "Alice: Can someone review?\nAlice · Claude: I can pick it up."
        );
    }

    #[test]
    fn issues_read_as_lines_of_number_state_and_title() {
        let issues: Vec<Issue> = serde_json::from_value(json!([
            {"repositoryId":"r","number":12,"title":"Fix sign-in","body":"","url":"https://example.invalid/12","closed":false,"assignees":[]},
            {"repositoryId":"r","number":7,"title":"Old crash","body":"","url":"https://example.invalid/7","closed":true,"assignees":["ada"]}
        ]))
        .unwrap();
        assert_eq!(
            issue_lines(&issues),
            "#12  open  Fix sign-in\n#7  closed  Old crash"
        );
    }
}
