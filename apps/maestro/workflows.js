// Seeds `workflows.yaml` through the sandbox host's API (HOST_PORT and HOST_TOKEN, passed with `-e`): a workflow whose
// trigger leads to an approval, and a run of it waiting there. `CLEANUP` deletes the workflow when the flow completes.
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
  api('startWorkflowRun', { id: workflow.id, requestId: 'e2e-' + Date.now() })
  output.workflow = { id: workflow.id, title }
}
