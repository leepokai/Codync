import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Entry } from '@shared/models'
import { requestDetail, requestIcon, requestName, requestParams, settledAs } from './requests.ts'

test('an MCP tool reads as its name in words and its server', () => {
  assert.deepEqual(requestName('memory-read_memory: read_memory'), { name: 'Read memory', source: 'memory', literal: false })
  assert.deepEqual(requestName('codync-mem-search_history: search_history'), { name: 'Search history', source: 'codync-mem', literal: false })
})

test('anything else stays as the agent wrote it', () => {
  assert.deepEqual(requestName('`date`'), { name: 'date', source: null, literal: true })
  assert.deepEqual(requestName('Edit src/app.ts'), { name: 'Edit src/app.ts', source: null, literal: true })
})

test('only a settled approval has an ending, and the answer is part of it', () => {
  const entry = (status: string, selected?: string) =>
    ({ kind: 'permission', data: { status, selected, options: [{ optionId: 'a', kind: 'allow_once' }, { optionId: 'r', kind: 'reject_once' }] } }) as unknown as Entry
  assert.equal(settledAs(entry('pending')), null)
  assert.equal(settledAs(entry('cancelled')), 'cancelled:')
  assert.notEqual(settledAs(entry('answered', 'a')), settledAs(entry('answered', 'r')))
})

test('a request of no kind takes its symbol from its verb', () => {
  assert.equal(requestIcon('execute', '`date`'), 'terminal')
  assert.equal(requestIcon('other', 'memory-search_history: search_history'), 'magnifyingglass')
  assert.equal(requestIcon(undefined, 'connectors-list_connectors: list_connectors'), 'list.bullet')
  assert.equal(requestIcon(undefined, 'anything'), 'wrench.adjustable')
})

test('a request shows its detail without the fence, and nothing for no arguments', () => {
  assert.equal(requestDetail('```json\n{}\n```'), null)
  assert.equal(requestDetail('```json\n{"id": 3}\n```'), '{"id": 3}')
  assert.equal(requestDetail('Not in allowlist: date'), 'Not in allowlist: date')
  assert.equal(requestDetail(undefined), null)
})

test("a request's JSON object reads as name and value rows; anything else stays a block", () => {
  assert.deepEqual(requestParams('{"limit": 20, "query": "foo", "shared": true, "tags": ["a"]}'), [['limit', '20'], ['query', 'foo'], ['shared', 'true'], ['tags', '["a"]']])
  assert.equal(requestParams('{}'), null)
  assert.equal(requestParams('[1, 2]'), null)
  assert.equal(requestParams('ls -la'), null)
  assert.equal(requestParams('{not json'), null)
  assert.equal(requestParams(null), null)
})
