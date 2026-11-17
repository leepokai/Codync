// Makes the chat `image-viewer.yaml` opens, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with
// `-e`). Its agent is host/tests/fixtures/file_agent.py (FILE_AGENT, its full path), which shares a real chart.png when
// asked to `share-image`. `CLEANUP`, when the flow completes, deletes the bot again.
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
  const name = `Image check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${FILE_AGENT}'`, connectors: [], skills: [], notify: false,
  }).bot
  output.bot = { name, id: bot.id }
  api('send', { botId: bot.id, text: 'share-image' })
  let shared = false
  for (let tries = 0; tries < 100 && !shared; tries++) {
    const until = Date.now() + 100
    while (Date.now() < until) {} // ponytail: the script has no sleep; a busy wait of at most 10 s
    shared = api('history', { botId: bot.id, limit: 5 }).entries.some((e) => e.data.final && e.data.text === 'The files are ready.')
  }
  if (!shared) throw new Error('the bot never shared chart.png')
}
