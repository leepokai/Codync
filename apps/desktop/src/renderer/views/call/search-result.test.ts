import assert from 'node:assert/strict'
import { test } from 'node:test'
import { SearchTurn, searchLink } from './search-result.ts'

test('grounding can precede transcription and never carries suggestions into a later plain answer', () => {
  const turn = new SearchTurn()
  assert.equal(turn.receive({ turnComplete: true }), null)
  const html = '<p>Original suggestions</p>'
  assert.deepEqual(turn.receive({ groundingMetadata: { searchEntryPoint: { renderedContent: html } } }), { html, text: '' })
  assert.deepEqual(turn.receive({ outputTranscription: { text: 'One' } }), { html, text: 'One' })
  assert.deepEqual(turn.receive({ outputTranscription: { text: ' answer.' }, turnComplete: true }), { html, text: 'One answer.' })
  assert.equal(turn.receive({ outputTranscription: { text: 'No search in this turn.' } }), null)
})

test('search links can only open explicit HTTP or HTTPS destinations', () => {
  for (const value of ['javascript:alert(1)', 'data:text/html,x', 'file:///etc/passwd', '/relative', 'codync://pair', '']) assert.equal(searchLink(value), null)
  assert.equal(searchLink('https://www.google.com/search?q=weather'), 'https://www.google.com/search?q=weather')
  assert.equal(searchLink('http://example.test/'), 'http://example.test/')
})
