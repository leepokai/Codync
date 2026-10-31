// Nodes stay in memory. The system clipboard only receives a UUID under the workflow MIME,
// so a paste from another app cannot resurrect a copy this window no longer holds.

import type { Edge, Node, Stroke } from './workflow.ts'

export const WORKFLOW_CLIPBOARD_MIME = 'application/x-codync-workflow-nodes'

export interface WorkflowClip {
  key: string
  nodes: Node[]
  edges: Edge[]
  strokes: Stroke[]
}

let memory: WorkflowClip | null = null

export function workflowClip() {
  return memory
}

export function rememberClip(nodes: Node[], edges: Edge[], strokes: Stroke[] = []): WorkflowClip {
  const clip = { key: crypto.randomUUID(), nodes, edges, strokes }
  memory = clip
  const item = typeof ClipboardItem === 'undefined' ? null : new ClipboardItem({
    [WORKFLOW_CLIPBOARD_MIME]: new Blob([clip.key], { type: WORKFLOW_CLIPBOARD_MIME }),
  })
  if (item && navigator.clipboard?.write) void navigator.clipboard.write([item]).catch(() => {})
  return clip
}

/** The in-memory copy when the clipboard still holds its key. */
export async function pasteClip(): Promise<WorkflowClip | null> {
  if (!memory) return null
  try {
    const items = await navigator.clipboard.read()
    for (const item of items) {
      if (!item.types.includes(WORKFLOW_CLIPBOARD_MIME)) continue
      const key = await (await item.getType(WORKFLOW_CLIPBOARD_MIME)).text()
      if (key === memory.key) return memory
    }
  } catch {
    return memory
  }
  return null
}
