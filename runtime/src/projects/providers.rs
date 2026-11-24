#[cfg(feature = "host")]
use std::{process::Stdio, time::Duration};

use reqwest::{Client, Url};
#[cfg(feature = "host")]
use reqwest::{Method, header::USER_AGENT};
#[cfg(feature = "host")]
use serde_json::{Value, json};
#[cfg(feature = "host")]
use tokio::io::AsyncWriteExt as _;
#[cfg(feature = "host")]
use zeroize::Zeroizing;

use super::{Issue, Repository, TaskChange};
#[cfg(feature = "host")]
use crate::network::invalid;
use crate::network::{Error, Result};
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
        let ssh = Url::parse(remote)
            .map_err(|_| Error::Text(text!("This repository address is not valid.")))?;
        format!(
            "https://{}{}",
            ssh.host_str()
                .ok_or_else(|| Error::Text(text!("The repository address has no server name.")))?,
            ssh.path()
        )
    } else {
        remote.to_owned()
    };
    let mut url = Url::parse(&normalized)
        .map_err(|_| Error::Text(text!("Use a repository URL or Git remote.")))?;
    crate::identity::oidc::validate_url(&url)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::Text(text!(
            "Use a repository address without credentials."
        )));
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
        return Err(Error::Text(text!(
            "Use the repository address, including its owner and name."
        )));
    }
    let (owner, name) = (parts[0].to_owned(), parts[1].to_owned());
    let host = url
        .host_str()
        .ok_or_else(|| Error::Text(text!("The repository address has no server name.")))?
        .to_owned();
    let kind = provider.unwrap_or(if host == "github.com" {
        "github"
    } else {
        "gitea"
    });
    if !matches!(kind, "github" | "gitea") {
        return Err(Error::Text(text!("Choose GitHub or Gitea.")));
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

/// A repository from project content links only the address that it names, so its requests and
/// credentials go to one host. A link of another person needs HTTPS.
pub(crate) fn linkable(repo: &Repository, own: bool) -> bool {
    repository(&repo.url, Some(&repo.provider)).is_ok_and(|checked| {
        checked.url == repo.url
            && checked.host == repo.host
            && checked.owner == repo.owner
            && checked.repository == repo.repository
            && checked.name == repo.name
            && (own || repo.url.starts_with("https://"))
    })
}

#[cfg(feature = "host")]
async fn gh() -> Option<std::path::PathBuf> {
    crate::agents::executable(&kodosi_os::process::default_shell(), "gh").await
}

/// The variables with which gh, also as a credential helper, gives one token to any host.
#[cfg(feature = "host")]
const HOSTLESS_TOKENS: [&str; 5] = [
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
    "CODESPACES",
];

/// A host other than github.com gets only a credential that was saved for it.
#[cfg(feature = "host")]
fn for_host(mut command: tokio::process::Command, host: &str) -> tokio::process::Command {
    if !host.eq_ignore_ascii_case("github.com") {
        for name in HOSTLESS_TOKENS {
            command.env_remove(name);
        }
    }
    command
}

#[cfg(feature = "host")]
async fn gh_token(gh: &std::path::Path, host: &str) -> Result<Option<Zeroizing<String>>> {
    let mut command = for_host(tokio::process::Command::new(gh), host);
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        command
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
            .map_err(|_| Error::Text(text!("Kodosi cannot get your GitHub credentials.")))?
            .trim()
            .to_owned(),
    );
    Ok((!token.is_empty()).then_some(token))
}

#[cfg(feature = "host")]
async fn token(repo: &Repository) -> Result<Option<Zeroizing<String>>> {
    let url = Url::parse(&repo.url)
        .map_err(|_| Error::Text(text!("This repository address is not valid.")))?;
    let host = url
        .host_str()
        .ok_or_else(|| Error::Text(text!("The repository address has no server name.")))?;
    if repo.provider == "github"
        && let Some(gh) = gh().await
        && let Some(token) = gh_token(&gh, host).await?
    {
        return Ok(Some(token));
    }
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
    let mut command = for_host(tokio::process::Command::new("git"), host);
    let host = url
        .port()
        .map_or_else(|| host.to_owned(), |port| format!("{host}:{port}"));
    let Ok(mut child) = command
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
        let text = Zeroizing::new(String::from_utf8(output.stdout).map_err(|_| {
            Error::Text(text!(
                "Kodosi cannot get the credentials of this repository."
            ))
        })?);
        return Ok(text.lines().find_map(|line| {
            line.strip_prefix("password=")
                .map(|value| Zeroizing::new(value.to_owned()))
        }));
    }
    Ok(None)
}

