import type { GitHubUser } from './github.js'

import assert              from 'node:assert/strict'
import test                from 'node:test'

import { buildBody }       from './dsm.js'
import { selectAssignees } from './dsm.js'

const user = (index: number): GitHubUser => ({
  login: `member-${index}`,
  nodeId: `node-${index}`,
})

test('selects assignable members in team order with the GitHub limit', () => {
  const members = Array.from({ length: 12 }, (_, index) => user(index))
  const assignable = Array.from({ length: 11 }, (_, index) => user(11 - index))

  assert.deepEqual(selectAssignees(members, assignable), [
    'member-1',
    'member-2',
    'member-3',
    'member-4',
    'member-5',
    'member-6',
    'member-7',
    'member-8',
    'member-9',
    'member-10',
  ])
})

test('appends all team members to the consumer template', () => {
  assert.equal(
    buildBody('Template', [user(0), user(1), user(2)]),
    'Template\n<details>\n@member-0 @member-1 @member-2\n</details>'
  )
})
