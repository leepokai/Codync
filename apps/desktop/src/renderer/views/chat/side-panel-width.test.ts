import assert from 'node:assert/strict'
import { test } from 'node:test'
import { fitPanel, fitsBeside, PANEL_MIN } from './side-panel-width.ts'

test('the panel keeps to its range beside a view wide enough for both', () => {
  assert.equal(fitPanel(400, 1000, 340), 400)
  assert.equal(fitPanel(100, 1000, 340), PANEL_MIN)
  assert.equal(fitPanel(Infinity, 1000, 340), 660)
  assert.equal(fitsBeside(1000, 340), true)
})

test('a view too narrow for both is covered by the panel, never squeezing it under its least', () => {
  assert.equal(fitsBeside(600, 340), false)
  assert.equal(fitsBeside(PANEL_MIN + 340, 340), false)
  assert.equal(fitPanel(400, 600, 340), PANEL_MIN)
  assert.equal(fitPanel(Infinity, 600, 340), PANEL_MIN)
})
