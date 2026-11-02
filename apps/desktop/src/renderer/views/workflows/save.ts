// One saveWorkflowGraph at a time. A change that arrives while one is in flight waits,
// then leaves with the revision that save returned. A conflict drops the local edit.

import type { Graph, WorkflowDetail } from './workflow.ts'

export interface SaveOk {
  ok: true
  revision: number
  workflow: WorkflowDetail
}

export interface SaveConflict {
  ok: false
  conflict: true
  workflow: WorkflowDetail
}

export type SaveResponse = SaveOk | SaveConflict

export type FlushResult =
  | { status: 'saved'; revision: number; workflow: WorkflowDetail }
  | { status: 'conflict'; workflow: WorkflowDetail }
  | { status: 'failed'; error: unknown }
  | { status: 'queued' }
  | { status: 'idle' }

export class GraphSaver {
  revision: number
  dirty = false
  private flying = false
  private queued: Graph | null = null
  private send: (baseRevision: number, graph: Graph) => Promise<SaveResponse>

  constructor(revision: number, send: (baseRevision: number, graph: Graph) => Promise<SaveResponse>) {
    this.revision = revision
    this.send = send
  }

  /** Remember a newer graph without starting a second request. */
  mark(graph: Graph) {
    this.dirty = true
    if (this.flying) this.queued = graph
  }

  get pending() {
    return this.flying || this.queued !== null
  }

  async flush(latest: Graph): Promise<FlushResult> {
    if (this.flying) {
      this.queued = latest
      this.dirty = true
      return { status: 'queued' }
    }
    if (!this.dirty && this.queued === null) return { status: 'idle' }
    this.flying = true
    let graph = this.queued ?? latest
    this.queued = null
    for (;;) {
      let response: SaveResponse
      try {
        response = await this.send(this.revision, graph)
      } catch (error) {
        const next = this.queued
        this.queued = null
        this.flying = false
        this.dirty = true
        if (next) return this.flush(next)
        return { status: 'failed', error }
      }
      if (!response.ok) {
        this.queued = null
        this.dirty = false
        this.flying = false
        this.revision = response.workflow.revision
        return { status: 'conflict', workflow: response.workflow }
      }
      this.revision = response.revision
      if (!this.queued) {
        this.dirty = false
        this.flying = false
        return { status: 'saved', revision: response.revision, workflow: response.workflow }
      }
      graph = this.queued
      this.queued = null
    }
  }
}
