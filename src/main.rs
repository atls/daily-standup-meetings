use anyhow::Result;
use chrono::Utc;
use std::fs;

use crate::{config::Config, github::GitHubClient};

mod config;
mod dsm;
mod github;

const DEFAULT_TEMPLATE: &str = include_str!("../templates/dsm.md");

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;
    let template = match &config.template_path {
        Some(path) => fs::read_to_string(path)?,
        None => DEFAULT_TEMPLATE.to_string(),
    };
    let title = config.title(Utc::now());
    let client = GitHubClient::new(&config.github_token)?;

    dsm::run(
        &client,
        &config.repo_owner,
        &config.repo_name,
        &config.team_slugs,
        &config.issue_type,
        &title,
        &template,
    )
    .await
}
