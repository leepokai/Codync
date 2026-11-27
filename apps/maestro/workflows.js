// Seeds `workflows.yaml` through the sandbox host's API (HOST_PORT and HOST_TOKEN, passed with `-e`): a workflow whose
// trigger leads to an approval, and a run of it waiting there. `SOLO`, once that run is answered, runs the approval on
// its own ("Run this step" on the desktop) and gives the link its push opens on Android (`SCOPE`: the phone's account,
// the SHA-256 hex of its user id as `LocalStore.referenceId` makes it, `local` when signed out). `CLEANUP` deletes the
// workflow when the flow completes.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

if (typeof CLEANUP !== 'undefined') {
  if (output.workflow) api('deleteWorkflow', { id: output.workflow.id })
} else if (typeof SOLO !== 'undefined') {
  const step = api('runWorkflowStep', { id: output.workflow.id, nodeId: 'a', requestId: 'e2e-solo-' + Date.now() }).step
  const computer = api('hello', {}).computerId
  const scope = typeof SCOPE !== 'undefined' ? SCOPE : 'local'
  output.soloLink = `codync-dev://bot/workflow?scope=${scope}&computer=${computer}&workflow=${output.workflow.id}&step=${step.id}`
} else {
  const title = 'Ship e2e ' + Date.now()
  const workflow = api('createWorkflow', { title }).workflow
  const graph = {
    nodes: [
      { id: 't', kind: 'trigger', at: { x: 80, y: 80 }, fields: { trigger: 'manual' } },
      { id: 'a', kind: 'approval', at: { x: 480, y: 80 }, fields: { question: 'Ship it?' } },
    ],
    edges: [{ id: 't>a:text', from: 't', to: 'a', type: 'text' }],
  }
  api('saveWorkflowGraph', { id: workflow.id, baseRevision: workflow.revision, graph })
  // An input, so the trigger has text for the approval when it later runs on its own.
  api('startWorkflowRun', { id: workflow.id, requestId: 'e2e-' + Date.now(), input: 'v1' })
  output.workflow = { id: workflow.id, title }
}
