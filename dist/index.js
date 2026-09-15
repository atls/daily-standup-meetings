import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
const require = createRequire(import.meta.url)
const __filename = fileURLToPath(import.meta.url)

import { createRequire as __WEBPACK_EXTERNAL_createRequire } from "node:module";
var __webpack_exports__ = {};

;// external "node:fs/promises"
const promises_namespaceObject = __WEBPACK_EXTERNAL_createRequire(import.meta.url)("node:fs/promises");
;// external "node:path"
const external_node_path_namespaceObject = __WEBPACK_EXTERNAL_createRequire(import.meta.url)("node:path");
;// external "node:url"
const external_node_url_namespaceObject = __WEBPACK_EXTERNAL_createRequire(import.meta.url)("node:url");
;// ./src/github.ts
const DEFAULT_API_ROOT = 'https://api.github.com/';
const API_VERSION = '2026-03-10';
const MAX_PAGE_SIZE = '100';
const asRecord = (value, context) => {
    if (!value || typeof value !== 'object' || Array.isArray(value)) {
        throw new Error(`GitHub ${context} response had an unexpected shape`);
    }
    return value;
};
const asArray = (value, context) => {
    if (!Array.isArray(value)) {
        throw new Error(`GitHub ${context} response had an unexpected shape`);
    }
    return value;
};
const readString = (record, name, context) => {
    const value = record[name];
    if (typeof value !== 'string') {
        throw new Error(`GitHub ${context} response had an unexpected ${name}`);
    }
    return value;
};
const readNumber = (record, name, context) => {
    const value = record[name];
    if (typeof value !== 'number') {
        throw new Error(`GitHub ${context} response had an unexpected ${name}`);
    }
    return value;
};
const parseUser = (value) => {
    const record = asRecord(value, 'user');
    return {
        login: readString(record, 'login', 'user'),
        nodeId: readString(record, 'node_id', 'user'),
    };
};
const parseIssue = (value) => {
    const record = asRecord(value, 'issue');
    const issueType = record.type ? asRecord(record.type, 'issue type') : undefined;
    const assignees = record.assignees === undefined ? [] : asArray(record.assignees, 'assignees');
    return {
        number: readNumber(record, 'number', 'issue'),
        title: readString(record, 'title', 'issue'),
        issueType: issueType ? readString(issueType, 'name', 'issue type') : undefined,
        assignees: assignees.map(parseUser),
        pullRequest: record.pull_request !== undefined,
    };
};
class GitHubClient {
    #apiRoot;
    #headers;
    constructor(token, apiRoot = process.env.GITHUB_API_URL ?? DEFAULT_API_ROOT) {
        this.#apiRoot = new URL(apiRoot.endsWith('/') ? apiRoot : `${apiRoot}/`);
        this.#headers = new Headers({
            Accept: 'application/vnd.github+json',
            Authorization: `Bearer ${token}`,
            'Content-Type': 'application/json',
            'User-Agent': 'atls-daily-standup-meetings',
            'X-GitHub-Api-Version': API_VERSION,
        });
    }
    async teamMembers(owner, teamSlug) {
        const url = this.endpoint('orgs', owner, 'teams', teamSlug, 'members');
        url.searchParams.set('role', 'all');
        url.searchParams.set('per_page', MAX_PAGE_SIZE);
        return this.getAll(url, parseUser);
    }
    async assignableUsers(owner, repo) {
        const url = this.endpoint('repos', owner, repo, 'assignees');
        url.searchParams.set('per_page', MAX_PAGE_SIZE);
        return this.getAll(url, parseUser);
    }
    async issueType(owner, repo, expected) {
        const values = asArray(await this.getJson(this.endpoint('repos', owner, repo, 'issue-types')), 'issue types');
        const issueTypes = values.map((value) => {
            const record = asRecord(value, 'issue type');
            return readString(record, 'name', 'issue type');
        });
        const issueType = issueTypes.find((value) => value.trim().toLowerCase() === expected.trim().toLowerCase());
        if (!issueType) {
            throw new Error(`repository issue type \`${expected}\` was not found`);
        }
        return issueType;
    }
    async latestOpenIssue(owner, repo, issueType) {
        const url = this.endpoint('repos', owner, repo, 'issues');
        url.searchParams.set('state', 'open');
        url.searchParams.set('type', issueType);
        url.searchParams.set('sort', 'created');
        url.searchParams.set('direction', 'desc');
        url.searchParams.set('per_page', '1');
        const issues = asArray(await this.getJson(url), 'issues').map(parseIssue);
        const issue = issues.find((candidate) => !candidate.pullRequest &&
            candidate.issueType?.trim().toLowerCase() === issueType.trim().toLowerCase());
        return issue ? { number: issue.number, title: issue.title } : undefined;
    }
    async createIssue(owner, repo, title, body, issueType, assignees) {
        const created = parseIssue(await this.getJson(this.endpoint('repos', owner, repo, 'issues'), {
            body: JSON.stringify({ title, body, assignees, type: issueType }),
            method: 'POST',
        }));
        try {
            this.verifyCreatedIssue(created, title, issueType, assignees);
        }
        catch (error) {
            try {
                await this.closeIssue(owner, repo, created.number);
            }
            catch (cleanupError) {
                throw new Error(`${this.errorMessage(error)}; failed to close invalid issue #${created.number}: ${this.errorMessage(cleanupError)}`);
            }
            throw error;
        }
        return { number: created.number, title: created.title };
    }
    async closeIssue(owner, repo, number) {
        await this.request(this.endpoint('repos', owner, repo, 'issues', String(number)), {
            body: JSON.stringify({ state: 'closed' }),
            method: 'PATCH',
        });
    }
    async getAll(url, parse) {
        return this.getAllPage(url, parse, []);
    }
    async getAllPage(url, parse, items) {
        const response = await this.request(url);
        const values = asArray(await this.responseJson(response), 'page');
        const next = this.nextPage(response);
        items.push(...values.map(parse));
        return next ? this.getAllPage(next, parse, items) : items;
    }
    async getJson(url, init) {
        return this.responseJson(await this.request(url, init));
    }
    async responseJson(response) {
        try {
            return await response.json();
        }
        catch {
            throw new Error('GitHub API response was not valid JSON');
        }
    }
    async request(url, init = {}) {
        const response = await fetch(url, {
            ...init,
            headers: this.#headers,
            redirect: 'manual',
        });
        const method = init.method ?? 'GET';
        if (!response.ok) {
            throw new Error(`GitHub API ${method} ${url.pathname} returned HTTP ${response.status}`);
        }
        return response;
    }
    nextPage(response) {
        const link = response.headers.get('link');
        if (!link) {
            return undefined;
        }
        for (const entry of link.split(',')) {
            const [target, ...parameters] = entry.trim().split(';');
            if (!parameters.some((parameter) => parameter.trim() === 'rel="next"')) {
                continue;
            }
            const value = target.trim().match(/^<(.+)>$/u)?.[1];
            if (!value) {
                throw new Error('GitHub pagination next link was malformed');
            }
            const next = new URL(value);
            if (next.origin !== this.#apiRoot.origin) {
                throw new Error('GitHub pagination next link changed API origin');
            }
            return next;
        }
        return undefined;
    }
    verifyCreatedIssue(created, expectedTitle, expectedType, expectedAssignees) {
        if (created.title !== expectedTitle) {
            throw new Error('GitHub created an issue with an unexpected title');
        }
        if (created.issueType?.trim().toLowerCase() !== expectedType.trim().toLowerCase()) {
            throw new Error('GitHub did not apply the requested issue type');
        }
        const actualAssignees = new Set(created.assignees.map(({ login }) => login.toLowerCase()));
        if (!expectedAssignees.every((login) => actualAssignees.has(login.toLowerCase()))) {
            throw new Error('GitHub did not apply all requested assignees');
        }
    }
    endpoint(...segments) {
        return new URL(segments.map(encodeURIComponent).join('/'), this.#apiRoot);
    }
    errorMessage(error) {
        return error instanceof Error ? error.message : String(error);
    }
}

