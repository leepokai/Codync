// Makes the short chat `find-in-conversation.yaml` searches, on the host the phone is paired with (HOST_PORT and
// HOST_TOKEN, passed with `-e`). Its agent is host/tests/fixtures/history_agent.py (HISTORY_AGENT, its full path), which
// answers `Message n` with `Reply n`; "needle" is in messages 1 and 3. `SEND` sends one more message, `CLEANUP`, when the
// flow completes, deletes the bot again.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

const send = (n, text) => {
  api('send', { botId: output.bot.id, text })
  for (let tries = 0; tries < 100; tries++) {
    const until = Date.now() + 100
    while (Date.now() < until) {} // ponytail: the script has no sleep; a busy wait of at most 10 s a turn
    if (api('history', { botId: output.bot.id, limit: 5 }).entries.some((e) => e.data.final && e.data.text === `Reply ${n}`)) return
  }
  throw new Error(`no reply to message ${n}`)
}

if (typeof CLEANUP !== 'undefined') {
  if (output.bot) api('deleteBot', { botId: output.bot.id })
} else if (typeof SEND !== 'undefined') {
  api('send', { botId: output.bot.id, text: `Message ${SEND}` })
} else {
  const name = `Find check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${HISTORY_AGENT}'`, connectors: [], skills: [], notify: false,
  }).bot
  output.bot = { name, id: bot.id }
  // One turn at a time, so each message sits above its reply.
  send(1, 'Message 1 has a needle')
  send(2, 'Message 2')
  send(3, 'Message 3 has a NEEDLE too')
}
