// Makes the two bots `icon-badge.yaml` reads, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with
// `-e`), each with one unread answer from host/tests/fixtures/mode_agent.py (MODE_AGENT, its full path). `CLEANUP`, when
// the flow completes, deletes them again.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

const answered = (botId) => {
  api('send', { botId, text: 'hi' })
  for (let tries = 0; tries < 100; tries++) {
    const until = Date.now() + 100
    while (Date.now() < until) {} // ponytail: the script has no sleep; a busy wait of at most 10 s
    if (api('history', { botId, limit: 5 }).entries.some((e) => e.kind === 'agent' && e.data.final)) return
  }
  throw new Error('the bot never answered')
}

if (typeof CLEANUP !== 'undefined') {
  for (const bot of output.bots || []) api('deleteBot', { botId: bot.id })
} else {
  const tag = Math.random().toString(36).slice(2, 6)
  output.bots = ['One', 'Two'].map((n) => {
    const name = `Badge ${n} ${tag}`
    const bot = api('createBot', {
      name, backend: 'custom', command: `python3 -X utf8 -u '${MODE_AGENT}'`, connectors: [], skills: [], notify: false,
    }).bot
    return { name, id: bot.id }
  })
  for (const bot of output.bots) answered(bot.id)
}
