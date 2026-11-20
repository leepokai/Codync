// Makes the bot `follow-ups.yaml` opens, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with
// `-e`), and sends it one message. Its agent is host/tests/fixtures/routine_agent.py (ROUTINE_AGENT, its full path),
// which sets a 30-minute follow-up with the note the message carries. `CLEANUP`, when the flow completes, deletes the
// bot, and its follow-up with it.
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
  const name = `Follow-up check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${ROUTINE_AGENT}'`, connectors: [], skills: [], notify: false,
  }).bot
  api('send', { botId: bot.id, text: 'FOLLOW_UP_HERE[Look at the build]' })
  output.bot = { name, id: bot.id }
}
