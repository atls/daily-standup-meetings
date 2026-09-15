use anyhow::{bail, Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, LINK, USER_AGENT},
    redirect::Policy,
    Client, Response, Url,
};
use serde::{de::DeserializeOwned, de::IgnoredAny, Deserialize, Serialize};

const API_ROOT: &str = "https://api.github.com/";
const API_VERSION: &str = "2026-03-10";
const USER_AGENT_VALUE: &str = "atls-daily-standup-meetings";
const MAX_PAGE_SIZE: &str = "100";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct User {
    pub login: String,
    pub node_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenIssue {
    pub number: u64,
    pub title: String,
}

#[derive(Debug, Deserialize)]
struct IssueType {
    name: String,
}

#[derive(Debug, Deserialize)]
struct ApiIssue {
    number: u64,
    title: String,
    #[serde(rename = "type")]
    issue_type: Option<IssueType>,
    #[serde(default)]
    assignees: Vec<User>,
    #[serde(default)]
    pull_request: Option<IgnoredAny>,
}

#[derive(Serialize)]
struct CreateIssue<'a> {
    title: &'a str,
    body: &'a str,
    assignees: &'a [String],
    #[serde(rename = "type")]
    issue_type: &'a str,
}

#[derive(Serialize)]
struct CloseIssue {
    state: &'static str,
}

pub struct GitHubClient {
    client: Client,
    api_root: Url,
}

impl GitHubClient {
    pub fn new(token: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static(API_VERSION),
        );

        let mut authorization = HeaderValue::from_str(&format!("Bearer {token}"))
            .context("GITHUB_TOKEN contains invalid header characters")?;
        authorization.set_sensitive(true);
        headers.insert(AUTHORIZATION, authorization);

        let client = Client::builder()
            .default_headers(headers)
            .redirect(Policy::none())
            .build()?;

