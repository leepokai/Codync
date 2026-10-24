import { mkdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { app, ipcMain, protocol, shell, type WebContents } from 'electron'

/**
 * Artifacts (a bot's HTML documents, `docs/features/artifacts.md`) shown in the chat window. Each loads from
 * `codync-artifact://<id>`: an origin of its own, apart from the app's, whose scripts run (the window's content
 * security policy would block them in a `srcdoc` frame). The renderer hands over the HTML it read from the host.
 */
const SCHEME = 'codync-artifact'
const KEPT = 50
const shown = new Map<string, Uint8Array>()

/** Before the app is ready. */
export function registerArtifactScheme() {
  protocol.registerSchemesAsPrivileged([{ scheme: SCHEME, privileges: { standard: true, secure: true } }])
}

export function registerArtifacts() {
  protocol.handle(SCHEME, (request) => {
    const html = shown.get(new URL(request.url).hostname)
    if (html === undefined) return new Response('', { status: 404 })
    return new Response(html as Uint8Array<ArrayBuffer>, { headers: { 'content-type': 'text/html; charset=utf-8' } })
  })
  ipcMain.handle('artifacts:show', (_e, id: string, html: Uint8Array) => {
    check(id, html)
    shown.delete(id)
    shown.set(id, html)
    // The least recently shown go first.
    for (const old of shown.keys()) if (shown.size > KEPT) shown.delete(old)
  })
  ipcMain.handle('artifacts:openInBrowser', async (_e, id: string, html: Uint8Array) => {
    check(id, html)
    const folder = join(app.getPath('temp'), 'codync-artifacts')
    await mkdir(folder, { recursive: true })
    const path = join(folder, `${id}.html`)
    await writeFile(path, html)
    const failed = await shell.openPath(path)
    if (failed) throw new Error(failed)
  })
}

function check(id: string, html: Uint8Array) {
  if (!/^[0-9a-f-]{36}$/.test(id) || !(html instanceof Uint8Array)) throw new Error('Invalid artifact')
}

/** A link followed in an artifact opens in the browser; the artifact stays (frames inside it go where they go). */
export function keepArtifactsInFrame(contents: WebContents) {
  contents.on('will-frame-navigate', (event) => {
    if (!event.frame?.url.startsWith(`${SCHEME}:`) || event.url.startsWith(`${SCHEME}:`)) return
    event.preventDefault()
    openWebLink(event.url)
  })
}

/** A web or mail link opens in the system's app for it; any other scheme (an app's, a file share's) goes nowhere. */
export function openWebLink(url: string) {
  if (/^(https?|mailto):/.test(url)) void shell.openExternal(url)
}
