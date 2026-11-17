import assert from 'node:assert/strict'
import { test } from 'node:test'
import { SearchTurn, searchLink } from './search-result.ts'

const audio = { modelTurn: { parts: [] } }

test('grounding can precede transcription and never carries suggestions into a later plain answer', () => {
  const turn = new SearchTurn()
  assert.equal(turn.receive({ turnComplete: true }), null)
  const html = '<p>Original suggestions</p>'
  assert.deepEqual(turn.receive({ ...audio, groundingMetadata: { searchEntryPoint: { renderedContent: html } } }), { html, text: '' })
  assert.deepEqual(turn.receive({ outputTranscription: { text: 'One' } }), { html, text: 'One' })
  assert.deepEqual(turn.receive({ outputTranscription: { text: ' answer.' }, turnComplete: true }), { html, text: 'One answer.' })
  assert.equal(turn.receive({ ...audio, outputTranscription: { text: 'No search in this turn.' } }), null)
})

test('transcription that arrives after turnComplete stays with its turn and keeps the suggestions', () => {
  const turn = new SearchTurn()
  const html = '<p>Original suggestions</p>'
  turn.receive({ ...audio, groundingMetadata: { searchEntryPoint: { renderedContent: html } }, turnComplete: true })
  assert.deepEqual(turn.receive({ outputTranscription: { text: 'Late piece.' } }), { html, text: 'Late piece.' })
  assert.equal(turn.receive({ ...audio, outputTranscription: { text: 'Next turn.' } }), null)
})

test('the user being heard starts the next turn, so its transcription before its audio stays with it', () => {
  const turn = new SearchTurn()
  const first = '<p>First suggestions</p>'
  const second = '<p>Second suggestions</p>'
  turn.receive({ ...audio, groundingMetadata: { searchEntryPoint: { renderedContent: first } }, outputTranscription: { text: 'A.' }, turnComplete: true })
  assert.deepEqual(turn.receive({ inputTranscription: { text: 'And tomorrow?' } }), null)
  assert.equal(turn.receive({ outputTranscription: { text: 'Tomorrow ' } }), null)
  turn.receive(audio)
  assert.deepEqual(turn.receive({ groundingMetadata: { searchEntryPoint: { renderedContent: second } }, outputTranscription: { text: 'is sunny.' } }),
    { html: second, text: 'Tomorrow is sunny.' })
  assert.equal(turn.answers, 2)
})

test('search links can only open explicit HTTP or HTTPS destinations', () => {
  for (const value of ['javascript:alert(1)', 'data:text/html,x', 'file:///etc/passwd', '/relative', 'codync://pair', '']) assert.equal(searchLink(value), null)
  assert.equal(searchLink('https://www.google.com/search?q=weather'), 'https://www.google.com/search?q=weather')
  assert.equal(searchLink('http://example.test/'), 'http://example.test/')
})
