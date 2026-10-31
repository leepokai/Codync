// Host workflow methods. The host owns the graph; these are the intents.

import type { HostClient } from '../../client/host-client'
import type { Graph, RunDetail, RunSummary, WorkflowDetail, WorkflowStep, WorkflowSummary, WorkflowVersion } from './workflow.ts'

export const listWorkflows = (client: HostClient) => client.call<{ workflows: WorkflowSummary[] }>('workflows')

export const fetchWorkflow = (client: HostClient, id: string, reveal = false) =>
  client.call<{ workflow: WorkflowDetail }>('workflow', { id, reveal })

export const createWorkflow = (client: HostClient, title: string, graph?: Graph) =>
  client.call<{ workflow: WorkflowDetail }>('createWorkflow', graph ? { title, graph } : { title })

export const renameWorkflow = (client: HostClient, id: string, title: string) =>
  client.call<{ workflow: WorkflowSummary }>('renameWorkflow', { id, title })

export const deleteWorkflow = (client: HostClient, id: string) => client.call('deleteWorkflow', { id })

export const setWorkflowEnabled = (client: HostClient, id: string, enabled: boolean) =>
  client.call<{ workflow: WorkflowSummary }>('setWorkflowEnabled', { id, enabled })

export const saveWorkflowGraph = (client: HostClient, id: string, baseRevision: number, graph: Graph) =>
  client.call<{ ok: true; revision: number; workflow: WorkflowDetail } | { ok: false; conflict: true; workflow: WorkflowDetail }>('saveWorkflowGraph', { id, baseRevision, graph })

export const saveWorkflowScript = (client: HostClient, id: string, baseRevision: number, script: string) =>
  client.call<{ ok: true; revision: number; workflow: WorkflowDetail } | { ok: false; conflict: true; workflow: WorkflowDetail }>('saveWorkflowScript', { id, baseRevision, script })

export const workflowVersions = (client: HostClient, id: string) => client.call<{ versions: WorkflowVersion[] }>('workflowVersions', { id })

export const workflowVersion = (client: HostClient, id: string, versionId: string) =>
  client.call<{ graph: Graph }>('workflowVersion', { id, versionId })

export const restoreWorkflowVersion = (client: HostClient, id: string, versionId: string) =>
  client.call<{ workflow: WorkflowDetail }>('restoreWorkflowVersion', { id, versionId })

export const startWorkflowRun = (client: HostClient, id: string, requestId: string, fromNodeId?: string) =>
  client.call<{ run: RunDetail }>('startWorkflowRun', fromNodeId ? { id, requestId, fromNodeId } : { id, requestId })

export const stopWorkflowRun = (client: HostClient, runId: string) => client.call('stopWorkflowRun', { runId })

export const runWorkflowStep = (client: HostClient, id: string, nodeId: string, requestId: string) =>
  client.call<{ step: WorkflowStep }>('runWorkflowStep', { id, nodeId, requestId })

export const stopWorkflowStep = (client: HostClient, stepId: string) => client.call('stopWorkflowStep', { stepId })

export const retryWorkflowStep = (client: HostClient, stepId: string) => client.call<{ step: WorkflowStep }>('retryWorkflowStep', { stepId })

export const decideWorkflowStep = (client: HostClient, stepId: string, approved: boolean) =>
  client.call('decideWorkflowStep', { stepId, approved })

export const workflowRuns = (client: HostClient, id: string, limit: number, before?: { startedAt: number; id: string }) =>
  client.call<{ runs: RunSummary[]; hasMore: boolean }>('workflowRuns', before ? { id, limit, before } : { id, limit })

export const fetchWorkflowRun = (client: HostClient, runId: string) => client.call<{ run: RunDetail }>('workflowRun', { runId })

export const rotateWorkflowWebhook = (client: HostClient, id: string, nodeId: string) =>
  client.call<{ secret: string }>('rotateWorkflowWebhook', { id, nodeId })
