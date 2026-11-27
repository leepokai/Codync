import assert from 'node:assert/strict'
import { test } from 'node:test'
import { closeView, DEFAULT_PANEL, multiTabs, multiView, panelLayout, previewAddress, showChooser, showView, threadTab, toggleSplit, type PanelLayout } from './panel-layout.ts'

const one: PanelLayout = [{ tabs: ['details', 'memory', 'settings'], shown: 'memory' }]

test('a view has one tab: shown where it is, opened where asked, and gone from the other pane', () => {
  assert.deepEqual(showView(one, 'settings'), [{ tabs: ['details', 'memory', 'settings'], shown: 'settings' }])
  assert.deepEqual(showView(one, 'library'), [{ tabs: ['details', 'memory', 'settings', 'library'], shown: 'library' }], 'a new tab goes last')
  const two = toggleSplit(one)
  assert.deepEqual(two[1], { tabs: [], shown: null }, 'the new pane starts on its chooser')
  const moved = showView(two, 'memory', 1)
  assert.deepEqual(moved, [{ tabs: ['details', 'settings'], shown: 'settings' }, { tabs: ['memory'], shown: 'memory' }], 'the tab beside it takes its place above')
  assert.deepEqual(showView(moved, 'memory'), moved, 'asked for without a pane, it shows where its tab is')
  assert.deepEqual(toggleSplit(moved), [{ tabs: ['details', 'settings', 'memory'], shown: 'settings' }], 'joined, one pane holds every tab')
})

test('a browser or terminal picked opens a new tab each time', () => {
  const first = showView(one, 'terminal')
  const second = showView(first, 'terminal')
  const [a, b] = second[0]!.tabs.slice(3)
  assert.ok(a && b && a !== b, 'two tabs')
  assert.equal(multiView(a), 'terminal')
  assert.equal(second[0]!.shown, b, 'the new one shows')
  assert.deepEqual(showView(second, a), [{ tabs: second[0]!.tabs, shown: a }], 'its tab shows it again')
  assert.equal(multiView('thread:t1'), null)
  assert.deepEqual(panelLayout([{ tabs: ['terminal', a], shown: a }], () => true), [{ tabs: [a], shown: a }], 'a bare view is no tab')
  const views = (tab: string) => ['details', 'browser', 'terminal'].includes(tab)
  assert.deepEqual(toggleSplit(panelLayout(second, views)), [{ tabs: ['details', a, b], shown: b }, { tabs: [], shown: null }], 'kept with their view, split')
  assert.deepEqual(multiTabs(['details', a, 'browser:x1', b]).map((t) => [t.view, t.n]), [['terminal', 1], ['browser', 1], ['terminal', 2]], 'numbered per view')
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
  const known = ['details', 'memory', 'settings']
  const views = (t: string) => known.includes(t)
  assert.deepEqual(panelLayout(null, views), [{ tabs: [], shown: null }], 'nothing stored: one pane on its chooser')
  assert.deepEqual(panelLayout([{ tabs: ['memory', 'library', 'memory'], shown: 'library' }, { tabs: ['memory', 'details'], shown: 'details' }, { tabs: ['settings'] }], views),
    [{ tabs: ['memory'], shown: null }, { tabs: ['details'], shown: 'details' }], 'unknown and repeated views go, and a third pane')
  assert.deepEqual(panelLayout([{ tabs: 'x' }], views), DEFAULT_PANEL)
})

test('a thread opens as a tab, and its tab goes with it', () => {
  const tab = threadTab('t1')
  const opened = showView([{ tabs: ['details'], shown: 'details' }], tab)
  assert.deepEqual(opened, [{ tabs: ['details', tab], shown: tab }])
  assert.deepEqual(panelLayout(opened, (t) => t === 'details' || t === tab), opened, 'its bot keeps it')
  assert.deepEqual(panelLayout(opened, (t) => t === 'details'), [{ tabs: ['details'], shown: 'details' }], 'gone, its pane shows the first tab')
})

test('the browser shows web pages only, and never the app in itself', () => {
  const own = 'http://localhost:5173'
  assert.equal(previewAddress('localhost:3000', own), 'http://localhost:3000/')
  assert.equal(previewAddress(' example.com/docs ', own), 'https://example.com/docs')
  assert.equal(previewAddress('http://192.168.1.4:8080/x?y=1', own), 'http://192.168.1.4:8080/x?y=1')
  for (const refused of ['', 'file:///etc/passwd', 'javascript://x', 'codync-artifact://a/b', 'localhost:5173/index.html', 'http://localhost:5173', 'not a page'])
    assert.equal(previewAddress(refused, own), null, refused)
})
