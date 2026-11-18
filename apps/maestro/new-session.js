// Makes the bot and group `new-session.yaml` checks, on the host the phone is paired with (HOST_PORT and HOST_TOKEN,
// passed with `-e`). The bot's agent is host/tests/fixtures/mode_agent.py (MODE_AGENT, its full path), which answers each
// message with `mode agent · ` and what it was sent; the first message of a session also carries the bot's profile, so
// an answer holding `</bot-profile>` ran in a fresh session. `STEP=second` sends the message after New session and
// fails unless it opened one; `CLEANUP`, when the flow completes, deletes both again.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

const answer = (botId, text) => {
  api('send', { botId, text })
  for (let tries = 0; tries < 100; tries++) {
    const until = Date.now() + 100
    while (Date.now() < until) {} // ponytail: the script has no sleep; a busy wait of at most 10 s
    const reply = api('history', { botId, limit: 5 }).entries.find((e) => e.kind === 'agent' && e.data.final && e.data.text.endsWith(text))
    if (reply) return reply.data.text
  }
  throw new Error(`the bot never answered ${text}`)
}

if (typeof CLEANUP !== 'undefined') {
  if (output.group) api('deleteBot', { botId: output.group.id })
  if (output.bot) api('deleteBot', { botId: output.bot.id })
} else if (typeof STEP !== 'undefined' && STEP === 'second') {
  if (!answer(output.bot.id, 'second').includes('</bot-profile>')) throw new Error('the next turn ran in the old session')
} else {
  const tag = Math.random().toString(36).slice(2, 6)
  const name = `Session check ${tag}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${MODE_AGENT}'`, connectors: [], skills: [], notify: false,
  }).bot
  output.bot = { name, id: bot.id }
  answer(bot.id, 'hello')
  const group = `Session crew ${tag}`
  output.group = { name: group, id: api('createBot', { name: group, kind: 'group', members: [bot.id] }).bot.id }
}
