// Makes the bot `routines.yaml` opens, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with `-e`),
// with a daily routine and one the bot set up (an interval). No agent runs. `CLEANUP`, when the flow completes, deletes it.
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
  const name = `Routine check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', { name, backend: 'custom', command: 'unused', connectors: [], skills: [], notify: false }).bot
  api('saveRoutine', {
    botId: bot.id, name: 'Daily check', instruction: 'Report once',
    triggers: [{ type: 'cron', expression: '0 9 * * *', timeZone: 'Europe/Rome' }], timeoutSeconds: 3600,
  })
  api('saveRoutine', { botId: bot.id, name: 'Bot interval', instruction: 'Report once', triggers: [{ type: 'interval', seconds: 600 }], timeoutSeconds: 90 })
  output.bot = { name, id: bot.id }
}
