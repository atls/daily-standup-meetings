use anyhow::Result;
use std::rc::Rc;

use crate::domain::{issue::IssueId, repo::RepoId, repository::IssueRepository};

#[derive(Clone)]
pub struct GetIssuesQuery<R: IssueRepository> {
    pub repo: Rc<R>,
}

impl<R: IssueRepository> GetIssuesQuery<R> {
    pub async fn execute(&self, repo_id: &RepoId, issue_type: &str) -> Result<Vec<IssueId>> {
        self.repo.get_issues(repo_id, issue_type).await
    }
}
