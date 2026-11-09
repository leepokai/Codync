import assert from 'node:assert/strict'
import { test } from 'node:test'
import { normalizeBot } from '../../../shared/models.ts'
import { mentionSuggestions, returnMentions, withMention } from './mentions.ts'

const member = (name: string) => normalizeBot({ id: name, name })
const suggested = (draft: string, ...names: string[]) => mentionSuggestions(draft, names.map(member)).map((m) => m.name)

test('a mention starts after a space and stays short and on one line', () => {
  assert.deepEqual(suggested('@', 'Ada', 'Bo'), ['Ada', 'Bo'])
  assert.deepEqual(suggested('hi @AD', 'Ada', 'Bo'), ['Ada'])
  assert.deepEqual(suggested('mail@host', 'host'), [])
  assert.deepEqual(suggested(`@${'x'.repeat(25)}`, 'x'.repeat(30)), [])
  assert.deepEqual(suggested('@Ada\nnext', 'Ada\nnext'), [])
})

test('an @ followed by a space is ordinary text', () => {
  assert.deepEqual(suggested('meet @ home', 'My Home'), [])
  assert.deepEqual(suggested('meet @ ', 'My Home'), [])
})

test('a full name and its space end the list, but a longer name can still be typed', () => {
  assert.deepEqual(suggested('@Alpha ', 'Alpha', 'Alpha Two'), [])
  assert.deepEqual(suggested('@alpha ', 'Alpha', 'Alpha Two'), [])
  assert.deepEqual(suggested('@Review ', 'Review', 'Code Review Bot'), [])
  assert.deepEqual(suggested('@Alpha', 'Alpha', 'Alpha Two'), ['Alpha', 'Alpha Two'])
  assert.deepEqual(suggested('@Alpha T', 'Alpha', 'Alpha Two'), ['Alpha Two'])
  assert.deepEqual(suggested('@Code ', 'Codex', 'Code Review'), ['Code Review'], 'no member is called just Code')
})

test('picking replaces the @partial and leaves room for the message', () => {
  assert.equal(withMention('@', member('Ada')), '@Ada ')
  assert.equal(withMention('ask me@home then @ad', member('Ada')), 'ask me@home then @Ada ')
})

test('Return picks a member, but sends a whole @all or @everyone', () => {
  const allen = member('Allen')
  assert.equal(returnMentions(allen, '@al'), true)
  assert.equal(returnMentions(allen, '@allen'), true)
  for (const draft of ['@all', 'thanks @ALL', '@everyone', '@all ']) assert.equal(returnMentions(allen, draft), false, draft)
})
