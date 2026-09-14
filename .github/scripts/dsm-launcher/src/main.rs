use anyhow::Result;
use application::commands::close_issue::CloseIssueCommand;
use application::commands::create_issue::CreateIssueCommand;
use application::queries::get_issue_types::GetIssueTypes;
use application::queries::get_issues::GetIssuesQuery;
use application::queries::get_org::GetOrgQuery;
use application::queries::get_repo::GetRepoQuery;
use application::queries::get_team::GetTeamQuery;
use application::queries::get_team_members::GetTeamMembersQuery;
use chrono::Utc;
use config::Config;
use infrastructure::github_adapter::GitHubAdapter;
use std::fs;
use std::rc::Rc;

mod graphql_queries {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/graphql/mod.rs"));
}

mod application;
mod config;
mod domain;
mod infrastructure;

use application::use_cases::{
    close_issues::{close_issues, get_issues_to_close},
    create_issue::create_issue,
};
use infrastructure::github_graphql_client::GitHubGraphQLClient;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;
    let body = fs::read_to_string(&config.template_path)?;
    let title = config.title(Utc::now());

    let client = GitHubGraphQLClient::new("dsm-launcher".to_string(), config.github_token)?;

    let adapter = Rc::new(GitHubAdapter::new(client.clone()));

    let get_org = GetOrgQuery {
        repo: adapter.clone(),
    };
    let get_repo = GetRepoQuery {
        repo: adapter.clone(),
    };
    let get_issues = GetIssuesQuery {
        repo: adapter.clone(),
    };
    let get_issue_types = GetIssueTypes {
        repo: adapter.clone(),
    };
    let close_issue = CloseIssueCommand {
        repo: adapter.clone(),
    };
    let create_issue_ = CreateIssueCommand {
        repo: adapter.clone(),
    };
    let get_team = GetTeamQuery {
        repo: adapter.clone(),
    };
    let get_team_members = GetTeamMembersQuery { repo: adapter };

    let (current_issue_exists, issues_to_close) = get_issues_to_close(
        get_org.clone(),
        get_repo.clone(),
        get_issues,
        &config.repo_owner,
        &config.repo_name,
        &config.team_slug,
        &title,
    )
    .await?;

    if !current_issue_exists {
        create_issue(
            get_org,
            get_repo,
            get_team,
            get_team_members,
            get_issue_types,
            create_issue_,
            &config.repo_owner,
            &config.repo_name,
            &config.team_slug,
            &config.team_slug,
            &title,
            &body,
        )
        .await?;
    }

    close_issues(close_issue, &issues_to_close).await?;

    Ok(())
}
