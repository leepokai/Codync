import { ipcMain, type WebContents } from 'electron'
import type { FindResult } from '../shared/ipc'

/** Find in the conversation (⌘F): Electron's find in page on the window that asked, the matches painted in the page itself. */
const MAX_QUERY = 500
const reporting = new WeakSet<WebContents>()

function report(contents: WebContents) {
  if (reporting.has(contents)) return
  reporting.add(contents)
  contents.on('found-in-page', (_e, result) => {
    // Only the last report of a search carries the final count.
    if (result.finalUpdate) contents.send('find:result', { active: result.activeMatchOrdinal, matches: result.matches } satisfies FindResult)
  })
}

export function registerFind() {
  ipcMain.on('find:search', (e, text: unknown, forward: unknown, step: unknown) => {
    if (typeof text !== 'string') return
    report(e.sender)
    if (!text) {
      e.sender.stopFindInPage('clearSelection')
      e.sender.send('find:result', { active: 0, matches: 0 } satisfies FindResult)
      return
    }
    // Electron's `findNext` marks the request that begins a search; the ones that step through its matches say false.
    e.sender.findInPage(text.slice(0, MAX_QUERY), { forward: forward !== false, findNext: step !== true })
  })
  ipcMain.on('find:stop', (e) => e.sender.stopFindInPage('clearSelection'))
}
