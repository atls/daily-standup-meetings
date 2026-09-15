const API_ROOT = 'https://api.github.com/'
const API_VERSION = '2026-03-10'
const MAX_PAGE_SIZE = '100'

export type GitHubUser = Readonly<{
  login: string
  nodeId: string
}>

export type OpenIssue = Readonly<{
  number: number
  title: string
}>

type JsonRecord = Record<string, unknown>

const asRecord = (value: unknown, context: string): JsonRecord => {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`GitHub ${context} response had an unexpected shape`)
  }

  return value as JsonRecord
}

const asArray = (value: unknown, context: string): Array<unknown> => {
  if (!Array.isArray(value)) {
    throw new Error(`GitHub ${context} response had an unexpected shape`)
  }

  return value
}

const readString = (record: JsonRecord, name: string, context: string): string => {
  const value = record[name]

  if (typeof value !== 'string') {
    throw new Error(`GitHub ${context} response had an unexpected ${name}`)
  }

  return value
}

const readNumber = (record: JsonRecord, name: string, context: string): number => {
  const value = record[name]

  if (typeof value !== 'number') {
    throw new Error(`GitHub ${context} response had an unexpected ${name}`)
  }

  return value
}

const parseUser = (value: unknown): GitHubUser => {
  const record = asRecord(value, 'user')

  return {
    login: readString(record, 'login', 'user'),
    nodeId: readString(record, 'node_id', 'user'),
  }
}

type ApiIssue = Readonly<{
  number: number
  title: string
  issueType?: string
  assignees: Array<GitHubUser>
  pullRequest: boolean
}>

const parseIssue = (value: unknown): ApiIssue => {
  const record = asRecord(value, 'issue')
  const issueType = record.type ? asRecord(record.type, 'issue type') : undefined
  const assignees = record.assignees === undefined ? [] : asArray(record.assignees, 'assignees')

  return {
    number: readNumber(record, 'number', 'issue'),
    title: readString(record, 'title', 'issue'),
    issueType: issueType ? readString(issueType, 'name', 'issue type') : undefined,
    assignees: assignees.map(parseUser),
    pullRequest: record.pull_request !== undefined,
  }
}

export class GitHubClient {
  readonly #headers: Headers

