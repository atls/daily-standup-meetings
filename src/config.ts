import { isAbsolute } from 'node:path'
import { resolve }    from 'node:path'

const DEFAULT_TIMEZONE = 'UTC'

export type ActionConfig = Readonly<{
  githubToken: string
  owner: string
  repo: string
  teamSlugs: Array<string>
  issueType: string
  templatePath?: string
  timezone: string
}>

type Environment = Readonly<Record<string, string | undefined>>

const required = (environment: Environment, name: string): string => {
  const value = environment[name]

  if (!value || value.trim().length === 0) {
    throw new Error(`${name} is required`)
  }

  return value
}

const parseTeamSlugs = (value: string): Array<string> => {
  const seen = new Set<string>()
  const teamSlugs = value
    .split(/[,\n]/u)
    .map((slug) => slug.trim())
    .filter((slug) => slug.length > 0)
    .filter((slug) => {
      const normalized = slug.toLowerCase()

      if (seen.has(normalized)) {
        return false
      }

      seen.add(normalized)

      return true
    })

  if (teamSlugs.length === 0) {
    throw new Error('INPUT_TEAM-SLUGS must contain at least one team slug')
  }

  return teamSlugs
}

const parseRepository = (value: string): Pick<ActionConfig, 'owner' | 'repo'> => {
  const [owner, repo, extra] = value.split('/')

  if (!owner || !repo || extra) {
    throw new Error('GITHUB_REPOSITORY must use the owner/repository format')
  }

  return { owner, repo }
}

const validateTimezone = (timezone: string): string => {
  try {
    new Intl.DateTimeFormat('en-US', { timeZone: timezone }).format()
  } catch {
    throw new Error(`invalid timezone \`${timezone}\``)
  }

  return timezone
}

const resolveTemplatePath = (
  templateInput: string | undefined,
  workspace: string
): string | undefined => {
  if (!templateInput) {
    return undefined
  }

  return isAbsolute(templateInput) ? templateInput : resolve(workspace, templateInput)
}

export const loadConfig = (environment: Environment = process.env): ActionConfig => {
  const repository = parseRepository(required(environment, 'GITHUB_REPOSITORY'))
  const issueType = required(environment, 'INPUT_ISSUE-TYPE').trim()
  const timezone = validateTimezone((environment.INPUT_TIMEZONE ?? DEFAULT_TIMEZONE).trim())
  const templateInput = environment['INPUT_TEMPLATE-PATH']?.trim()
  const workspace = environment.GITHUB_WORKSPACE ?? process.cwd()

  return {
    githubToken: required(environment, 'INPUT_GITHUB-TOKEN'),
    ...repository,
    teamSlugs: parseTeamSlugs(required(environment, 'INPUT_TEAM-SLUGS')),
    issueType,
    templatePath: resolveTemplatePath(templateInput, workspace),
    timezone,
  }
}

export const formatTitle = (timezone: string, now: Date = new Date()): string => {
  const parts = new Intl.DateTimeFormat('en-US', {
    day: '2-digit',
    month: 'short',
    timeZone: timezone,
    weekday: 'short',
    year: 'numeric',
  })
    .formatToParts(now)
    .reduce<Record<string, string>>((result, part) => {
      result[part.type] = part.value

      return result
    }, {})

  return `[DSM] ${parts.weekday} ${parts.month} ${parts.day} ${parts.year}`
}
