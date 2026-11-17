import { ipcMain, type Input, type WebContents } from 'electron'

const isMac = process.platform === 'darwin'

/** Windows whose remote screen keyboard has focus: their shortcuts go to the other computer, not the menu. */
const capturing = new WeakSet<WebContents>()
const watched = new WeakSet<WebContents>()

/** Paste stays with the menu: its paste event carries the local clipboard over to the computer first. */
const isPaste = (input: Input) => input.key.toLowerCase() === 'v' && (isMac ? input.meta : input.control) && !input.alt

export function registerRemoteKeyboard() {
  ipcMain.on('app:remoteKeyboard', (e, on: unknown) => {
    const contents = e.sender
    const release = () => {
      capturing.delete(contents)
      contents.setIgnoreMenuShortcuts(false)
    }
    if (on === true) capturing.add(contents)
    else release()
    if (watched.has(contents)) return
    watched.add(contents)
    // A page that crashed or reloaded with the field focused never says it let go: its shortcuts come back.
    contents.on('render-process-gone', release)
    contents.on('did-start-navigation', (details) => {
      if (details.isMainFrame && !details.isSameDocument) release()
    })
    contents.on('before-input-event', (_event, input) => {
      if (input.type === 'keyDown') contents.setIgnoreMenuShortcuts(capturing.has(contents) && !isPaste(input))
    })
  })
}
