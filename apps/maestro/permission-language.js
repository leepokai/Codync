// An approval card on the paired worktree sandbox only. HOST_TOKEN comes from the existing test harness;
// never log it. FAKE_AGENT is the absolute path to host/tests/fake_agent.py, which asks before editing a.txt.
const healthResponse = http.get(`http://127.0.0.1:${HOST_PORT}/health`)
if (healthResponse.status !== 200 || json(healthResponse.body).environment !== 'dev') {
  throw new Error('Approval card checks require an isolated dev host')
}
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method} failed: HTTP ${res.status}`)
  return json(res.body)
}

if (typeof CLEANUP !== 'undefined') {
  if (output.bot) api('deleteBot', { botId: output.bot.id })
} else if (typeof SEND !== 'undefined') {
  api('send', { botId: output.bot.id, text: 'go' })
} else {
  const name = `Approval language ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${FAKE_AGENT}'`, connectors: [], skills: [], notify: false, permission: 'ask',
  }).bot
  output.bot = { name, id: bot.id }
}
