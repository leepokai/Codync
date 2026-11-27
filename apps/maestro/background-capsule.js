// Makes the bot `background-capsule.yaml` opens, on the host the phone is paired with (HOST_PORT and HOST_TOKEN,
// passed with `-e`), on host/tests/fixtures/work_agent.py (TASK_AGENT, its full path) started with `tasks`, so it
// reports its background work as Claude's adapter does, and has it add a task whose thread runs a workflow in the
// background that ends after 90 s. `CLEANUP`, when the flow completes, stops the bot's work and deletes it.
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
    api('stopWork', { botId: output.bot.id })
    api('deleteBot', { botId: output.bot.id })
  }
} else {
  // Each phone's shard makes its own bot on the one host, so each task has a name of its own.
  const id = Math.random().toString(36).slice(2, 6)
  const name = `Background check ${id}`
  const task = `Run the review ${id}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${TASK_AGENT}' tasks`, connectors: [], skills: [], notify: false,
  }).bot
  output.bot = { name, id: bot.id, task }
  api('send', { botId: bot.id, text: `ADD ${task}|WORKFLOW:review AFTER:90` })
}
