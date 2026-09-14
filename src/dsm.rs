use anyhow::Result;
use std::collections::HashSet;

use crate::github::{GitHubClient, OpenIssue, User};

const DSM_TITLE_PREFIX: &str = "[DSM] ";
const MAX_ASSIGNEES: usize = 10;

#[derive(Debug, PartialEq, Eq)]
struct Plan {
    create: bool,
    close: Vec<u64>,
}

pub async fn run(
    client: &GitHubClient,
    owner: &str,
    repo: &str,
    team_slug: &str,
    title: &str,
    template: &str,
) -> Result<()> {
    let issue_type = client.issue_type(owner, repo, team_slug).await?;
    let issues = client.open_issues(owner, repo, &issue_type).await?;
    let plan = plan_issues(issues, title);

    if plan.create {
        let members = client.team_members(owner, team_slug).await?;
        let assignable = client.assignable_users(owner, repo).await?;
        let assignees = select_assignees(&members, &assignable);
        let body = build_body(template, &members);

        client
            .create_issue(owner, repo, title, &body, &issue_type, &assignees)
            .await?;
    }

    for number in plan.close {
        client.close_issue(owner, repo, number).await?;
    }

    Ok(())
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

fn plan_issues(issues: Vec<OpenIssue>, current_title: &str) -> Plan {
    let mut create = true;
    let mut close = Vec::new();

    for issue in issues
        .into_iter()
        .filter(|issue| issue.title.starts_with(DSM_TITLE_PREFIX))
    {
        if issue.title == current_title && create {
            create = false;
        } else {
            close.push(issue.number);
        }
    }

    Plan { create, close }
}

#[cfg(test)]
mod tests {
    use super::{build_body, plan_issues, run, select_assignees, Plan, MAX_ASSIGNEES};
    use crate::github::{test_support::MockServer, GitHubClient, OpenIssue, User};

    fn user(index: usize) -> User {
        User {
            login: format!("member-{index}"),
            node_id: format!("node-{index}"),
        }
    }

    #[test]
    fn preserves_one_current_issue_and_closes_stale_or_duplicate_dsm_issues() {
        let issues = vec![
            OpenIssue {
                number: 1,
                title: "[DSM] Sunday".to_string(),
            },
            OpenIssue {
                number: 2,
                title: "[DSM] Monday".to_string(),
            },
            OpenIssue {
                number: 3,
                title: "[DSM] Monday".to_string(),
            },
            OpenIssue {
                number: 4,
                title: "Unrelated task".to_string(),
            },
        ];

        assert_eq!(
            plan_issues(issues, "[DSM] Monday"),
            Plan {
                create: false,
                close: vec![1, 3],
            }
        );
    }

    #[test]
    fn requests_creation_before_closing_stale_issues() {
        let issues = vec![OpenIssue {
            number: 1,
            title: "[DSM] Sunday".to_string(),
        }];

        assert_eq!(
            plan_issues(issues, "[DSM] Monday"),
            Plan {
                create: true,
                close: vec![1],
            }
        );
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
    async fn creates_and_verifies_the_current_dsm_before_closing_stale_issues() {
        let server = MockServer::start(vec![
            MockServer::response("200 OK", r#"[{"name":"DSM"}]"#, &[]),
            MockServer::response(
                "200 OK",
                r#"[{"number":1,"title":"[DSM] Sunday","type":{"name":"DSM"},"assignees":[]}]"#,
                &[],
            ),
            MockServer::response("200 OK", r#"[{"login":"member","node_id":"node-1"}]"#, &[]),
            MockServer::response("200 OK", r#"[{"login":"member","node_id":"node-1"}]"#, &[]),
            MockServer::response(
                "201 Created",
                r#"{"number":2,"title":"[DSM] Monday","type":{"name":"DSM"},"assignees":[{"login":"member","node_id":"node-1"}]}"#,
                &[],
            ),
            MockServer::response("200 OK", "{}", &[]),
        ]);
        let client = GitHubClient::with_api_root("test-secret", server.api_root.clone()).unwrap();

        run(&client, "org", "repo", "DSM", "[DSM] Monday", "Template")
            .await
            .unwrap();
        let requests = server.finish();

        assert_eq!(requests.len(), 6);
        assert!(requests[0].starts_with("GET /repos/org/repo/issue-types HTTP/1.1"));
        assert!(requests[1]
            .starts_with("GET /repos/org/repo/issues?state=open&type=DSM&per_page=100 HTTP/1.1"));
        assert!(requests[2]
            .starts_with("GET /orgs/org/teams/DSM/members?role=all&per_page=100 HTTP/1.1"));
        assert!(requests[3].starts_with("GET /repos/org/repo/assignees?per_page=100 HTTP/1.1"));
        assert!(requests[4].starts_with("POST /repos/org/repo/issues HTTP/1.1"));
        assert!(requests[4].contains("Template\\n<details>\\n@member\\n</details>"));
        assert!(requests[5].starts_with("PATCH /repos/org/repo/issues/1 HTTP/1.1"));
    }

    #[tokio::test]
    async fn leaves_stale_issues_open_when_github_drops_create_fields() {
        let server = MockServer::start(vec![
            MockServer::response("200 OK", r#"[{"name":"DSM"}]"#, &[]),
            MockServer::response(
                "200 OK",
                r#"[{"number":1,"title":"[DSM] Sunday","type":{"name":"DSM"},"assignees":[]}]"#,
                &[],
            ),
            MockServer::response("200 OK", r#"[{"login":"member","node_id":"node-1"}]"#, &[]),
            MockServer::response("200 OK", r#"[{"login":"member","node_id":"node-1"}]"#, &[]),
            MockServer::response(
                "201 Created",
                r#"{"number":2,"title":"[DSM] Monday","type":null,"assignees":[]}"#,
                &[],
            ),
        ]);
        let client = GitHubClient::with_api_root("test-secret", server.api_root.clone()).unwrap();

        let error = run(&client, "org", "repo", "DSM", "[DSM] Monday", "Template")
            .await
            .unwrap_err();
        let requests = server.finish();

        assert_eq!(
            error.to_string(),
            "GitHub did not apply the requested issue type"
        );
        assert_eq!(requests.len(), 5);
        assert!(requests
            .iter()
            .all(|request| !request.starts_with("PATCH ")));
    }
}
