use anyhow::{Ok, Result};

use crate::application::{
    commands::close_issue::CloseIssueCommand,
    queries::{get_issues::GetIssuesQuery, get_org::GetOrgQuery, get_repo::GetRepoQuery},
};
use crate::domain::{
    issue::{IssueId, OpenIssue},
    repository::{IssueRepository, OrgRepository},
};

pub async fn get_issues_to_close<R: OrgRepository, I: IssueRepository>(
    get_org: GetOrgQuery<R>,
    get_repo: GetRepoQuery<R>,
    get_issues: GetIssuesQuery<I>,
    owner: &str,
    repo_name: &str,
    issue_type: &str,
    current_title: &str,
) -> Result<(bool, Vec<IssueId>)> {
    let org_id = get_org.execute(owner).await?;
    let repo_id = get_repo.execute(&org_id, &repo_name).await?;
    let issues = get_issues.execute(&repo_id, issue_type).await?;

    Ok(select_issues_to_close(issues, current_title))
}

pub async fn close_issues<I: IssueRepository>(
    close_issue: CloseIssueCommand<I>,
    issues: &[IssueId],
) -> Result<()> {
    for issue in issues {
        close_issue.execute(issue).await?;
    }

    Ok(())
}

fn select_issues_to_close(issues: Vec<OpenIssue>, current_title: &str) -> (bool, Vec<IssueId>) {
    let mut current_issue_exists = false;
    let mut issues_to_close = Vec::new();

    for issue in issues {
        if issue.title == current_title && !current_issue_exists {
            current_issue_exists = true;
        } else {
            issues_to_close.push(issue.id);
        }
    }

    (current_issue_exists, issues_to_close)
}

#[cfg(test)]
mod tests {
    use super::select_issues_to_close;
    use crate::domain::issue::{IssueId, OpenIssue};

    #[test]
    fn preserves_one_current_issue_and_closes_stale_or_duplicate_issues() {
        let issues = vec![
            OpenIssue::new(IssueId::new("old".to_string()), "[DSM] Sunday".to_string()),
            OpenIssue::new(
                IssueId::new("current".to_string()),
                "[DSM] Monday".to_string(),
            ),
            OpenIssue::new(
                IssueId::new("duplicate".to_string()),
                "[DSM] Monday".to_string(),
            ),
        ];

        let (current_issue_exists, issues_to_close) =
            select_issues_to_close(issues, "[DSM] Monday");

        assert!(current_issue_exists);
        assert_eq!(issues_to_close.len(), 2);
        assert_eq!(&*issues_to_close[0], "old");
        assert_eq!(&*issues_to_close[1], "duplicate");
    }

    #[test]
    fn requests_creation_when_no_current_issue_exists() {
        let issues = vec![OpenIssue::new(
            IssueId::new("old".to_string()),
            "[DSM] Sunday".to_string(),
        )];

        let (current_issue_exists, issues_to_close) =
            select_issues_to_close(issues, "[DSM] Monday");

        assert!(!current_issue_exists);
        assert_eq!(issues_to_close.len(), 1);
    }
}
