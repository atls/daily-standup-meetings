use anyhow::Result;
use std::collections::HashSet;

use crate::github::{GitHubClient, User};

const MAX_ASSIGNEES: usize = 10;

pub async fn run(
    client: &GitHubClient,
    owner: &str,
    repo: &str,
    team_slugs: &[String],
    issue_type: &str,
    title: &str,
    template: &str,
) -> Result<()> {
    let issue_type = client.issue_type(owner, repo, issue_type).await?;
    let current_issue = client.latest_open_issue(owner, repo, &issue_type).await?;

    if current_issue
        .as_ref()
        .is_some_and(|issue| issue.title == title)
    {
        return Ok(());
    }

    let members = collect_team_members(client, owner, team_slugs).await?;
    let assignable = client.assignable_users(owner, repo).await?;
    let assignees = select_assignees(&members, &assignable);
    let body = build_body(template, &members);

    if let Some(issue) = current_issue {
        client.close_issue(owner, repo, issue.number).await?;
    }

    client
        .create_issue(owner, repo, title, &body, &issue_type, &assignees)
        .await?;

    Ok(())
}

async fn collect_team_members(
    client: &GitHubClient,
    owner: &str,
    team_slugs: &[String],
) -> Result<Vec<User>> {
    let mut member_ids = HashSet::new();
    let mut members = Vec::new();

    for team_slug in team_slugs {
        for member in client.team_members(owner, team_slug).await? {
            if member_ids.insert(member.node_id.clone()) {
                members.push(member);
            }
        }
    }

    Ok(members)
}

fn build_body(template: &str, members: &[User]) -> String {
    let mentions = members
        .iter()
        .map(|member| format!("@{}", member.login))
        .collect::<Vec<_>>()
        .join(" ");

    format!("{template}\n<details>\n{mentions}\n</details>")
}

