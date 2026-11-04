import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  cleanRules,
  describeEvaluation,
  MAX_PATTERN,
  MAX_RULES,
  moveRule,
  rulesProblem,
  type ApprovalEvaluation,
  type ApprovalRule,
} from './approval-rules.ts'

const rule = (pattern: string, decision: ApprovalRule['decision'] = 'allow'): ApprovalRule => ({ pattern, decision })
const patterns = (rules: ApprovalRule[]) => rules.map((r) => r.pattern)

test('moving a rule changes who decides first, and a bad move changes nothing', () => {
  const rules = [rule('a'), rule('b'), rule('c')]
  assert.deepEqual(patterns(moveRule(rules, 2, 0)), ['c', 'a', 'b'])
  assert.deepEqual(patterns(moveRule(rules, 0, 1)), ['b', 'a', 'c'])
  assert.equal(moveRule(rules, 0, 3), rules)
  assert.equal(moveRule(rules, -1, 0), rules)
  assert.deepEqual(patterns(rules), ['a', 'b', 'c'], 'the original is left alone')
})

test('saving drops blank patterns and trims the rest', () => {
  const cleaned = cleanRules([rule('  git status*  '), rule('   '), { pattern: 'ls', decision: 'ask', kind: '' }])
  assert.deepEqual(cleaned, [
    { pattern: 'git status*', kind: null, decision: 'allow' },
    { pattern: 'ls', kind: null, decision: 'ask' },
  ])
})

test('the limits are the host\'s', () => {
  assert.equal(rulesProblem([rule('x')]), null)
  assert.match(rulesProblem(Array.from({ length: MAX_RULES + 1 }, () => rule('x'))) ?? '', /at most 100/)
  assert.match(rulesProblem([rule('x'.repeat(MAX_PATTERN + 1))]) ?? '', /at most 500/)
})

test('the tester\'s answer is worded from the host\'s decision', () => {
  const base = { chained: false, allowPassedOver: null }
  const forbidden: ApprovalEvaluation = { outcome: 'forbidden', pattern: 'rm -rf /', ...base }
  assert.match(describeEvaluation(forbidden), /Never allowed.*rm -rf \//)
  const allowed: ApprovalEvaluation = { outcome: 'rule', index: 1, decision: 'allow', pattern: 'git *', kind: null, quick: false, ...base }
  assert.equal(describeEvaluation(allowed), 'Allowed by rule 2 (“git *”).')
  assert.equal(describeEvaluation({ ...allowed, pattern: '*', kind: 'execute', quick: true, decision: 'deny' }), 'Denied by the quick rule for shell commands.')
  assert.match(describeEvaluation({ ...allowed, decision: 'deny' }), /^Denied by rule 2/)
  assert.match(describeEvaluation({ ...allowed, decision: 'ask' }), /^Asks you by rule 2/)
  const setting: ApprovalEvaluation = { outcome: 'setting', permission: 'ask', chained: true, allowPassedOver: 0 }
  const text = describeEvaluation(setting)
  assert.match(text, /No rule matches: it asks you/)
  assert.match(text, /Rule 1 would allow it, but it runs several commands/)
  assert.match(describeEvaluation({ ...setting, permission: 'auto', allowPassedOver: null }), /approved automatically/)
})
