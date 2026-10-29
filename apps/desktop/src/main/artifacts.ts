import { mkdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { app, ipcMain, protocol, shell, webFrameMain, type WebContents, type WebFrameMain } from 'electron'
import { ARTIFACT_SCHEME, followsUser, isArtifactURL, isArtifactViewer } from '../shared/artifacts'

/**
 * Artifacts (a bot's HTML documents, `docs/features/artifacts.md`) shown in the chat window. Each loads from
 * `codync-artifact://<id>`: an origin of its own, apart from the app's, whose scripts run (the window's content
 * security policy would block them in a `srcdoc` frame). The renderer hands over the HTML it read from the host.
 */
const KEPT = 50
const shown = new Map<string, Uint8Array>()

/** Before the app is ready. */
export function registerArtifactScheme() {
  protocol.registerSchemesAsPrivileged([{ scheme: ARTIFACT_SCHEME, privileges: { standard: true, secure: true } }])
}

export function registerArtifacts() {
  protocol.handle(ARTIFACT_SCHEME, (request) => {
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

/**
 * An artifact stays in its frame (frames inside it go where they go). What it opens goes to the browser only when the
 * user just clicked or pressed Return/Space in the window, after its viewer loaded, one link per press: a card opens
 * nothing, and neither does a script sending the artifact or a new window elsewhere on its own. The window's input
 * comes from here, not the page, so an artifact can't fake it. Electron reports no touchscreen tap in an artifact's
 * frame here, so a tapped link opens nothing.
 */
export function keepArtifactsInFrame(contents: WebContents) {
  let input = 0
  let loaded = 0
  contents.on('before-mouse-event', (_e, mouse) => {
    if (mouse.type === 'mouseUp') input = Date.now()
  })
  contents.on('before-input-event', (_e, key) => {
    if (key.type === 'keyDown' && (key.key === 'Enter' || key.key === ' ')) input = Date.now()
  })
  // At commit, before the viewer's scripts run: the click that opened it doesn't count for what they open.
  contents.on('did-frame-navigate', (_e, _url, _code, _status, _isMainFrame, processId, routingId) => {
    if (isViewer(webFrameMain.fromId(processId, routingId))) loaded = Date.now()
  })
  const follow = (url: string) => {
    if (!followsUser(input, loaded, Date.now())) return
    input = 0
    openWebLink(url)
  }
  contents.on('will-frame-navigate', (event) => {
    const { frame } = event
    if (!isArtifactURL(frame?.url)) return
    if (isArtifactURL(event.url)) {
      // A card can't make itself a viewer.
      if (!isArtifactViewer(frame.url) && isArtifactViewer(event.url)) event.preventDefault()
      return
    }
    event.preventDefault()
    if (isViewer(frame)) follow(event.url)
  })
  // A new window: an artifact's target=_blank link or window.open (the app opens its own links through `app:openExternal`).
  contents.setWindowOpenHandler(({ url }) => {
    follow(url)
    return { action: 'deny' }
  })
}

/** The viewer: an artifact frame in the chat window's own page (one inside an artifact never is), not a card's. */
const isViewer = (frame: WebFrameMain | null | undefined) => frame?.parent?.parent === null && isArtifactViewer(frame.url)

/** A web or mail link opens in the system's app for it; any other scheme (an app's, a file share's) goes nowhere. */
function openWebLink(url: string) {
  if (/^(https?|mailto):/.test(url)) void shell.openExternal(url)
}
