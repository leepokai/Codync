// Synthetic choice flows on the paired worktree sandbox only. HOST_TOKEN comes from the existing test harness;
// never log it or copy a production chat. TEAM_AGENT is the absolute path to host/tests/fixtures/team_agent.py.
const healthResponse = http.get(`http://127.0.0.1:${HOST_PORT}/health`)
if (healthResponse.status !== 200 || json(healthResponse.body).environment !== 'dev') {
  throw new Error('Choice UI checks require an isolated dev host')
}
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method} failed: HTTP ${res.status}`)
  return json(res.body)
}
const cards = () => api('history', { botId: output.bot.id }).entries.filter((e) => e.data.decision === true)

if (typeof CLEANUP !== 'undefined') {
  if (output.bot) {
    api('stop', { botId: output.bot.id })
    api('deleteBot', { botId: output.bot.id })
  }
} else if (typeof SEND !== 'undefined') {
  api('send', { botId: output.bot.id, text: SEND })
} else if (typeof STOP !== 'undefined') {
  api('stop', { botId: output.bot.id })
} else if (typeof REMOTE !== 'undefined') {
  const pending = cards().filter((e) => e.data.status === 'pending')
  if (pending.length !== 1) throw new Error('Expected one pending synthetic card')
  api('respondDecision', { entryId: pending[0].id, option: REMOTE })
} else if (typeof VERIFY !== 'undefined') {
  const history = api('history', { botId: output.bot.id }).entries
  const choices = history.filter((e) => e.data.decision === true)
  const latest = choices[choices.length - 1]
  if (!latest || latest.data.status !== VERIFY) throw new Error('Wrong card outcome after native UI action')
  const replies = history.filter((e) => e.kind === 'user' && e.data.from === 'decision')
  if (replies.length !== Number(EXPECTED_REPLIES)) throw new Error('A choice was omitted or submitted twice')
  if (VERIFY === 'answered' && latest.data.selected !== EXPECTED_CHOICE) throw new Error('Wrong selected label')
} else {
  const name = `Choice check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${TEAM_AGENT}'`, connectors: [], skills: [], notify: false,
  }).bot
  output.bot = { name, id: bot.id }
  api('send', { botId: bot.id, text: 'ASK One\nTwo\nThree\nFour\nFifth line matters|2|4|4' })
}
