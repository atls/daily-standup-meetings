use super::github_graphql_client::GitHubGraphQLClient;

mod errors;
mod issue_adapter;
mod member_adapter;
mod org_adapter;

#[derive(Clone)]
pub struct GitHubAdapter {
    pub client: GitHubGraphQLClient,
}

impl GitHubAdapter {
    pub fn new(client: GitHubGraphQLClient) -> Self {
        GitHubAdapter { client }
    }
}
