use anyhow::{Ok, Result};
use async_trait::async_trait;

use crate::domain::{
    issue::{Issue, IssueId, IssueType},
    repo::RepoId,
    repository::IssueRepository,
};
use crate::graphql_queries::{
    close_issue::{close_issue::Variables as CloseIssueVars, CloseIssue},
    create_issue::{create_issue::Variables as CreateIssueVars, CreateIssue},
    get_issue_types::{
        get_issue_types::{GetIssueTypesNode, Variables as GetIssueTypesVars},
        GetIssueTypes,
    },
    get_open_issues::{
        get_open_issues::{GetOpenIssuesNode, Variables as GetOpenIssuesVars},
        GetOpenIssues,
    },
};

use crate::domain::errors::{issue::IssueError, issue_types::IssueTypesError, repo::RepoError};

use super::{errors::GitHubAdapterError, GitHubAdapter};

const DSM_TITLE_PREFIX: &str = "[DSM] ";
const MAX_ASSIGNEES: usize = 10;

#[async_trait]
impl IssueRepository for GitHubAdapter {
    async fn get_issues(&self, repo_id: &RepoId, issue_type: &str) -> Result<Vec<IssueId>> {
        let mut cursor = None;
        let mut issues = Vec::new();

        loop {
            let vars = GetOpenIssuesVars {
                id: repo_id.to_string(),
                cursor: cursor.clone(),
            };

            let response = self.client.execute::<GetOpenIssues>(vars).await?;

            if let Some(errors) = response.errors {
                return Err(GitHubAdapterError::GraphQL(errors).into());
            }

            let repo_data = response.data.ok_or(RepoError::RepoDataNotFound)?;
            let node = repo_data.node.ok_or(RepoError::RepoNodeNotFound)?;
            let repo = match node {
                GetOpenIssuesNode::Repository(repo) => repo,
                _ => return Err(GitHubAdapterError::UnexpectedNodeType.into()),
            };

            issues.extend(
                repo.issues
                    .nodes
                    .ok_or(IssueError::IssuesWereNotFound)?
                    .into_iter()
                    .flatten()
                    .filter(|issue| {
                        is_dsm_issue(
                            &issue.title,
                            issue
                                .issue_type
                                .as_ref()
                                .map(|issue_type| issue_type.name.as_str()),
                            issue_type,
                        )
                    })
                    .map(|issue| IssueId::new(issue.id)),
            );

            cursor = next_issues_cursor(
                repo.issues.page_info.has_next_page,
                repo.issues.page_info.end_cursor,
            )?;

            if cursor.is_none() {
                break;
            }
        }

        Ok(issues)
    }

    async fn get_issue_types(&self, repo_id: &RepoId) -> Result<Vec<IssueType>> {
        let vars = GetIssueTypesVars {
            id: repo_id.to_string(),
        };

        let response = self.client.execute::<GetIssueTypes>(vars).await?;

        if let Some(errors) = response.errors {
            return Err(GitHubAdapterError::GraphQL(errors).into());
        }

        let data = response.data.ok_or(RepoError::RepoDataNotFound)?;
        let node = data.node.ok_or(RepoError::RepoNodeNotFound)?;
        let issue_types = match node {
            GetIssueTypesNode::Repository(repo) => repo.issue_types,
            _ => return Err(GitHubAdapterError::UnexpectedNodeType.into()),
        };

        let issue_types = issue_types
            .ok_or(IssueTypesError::IssueTypesNodeNotFound)?
            .nodes
            .ok_or(IssueTypesError::IssueTypesNotFound)?
            .into_iter()
            .filter_map(|x| x.map(|y| IssueType::new(y.id, y.name)))
            .collect::<Vec<IssueType>>();

        Ok(issue_types)
    }

    async fn create_issue(&self, issue: Issue) -> Result<IssueId> {
        let logins = assignee_ids(&issue);

        let vars = CreateIssueVars {
            repo_id: issue.repo_id,
            title: issue.title,
            body: issue.body,
            assignee_ids: logins,
            issue_type_id: issue.issue_type_id,
        };

        let response = self.client.execute::<CreateIssue>(vars).await?;

        if let Some(errors) = response.errors {
            return Err(GitHubAdapterError::GraphQL(errors).into());
        }

        let response_data = response.data.ok_or(IssueError::EmptyCreateIssueResponse)?;
        let issue = response_data
            .create_issue
            .ok_or(IssueError::CreatedIssueNotFound)?;

        Ok(IssueId::new(
            issue.issue.ok_or(IssueError::CreatedIssueBodyNotFound)?.id,
        ))
    }

    async fn close_issue(&self, issue_id: &IssueId) -> Result<()> {
        let vars = CloseIssueVars {
            id: issue_id.to_string(),
        };

        let response = self.client.execute::<CloseIssue>(vars).await?;

        if let Some(errors) = response.errors {
            return Err(GitHubAdapterError::GraphQL(errors).into());
        }

        Ok(())
    }
}

fn assignee_ids(issue: &Issue) -> Vec<String> {
    issue
        .assignees
        .iter()
        .take(MAX_ASSIGNEES)
        .map(|member| member.id.to_string())
        .collect()
}

fn next_issues_cursor(has_next_page: bool, end_cursor: Option<String>) -> Result<Option<String>> {
    if has_next_page {
        return end_cursor
            .map(Some)
            .ok_or_else(|| IssueError::IssuesCursorNotFound.into());
    }

    Ok(None)
}

fn is_dsm_issue(title: &str, actual_issue_type: Option<&str>, expected_issue_type: &str) -> bool {
    title.starts_with(DSM_TITLE_PREFIX)
        && actual_issue_type.is_some_and(|issue_type| {
            issue_type
                .trim()
                .eq_ignore_ascii_case(expected_issue_type.trim())
        })
}

#[cfg(test)]
mod tests {
    use super::{assignee_ids, is_dsm_issue, next_issues_cursor};
    use crate::domain::{
        issue::Issue,
        member::{Member, MemberId},
    };

    #[test]
    fn identifies_dsm_issues_by_title_and_type() {
        assert!(is_dsm_issue("[DSM] Mon Sep 14 2026", Some("DSM"), "dsm"));
        assert!(!is_dsm_issue("Customer report", Some("DSM"), "DSM"));
        assert!(!is_dsm_issue("[DSM] Customer report", Some("Bug"), "DSM"));
        assert!(!is_dsm_issue("[DSM] Customer report", None, "DSM"));
    }

    #[test]
    fn caps_issue_assignees_without_removing_members_from_the_issue() {
        let issue = Issue {
            id: None,
            repo_id: "repo".to_string(),
            title: "title".to_string(),
            issue_type_id: "type".to_string(),
            body: "body".to_string(),
            assignees: (0..12)
                .map(|index| {
                    Member::new(
                        MemberId::new(format!("id-{index}")),
                        format!("member-{index}"),
                    )
                })
                .collect(),
        };

        let ids = assignee_ids(&issue);

        assert_eq!(ids.len(), 10);
        assert_eq!(ids.first().map(String::as_str), Some("id-0"));
        assert_eq!(ids.last().map(String::as_str), Some("id-9"));
        assert_eq!(issue.assignees.len(), 12);
    }

    #[test]
    fn advances_through_open_issue_pages() {
        assert_eq!(
            next_issues_cursor(true, Some("cursor".to_string())).unwrap(),
            Some("cursor".to_string())
        );
        assert_eq!(next_issues_cursor(false, None).unwrap(), None);
        assert!(next_issues_cursor(true, None).is_err());
    }
}
