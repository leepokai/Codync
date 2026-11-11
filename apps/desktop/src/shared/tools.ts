import { bytesText } from './health.ts'

/** Bundled tools: programs the host runs for the user (see host/src/tools). Only ComfyUI so far; n8n has its own types. */

export type ToolId = 'comfy'

export interface ToolSpec {
  id: ToolId
  /** The renderer page the tool's window shows while the host brings it up (`?window=<page>`). */
  page: string
  title: string
  /** SF Symbol name. */
  icon: string
}

export const TOOLS: Record<ToolId, ToolSpec> = {
  comfy: { id: 'comfy', page: 'comfy', title: 'ComfyUI', icon: 'comfyui.logo' },
}

export type ToolRunState = 'stopped' | 'starting' | 'running' | 'error'

export interface ToolJob {
  id: string
  phase: 'queued' | 'running' | 'done' | 'cancelled' | 'error'
  error: string | null
  files: string[]
}

/** What the host shows on ComfyUI's card. */
export interface ComfyCard {
  version: string
  /** False on a computer ComfyUI doesn't run on yet (it is built for Windows x64). */
  bundled: boolean
  /** Whether its program is on this computer; the first start downloads it. */
  installed: boolean
  /** The download's size in bytes. */
  downloadBytes: number
  dataDir: string
  modelsDir: string
  modelsUrl: string
  usedBytes: number
  /** Set when the models run on the processor. */
  notice: string | null
}

/** The host's `toolStatus` for one tool. */
export interface ToolState {
  id: ToolId
  enabled: boolean
  state: ToolRunState
  error: string | null
  port: number
  url: string | null
  log: string
  logTail: string
  comfy?: ComfyCard
  job?: ToolJob | null
  install?: ToolInstall
}

/** The program's first download (`host/src/tools/comfy_install.rs`). */
export interface ToolInstall {
  phase: 'idle' | 'downloading' | 'unpacking' | 'failed'
  done: number
  total: number
  error: string | null
}

/** Whether the program is arriving right now. */
export function installing(install: ToolInstall | undefined): boolean {
  return install?.phase === 'downloading' || install?.phase === 'unpacking'
}

/** How the download goes, in the caller's language; null when nothing is arriving. */
export function installLine(title: string, install: ToolInstall | undefined, tr: (text: string) => string): string | null {
  if (install?.phase === 'downloading') {
    return tr('Downloading {name}… {done} of {total}')
      .replace('{name}', title)
      .replace('{done}', bytesText(install.done))
      .replace('{total}', bytesText(install.total))
  }
  if (install?.phase === 'unpacking') return tr('Unpacking {name}…').replace('{name}', title)
  return null
}
