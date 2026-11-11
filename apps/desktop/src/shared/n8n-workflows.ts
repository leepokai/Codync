/** The workflows n8n runs on the computer and their latest runs: what the host's `n8nWorkflows` and `n8nExecutions` answer. */

export interface N8nWorkflow {
  id: string
  name: string
  active: boolean
}

export interface N8nRun {
  id: string
  workflowId: string | null
  status: string
  startedAt: string | null
}

export interface N8nWorkflowList {
  workflows: N8nWorkflow[]
  /** n8n has more than the page the host fetched. */
  more: boolean
}

/** The most workflows the card lists; the rest are in n8n. */
export const SHOWN_WORKFLOWS = 25
/** The runs the card lists. */
export const SHOWN_RUNS = 5

/** What `n8nWorkflows` answered; a row without an id is dropped, an older host's empty reply is an empty list. */
export function decodeWorkflows(raw: unknown): N8nWorkflowList {
  const body = (raw ?? {}) as { workflows?: Array<Partial<N8nWorkflow>>; more?: unknown }
  const workflows = (body.workflows ?? []).flatMap((w) =>
    typeof w.id === 'string' && w.id ? [{ id: w.id, name: typeof w.name === 'string' ? w.name : '', active: w.active === true }] : [],
  )
  return { workflows, more: body.more === true }
}

/** What `n8nExecutions` answered; a row without an id is dropped. */
export function decodeRuns(raw: unknown): N8nRun[] {
  const body = (raw ?? {}) as { executions?: Array<Record<string, unknown>> }
  return (body.executions ?? []).flatMap((e) =>
    typeof e.id === 'string' && e.id
      ? [{
          id: e.id,
          workflowId: typeof e.workflowId === 'string' ? e.workflowId : null,
          status: typeof e.status === 'string' ? e.status : '',
          startedAt: typeof e.startedAt === 'string' ? e.startedAt : null,
        }]
      : [],
  )
}

/** A run's status as a catalog key; the phone's and the terminal's words. */
export function runLabel(status: string): string {
  switch (status) {
    case 'success': return 'Succeeded'
    case 'error': return 'Failed'
    case 'crashed': return 'Crashed'
    case 'running': return 'Running'
    case 'waiting': return 'Waiting'
    case 'canceled': return 'Cancelled'
    case 'new': return 'Waiting in the queue'
    default: return 'Unknown'
  }
}

/** Whether a run ended badly: it reads in the danger color. */
export const runFailed = (status: string) => status === 'error' || status === 'crashed'
