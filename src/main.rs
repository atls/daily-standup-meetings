use anyhow::Result;
use chrono::Utc;
use std::fs;

use crate::{config::Config, github::GitHubClient};

mod config;
mod dsm;
mod github;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;
    let template = fs::read_to_string(&config.template_path)?;
    let title = config.title(Utc::now());
    let client = GitHubClient::new(&config.github_token)?;

    dsm::run(
        &client,
        &config.repo_owner,
        &config.repo_name,
        &config.team_slug,
        &title,
        &template,
    )
    .await
}
