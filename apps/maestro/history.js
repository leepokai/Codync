// Makes the long chat `history.yaml` scrolls back through, on the host the phone is paired with (HOST_PORT and
// HOST_TOKEN, passed with `-e`). Its agent is host/tests/fixtures/history_agent.py (HISTORY_AGENT, its full path), which
// answers each message at once after a tool call; the first message's turn runs 120 of them. `CLEANUP`, when the flow
// completes, deletes the bot again.
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
  const name = `History check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${HISTORY_AGENT}'`, connectors: [], skills: [], notify: false,
  }).bot
  output.bot = { name, id: bot.id }
  // One turn at a time, so each message sits above its reply: 320 messages, more than Android holds in memory at once.
  for (let n = 1; n <= 160; n++) {
    api('send', { botId: bot.id, text: n === 1 ? 'Message 1 opens with 120 tool calls' : `Message ${n}` })
    let replied = false
    for (let tries = 0; tries < 100 && !replied; tries++) {
      const until = Date.now() + 100
      while (Date.now() < until) {} // ponytail: the script has no sleep; a busy wait of at most 10 s a turn
      replied = api('history', { botId: bot.id, limit: 5 }).entries.some((e) => e.data.final && e.data.text === `Reply ${n}`)
    }
    if (!replied) throw new Error(`no reply to message ${n}`)
  }
}
