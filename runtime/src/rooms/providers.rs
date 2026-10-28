#[cfg(feature = "host")]
use std::{process::Stdio, time::Duration};

#[cfg(feature = "host")]
use reqwest::Method;
use reqwest::Url;
#[cfg(feature = "host")]
use serde_json::{Value, json};
#[cfg(feature = "host")]
use tokio::io::AsyncWriteExt as _;
#[cfg(feature = "host")]
use zeroize::Zeroizing;

use super::{Issue, Repository, TaskChange};
use crate::network::{Result, invalid};
#[cfg(feature = "host")]
use crate::protocol::{GitHubState, RepositoryChoice, RepositoryList};

pub(crate) fn repository(remote: &str, provider: Option<&str>) -> Result<Repository> {
    let remote = remote.trim();
    let normalized = if !remote.contains("://")
        && let Some((host, path)) = remote.split_once(':')
    {
        format!(
            "https://{}/{}",
            host.rsplit('@').next().unwrap_or(host),
            path
        )
    } else if remote.starts_with("ssh://") {
        let ssh = Url::parse(remote).map_err(|_| invalid("Invalid repository address."))?;
        format!(
            "https://{}{}",
            ssh.host_str()
                .ok_or_else(|| invalid("Repository host is missing."))?,
            ssh.path()
        )
    } else {
        remote.to_owned()
    };
    let mut url =
        Url::parse(&normalized).map_err(|_| invalid("Use a repository URL or Git remote."))?;
    crate::identity::oidc::validate_url(&url)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid("Use a repository address without credentials."));
    }
    let path = url.path().trim_matches('/').trim_end_matches(".git");
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() != 2
        || parts.iter().any(|part| {
            part.is_empty()
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
    {
        return Err(invalid(
            "Use the repository address, including its owner and name.",
        ));
    }
    let (owner, name) = (parts[0].to_owned(), parts[1].to_owned());
    let host = url
        .host_str()
        .ok_or_else(|| invalid("Repository host is missing."))?
        .to_owned();
    let kind = provider.unwrap_or(if host == "github.com" {
        "github"
    } else {
        "gitea"
    });
    if !matches!(kind, "github" | "gitea") {
        return Err(invalid("Choose GitHub or Gitea."));
    }
    url.set_path(&format!("/{owner}/{name}"));
    url.set_query(None);
    url.set_fragment(None);
    Ok(Repository {
        id: String::new(),
        name: format!("{owner}/{name}"),
        url: url.to_string().trim_end_matches('/').to_owned(),
        host,
        owner,
        repository: name,
        provider: kind.to_owned(),
    })
}

#[cfg(feature = "host")]
async fn gh() -> Option<std::path::PathBuf> {
    crate::agents::executable(&kodosi_os::process::default_shell(), "gh").await
}

#[cfg(feature = "host")]
async fn gh_token(gh: &std::path::Path, host: &str) -> Result<Option<Zeroizing<String>>> {
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new(gh)
            .args(["auth", "token", "--hostname", host])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await;
    let Ok(Ok(output)) = output else {
        return Ok(None);
    };
    if !output.status.success() {
        return Ok(None);
    }
    let token = Zeroizing::new(
        String::from_utf8(output.stdout)
            .map_err(|_| invalid("GitHub credentials are unavailable."))?
            .trim()
            .to_owned(),
    );
    Ok((!token.is_empty()).then_some(token))
}

#[cfg(feature = "host")]
async fn token(repo: &Repository) -> Result<Option<Zeroizing<String>>> {
    if repo.provider == "github"
        && let Some(gh) = gh().await
        && let Some(token) = gh_token(&gh, &repo.host).await?
    {
        return Ok(Some(token));
    }
    let url = Url::parse(&repo.url).map_err(|_| invalid("Invalid repository address."))?;
    if repo.provider == "gitea"
        && let (Ok(origin), Ok(value)) = (
            std::env::var("KODOSI_GITEA_URL"),
            std::env::var("KODOSI_GITEA_TOKEN"),
        )
    {
        let origin =
            Url::parse(&origin).map_err(|_| invalid("Invalid configured Gitea address."))?;
        if origin.origin() == url.origin() {
            return Ok(Some(Zeroizing::new(value)));
        }
    }
    let host = url
        .port()
        .map_or_else(|| repo.host.clone(), |port| format!("{}:{port}", repo.host));
    let Ok(mut child) = tokio::process::Command::new("git")
        .args(["credential", "fill"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    else {
        return Ok(None);
    };
    if let Some(mut input) = child.stdin.take() {
        input
            .write_all(format!("protocol={}\nhost={host}\n\n", url.scheme()).as_bytes())
            .await?;
    }
    let result = tokio::time::timeout(Duration::from_secs(5), child.wait_with_output()).await;
    if let Ok(Ok(output)) = result
        && output.status.success()
    {
        let text = Zeroizing::new(
            String::from_utf8(output.stdout)
                .map_err(|_| invalid("Repository credentials are unavailable."))?,
        );
        return Ok(text.lines().find_map(|line| {
            line.strip_prefix("password=")
                .map(|value| Zeroizing::new(value.to_owned()))
        }));
    }
    Ok(None)
}

#[cfg(feature = "host")]
fn api_url(repo: &Repository, path: &str) -> Result<Url> {
    let mut url = Url::parse(&repo.url).map_err(|_| invalid("Invalid repository address."))?;
    let prefix = if repo.provider == "github" {
        if repo.host == "github.com" {
            url.set_host(Some("api.github.com"))
                .map_err(|_| invalid("Invalid GitHub host."))?;
            ""
        } else {
            "/api/v3"
        }
    } else {
        "/api/v1"
    };
    let (path, query) = path
        .split_once('?')
        .map_or((path, None), |(path, query)| (path, Some(query)));
    url.set_path(&format!("{prefix}/{path}"));
    url.set_query(query);
    Ok(url)
}

#[cfg(feature = "host")]
async fn api(repo: &Repository, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Kodosi")
        .build()?;
    let mut request = client
        .request(method, api_url(repo, path)?)
        .header("Accept", "application/json");
    if let Some(token) = token(repo).await? {
        request = request.bearer_auth(token.as_str());
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await?;
    let status = response.status();
    let limited = response
        .headers()
        .get("x-ratelimit-remaining")
        .is_some_and(|remaining| remaining.as_bytes() == b"0");
    let bytes = crate::identity::oidc::read_bounded(response, 2 * 1024 * 1024).await?;
    if !status.is_success() {
        return Err(invalid(match status.as_u16() {
            403 | 429 if limited => "The issue provider limits your requests now. Try again later, or connect your account for a higher limit.".to_owned(),
            403 => "Your provider account or token does not have access to these issues. Check its issue permissions.".to_owned(),
            401 => connect_message(repo).to_owned(),
            404 => "The repository or issue is unavailable to your account.".to_owned(),
            409 | 422 => "The issue changed or the requested update could not be applied. Refresh and try again.".to_owned(),
            429 => "The issue provider is busy. Try again shortly.".to_owned(),
            _ => format!("The issue provider returned HTTP {}.", status.as_u16()),
        }));
    }
    if bytes.is_empty() {
        Ok(Value::Null)
    } else {
        Ok(serde_json::from_slice(&bytes)?)
    }
}

#[cfg(feature = "host")]
fn connect_message(repo: &Repository) -> &'static str {
    if repo.provider == "github" {
        "Connect your GitHub account with gh auth login, then try again."
    } else {
        "Connect your Gitea account through Git credentials or KODOSI_GITEA_URL and KODOSI_GITEA_TOKEN, then try again."
    }
}

#[cfg(feature = "host")]
fn issue(repo: &Repository, value: &Value) -> Result<Issue> {
    Ok(Issue {
        repository_id: repo.id.clone(),
        number: value["number"]
            .as_u64()
            .ok_or_else(|| invalid("Issue number is missing."))?,
        title: value["title"].as_str().unwrap_or_default().to_owned(),
        body: value["body"].as_str().unwrap_or_default().to_owned(),
        url: value["html_url"].as_str().unwrap_or_default().to_owned(),
        closed: value["state"].as_str() == Some("closed"),
        assignees: value["assignees"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|user| user["login"].as_str().map(str::to_owned))
            .collect(),
    })
}

#[cfg(feature = "host")]
const LISTED_ISSUES: usize = 100;

#[cfg(feature = "host")]
pub(crate) async fn issues(repo: &Repository, number: Option<u64>) -> Result<Vec<Issue>> {
    let path = format!(
        "repos/{}/{}/issues{}",
        repo.owner,
        repo.repository,
        number.map_or_else(String::new, |n| format!("/{n}"))
    );
    if number.is_some() {
        let value = api(repo, Method::GET, &path, None).await?;
        return Ok(vec![issue(repo, &value)?]);
    }
    let size = if repo.provider == "github" {
        "per_page"
    } else {
        "limit"
    };
    let value = api(
        repo,
        Method::GET,
        &format!("{path}?state=open&{size}={LISTED_ISSUES}"),
        None,
    )
    .await?;
    value
        .as_array()
        .ok_or_else(|| invalid("The provider did not return an issue list."))?
        .iter()
        .filter(|value| value.get("pull_request").is_none_or(Value::is_null))
        .map(|item| issue(repo, item))
        .collect()
}

#[cfg(feature = "host")]
pub(crate) async fn update_issue(
    repo: &Repository,
    number: u64,
    change: TaskChange,
) -> Result<Issue> {
    let path = format!("repos/{}/{}/issues/{number}", repo.owner, repo.repository);
    let change = match change {
        TaskChange::Close => json!({"state":"closed"}),
        TaskChange::Reopen => json!({"state":"open"}),
        TaskChange::Claim => {
            let user = api(repo, Method::GET, "user", None).await?;
            let login = user["login"]
                .as_str()
                .ok_or_else(|| invalid("Your provider account is unavailable."))?;
            json!({"assignees":[login]})
        }
        TaskChange::Release => json!({"assignees":[]}),
    };
    issue(repo, &api(repo, Method::PATCH, &path, Some(change)).await?)
}

#[cfg(feature = "host")]
pub(crate) async fn create_issue(repo: &Repository, title: &str, body: &str) -> Result<Issue> {
    if token(repo).await?.is_none() {
        return Err(invalid(connect_message(repo)));
    }
    let path = format!("repos/{}/{}/issues", repo.owner, repo.repository);
    let created = api(
        repo,
        Method::POST,
        &path,
        Some(json!({"title":title,"body":body})),
    )
    .await?;
    issue(repo, &created)
}

#[cfg(feature = "host")]
const CHOICE_FOLDERS: usize = 16;

#[cfg(feature = "host")]
pub(crate) async fn choices(folders: Vec<String>) -> Result<RepositoryList> {
    let mut repositories = folder_choices(folders).await;
    let (github, account) = account_repositories().await;
    for choice in account {
        if !repositories
            .iter()
            .any(|known| known.url.eq_ignore_ascii_case(&choice.url))
        {
            repositories.push(choice);
        }
    }
    Ok(RepositoryList {
        github,
        repositories,
    })
}

#[cfg(feature = "host")]
async fn folder_choices(folders: Vec<String>) -> Vec<RepositoryChoice> {
    let mut repositories: Vec<RepositoryChoice> = Vec::new();
    for folder in folders.into_iter().take(CHOICE_FOLDERS) {
        let Some(repo) = origin(&folder)
            .await
            .and_then(|remote| repository(&remote, None).ok())
        else {
            continue;
        };
        if !repositories
            .iter()
            .any(|known| known.url.eq_ignore_ascii_case(&repo.url))
        {
            repositories.push(RepositoryChoice {
                name: repo.name,
                url: repo.url,
                provider: repo.provider,
                folder: Some(folder),
                private: false,
            });
        }
    }
    repositories
}

#[cfg(feature = "host")]
async fn origin(folder: &str) -> Option<String> {
    let output = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::process::Command::new("git")
            .args(["-C", folder, "remote", "get-url", "origin"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    let remote = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (output.status.success() && !remote.is_empty()).then_some(remote)
}

#[cfg(feature = "host")]
async fn account_repositories() -> (GitHubState, Vec<RepositoryChoice>) {
    let Some(gh) = gh().await else {
        return (GitHubState::Missing, Vec::new());
    };
    if !matches!(gh_token(&gh, "github.com").await, Ok(Some(_))) {
        return (GitHubState::SignedOut, Vec::new());
    }
    let Ok(github) = repository("https://github.com/github/github", Some("github")) else {
        return (GitHubState::Ready, Vec::new());
    };
    let listed = api(
        &github,
        Method::GET,
        "user/repos?per_page=100&sort=pushed&affiliation=owner,collaborator,organization_member",
        None,
    )
    .await;
    let items = match listed {
        Ok(Value::Array(items)) => items,
        Ok(_) => Vec::new(),
        Err(error) => {
            tracing::warn!(%error, "could not list the GitHub repositories");
            Vec::new()
        }
    };
    (GitHubState::Ready, account_choices(&items))
}

#[cfg(feature = "host")]
fn account_choices(items: &[Value]) -> Vec<RepositoryChoice> {
    items
        .iter()
        .filter_map(|item| {
            let repo = repository(item["html_url"].as_str()?, Some("github")).ok()?;
            Some(RepositoryChoice {
                name: repo.name,
                url: repo.url,
                provider: repo.provider,
                folder: None,
                private: item["private"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

#[cfg(not(feature = "host"))]
#[expect(
    clippy::unused_async,
    reason = "issues are created through the provider credentials of a computer"
)]
pub(crate) async fn create_issue(_repo: &Repository, _title: &str, _body: &str) -> Result<Issue> {
    Err(invalid("Create issues from a computer."))
}

#[cfg(not(feature = "host"))]
#[expect(
    clippy::unused_async,
    reason = "issues are read through the provider credentials of a computer"
)]
pub(crate) async fn issues(_repo: &Repository, _number: Option<u64>) -> Result<Vec<Issue>> {
    Err(invalid("Import issues from a computer."))
}

#[cfg(not(feature = "host"))]
#[expect(
    clippy::unused_async,
    reason = "linked issues change through the provider credentials of a computer"
)]
pub(crate) async fn update_issue(
    _repo: &Repository,
    _number: u64,
    _change: TaskChange,
) -> Result<Issue> {
    Err(invalid("Linked issues are changed from a computer."))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remotes_from_multiple_checkouts_have_one_repository_identity() {
        let ssh = repository("git@github.com:team/project.git", None).unwrap();
        let https = repository("https://github.com/team/project", None).unwrap();
        assert_eq!(ssh.url, https.url);
        assert_eq!(ssh.provider, "github");
        let gitea = repository("ssh://git@forge.example:2222/team/project.git", None).unwrap();
        assert_eq!(gitea.url, "https://forge.example/team/project");
        assert_eq!(gitea.provider, "gitea");
        assert!(repository("https://token@github.com/team/project", None).is_err());
    }

    #[cfg(not(feature = "host"))]
    #[tokio::test]
    async fn a_viewer_leaves_issue_providers_to_a_computer() {
        let repository = repository("https://github.com/team/api", None).unwrap();
        assert_eq!(
            issues(&repository, None).await.unwrap_err().to_string(),
            "Import issues from a computer."
        );
        assert_eq!(
            update_issue(&repository, 7, TaskChange::Close)
                .await
                .unwrap_err()
                .to_string(),
            "Linked issues are changed from a computer."
        );
    }

    #[cfg(feature = "host")]
    #[test]
    fn provider_endpoints_and_issue_fields_preserve_repository_and_assignment() {
        let github = repository("https://github.com/team/api", None).unwrap();
        assert_eq!(
            api_url(&github, "repos/team/api/issues?state=open&per_page=100")
                .unwrap()
                .as_str(),
            "https://api.github.com/repos/team/api/issues?state=open&per_page=100"
        );
        let enterprise = repository("https://git.example/team/api", Some("github")).unwrap();
        assert_eq!(
            api_url(&enterprise, "user").unwrap().as_str(),
            "https://git.example/api/v3/user"
        );
        let gitea = repository("https://forge.example/team/ui", None).unwrap();
        assert_eq!(
            api_url(&gitea, "user").unwrap().as_str(),
            "https://forge.example/api/v1/user"
        );
        let mapped = issue(&gitea,&json!({"number":7,"title":"Fix layout","body":null,"state":"closed",
            "html_url":"https://forge.example/team/ui/issues/7","assignees":[{"login":"alice"},{"login":"bob"}]})).unwrap();
        assert_eq!(mapped.title, "Fix layout");
        assert_eq!(mapped.body, "");
        assert_eq!(mapped.assignees, ["alice", "bob"]);
        assert!(mapped.closed);
        assert!(issue(&gitea, &json!({"title":"missing identity"})).is_err());
    }

    #[cfg(feature = "host")]
    #[test]
    fn an_account_list_keeps_the_repositories_that_kodosi_can_link() {
        let items = json!([
            {"html_url":"https://github.com/maya/notes","private":true},
            {"html_url":"https://github.com/team/api"},
            {"html_url":"not an address"},
            {"private":false}
        ]);
        let choices = account_choices(items.as_array().unwrap());
        assert_eq!(
            choices
                .iter()
                .map(|choice| (
                    choice.name.as_str(),
                    choice.private,
                    choice.folder.is_none()
                ))
                .collect::<Vec<_>>(),
            [("maya/notes", true, true), ("team/api", false, true)]
        );
    }

    #[cfg(feature = "host")]
    #[tokio::test]
    async fn the_folders_of_terminals_offer_their_origins_once_in_order() {
        let root = tempfile::tempdir().unwrap();
        let checkout = |name: &str, remote: Option<&str>| {
            let folder = root.path().join(name);
            std::fs::create_dir(&folder).unwrap();
            let git = |args: &[&str]| {
                assert!(
                    std::process::Command::new("git")
                        .arg("-C")
                        .arg(&folder)
                        .args(args)
                        .output()
                        .unwrap()
                        .status
                        .success()
                );
            };
            git(&["init", "-q"]);
            if let Some(remote) = remote {
                git(&["remote", "add", "origin", remote]);
            }
            folder.to_string_lossy().into_owned()
        };
        let api = checkout("api", Some("git@github.com:team/api.git"));
        let again = checkout("api-branch", Some("https://github.com/team/api"));
        let ui = checkout("ui", Some("https://forge.example/team/ui.git"));
        let local = checkout("local", None);
        let plain = root.path().join("plain").to_string_lossy().into_owned();
        let choices = folder_choices(vec![ui.clone(), local, api.clone(), again, plain]).await;
        assert_eq!(
            choices
                .iter()
                .map(|choice| (
                    choice.name.as_str(),
                    choice.provider.as_str(),
                    choice.folder.as_deref()
                ))
                .collect::<Vec<_>>(),
            [
                ("team/ui", "gitea", Some(ui.as_str())),
                ("team/api", "github", Some(api.as_str())),
            ]
        );
    }
}
