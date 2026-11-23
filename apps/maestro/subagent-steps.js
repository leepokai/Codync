// Makes the chat `subagent-steps.yaml` reads, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed
// with `-e`). Its agent is host/tests/fixtures/subagent_agent.py (SUBAGENT_AGENT, its full path), which runs a
// foreground and a background subagent as Claude's adapter does, then replies. `CLEANUP` deletes the bot again.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

if (typeof CLEANUP !== 'undefined') {
  if (output.bot) api('deleteBot', { botId: output.bot.id })
} else {
  const name = `Subagents ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${SUBAGENT_AGENT}'`, connectors: [], skills: [], notify: false,
  }).bot
  output.bot = { name, id: bot.id }
  api('send', { botId: bot.id, text: 'go' })
}
