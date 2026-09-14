use anyhow::Result;
use std::rc::Rc;

use crate::domain::{member::MemberId, repo::RepoId, repository::MemberRepository};

pub struct GetAssignableMembersQuery<R: MemberRepository> {
    pub repo: Rc<R>,
}

impl<R: MemberRepository> GetAssignableMembersQuery<R> {
    pub async fn execute(&self, repo_id: &RepoId) -> Result<Vec<MemberId>> {
        self.repo.get_assignable_members(repo_id).await
    }
}
