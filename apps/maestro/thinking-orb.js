// A working bot and one waiting on an approval card, on the paired worktree sandbox only. HOST_TOKEN comes from the
// existing test harness; never log it. FAKE_AGENT is host/tests/fake_agent.py (asks before editing a.txt), STEER_AGENT
// host/tests/fixtures/steer_agent.py (its turn runs until it is stopped or two minutes pass).
const healthResponse = http.get(`http://127.0.0.1:${HOST_PORT}/health`)
if (healthResponse.status !== 200 || json(healthResponse.body).environment !== 'dev') {
  throw new Error('Thinking orb checks require an isolated dev host')
}
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method} failed: HTTP ${res.status}`)
  return json(res.body)
}
const bot = (name, command, extra) =>
  ({ name, id: api('createBot', Object.assign({ name, backend: 'custom', command, connectors: [], skills: [], notify: false }, extra)).bot.id })

if (typeof CLEANUP !== 'undefined') {
  for (const b of [output.bot, output.waiting]) if (b) api('deleteBot', { botId: b.id })
} else if (typeof SEND !== 'undefined') {
  api('send', { botId: output.bot.id, text: 'go' })
  api('send', { botId: output.waiting.id, text: 'go' })
} else {
  const tag = Math.random().toString(36).slice(2, 6)
  output.bot = bot(`Orb working ${tag}`, `python3 -X utf8 -u '${STEER_AGENT}' steer 120`)
  output.waiting = bot(`Orb waiting ${tag}`, `python3 -X utf8 -u '${FAKE_AGENT}'`, { permission: 'ask' })
}
