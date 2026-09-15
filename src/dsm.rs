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
    use super::{build_body, select_assignees, MAX_ASSIGNEES};
    use crate::github::User;

    fn user(index: usize) -> User {
        User {
            login: format!("member-{index}"),
            node_id: format!("node-{index}"),
        }
    }

    #[test]
    fn selects_assignable_members_in_team_order_with_github_limit() {
        let members = (0..12).map(user).collect::<Vec<_>>();
        let assignable = (1..12).rev().map(user).collect::<Vec<_>>();

        let assignees = select_assignees(&members, &assignable);

        assert_eq!(assignees.len(), MAX_ASSIGNEES);
        assert_eq!(assignees.first().map(String::as_str), Some("member-1"));
        assert_eq!(assignees.last().map(String::as_str), Some("member-10"));
    }

    #[test]
    fn appends_all_team_members_to_the_consumer_template() {
        let members = vec![user(0), user(1), user(2)];

        assert_eq!(
            build_body("Template", &members),
            "Template\n<details>\n@member-0 @member-1 @member-2\n</details>"
        );
    }
}
