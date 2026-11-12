// Makes the chat `voice-note.yaml` plays, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with
// `-e`). Its agent is host/tests/fixtures/steer_agent.py (STEER_AGENT, its full path) in `voice-wav` mode, which sends
// four seconds of real WAV with text as a voice note. `CLEANUP`, when the flow completes, deletes the bot again.
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
  const name = `Voice check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${STEER_AGENT}' voice-wav 0`, connectors: [], skills: [], notify: false,
  }).bot
  output.bot = { name, id: bot.id }
  api('send', { botId: bot.id, text: 'Send a voice note' })
  let sent = false
  for (let tries = 0; tries < 100 && !sent; tries++) {
    const until = Date.now() + 100
    while (Date.now() < until) {} // ponytail: the script has no sleep; a busy wait of at most 10 s
    sent = api('history', { botId: bot.id, limit: 5 }).entries.some((e) => e.data.text === 'Listen before it joined')
  }
  if (!sent) throw new Error('the bot never sent its voice note')
}