#[cfg(feature = "host")]
fn api_url(repo: &Repository, path: &str) -> Result<Url> {
    let mut url = Url::parse(&repo.url)
        .map_err(|_| Error::Text(text!("This repository address is not valid.")))?;
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
async fn api(
    client: &Client,
    repo: &Repository,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value> {
    let mut request = client
        .request(method, api_url(repo, path)?)
        .header(USER_AGENT, "Kodosi")
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
        return Err(Error::Text(match status.as_u16() {
            403 | 429 if limited => text!(
                "The issue provider limits your requests now. Try again later, or connect your account for a higher limit."
            ),
            403 => text!(
                "Your provider account or token does not have access to these issues. Check its issue permissions."
            ),
            401 => connect_message(repo),
            404 => text!("The repository or issue is unavailable to your account."),
            409 | 422 => text!(
                "The issue changed, or the provider could not apply the update. Refresh and try again."
            ),
            429 => text!("The issue provider is busy. Try again in a moment."),
            _ => text!(
                "The issue provider returned HTTP {status}.",
                status = status.as_u16()
            ),
        }));
    }
    if bytes.is_empty() {
        Ok(Value::Null)
    } else {
        Ok(serde_json::from_slice(&bytes)?)
    }
}

#[cfg(feature = "host")]
fn connect_message(repo: &Repository) -> crate::text::Text {
    if repo.provider == "github" {
        text!("Connect your GitHub account with gh auth login, then try again.")
    } else {
        text!(
            "Connect your Gitea account through Git credentials or KODOSI_GITEA_URL and KODOSI_GITEA_TOKEN, then try again."
        )
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
pub(crate) async fn issues(
    client: &Client,
    repo: &Repository,
    number: Option<u64>,
) -> Result<Vec<Issue>> {
    let path = format!(
        "repos/{}/{}/issues{}",
        repo.owner,
        repo.repository,
        number.map_or_else(String::new, |n| format!("/{n}"))
    );
    if number.is_some() {
        let value = api(client, repo, Method::GET, &path, None).await?;
        return Ok(vec![issue(repo, &value)?]);
    }
    let size = if repo.provider == "github" {
        "per_page"
    } else {
        "limit"
    };
    let value = api(
        client,
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
    client: &Client,
    repo: &Repository,
    number: u64,
    change: TaskChange,
) -> Result<Issue> {
    let path = format!("repos/{}/{}/issues/{number}", repo.owner, repo.repository);
    let change = match change {
        TaskChange::Close => json!({"state":"closed"}),
        TaskChange::Reopen => json!({"state":"open"}),
        TaskChange::Claim => {
            let user = api(client, repo, Method::GET, "user", None).await?;
            let login = user["login"]
                .as_str()
                .ok_or_else(|| Error::Text(text!("Your provider account is unavailable.")))?;
            json!({"assignees":[login]})
        }
        TaskChange::Release => json!({"assignees":[]}),
    };
    issue(
        repo,
        &api(client, repo, Method::PATCH, &path, Some(change)).await?,
    )
}

#[cfg(feature = "host")]
pub(crate) async fn create_issue(
    client: &Client,
    repo: &Repository,
    title: &str,
    body: &str,
) -> Result<Issue> {
    if token(repo).await?.is_none() {
        return Err(Error::Text(connect_message(repo)));
    }
    let path = format!("repos/{}/{}/issues", repo.owner, repo.repository);
    let created = api(
        client,
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
pub(crate) async fn choices(client: &Client, folders: Vec<String>) -> Result<RepositoryList> {
    let mut repositories = folder_choices(folders).await;
    let (github, account) = account_repositories(client).await;
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
async fn account_repositories(client: &Client) -> (GitHubState, Vec<RepositoryChoice>) {
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
        client,
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
pub(crate) async fn create_issue(
    _client: &Client,
    _repo: &Repository,
    _title: &str,
    _body: &str,
) -> Result<Issue> {
    Err(Error::Text(text!("Create issues from a computer.")))
}

#[cfg(not(feature = "host"))]
#[expect(
    clippy::unused_async,
    reason = "issues are read through the provider credentials of a computer"
)]
pub(crate) async fn issues(
    _client: &Client,
    _repo: &Repository,
    _number: Option<u64>,
) -> Result<Vec<Issue>> {
    Err(Error::Text(text!("Import issues from a computer.")))
}

#[cfg(not(feature = "host"))]
#[expect(
    clippy::unused_async,
    reason = "linked issues change through the provider credentials of a computer"
)]
pub(crate) async fn update_issue(
    _client: &Client,
    _repo: &Repository,
    _number: u64,
    _change: TaskChange,
) -> Result<Issue> {
    Err(Error::Text(text!("Change linked issues from a computer.")))
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

    #[test]
    fn a_project_repository_links_only_the_host_of_its_own_address() {
        let linked = repository("https://github.com/team/project", None).unwrap();
        assert!(linkable(&linked, false));
        let elsewhere = Repository {
            url: "https://collector.example/team/project".into(),
            provider: "gitea".into(),
            ..linked
        };
        assert!(!linkable(&elsewhere, false));
        assert!(!linkable(&elsewhere, true));
        let local = repository("http://127.0.0.1:3000/team/project", None).unwrap();
        assert!(linkable(&local, true));
        assert!(!linkable(&local, false));
    }

    #[cfg(feature = "host")]
    #[test]
    fn only_github_com_gets_a_token_from_the_environment() {
        let removed = |host: &str| {
            for_host(tokio::process::Command::new("gh"), host)
                .as_std()
                .get_envs()
                .filter(|(_, value)| value.is_none())
                .map(|(name, _)| name.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(removed("github.com"), Vec::<String>::new());
        let mut collector = removed("collector.example");
        collector.sort();
        let mut expected = HOSTLESS_TOKENS.map(str::to_owned).to_vec();
        expected.sort();
        assert_eq!(collector, expected);
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
}
