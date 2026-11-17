// Makes the bot `tasks.yaml` opens and has it add an item to its box, on the host the phone is paired with (HOST_PORT
// and HOST_TOKEN, passed with `-e`). Its agent is host/tests/fixtures/task_agent.py (TASK_AGENT, its full path): a
// message `ADD <title>|<brief>` adds an item, whose task works until stopped when its brief says `HOLD:<name>` and
// raises an approval card when it says PERMISSION. `ADD` sends another one and waits until the box has it and the
// bot's own turn is over (a message sent during it would wait above the composer); `MAX` sets how many agents work at
// once; `SIGN_IN` makes the bot Kimi Code (an agent the host knows by name, so a sign-in gets a card) and adds an item of
// that title whose task fails once asking to sign in; `SAY` sends the bot a message; `CLEANUP`, when the flow
// completes, stops the bot's work and deletes it.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

const add = (botId, item) => {
  const title = item.split('|')[0]
  api('send', { botId, text: `ADD ${item}` })
  // An update that changes nothing answers with the bot as the host has it.
  for (let tries = 0; tries < 100; tries++) {
    const until = Date.now() + 100
    while (Date.now() < until) {} // ponytail: the script has no sleep; a busy wait of at most 10 s
    const bot = api('updateBot', { id: botId }).bot
    if (bot.status === 'idle' && (bot.work || []).some((w) => w.title === title)) return
  }
  throw new Error(`"${title}" never reached the box`)
}

if (typeof CLEANUP !== 'undefined') {
  if (output.bot) {
    api('stopWork', { botId: output.bot.id })
    api('deleteBot', { botId: output.bot.id })
  }
} else if (typeof MAX !== 'undefined') {
  api('updateBot', { id: output.bot.id, maxAgents: Number(MAX) })
} else if (typeof ADD !== 'undefined') {
  add(output.bot.id, ADD)
} else if (typeof SIGN_IN !== 'undefined') {
  api('updateBot', { id: output.bot.id, backend: 'kimi' })
  add(output.bot.id, `${SIGN_IN}|SIGNIN`)
} else if (typeof SAY !== 'undefined') {
  api('send', { botId: output.bot.id, text: SAY })
} else {
  const name = `Tasks check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${TASK_AGENT}'`, connectors: [], skills: [], notify: false,
  }).bot
  api('updateBot', { id: bot.id, permission: 'ask' })
  output.bot = { name, id: bot.id }
  add(bot.id, 'Audit the repo|HOLD:audit')
}