  constructor(token: string) {
    this.#headers = new Headers({
      Accept: 'application/vnd.github+json',
      Authorization: `Bearer ${token}`,
      'User-Agent': 'atls-daily-standup-meetings',
      'X-GitHub-Api-Version': API_VERSION,
    })
  }

  async teamMembers(owner: string, teamSlug: string): Promise<Array<GitHubUser>> {
    const url = this.endpoint('orgs', owner, 'teams', teamSlug, 'members')
    url.searchParams.set('role', 'all')
    url.searchParams.set('per_page', MAX_PAGE_SIZE)

    return this.getAll(url, parseUser)
  }

  async assignableUsers(owner: string, repo: string): Promise<Array<GitHubUser>> {
    const url = this.endpoint('repos', owner, repo, 'assignees')
    url.searchParams.set('per_page', MAX_PAGE_SIZE)

    return this.getAll(url, parseUser)
  }

  async issueType(owner: string, repo: string, expected: string): Promise<string> {
    const values = asArray(
      await this.getJson(this.endpoint('repos', owner, repo, 'issue-types')),
      'issue types'
    )
    const issueTypes = values.map((value) => {
      const record = asRecord(value, 'issue type')

      return readString(record, 'name', 'issue type')
    })
    const issueType = issueTypes.find(
      (value) => value.trim().toLowerCase() === expected.trim().toLowerCase()
    )

    if (!issueType) {
      throw new Error(`repository issue type \`${expected}\` was not found`)
    }

    return issueType
  }

  async latestOpenIssue(
    owner: string,
    repo: string,
    issueType: string
  ): Promise<OpenIssue | undefined> {
    const url = this.endpoint('repos', owner, repo, 'issues')
    url.searchParams.set('state', 'open')
    url.searchParams.set('type', issueType)
    url.searchParams.set('sort', 'created')
    url.searchParams.set('direction', 'desc')
    url.searchParams.set('per_page', '1')

    const issues = asArray(await this.getJson(url), 'issues').map(parseIssue)
    const issue = issues.find(
      (candidate) =>
        !candidate.pullRequest &&
        candidate.issueType?.trim().toLowerCase() === issueType.trim().toLowerCase()
    )

    return issue ? { number: issue.number, title: issue.title } : undefined
  }

  async createIssue(
    owner: string,
    repo: string,
    title: string,
    body: string,
    issueType: string,
    assignees: Array<string>
  ): Promise<OpenIssue> {
    const created = parseIssue(
      await this.getJson(this.endpoint('repos', owner, repo, 'issues'), {
        body: JSON.stringify({ title, body, assignees, type: issueType }),
        method: 'POST',
      })
    )

    try {
      this.verifyCreatedIssue(created, title, issueType, assignees)
    } catch (error) {
      try {
        await this.closeIssue(owner, repo, created.number)
      } catch (cleanupError) {
        throw new Error(
          `${this.errorMessage(error)}; failed to close invalid issue #${created.number}: ${this.errorMessage(cleanupError)}`
        )
      }

      throw error
    }

    return { number: created.number, title: created.title }
  }

  async closeIssue(owner: string, repo: string, number: number): Promise<void> {
    await this.request(this.endpoint('repos', owner, repo, 'issues', String(number)), {
      body: JSON.stringify({ state: 'closed' }),
      method: 'PATCH',
    })
  }

  private async getAll<T>(url: URL, parse: (value: unknown) => T): Promise<Array<T>> {
    return this.getAllPage(url, parse, [])
  }

  private async getAllPage<T>(
    url: URL,
    parse: (value: unknown) => T,
    items: Array<T>
  ): Promise<Array<T>> {
    const response = await this.request(url)
    const values = asArray(await this.responseJson(response), 'page')
    const next = this.nextPage(response)

    items.push(...values.map(parse))

    return next ? this.getAllPage(next, parse, items) : items
  }

  private async getJson(url: URL, init?: RequestInit): Promise<unknown> {
    return this.responseJson(await this.request(url, init))
  }

  private async responseJson(response: Response): Promise<unknown> {
    try {
      return await response.json()
    } catch {
      throw new Error('GitHub API response was not valid JSON')
    }
  }

  private async request(url: URL, init: RequestInit = {}): Promise<Response> {
    const response = await fetch(url, {
      ...init,
      headers: this.#headers,
      redirect: 'manual',
    })
    const method = init.method ?? 'GET'

    if (!response.ok) {
      throw new Error(`GitHub API ${method} ${url.pathname} returned HTTP ${response.status}`)
    }

    return response
  }

  private nextPage(response: Response): URL | undefined {
    const link = response.headers.get('link')

    if (!link) {
      return undefined
    }

    for (const entry of link.split(',')) {
      const [target, ...parameters] = entry.trim().split(';')

      if (!parameters.some((parameter) => parameter.trim() === 'rel="next"')) {
        continue
      }

      const value = target.trim().match(/^<(.+)>$/u)?.[1]

      if (!value) {
        throw new Error('GitHub pagination next link was malformed')
      }

      const next = new URL(value)

      if (next.origin !== new URL(API_ROOT).origin) {
        throw new Error('GitHub pagination next link changed API origin')
      }

      return next
    }

    return undefined
  }

  private verifyCreatedIssue(
    created: ApiIssue,
    expectedTitle: string,
    expectedType: string,
    expectedAssignees: Array<string>
  ): void {
    if (created.title !== expectedTitle) {
      throw new Error('GitHub created an issue with an unexpected title')
    }

    if (created.issueType?.trim().toLowerCase() !== expectedType.trim().toLowerCase()) {
      throw new Error('GitHub did not apply the requested issue type')
    }

    const actualAssignees = new Set(created.assignees.map(({ login }) => login.toLowerCase()))

    if (!expectedAssignees.every((login) => actualAssignees.has(login.toLowerCase()))) {
      throw new Error('GitHub did not apply all requested assignees')
    }
  }

  private endpoint(...segments: Array<string>): URL {
    return new URL(segments.map(encodeURIComponent).join('/'), API_ROOT)
  }

  private errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error)
  }
}