fn select_assignees(members: &[User], assignable: &[User]) -> Vec<String> {
    let assignable_ids = assignable
        .iter()
        .map(|member| member.node_id.as_str())
        .collect::<HashSet<_>>();

    members
        .iter()
        .filter(|member| assignable_ids.contains(member.node_id.as_str()))
        .take(MAX_ASSIGNEES)
        .map(|member| member.login.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{build_body, collect_team_members, run, select_assignees, MAX_ASSIGNEES};
    use crate::github::{test_support::MockServer, GitHubClient, User};

    fn user(index: usize) -> User {
        User {
            login: format!("member-{index}"),
            node_id: format!("node-{index}"),
        }
    }

    fn team_slugs(slugs: &[&str]) -> Vec<String> {
        slugs.iter().map(|slug| (*slug).to_string()).collect()
    }

    #[test]
    fn assigns_only_repository_assignable_members_in_team_order() {
        let members = (0..12).map(user).collect::<Vec<_>>();
        let assignable = (1..12).rev().map(user).collect::<Vec<_>>();

        let assignees = select_assignees(&members, &assignable);

        assert_eq!(assignees.len(), MAX_ASSIGNEES);
        assert_eq!(assignees.first().map(String::as_str), Some("member-1"));
        assert_eq!(assignees.last().map(String::as_str), Some("member-10"));
    }

    #[test]
    fn mentions_every_team_member_even_when_only_some_are_assignable() {
        let members = vec![user(0), user(1), user(2)];

        assert_eq!(
            build_body("Template", &members),
            "Template\n<details>\n@member-0 @member-1 @member-2\n</details>"
        );
    }

    #[tokio::test]
    async fn merges_members_from_multiple_teams_in_input_order() {
        let server = MockServer::start(vec![
            MockServer::response(
                "200 OK",
                r#"[{"login":"first","node_id":"node-1"},{"login":"shared","node_id":"node-2"}]"#,
                &[],
            ),
            MockServer::response(
                "200 OK",
                r#"[{"login":"shared","node_id":"node-2"},{"login":"last","node_id":"node-3"}]"#,
                &[],
            ),
        ]);
        let client = GitHubClient::with_api_root("test-secret", server.api_root.clone()).unwrap();

        let members =
            collect_team_members(&client, "org", &team_slugs(&["engineering", "operations"]))
                .await
                .unwrap();
        let requests = server.finish();

        assert_eq!(
            members
                .iter()
                .map(|member| member.login.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "shared", "last"]
        );
        assert!(requests[0]
            .starts_with("GET /orgs/org/teams/engineering/members?role=all&per_page=100 HTTP/1.1"));
        assert!(requests[1]
            .starts_with("GET /orgs/org/teams/operations/members?role=all&per_page=100 HTTP/1.1"));
    }

    #[tokio::test]
    async fn closes_the_latest_stale_dsm_before_creating_the_current_one() {
        let server = MockServer::start(vec![
            MockServer::response("200 OK", r#"[{"name":"DSM"}]"#, &[]),
            MockServer::response(
                "200 OK",
                r#"[{"number":1,"title":"[DSM] Sunday","type":{"name":"DSM"},"assignees":[]}]"#,
                &[],
            ),
            MockServer::response("200 OK", r#"[{"login":"member","node_id":"node-1"}]"#, &[]),
            MockServer::response("200 OK", r#"[{"login":"member","node_id":"node-1"}]"#, &[]),
            MockServer::response("200 OK", "{}", &[]),
            MockServer::response(
                "201 Created",
                r#"{"number":2,"title":"[DSM] Monday","type":{"name":"DSM"},"assignees":[{"login":"member","node_id":"node-1"}]}"#,
                &[],
            ),
        ]);
        let client = GitHubClient::with_api_root("test-secret", server.api_root.clone()).unwrap();

        run(
            &client,
            "org",
            "repo",
            &team_slugs(&["engineering"]),
            "DSM",
            "[DSM] Monday",
            "Template",
        )
        .await
        .unwrap();
        let requests = server.finish();

        assert_eq!(requests.len(), 6);
        assert!(requests[0].starts_with("GET /repos/org/repo/issue-types HTTP/1.1"));
        assert!(requests[1].starts_with(
            "GET /repos/org/repo/issues?state=open&type=DSM&sort=created&direction=desc&per_page=1 HTTP/1.1"
        ));
        assert!(requests[2]
            .starts_with("GET /orgs/org/teams/engineering/members?role=all&per_page=100 HTTP/1.1"));
        assert!(requests[3].starts_with("GET /repos/org/repo/assignees?per_page=100 HTTP/1.1"));
        assert!(requests[4].starts_with("PATCH /repos/org/repo/issues/1 HTTP/1.1"));
        assert!(requests[5].starts_with("POST /repos/org/repo/issues HTTP/1.1"));
        assert!(requests[5].contains("Template\\n<details>\\n@member\\n</details>"));
    }

    #[tokio::test]
    async fn creates_the_current_dsm_when_the_repository_has_no_open_issue() {
        let server = MockServer::start(vec![
            MockServer::response("200 OK", r#"[{"name":"DSM"}]"#, &[]),
            MockServer::response("200 OK", "[]", &[]),
            MockServer::response("200 OK", r#"[{"login":"member","node_id":"node-1"}]"#, &[]),
            MockServer::response("200 OK", r#"[{"login":"member","node_id":"node-1"}]"#, &[]),
            MockServer::response(
                "201 Created",
                r#"{"number":1,"title":"[DSM] Monday","type":{"name":"DSM"},"assignees":[{"login":"member","node_id":"node-1"}]}"#,
                &[],
            ),
        ]);
        let client = GitHubClient::with_api_root("test-secret", server.api_root.clone()).unwrap();

        run(
            &client,
            "org",
            "repo",
            &team_slugs(&["engineering"]),
            "DSM",
            "[DSM] Monday",
            "Template",
        )
        .await
        .unwrap();
        let requests = server.finish();

        assert_eq!(requests.len(), 5);
        assert!(requests[4].starts_with("POST /repos/org/repo/issues HTTP/1.1"));
        assert!(!requests
            .iter()
            .any(|request| request.starts_with("PATCH /repos/org/repo/issues/")));
    }

    #[tokio::test]
    async fn keeps_the_current_dsm_even_when_its_assignees_changed() {
        let server = MockServer::start(vec![
            MockServer::response("200 OK", r#"[{"name":"DSM"}]"#, &[]),
            MockServer::response(
                "200 OK",
                r#"[{"number":1,"title":"[DSM] Monday","type":{"name":"DSM"},"assignees":[]}]"#,
                &[],
            ),
        ]);
        let client = GitHubClient::with_api_root("test-secret", server.api_root.clone()).unwrap();

        run(
            &client,
            "org",
            "repo",
            &team_slugs(&["engineering"]),
            "DSM",
            "[DSM] Monday",
            "Template",
        )
        .await
        .unwrap();
        let requests = server.finish();

        assert_eq!(requests.len(), 2);
        assert!(requests[1].starts_with(
            "GET /repos/org/repo/issues?state=open&type=DSM&sort=created&direction=desc&per_page=1 HTTP/1.1"
        ));
    }
}
