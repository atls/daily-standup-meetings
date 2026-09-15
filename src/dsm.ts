import type { GitHubClient } from './github.js'
import type { GitHubUser }   from './github.js'

const MAX_ASSIGNEES = 10
const MAX_ISSUE_BODY_LENGTH = 65_536

export const buildBody = (template: string, members: Array<GitHubUser>): string => {
  const mentions = members.map(({ login }) => `@${login}`).join(' ')
  const body = `${template}\n<details>\n${mentions}\n</details>`

  if (body.length > MAX_ISSUE_BODY_LENGTH) {
    throw new Error(`DSM issue body exceeds ${MAX_ISSUE_BODY_LENGTH} characters`)
  }

  return body
}

export const selectAssignees = (
  members: Array<GitHubUser>,
  assignable: Array<GitHubUser>
): Array<string> => {
  const assignableIds = new Set(assignable.map(({ nodeId }) => nodeId))

  return members
    .filter(({ nodeId }) => assignableIds.has(nodeId))
    .slice(0, MAX_ASSIGNEES)
    .map(({ login }) => login)
}

const collectTeamMembers = async (
  client: GitHubClient,
  owner: string,
  teamSlugs: Array<string>
): Promise<Array<GitHubUser>> => {
  const memberIds = new Set<string>()
  const members: Array<GitHubUser> = []
  const teams = await Promise.all(
    teamSlugs.map(async (teamSlug) => client.teamMembers(owner, teamSlug))
  )

  for (const team of teams) {
    for (const member of team) {
      if (!memberIds.has(member.nodeId)) {
        memberIds.add(member.nodeId)
        members.push(member)
      }
    }
  }

  return members
}

export const runDsm = async (
  client: GitHubClient,
  owner: string,
  repo: string,
  teamSlugs: Array<string>,
  expectedIssueType: string,
  title: string,
  template: string
): Promise<void> => {
  const issueType = await client.issueType(owner, repo, expectedIssueType)
  const currentIssue = await client.latestOpenIssue(owner, repo, issueType)

  if (currentIssue?.title === title) {
    return
  }

  const members = await collectTeamMembers(client, owner, teamSlugs)
  const assignable = await client.assignableUsers(owner, repo)
  const assignees = selectAssignees(members, assignable)
  const body = buildBody(template, members)

  if (currentIssue) {
    await client.closeIssue(owner, repo, currentIssue.number)
  }

  await client.createIssue(owner, repo, title, body, issueType, assignees)
}
