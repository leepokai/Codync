import assert from 'node:assert/strict'
import { test } from 'node:test'
import { closeView, DEFAULT_PANEL, panelLayout, previewAddress, showChooser, showView, toggleSplit, type PanelLayout } from './panel-layout.ts'

const one: PanelLayout = [{ tabs: ['details', 'memory', 'settings'], shown: 'memory' }]

test('a view has one tab: shown where it is, opened where asked, and gone from the other pane', () => {
  assert.deepEqual(showView(one, 'settings'), [{ tabs: ['details', 'memory', 'settings'], shown: 'settings' }])
  assert.deepEqual(showView(one, 'browser'), [{ tabs: ['details', 'memory', 'settings', 'browser'], shown: 'browser' }], 'a new tab goes last')
  const two = toggleSplit(one)
  assert.deepEqual(two[1], { tabs: [], shown: null }, 'the new pane starts on its chooser')
  const moved = showView(two, 'memory', 1)
  assert.deepEqual(moved, [{ tabs: ['details', 'settings'], shown: 'settings' }, { tabs: ['memory'], shown: 'memory' }], 'the tab beside it takes its place above')
  assert.deepEqual(showView(moved, 'memory'), moved, 'asked for without a pane, it shows where its tab is')
  assert.deepEqual(toggleSplit(moved), [{ tabs: ['details', 'settings', 'memory'], shown: 'settings' }], 'joined, one pane holds every tab')
})

test('closing the tab shown shows its neighbor, then the chooser', () => {
  assert.deepEqual(closeView(one, 0, 'details'), [{ tabs: ['memory', 'settings'], shown: 'memory' }])
  assert.deepEqual(closeView(one, 0, 'memory'), [{ tabs: ['details', 'settings'], shown: 'settings' }])
  const last = closeView(closeView(one, 0, 'memory'), 0, 'settings')
  assert.deepEqual(last, [{ tabs: ['details'], shown: 'details' }])
  assert.deepEqual(closeView(last, 0, 'details'), [{ tabs: [], shown: null }])
  assert.deepEqual(showChooser(one, 0), [{ tabs: ['details', 'memory', 'settings'], shown: null }])
})

test('a stored layout is drawn only as far as it makes sense', () => {
  const views = ['details', 'memory', 'settings']
  assert.deepEqual(panelLayout(null, views), [{ tabs: views, shown: 'details' }], 'nothing stored: every view this computer has')
  assert.deepEqual(panelLayout([{ tabs: ['memory', 'library', 'memory'], shown: 'library' }, { tabs: ['memory', 'details'], shown: 'details' }, { tabs: ['settings'] }], views),
    [{ tabs: ['memory'], shown: null }, { tabs: ['details'], shown: 'details' }], 'unknown and repeated views go, and a third pane')
  assert.deepEqual(panelLayout([{ tabs: 'x' }], views), [{ tabs: [], shown: null }])
  assert.equal(panelLayout(DEFAULT_PANEL, views)[0]?.tabs.includes('library'), false)
})

test('the browser shows web pages only, and never the app in itself', () => {
  const own = 'http://localhost:5173'
  assert.equal(previewAddress('localhost:3000', own), 'http://localhost:3000/')
  assert.equal(previewAddress(' example.com/docs ', own), 'https://example.com/docs')
  assert.equal(previewAddress('http://192.168.1.4:8080/x?y=1', own), 'http://192.168.1.4:8080/x?y=1')
  for (const refused of ['', 'file:///etc/passwd', 'javascript://x', 'codync-artifact://a/b', 'localhost:5173/index.html', 'http://localhost:5173', 'not a page'])
    assert.equal(previewAddress(refused, own), null, refused)
})
