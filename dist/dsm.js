const MAX_ASSIGNEES = 10;
const MAX_ISSUE_BODY_LENGTH = 65_536;
export const buildBody = (template, members) => {
    const mentions = members.map(({ login }) => `@${login}`).join(' ');
    const body = `${template}\n<details>\n${mentions}\n</details>`;
    if (body.length > MAX_ISSUE_BODY_LENGTH) {
        throw new Error(`DSM issue body exceeds ${MAX_ISSUE_BODY_LENGTH} characters`);
    }
    return body;
};
export const selectAssignees = (members, assignable) => {
    const assignableIds = new Set(assignable.map(({ nodeId }) => nodeId));
    return members
        .filter(({ nodeId }) => assignableIds.has(nodeId))
        .slice(0, MAX_ASSIGNEES)
        .map(({ login }) => login);
};
const collectTeamMembers = async (client, owner, teamSlugs) => {
    const memberIds = new Set();
    const members = [];
    const teams = await Promise.all(teamSlugs.map(async (teamSlug) => client.teamMembers(owner, teamSlug)));
    for (const team of teams) {
        for (const member of team) {
            if (!memberIds.has(member.nodeId)) {
                memberIds.add(member.nodeId);
                members.push(member);
            }
        }
    }
    return members;
};
export const runDsm = async (client, owner, repo, teamSlugs, expectedIssueType, title, template) => {
    const issueType = await client.issueType(owner, repo, expectedIssueType);
    const currentIssue = await client.latestOpenIssue(owner, repo, issueType);
    if (currentIssue?.title === title) {
        return;
    }
    const members = await collectTeamMembers(client, owner, teamSlugs);
    const assignable = await client.assignableUsers(owner, repo);
    const assignees = selectAssignees(members, assignable);
    const body = buildBody(template, members);
    if (currentIssue) {
        await client.closeIssue(owner, repo, currentIssue.number);
    }
    await client.createIssue(owner, repo, title, body, issueType, assignees);
};