;// ./src/config.ts


const DEFAULT_TIMEZONE = 'UTC';
const required = (environment, name) => {
    const value = environment[name];
    if (!value || value.trim().length === 0) {
        throw new Error(`${name} is required`);
    }
    return value;
};
const parseTeamSlugs = (value) => {
    const seen = new Set();
    const teamSlugs = value
        .split(/[,\n]/u)
        .map((slug) => slug.trim())
        .filter((slug) => slug.length > 0)
        .filter((slug) => {
        const normalized = slug.toLowerCase();
        if (seen.has(normalized)) {
            return false;
        }
        seen.add(normalized);
        return true;
    });
    if (teamSlugs.length === 0) {
        throw new Error('INPUT_TEAM-SLUGS must contain at least one team slug');
    }
    return teamSlugs;
};
const parseRepository = (value) => {
    const [owner, repo, extra] = value.split('/');
    if (!owner || !repo || extra) {
        throw new Error('GITHUB_REPOSITORY must use the owner/repository format');
    }
    return { owner, repo };
};
const validateTimezone = (timezone) => {
    try {
        new Intl.DateTimeFormat('en-US', { timeZone: timezone }).format();
    }
    catch {
        throw new Error(`invalid timezone \`${timezone}\``);
    }
    return timezone;
};
const resolveTemplatePath = (templateInput, workspace) => {
    if (!templateInput) {
        return undefined;
    }
    return (0,external_node_path_namespaceObject.isAbsolute)(templateInput) ? templateInput : (0,external_node_path_namespaceObject.resolve)(workspace, templateInput);
};
const loadConfig = (environment = process.env) => {
    const repository = parseRepository(required(environment, 'GITHUB_REPOSITORY'));
    const issueType = required(environment, 'INPUT_ISSUE-TYPE').trim();
    const timezone = validateTimezone((environment.INPUT_TIMEZONE ?? DEFAULT_TIMEZONE).trim());
    const templateInput = environment['INPUT_TEMPLATE-PATH']?.trim();
    const workspace = environment.GITHUB_WORKSPACE ?? process.cwd();
    return {
        githubToken: required(environment, 'INPUT_GITHUB-TOKEN'),
        ...repository,
        teamSlugs: parseTeamSlugs(required(environment, 'INPUT_TEAM-SLUGS')),
        issueType,
        templatePath: resolveTemplatePath(templateInput, workspace),
        timezone,
    };
};
const formatTitle = (timezone, now = new Date()) => {
    const parts = new Intl.DateTimeFormat('en-US', {
        day: '2-digit',
        month: 'short',
        timeZone: timezone,
        weekday: 'short',
        year: 'numeric',
    })
        .formatToParts(now)
        .reduce((result, part) => {
        result[part.type] = part.value;
        return result;
    }, {});
    return `[DSM] ${parts.weekday} ${parts.month} ${parts.day} ${parts.year}`;
};

