import { readdir, readFile, stat } from 'node:fs/promises'
import path from 'node:path'
import { BrowserWindow, dialog, ipcMain } from 'electron'
import { isCandidate, isSecretFile, LIMITS } from '../shared/agent-import'
import type { AgentFolder } from '../shared/ipc'

/** Folders that only hold other agents' runtime data. */
const SKIP = new Set(['node_modules', '.git', 'sessions', 'logs', 'cache', 'tmp'])
/** An agent's home keeps its files in `workspace/` (or `workspace-<id>/`); the home's own `skills/` sits beside it. */
const WORKSPACE = /^workspace(?:-[^/]+)?\//
const MAX_DEPTH = 6

/**
 * Reads the few files of an agent's folder that matter (see `isCandidate`), within limits, and names the settings and
 * secret files without reading them. Symbolic links are never followed: a link out of the folder must not become a
 * file read.
 */
export async function readAgentFolder(root: string): Promise<AgentFolder> {
  const files: Record<string, string> = {}
  const secretPaths: string[] = []
  let total = 0
  let count = 0
  let truncated = false
  const found: { rel: string; full: string }[] = []

  async function walk(dir: string, rel: string, depth: number) {
    if (depth > MAX_DEPTH) return
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const next = rel ? `${rel}/${entry.name}` : entry.name
      if (entry.isSymbolicLink()) continue
      if (entry.isDirectory()) {
        if (!SKIP.has(entry.name)) await walk(path.join(dir, entry.name), next, depth + 1)
      } else if (entry.isFile()) {
        found.push({ rel: next, full: path.join(dir, entry.name) })
      }
    }
  }
  await walk(root, '', 0)

  // Workspace files come last so they win over the home's own files of the same name.
  found.sort((a, b) => Number(WORKSPACE.test(a.rel)) - Number(WORKSPACE.test(b.rel)) || a.rel.localeCompare(b.rel))
  for (const { rel, full } of found) {
    const logical = rel.replace(WORKSPACE, '')
    if (isSecretFile(logical)) {
      secretPaths.push(logical)
      continue
    }
    if (!isCandidate(logical)) continue
    if (count >= LIMITS.files) { truncated = true; break }
    const size = (await stat(full)).size
    if (size > LIMITS.file || total + size > LIMITS.total) { truncated = true; continue }
    files[logical] = await readFile(full, 'utf8')
    total += size
    count += 1
  }
  return { root, files, secretPaths, truncated }
}

/** `agentImport:pick`: asks for a folder and reads it. Null when the person cancels. */
export function registerAgentImport() {
  ipcMain.handle('agentImport:pick', async (event): Promise<AgentFolder | null> => {
    const window = BrowserWindow.fromWebContents(event.sender)
    const options = { properties: ['openDirectory' as const], buttonLabel: 'Import' }
    const result = window ? await dialog.showOpenDialog(window, options) : await dialog.showOpenDialog(options)
    const root = result.filePaths[0]
    if (result.canceled || !root) return null
    return readAgentFolder(root)
  })
}
