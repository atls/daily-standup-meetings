use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use std::{collections::HashSet, env, path::PathBuf};

const DEFAULT_TIMEZONE: &str = "UTC";
const TITLE_DATE_FORMAT: &str = "%a %b %d %Y";

pub struct Config {
    pub github_token: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub team_slugs: Vec<String>,
    pub issue_type: String,
    pub template_path: Option<PathBuf>,
    timezone: Tz,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|name| env::var(name).ok())
    }

    pub fn title(&self, now: DateTime<Utc>) -> String {
        format!(
            "[DSM] {}",
            now.with_timezone(&self.timezone)
                .date_naive()
                .format(TITLE_DATE_FORMAT)
        )
    }

    fn from_lookup<F>(mut lookup: F) -> Result<Self>
    where
        F: FnMut(&str) -> Option<String>,
    {
        let timezone_name = lookup("DSM_TIMEZONE").unwrap_or_else(|| DEFAULT_TIMEZONE.to_string());
        let timezone = timezone_name
            .parse::<Tz>()
            .with_context(|| format!("invalid DSM_TIMEZONE `{timezone_name}`"))?;

        Ok(Self {
            github_token: lookup("GITHUB_TOKEN").context("GITHUB_TOKEN is required")?,
            repo_owner: lookup("GITHUB_REPO_OWNER").context("GITHUB_REPO_OWNER is required")?,
            repo_name: lookup("GITHUB_REPO_NAME").context("GITHUB_REPO_NAME is required")?,
            team_slugs: Self::team_slugs(
                lookup("DSM_TEAM_SLUGS").context("DSM_TEAM_SLUGS is required")?,
            )?,
            issue_type: lookup("DSM_ISSUE_TYPE")
                .map(|issue_type| issue_type.trim().to_string())
                .filter(|issue_type| !issue_type.is_empty())
                .context("DSM_ISSUE_TYPE is required")?,
            template_path: lookup("DSM_TEMPLATE_PATH")
                .filter(|path| !path.trim().is_empty())
                .map(PathBuf::from)
                .map(|path| {
                    Self::template_path(
                        path,
                        lookup("DSM_HOST_WORKSPACE").map(PathBuf::from),
                        lookup("GITHUB_WORKSPACE").map(PathBuf::from),
                    )
                }),
            timezone,
        })
    }

    fn team_slugs(value: String) -> Result<Vec<String>> {
        let mut seen = HashSet::new();
        let team_slugs = value
            .split([',', '\n'])
            .map(str::trim)
            .filter(|slug| !slug.is_empty())
            .filter(|slug| seen.insert(slug.to_ascii_lowercase()))
            .map(str::to_string)
            .collect::<Vec<_>>();

        if team_slugs.is_empty() {
            bail!("DSM_TEAM_SLUGS must contain at least one team slug");
        }

        Ok(team_slugs)
    }

    fn template_path(
        path: PathBuf,
        host_workspace: Option<PathBuf>,
        container_workspace: Option<PathBuf>,
    ) -> PathBuf {
        if let (Some(host_workspace), Some(container_workspace)) =
            (host_workspace, container_workspace)
        {
            if let Ok(relative_path) = path.strip_prefix(host_workspace) {
                return container_workspace.join(relative_path);
            }
        }

        path
    }
}

#[cfg(test)]
mod tests {
    use super::Config;
    use chrono::{TimeZone, Utc};
    use std::collections::HashMap;

    fn required_values() -> HashMap<&'static str, String> {
        HashMap::from([
            ("GITHUB_TOKEN", "token".to_string()),
            ("GITHUB_REPO_OWNER", "example".to_string()),
            ("GITHUB_REPO_NAME", "service".to_string()),
            ("DSM_TEAM_SLUGS", "engineering".to_string()),
            ("DSM_ISSUE_TYPE", "DSM".to_string()),
        ])
    }

    #[test]
    fn loads_action_inputs_and_formats_the_title_in_the_requested_timezone() {
        let mut values = required_values();
        values.insert(
            "DSM_TEAM_SLUGS",
            "platform\nproduct,Platform\noperations".to_string(),
        );
        values.insert("DSM_ISSUE_TYPE", "Standup".to_string());
        values.insert("DSM_TEMPLATE_PATH", "templates/standup.md".to_string());
        values.insert("DSM_TIMEZONE", "Europe/Moscow".to_string());

        let config = Config::from_lookup(|name| values.get(name).cloned()).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 9, 10, 21, 30, 0).unwrap();

        assert_eq!(config.team_slugs, vec!["platform", "product", "operations"]);
        assert_eq!(config.issue_type, "Standup");
        assert_eq!(
            config.template_path.as_deref(),
            Some(std::path::Path::new("templates/standup.md"))
        );
        assert_eq!(config.title(now), "[DSM] Fri Sep 11 2026");
    }

    #[test]
    fn maps_a_runner_workspace_path_to_the_container_workspace() {
        let mut values = required_values();
        values.insert(
            "DSM_TEMPLATE_PATH",
            "/home/runner/work/service/service/.github/ISSUE_TEMPLATE/dsm.md".to_string(),
        );
        values.insert(
            "DSM_HOST_WORKSPACE",
            "/home/runner/work/service/service".to_string(),
        );
        values.insert("GITHUB_WORKSPACE", "/github/workspace".to_string());

        let config = Config::from_lookup(|name| values.get(name).cloned()).unwrap();

        assert_eq!(
            config.template_path.as_deref(),
            Some(std::path::Path::new(
                "/github/workspace/.github/ISSUE_TEMPLATE/dsm.md"
            ))
        );
    }

    #[test]
    fn uses_the_built_in_template_when_no_override_is_set() {
        let values = required_values();

        let config = Config::from_lookup(|name| values.get(name).cloned()).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 9, 10, 21, 30, 0).unwrap();

        assert_eq!(config.team_slugs, vec!["engineering"]);
        assert_eq!(config.issue_type, "DSM");
        assert_eq!(config.template_path, None);
        assert_eq!(config.title(now), "[DSM] Thu Sep 10 2026");
    }

    #[test]
    fn rejects_an_empty_team_list_before_github_is_called() {
        let mut values = required_values();
        values.insert("DSM_TEAM_SLUGS", " , \n".to_string());

        let error = Config::from_lookup(|name| values.get(name).cloned())
            .err()
            .unwrap();

        assert_eq!(
            error.to_string(),
            "DSM_TEAM_SLUGS must contain at least one team slug"
        );
    }

    #[test]
    fn rejects_an_empty_issue_type_before_github_is_called() {
        let mut values = required_values();
        values.insert("DSM_ISSUE_TYPE", "  ".to_string());

        let error = Config::from_lookup(|name| values.get(name).cloned())
            .err()
            .unwrap();

        assert_eq!(error.to_string(), "DSM_ISSUE_TYPE is required");
    }

    #[test]
    fn uses_the_built_in_template_when_action_passes_an_empty_override() {
        let mut values = required_values();
        values.insert("DSM_TEMPLATE_PATH", String::new());

        let config = Config::from_lookup(|name| values.get(name).cloned()).unwrap();

        assert_eq!(config.template_path, None);
    }

    #[test]
    fn rejects_an_unknown_timezone_before_github_is_called() {
        let mut values = required_values();
        values.insert("DSM_TIMEZONE", "Mars/Olympus_Mons".to_string());

        let error = Config::from_lookup(|name| values.get(name).cloned())
            .err()
            .unwrap();

        assert_eq!(
            error.to_string(),
            "invalid DSM_TIMEZONE `Mars/Olympus_Mons`"
        );
    }
}
