// Makes the chat `swipe-to-reply.yaml` swipes, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed
// with `-e`). Its agent is host/tests/fixtures/file_agent.py (FILE_AGENT, its full path), working in FILES, a folder
// holding note.m4a and report.pdf: "Send the report" gets the report and its other files, "spoken" a voice note with
// a caption. `CLEANUP`, when the flow completes, deletes the bot again.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

// One turn at a time, so each message sits above what it got.
const send = (text) => {
  const before = api('history', { botId: output.bot.id, limit: 50 }).entries.filter((e) => e.data.final).length
  api('send', { botId: output.bot.id, text })
  for (let tries = 0; tries < 100; tries++) {
    const until = Date.now() + 100
    while (Date.now() < until) {} // the script has no sleep; a busy wait of at most 10 s a turn
    if (api('history', { botId: output.bot.id, limit: 50 }).entries.filter((e) => e.data.final).length > before) return
  }
  throw new Error(`no answer to "${text}"`)
}

if (typeof CLEANUP !== 'undefined') {
  if (output.bot) api('deleteBot', { botId: output.bot.id })
} else {
  const name = `Swipe check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', cwd: FILES, command: `python3 -X utf8 -u '${FILE_AGENT}'`, connectors: [], skills: [], notify: false,
  }).bot
  output.bot = { name, id: bot.id }
  send('Send the report')
  send('Read me the spoken plan')
}