;// ./src/dsm.ts
const MAX_ASSIGNEES = 10;
const MAX_ISSUE_BODY_LENGTH = 65_536;
const buildBody = (template, members) => {
    const mentions = members.map(({ login }) => `@${login}`).join(' ');
    const body = `${template}\n<details>\n${mentions}\n</details>`;
    if (body.length > MAX_ISSUE_BODY_LENGTH) {
        throw new Error(`DSM issue body exceeds ${MAX_ISSUE_BODY_LENGTH} characters`);
    }
    return body;
};
const selectAssignees = (members, assignable) => {
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
const runDsm = async (client, owner, repo, teamSlugs, expectedIssueType, title, template) => {
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

;// ./src/index.ts








const actionRoot = (0,external_node_path_namespaceObject.resolve)((0,external_node_path_namespaceObject.dirname)((0,external_node_url_namespaceObject.fileURLToPath)(import.meta.url)), '..');
const main = async () => {
    const config = loadConfig();
    const template = await (0,promises_namespaceObject.readFile)(config.templatePath ?? (0,external_node_path_namespaceObject.resolve)(actionRoot, 'templates', 'dsm.md'), 'utf8');
    const client = new GitHubClient(config.githubToken);
    await runDsm(client, config.owner, config.repo, config.teamSlugs, config.issueType, formatTitle(config.timezone), template);
};
main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
});


//# sourceMappingURL=index.js.map