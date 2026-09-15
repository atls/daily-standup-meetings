const API_ROOT = 'https://api.github.com/';
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
export class GitHubClient {
    #headers;
    constructor(token) {
        this.#headers = new Headers({
            Accept: 'application/vnd.github+json',
            Authorization: `Bearer ${token}`,
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
            if (next.origin !== new URL(API_ROOT).origin) {
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
        return new URL(segments.map(encodeURIComponent).join('/'), API_ROOT);
    }
    errorMessage(error) {
        return error instanceof Error ? error.message : String(error);
    }
}
