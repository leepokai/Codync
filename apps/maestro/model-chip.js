// Makes the bot and group `model-chip.yaml` opens, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed
// with `-e`): a bot on Codex with the model "e2e-model", and a group of it. `STEP=saved` waits for the bot to be on the
// model picked from its chip (`MODEL`, else its agent's default); `CLEANUP`, when the flow completes, deletes both.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

if (typeof CLEANUP !== 'undefined') {
  for (const it of [output.group, output.bot]) if (it) api('deleteBot', { botId: it.id })
} else if (typeof STEP !== 'undefined') {
  const want = typeof MODEL !== 'undefined' ? MODEL : null
  for (let tries = 0; ; tries++) {
    const model = api('updateBot', { id: output.bot.id }).bot.model || null
    if (model === want) break
    if (tries === 20) throw new Error(`the bot is still on ${model}`)
    const stop = Date.now() + 500
    while (Date.now() < stop) {} // ponytail: the script has no sleep; a busy wait of at most 10 s
  }
} else {
  const tag = Math.random().toString(36).slice(2, 6)
  const name = `Chip check ${tag}`
  const bot = api('createBot', { name, backend: 'codex', model: 'e2e-model', connectors: [], skills: [], notify: false }).bot
  output.bot = { name, id: bot.id }
  const group = `Chip crew ${tag}`
  output.group = { name: group, id: api('createBot', { name: group, kind: 'group', members: [bot.id] }).bot.id }
}
