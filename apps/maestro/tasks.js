// Makes the bot `tasks.yaml` opens and has it start a task, on the host the phone is paired with (HOST_PORT and
// HOST_TOKEN, passed with `-e`). Its agent is host/tests/fixtures/task_agent.py (TASK_AGENT, its full path): a message
// `START <title>|<brief>` starts a task, which works until stopped when its brief says HOLD and raises an approval card
// when it says PERMISSION. `SEND` sends another message, and `CLEANUP`, when the flow completes, stops the bot's tasks
// and deletes it.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

if (typeof CLEANUP !== 'undefined') {
  if (output.bot) {
    api('stopTask', { botId: output.bot.id })
    api('deleteBot', { botId: output.bot.id })
  }
} else if (typeof SEND !== 'undefined') {
  api('send', { botId: output.bot.id, text: SEND })
} else {
  const name = `Tasks check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${TASK_AGENT}'`, connectors: [], skills: [], notify: false,
  }).bot
  api('updateBot', { id: bot.id, permission: 'ask' })
  api('send', { botId: bot.id, text: 'START Audit the repo|HOLD until released' })
  output.bot = { name, id: bot.id }
}
