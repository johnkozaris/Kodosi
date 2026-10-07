use std::{process::Stdio, time::Duration};

use reqwest::{Method, Url};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt as _;
use zeroize::Zeroizing;

use super::{Issue, Repository, TaskChange};
use crate::network::{Result, invalid};

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

async fn token(repo: &Repository) -> Result<Option<Zeroizing<String>>> {
    if repo.provider == "github" {
        let output = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::process::Command::new("gh")
                .args(["auth", "token", "--hostname", &repo.host])
                .stdin(Stdio::null())
                .kill_on_drop(true)
                .output(),
        )
        .await;
        if let Ok(Ok(output)) = output
            && output.status.success()
        {
            return Ok(Some(Zeroizing::new(
                String::from_utf8(output.stdout)
                    .map_err(|_| invalid("GitHub credentials are unavailable."))?
                    .trim()
                    .to_owned(),
            )));
        }
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
    let bytes = crate::identity::oidc::read_bounded(response, 2 * 1024 * 1024).await?;
    if !status.is_success() {
        return Err(invalid(match status.as_u16() {
            403 => "Your provider account or token does not have access to these issues. Check its issue permissions.".to_owned(),
            401 => if repo.provider == "github" { "Connect your GitHub account with gh auth login, then try again.".to_owned() }
                else { "Connect your Gitea account through Git credentials or KODOSI_GITEA_URL and KODOSI_GITEA_TOKEN, then try again.".to_owned() },
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
    let mut result = Vec::new();
    for page in 1.. {
        let size = if repo.provider == "github" {
            "per_page"
        } else {
            "limit"
        };
        let value = api(
            repo,
            Method::GET,
            &format!("{path}?state=open&{size}=100&page={page}"),
            None,
        )
        .await?;
        let items = value
            .as_array()
            .ok_or_else(|| invalid("The provider did not return an issue list."))?;
        for item in items
            .iter()
            .filter(|value| value.get("pull_request").is_none_or(Value::is_null))
        {
            result.push(issue(repo, item)?);
        }
        if items.len() < 100 {
            break;
        }
    }
    Ok(result)
}

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
    fn provider_endpoints_and_issue_fields_preserve_repository_and_assignment() {
        let github = repository("https://github.com/team/api", None).unwrap();
        assert_eq!(
            api_url(&github, "repos/team/api/issues?per_page=100&page=2")
                .unwrap()
                .as_str(),
            "https://api.github.com/repos/team/api/issues?per_page=100&page=2"
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
