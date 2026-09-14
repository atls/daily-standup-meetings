# Daily Standup Meetings

[![GitHub Marketplace](https://img.shields.io/badge/Marketplace-Daily%20Standup%20Meetings-blue?logo=github)](https://github.com/marketplace/actions/daily-standup-meetings)

DSM creates a dated issue in the current repository, mentions every member of
the selected GitHub organization team, and assigns the first 10 members allowed
by GitHub's per-issue assignee limit. The issue body is read from a template in
the consumer repository; the token and all organization-specific settings stay
in that repository.

## Usage

```yaml
name: DSM

on:
  schedule:
    - cron: "0 4 * * 1-5"
  workflow_dispatch:

concurrency:
  group: dsm-${{ github.repository }}
  cancel-in-progress: false

permissions:
  contents: read

jobs:
  dsm:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7.0.1

      - uses: actions/create-github-app-token@v3.2.0
        id: app-token
        with:
          app-id: ${{ vars.DSM_APP_ID }}
          private-key: ${{ secrets.DSM_APP_PRIVATE_KEY }}
          owner: ${{ github.repository_owner }}
          repositories: ${{ github.event.repository.name }}

      - uses: atls/daily-standup-meetings@v1.0.0
        with:
          github-token: ${{ steps.app-token.outputs.token }}
          team-slug: engineering
          template-path: .github/ISSUE_TEMPLATE/dsm.md
          timezone: Europe/Moscow
```

Use the same `dsm-${{ github.repository }}` concurrency group in every workflow
that invokes the Action so overlapping runs cannot create duplicate issues.

The Action runs its Rust binary directly in a Docker container. Consumers need
a Linux runner with Docker support and do not need Rust, Bash, GitHub CLI, or a
separately downloaded release asset.

## Inputs

| Input | Description |
| --- | --- |
| `github-token` | GitHub App installation token used to read the team and create or close DSM issues. |
| `team-slug` | Slug of the organization team whose members are mentioned; the first 10 are assigned. The same value must exist as a repository issue type. |
| `template-path` | Absolute path or path relative to `GITHUB_WORKSPACE` containing the issue body. |
| `timezone` | IANA timezone used to calculate the date in the issue title, for example `Europe/Moscow`. |

## GitHub App access

Configure the GitHub App with the following minimum permissions:

- Repository permissions: `Metadata: read` and `Issues: read and write`.
- Organization permissions: `Members: read`.

Install the App on the consumer organization and grant it access only to the
repositories where DSM may manage issues. Keep the App ID in repository
variables and its private key in repository secrets.

If the organization restricts Actions, allow `atls/daily-standup-meetings@*` in
the selected actions allowlist.

## Release boundary

Create `v1.0.0` at the reviewed release commit, point the compatible `v1` tag at
the same commit, and publish that release to GitHub Marketplace.

License selection, release and tag creation, Marketplace publication, and
acceptance in a private consumer repository are post-merge steps and are not
performed by this change.

## DSM template

#### What did you work on yesterday?
#### What will you work on today? Include issue references.
#### Is anything blocking you? (Use this section only when you are blocked.)
#### Do you need to be away from work for personal reasons during the day? (Use this section only when applicable.)
