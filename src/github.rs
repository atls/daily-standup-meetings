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

#[cfg(test)]
pub(crate) mod test_support {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
        thread::{self, JoinHandle},
        time::Duration,
    };

    use reqwest::Url;

    pub struct MockServer {
        pub api_root: Url,
        pub requests: Arc<Mutex<Vec<String>>>,
        handle: Option<JoinHandle<()>>,
    }

    impl MockServer {
        pub fn start(responses: Vec<String>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let api_root = Url::parse(&format!("http://{address}/")).unwrap();
            let api_root_text = api_root.to_string();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let recorded = requests.clone();

            let handle = thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let request = read_request(&mut stream);
                    recorded.lock().unwrap().push(request);
                    stream
                        .write_all(response.replace("{ROOT}", &api_root_text).as_bytes())
                        .unwrap();
                    stream.flush().unwrap();
                }
            });

            Self {
                api_root,
                requests,
                handle: Some(handle),
            }
        }

        pub fn response(status: &str, body: &str, headers: &[(&str, &str)]) -> String {
            let extra_headers = headers
                .iter()
                .map(|(name, value)| format!("{name}: {value}\r\n"))
                .collect::<String>();

            format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n{body}",
                body.len()
            )
        }

        pub fn finish(mut self) -> Vec<String> {
            self.handle.take().unwrap().join().unwrap();
            Arc::try_unwrap(self.requests)
                .unwrap()
                .into_inner()
                .unwrap()
        }
    }

    fn read_request(stream: &mut impl Read) -> String {
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1024];

        loop {
            let read = stream.read(&mut chunk).unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..read]);

            if let Some(header_end) = find_bytes(&request, b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);

                if request.len() >= header_end + 4 + content_length {
                    break;
                }
            }
        }

        String::from_utf8(request).unwrap()
    }

    fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }
}

#[cfg(test)]
mod tests {
    use super::{test_support::MockServer, GitHubClient};

    #[tokio::test]
    async fn follows_same_origin_pagination_and_sends_required_headers() {
        let first_body = r#"[{"login":"first","node_id":"node-1"}]"#;
        let second_body = r#"[{"login":"second","node_id":"node-2"}]"#;
        let server = MockServer::start(vec![
            MockServer::response(
                "200 OK",
                first_body,
                &[(
                    "Link",
                    "<{ROOT}orgs/my%20org/teams/Team%2FOne/members?page=2>; rel=\"next\"",
                )],
            ),
            MockServer::response("200 OK", second_body, &[]),
        ]);
        let client = GitHubClient::with_api_root("test-secret", server.api_root.clone()).unwrap();

        let members = client.team_members("my org", "Team/One").await.unwrap();
        let requests = server.finish();

        assert_eq!(members.len(), 2);
        assert!(requests[0].starts_with(
            "GET /orgs/my%20org/teams/Team%2FOne/members?role=all&per_page=100 HTTP/1.1"
        ));
        assert!(requests[0]
            .to_ascii_lowercase()
            .contains("authorization: bearer test-secret"));
        assert!(requests[0]
            .to_ascii_lowercase()
            .contains("x-github-api-version: 2026-03-10"));
        assert!(requests[0]
            .to_ascii_lowercase()
            .contains("accept: application/vnd.github+json"));
        assert!(
            requests[1].starts_with("GET /orgs/my%20org/teams/Team%2FOne/members?page=2 HTTP/1.1")
        );
    }

    #[tokio::test]
    async fn rejects_cross_origin_pagination_before_sending_the_token() {
        let body = r#"[{"login":"first","node_id":"node-1"}]"#;
        let server = MockServer::start(vec![MockServer::response(
            "200 OK",
            body,
            &[("Link", "<https://example.com/page/2>; rel=\"next\"")],
        )]);
        let client = GitHubClient::with_api_root("test-secret", server.api_root.clone()).unwrap();

        let error = client.team_members("org", "team").await.unwrap_err();
        let requests = server.finish();

        assert_eq!(requests.len(), 1);
        assert_eq!(
            error.to_string(),
            "GitHub pagination next link changed API origin"
        );
    }

    #[tokio::test]
    async fn rejects_a_create_response_that_dropped_type_or_assignees() {
        let body = r#"{"number":2,"title":"[DSM] Monday","type":null,"assignees":[]}"#;
        let server = MockServer::start(vec![MockServer::response("201 Created", body, &[])]);
        let client = GitHubClient::with_api_root("test-secret", server.api_root.clone()).unwrap();

        let error = client
            .create_issue(
                "org",
                "repo",
                "[DSM] Monday",
                "Template",
                "DSM",
                &["member".to_string()],
            )
            .await
            .unwrap_err();
        let requests = server.finish();

        assert_eq!(
            error.to_string(),
            "GitHub did not apply the requested issue type"
        );
        assert!(requests[0].starts_with("POST /repos/org/repo/issues HTTP/1.1"));
        assert!(requests[0].contains("\"type\":\"DSM\""));
        assert!(requests[0].contains("\"assignees\":[\"member\"]"));
    }

    #[tokio::test]
    async fn filters_pull_requests_and_unexpected_issue_types() {
        let body = r#"[
            {"number":1,"title":"[DSM] Monday","type":{"name":"DSM"},"assignees":[]},
            {"number":2,"title":"[DSM] PR","type":{"name":"DSM"},"assignees":[],"pull_request":{}},
            {"number":3,"title":"[DSM] Bug","type":{"name":"Bug"},"assignees":[]}
        ]"#;
        let server = MockServer::start(vec![MockServer::response("200 OK", body, &[])]);
        let client = GitHubClient::with_api_root("test-secret", server.api_root.clone()).unwrap();

        let issues = client.open_issues("org", "repo", "DSM").await.unwrap();
        server.finish();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 1);
    }
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
        Self::with_api_root(token, Url::parse(API_ROOT)?)
    }

    pub(crate) fn with_api_root(token: &str, api_root: Url) -> Result<Self> {
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

        Ok(Self { client, api_root })
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

    pub async fn open_issues(
        &self,
        owner: &str,
        repo: &str,
        issue_type: &str,
    ) -> Result<Vec<OpenIssue>> {
        let mut url = self.endpoint(&["repos", owner, repo, "issues"])?;
        url.query_pairs_mut()
            .append_pair("state", "open")
            .append_pair("type", issue_type)
            .append_pair("per_page", MAX_PAGE_SIZE);

        let issues: Vec<ApiIssue> = self.get_all(url).await?;

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
            .collect())
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

        self.verify_created_issue(&created, title, issue_type, assignees)?;

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
