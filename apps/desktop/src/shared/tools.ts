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
  /** False when this build ships without the portable (Windows ARM64, a quick development package). */
  bundled: boolean
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
}
