// Makes the group `mentions.yaml` opens, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with
// `-e`). `CLEANUP`, when the flow completes, deletes it and its members again.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

if (typeof CLEANUP !== 'undefined') {
  for (const id of output.made || []) api('deleteBot', { botId: id })
} else {
  const run = Math.random().toString(36).slice(2, 6)
  const member = (name, description) =>
    api('createBot', { name, description, backend: 'custom', command: 'unused', connectors: [], permission: 'auto' }).bot
  const alpha = member(`Mention Alpha ${run}`, 'Reviews the code')
  const beta = member(`Mention Beta ${run}`, 'Writes the docs')
  const group = api('createBot', { name: `Mention Group ${run}`, kind: 'group', members: [alpha.id, beta.id] }).bot
  output.made = [group.id, alpha.id, beta.id]
  output.group = group.name
  output.alpha = alpha.name
  output.beta = beta.name
}
