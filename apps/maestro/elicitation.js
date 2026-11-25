// An agent's own question (ACP elicitation/create) on the paired worktree sandbox only. HOST_TOKEN comes from the
// existing test harness; never log it. ELICITATION_AGENT is the absolute path to host/tests/fixtures/elicitation_agent.py.
const healthResponse = http.get(`http://127.0.0.1:${HOST_PORT}/health`)
if (healthResponse.status !== 200 || json(healthResponse.body).environment !== 'dev') {
  throw new Error('Question UI checks require an isolated dev host')
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
  if (output.bot) {
    api('stop', { botId: output.bot.id })
    api('deleteBot', { botId: output.bot.id })
  }
} else if (typeof SEND !== 'undefined') {
  api('send', { botId: output.bot.id, text: SEND })
} else if (typeof REPLIED !== 'undefined') {
  // The fixture agent says what it got back; the card's options and the user's own entry hold the words too.
  const reply = `Reply: {"action": "accept", "content": {"answer": "${REPLIED}"}}`
  const until = Date.now() + 10000
  while (!api('history', { botId: output.bot.id }).entries.some((e) => (e.data.text || '').includes(reply))) {
    if (Date.now() > until) throw new Error('The agent did not get the expected reply')
    const wait = Date.now() + 500
    while (Date.now() < wait) {} // ponytail: the script has no sleep; a busy wait of at most 10 s
  }
} else {
  const name = `Question check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${ELICITATION_AGENT}'`, connectors: [], skills: [], notify: false,
  }).bot
  output.bot = { name, id: bot.id }
  api('send', { botId: bot.id, text: 'ASK_CHOICE' })
}
