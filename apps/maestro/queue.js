// Makes the bot `queue.yaml` opens and starts its first turn, on the host the phone is paired with (HOST_PORT and
// HOST_TOKEN, passed with `-e`). Its agent is host/tests/fixtures/steer_agent.py (STEER_AGENT, its full path), whose turn
// waits until a steered message joins it. `SEND` instead starts another turn with that text, `FILL` queues that many
// messages behind it, and `CLEANUP`, when the flow completes, deletes the bot again.
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
} else if (typeof SEND !== 'undefined') {
  api('send', { botId: output.bot.id, text: SEND })
} else if (typeof FILL !== 'undefined') {
  for (let i = 1; i <= Number(FILL); i++) api('send', { botId: output.bot.id, text: `Waiting ${i}` })
} else {
  const name = `Queue check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${STEER_AGENT}' steer 600`, connectors: [], skills: [], notify: false,
  }).bot
  api('send', { botId: bot.id, text: 'first' })
  output.bot = { name, id: bot.id }
}
