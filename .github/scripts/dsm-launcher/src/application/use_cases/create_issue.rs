use anyhow::{Ok, Result};
use std::collections::HashSet;

use crate::application::{
    commands::create_issue::CreateIssueCommand,
    queries::{
        get_assignable_members::GetAssignableMembersQuery, get_issue_types::GetIssueTypes,
        get_org::GetOrgQuery, get_repo::GetRepoQuery, get_team::GetTeamQuery,
        get_team_members::GetTeamMembersQuery,
    },
};
use crate::domain::{
    errors::issue_types::IssueTypesError,
    issue::Issue,
    member::Member,
    repository::{IssueRepository, MemberRepository, OrgRepository},
};

pub async fn create_issue<R: OrgRepository, M: MemberRepository, I: IssueRepository>(
    get_org: GetOrgQuery<R>,
    get_repo: GetRepoQuery<R>,
    get_team: GetTeamQuery<M>,
    get_team_members: GetTeamMembersQuery<M>,
    get_assignable_members: GetAssignableMembersQuery<M>,
    get_issue_types: GetIssueTypes<I>,
    create_issue: CreateIssueCommand<I>,
    owner: &str,
    repo_name: &str,
    team_slug: &str,
    issue_type: &str,
    title: &str,
    body: &str,
) -> Result<()> {
    let org_id = get_org.execute(owner).await?;
    let repo_id = get_repo.execute(&org_id, &repo_name).await?;

    let team_id = get_team.execute(&org_id, &team_slug).await?;
    let members = get_team_members.execute(&team_id).await?;
    let assignable_member_ids = get_assignable_members
        .execute(&repo_id)
        .await?
        .into_iter()
        .map(|member_id| member_id.to_string())
        .collect::<HashSet<String>>();

    let issue_type_id = get_issue_types
        .execute(&repo_id)
        .await?
        .into_iter()
        .filter_map(|x| {
            if x.name.trim().eq_ignore_ascii_case(issue_type.trim()) {
                Some(x.id)
            } else {
                None
            }
        })
        .collect::<Vec<String>>()
        .first()
        .ok_or(IssueTypesError::IssueTypeNotFound)?
        .clone();

    let extended_body = format!(
        "{}\n<details>\n{}\n</details>",
        body,
        members
            .iter()
            .map(|x| format!("@{} ", x.login))
            .collect::<String>()
    );
    let assignees = filter_assignable_members(members, &assignable_member_ids);

    let new_issue = Issue {
        id: None,
        repo_id: repo_id.to_string(),
        title: title.to_string(),
        body: extended_body,
        issue_type_id: issue_type_id,
        assignees,
    };

    create_issue.execute(new_issue).await?;

    Ok(())
}

fn filter_assignable_members(
    members: Vec<Member>,
    assignable_member_ids: &HashSet<String>,
) -> Vec<Member> {
    members
        .into_iter()
        .filter(|member| assignable_member_ids.contains(&member.id.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::filter_assignable_members;
    use crate::domain::member::{Member, MemberId};
    use std::collections::HashSet;

    #[test]
    fn keeps_only_repository_assignable_team_members() {
        let members = vec![
            Member::new(
                MemberId::new("not-assignable".to_string()),
                "first".to_string(),
            ),
            Member::new(
                MemberId::new("assignable".to_string()),
                "second".to_string(),
            ),
        ];
        let assignable_member_ids = HashSet::from(["assignable".to_string()]);

        let filtered = filter_assignable_members(members, &assignable_member_ids);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].login, "second");
    }
}