        Ok(Self {
            client,
            api_root: Url::parse(API_ROOT)?,
        })
    }

    pub async fn team_members(&self, owner: &str, team_slug: &str) -> Result<Vec<User>> {
        let mut url = self.endpoint(&["orgs", owner, "teams", team_slug, "members"])?;
        url.query_pairs_mut()
            .append_pair("role", "all")
            .append_pair("per_page", MAX_PAGE_SIZE);

        self.get_all(url).await
    }

    pub async fn assignable_users(&self, owner: &str, repo: &str) -> Result<Vec<User>> {
        let mut url = self.endpoint(&["repos", owner, repo, "assignees"])?;
        url.query_pairs_mut().append_pair("per_page", MAX_PAGE_SIZE);

        self.get_all(url).await
    }

    pub async fn issue_type(&self, owner: &str, repo: &str, expected: &str) -> Result<String> {
        let url = self.endpoint(&["repos", owner, repo, "issue-types"])?;
        let issue_types: Vec<IssueType> = self.get_json(url).await?;

        issue_types
            .into_iter()
            .find(|issue_type| issue_type.name.trim().eq_ignore_ascii_case(expected.trim()))
            .map(|issue_type| issue_type.name)
            .with_context(|| format!("repository issue type `{expected}` was not found"))
    }

    pub async fn latest_open_issue(
        &self,
        owner: &str,
        repo: &str,
        issue_type: &str,
    ) -> Result<Option<OpenIssue>> {
        let mut url = self.endpoint(&["repos", owner, repo, "issues"])?;
        url.query_pairs_mut()
            .append_pair("state", "open")
            .append_pair("type", issue_type)
            .append_pair("sort", "created")
            .append_pair("direction", "desc")
            .append_pair("per_page", "1");

        let issues: Vec<ApiIssue> = self.get_json(url).await?;

        Ok(issues
            .into_iter()
            .filter(|issue| issue.pull_request.is_none())
            .filter(|issue| {
                issue.issue_type.as_ref().is_some_and(|actual| {
                    actual.name.trim().eq_ignore_ascii_case(issue_type.trim())
                })
            })
            .map(|issue| OpenIssue {
                number: issue.number,
                title: issue.title,
            })
            .next())
    }

    pub async fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
        issue_type: &str,
        assignees: &[String],
    ) -> Result<OpenIssue> {
        let url = self.endpoint(&["repos", owner, repo, "issues"])?;
        let response = self
            .send(self.client.post(url).json(&CreateIssue {
                title,
                body,
                assignees,
                issue_type,
            }))
            .await?;
        let created: ApiIssue = response
            .json()
            .await
            .context("GitHub create issue response was not valid JSON")?;

        if let Err(error) = self.verify_created_issue(&created, title, issue_type, assignees) {
            if let Err(cleanup_error) = self.close_issue(owner, repo, created.number).await {
                bail!(
                    "{error}; failed to close invalid issue #{}: {cleanup_error}",
                    created.number
                );
            }

            return Err(error);
        }

        Ok(OpenIssue {
            number: created.number,
            title: created.title,
        })
    }

    pub async fn close_issue(&self, owner: &str, repo: &str, number: u64) -> Result<()> {
        let url = self.endpoint(&["repos", owner, repo, "issues", &number.to_string()])?;
        self.send(self.client.patch(url).json(&CloseIssue { state: "closed" }))
            .await?;

        Ok(())
    }

    fn verify_created_issue(
        &self,
        created: &ApiIssue,
        expected_title: &str,
        expected_type: &str,
        expected_assignees: &[String],
    ) -> Result<()> {
        if created.title != expected_title {
            bail!("GitHub created an issue with an unexpected title");
        }

        if !created.issue_type.as_ref().is_some_and(|actual| {
            actual
                .name
                .trim()
                .eq_ignore_ascii_case(expected_type.trim())
        }) {
            bail!("GitHub did not apply the requested issue type");
        }

        let has_all_assignees = expected_assignees.iter().all(|expected| {
            created
                .assignees
                .iter()
                .any(|actual| actual.login.eq_ignore_ascii_case(expected))
        });

        if !has_all_assignees {
            bail!("GitHub did not apply all requested assignees");
        }

        Ok(())
    }

    async fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T> {
        self.send(self.client.get(url))
            .await?
            .json()
            .await
            .context("GitHub API response was not valid JSON")
    }

    async fn get_all<T: DeserializeOwned>(&self, mut url: Url) -> Result<Vec<T>> {
        let mut items = Vec::new();

        loop {
            let response = self.send(self.client.get(url)).await?;
            let next = self.next_page(&response)?;
            let mut page: Vec<T> = response
                .json()
                .await
                .context("GitHub API page was not valid JSON")?;
            items.append(&mut page);

            match next {
                Some(next) => url = next,
                None => return Ok(items),
            }
        }
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Response> {
        let request = request.build()?;
        let method = request.method().clone();
        let path = request.url().path().to_string();
        let response = self.client.execute(request).await?;

        if !response.status().is_success() {
            bail!(
                "GitHub API {method} {path} returned HTTP {}",
                response.status()
            );
        }

        Ok(response)
    }

    fn next_page(&self, response: &Response) -> Result<Option<Url>> {
        let Some(link) = response.headers().get(LINK) else {
            return Ok(None);
        };
        let link = link
            .to_str()
            .context("GitHub pagination Link header was not valid text")?;

        for entry in link.split(',') {
            let mut parts = entry.trim().split(';');
            let Some(target) = parts.next() else {
                continue;
            };
            let is_next = parts.any(|part| part.trim() == "rel=\"next\"");

            if is_next {
                let target = target
                    .trim()
                    .strip_prefix('<')
                    .and_then(|value| value.strip_suffix('>'))
                    .context("GitHub pagination next link was malformed")?;
                let next = Url::parse(target).context("GitHub pagination next link was invalid")?;

                if next.origin() != self.api_root.origin() {
                    bail!("GitHub pagination next link changed API origin");
                }

                return Ok(Some(next));
            }
        }

        Ok(None)
    }

    fn endpoint(&self, segments: &[&str]) -> Result<Url> {
        let mut url = self.api_root.clone();
        let mut path = url
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("GitHub API root cannot contain path segments"))?;
        path.pop_if_empty();
        path.extend(segments);
        drop(path);

        Ok(url)
    }
}
