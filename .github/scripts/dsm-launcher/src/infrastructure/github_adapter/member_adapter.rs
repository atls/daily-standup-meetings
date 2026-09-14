use anyhow::{Ok, Result};
use async_trait::async_trait;

use crate::{
    domain::{
        member::{Member, MemberId},
        org::OrgId,
        repo::RepoId,
        repository::MemberRepository,
        team::TeamId,
    },
    graphql_queries::{
        get_assignable_users::{
            get_assignable_users::{GetAssignableUsersNode, Variables as GetAssignableUsersVars},
            GetAssignableUsers,
        },
        get_team::{
            get_team::{GetTeamNode, Variables as GetTeamVars},
            GetTeam,
        },
        get_team_members::{
            get_team_members::{GetTeamMembersNode, Variables as GetTeamMembersVars},
            GetTeamMembers,
        },
    },
};

use crate::domain::errors::{member::MemberError, org::OrgError, team::TeamError};

use super::{errors::GitHubAdapterError, GitHubAdapter};

#[async_trait]
impl MemberRepository for GitHubAdapter {
    async fn get_assignable_members(&self, repo_id: &RepoId) -> Result<Vec<MemberId>> {
        let mut cursor = None;
        let mut member_ids = Vec::new();

        loop {
            let variables = GetAssignableUsersVars {
                id: repo_id.to_string(),
                cursor: cursor.clone(),
            };

            let response = self.client.execute::<GetAssignableUsers>(variables).await?;

            if let Some(errors) = response.errors {
                return Err(GitHubAdapterError::GraphQL(errors).into());
            }

            let response_data = response
                .data
                .ok_or(MemberError::EmptyAssignableUsersResponse)?;
            let node = response_data
                .node
                .ok_or(crate::domain::errors::repo::RepoError::RepoNodeNotFound)?;
            let repo = match node {
                GetAssignableUsersNode::Repository(repo) => repo,
                _ => return Err(GitHubAdapterError::UnexpectedNodeType.into()),
            };

            member_ids.extend(
                repo.assignable_users
                    .nodes
                    .ok_or(MemberError::AssignableUsersWereNotFound)?
                    .into_iter()
                    .flatten()
                    .map(|member| MemberId::new(member.id)),
            );

            cursor = next_assignable_users_cursor(
                repo.assignable_users.page_info.has_next_page,
                repo.assignable_users.page_info.end_cursor,
            )?;

            if cursor.is_none() {
                break;
            }
        }

        Ok(member_ids)
    }

    async fn get_team_members(&self, team_id: &TeamId) -> Result<Vec<Member>> {
        let mut cursor = None;
        let mut members = Vec::new();

        loop {
            let variables = GetTeamMembersVars {
                id: team_id.to_string(),
                cursor: cursor.clone(),
            };

            let response = self.client.execute::<GetTeamMembers>(variables).await?;

            if let Some(errors) = response.errors {
                return Err(GitHubAdapterError::GraphQL(errors).into());
            }

            let response_data = response.data.ok_or(MemberError::EmptyTeamMembersResponse)?;
            let node = response_data.node.ok_or(TeamError::TeamNodeNotFound)?;
            let team = match node {
                GetTeamMembersNode::Team(team) => team,
                _ => return Err(GitHubAdapterError::UnexpectedNodeType.into()),
            };

            members.extend(
                team.members
                    .nodes
                    .ok_or(MemberError::TeamMembersWereNotFound)?
                    .into_iter()
                    .flatten()
                    .map(|member| Member::new(MemberId::new(member.id), member.login)),
            );

            cursor = next_cursor(
                team.members.page_info.has_next_page,
                team.members.page_info.end_cursor,
            )?;

            if cursor.is_none() {
                break;
            }
        }

        Ok(members)
    }

    async fn get_team(&self, org_id: &OrgId, team_slug: &str) -> Result<TeamId> {
        let variables = GetTeamVars {
            id: org_id.to_string(),
            team_slug: team_slug.to_string(),
        };

        let response = self.client.execute::<GetTeam>(variables).await?;

        if let Some(errors) = response.errors {
            return Err(GitHubAdapterError::GraphQL(errors).into());
        }

        let response_data = response.data.ok_or(TeamError::EmptyTeamResponse)?;
        let node = response_data
            .node
            .ok_or(TeamError::TeamResponseNodeNotFound)?;
        let org = match node {
            GetTeamNode::Organization(org) => org,
            _ => {
                return Err(OrgError::OrgNotFound.into());
            }
        };
        let team = org.team.ok_or(TeamError::TeamNotFound)?;

        Ok(TeamId::new(team.id))
    }
}

fn next_cursor(has_next_page: bool, end_cursor: Option<String>) -> Result<Option<String>> {
    if has_next_page {
        return end_cursor
            .map(Some)
            .ok_or_else(|| MemberError::TeamMembersCursorNotFound.into());
    }

    Ok(None)
}

fn next_assignable_users_cursor(
    has_next_page: bool,
    end_cursor: Option<String>,
) -> Result<Option<String>> {
    if has_next_page {
        return end_cursor
            .map(Some)
            .ok_or_else(|| MemberError::AssignableUsersCursorNotFound.into());
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::{next_assignable_users_cursor, next_cursor};

    #[test]
    fn advances_when_another_page_exists() {
        assert_eq!(
            next_cursor(true, Some("cursor".to_string())).unwrap(),
            Some("cursor".to_string())
        );
    }

    #[test]
    fn stops_after_the_last_page() {
        assert_eq!(
            next_cursor(false, Some("ignored".to_string())).unwrap(),
            None
        );
    }

    #[test]
    fn rejects_a_missing_cursor_for_the_next_page() {
        assert!(next_cursor(true, None).is_err());
    }

    #[test]
    fn advances_through_assignable_user_pages() {
        assert_eq!(
            next_assignable_users_cursor(true, Some("cursor".to_string())).unwrap(),
            Some("cursor".to_string())
        );
        assert_eq!(next_assignable_users_cursor(false, None).unwrap(), None);
        assert!(next_assignable_users_cursor(true, None).is_err());
    }
}
