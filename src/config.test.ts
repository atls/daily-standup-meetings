import assert          from 'node:assert/strict'
import test            from 'node:test'

import { formatTitle } from './config.js'
import { loadConfig }  from './config.js'

const requiredEnvironment = (): Record<string, string> => ({
  GITHUB_REPOSITORY: 'example/service',
  GITHUB_WORKSPACE: '/workspace',
  'INPUT_GITHUB-TOKEN': 'token',
  'INPUT_ISSUE-TYPE': 'DSM',
  'INPUT_TEAM-SLUGS': 'engineering',
  INPUT_TIMEZONE: 'UTC',
})

test('loads the Action contract', () => {
  const environment = requiredEnvironment()
  environment['INPUT_TEAM-SLUGS'] = 'platform\nproduct,Platform\noperations'
  environment['INPUT_ISSUE-TYPE'] = 'Standup'
  environment['INPUT_TEMPLATE-PATH'] = 'templates/standup.md'
  environment.INPUT_TIMEZONE = 'Europe/Moscow'

  const config = loadConfig(environment)

  assert.deepEqual(config.teamSlugs, ['platform', 'product', 'operations'])
  assert.equal(config.issueType, 'Standup')
  assert.equal(config.templatePath, '/workspace/templates/standup.md')
  assert.equal(
    formatTitle(config.timezone, new Date('2026-09-10T21:30:00Z')),
    '[DSM] Fri Sep 11 2026'
  )
})

test('rejects invalid Action inputs', () => {
  for (const [name, value, expected] of [
    ['INPUT_TEAM-SLUGS', ' , \n', 'INPUT_TEAM-SLUGS must contain at least one team slug'],
    ['INPUT_ISSUE-TYPE', '  ', 'INPUT_ISSUE-TYPE is required'],
    ['INPUT_TIMEZONE', 'Mars/Olympus_Mons', 'invalid timezone `Mars/Olympus_Mons`'],
  ]) {
    const environment = requiredEnvironment()
    environment[name] = value

    assert.throws(() => loadConfig(environment), new Error(expected))
  }
})
