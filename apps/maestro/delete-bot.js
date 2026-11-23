// Makes the bot and group `delete-bot.yaml` deletes from the phone, on the host the phone is paired with (HOST_PORT and
// HOST_TOKEN, passed with `-e`). `GONE` names the one (bot or group) the host must no longer have; `CLEANUP`, when the
// flow completes, deletes what is left.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  return res
}
const ok = (res, method) => {
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

if (typeof CLEANUP !== 'undefined') {
  for (const it of [output.group, output.bot]) if (it) api('deleteBot', { botId: it.id })
} else if (typeof GONE !== 'undefined') {
  const it = output[GONE]
  // Renaming to its own name succeeds only while the host still has it.
  const res = api('updateBot', { id: it.id, name: it.name })
  if (res.status === 200) throw new Error(`the host still has ${it.name}`)
} else {
  const tag = Math.random().toString(36).slice(2, 6)
  const name = `Delete check ${tag}`
  const bot = ok(api('createBot', { name, backend: 'custom', command: 'true', connectors: [], skills: [], notify: false }), 'createBot').bot
  output.bot = { name, id: bot.id }
  const group = `Delete crew ${tag}`
  output.group = { name: group, id: ok(api('createBot', { name: group, kind: 'group', members: [bot.id] }), 'createBot').bot.id }
}
